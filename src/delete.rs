use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    time::{Duration, Instant},
};

use crate::scan::disk_usage;

pub enum Msg {
    Start(PathBuf),
    Bytes(u64),
    Item { path: PathBuf, bytes: u64, complete: bool },
    Failed(String),
    Done,
}

pub enum Mode {
    Permanent,
    Trash(PathBuf),
}

pub fn default_trash_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Trash"))
    } else {
        None
    }
}

fn trash_name(path: &Path) -> String {
    let name = path.file_name().map_or_else(|| "folder".into(), |n| n.to_string_lossy().into_owned());
    match path.parent().and_then(Path::file_name) {
        Some(parent) => format!("{name} ({})", parent.to_string_lossy()),
        None => name,
    }
}

fn move_to_trash(path: &Path, trash: &Path) -> io::Result<()> {
    fs::create_dir_all(trash)?;
    let base = trash_name(path);
    let mut dest = trash.join(&base);
    let mut n = 2;
    while fs::symlink_metadata(&dest).is_ok() {
        dest = trash.join(format!("{base} {n}"));
        n += 1;
    }
    fs::rename(path, &dest).map_err(|e| {
        if e.raw_os_error() == Some(18) {
            io::Error::new(e.kind(), "Trash is on a different volume; use permanent delete for this folder")
        } else {
            e
        }
    })
}

const REPORT_EVERY: Duration = Duration::from_millis(40);

fn remove_tree(root: &Path, cancel: &AtomicBool, report: &mut impl FnMut(u64)) -> io::Result<bool> {
    let meta = fs::symlink_metadata(root)?;
    if !meta.is_dir() {
        fs::remove_file(root)?;
        report(disk_usage(&meta));
        return Ok(true);
    }
    let mut stack = vec![(root.to_path_buf(), false)];
    while let Some((dir, expanded)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if expanded {
            let meta = fs::symlink_metadata(&dir)?;
            fs::remove_dir(&dir)?;
            report(disk_usage(&meta));
            continue;
        }
        stack.push((dir.clone(), true));
        for entry in fs::read_dir(&dir)? {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push((path, false));
            } else {
                let size = entry.metadata().map(|m| disk_usage(&m)).unwrap_or(0);
                fs::remove_file(&path)?;
                report(size);
            }
        }
    }
    Ok(true)
}

pub fn run(items: Vec<(PathBuf, u64)>, mode: Mode, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    for (path, size) in items {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if tx.send(Msg::Start(path.clone())).is_err() {
            return;
        }
        let (bytes, complete) = match &mode {
            Mode::Permanent => remove_permanently(&path, &tx, &cancel),
            Mode::Trash(dir) => match move_to_trash(&path, dir) {
                Ok(()) => {
                    let _ = tx.send(Msg::Bytes(size));
                    (size, true)
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("{}: {e}", path.display())));
                    (0, false)
                }
            },
        };
        if tx.send(Msg::Item { path, bytes, complete }).is_err() {
            return;
        }
    }
    let _ = tx.send(Msg::Done);
}

fn remove_permanently(path: &Path, tx: &Sender<Msg>, cancel: &AtomicBool) -> (u64, bool) {
    let (mut freed, mut pending, mut last) = (0u64, 0u64, Instant::now());
    let result = remove_tree(path, cancel, &mut |bytes| {
        freed += bytes;
        pending += bytes;
        if last.elapsed() >= REPORT_EVERY {
            let _ = tx.send(Msg::Bytes(pending));
            pending = 0;
            last = Instant::now();
        }
    });
    let _ = tx.send(Msg::Bytes(pending));
    match result {
        Ok(done) => (freed, done),
        Err(e) => {
            let _ = tx.send(Msg::Failed(format!("{}: {e}", path.display())));
            (freed, false)
        }
    }
}
