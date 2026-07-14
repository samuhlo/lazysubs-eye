use crate::cache;
use chrono::{DateTime, Local, TimeZone};
use rusqlite::{types::Value, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

const INDEX_FORMAT: &str = "lazysubs-eye-opencode-daily";
const INDEX_VERSION: u8 = 1;
const RECONCILE_INTERVAL_MS: i64 = 24 * 60 * 60 * 1_000;

const SQL_CURSOR_PROBE: &str = "
SELECT COALESCE(MAX(rowid), 0),
       (SELECT id FROM part WHERE rowid = ?1)
FROM part";

const SQL_SUFFIX: &str = "
WITH projected AS (
  SELECT p.rowid AS part_rowid, p.id AS part_id,
         CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.providerID') END AS provider_id,
         CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.modelID') END AS model_id,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.input') END AS input_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.output') END AS output_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.reasoning') END AS reasoning_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.cache.read') END AS cache_read_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.cache.write') END AS cache_write_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.total') END AS total_tokens,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.cost') END AS cost,
         CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.type') END AS part_type,
         CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.role') END AS message_role
  FROM part AS p JOIN message AS m ON m.id = p.message_id
  WHERE p.rowid > ?1 AND p.rowid <= ?2
    AND p.time_created >= ?3 AND p.time_created < ?4
)
SELECT part_rowid, part_id, provider_id, model_id, input_tokens, output_tokens,
       reasoning_tokens, cache_read_tokens, cache_write_tokens, total_tokens, cost
FROM projected WHERE part_type = 'step-finish' AND message_role = 'assistant'";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvSnapshot {
    pub opencode_db: Option<String>,
    pub xdg_data_home: Option<String>,
    pub home: Option<String>,
}

impl EnvSnapshot {
    fn current() -> Self {
        Self {
            opencode_db: std::env::var("OPENCODE_DB").ok(),
            xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
            home: std::env::var("HOME").ok(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbResolution {
    File(PathBuf),
    EphemeralDatabase,
    MissingHome,
}

pub fn resolve_opencode_db(env: &EnvSnapshot) -> DbResolution {
    let data_home = env
        .xdg_data_home
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env.home
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        });

    match env.opencode_db.as_deref().filter(|value| !value.is_empty()) {
        Some(":memory:") => DbResolution::EphemeralDatabase,
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                DbResolution::File(path)
            } else {
                data_home
                    .map(|base| DbResolution::File(base.join("opencode").join(path)))
                    .unwrap_or(DbResolution::MissingHome)
            }
        }
        None => data_home
            .map(|base| DbResolution::File(base.join("opencode/opencode.db")))
            .unwrap_or(DbResolution::MissingHome),
    }
}

fn sqlite_read_only_uri(path: &Path) -> String {
    let encoded: String = path
        .as_os_str()
        .to_string_lossy()
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect();
    format!("file:{encoded}?mode=ro")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenCodeUnavailableReason {
    Missing,
    PermissionDenied,
    Busy,
    EphemeralDatabase,
    SchemaIncompatible,
    InvalidUsage,
    CacheWriteFailed,
    ReadFailed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpenCodePanelState {
    Loading,
    Ready(Vec<OpenCodeUsageRow>),
    Empty,
    Unavailable(OpenCodeUnavailableReason),
    Stale {
        rows: Vec<OpenCodeUsageRow>,
        reason: OpenCodeUnavailableReason,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeUsageRow {
    pub provider: String,
    pub model: String,
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub total: Option<u64>,
    pub cost: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectedRow {
    part_id: String,
    provider: String,
    model: String,
    input: Option<u64>,
    output: Option<u64>,
    reasoning: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    total: Option<u64>,
    cost: Option<f64>,
}

impl ProjectedRow {
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn synthetic(
        part_id: &str,
        provider: &str,
        model: &str,
        input: Option<u64>,
        output: Option<u64>,
        reasoning: Option<u64>,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
        total: Option<u64>,
        cost: Option<f64>,
    ) -> Self {
        Self {
            part_id: part_id.into(),
            provider: provider.into(),
            model: model.into(),
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            total,
            cost,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenCodeError {
    Missing,
    PermissionDenied,
    Busy,
    SchemaIncompatible,
    InvalidUsage,
    CacheWriteFailed,
    ReadFailed,
}

impl From<OpenCodeError> for OpenCodeUnavailableReason {
    fn from(error: OpenCodeError) -> Self {
        match error {
            OpenCodeError::Missing => Self::Missing,
            OpenCodeError::PermissionDenied => Self::PermissionDenied,
            OpenCodeError::Busy => Self::Busy,
            OpenCodeError::SchemaIncompatible => Self::SchemaIncompatible,
            OpenCodeError::InvalidUsage => Self::InvalidUsage,
            OpenCodeError::CacheWriteFailed => Self::CacheWriteFailed,
            OpenCodeError::ReadFailed => Self::ReadFailed,
        }
    }
}

fn open_read_only(path: &Path) -> Result<Connection, OpenCodeError> {
    if !path.is_file() {
        return Err(OpenCodeError::Missing);
    }
    let connection = Connection::open_with_flags(
        sqlite_read_only_uri(path),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(map_sqlite_error)?;
    connection
        .busy_timeout(Duration::from_millis(100))
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "query_only", "ON")
        .map_err(map_sqlite_error)?;
    Ok(connection)
}

fn map_sqlite_error(error: rusqlite::Error) -> OpenCodeError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if matches!(
                code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            OpenCodeError::Busy
        }
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::PermissionDenied =>
        {
            OpenCodeError::PermissionDenied
        }
        _ => OpenCodeError::ReadFailed,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DayWindow {
    local_date: String,
    start_ms: i64,
    next_start_ms: i64,
    start_offset_s: i32,
    next_offset_s: i32,
}

impl DayWindow {
    fn at(now: DateTime<Local>) -> Self {
        let date = now.date_naive();
        let start = Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
            .earliest()
            .expect("local midnight exists");
        let next_date = date.succ_opt().expect("date can advance");
        let next = Local
            .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
            .earliest()
            .expect("local midnight exists");
        Self {
            local_date: date.to_string(),
            start_ms: start.timestamp_millis(),
            next_start_ms: next.timestamp_millis(),
            start_offset_s: start.offset().local_minus_utc(),
            next_offset_s: next.offset().local_minus_utc(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DbIdentity {
    platform_file_id: Option<String>,
    path_fingerprint: u64,
    schema_version: i64,
    user_version: i64,
    schema_fingerprint: u64,
    page_size: i64,
    watermark_part_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCodeIndexV1 {
    format: String,
    version: u8,
    identity: DbIdentity,
    day: DayWindow,
    watermark_rowid: i64,
    seen_part_ids: BTreeSet<String>,
    totals: BTreeMap<String, OpenCodeUsageRow>,
    last_full_rebuild_ms: i64,
}

fn validate_schema(connection: &Connection) -> Result<(), OpenCodeError> {
    for (table, required) in [
        ("message", &["id", "data"][..]),
        ("part", &["id", "message_id", "time_created", "data"][..]),
    ] {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(map_sqlite_error)?;
        let columns: BTreeMap<String, i64> = statement
            .query_map([], |row| Ok((row.get(1)?, row.get(5)?)))
            .map_err(map_sqlite_error)?
            .filter_map(Result::ok)
            .collect();
        if required.iter().any(|column| !columns.contains_key(*column))
            || columns.get("id") != Some(&1)
        {
            return Err(OpenCodeError::SchemaIncompatible);
        }
    }
    connection
        .query_row("SELECT json_valid('{}')", [], |_| Ok(()))
        .map_err(|_| OpenCodeError::SchemaIncompatible)
}

fn db_identity(
    connection: &Connection,
    path: &Path,
    watermark: i64,
) -> Result<DbIdentity, OpenCodeError> {
    let schema_version = connection
        .query_row("PRAGMA schema_version", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let user_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let watermark_part_id = if watermark == 0 {
        None
    } else {
        connection
            .query_row("SELECT id FROM part WHERE rowid = ?1", [watermark], |row| {
                row.get(0)
            })
            .ok()
    };
    let page_size = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let schema_fingerprint = schema_fingerprint(connection)?;
    Ok(DbIdentity {
        platform_file_id: platform_file_id(path),
        path_fingerprint: fnv1a(
            &path
                .canonicalize()
                .unwrap_or_else(|_| path.to_path_buf())
                .to_string_lossy(),
        ),
        schema_version,
        user_version,
        schema_fingerprint,
        page_size,
        watermark_part_id,
    })
}

fn schema_fingerprint(connection: &Connection) -> Result<u64, OpenCodeError> {
    let mut columns = Vec::new();
    for table in ["message", "part"] {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        for row in rows {
            let (name, kind, primary_key) = row.map_err(map_sqlite_error)?;
            columns.push(format!("{table}:{name}:{kind}:{primary_key}"));
        }
    }
    columns.sort();
    Ok(fnv1a(&columns.join("|")))
}

#[cfg(unix)]
fn platform_file_id(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).ok()?;
    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn platform_file_id(_path: &Path) -> Option<String> {
    None
}

fn fnv1a(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn cursor_probe(
    connection: &Connection,
    watermark: i64,
) -> Result<(i64, Option<String>), OpenCodeError> {
    connection
        .query_row(SQL_CURSOR_PROBE, [watermark], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(map_sqlite_error)
}

fn value_token(value: Value) -> Result<Option<u64>, OpenCodeError> {
    match value {
        Value::Null => Ok(None),
        Value::Integer(value) if value >= 0 => Ok(Some(value as u64)),
        _ => Err(OpenCodeError::InvalidUsage),
    }
}

fn value_cost(value: Value) -> Result<Option<f64>, OpenCodeError> {
    match value {
        Value::Null => Ok(None),
        Value::Integer(value) if value >= 0 => Ok(Some(value as f64)),
        Value::Real(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
        _ => Err(OpenCodeError::InvalidUsage),
    }
}

fn project_step_finish_rows(
    connection: &Connection,
    after_rowid: i64,
    snapshot_max: i64,
    day: &DayWindow,
) -> Result<Vec<ProjectedRow>, OpenCodeError> {
    let mut statement = connection.prepare(SQL_SUFFIX).map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(
            [after_rowid, snapshot_max, day.start_ms, day.next_start_ms],
            |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Value>(4)?,
                    row.get::<_, Value>(5)?,
                    row.get::<_, Value>(6)?,
                    row.get::<_, Value>(7)?,
                    row.get::<_, Value>(8)?,
                    row.get::<_, Value>(9)?,
                    row.get::<_, Value>(10)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?;
    rows.map(|row| {
        let (
            part_id,
            provider,
            model,
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            total,
            cost,
        ) = row.map_err(map_sqlite_error)?;
        let provider = provider
            .filter(|value| !value.is_empty())
            .ok_or(OpenCodeError::InvalidUsage)?;
        let model = model
            .filter(|value| !value.is_empty())
            .ok_or(OpenCodeError::InvalidUsage)?;
        Ok(ProjectedRow {
            part_id,
            provider,
            model,
            input: value_token(input)?,
            output: value_token(output)?,
            reasoning: value_token(reasoning)?,
            cache_read: value_token(cache_read)?,
            cache_write: value_token(cache_write)?,
            total: value_token(total)?,
            cost: value_cost(cost)?,
        })
    })
    .collect()
}

fn effective_total(row: &ProjectedRow) -> Result<Option<u64>, OpenCodeError> {
    if row.total.is_some() {
        return Ok(row.total);
    }
    let values = [
        row.input,
        row.output,
        row.reasoning,
        row.cache_read,
        row.cache_write,
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    values
        .into_iter()
        .flatten()
        .try_fold(0u64, |sum, value| {
            sum.checked_add(value).ok_or(OpenCodeError::InvalidUsage)
        })
        .map(Some)
}

#[derive(Default)]
struct GroupTotals {
    row: Option<OpenCodeUsageRow>,
}

impl GroupTotals {
    fn add(&mut self, item: &ProjectedRow) -> Result<(), OpenCodeError> {
        let total = effective_total(item)?;
        let next = OpenCodeUsageRow {
            provider: item.provider.clone(),
            model: item.model.clone(),
            input: item.input,
            output: item.output,
            reasoning: item.reasoning,
            cache_read: item.cache_read,
            cache_write: item.cache_write,
            total,
            cost: item.cost,
        };
        match &mut self.row {
            None => self.row = Some(next),
            Some(current) => {
                current.input = sum_optional(current.input, next.input)?;
                current.output = sum_optional(current.output, next.output)?;
                current.reasoning = sum_optional(current.reasoning, next.reasoning)?;
                current.cache_read = sum_optional(current.cache_read, next.cache_read)?;
                current.cache_write = sum_optional(current.cache_write, next.cache_write)?;
                current.total = sum_optional(current.total, next.total)?;
                current.cost = sum_cost(current.cost, next.cost)?;
            }
        }
        Ok(())
    }
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Result<Option<u64>, OpenCodeError> {
    match (left, right) {
        (Some(left), Some(right)) => left
            .checked_add(right)
            .map(Some)
            .ok_or(OpenCodeError::InvalidUsage),
        _ => Ok(None),
    }
}

fn sum_cost(left: Option<f64>, right: Option<f64>) -> Result<Option<f64>, OpenCodeError> {
    match (left, right) {
        (Some(left), Some(right)) => {
            let total = left + right;
            (total.is_finite() && total >= 0.0)
                .then_some(Some(total))
                .ok_or(OpenCodeError::InvalidUsage)
        }
        _ => Ok(None),
    }
}

fn aggregate_projected_rows(
    rows: Vec<ProjectedRow>,
) -> Result<Vec<OpenCodeUsageRow>, OpenCodeError> {
    let mut groups: BTreeMap<(String, String), GroupTotals> = BTreeMap::new();
    for row in rows {
        groups
            .entry((row.provider.clone(), row.model.clone()))
            .or_default()
            .add(&row)?;
    }
    let mut result: Vec<_> = groups.into_values().filter_map(|group| group.row).collect();
    result.sort_by(|left, right| {
        right
            .total
            .cmp(&left.total)
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.model.cmp(&right.model))
    });
    Ok(result)
}

fn total_key(row: &OpenCodeUsageRow) -> String {
    format!("{}\u{1f}{}", row.provider, row.model)
}

fn rows_to_totals(rows: Vec<OpenCodeUsageRow>) -> BTreeMap<String, OpenCodeUsageRow> {
    rows.into_iter().map(|row| (total_key(&row), row)).collect()
}

fn totals_to_rows(totals: &BTreeMap<String, OpenCodeUsageRow>) -> Vec<OpenCodeUsageRow> {
    let mut rows: Vec<_> = totals.values().cloned().collect();
    rows.sort_by(|left, right| {
        right
            .total
            .cmp(&left.total)
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.model.cmp(&right.model))
    });
    rows
}

fn apply_day_rollover(index: &mut OpenCodeIndexV1, new_window: DayWindow) {
    index.day = new_window;
    index.seen_part_ids.clear();
    index.totals.clear();
}

fn load_index(path: &Path) -> Option<OpenCodeIndexV1> {
    serde_json::from_slice(&std::fs::read(path).ok()?)
        .ok()
        .filter(|index: &OpenCodeIndexV1| {
            index.format == INDEX_FORMAT && index.version == INDEX_VERSION
        })
}

fn merge_rows(
    existing: Vec<OpenCodeUsageRow>,
    fresh: Vec<ProjectedRow>,
) -> Result<Vec<OpenCodeUsageRow>, OpenCodeError> {
    let mut all = Vec::new();
    for row in existing {
        all.push(ProjectedRow {
            part_id: String::new(),
            provider: row.provider,
            model: row.model,
            input: row.input,
            output: row.output,
            reasoning: row.reasoning,
            cache_read: row.cache_read,
            cache_write: row.cache_write,
            total: row.total,
            cost: row.cost,
        });
    }
    all.extend(fresh);
    aggregate_projected_rows(all)
}

fn collect_at(path: &Path, cache_path: &Path, now: DateTime<Local>) -> OpenCodePanelState {
    let mut connection = match open_read_only(path) {
        Ok(connection) => connection,
        Err(error) => return OpenCodePanelState::Unavailable(error.into()),
    };
    if let Err(error) = validate_schema(&connection) {
        return OpenCodePanelState::Unavailable(error.into());
    }
    let transaction = match connection.transaction() {
        Ok(transaction) => transaction,
        Err(error) => return OpenCodePanelState::Unavailable(map_sqlite_error(error).into()),
    };
    let day = DayWindow::at(now);
    let (max_rowid, _) = match cursor_probe(&transaction, 0) {
        Ok(probe) => probe,
        Err(error) => return OpenCodePanelState::Unavailable(error.into()),
    };
    let now_ms = now.timestamp_millis();
    let mut index = load_index(cache_path).filter(|index| {
        index.watermark_rowid <= max_rowid
            && db_identity(&transaction, path, index.watermark_rowid)
                .map(|identity| identity == index.identity)
                .unwrap_or(false)
    });
    let rolled_over = index.as_ref().is_some_and(|index| index.day != day);
    let rebuild = index.as_ref().is_none()
        || (!rolled_over
            && index
                .as_ref()
                .is_some_and(|index| now_ms - index.last_full_rebuild_ms >= RECONCILE_INTERVAL_MS));
    let (after, old_rows, seen, last_full_rebuild_ms) = if rebuild {
        (0, Vec::new(), BTreeSet::new(), now_ms)
    } else {
        let mut index = index.take().expect("valid index exists");
        if rolled_over {
            apply_day_rollover(&mut index, day.clone());
        }
        (
            index.watermark_rowid,
            totals_to_rows(&index.totals),
            index.seen_part_ids,
            index.last_full_rebuild_ms,
        )
    };
    let fresh = match project_step_finish_rows(&transaction, after, max_rowid, &day) {
        Ok(rows) => rows,
        Err(error) => return stale_or_unavailable(old_rows, error),
    };
    let fresh: Vec<_> = fresh
        .into_iter()
        .filter(|row| !seen.contains(&row.part_id))
        .collect();
    let rows = match merge_rows(old_rows.clone(), fresh.clone()) {
        Ok(rows) => rows,
        Err(error) => return stale_or_unavailable(old_rows, error),
    };
    let mut seen_part_ids = seen;
    seen_part_ids.extend(fresh.into_iter().map(|row| row.part_id));
    let identity = match db_identity(&transaction, path, max_rowid) {
        Ok(identity) => identity,
        Err(error) => return stale_or_unavailable(old_rows, error),
    };
    let index = OpenCodeIndexV1 {
        format: INDEX_FORMAT.into(),
        version: INDEX_VERSION,
        identity,
        day,
        watermark_rowid: max_rowid,
        seen_part_ids,
        totals: rows_to_totals(rows.clone()),
        last_full_rebuild_ms,
    };
    if serde_json::to_vec(&index)
        .ok()
        .and_then(|bytes| cache::atomic_save(cache_path, &bytes).ok())
        .is_none()
    {
        return stale_or_unavailable(old_rows, OpenCodeError::CacheWriteFailed);
    }
    if rows.is_empty() {
        OpenCodePanelState::Empty
    } else {
        OpenCodePanelState::Ready(rows)
    }
}

fn stale_or_unavailable(rows: Vec<OpenCodeUsageRow>, error: OpenCodeError) -> OpenCodePanelState {
    if rows.is_empty() {
        OpenCodePanelState::Unavailable(error.into())
    } else {
        OpenCodePanelState::Stale {
            rows,
            reason: error.into(),
        }
    }
}

pub fn scan_opencode_today() -> OpenCodePanelState {
    match resolve_opencode_db(&EnvSnapshot::current()) {
        DbResolution::File(path) => {
            collect_at(&path, &cache::opencode_daily_index_file(), Local::now())
        }
        DbResolution::EphemeralDatabase => {
            OpenCodePanelState::Unavailable(OpenCodeUnavailableReason::EphemeralDatabase)
        }
        DbResolution::MissingHome => {
            OpenCodePanelState::Unavailable(OpenCodeUnavailableReason::Missing)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct FixtureDb {
        root: PathBuf,
        path: PathBuf,
    }

    impl Drop for FixtureDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn fixture_db() -> FixtureDb {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "lazysubs-eye-opencode-fixture-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("opencode.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);\
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);\
             CREATE INDEX message_session_time_id ON message(session_id,time_created,id);\
             CREATE INDEX part_message_id ON part(message_id,id);\
             CREATE INDEX part_session_id ON part(session_id);",
        ).unwrap();
        FixtureDb { root, path }
    }

    #[test]
    fn aggregate_keeps_reasoning_separate_and_preserves_absence() {
        let rows = vec![
            ProjectedRow::synthetic(
                "part-1",
                "openai",
                "gpt",
                Some(2),
                Some(3),
                Some(5),
                Some(7),
                Some(11),
                Some(40),
                Some(1.5),
            ),
            ProjectedRow::synthetic(
                "part-2",
                "openai",
                "gpt",
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                None,
                None,
                None,
            ),
        ];
        let aggregate = aggregate_projected_rows(rows).unwrap();
        let row = &aggregate[0];
        assert_eq!(row.input, Some(2));
        assert_eq!(row.output, Some(3));
        assert_eq!(row.reasoning, Some(5));
        assert_eq!(row.cache_read, Some(7));
        assert_eq!(row.cache_write, None);
        assert_eq!(row.total, Some(40));
        assert_eq!(row.cost, None);
    }

    #[test]
    fn resolve_override_and_default_paths_without_reading_environment() {
        let absolute = EnvSnapshot {
            opencode_db: Some("/tmp/custom.db".into()),
            xdg_data_home: Some("/data".into()),
            home: Some("/home/test".into()),
        };
        assert_eq!(
            resolve_opencode_db(&absolute),
            DbResolution::File(PathBuf::from("/tmp/custom.db"))
        );
        assert_eq!(
            resolve_opencode_db(&EnvSnapshot {
                opencode_db: Some("channel.db".into()),
                ..absolute.clone()
            }),
            DbResolution::File(PathBuf::from("/data/opencode/channel.db"))
        );
        assert_eq!(
            resolve_opencode_db(&EnvSnapshot {
                opencode_db: Some(":memory:".into()),
                ..absolute.clone()
            }),
            DbResolution::EphemeralDatabase
        );
        assert_eq!(
            resolve_opencode_db(&EnvSnapshot {
                opencode_db: None,
                xdg_data_home: None,
                home: Some("/home/test".into())
            }),
            DbResolution::File(PathBuf::from(
                "/home/test/.local/share/opencode/opencode.db"
            ))
        );
    }

    #[test]
    fn day_rollover_keeps_cursor_but_discards_daily_totals_and_seen_ids() {
        let old_day = DayWindow {
            local_date: "2026-07-12".into(),
            start_ms: 1,
            next_start_ms: 2,
            start_offset_s: 0,
            next_offset_s: 0,
        };
        let new_day = DayWindow {
            local_date: "2026-07-13".into(),
            start_ms: 2,
            next_start_ms: 3,
            start_offset_s: 0,
            next_offset_s: 0,
        };
        let mut index = OpenCodeIndexV1 {
            format: INDEX_FORMAT.into(),
            version: INDEX_VERSION,
            identity: DbIdentity {
                platform_file_id: Some("1:2".into()),
                path_fingerprint: 42,
                schema_version: 1,
                user_version: 0,
                schema_fingerprint: 3,
                page_size: 4096,
                watermark_part_id: Some("part-1".into()),
            },
            day: old_day,
            watermark_rowid: 1,
            seen_part_ids: BTreeSet::from(["part-1".into()]),
            totals: BTreeMap::from([(
                "group".into(),
                OpenCodeUsageRow {
                    provider: "openai".into(),
                    model: "gpt-test".into(),
                    input: Some(1),
                    output: None,
                    reasoning: None,
                    cache_read: None,
                    cache_write: None,
                    total: Some(1),
                    cost: None,
                },
            )]),
            last_full_rebuild_ms: 1,
        };
        apply_day_rollover(&mut index, new_day.clone());
        assert_eq!(index.day, new_day);
        assert_eq!(index.watermark_rowid, 1);
        assert_eq!(index.identity.watermark_part_id.as_deref(), Some("part-1"));
        assert!(index.totals.is_empty());
        assert!(index.seen_part_ids.is_empty());
    }

    #[test]
    fn index_persists_grouped_totals_without_a_plaintext_path() {
        let index = OpenCodeIndexV1 {
            format: INDEX_FORMAT.into(),
            version: INDEX_VERSION,
            identity: DbIdentity {
                platform_file_id: Some("1:2".into()),
                path_fingerprint: 42,
                schema_version: 1,
                user_version: 0,
                schema_fingerprint: 3,
                page_size: 4096,
                watermark_part_id: Some("part-1".into()),
            },
            day: DayWindow::at(Local::now()),
            watermark_rowid: 1,
            seen_part_ids: BTreeSet::from(["part-1".into()]),
            totals: rows_to_totals(vec![OpenCodeUsageRow {
                provider: "openai".into(),
                model: "gpt-test".into(),
                input: Some(1),
                output: Some(2),
                reasoning: None,
                cache_read: None,
                cache_write: None,
                total: Some(3),
                cost: None,
            }]),
            last_full_rebuild_ms: 1,
        };
        let json = serde_json::to_string(&index).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&json).unwrap()["totals"].is_object());
        assert!(!json.contains("/private/opencode.db"));
        assert!(serde_json::from_str::<OpenCodeIndexV1>(&format!("{json} ")).is_ok());
        let with_prompt = json.replacen('{', r#"{\"prompt\":\"SECRET-PROMPT\","#, 1);
        assert!(serde_json::from_str::<OpenCodeIndexV1>(&with_prompt).is_err());
    }

    #[test]
    fn schema_validation_rejects_missing_primary_keys() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE message (id TEXT, data TEXT);\
             CREATE TABLE part (id TEXT, message_id TEXT, time_created INTEGER, data TEXT);",
            )
            .unwrap();
        assert_eq!(
            validate_schema(&connection),
            Err(OpenCodeError::SchemaIncompatible)
        );
    }

    #[test]
    fn fixture_provides_the_minimal_private_schema() {
        let fixture = fixture_db();
        let connection = Connection::open(&fixture.path).unwrap();
        let names: BTreeSet<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'index')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for name in [
            "message",
            "part",
            "message_session_time_id",
            "part_message_id",
            "part_session_id",
        ] {
            assert!(names.contains(name));
        }
    }

    #[test]
    fn read_only_connection_sees_a_committed_wal_row() {
        let fixture = fixture_db();
        let writer = Connection::open(&fixture.path).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        let now = Local::now();
        writer
            .execute(
                "INSERT INTO message VALUES ('wal-message','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"role":"assistant","providerID":"openai","modelID":"gpt-test"}"#
                ],
            )
            .unwrap();
        writer
            .execute(
                "INSERT INTO part VALUES ('wal-part','wal-message','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"type":"step-finish","tokens":{"total":1}}"#
                ],
            )
            .unwrap();
        let reader = open_read_only(&fixture.path).unwrap();
        let rows = project_step_finish_rows(&reader, 0, i64::MAX, &DayWindow::at(now)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].part_id, "wal-part");
    }

    #[test]
    fn projection_limits_rows_to_today_assistant_step_finishes() {
        let fixture = fixture_db();
        let now = Local::now();
        let day = DayWindow::at(now);
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute(
                "INSERT INTO message VALUES ('assistant','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"role":"assistant","providerID":"openai","modelID":"gpt-test"}"#
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO message VALUES ('user','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"role":"user","providerID":"openai","modelID":"gpt-test"}"#
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('today','assistant','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"type":"step-finish","tokens":{"input":2},"content":"SECRET-PROMPT"}"#
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('user-step','user','s',?1,?1,?2)",
                params![
                    now.timestamp_millis(),
                    r#"{"type":"step-finish","tokens":{"input":99}}"#
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('old','assistant','s',?1,?1,?2)",
                params![
                    day.start_ms - 1,
                    r#"{"type":"step-finish","tokens":{"input":99}}"#
                ],
            )
            .unwrap();
        let rows = project_step_finish_rows(&connection, 0, i64::MAX, &day).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].part_id, "today");
        let aggregate = aggregate_projected_rows(rows).unwrap();
        assert_eq!(aggregate[0].input, Some(2));
        assert!(!format!("{:?}", aggregate).contains("SECRET-PROMPT"));
    }

    #[test]
    fn suffix_query_uses_rowid_search_in_the_fixture() {
        let fixture = fixture_db();
        let connection = Connection::open(&fixture.path).unwrap();
        let plan: Vec<String> = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {SQL_SUFFIX}"))
            .unwrap()
            .query_map([1_i64, 100_i64, 0_i64, i64::MAX], |row| row.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let plan = plan.join(" | ");
        assert!(
            plan.contains("SEARCH p USING INTEGER PRIMARY KEY"),
            "{plan}"
        );
        assert!(
            plan.contains("SEARCH m USING INDEX sqlite_autoindex_message_1"),
            "{plan}"
        );
        assert!(!plan.contains("SCAN p"), "{plan}");
    }

    #[test]
    fn collector_reads_only_assistant_step_finishes_and_uses_the_cache_incrementally() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-eye-opencode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("fixture.db");
        let index = root.join("index.json");
        let connection = Connection::open(&db).unwrap();
        connection.execute_batch("CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT); CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT); CREATE INDEX message_session_time_id ON message(session_id,time_created,id); CREATE INDEX part_message_id ON part(message_id,id); CREATE INDEX part_session_id ON part(session_id);").unwrap();
        let now = Local::now();
        let timestamp = now.timestamp_millis();
        connection
            .execute(
                "INSERT INTO message VALUES ('m1','s',?1,?1,?2)",
                params![
                    timestamp,
                    r#"{"role":"assistant","providerID":"openai","modelID":"gpt"}"#
                ],
            )
            .unwrap();
        connection.execute("INSERT INTO part VALUES ('p1','m1','s',?1,?1,?2)", params![timestamp, r#"{"type":"step-finish","tokens":{"input":2,"output":3,"reasoning":5,"cache":{"read":7,"write":11},"total":40},"cost":1.5,"content":"SECRET-PROMPT"}"#]).unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('p2','m1','s',?1,?1,?2)",
                params![timestamp, r#"{"type":"text","tokens":{"input":999}}"#],
            )
            .unwrap();
        drop(connection);
        let state = collect_at(&db, &index, now);
        let OpenCodePanelState::Ready(rows) = state else {
            panic!("expected ready state")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total, Some(40));
        assert!(!format!("{:?}", rows).contains("SECRET-PROMPT"));
        assert!(
            matches!(collect_at(&db, &index, now), OpenCodePanelState::Ready(rows) if rows[0].total == Some(40))
        );
        let connection = Connection::open(&db).unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('p3','m1','s',?1,?1,?2)",
                params![
                    timestamp,
                    r#"{"type":"step-finish","tokens":{"input":1,"total":2},"cost":0.5}"#
                ],
            )
            .unwrap();
        drop(connection);
        assert!(
            matches!(collect_at(&db, &index, now), OpenCodePanelState::Ready(rows) if rows[0].total == Some(42))
        );
        let before: OpenCodeIndexV1 =
            serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
        assert!(matches!(
            collect_at(&db, &index, now + chrono::Duration::days(1)),
            OpenCodePanelState::Empty
        ));
        let after: OpenCodeIndexV1 =
            serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
        assert_eq!(after.watermark_rowid, before.watermark_rowid);
        assert_ne!(after.day, before.day);
        let _ = std::fs::remove_dir_all(root);
    }
}
