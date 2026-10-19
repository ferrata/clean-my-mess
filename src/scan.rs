use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
};

pub struct Item {
    pub path: PathBuf,
    pub kind: &'static str,
    pub size: u64,
}

pub enum Msg {
    Found(Item),
    Progress(usize),
    Done,
}

struct Rule {
    name: &'static str,
    kind: &'static str,
    /// Any of these must exist next to the directory; empty means always match.
    sibling_markers: &'static [&'static str],
    /// Any of these must exist inside the directory.
    inner_markers: &'static [&'static str],
}

const fn rule(name: &'static str, kind: &'static str, sibling_markers: &'static [&'static str]) -> Rule {
    Rule { name, kind, sibling_markers, inner_markers: &[] }
}

const RULES: &[Rule] = &[
    rule("node_modules", "Node", &[]),
    rule(".next", "Next.js", &["package.json"]),
    rule(".nuxt", "Nuxt", &["package.json"]),
    rule(".svelte-kit", "SvelteKit", &["package.json"]),
    rule(".angular", "Angular", &["angular.json"]),
    rule(".turbo", "Turborepo", &[]),
    rule(".parcel-cache", "Parcel", &[]),
    rule("dist", "JS build", &["package.json"]),
    rule("build", "JS build", &["package.json"]),
    rule("coverage", "Coverage", &["package.json"]),
    rule("target", "Rust", &["Cargo.toml"]),
    rule("target", "Maven", &["pom.xml"]),
    rule("build", "Gradle", &["build.gradle", "build.gradle.kts"]),
    rule(".gradle", "Gradle", &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"]),
    rule("__pycache__", "Python", &[]),
    rule(".pytest_cache", "Python", &[]),
    rule(".mypy_cache", "Python", &[]),
    rule(".ruff_cache", "Python", &[]),
    rule(".tox", "Python", &["tox.ini", "pyproject.toml", "setup.py"]),
    rule(".venv", "Python venv", &[]),
    Rule { name: "venv", kind: "Python venv", sibling_markers: &[], inner_markers: &["pyvenv.cfg"] },
    rule("Pods", "CocoaPods", &["Podfile"]),
    rule("DerivedData", "Xcode", &[]),
    rule(".dart_tool", "Dart", &["pubspec.yaml"]),
    rule(".terraform", "Terraform", &[]),
];

fn match_rule(parent: &Path, name: &str, path: &Path) -> Option<&'static Rule> {
    RULES.iter().find(|r| {
        r.name == name
            && (r.sibling_markers.is_empty() || r.sibling_markers.iter().any(|m| parent.join(m).exists()))
            && (r.inner_markers.is_empty() || r.inner_markers.iter().any(|m| path.join(m).exists()))
    })
}

#[cfg(unix)]
pub fn disk_usage(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
pub fn disk_usage(meta: &fs::Metadata) -> u64 {
    meta.len()
}

fn dir_size(root: &Path, cancel: &AtomicBool) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return total;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            total += disk_usage(&meta);
            if meta.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    total
}

pub fn scan(root: PathBuf, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    let mut stack = vec![root];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        visited += 1;
        if visited % 200 == 0 && tx.send(Msg::Progress(visited)).is_err() {
            return;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == ".git" {
                continue;
            }
            let path = entry.path();
            match match_rule(&dir, name, &path) {
                Some(rule) => {
                    let size = dir_size(&path, &cancel);
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if tx.send(Msg::Found(Item { path, kind: rule.kind, size })).is_err() {
                        return;
                    }
                }
                None => stack.push(path),
            }
        }
    }
    let _ = tx.send(Msg::Progress(visited));
    let _ = tx.send(Msg::Done);
}
