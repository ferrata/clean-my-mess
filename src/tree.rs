use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::scan::Item;

pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) count: usize,
    pub(crate) kind: Option<&'static str>,
    pub(crate) by_kind: Vec<(&'static str, u64)>,
}

impl Entry {
    pub(crate) fn dominant(&self) -> &'static str {
        self.by_kind.iter().max_by_key(|(_, s)| *s).map(|(k, _)| *k).unwrap_or("")
    }

    pub(crate) fn is_waste(&self) -> bool {
        self.kind.is_some()
    }
}

pub(crate) fn list_dirs(path: &Path) -> Vec<(PathBuf, String)> {
    let Ok(read) = fs::read_dir(path) else { return Vec::new() };
    read.flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok().map(|name| (e.path(), name)))
        .filter(|(_, name)| name != ".git")
        .collect()
}

fn blank_entry(path: PathBuf, name: String) -> Entry {
    Entry { path, name, size: 0, count: 0, kind: None, by_kind: Vec::new() }
}

pub(crate) fn build_entries(items: &[Item], cwd: &Path, dirs: &[(PathBuf, String)]) -> Vec<Entry> {
    let mut map: HashMap<PathBuf, Entry> =
        dirs.iter().map(|(path, name)| (path.clone(), blank_entry(path.clone(), name.clone()))).collect();
    for item in items {
        let Ok(rel) = item.path.strip_prefix(cwd) else { continue };
        let mut comps = rel.components();
        let Some(first) = comps.next() else { continue };
        let is_self = comps.next().is_none();
        let path = cwd.join(first);
        let entry = map
            .entry(path.clone())
            .or_insert_with(|| blank_entry(path, first.as_os_str().to_string_lossy().into_owned()));
        entry.size += item.size;
        entry.count += 1;
        if is_self {
            entry.kind = Some(item.kind);
        }
        match entry.by_kind.iter_mut().find(|(k, _)| *k == item.kind) {
            Some(slot) => slot.1 += item.size,
            None => entry.by_kind.push((item.kind, item.size)),
        }
    }
    let mut entries: Vec<Entry> = map.into_values().collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    entries
}
