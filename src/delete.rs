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
    Item { path: PathBuf, freed: u64, complete: bool },
    Failed(String),
    Done,
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

pub fn run(paths: Vec<PathBuf>, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if tx.send(Msg::Start(path.clone())).is_err() {
            return;
        }
        let (mut freed, mut pending, mut last) = (0u64, 0u64, Instant::now());
        let result = remove_tree(&path, &cancel, &mut |bytes| {
            freed += bytes;
            pending += bytes;
            if last.elapsed() >= REPORT_EVERY {
                let _ = tx.send(Msg::Bytes(pending));
                pending = 0;
                last = Instant::now();
            }
        });
        let _ = tx.send(Msg::Bytes(pending));
        let complete = match result {
            Ok(done) => done,
            Err(e) => {
                let _ = tx.send(Msg::Failed(format!("{}: {e}", path.display())));
                false
            }
        };
        if tx.send(Msg::Item { path, freed, complete }).is_err() {
            return;
        }
    }
    let _ = tx.send(Msg::Done);
}
