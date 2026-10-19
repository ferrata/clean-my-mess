use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::Instant,
};

use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyModifiers},
    Terminal,
};

use crate::{
    delete,
    format::{human, plural},
    scan::{self, Item, Msg},
    tree::{build_entries, list_dirs, Entry},
    ui,
};

pub(crate) enum Mode {
    Normal,
    Confirm,
    Deleting,
}

pub(crate) struct DeleteJob {
    pub(crate) rx: Receiver<delete::Msg>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) paths: HashSet<PathBuf>,
    pub(crate) total_bytes: u64,
    pub(crate) total_items: usize,
    pub(crate) done_bytes: u64,
    pub(crate) done_items: usize,
    pub(crate) freed: u64,
    pub(crate) current: PathBuf,
    pub(crate) failed: Vec<String>,
    pub(crate) stopped: bool,
}

pub struct App {
    pub(crate) root: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) rx: Receiver<Msg>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) items: Vec<Item>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) dirs: Vec<(PathBuf, String)>,
    pub(crate) selected: HashSet<PathBuf>,
    pub(crate) cursor: usize,
    pub(crate) focus: Option<PathBuf>,
    pub(crate) mode: Mode,
    pub(crate) delete_focused: bool,
    pub(crate) job: Option<DeleteJob>,
    pub(crate) gone: HashSet<PathBuf>,
    pub(crate) scanning: bool,
    pub(crate) visited: usize,
    pub(crate) freed: u64,
    pub(crate) status: String,
    pub(crate) quit: bool,
    pub(crate) started: Instant,
}

fn spawn_scan(root: &Path) -> (Receiver<Msg>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let (root, flag) = (root.to_path_buf(), cancel.clone());
    thread::spawn(move || scan::scan(root, tx, flag));
    (rx, cancel)
}

impl Drop for App {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        let (rx, cancel) = spawn_scan(&root);
        let mut app = Self::build(root, rx, cancel, true);
        app.refresh();
        app
    }

    pub fn with_items(root: PathBuf, items: Vec<Item>) -> Self {
        let (_tx, rx) = mpsc::channel();
        let mut app = Self::build(root, rx, Arc::new(AtomicBool::new(false)), false);
        app.items = items;
        app.refresh();
        app
    }

    pub fn press(&mut self, code: KeyCode) {
        self.on_key(code, KeyModifiers::NONE);
        self.refresh();
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    pub fn dialog_open(&self) -> bool {
        matches!(self.mode, Mode::Confirm | Mode::Deleting)
    }

    pub fn delete_focused(&self) -> bool {
        self.delete_focused
    }

    pub fn render_buffer(&self, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|f| ui::draw(f, self)).expect("draw");
        terminal.backend().buffer().clone()
    }

    pub fn render(&self, width: u16, height: u16) -> String {
        let buffer = self.render_buffer(width, height);
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn build(root: PathBuf, rx: Receiver<Msg>, cancel: Arc<AtomicBool>, scanning: bool) -> Self {
        Self {
            dirs: list_dirs(&root),
            cwd: root.clone(),
            root,
            rx,
            cancel,
            items: Vec::new(),
            entries: Vec::new(),
            selected: HashSet::new(),
            cursor: 0,
            focus: None,
            mode: Mode::Normal,
            delete_focused: false,
            job: None,
            gone: HashSet::new(),
            scanning,
            visited: 0,
            freed: 0,
            status: String::new(),
            quit: false,
            started: Instant::now(),
        }
    }

    fn drain_scan(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Found(item) => {
                    let busy = self.job.as_ref().is_some_and(|j| j.paths.contains(&item.path));
                    if !busy && !self.gone.contains(&item.path) {
                        self.items.push(item);
                    }
                }
                Msg::Progress(n) => self.visited = n,
                Msg::Done => self.scanning = false,
            }
        }
    }

    pub fn pump(&mut self) {
        self.drain_scan();
        self.drain_delete();
        self.refresh();
    }

    fn rescan(&mut self, root: PathBuf, focus: Option<PathBuf>) {
        self.cancel.store(true, Ordering::Relaxed);
        let (rx, cancel) = spawn_scan(&root);
        self.rx = rx;
        self.cancel = cancel;
        self.dirs = list_dirs(&root);
        self.cwd = root.clone();
        self.root = root;
        self.items.clear();
        self.entries.clear();
        self.selected.clear();
        self.cursor = 0;
        self.focus = focus;
        self.scanning = true;
        self.visited = 0;
    }

    pub(crate) fn refresh(&mut self) {
        let keep = self.focus.take().or_else(|| self.entries.get(self.cursor).map(|e| e.path.clone()));
        self.entries = build_entries(&self.items, &self.cwd, &self.dirs);
        self.cursor = keep
            .and_then(|p| self.entries.iter().position(|e| e.path == p))
            .unwrap_or_else(|| self.cursor.min(self.entries.len().saturating_sub(1)));
    }

    pub(crate) fn scope_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.path.starts_with(&self.cwd))
    }

    pub(crate) fn targets(&self) -> Vec<&Entry> {
        let picked: Vec<&Entry> = if self.selected.is_empty() {
            self.entries.get(self.cursor).into_iter().collect()
        } else {
            self.entries.iter().filter(|e| self.selected.contains(&e.path)).collect()
        };
        picked.into_iter().filter(|e| e.count > 0).collect()
    }

    fn set_cwd(&mut self, path: PathBuf) {
        self.dirs = list_dirs(&path);
        self.cwd = path;
        self.entries.clear();
    }

    pub(crate) fn selected_size(&self) -> u64 {
        self.entries.iter().filter(|e| self.selected.contains(&e.path)).map(|e| e.size).sum()
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let max = self.entries.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
    }

    fn enter(&mut self) {
        let Some(entry) = self.entries.get(self.cursor) else { return };
        if entry.is_waste() {
            self.status = "This is a waste folder itself — press d to delete it".into();
            return;
        }
        let path = entry.path.clone();
        self.selected.clear();
        self.cursor = 0;
        if self.scanning {
            self.rescan(path, None);
        } else {
            self.set_cwd(path);
        }
    }

    fn up(&mut self) {
        let came_from = self.cwd.clone();
        self.selected.clear();
        if self.cwd == self.root {
            match self.root.parent().map(Path::to_path_buf) {
                Some(parent) => self.rescan(parent, Some(came_from)),
                None => self.status = "Already at the filesystem root".into(),
            }
        } else if let Some(parent) = self.cwd.parent() {
            self.set_cwd(parent.to_path_buf());
            self.focus = Some(came_from);
        }
    }

    fn confirm_delete(&mut self) {
        let target_paths: Vec<PathBuf> = self.targets().iter().map(|e| e.path.clone()).collect();
        let doomed: Vec<(PathBuf, u64)> = self
            .items
            .iter()
            .filter(|i| target_paths.iter().any(|t| i.path.starts_with(t)))
            .map(|i| (i.path.clone(), i.size))
            .collect();
        if doomed.is_empty() {
            self.mode = Mode::Normal;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let paths: Vec<PathBuf> = doomed.iter().map(|(p, _)| p.clone()).collect();
        thread::spawn(move || delete::run(paths, tx, flag));
        self.job = Some(DeleteJob {
            rx,
            cancel,
            total_bytes: doomed.iter().map(|(_, s)| s).sum(),
            total_items: doomed.len(),
            paths: doomed.into_iter().map(|(p, _)| p).collect(),
            done_bytes: 0,
            done_items: 0,
            freed: 0,
            current: PathBuf::new(),
            failed: Vec::new(),
            stopped: false,
        });
        self.selected.clear();
        self.mode = Mode::Deleting;
    }

    fn drain_delete(&mut self) {
        let mut finished = false;
        while let Some(msg) = self.job.as_ref().and_then(|j| j.rx.try_recv().ok()) {
            let Some(job) = self.job.as_mut() else { break };
            match msg {
                delete::Msg::Start(path) => job.current = path,
                delete::Msg::Bytes(n) => job.done_bytes += n,
                delete::Msg::Failed(e) => job.failed.push(e),
                delete::Msg::Item { path, freed, complete } => {
                    job.freed += freed;
                    self.freed += freed;
                    if complete {
                        job.done_items += 1;
                        job.paths.remove(&path);
                        self.items.retain(|i| i.path != path);
                        self.gone.insert(path);
                    } else if let Some(item) = self.items.iter_mut().find(|i| i.path == path) {
                        item.size = item.size.saturating_sub(freed);
                    }
                }
                delete::Msg::Done => finished = true,
            }
        }
        if !finished {
            return;
        }
        let Some(job) = self.job.take() else { return };
        self.dirs = list_dirs(&self.cwd);
        let freed = human(job.freed);
        self.status = if !job.failed.is_empty() {
            format!("Freed {freed}, {} failed: {}", job.failed.len(), job.failed[0])
        } else if job.stopped {
            format!("Stopped after {}, freed {freed}", plural(job.done_items, "folder"))
        } else {
            format!("Deleted {}, freed {freed}", plural(job.done_items, "folder"))
        };
        self.mode = Mode::Normal;
    }

    fn cancel_delete(&mut self) {
        self.status = "Deletion cancelled".into();
        self.mode = Mode::Normal;
    }

    pub(crate) fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if mods.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::Confirm => match code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('h' | 'l') => {
                    self.delete_focused = !self.delete_focused;
                }
                KeyCode::Char('y') => self.confirm_delete(),
                KeyCode::Enter | KeyCode::Char(' ') if self.delete_focused => self.confirm_delete(),
                KeyCode::Enter | KeyCode::Char(' ' | 'n' | 'q') | KeyCode::Esc => self.cancel_delete(),
                _ => {}
            },
            Mode::Deleting => {
                if matches!(code, KeyCode::Esc | KeyCode::Char('q' | 's')) {
                    if let Some(job) = self.job.as_mut() {
                        job.stopped = true;
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            Mode::Normal => {
                self.status.clear();
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
                    KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
                    KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
                    KeyCode::PageDown => self.move_cursor(10),
                    KeyCode::PageUp => self.move_cursor(-10),
                    KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
                    KeyCode::End | KeyCode::Char('G') => self.cursor = self.entries.len().saturating_sub(1),
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.enter(),
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.up(),
                    KeyCode::Char(' ') => {
                        if let Some(entry) = self.entries.get(self.cursor) {
                            if !self.selected.remove(&entry.path) {
                                self.selected.insert(entry.path.clone());
                            }
                            self.move_cursor(1);
                        }
                    }
                    KeyCode::Char('a') => {
                        self.selected = self.entries.iter().filter(|e| e.count > 0).map(|e| e.path.clone()).collect()
                    }
                    KeyCode::Char('n') => self.selected.clear(),
                    KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                        if !self.targets().is_empty() {
                            self.delete_focused = false;
                            self.mode = Mode::Confirm;
                        } else if self.entries.get(self.cursor).is_some() {
                            self.status = "No waste found there — nothing to delete".into();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn kind_totals(&self) -> Vec<(&'static str, u64)> {
        let mut totals: Vec<(&'static str, u64)> = Vec::new();
        for item in self.scope_items() {
            match totals.iter_mut().find(|(k, _)| *k == item.kind) {
                Some(slot) => slot.1 += item.size,
                None => totals.push((item.kind, item.size)),
            }
        }
        totals.sort_by(|a, b| b.1.cmp(&a.1));
        totals
    }
}
