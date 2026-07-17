//! [CORE] ADVISORY FILE LOCKS
//!
//! Unix `flock(2)` releases the lock when its process exits, including after a
//! crash. Locks are per file rather than global, so unrelated state updates do
//! not serialize behind each other.

use crate::cache::AtomicSaveError;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Blocking,
    NonBlocking,
}

pub struct FileLockGuard {
    file: File,
    path: PathBuf,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

impl FileLockGuard {
    /// [CORE] BOUNDED LOCK ACQUISITION
    ///
    /// Opens a private lock file and retries non-blocking `flock` every 10 ms
    /// until the deadline. Non-blocking mode reports contention immediately.
    /// TRADE-OFF: polling keeps a timeout possible because blocking `flock` cannot.
    fn acquire(path: &Path, mode: LockMode, timeout: Duration) -> Result<Self, AtomicSaveError> {
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        let (dev, ino) = {
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            (metadata.dev(), metadata.ino())
        };
        let deadline = Instant::now() + timeout;
        loop {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(Self {
                    file,
                    path: path.to_owned(),
                    #[cfg(unix)]
                    dev,
                    #[cfg(unix)]
                    ino,
                });
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::WouldBlock {
                return Err(AtomicSaveError::Io(err));
            }
            if mode == LockMode::NonBlocking {
                return Err(AtomicSaveError::LockNotAvailable);
            }
            if Instant::now() >= deadline {
                return Err(AtomicSaveError::LockTimeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// [CORE] LOCK IDENTITY CHECK
    ///
    /// Detects replacement or deletion of the lock file during a write. Another
    /// process could lock the new inode while this guard still owns the old one,
    /// so publication must abort.
    pub fn verify(&self) -> Result<(), AtomicSaveError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(&self.path).map_err(|_| AtomicSaveError::LockLost)?;
            if metadata.dev() != self.dev || metadata.ino() != self.ino {
                return Err(AtomicSaveError::LockLost);
            }
        }
        Ok(())
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        // [FLOW] Drop cannot report failure; closing the fd also releases flock,
        // so this explicit unlock only shortens a waiting process's delay.
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

pub struct FileLock;

impl FileLock {
    pub fn acquire(
        path: &Path,
        mode: LockMode,
        timeout: Duration,
    ) -> Result<FileLockGuard, AtomicSaveError> {
        FileLockGuard::acquire(path, mode, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_blocking_lock_reports_contention_and_drop_releases_it() {
        let path = std::env::temp_dir().join(format!("lazysubs-eye-lock-{}", std::process::id()));
        let first = FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        assert!(matches!(
            FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO),
            Err(AtomicSaveError::LockNotAvailable)
        ));
        drop(first);
        FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn blocking_lock_times_out_and_then_recovers() {
        let path =
            std::env::temp_dir().join(format!("lazysubs-eye-lock-timeout-{}", std::process::id()));
        let first = FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        let started = Instant::now();
        assert!(matches!(
            FileLock::acquire(&path, LockMode::Blocking, Duration::from_millis(20)),
            Err(AtomicSaveError::LockTimeout)
        ));
        assert!(started.elapsed() >= Duration::from_millis(20));
        drop(first);
        FileLock::acquire(&path, LockMode::Blocking, Duration::from_millis(20)).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn multiprocess_lock_reports_real_contention() {
        const CHILD_PATH: &str = "LAZYSUBS_TEST_LOCK_CHILD_PATH";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let result = FileLock::acquire(Path::new(&path), LockMode::NonBlocking, Duration::ZERO);
            std::process::exit(
                if matches!(result, Err(AtomicSaveError::LockNotAvailable)) {
                    42
                } else {
                    43
                },
            );
        }

        let path =
            std::env::temp_dir().join(format!("lazysubs-eye-lock-process-{}", std::process::id()));
        let guard = FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "file_lock::tests::multiprocess_lock_reports_real_contention",
                "--nocapture",
            ])
            .env(CHILD_PATH, &path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(42));
        drop(guard);
        FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn kernel_releases_lock_when_process_exits_without_drop() {
        const CHILD_PATH: &str = "LAZYSUBS_TEST_LOCK_CRASH_PATH";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let _guard =
                FileLock::acquire(Path::new(&path), LockMode::NonBlocking, Duration::ZERO).unwrap();
            std::process::exit(0);
        }
        let path =
            std::env::temp_dir().join(format!("lazysubs-eye-lock-crash-{}", std::process::id()));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "file_lock::tests::kernel_releases_lock_when_process_exits_without_drop",
                "--nocapture",
            ])
            .env(CHILD_PATH, &path)
            .status()
            .unwrap();
        assert!(status.success());
        FileLock::acquire(&path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        let _ = std::fs::remove_file(path);
    }
}
