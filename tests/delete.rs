mod common;

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc, Arc},
};

use clean_my_mess::delete;
use common::TempDir;

fn run_delete(paths: Vec<PathBuf>, cancelled: bool) -> Vec<delete::Msg> {
    run_with(paths, delete::Mode::Permanent, cancelled)
}

fn run_with(paths: Vec<PathBuf>, mode: delete::Mode, cancelled: bool) -> Vec<delete::Msg> {
    let (tx, rx) = mpsc::channel();
    delete::run(paths.into_iter().map(|p| (p, 1000)).collect(), mode, tx, Arc::new(AtomicBool::new(cancelled)));
    rx.into_iter().collect()
}

#[test]
fn delete_removes_tree_but_never_follows_symlinks() {
    let dir = TempDir::new();
    dir.file("proj/node_modules/pkg/a.js", 4000);
    dir.file("proj/node_modules/pkg/deep/b.js", 4000);
    dir.file("outside/precious.txt", 10);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(dir.0.join("outside"), dir.0.join("proj/node_modules/link-to-outside")).expect("symlink");
        symlink("/definitely/not/here", dir.0.join("proj/node_modules/dangling")).expect("symlink");
    }

    let target = dir.0.join("proj/node_modules");
    let msgs = run_delete(vec![target.clone()], false);

    assert!(!target.exists(), "target should be gone");
    assert!(dir.0.join("proj").exists(), "only the waste folder is removed");
    assert!(dir.0.join("outside/precious.txt").exists(), "symlink target must survive");
    let item = msgs.iter().find_map(|m| match m {
        delete::Msg::Item { complete, bytes, .. } => Some((*complete, *bytes)),
        _ => None,
    });
    let (complete, freed) = item.expect("item message");
    assert!(complete);
    assert!(freed >= 8000);
    assert!(matches!(msgs.last(), Some(delete::Msg::Done)));
}

#[test]
fn delete_does_nothing_when_cancelled_up_front() {
    let dir = TempDir::new();
    dir.file("proj/node_modules/a.js", 100);
    let target = dir.0.join("proj/node_modules");
    run_delete(vec![target.clone()], true);
    assert!(target.join("a.js").exists());
}

#[test]
fn delete_reports_missing_path_as_failure() {
    let dir = TempDir::new();
    let msgs = run_delete(vec![dir.0.join("nope")], false);
    assert!(msgs.iter().any(|m| matches!(m, delete::Msg::Failed(_))));
    assert!(msgs.iter().any(|m| matches!(m, delete::Msg::Item { complete: false, .. })));
}

#[test]
fn trash_moves_the_folder_intact_and_never_deletes() {
    let dir = TempDir::new();
    dir.file("proj/node_modules/pkg/a.js", 4000);
    dir.file("outside/precious.txt", 10);
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.0.join("outside"), dir.0.join("proj/node_modules/link")).expect("symlink");
    let (target, trash) = (dir.0.join("proj/node_modules"), dir.0.join("trash"));

    let msgs = run_with(vec![target.clone()], delete::Mode::Trash(trash.clone()), false);

    assert!(!target.exists());
    assert_eq!(std::fs::read(trash.join("node_modules (proj)/pkg/a.js")).expect("moved intact").len(), 4000);
    assert!(dir.0.join("outside/precious.txt").exists());
    assert!(msgs.iter().any(|m| matches!(m, delete::Msg::Item { complete: true, .. })));
}

#[test]
fn trash_never_overwrites_an_existing_entry() {
    let dir = TempDir::new();
    dir.file("proj/node_modules/one.js", 1);
    dir.file("other/proj/node_modules/two.js", 1);
    let trash = dir.0.join("trash");
    for target in [dir.0.join("proj/node_modules"), dir.0.join("other/proj/node_modules")] {
        run_with(vec![target], delete::Mode::Trash(trash.clone()), false);
    }
    assert!(trash.join("node_modules (proj)/one.js").exists());
    assert!(trash.join("node_modules (proj) 2/two.js").exists());
}

#[test]
fn trash_reports_failure_and_leaves_source_alone() {
    let dir = TempDir::new();
    let missing = dir.0.join("nope");
    let msgs = run_with(vec![missing], delete::Mode::Trash(dir.0.join("trash")), false);
    assert!(msgs.iter().any(|m| matches!(m, delete::Msg::Failed(_))));
    assert!(msgs.iter().any(|m| matches!(m, delete::Msg::Item { complete: false, bytes: 0, .. })));
}

#[test]
fn trash_does_nothing_when_cancelled_up_front() {
    let dir = TempDir::new();
    dir.file("proj/node_modules/a.js", 5);
    let target = dir.0.join("proj/node_modules");
    run_with(vec![target.clone()], delete::Mode::Trash(dir.0.join("trash")), true);
    assert!(target.join("a.js").exists());
}
