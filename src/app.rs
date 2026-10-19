use std::{
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
    scan::{self, Item, Msg},
    tree::{build_entries, list_dirs, Entry},
    ui,
};

pub struct App {
    pub(crate) root: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) rx: Receiver<Msg>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) items: Vec<Item>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) dirs: Vec<(PathBuf, String)>,
    pub(crate) cursor: usize,
    pub(crate) focus: Option<PathBuf>,
    pub(crate) scanning: bool,
    pub(crate) visited: usize,
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
            cursor: 0,
            focus: None,
            scanning,
            visited: 0,
            status: String::new(),
            quit: false,
            started: Instant::now(),
        }
    }

    fn drain_scan(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Found(item) => self.items.push(item),
                Msg::Progress(n) => self.visited = n,
                Msg::Done => self.scanning = false,
            }
        }
    }

    pub fn pump(&mut self) {
        self.drain_scan();
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

    fn set_cwd(&mut self, path: PathBuf) {
        self.dirs = list_dirs(&path);
        self.cwd = path;
        self.entries.clear();
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
            self.status = "This is a waste folder itself — there is nothing to open".into();
            return;
        }
        let path = entry.path.clone();
        self.cursor = 0;
        if self.scanning {
            self.rescan(path, None);
        } else {
            self.set_cwd(path);
        }
    }

    fn up(&mut self) {
        let came_from = self.cwd.clone();
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

    pub(crate) fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if mods.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
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
            _ => {}
        }
    }
}
