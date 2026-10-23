use super::App;
use crate::{
    scan::Item,
    tree::{build_entries, Entry},
};

impl App {
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

    pub(crate) fn selected_size(&self) -> u64 {
        self.visible_marked().iter().map(|e| e.size).sum()
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
