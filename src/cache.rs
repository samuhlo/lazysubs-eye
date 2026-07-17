use crate::file_lock::{FileLock, LockMode};
use crate::providers::Status;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
pub enum AtomicSaveError {
    SymlinkPolicyViolation,
    LockNotAvailable,
    LockTimeout,
    #[allow(dead_code)] // [NOTE] Reserved for a lock backend that can detect lease loss.
    LockLost,
    PermissionChangeFailed(std::io::Error),
    Io(std::io::Error),
}

impl fmt::Display for AtomicSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SymlinkPolicyViolation => {
                write!(f, "la ruta de persistencia contiene un enlace simbólico")
            }
            Self::LockNotAvailable => write!(f, "otro proceso está escribiendo este estado"),
            Self::LockTimeout => write!(f, "se agotó la espera del bloqueo de escritura"),
            Self::LockLost => write!(f, "se perdió el bloqueo de escritura"),
            Self::PermissionChangeFailed(_) => write!(f, "no se pudieron fijar permisos privados"),
            Self::Io(_) => write!(f, "falló la persistencia local"),
        }
    }
}

impl std::error::Error for AtomicSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PermissionChangeFailed(err) | Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AtomicSaveError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache")
        });
    base.join("lazysubs-eye")
}

/// [CACHE] SHARED STATE DIRECTORY
///
/// Holds status snapshots, token indexes, and notification state so separate
/// commands observe the same local view.
pub fn dir() -> PathBuf {
    cache_dir()
}

fn cache_file() -> PathBuf {
    cache_dir().join("status.json")
}

pub fn pi_daily_index_file() -> PathBuf {
    cache_dir().join("pi-daily-token-index-v1.json")
}

pub fn opencode_daily_index_file() -> PathBuf {
    cache_dir().join("opencode-daily-token-index-v1.json")
}

/// [CACHE] ATOMIC PUBLISH
///
/// Writes and flushes a sibling temporary file, then renames it over the target.
/// WHY: readers see either the old complete document or the new complete document.
/// INVARIANT: the destination and every existing ancestor must not be symlinks.
pub fn atomic_save(path: &Path, bytes: &[u8]) -> Result<(), AtomicSaveError> {
    atomic_save_with_rename(path, bytes, true, &|| Ok(()), &|from, to| {
        std::fs::rename(from, to)
    })
}

/// [CACHE] ATOMIC SYSTEM CONFIG PUBLISH
///
/// Keeps the rename and symlink protections but preserves permissions owned by
/// the surrounding desktop configuration.
pub fn atomic_save_system(path: &Path, bytes: &[u8]) -> Result<(), AtomicSaveError> {
    atomic_save_with_rename(path, bytes, false, &|| Ok(()), &|from, to| {
        std::fs::rename(from, to)
    })
}

/// [CACHE] LOCKED ATOMIC PUBLISH
///
/// Acquires the sibling lock before preparing the replacement, then verifies the
/// lock identity immediately before rename. WHY: a replaced lock file cannot
/// silently let two writers publish competing snapshots.
pub fn atomic_save_locked(
    path: &Path,
    bytes: &[u8],
    mode: LockMode,
    timeout: Duration,
) -> Result<(), AtomicSaveError> {
    let lock_path = path.with_extension(format!(
        "{}.lock",
        path.extension().and_then(|x| x.to_str()).unwrap_or("lock")
    ));
    let lock = FileLock::acquire(&lock_path, mode, timeout)?;
    atomic_save_with_rename(path, bytes, true, &|| lock.verify(), &|from, to| {
        std::fs::rename(from, to)
    })
}

pub fn set_permissions_restrictive(path: &Path, is_dir: bool) -> Result<(), AtomicSaveError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if is_dir { 0o700 } else { 0o600 }),
        )
        .map_err(AtomicSaveError::PermissionChangeFailed)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, is_dir);
        Err(AtomicSaveError::PermissionChangeFailed(
            std::io::Error::other("permisos privados no soportados en esta plataforma"),
        ))
    }
}

/// [CORE] SYMLINK BOUNDARY
///
/// Walks every existing ancestor with `symlink_metadata`, which inspects the
/// link itself rather than its target. FAIL CLOSED -> persistence never escapes
/// the requested tree through a substituted path.
fn reject_symlinks(path: &Path) -> Result<(), AtomicSaveError> {
    for component in path.ancestors() {
        match std::fs::symlink_metadata(component) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AtomicSaveError::SymlinkPolicyViolation)
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

/// [CACHE] DURABLE REPLACEMENT PROTOCOL
///
/// Creates a unique sibling file, writes and syncs its bytes, verifies the
/// pre-commit boundary, then renames it into place and syncs the directory.
/// FAILURE MODE: any step removes the temporary file and preserves the old target.
fn atomic_save_with_rename(
    path: &Path,
    bytes: &[u8],
    private: bool,
    before_commit: &dyn Fn() -> Result<(), AtomicSaveError>,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), AtomicSaveError> {
    let dir = path
        .parent()
        .ok_or_else(|| AtomicSaveError::Io(std::io::Error::other("missing cache directory")))?;
    reject_symlinks(path)?;
    std::fs::create_dir_all(dir)?;
    if private {
        set_permissions_restrictive(dir, true)?;
    }
    let nonce = format!(
        "{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let temp = dir.join(format!(".pi-index-{nonce}.tmp"));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if private {
            // FAIL CLOSED -> do not publish private state when the filesystem rejects 0600.
            set_permissions_restrictive(&temp, false)?;
        }
        before_commit()?;
        rename(&temp, path)?;
        if private {
            set_permissions_restrictive(path, false)?;
        }
        #[cfg(unix)]
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub fn load(ttl_secs: i64) -> Option<Status> {
    let status = load_stale()?;
    let age = chrono::Utc::now().timestamp() - status.fetched_at;
    (age >= 0 && age < ttl_secs).then_some(status)
}

/// [CACHE] STALE FALLBACK
///
/// Returns the last snapshot even after its TTL expires, allowing callers to
/// degrade with known data when a provider fails, such as with HTTP 429.
pub fn load_stale() -> Option<Status> {
    let raw = std::fs::read_to_string(cache_file()).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save(status: &Status) {
    let path = cache_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(raw) = serde_json::to_string(status) {
        let _ = atomic_save_locked(
            &path,
            raw.as_bytes(),
            LockMode::Blocking,
            Duration::from_millis(100),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_daily_index_has_an_independent_v1_name() {
        assert!(opencode_daily_index_file()
            .ends_with("lazysubs-eye/opencode-daily-token-index-v1.json"));
    }

    #[test]
    fn atomic_save_replaces_a_complete_index() {
        let path = std::env::temp_dir().join(format!("lazysubs-eye-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let index = path.join("index.json");
        atomic_save(&index, br#"{"version":1}"#).unwrap();
        atomic_save(&index, br#"{"version":2}"#).unwrap();
        assert_eq!(std::fs::read(&index).unwrap(), br#"{"version":2}"#);
        let _ = std::fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_makes_the_file_and_directory_private() {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("lazysubs-eye-permissions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let file = path.join("state.json");
        atomic_save(&file, b"ok").unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_rejects_a_symlink_destination() {
        let path =
            std::env::temp_dir().join(format!("lazysubs-eye-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let target = path.join("target");
        let link = path.join("state.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(
            atomic_save(&link, b"no"),
            Err(AtomicSaveError::SymlinkPolicyViolation)
        ));
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_rejects_a_symlink_parent() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-eye-parent-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("linked")).unwrap();
        let result = atomic_save(&root.join("linked/state.json"), b"no");
        assert!(matches!(
            result,
            Err(AtomicSaveError::SymlinkPolicyViolation)
        ));
        assert!(!root.join("real/state.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_final_rename_keeps_the_previous_complete_index_readable() {
        let path = std::env::temp_dir().join(format!("lazysubs-eye-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let index = path.join("index.json");
        let previous = br#"{"version":1}"#;
        atomic_save(&index, previous).unwrap();

        let result =
            atomic_save_with_rename(&index, br#"{"version":2}"#, true, &|| Ok(()), &|_, _| {
                Err(std::io::Error::other("injected rename failure"))
            });

        assert!(result.is_err());
        let raw = std::fs::read(&index).unwrap();
        assert_eq!(raw, previous);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&raw).unwrap()["version"],
            1
        );
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn lost_lock_aborts_before_replacing_destination() {
        let root = std::env::temp_dir().join(format!("lazysubs-lost-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("status.json");
        std::fs::write(&destination, b"old").unwrap();
        let lock_path = root.join("status.json.lock");
        let lock = FileLock::acquire(&lock_path, LockMode::NonBlocking, Duration::ZERO).unwrap();
        std::fs::remove_file(&lock_path).unwrap();
        let result = atomic_save_with_rename(
            &destination,
            b"new",
            true,
            &|| lock.verify(),
            &|from, to| std::fs::rename(from, to),
        );
        assert!(matches!(result, Err(AtomicSaveError::LockLost)));
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(root);
    }
}
