mod common;

use std::{
    path::Path,
    sync::{atomic::AtomicBool, mpsc, Arc},
};

use clean_my_mess::scan;
use common::TempDir;

fn run_scan(root: &Path) -> Vec<(String, &'static str, u64)> {
    let (tx, rx) = mpsc::channel();
    scan::scan(root.to_path_buf(), tx, Arc::new(AtomicBool::new(false)));
    let mut found: Vec<_> = rx
        .into_iter()
        .filter_map(|msg| match msg {
            scan::Msg::Found(i) => {
                Some((i.path.strip_prefix(root).expect("under root").display().to_string(), i.kind, i.size))
            }
            _ => None,
        })
        .collect();
    found.sort();
    found
}

#[test]
fn scan_recognises_waste_and_ignores_lookalikes() {
    let dir = TempDir::new();
    dir.file("web/package.json", 2);
    dir.file("web/node_modules/pkg/index.js", 5000);
    dir.file("web/node_modules/pkg/node_modules/inner/index.js", 5000);
    dir.file("rs/Cargo.toml", 2);
    dir.file("rs/target/debug/bin", 5000);
    dir.file("plain/target/output", 5000);
    dir.file("site/dist/app.js", 5000);
    dir.file("py/__pycache__/a.pyc", 5000);
    dir.file(".git/node_modules/x", 5000);

    let found = run_scan(&dir.0);
    let names: Vec<(&str, &str)> = found.iter().map(|(p, k, _)| (p.as_str(), *k)).collect();
    assert_eq!(
        names,
        vec![("py/__pycache__", "Python"), ("rs/target", "Rust"), ("web/node_modules", "Node")],
        "target without Cargo.toml, dist without package.json, nested node_modules and .git must not match"
    );
    assert!(found.iter().all(|(_, _, size)| *size >= 5000));
}

#[test]
fn scan_stops_when_cancelled() {
    let dir = TempDir::new();
    dir.file("a/node_modules/x", 100);
    let (tx, rx) = mpsc::channel();
    scan::scan(dir.0.clone(), tx, Arc::new(AtomicBool::new(true)));
    assert_eq!(rx.into_iter().count(), 0);
}

fn collect_until_done(rx: &mpsc::Receiver<scan::Msg>) -> Vec<String> {
    let mut found = Vec::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(10)).expect("scan finished in time") {
            scan::Msg::Found(item) => found.push(item.path.display().to_string()),
            scan::Msg::Done => return found,
            scan::Msg::Progress(_) => {}
        }
    }
}

#[test]
fn extending_the_root_reuses_earlier_results() {
    let dir = TempDir::new();
    dir.file("top/p/node_modules/x", 100);
    dir.file("top/sibling/node_modules/x", 100);
    dir.file("top/sibling/deeper/__pycache__/x", 100);
    let (top, p) = (dir.0.join("top"), dir.0.join("top/p"));

    let (tx, rx) = mpsc::channel();
    let scanner = scan::Scanner::start(p.clone(), tx);
    let first = collect_until_done(&rx);
    assert_eq!(first, vec![p.join("node_modules").display().to_string()]);

    scanner.extend(top.clone(), p);
    let mut second = collect_until_done(&rx);
    second.sort();
    assert_eq!(
        second,
        vec![
            top.join("sibling/deeper/__pycache__").display().to_string(),
            top.join("sibling/node_modules").display().to_string(),
        ],
        "only the new siblings are scanned; p/ is not reported twice"
    );
}

#[test]
fn progress_reports_which_subtrees_are_still_pending() {
    let dir = TempDir::new();
    dir.file("top/a/node_modules/x", 100);
    let (tx, rx) = mpsc::channel();
    let scanner = scan::Scanner::start(dir.0.join("top"), tx);
    collect_until_done(&rx);
    let progress = scanner.progress(&dir.0.join("top"));
    assert!(!progress.running);
    assert!(!progress.base_pending);
    assert!(progress.pending_children.is_empty());
}
