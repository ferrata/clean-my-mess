use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex, MutexGuard,
    },
    thread,
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

struct State {
    pending: Vec<PathBuf>,
    current: Option<PathBuf>,
    skip: HashSet<PathBuf>,
    focus: Option<PathBuf>,
    focus_exhausted: bool,
    running: bool,
    visited: usize,
}

impl State {
    fn new(root: PathBuf) -> Self {
        Self {
            pending: vec![root],
            current: None,
            skip: HashSet::new(),
            focus: None,
            focus_exhausted: false,
            running: true,
            visited: 0,
        }
    }

    fn next_dir(&mut self) -> Option<PathBuf> {
        if let Some(focus) = &self.focus {
            if !self.focus_exhausted {
                match self.pending.iter().rposition(|p| p.starts_with(focus)) {
                    Some(i) => return Some(self.pending.remove(i)),
                    None => self.focus_exhausted = true,
                }
            }
        }
        self.pending.pop()
    }
}

pub struct Progress {
    pub base_pending: bool,
    pub pending_children: HashSet<PathBuf>,
    pub already_scanned: HashSet<PathBuf>,
    pub visited: usize,
    pub running: bool,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct Scanner {
    state: Arc<Mutex<State>>,
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Scanner {
    pub fn start(root: PathBuf, tx: Sender<Msg>) -> Self {
        let scanner =
            Self { state: Arc::new(Mutex::new(State::new(root))), tx, cancel: Arc::new(AtomicBool::new(false)) };
        scanner.spawn();
        scanner
    }

    fn spawn(&self) {
        let scanner = self.clone();
        thread::spawn(move || worker(&scanner.state, &scanner.tx, &scanner.cancel));
    }

    #[doc(hidden)]
    pub fn idle(pending: Vec<PathBuf>, tx: Sender<Msg>) -> Self {
        let mut state = State::new(PathBuf::new());
        state.pending = pending;
        Self { state: Arc::new(Mutex::new(state)), tx, cancel: Arc::new(AtomicBool::new(false)) }
    }

    pub fn prioritize(&self, path: &Path) {
        let mut state = lock(&self.state);
        state.focus = Some(path.to_path_buf());
        state.focus_exhausted = false;
    }

    pub fn extend(&self, parent: PathBuf, old_root: PathBuf) {
        let mut state = lock(&self.state);
        state.skip.insert(old_root);
        state.focus = Some(parent.clone());
        state.focus_exhausted = false;
        state.pending.push(parent);
        if !state.running {
            state.running = true;
            drop(state);
            self.spawn();
        }
    }

    pub fn progress(&self, base: &Path) -> Progress {
        let state = lock(&self.state);
        let mut base_pending = false;
        let mut pending_children = HashSet::new();
        for path in state.pending.iter().chain(state.current.iter()) {
            if base.starts_with(path) {
                base_pending = true;
            } else if let Ok(rel) = path.strip_prefix(base) {
                if let Some(first) = rel.components().next() {
                    pending_children.insert(base.join(first));
                }
            }
        }
        Progress {
            base_pending,
            pending_children,
            already_scanned: state.skip.clone(),
            visited: state.visited,
            running: state.running,
        }
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn worker(state: &Mutex<State>, tx: &Sender<Msg>, cancel: &AtomicBool) {
    loop {
        let (dir, skip, visited) = {
            let mut st = lock(state);
            if cancel.load(Ordering::Relaxed) {
                st.running = false;
                st.current = None;
                return;
            }
            match st.next_dir() {
                Some(dir) => {
                    st.current = Some(dir.clone());
                    st.visited += 1;
                    (dir, st.skip.clone(), st.visited)
                }
                None => {
                    st.running = false;
                    st.current = None;
                    let visited = st.visited;
                    drop(st);
                    let _ = tx.send(Msg::Progress(visited));
                    let _ = tx.send(Msg::Done);
                    return;
                }
            }
        };
        if visited % 200 == 0 && tx.send(Msg::Progress(visited)).is_err() {
            return;
        }

        let mut children = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else { continue };
                if !file_type.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let path = entry.path();
                if name == ".git" || skip.contains(&path) {
                    continue;
                }
                match match_rule(&dir, name, &path) {
                    Some(rule) => {
                        let size = dir_size(&path, cancel);
                        if cancel.load(Ordering::Relaxed) {
                            let mut st = lock(state);
                            st.running = false;
                            return;
                        }
                        if tx.send(Msg::Found(Item { path, kind: rule.kind, size })).is_err() {
                            return;
                        }
                    }
                    None => children.push(path),
                }
            }
        }

        let mut st = lock(state);
        if st.focus.as_ref().is_some_and(|f| dir.starts_with(f)) {
            st.focus_exhausted = false;
        }
        st.pending.extend(children);
        st.current = None;
    }
}

pub fn scan(root: PathBuf, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    let state = Mutex::new(State::new(root));
    worker(&state, &tx, &cancel);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(pending: &[&str]) -> State {
        let mut st = State::new(PathBuf::from("/never"));
        st.pending = pending.iter().map(PathBuf::from).collect();
        st
    }

    #[test]
    fn focused_subtree_is_scanned_first_then_normal_order_resumes() {
        let mut st = state(&["/t/a/1", "/t/b/1", "/t/a/2", "/t/c"]);
        st.focus = Some(PathBuf::from("/t/b"));
        assert_eq!(st.next_dir(), Some(PathBuf::from("/t/b/1")));
        assert_eq!(st.next_dir(), Some(PathBuf::from("/t/c")));
        assert!(st.focus_exhausted);
        assert_eq!(st.next_dir(), Some(PathBuf::from("/t/a/2")));
    }

    #[test]
    fn without_focus_the_queue_is_depth_first() {
        let mut st = state(&["/t/a", "/t/b"]);
        assert_eq!(st.next_dir(), Some(PathBuf::from("/t/b")));
    }

    fn scanner_with(pending: &[&str], current: Option<&str>) -> Scanner {
        let mut st = state(pending);
        st.current = current.map(PathBuf::from);
        st.running = true;
        Scanner {
            state: Arc::new(Mutex::new(st)),
            tx: std::sync::mpsc::channel().0,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn progress_marks_only_unfinished_subtrees() {
        let scanner = scanner_with(&["/t/a/x"], Some("/t/b"));
        let at_top = scanner.progress(Path::new("/t"));
        assert!(!at_top.base_pending);
        let expected: HashSet<PathBuf> = ["/t/a", "/t/b"].iter().map(PathBuf::from).collect();
        assert_eq!(at_top.pending_children, expected);

        let finished = scanner.progress(Path::new("/t/c"));
        assert!(!finished.base_pending && finished.pending_children.is_empty());

        let below_pending = scanner.progress(Path::new("/t/b/deeper"));
        assert!(below_pending.base_pending, "a pending ancestor means its children are not even listed yet");
    }
}
