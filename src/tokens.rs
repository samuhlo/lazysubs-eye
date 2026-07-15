use chrono::Timelike;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct ModelTokens {
    pub model: String,
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

impl ModelTokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_creation
    }
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

/// Tokens de hoy por modelo, agregados de los JSONL de `~/.claude/projects`.
/// Solo abre ficheros modificados hoy; el filtro fino es el timestamp de cada entrada.
pub fn claude_today() -> Vec<ModelTokens> {
    let home = std::env::var("HOME").unwrap_or_default();
    let projects = PathBuf::from(home).join(".claude/projects");
    let today = chrono::Local::now().date_naive();
    let midnight = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let mut by_model: HashMap<String, ModelTokens> = HashMap::new();

    let dirs = match std::fs::read_dir(&projects) {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let mtime = file
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if mtime < midnight {
                continue;
            }
            let Ok(f) = std::fs::File::open(&path) else {
                continue;
            };
            for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                let Ok(entry) = serde_json::from_str::<Entry>(&line) else {
                    continue;
                };
                if entry.kind != "assistant" {
                    continue;
                }
                let entry_is_today = entry
                    .timestamp
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Local).date_naive() == today)
                    .unwrap_or(false);
                if !entry_is_today {
                    continue;
                }
                let Some(msg) = entry.message else { continue };
                let (Some(model), Some(usage)) = (msg.model, msg.usage) else {
                    continue;
                };
                if model.starts_with('<') {
                    continue; // p.ej. "<synthetic>"
                }
                let agg = by_model
                    .entry(model.clone())
                    .or_insert_with(|| ModelTokens {
                        model,
                        ..Default::default()
                    });
                agg.requests += 1;
                agg.input += usage.input_tokens;
                agg.output += usage.output_tokens;
                agg.cache_read += usage.cache_read_input_tokens;
                agg.cache_creation += usage.cache_creation_input_tokens;
            }
        }
    }

    let mut models: Vec<_> = by_model.into_values().collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.total()));
    models
}

/// Tokens por modelo agrupados por día local, de todos los JSONL de Claude
/// (sin filtro de mtime ni índice incremental). Para el backfill del historial
/// la primera vez; es un escaneo completo one-shot.
pub fn claude_by_day() -> Vec<(String, Vec<ModelTokens>)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let projects = PathBuf::from(home).join(".claude/projects");
    let mut by_day: HashMap<String, HashMap<String, ModelTokens>> = HashMap::new();

    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return vec![];
    };
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(f) = std::fs::File::open(&path) else {
                continue;
            };
            for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                let Ok(entry) = serde_json::from_str::<Entry>(&line) else {
                    continue;
                };
                if entry.kind != "assistant" {
                    continue;
                }
                let Some(date) = entry
                    .timestamp
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Local).date_naive().to_string())
                else {
                    continue;
                };
                let Some(msg) = entry.message else { continue };
                let (Some(model), Some(usage)) = (msg.model, msg.usage) else {
                    continue;
                };
                if model.starts_with('<') {
                    continue;
                }
                let agg = by_day
                    .entry(date)
                    .or_default()
                    .entry(model.clone())
                    .or_insert_with(|| ModelTokens {
                        model,
                        ..Default::default()
                    });
                agg.requests += 1;
                agg.input += usage.input_tokens;
                agg.output += usage.output_tokens;
                agg.cache_read += usage.cache_read_input_tokens;
                agg.cache_creation += usage.cache_creation_input_tokens;
            }
        }
    }

    by_day
        .into_iter()
        .map(|(date, models)| {
            let mut rows: Vec<_> = models.into_values().collect();
            rows.sort_by_key(|m| std::cmp::Reverse(m.total()));
            (date, rows)
        })
        .collect()
}

/// Total de tokens de Claude por hora local de HOY (24 buckets). Para la
/// gráfica de gasto por horas; escaneo directo de los JSONL de hoy.
pub fn claude_today_hourly() -> [u64; 24] {
    let mut hours = [0u64; 24];
    let home = std::env::var("HOME").unwrap_or_default();
    let projects = PathBuf::from(home).join(".claude/projects");
    let today = chrono::Local::now().date_naive();
    let midnight = today
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return hours;
    };
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let mtime = file
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if mtime < midnight {
                continue;
            }
            let Ok(f) = std::fs::File::open(&path) else {
                continue;
            };
            for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                let Ok(entry) = serde_json::from_str::<Entry>(&line) else {
                    continue;
                };
                if entry.kind != "assistant" {
                    continue;
                }
                let Some(dt) = entry
                    .timestamp
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Local))
                else {
                    continue;
                };
                if dt.date_naive() != today {
                    continue;
                }
                let Some(msg) = entry.message else { continue };
                let (Some(model), Some(usage)) = (msg.model, msg.usage) else {
                    continue;
                };
                if model.starts_with('<') {
                    continue;
                }
                let hour = dt.hour() as usize;
                hours[hour] += usage.input_tokens
                    + usage.output_tokens
                    + usage.cache_read_input_tokens
                    + usage.cache_creation_input_tokens;
            }
        }
    }
    hours
}

pub fn fmt_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}
