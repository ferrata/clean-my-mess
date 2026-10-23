use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::Instant,
};

use super::{App, Mode};
use crate::{
    delete,
    disk::DiskStats,
    format::{human, plural},
    tree::list_dirs,
};

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
    pub(crate) free_before: Option<DiskStats>,
    pub(crate) started: Instant,
}

impl App {
    pub(super) fn confirm_delete(&mut self, permanent: bool) {
        let Some(mode) = self.delete_mode(permanent) else { return };
        let target_paths: Vec<PathBuf> = self.targets().iter().map(|e| e.path.clone()).collect();
        let doomed = self.doomed_items(&target_paths);
        if doomed.is_empty() {
            self.mode = Mode::Normal;
            return;
        }
        self.start_job(permanent, mode, doomed);
        self.selected.retain(|p| !target_paths.iter().any(|t| p.starts_with(t)));
        self.mode = Mode::Deleting;
    }

    fn delete_mode(&mut self, permanent: bool) -> Option<delete::Mode> {
        if permanent {
            return Some(delete::Mode::Permanent);
        }
        match self.trash_dir.clone() {
            Some(dir) => Some(delete::Mode::Trash(dir)),
            None => {
                self.status = "Moving to Trash isn't supported on this platform — use permanent delete".into();
                self.mode = Mode::Normal;
                None
            }
        }
    }

    fn doomed_items(&self, target_paths: &[PathBuf]) -> Vec<(PathBuf, u64)> {
        self.items
            .iter()
            .filter(|i| target_paths.iter().any(|t| i.path.starts_with(t)))
            .map(|i| (i.path.clone(), i.size))
            .collect()
    }

    fn start_job(&mut self, permanent: bool, mode: delete::Mode, doomed: Vec<(PathBuf, u64)>) {
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
            free_before: self.disk,
            started: Instant::now(),
        });
    }

    pub(super) fn drain_delete(&mut self) {
        let mut finished = false;
        while let Some(msg) = self.job.as_ref().and_then(|j| j.rx.try_recv().ok()) {
            finished |= self.apply_delete_msg(msg);
        }
        if finished {
            self.finish_delete();
        }
    }

    fn apply_delete_msg(&mut self, msg: delete::Msg) -> bool {
        let Some(job) = self.job.as_mut() else { return false };
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
            delete::Msg::Done => return true,
        }
        false
    }

    fn finish_delete(&mut self) {
        let Some(job) = self.job.take() else { return };
        self.poll_disk(true);
        self.measure_gain(&job);
        self.remember_rate(&job);
        self.dirs = list_dirs(&self.cwd);
        self.status = job_summary(&job);
        self.mode = Mode::Normal;
    }

    fn measure_gain(&mut self, job: &DeleteJob) {
        if let (true, Some(before), Some(after)) = (job.permanent, job.free_before, self.disk) {
            if before.device == after.device {
                let delta = i128::from(after.free) - i128::from(before.free);
                self.gained = Some(self.gained.unwrap_or(0) + delta);
            }
        }
    }

    fn remember_rate(&mut self, job: &DeleteJob) {
        let secs = job.started.elapsed().as_secs_f64();
        if job.permanent && !job.stopped && job.failed.is_empty() && secs >= 1.0 && job.done_bytes > 0 {
            self.last_rate = Some(job.done_bytes as f64 / secs);
        }
    }

    pub(super) fn stop_delete(&mut self) {
        if let Some(job) = self.job.as_mut() {
            job.stopped = true;
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn cancel_delete(&mut self) {
        self.status = "Deletion cancelled".into();
        self.mode = Mode::Normal;
    }
}

fn job_summary(job: &DeleteJob) -> String {
    let size = human(job.freed);
    let (done, action) = if job.permanent { ("freed", "Deleted") } else { ("moved", "Moved to Trash:") };
    if !job.failed.is_empty() {
        format!("{size} {done}, {} failed: {}", job.failed.len(), job.failed[0])
    } else if job.stopped {
        format!("Stopped after {}, {size} {done}", plural(job.done_items, "folder"))
    } else if job.permanent {
        format!("{action} {}, freed {size}", plural(job.done_items, "folder"))
    } else {
        format!("{action} {} ({size}) — space is freed when you empty the Trash", plural(job.done_items, "folder"))
    }
}
