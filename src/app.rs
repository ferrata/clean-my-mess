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
    scan::{Item, Msg, Scanner},
    tree::{build_entries, list_dirs, Entry},
    ui,
};

pub(crate) enum Mode {
    Normal,
    Confirm,
    Deleting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Trash,
    Delete,
    Cancel,
}

impl Choice {
    fn next(self) -> Self {
        match self {
            Self::Trash => Self::Delete,
            Self::Delete => Self::Cancel,
            Self::Cancel => Self::Trash,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Trash => Self::Cancel,
            Self::Delete => Self::Trash,
            Self::Cancel => Self::Delete,
        }
    }
}

pub(crate) struct DeleteJob {
    pub(crate) permanent: bool,
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
    pub(crate) scanner: Option<Scanner>,
    pub(crate) base_pending: bool,
    pub(crate) pending_children: HashSet<PathBuf>,
    pub(crate) already_scanned: HashSet<PathBuf>,
    pub(crate) items: Vec<Item>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) dirs: Vec<(PathBuf, String)>,
    pub(crate) selected: HashSet<PathBuf>,
    pub(crate) cursor: usize,
    pub(crate) focus: Option<PathBuf>,
    pub(crate) mode: Mode,
    pub(crate) choice: Choice,
    pub(crate) trash_dir: Option<PathBuf>,
    pub(crate) trashed: u64,
    pub(crate) job: Option<DeleteJob>,
    pub(crate) gone: HashSet<PathBuf>,
    pub(crate) scanning: bool,
    pub(crate) visited: usize,
    pub(crate) freed: u64,
    pub(crate) status: String,
    pub(crate) quit: bool,
    pub(crate) started: Instant,
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(scanner) = &self.scanner {
            scanner.stop();
        }
    }
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let scanner = Scanner::start(root.clone(), tx);
        let mut app = Self::build(root, rx, Some(scanner), true);
        app.refresh();
        app
    }

    pub fn with_items(root: PathBuf, items: Vec<Item>) -> Self {
        let (_tx, rx) = mpsc::channel();
        let mut app = Self::build(root, rx, None, false);
        app.items = items;
        app.refresh();
        app
    }

    #[doc(hidden)]
    pub fn with_unfinished_scan(root: PathBuf, items: Vec<Item>, pending: Vec<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self::build(root, rx, Some(Scanner::idle(pending, tx)), true);
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

    pub fn choice(&self) -> Choice {
        self.choice
    }

    pub fn set_trash_dir(&mut self, dir: Option<PathBuf>) {
        self.trash_dir = dir;
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

    fn build(root: PathBuf, rx: Receiver<Msg>, scanner: Option<Scanner>, scanning: bool) -> Self {
        Self {
            dirs: list_dirs(&root),
            cwd: root.clone(),
            root,
            rx,
            scanner,
            base_pending: false,
            pending_children: HashSet::new(),
            already_scanned: HashSet::new(),
            items: Vec::new(),
            entries: Vec::new(),
            selected: HashSet::new(),
            cursor: 0,
            focus: None,
            mode: Mode::Normal,
            choice: Choice::Cancel,
            trash_dir: delete::default_trash_dir(),
            trashed: 0,
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
                Msg::Progress(_) | Msg::Done => {}
            }
        }
    }

    pub fn pump(&mut self) {
        self.drain_scan();
        self.drain_delete();
        self.refresh();
    }

    pub(crate) fn view_complete(&self) -> bool {
        !self.base_pending && self.pending_children.is_empty()
    }

    pub(crate) fn entry_pending(&self, entry: &Entry) -> bool {
        let unknown = self.base_pending && !self.already_scanned.contains(&entry.path);
        !entry.is_waste() && (unknown || self.pending_children.contains(&entry.path))
    }

    fn sync_progress(&mut self) {
        if let Some(scanner) = &self.scanner {
            let progress = scanner.progress(&self.cwd);
            self.base_pending = progress.base_pending;
            self.pending_children = progress.pending_children;
            self.already_scanned = progress.already_scanned;
            self.visited = progress.visited;
            self.scanning = progress.running;
        }
    }

    pub(crate) fn refresh(&mut self) {
        self.sync_progress();
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
        let marked = self.visible_marked();
        let picked: Vec<&Entry> =
            if marked.is_empty() { self.entries.get(self.cursor).into_iter().collect() } else { marked };
        picked.into_iter().filter(|e| e.count > 0).collect()
    }

    pub(crate) fn visible_marked(&self) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.count > 0 && self.selected.contains(&e.path)).collect()
    }

    pub(crate) fn hidden_marks(&self) -> usize {
        self.selected.len() - self.entries.iter().filter(|e| self.selected.contains(&e.path)).count()
    }

    fn set_cwd(&mut self, path: PathBuf) {
        self.dirs = list_dirs(&path);
        self.cwd = path;
        self.entries.clear();
    }

    pub(crate) fn selected_size(&self) -> u64 {
        self.visible_marked().iter().map(|e| e.size).sum()
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let max = self.entries.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
    }

    fn prioritize_view(&self) {
        if let Some(scanner) = &self.scanner {
            scanner.prioritize(&self.cwd);
        }
    }

    fn enter(&mut self) {
        let Some(entry) = self.entries.get(self.cursor) else { return };
        if entry.is_waste() {
            self.status = "This is a waste folder itself — press d to delete it".into();
            return;
        }
        let path = entry.path.clone();
        self.cursor = 0;
        self.set_cwd(path);
        self.prioritize_view();
    }

    fn up(&mut self) {
        let came_from = self.cwd.clone();
        if self.cwd == self.root {
            match self.root.parent().map(Path::to_path_buf) {
                Some(parent) => {
                    let old_root = std::mem::replace(&mut self.root, parent.clone());
                    self.set_cwd(parent.clone());
                    self.cursor = 0;
                    self.focus = Some(came_from);
                    if let Some(scanner) = &self.scanner {
                        scanner.extend(parent, old_root);
                    }
                }
                None => self.status = "Already at the filesystem root".into(),
            }
        } else if let Some(parent) = self.cwd.parent() {
            self.set_cwd(parent.to_path_buf());
            self.focus = Some(came_from);
            self.prioritize_view();
        }
    }

    fn confirm_delete(&mut self, permanent: bool) {
        let mode = if permanent {
            delete::Mode::Permanent
        } else {
            match self.trash_dir.clone() {
                Some(dir) => delete::Mode::Trash(dir),
                None => {
                    self.status = "Moving to Trash isn't supported on this platform — use permanent delete".into();
                    self.mode = Mode::Normal;
                    return;
                }
            }
        };
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
        let work = doomed.clone();
        thread::spawn(move || delete::run(work, mode, tx, flag));
        self.job = Some(DeleteJob {
            permanent,
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
        self.selected.retain(|p| !target_paths.iter().any(|t| p.starts_with(t)));
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
                delete::Msg::Item { path, bytes, complete } => {
                    job.freed += bytes;
                    if job.permanent {
                        self.freed += bytes;
                    } else {
                        self.trashed += bytes;
                    }
                    if complete {
                        job.done_items += 1;
                        job.paths.remove(&path);
                        self.items.retain(|i| i.path != path);
                        self.gone.insert(path);
                    } else if let Some(item) = self.items.iter_mut().find(|i| i.path == path) {
                        item.size = item.size.saturating_sub(bytes);
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
        let size = human(job.freed);
        let (done, action) = if job.permanent { ("freed", "Deleted") } else { ("moved", "Moved to Trash:") };
        self.status = if !job.failed.is_empty() {
            format!("{size} {done}, {} failed: {}", job.failed.len(), job.failed[0])
        } else if job.stopped {
            format!("Stopped after {}, {size} {done}", plural(job.done_items, "folder"))
        } else if job.permanent {
            format!("{action} {}, freed {size}", plural(job.done_items, "folder"))
        } else {
            format!("{action} {} ({size}) — space is freed when you empty the Trash", plural(job.done_items, "folder"))
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
                KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => self.choice = self.choice.next(),
                KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => self.choice = self.choice.prev(),
                KeyCode::Char('t') => self.confirm_delete(false),
                KeyCode::Char('y') => self.confirm_delete(true),
                KeyCode::Enter | KeyCode::Char(' ') => match self.choice {
                    Choice::Trash => self.confirm_delete(false),
                    Choice::Delete => self.confirm_delete(true),
                    Choice::Cancel => self.cancel_delete(),
                },
                KeyCode::Char('n' | 'q') | KeyCode::Esc => self.cancel_delete(),
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
                        let visible: Vec<PathBuf> =
                            self.entries.iter().filter(|e| e.count > 0).map(|e| e.path.clone()).collect();
                        self.selected.extend(visible);
                    }
                    KeyCode::Char('n') => self.selected.clear(),
                    KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                        if !self.targets().is_empty() {
                            self.choice = Choice::Cancel;
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
