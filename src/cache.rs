use crate::providers::Status;
use std::io::Write;
use std::path::{Path, PathBuf};

fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache")
        });
    base.join("lazysubs")
}

fn cache_file() -> PathBuf {
    cache_dir().join("status.json")
}

pub fn pi_daily_index_file() -> PathBuf {
    cache_dir().join("pi-daily-token-index-v1.json")
}

pub fn atomic_save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_save_with_rename(path, bytes, &|from, to| std::fs::rename(from, to))
}

fn atomic_save_with_rename(
    path: &Path,
    bytes: &[u8],
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing cache directory"))?;
    std::fs::create_dir_all(dir)?;
    let nonce = format!(
        "{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let temp = dir.join(format!(".pi-index-{nonce}.tmp"));
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        rename(&temp, path)?;
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
    let raw = std::fs::read_to_string(cache_file()).ok()?;
    let status: Status = serde_json::from_str(&raw).ok()?;
    let age = chrono::Utc::now().timestamp() - status.fetched_at;
    (age >= 0 && age < ttl_secs).then_some(status)
}

pub fn save(status: &Status) {
    let path = cache_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(raw) = serde_json::to_string(status) {
        let _ = std::fs::write(path, raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_save_replaces_a_complete_index() {
        let path = std::env::temp_dir().join(format!("lazysubs-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let index = path.join("index.json");
        atomic_save(&index, br#"{"version":1}"#).unwrap();
        atomic_save(&index, br#"{"version":2}"#).unwrap();
        assert_eq!(std::fs::read(&index).unwrap(), br#"{"version":2}"#);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn failed_final_rename_keeps_the_previous_complete_index_readable() {
        let path = std::env::temp_dir().join(format!("lazysubs-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let index = path.join("index.json");
        let previous = br#"{"version":1}"#;
        atomic_save(&index, previous).unwrap();

        let result = atomic_save_with_rename(&index, br#"{"version":2}"#, &|_, _| {
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
}
