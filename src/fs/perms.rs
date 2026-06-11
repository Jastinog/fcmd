//! Permission and ownership changes: `chmod` / `chown`.

use std::fs;
use std::path::Path;

#[cfg(unix)]
pub fn chmod(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub fn chmod(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "chmod is not supported on this platform",
    ))
}

#[cfg(unix)]
pub fn chown(path: &Path, uid: Option<u32>, gid: Option<u32>) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let uid = uid.map(|u| u as libc::uid_t).unwrap_or(u32::MAX);
    let gid = gid.map(|g| g as libc::gid_t).unwrap_or(u32::MAX);
    let ret = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
pub fn chown(_path: &Path, _uid: Option<u32>, _gid: Option<u32>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "chown is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fc_perms_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn chmod_changes_permissions() {
        let dir = tmp_dir();
        let f = dir.join("file.txt");
        fs::write(&f, "data").unwrap();
        chmod(&f, 0o644).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&f).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o644);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn chmod_nonexistent_fails() {
        assert!(chmod(Path::new("/nonexistent/file"), 0o755).is_err());
    }
}
