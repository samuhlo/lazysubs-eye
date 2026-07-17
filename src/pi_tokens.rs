use crate::cache;
use chrono::{Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u8 = 1;
const FINGERPRINT_BYTES: usize = 4096;
#[cfg(test)]
thread_local! {
    static SUFFIX_BYTES_READ: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_suffix_bytes_read() {
    SUFFIX_BYTES_READ.with(|bytes| bytes.set(0));
}

#[cfg(test)]
fn suffix_bytes_read() -> usize {
    SUFFIX_BYTES_READ.with(Cell::get)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PiUsageTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost_input: f64,
    pub cost_output: f64,
    pub cost_cache_read: f64,
    pub cost_cache_write: f64,
    pub cost_total: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PiUsageRow {
    pub provider: String,
    pub model: String,
    pub totals: PiUsageTotals,
}

// [CACHE] The index belongs to one local calendar interpretation, not UTC.
// Offset is part of the key so a zone/DST change invalidates a same-date cache
// instead of silently assigning old entries to today's local buckets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DayKey {
    local_date: String,
    timezone_offset_seconds: i32,
}

impl DayKey {
    fn now() -> Self {
        let now = Local::now();
        Self {
            local_date: now.date_naive().to_string(),
            timezone_offset_seconds: now.offset().local_minus_utc(),
        }
    }
}

// [CACHE] Per-file checkpoint. `safe_offset` ends after a newline; the two
// fingerprints prove that both the header and bytes immediately before that
// cursor still name the same stream before suffix reuse is allowed.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct FileState {
    #[serde(default)]
    dev: Option<u64>,
    #[serde(default)]
    ino: Option<u64>,
    size: u64,
    modified_ms: u128,
    header_fingerprint: u64,
    cursor_fingerprint: u64,
    safe_offset: u64,
    entry_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EntryState {
    provider: String,
    model: String,
    contribution: PiUsageTotals,
    source_paths: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DailyPiIndexV1 {
    schema_version: u8,
    day_key: DayKey,
    files: BTreeMap<String, FileState>,
    seen_entries: BTreeMap<String, EntryState>,
}

impl DailyPiIndexV1 {
    fn empty(day_key: DayKey) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            day_key,
            files: BTreeMap::new(),
            seen_entries: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedEntry {
    id: String,
    provider: String,
    model: String,
    timestamp_ms: i64,
    totals: PiUsageTotals,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    role: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    timestamp: Option<i64>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input: u64,
    output: u64,
    #[serde(rename = "cacheRead")]
    cache_read: u64,
    #[serde(rename = "cacheWrite")]
    cache_write: u64,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    cost: Cost,
}

#[derive(Deserialize)]
struct Cost {
    input: f64,
    output: f64,
    #[serde(rename = "cacheRead")]
    cache_read: f64,
    #[serde(rename = "cacheWrite")]
    cache_write: f64,
    total: f64,
}

#[derive(Deserialize)]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    version: u8,
    id: String,
    timestamp: String,
}

// [API] Accept only complete assistant-usage messages from Pi session JSONL.
// FAIL CLOSED -> malformed JSON, missing identity, invalid cost, or ambiguous
// local timestamp contributes nothing; partial records must not inflate totals.
fn parse_pi_line(line: &str) -> Option<ParsedEntry> {
    let envelope: Envelope = serde_json::from_str(line).ok()?;
    let id = envelope.id?.trim().to_owned();
    let message = envelope.message?;
    let provider = message.provider?.trim().to_owned();
    let model = message.model?.trim().to_owned();
    let timestamp_ms = message.timestamp?;
    let usage = message.usage?;
    let cost = usage.cost;
    let totals = PiUsageTotals {
        input: usage.input,
        output: usage.output,
        cache_read: usage.cache_read,
        cache_write: usage.cache_write,
        total_tokens: usage.total_tokens,
        cost_input: cost.input,
        cost_output: cost.output,
        cost_cache_read: cost.cache_read,
        cost_cache_write: cost.cache_write,
        cost_total: cost.total,
    };
    (envelope.kind == "message"
        && message.role.as_deref() == Some("assistant")
        && !id.is_empty()
        && !provider.is_empty()
        && !model.is_empty()
        && costs_are_valid(&totals)
        && Local.timestamp_millis_opt(timestamp_ms).single().is_some())
    .then_some(ParsedEntry {
        id,
        provider,
        model,
        timestamp_ms,
        totals,
    })
}

fn costs_are_valid(totals: &PiUsageTotals) -> bool {
    [
        totals.cost_input,
        totals.cost_output,
        totals.cost_cache_read,
        totals.cost_cache_write,
        totals.cost_total,
    ]
    .into_iter()
    .all(|value| value.is_finite() && value >= 0.0)
}

fn is_countable_entry(entry: &ParsedEntry, day_key: &DayKey) -> bool {
    Local
        .timestamp_millis_opt(entry.timestamp_ms)
        .single()
        .map(|time| time.date_naive().to_string() == day_key.local_date)
        .unwrap_or(false)
}

fn valid_header(line: &[u8]) -> bool {
    let Ok(header) = serde_json::from_slice::<SessionHeader>(line) else {
        return false;
    };
    header.kind == "session"
        && header.version == 3
        && !header.id.trim().is_empty()
        && chrono::DateTime::parse_from_rfc3339(&header.timestamp).is_ok()
}

fn hash(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

// [DATA] Validate every integer and float addition before mutating `target`.
// INVARIANT -> a rejected overflow leaves the prior aggregate untouched.
fn merge_totals(target: &mut PiUsageTotals, contribution: &PiUsageTotals) -> bool {
    let Some(input) = target.input.checked_add(contribution.input) else {
        return false;
    };
    let Some(output) = target.output.checked_add(contribution.output) else {
        return false;
    };
    let Some(cache_read) = target.cache_read.checked_add(contribution.cache_read) else {
        return false;
    };
    let Some(cache_write) = target.cache_write.checked_add(contribution.cache_write) else {
        return false;
    };
    let Some(total_tokens) = target.total_tokens.checked_add(contribution.total_tokens) else {
        return false;
    };
    let costs = [
        target.cost_input + contribution.cost_input,
        target.cost_output + contribution.cost_output,
        target.cost_cache_read + contribution.cost_cache_read,
        target.cost_cache_write + contribution.cost_cache_write,
        target.cost_total + contribution.cost_total,
    ];
    if costs.iter().any(|cost| !cost.is_finite()) {
        return false;
    }
    target.input = input;
    target.output = output;
    target.cache_read = cache_read;
    target.cache_write = cache_write;
    target.total_tokens = total_tokens;
    target.cost_input = costs[0];
    target.cost_output = costs[1];
    target.cost_cache_read = costs[2];
    target.cost_cache_write = costs[3];
    target.cost_total = costs[4];
    true
}

fn group_totals(index: &DailyPiIndexV1) -> BTreeMap<(String, String), PiUsageTotals> {
    let mut groups = BTreeMap::new();
    for entry in index.seen_entries.values() {
        let total = groups
            .entry((entry.provider.clone(), entry.model.clone()))
            .or_insert_with(PiUsageTotals::default);
        let _ = merge_totals(total, &entry.contribution);
    }
    groups
}

// [CACHE] Entry IDs deduplicate mirrored session files. Track every source path
// so removing one file keeps the contribution while another still references it.
fn add_entry(index: &mut DailyPiIndexV1, path: &str, entry: ParsedEntry) {
    if let Some(existing) = index.seen_entries.get_mut(&entry.id) {
        existing.source_paths.insert(path.to_owned());
        return;
    }
    let mut candidate = group_totals(index)
        .remove(&(entry.provider.clone(), entry.model.clone()))
        .unwrap_or_default();
    if !merge_totals(&mut candidate, &entry.totals) {
        return;
    }
    index.seen_entries.insert(
        entry.id,
        EntryState {
            provider: entry.provider,
            model: entry.model,
            contribution: entry.totals,
            source_paths: BTreeSet::from([path.to_owned()]),
        },
    );
}

fn remove_file_sources(index: &mut DailyPiIndexV1, path: &str) {
    let Some(state) = index.files.remove(path) else {
        return;
    };
    for id in state.entry_ids {
        let remove = index
            .seen_entries
            .get_mut(&id)
            .map(|entry| {
                entry.source_paths.remove(path);
                entry.source_paths.is_empty()
            })
            .unwrap_or(false);
        if remove {
            index.seen_entries.remove(&id);
        }
    }
}

// [CACHE] On Unix, device+inode survives a rename but changes on replacement.
// The cache key follows the object rather than its path, so moving a live
// session keeps its cursor while an atomic writer cannot inherit old entries.
#[cfg(unix)]
fn stable_file_identity(
    _path: &Path,
    metadata: &std::fs::Metadata,
) -> (String, Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;

    let dev = metadata.dev();
    let ino = metadata.ino();
    (format!("unix:{dev}:{ino}"), Some(dev), Some(ino))
}

#[cfg(not(unix))]
fn stable_file_identity(path: &Path, _: &std::fs::Metadata) -> (String, Option<u64>, Option<u64>) {
    (format!("path:{}", path.to_string_lossy()), None, None)
}

#[cfg(unix)]
fn index_is_compatible(index: &DailyPiIndexV1) -> bool {
    index.files.iter().all(|(key, state)| {
        let (Some(dev), Some(ino)) = (state.dev, state.ino) else {
            return false;
        };
        key == &format!("unix:{dev}:{ino}")
    })
}

#[cfg(not(unix))]
fn index_is_compatible(index: &DailyPiIndexV1) -> bool {
    index.files.keys().all(|key| key.starts_with("path:"))
}

fn read_window(path: &Path, offset: u64, length: usize) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length];
    let used = file.read(&mut bytes)?;
    bytes.truncate(used);
    Ok(bytes)
}

// [CACHE] Hash a bounded tail ending at the cursor. Metadata alone can lie on
// coarse filesystems; this window detects in-place edits before appended bytes
// are trusted. Offset zero deliberately fingerprints an empty window.
fn cursor_window(path: &Path, offset: u64) -> std::io::Result<Vec<u8>> {
    let length = offset.min(FINGERPRINT_BYTES as u64) as usize;
    read_window(path, offset.saturating_sub(length as u64), length)
}

fn read_suffix(path: &Path, offset: u64, snapshot_size: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    // The pre-open `metadata.len()` is this scan's cutoff.
    // ORDERING -> a concurrent append is deferred to the next refresh.
    file.take(snapshot_size.saturating_sub(offset))
        .read_to_end(&mut bytes)?;
    #[cfg(test)]
    SUFFIX_BYTES_READ.with(|read| read.set(read.get() + bytes.len()));
    Ok(bytes)
}

// [FLOW] Incremental JSONL scan: reuse the safe cursor when file identity and
// fingerprints match; otherwise remove this file's old contributions and rebuild.
// A trailing partial line stays beyond `safe_offset` until a later newline closes it.
fn process_file(index: &mut DailyPiIndexV1, path: &Path, day_key: &DayKey) {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return,
    };
    let size = metadata.len();
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_millis())
        .unwrap_or(0);
    let (key, dev, ino) = stable_file_identity(path, &metadata);
    let known = index.files.get(&key).cloned();
    // [CACHE] Steady-state fast path: inspect metadata without opening JSONL.
    // Size, mtime, and identity together detect append, replacement, and rename.
    if known.as_ref().is_some_and(|state| {
        state.size == size
            && state.modified_ms == modified_ms
            && state.dev == dev
            && state.ino == ino
    }) {
        return;
    }
    let header_window = match read_window(path, 0, FINGERPRINT_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    let Some(header_end) = header_window.iter().position(|byte| *byte == b'\n') else {
        return;
    };
    if !valid_header(&header_window[..header_end]) {
        return;
    }
    let header_fingerprint = hash(&header_window[..header_end]);
    let rebuild = known.as_ref().is_some_and(|state| {
        if size < state.safe_offset || state.header_fingerprint != header_fingerprint {
            return true;
        }
        cursor_window(path, state.safe_offset)
            .map(|window| hash(&window) != state.cursor_fingerprint)
            .unwrap_or(true)
    });
    if rebuild {
        remove_file_sources(index, &key);
    }
    let start = if rebuild {
        0
    } else {
        known.as_ref().map(|state| state.safe_offset).unwrap_or(0)
    };
    let suffix = match read_suffix(path, start, size) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    let mut entry_ids = if rebuild {
        BTreeSet::new()
    } else {
        known.map(|state| state.entry_ids).unwrap_or_default()
    };
    let complete_len = suffix
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    for line in suffix[..complete_len].split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(line) = std::str::from_utf8(line) else {
            continue;
        };
        let Some(entry) = parse_pi_line(line) else {
            continue;
        };
        if is_countable_entry(&entry, day_key) {
            entry_ids.insert(entry.id.clone());
            add_entry(index, &key, entry);
        }
    }
    let safe_offset = start + complete_len as u64;
    let cursor_fingerprint = cursor_window(path, safe_offset)
        .map(|window| hash(&window))
        .unwrap_or_default();
    index.files.insert(
        key,
        FileState {
            dev,
            ino,
            size,
            modified_ms,
            header_fingerprint,
            cursor_fingerprint,
            safe_offset,
            entry_ids,
        },
    );
}

fn walk(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, paths);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
            paths.push(path);
        }
    }
}

fn rows(index: &DailyPiIndexV1) -> Vec<PiUsageRow> {
    let mut rows: Vec<_> = group_totals(index)
        .into_iter()
        .map(|((provider, model), totals)| PiUsageRow {
            provider,
            model,
            totals,
        })
        .collect();
    rows.sort_by(|left, right| {
        right
            .totals
            .total_tokens
            .cmp(&left.totals.total_tokens)
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.model.cmp(&right.model))
    });
    rows
}

// [CACHE] Cache JSON is an optimization, never evidence. Bad JSON, a schema
// change, an incompatible identity format, or a different local day starts an
// empty index and reconstructs it from source JSONL.
fn load_index(path: &Path, day_key: &DayKey) -> DailyPiIndexV1 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<DailyPiIndexV1>(&raw).ok())
        .filter(|index| {
            index.schema_version == SCHEMA_VERSION
                && &index.day_key == day_key
                && index_is_compatible(index)
        })
        .unwrap_or_else(|| DailyPiIndexV1::empty(day_key.clone()))
}

// [CACHE] Reconcile discovered files with the persisted index before scanning.
// Missing files withdraw only their source references; shared entry IDs survive.
fn update_pi_index(root: &Path, index_path: &Path, day_key: DayKey) -> Vec<PiUsageRow> {
    let mut index = load_index(index_path, &day_key);
    let mut paths = Vec::new();
    walk(root, &mut paths);
    let present: BTreeSet<String> = paths
        .iter()
        .filter_map(|path| {
            std::fs::metadata(path)
                .ok()
                .map(|metadata| stable_file_identity(path, &metadata).0)
        })
        .collect();
    let stale: Vec<String> = index
        .files
        .keys()
        .filter(|path| !present.contains(*path))
        .cloned()
        .collect();
    for path in stale {
        remove_file_sources(&mut index, &path);
    }
    for path in paths {
        process_file(&mut index, &path, &day_key);
    }
    let snapshot = rows(&index);
    if let Ok(raw) = serde_json::to_vec(&index) {
        let _ = cache::atomic_save(index_path, &raw);
    }
    snapshot
}

// [FLOW] Today refresh: discover session JSONL, reconcile vanished/replaced
// files, consume only safe suffixes, aggregate unique IDs, then atomically
// persist the checkpoint for the next refresh. Discovery/read failures degrade
// to an empty or previously reconstructible snapshot rather than blocking UI.
pub fn scan_pi_today() -> Vec<PiUsageRow> {
    let Some(root) = pi_sessions_root() else {
        return vec![];
    };
    update_pi_index(&root, &cache::pi_daily_index_file(), DayKey::now())
}

// [DATA] Backfill uses trusted parser output and aggregates once per entry ID.
// Unlike the incremental path, this one-shot reconstruction has no persisted
// accumulator; source counters are added directly for each historical day.
fn add_pi_totals(acc: &mut PiUsageTotals, add: &PiUsageTotals) {
    acc.input += add.input;
    acc.output += add.output;
    acc.cache_read += add.cache_read;
    acc.cache_write += add.cache_write;
    acc.total_tokens += add.total_tokens;
    acc.cost_input += add.cost_input;
    acc.cost_output += add.cost_output;
    acc.cost_cache_read += add.cost_cache_read;
    acc.cost_cache_write += add.cost_cache_write;
    acc.cost_total += add.cost_total;
}

fn pi_sessions_root() -> Option<PathBuf> {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|root| root.join("sessions"))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".pi/agent/sessions"))
        })
}

/// [DATA] Pi usage grouped by `(provider, model)` and local day by scanning all
/// session JSONL without the incremental index. One-shot backfill deduplicates
/// entry IDs across files.
pub fn scan_pi_all_days() -> Vec<(String, Vec<PiUsageRow>)> {
    let Some(root) = pi_sessions_root() else {
        return vec![];
    };
    let mut paths = Vec::new();
    walk(&root, &mut paths);

    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    let mut by_day: BTreeMap<String, BTreeMap<(String, String), PiUsageTotals>> = BTreeMap::new();
    for path in paths {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Some(entry) = parse_pi_line(&line) else {
                continue;
            };
            if !seen_ids.insert(entry.id.clone()) {
                continue;
            }
            let Some(date) = Local
                .timestamp_millis_opt(entry.timestamp_ms)
                .single()
                .map(|time| time.date_naive().to_string())
            else {
                continue;
            };
            let acc = by_day
                .entry(date)
                .or_default()
                .entry((entry.provider, entry.model))
                .or_default();
            add_pi_totals(acc, &entry.totals);
        }
    }

    by_day
        .into_iter()
        .map(|(date, groups)| {
            let mut rows: Vec<_> = groups
                .into_iter()
                .map(|((provider, model), totals)| PiUsageRow {
                    provider,
                    model,
                    totals,
                })
                .collect();
            rows.sort_by(|left, right| {
                right
                    .totals
                    .total_tokens
                    .cmp(&left.totals.total_tokens)
                    .then_with(|| left.provider.cmp(&right.provider))
                    .then_with(|| left.model.cmp(&right.model))
            });
            (date, rows)
        })
        .collect()
}

/// [FLOW] Pi token totals for TODAY's 24 local-hour buckets. Direct scan,
/// deduplicating each entry ID.
///
/// File mtime is only a cheap candidate filter; each accepted timestamp decides
/// the local day and `0..23` bucket. Missing roots, unreadable files, and parse
/// failures leave their buckets at the zero sentinel.
pub fn scan_pi_today_hourly() -> [u64; 24] {
    let mut hours = [0u64; 24];
    let Some(root) = pi_sessions_root() else {
        return hours;
    };
    let today = Local::now().date_naive();
    let midnight = today
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(Local).single())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for path in paths {
        // Only inspect files touched today; each entry timestamp remains the precise filter.
        // FRICTION CUT -> avoid reparsing the full session history.
        let modified_today = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64 >= midnight)
            .unwrap_or(false);
        if !modified_today {
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Some(entry) = parse_pi_line(&line) else {
                continue;
            };
            if !seen.insert(entry.id.clone()) {
                continue;
            }
            let Some(dt) = Local.timestamp_millis_opt(entry.timestamp_ms).single() else {
                continue;
            };
            if dt.date_naive() != today {
                continue;
            }
            hours[dt.hour() as usize] += entry.totals.total_tokens;
        }
    }
    hours
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        r#"{"type":"session","version":3,"id":"session-1","timestamp":"2026-07-13T00:00:00Z"}"#;
    const ASSISTANT: &str = r#"{"type":"message","id":"entry-1","message":{"role":"assistant","provider":"anthropic","model":"claude","timestamp":1783900800000,"usage":{"input":10,"output":20,"cacheRead":3,"cacheWrite":4,"totalTokens":37,"cost":{"input":0.1,"output":0.2,"cacheRead":0.03,"cacheWrite":0.04,"total":0.37}}}}"#;

    #[test]
    fn parse_pi_line_accepts_only_complete_assistant_usage() {
        assert!(parse_pi_line(HEADER).is_none());
        assert!(parse_pi_line(ASSISTANT).is_some());
        assert!(
            parse_pi_line(r#"{"type":"message","id":"u","message":{"role":"user"}}"#).is_none()
        );
        assert!(parse_pi_line("not json").is_none());
    }

    #[test]
    fn parser_rejects_missing_or_invalid_numeric_metadata() {
        assert!(parse_pi_line(&ASSISTANT.replace("\"totalTokens\":37,", "")).is_none());
        assert!(parse_pi_line(&ASSISTANT.replace("\"input\":10", "\"input\":-1")).is_none());
        assert!(parse_pi_line(&ASSISTANT.replace("\"total\":0.37", "\"total\":-0.37")).is_none());
    }

    fn entry(id: &str) -> String {
        ASSISTANT.replace("entry-1", id).replace(
            "1783900800000",
            &Local::now().timestamp_millis().to_string(),
        )
    }

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("lazysubs-eye-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn merge_rejects_overflow_without_mutating_totals() {
        let mut total = PiUsageTotals {
            input: u64::MAX,
            ..Default::default()
        };
        assert!(!merge_totals(
            &mut total,
            &PiUsageTotals {
                input: 1,
                ..Default::default()
            }
        ));
        assert_eq!(total.input, u64::MAX);
    }

    #[test]
    fn steady_state_suffix_only_reads_zero_then_exact_append_bytes() {
        let root = test_dir("suffix");
        let file = root.join("run-7/session.jsonl");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry("one"))).unwrap();
        let index = root.join("index.json");
        let day = DayKey::now();
        let first = update_pi_index(&root, &index, day.clone());
        assert_eq!(first.len(), 1);
        reset_suffix_bytes_read();
        let second = update_pi_index(&root, &index, day.clone());
        assert_eq!(second.len(), 1);
        assert_eq!(suffix_bytes_read(), 0);
        let appended = format!("{}\n", entry("two"));
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(appended.as_bytes())
            .unwrap();
        reset_suffix_bytes_read();
        let third = update_pi_index(&root, &index, day);
        assert_eq!(third[0].totals.total_tokens, 74);
        assert_eq!(suffix_bytes_read(), appended.len());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn serialized_index_excludes_message_content_and_cwd() {
        let root = test_dir("privacy");
        let file = root.join("session.jsonl");
        let private_entry = entry("private").replace(
            "\"provider\":\"anthropic\"",
            "\"provider\":\"anthropic\",\"content\":\"SECRET-PROMPT\",\"cwd\":\"/private/work\"",
        );
        std::fs::write(&file, format!("{HEADER}\n{private_entry}\n")).unwrap();
        let index = root.join("index.json");
        let snapshot = update_pi_index(&root, &index, DayKey::now());
        let raw = std::fs::read_to_string(&index).unwrap();
        assert_eq!(snapshot.len(), 1);
        assert!(!raw.contains("SECRET-PROMPT"));
        assert!(!raw.contains("/private/work"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_ids_and_partial_lines_do_not_double_count() {
        let root = test_dir("dedup");
        let first = root.join("a.jsonl");
        let second = root.join("nested/run-3/session.jsonl");
        std::fs::create_dir_all(second.parent().unwrap()).unwrap();
        let contents = format!("{HEADER}\n{}\n", entry("same"));
        std::fs::write(&first, &contents).unwrap();
        std::fs::write(&second, contents).unwrap();
        let index = root.join("index.json");
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        let _ = std::fs::remove_file(&first);
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        let _ = std::fs::remove_file(&second);
        assert!(update_pi_index(&root, &index, DayKey::now()).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_or_unavailable_root_returns_an_empty_snapshot() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-eye-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(update_pi_index(&root, &root.join("index.json"), DayKey::now()).is_empty());
    }

    #[test]
    fn truncated_file_rebuilds_without_its_previous_contribution() {
        let root = test_dir("truncated");
        let file = root.join("session.jsonl");
        let index = root.join("index.json");
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry(&"old".repeat(64)))).unwrap();
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry("new"))).unwrap();
        let snapshot = update_pi_index(&root, &index, DayKey::now());
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].totals.total_tokens, 37);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_or_incompatible_index_bootstraps_safely() {
        let root = test_dir("corrupt-index");
        let file = root.join("session.jsonl");
        let index = root.join("index.json");
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry("one"))).unwrap();
        std::fs::write(&index, "not json").unwrap();
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
        raw["schema_version"] = serde_json::json!(99);
        std::fs::write(&index, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_day_rollover_discards_previous_daily_state() {
        let root = test_dir("day-rollover");
        let file = root.join("session.jsonl");
        let index = root.join("index.json");
        let today = DayKey::now();
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry("one"))).unwrap();
        assert_eq!(
            update_pi_index(&root, &index, today.clone())[0]
                .totals
                .total_tokens,
            37
        );
        let tomorrow = DayKey {
            local_date: "1900-01-01".into(),
            timezone_offset_seconds: today.timezone_offset_seconds.saturating_add(1),
        };
        assert!(update_pi_index(&root, &index, tomorrow).is_empty());
        assert_eq!(
            update_pi_index(&root, &index, today)[0].totals.total_tokens,
            37
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn groups_the_same_model_separately_by_provider_and_counts_error_stops() {
        let root = test_dir("providers-and-errors");
        let file = root.join("session.jsonl");
        let index = root.join("index.json");
        let aborted = entry("aborted").replace(
            "\"provider\":\"anthropic\"",
            "\"provider\":\"openai\",\"stopReason\":\"aborted\"",
        );
        let errored = entry("errored").replace(
            "\"model\":\"claude\"",
            "\"model\":\"claude\",\"stopReason\":\"error\"",
        );
        std::fs::write(&file, format!("{HEADER}\n{aborted}\n{errored}\n")).unwrap();
        let snapshot = update_pi_index(&root, &index, DayKey::now());
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot
                .iter()
                .map(|row| row.totals.total_tokens)
                .sum::<u64>(),
            74
        );
        assert!(snapshot
            .iter()
            .any(|row| row.provider == "anthropic" && row.model == "claude"));
        assert!(snapshot
            .iter()
            .any(|row| row.provider == "openai" && row.model == "claude"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn closed_malformed_json_advances_the_cursor_and_partial_line_retries_after_completion() {
        let root = test_dir("cursor-recovery");
        let file = root.join("nested/run-9/session.jsonl");
        let index = root.join("index.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let first = format!("{HEADER}\n{{malformed}}\n{}", entry("partial"));
        std::fs::write(&file, &first).unwrap();
        assert!(update_pi_index(&root, &index, DayKey::now()).is_empty());
        let state = load_index(&index, &DayKey::now())
            .files
            .into_values()
            .next()
            .unwrap();
        assert_eq!(
            state.safe_offset,
            (format!("{HEADER}\n{{malformed}}\n")).len() as u64
        );

        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"\n")
            .unwrap();
        let snapshot = update_pi_index(&root, &index, DayKey::now());
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].totals.total_tokens, 37);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_path_keyed_index_rebuilds_instead_of_reusing_stale_entries() {
        let root = test_dir("legacy-index");
        let file = root.join("session.jsonl");
        let index = root.join("index.json");
        std::fs::write(&file, format!("{HEADER}\n{}\n", entry("real"))).unwrap();
        update_pi_index(&root, &index, DayKey::now());

        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
        let file_key = raw["files"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        raw["files"][&file_key]
            .as_object_mut()
            .unwrap()
            .remove("dev");
        raw["files"][&file_key]
            .as_object_mut()
            .unwrap()
            .remove("ino");
        raw["seen_entries"]["phantom"] = serde_json::json!({
            "provider": "anthropic",
            "model": "claude",
            "contribution": {"input": 1, "output": 1, "cache_read": 1, "cache_write": 1, "total_tokens": 999, "cost_input": 0.0, "cost_output": 0.0, "cost_cache_read": 0.0, "cost_cache_write": 0.0, "cost_total": 0.0},
            "source_paths": [file_key]
        });
        std::fs::write(&index, serde_json::to_vec(&raw).unwrap()).unwrap();

        let snapshot = update_pi_index(&root, &index, DayKey::now());
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].totals.total_tokens, 37);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn unix_inode_identity_preserves_a_renamed_cursor_and_rebuilds_a_replacement() {
        use std::os::unix::fs::MetadataExt;

        let root = test_dir("inode-identity");
        let original = root.join("session.jsonl");
        let renamed = root.join("renamed.jsonl");
        let index = root.join("index.json");
        std::fs::write(&original, format!("{HEADER}\n{}\n", entry("first"))).unwrap();
        update_pi_index(&root, &index, DayKey::now());
        let original_metadata = std::fs::metadata(&original).unwrap();
        let before = load_index(&index, &DayKey::now());
        let (identity, state) = before.files.iter().next().unwrap();
        assert_eq!(state.dev, Some(original_metadata.dev()));
        assert_eq!(state.ino, Some(original_metadata.ino()));
        assert!(identity.contains(&format!(
            "{}:{}",
            original_metadata.dev(),
            original_metadata.ino()
        )));

        std::fs::rename(&original, &renamed).unwrap();
        reset_suffix_bytes_read();
        assert_eq!(
            update_pi_index(&root, &index, DayKey::now())[0]
                .totals
                .total_tokens,
            37
        );
        assert_eq!(suffix_bytes_read(), 0);
        assert_eq!(load_index(&index, &DayKey::now()).files.len(), 1);

        let replacement = root.join("replacement.jsonl");
        std::fs::write(
            &replacement,
            format!("{HEADER}\n{}\n", entry("replacement")),
        )
        .unwrap();
        std::fs::rename(&replacement, &renamed).unwrap();
        let replacement_metadata = std::fs::metadata(&renamed).unwrap();
        assert_ne!(original_metadata.ino(), replacement_metadata.ino());
        let snapshot = update_pi_index(&root, &index, DayKey::now());
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].totals.total_tokens, 37);
        let state = load_index(&index, &DayKey::now())
            .files
            .into_values()
            .next()
            .unwrap();
        assert_eq!(state.dev, Some(replacement_metadata.dev()));
        assert_eq!(state.ino, Some(replacement_metadata.ino()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn suffix_respects_snapshot_cutoff_before_concurrent_append() {
        let root = test_dir("snapshot-cutoff");
        let file = root.join("session.jsonl");
        std::fs::write(&file, b"first\n").unwrap();
        let cutoff = std::fs::metadata(&file).unwrap().len();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"second\n")
            .unwrap();
        assert_eq!(read_suffix(&file, 0, cutoff).unwrap(), b"first\n");
        let _ = std::fs::remove_dir_all(root);
    }
}
