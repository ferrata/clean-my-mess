use std::path::{Path, PathBuf};

use super::App;
use crate::tree::list_dirs;

impl App {
    fn set_cwd(&mut self, path: PathBuf) {
        self.dirs = list_dirs(&path);
        self.cwd = path;
        self.entries.clear();
    }

    pub(super) fn move_cursor(&mut self, delta: isize) {
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

    pub(super) fn enter(&mut self) {
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

    pub(super) fn up(&mut self) {
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
}
