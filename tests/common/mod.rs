use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let name = format!("cmm-test-{}-{}-{nanos}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path.canonicalize().expect("canonicalize"))
    }

    pub fn file(&self, rel: &str, bytes: usize) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, vec![b'x'; bytes]).expect("write");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let is_ours = self.0.starts_with(std::env::temp_dir().canonicalize().unwrap_or_default())
            && self.0.file_name().is_some_and(|n| n.to_string_lossy().starts_with("cmm-test-"));
        if is_ours {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
