use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskStats {
    pub device: u64,
    pub total: u64,
    pub free: u64,
}

impl DiskStats {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }
}

#[cfg(unix)]
pub fn probe(path: &Path) -> Option<DiskStats> {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::MetadataExt},
    };

    let device = std::fs::metadata(path).ok()?.dev();
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c_path is a valid NUL-terminated string and stat points to writable memory.
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs returned 0, so the struct was fully initialised.
    let stat = unsafe { stat.assume_init() };
    let block = stat.f_frsize as u64;
    Some(DiskStats { device, total: stat.f_blocks as u64 * block, free: stat.f_bavail as u64 * block })
}

#[cfg(not(unix))]
pub fn probe(_path: &Path) -> Option<DiskStats> {
    None
}
