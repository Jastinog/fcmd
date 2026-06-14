//! Filesystem operations.
//!
//! - [`ops`]: copy / move / delete / rename / mkdir / touch, the yank-register
//!   and the undo stack, plus conflict-resolution and progress reporting.
//! - [`du`]: recursive directory-size calculation.
//! - [`perms`]: `chmod` / `chown`.

pub mod du;
pub mod ops;
pub mod perms;

/// Free and total bytes of the filesystem containing `path`.
///
/// `free` is the space available to an unprivileged user (not counting
/// root-reserved blocks). Returns `None` if the query fails or on platforms
/// without `statvfs` (e.g. Windows).
#[cfg(unix)]
pub fn disk_free(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // Safety: `statvfs` only reads the path and writes into the zeroed struct.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    // Block counts are in units of f_frsize (fall back to f_bsize if unset).
    let unit = if stat.f_frsize != 0 {
        stat.f_frsize as u64
    } else {
        stat.f_bsize as u64
    };
    let total = (stat.f_blocks as u64).checked_mul(unit)?;
    let free = (stat.f_bavail as u64).checked_mul(unit)?;
    Some((free, total))
}

#[cfg(not(unix))]
pub fn disk_free(_path: &std::path::Path) -> Option<(u64, u64)> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn disk_free_root_is_plausible() {
        let (free, total) = disk_free(std::path::Path::new("/")).expect("statvfs on /");
        assert!(total > 0, "total should be positive");
        assert!(free <= total, "free ({free}) must not exceed total ({total})");
    }

    #[test]
    fn disk_free_nonexistent_is_none() {
        assert!(disk_free(std::path::Path::new("/no/such/path/here/xyz")).is_none());
    }
}
