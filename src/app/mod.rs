mod delete_job;
mod keys;
mod nav;
mod view;

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyModifiers},
    Terminal,
};

use crate::{
    delete,
    disk::{self, DiskStats},
    scan::{Item, Msg, Scanner},
    tree::{list_dirs, Entry},
    ui,
};
use delete_job::DeleteJob;

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
    pub(crate) disk: Option<DiskStats>,
    pub(crate) gained: Option<i128>,
    pub(crate) probe_disk: bool,
    pub(crate) last_probe: Option<Instant>,
    pub(crate) last_rate: Option<f64>,
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
        app.probe_disk = true;
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

    #[doc(hidden)]
    pub fn set_last_delete_rate(&mut self, bytes_per_sec: Option<f64>) {
        self.last_rate = bytes_per_sec;
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
            disk: None,
            gained: None,
            probe_disk: false,
            last_probe: None,
            last_rate: None,
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

    #[doc(hidden)]
    pub fn set_drive(&mut self, stats: Option<DiskStats>) {
        self.disk = stats;
    }

    fn poll_disk(&mut self, force: bool) {
        let due = self.last_probe.map_or(true, |t| t.elapsed() >= Duration::from_secs(1));
        if self.probe_disk && (force || due) {
            self.last_probe = Some(Instant::now());
            self.set_drive(disk::probe(&self.cwd));
        }
    }

    pub fn pump(&mut self) {
        self.poll_disk(false);
        self.drain_scan();
        self.drain_delete();
        self.refresh();
    }
}
