use crate::providers::Status;
use std::path::PathBuf;

fn cache_file() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache"));
    base.join("lazysubs/status.json")
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
