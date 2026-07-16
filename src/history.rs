//! Historial de gasto de tokens por día en SQLite
//! (`~/.local/state/lazysubs-eye/history.db`, respetando `XDG_STATE_HOME`).
//!
//! Cada escaneo de "hoy" hace un upsert (delete+insert) de las filas del día
//! en curso, así el historial sobrevive aunque los JSONL/DB de origen se poden.
//! La primera vez se pueblan los días pasados desde las fuentes (backfill).
//!
//! Las funciones de bajo nivel operan sobre `&Connection` para poder testearse
//! con una base en memoria; los puntos de entrada (`record_source`,
//! `ingest_today`, `maybe_backfill`, `period_rows`, `sparkline`) abren la base
//! real y **nunca rompen el flujo**: ante cualquier error se degradan a vacío.

use anyhow::{Context, Result};
use chrono::{Datelike, Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::PathBuf;

pub const SOURCE_CLAUDE: &str = "claude";
pub const SOURCE_PI: &str = "pi";
pub const SOURCE_OPENCODE: &str = "opencode";

/// Fuentes con historial, en el orden de los paneles de la TUI.
pub const SOURCES: [&str; 3] = [SOURCE_CLAUDE, SOURCE_PI, SOURCE_OPENCODE];

const BACKFILL_META_KEY: &str = "backfill_v1";
const BACKFILL_LAST_PREFIX: &str = "backfill_last_day_v1";
const BACKFILL_PROGRESS_KEY: &str = "backfill_progress_v1";
/// Nº de días que abarca el sparkline bajo cada panel.
pub const SPARKLINE_DAYS: usize = 14;

/// Progreso observable de un backfill. La fuente de verdad continúa en
/// `meta`; este valor solo permite que la TUI se mantenga responsiva.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackfillProgress {
    pub current_day: Option<String>,
    pub completed_days: usize,
    pub total_days: usize,
    pub failed_days: usize,
}

/// Retención de una fuente de historial. `KeepForever` evita que la limpieza
/// global borre accidentalmente datos de una fuente con una política distinta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetentionPolicy {
    KeepDays(i64),
    KeepForever,
}

/// Fila agregada de uso: una por (día, fuente, provider, modelo) en la base.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageRow {
    pub provider: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    pub total: u64,
    pub cost: f64,
}

/// Estado persistido de una ingesta para una fuente y un día concretos.
///
/// La clave de `meta` aporta la fuente y el día; los repetimos en el valor
/// para que un export de la tabla siga siendo autoexplicativo y verificable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IngestState {
    Ingested {
        day: String,
        record_count: u64,
        last_rowid: Option<i64>,
        ingested_at: i64,
    },
    Partial {
        day: String,
        record_count: u64,
        last_rowid: Option<i64>,
        ingested_at: i64,
        reason: String,
    },
    InProgress {
        day: String,
        started_at: i64,
    },
    Pending {
        day: String,
    },
    Skipped {
        day: String,
        reason: String,
    },
    Failed {
        day: String,
        attempted_at: i64,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub source: String,
    pub first_ingest: String,
    pub last_ingest: String,
    pub record_count: u64,
}

fn source_meta_key(source: &str) -> String {
    format!("source_v1:{source}")
}

fn backfill_last_key(source: &str) -> String {
    format!("{BACKFILL_LAST_PREFIX}:{source}")
}

/// Cutoff capturado antes de consultar una fuente. Para días históricos es el
/// final del día local; para hoy nunca queda en el futuro.
pub fn get_cutoff_timestamp(source: &str, day: NaiveDate) -> i64 {
    let _ = source; // seam por fuente: permite políticas distintas sin duplicar scanners.
    use chrono::TimeZone;
    let day_end = Local
        .from_local_datetime(&day.and_hms_opt(23, 59, 59).expect("hora válida"))
        .latest()
        .map(|datetime| datetime.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());
    day_end.min(chrono::Utc::now().timestamp())
}

fn ingest_meta_key(source: &str, day: &str) -> String {
    format!("ingest_state_v1:{source}:{day}")
}

fn fingerprint_meta_key(source: &str, day: &str) -> String {
    format!("ingest_fingerprint_v1:{source}:{day}")
}

/// Huella estable del agregado que se va a persistir. Es deliberadamente
/// independiente de rutas o mtimes de los providers: la misma instantánea
/// produce la misma huella y una fila cambiada fuerza reingesta.
pub fn compute_day_fingerprint(source: &str, day: &str, rows: &[UsageRow]) -> String {
    let mut hash = 0xcbf29ce484222325_u64; // FNV-1a, estable entre procesos.
    let mut feed = |text: &str| {
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    feed(source);
    feed(day);
    for row in rows {
        let mut encoded = String::new();
        let _ = write!(
            encoded,
            "\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            row.provider,
            row.model,
            row.input,
            row.output,
            row.cache_read,
            row.cache_write,
            row.reasoning,
            row.total,
            row.cost.to_bits()
        );
        feed(&encoded);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Lee el estado normalizado de una fuente y día. Un valor de meta corrupto
/// se trata como error, nunca como un día correctamente ingestado.
pub fn ingest_state(conn: &Connection, source: &str, day: &str) -> Result<Option<IngestState>> {
    get_meta(conn, &ingest_meta_key(source, day))?
        .map(|raw| serde_json::from_str(&raw).context("estado de ingesta corrupto"))
        .transpose()
}

/// Último estado diario persistido de una fuente. Las fechas ISO forman parte
/// de la clave, por lo que el orden lexicográfico coincide con el cronológico.
pub fn latest_ingest_state(conn: &Connection, source: &str) -> Result<Option<IngestState>> {
    let prefix = format!("ingest_state_v1:{source}:%");
    let raw = conn
        .query_row(
            "SELECT value FROM meta WHERE key LIKE ?1 ORDER BY key DESC LIMIT 1",
            [prefix],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).context("estado de ingesta corrupto"))
        .transpose()
}

fn set_ingest_state_tx(tx: &Transaction<'_>, source: &str, state: &IngestState) -> Result<()> {
    let day = match state {
        IngestState::Ingested { day, .. }
        | IngestState::Partial { day, .. }
        | IngestState::InProgress { day, .. }
        | IngestState::Pending { day }
        | IngestState::Skipped { day, .. }
        | IngestState::Failed { day, .. } => day,
    };
    let value = serde_json::to_string(state)?;
    tx.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![ingest_meta_key(source, day), value],
    )?;
    Ok(())
}

fn set_meta_tx(tx: &Transaction<'_>, key: &str, value: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Registra un fallo recuperable sin tocar los agregados que ya existían para
/// ese día. El motivo es deliberadamente genérico: los errores de SQLite no
/// deben acabar almacenados ni mostrados con rutas o detalles del sistema.
fn mark_ingest_failed(conn: &Connection, source: &str, day: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    set_ingest_state_tx(
        &tx,
        source,
        &IngestState::Failed {
            day: day.to_owned(),
            attempted_at: chrono::Utc::now().timestamp(),
            reason: "no se pudo persistir este día".into(),
        },
    )?;
    tx.commit()?;
    Ok(())
}

fn has_same_fingerprint(conn: &Connection, source: &str, day: &str, rows: &[UsageRow]) -> bool {
    get_meta(conn, &fingerprint_meta_key(source, day))
        .ok()
        .flatten()
        .is_some_and(|previous| previous == compute_day_fingerprint(source, day, rows))
}

/// Frontera transaccional de un día. Un fingerprint idéntico retorna Skipped
/// sin reescribir ni datos ni meta; un fallo conserva el agregado previo y
/// registra Failed en una transacción independiente.
pub fn ingest_single_day(
    conn: &Connection,
    day: &str,
    source: &str,
    rows: &[UsageRow],
) -> Result<IngestState> {
    if has_same_fingerprint(conn, source, day, rows) {
        return Ok(IngestState::Skipped {
            day: day.to_owned(),
            reason: "fingerprint sin cambios".into(),
        });
    }
    if let Err(error) = record_day(conn, day, source, rows) {
        mark_ingest_failed(conn, source, day)?;
        return Err(error);
    }
    ingest_state(conn, source, day)?.context("estado de ingesta ausente tras commit")
}

/// Conserva los registros recuperables y deja constancia explícita de que la
/// instantánea no fue completa. El detalle del parser no cruza esta frontera:
/// así `meta` nunca filtra rutas, SQL ni contenido potencialmente sensible.
#[allow(dead_code)] // Seam para collectors que reporten errores recuperables por día.
pub fn ingest_partial_day(
    conn: &Connection,
    day: &str,
    source: &str,
    rows: &[UsageRow],
) -> Result<IngestState> {
    record_day_with_state(
        conn,
        day,
        source,
        rows,
        Some("algunos registros de origen no eran válidos"),
    )?;
    ingest_state(conn, source, day)?.context("estado parcial ausente tras commit")
}

/// Periodo que agregan los paneles de tokens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Period {
    Today,
    Week,
    Month,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "hoy",
            Period::Week => "semana",
            Period::Month => "mes",
        }
    }

    /// Cicla hoy → semana → mes → hoy.
    pub fn next(self) -> Period {
        match self {
            Period::Today => Period::Week,
            Period::Week => Period::Month,
            Period::Month => Period::Today,
        }
    }

    pub fn parse(s: &str) -> Period {
        match s {
            "semana" => Period::Week,
            "mes" => Period::Month,
            _ => Period::Today,
        }
    }
}

/// Rango de fechas [inicio, fin] (inclusive) del periodo respecto a `today`,
/// en formato `YYYY-MM-DD`. Semana = lunes de la semana en curso; mes = día 1.
pub fn period_bounds(period: Period, today: NaiveDate) -> (String, String) {
    let start = match period {
        Period::Today => today,
        Period::Week => {
            today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)
        }
        Period::Month => today.with_day(1).unwrap_or(today),
    };
    (start.to_string(), today.to_string())
}

/// Vista de la gráfica de gasto: días de la semana en curso, del mes en curso,
/// o por horas de hoy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphView {
    Week,
    Month,
    Hours,
}

impl GraphView {
    pub fn label(self) -> &'static str {
        match self {
            GraphView::Week => "semana",
            GraphView::Month => "mes",
            GraphView::Hours => "hoy por horas",
        }
    }

    pub fn next(self) -> GraphView {
        match self {
            GraphView::Week => GraphView::Month,
            GraphView::Month => GraphView::Hours,
            GraphView::Hours => GraphView::Week,
        }
    }
}

/// Serie lista para pintar: valores + etiquetas del eje x (una por valor).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphData {
    pub values: Vec<u64>,
    pub labels: Vec<String>,
}

// --- capa de base de datos (pura, testable con Connection en memoria) --------

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS daily_usage (
            date        TEXT NOT NULL,
            source      TEXT NOT NULL,
            provider    TEXT NOT NULL,
            model       TEXT NOT NULL,
            input       INTEGER NOT NULL DEFAULT 0,
            output      INTEGER NOT NULL DEFAULT 0,
            cache_read  INTEGER NOT NULL DEFAULT 0,
            cache_write INTEGER NOT NULL DEFAULT 0,
            reasoning   INTEGER NOT NULL DEFAULT 0,
            total       INTEGER NOT NULL DEFAULT 0,
            cost        REAL    NOT NULL DEFAULT 0,
            PRIMARY KEY (date, source, provider, model)
         );
         CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
         );",
    )?;
    Ok(())
}

/// Reemplaza (delete + insert) todas las filas de `(date, source)`. El escaneo
/// es autoritativo para su día y fuente, así los modelos que desaparezcan no
/// dejan restos.
pub fn record_day(conn: &Connection, date: &str, source: &str, rows: &[UsageRow]) -> Result<()> {
    record_day_with_state(conn, date, source, rows, None)
}

fn record_day_with_state(
    conn: &Connection,
    date: &str,
    source: &str,
    rows: &[UsageRow],
    partial_reason: Option<&str>,
) -> Result<()> {
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").context("día de ingesta inválido")?;
    record_day_at(
        conn,
        date,
        source,
        rows,
        get_cutoff_timestamp(source, day),
        partial_reason,
    )
}

fn record_day_at(
    conn: &Connection,
    date: &str,
    source: &str,
    rows: &[UsageRow],
    cutoff: i64,
    partial_reason: Option<&str>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM daily_usage WHERE date = ?1 AND source = ?2",
        params![date, source],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO daily_usage
               (date, source, provider, model, input, output, cache_read,
                cache_write, reasoning, total, cost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for r in rows {
            stmt.execute(params![
                date,
                source,
                r.provider,
                r.model,
                r.input as i64,
                r.output as i64,
                r.cache_read as i64,
                r.cache_write as i64,
                r.reasoning as i64,
                r.total as i64,
                r.cost,
            ])?;
        }
    }
    let state = match partial_reason {
        Some(reason) => IngestState::Partial {
            day: date.to_owned(),
            record_count: rows.len() as u64,
            last_rowid: None,
            ingested_at: cutoff,
            reason: reason.to_owned(),
        },
        None => IngestState::Ingested {
            day: date.to_owned(),
            record_count: rows.len() as u64,
            last_rowid: None,
            ingested_at: cutoff,
        },
    };
    set_ingest_state_tx(&tx, source, &state)?;
    set_meta_tx(
        &tx,
        &fingerprint_meta_key(source, date),
        &compute_day_fingerprint(source, date, rows),
    )?;
    let provenance = tx.query_row(
        "SELECT MIN(date), MAX(date), COUNT(*) FROM daily_usage WHERE source = ?1",
        [source],
        |row| {
            Ok(SourceProvenance {
                source: source.to_owned(),
                first_ingest: row
                    .get::<_, Option<String>>(0)?
                    .unwrap_or_else(|| date.to_owned()),
                last_ingest: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| date.to_owned()),
                record_count: row.get::<_, i64>(2)?.max(0) as u64,
            })
        },
    )?;
    set_meta_tx(
        &tx,
        &source_meta_key(source),
        &serde_json::to_string(&provenance)?,
    )?;
    tx.commit()?;
    Ok(())
}

/// Uso agregado por (provider, modelo) de una fuente en el rango [start, end].
pub fn query_period(
    conn: &Connection,
    source: &str,
    start: &str,
    end: &str,
) -> Result<Vec<UsageRow>> {
    let mut stmt = conn.prepare(
        "SELECT provider, model,
                SUM(input), SUM(output), SUM(cache_read), SUM(cache_write),
                SUM(reasoning), SUM(total), SUM(cost)
         FROM daily_usage
         WHERE source = ?1 AND date >= ?2 AND date <= ?3
         GROUP BY provider, model
         ORDER BY SUM(total) DESC, provider, model",
    )?;
    let rows = stmt.query_map(params![source, start, end], |row| {
        let as_u64 = |v: i64| v.max(0) as u64;
        Ok(UsageRow {
            provider: row.get(0)?,
            model: row.get(1)?,
            input: as_u64(row.get(2)?),
            output: as_u64(row.get(3)?),
            cache_read: as_u64(row.get(4)?),
            cache_write: as_u64(row.get(5)?),
            reasoning: as_u64(row.get(6)?),
            total: as_u64(row.get(7)?),
            cost: row.get(8)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Itera SQLite en lotes acotados. El statement conserva su cursor y solo el
/// batch actual vive en memoria; el consumidor decide si agrega o persiste.
pub fn query_streaming<T, P, F, C>(
    conn: &Connection,
    sql: &str,
    params: P,
    batch_size: usize,
    mut mapper: F,
    mut consume: C,
) -> Result<()>
where
    P: rusqlite::Params,
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    C: FnMut(&[T]) -> Result<()>,
{
    if batch_size == 0 {
        anyhow::bail!("batch_size debe ser mayor que cero");
    }
    let mut statement = conn.prepare(sql)?;
    let mut rows = statement.query(params)?;
    let mut batch = Vec::with_capacity(batch_size);
    while let Some(row) = rows.next()? {
        batch.push(mapper(row)?);
        if batch.len() == batch_size {
            consume(&batch)?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        consume(&batch)?;
    }
    Ok(())
}

/// Total diario sumando **todas** las fuentes en el rango [start, end].
pub fn all_daily_totals(conn: &Connection, start: &str, end: &str) -> Result<Vec<(String, u64)>> {
    let mut totals = Vec::new();
    query_streaming(
        conn,
        "SELECT date, SUM(total) FROM daily_usage
         WHERE date >= ?1 AND date <= ?2
         GROUP BY date ORDER BY date",
        params![start, end],
        256,
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u64,
            ))
        },
        |batch| {
            totals.extend_from_slice(batch);
            Ok(())
        },
    )?;
    Ok(totals)
}

/// Total diario de una fuente en el rango [start, end], por fecha.
pub fn daily_totals(
    conn: &Connection,
    source: &str,
    start: &str,
    end: &str,
) -> Result<Vec<(String, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT date, SUM(total) FROM daily_usage
         WHERE source = ?1 AND date >= ?2 AND date <= ?3
         GROUP BY date ORDER BY date",
    )?;
    let rows = stmt.query_map(params![source, start, end], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?.max(0) as u64,
        ))
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Serie de totales diarios rellenada con ceros para los últimos `days` días
/// terminando en `today` (más antiguo primero), lista para un Sparkline.
pub fn series_last_n(
    conn: &Connection,
    source: &str,
    today: NaiveDate,
    days: usize,
) -> Result<Vec<u64>> {
    let days = days.max(1);
    let start = today - chrono::Duration::days(days as i64 - 1);
    let totals = daily_totals(conn, source, &start.to_string(), &today.to_string())?;
    let lookup: std::collections::HashMap<String, u64> = totals.into_iter().collect();
    Ok((0..days)
        .map(|i| {
            let date = (start + chrono::Duration::days(i as i64)).to_string();
            lookup.get(&date).copied().unwrap_or(0)
        })
        .collect())
}

/// Borra las filas más viejas que `keep_days` respecto a `today`. `keep_days`
/// <= 0 = sin límite (no borra nada).
pub fn prune(conn: &Connection, keep_days: i64, today: NaiveDate) -> Result<()> {
    let policy = if keep_days <= 0 {
        RetentionPolicy::KeepForever
    } else {
        RetentionPolicy::KeepDays(keep_days)
    };
    for source in SOURCES {
        prune_source(conn, source, policy, today)?;
    }
    Ok(())
}

/// Poda exclusivamente la fuente indicada; nunca mezcla providers al aplicar
/// una política de retención.
pub fn prune_source(
    conn: &Connection,
    source: &str,
    policy: RetentionPolicy,
    today: NaiveDate,
) -> Result<()> {
    let RetentionPolicy::KeepDays(days) = policy else {
        return Ok(());
    };
    if days <= 0 {
        return Ok(());
    }
    let cutoff = (today - chrono::Duration::days(days)).to_string();
    conn.execute(
        "DELETE FROM daily_usage WHERE source = ?1 AND date < ?2",
        params![source, cutoff],
    )?;
    Ok(())
}

fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let value = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(value)
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// --- conversiones de las filas de los escáneres a UsageRow -------------------

pub fn rows_from_claude(models: &[crate::tokens::ModelTokens]) -> Vec<UsageRow> {
    models
        .iter()
        .map(|m| UsageRow {
            provider: "anthropic".into(),
            model: m.model.clone(),
            input: m.input,
            output: m.output,
            cache_read: m.cache_read,
            cache_write: m.cache_creation,
            reasoning: 0,
            total: m.total(),
            cost: 0.0,
        })
        .collect()
}

pub fn rows_from_pi(rows: &[crate::pi_tokens::PiUsageRow]) -> Vec<UsageRow> {
    rows.iter()
        .map(|r| UsageRow {
            provider: r.provider.clone(),
            model: r.model.clone(),
            input: r.totals.input,
            output: r.totals.output,
            cache_read: r.totals.cache_read,
            cache_write: r.totals.cache_write,
            reasoning: 0,
            total: r.totals.total_tokens,
            cost: r.totals.cost_total,
        })
        .collect()
}

pub fn rows_from_opencode(rows: &[crate::opencode_tokens::OpenCodeUsageRow]) -> Vec<UsageRow> {
    rows.iter()
        .map(|r| {
            let input = r.input.unwrap_or(0);
            let output = r.output.unwrap_or(0);
            let cache_read = r.cache_read.unwrap_or(0);
            let cache_write = r.cache_write.unwrap_or(0);
            let reasoning = r.reasoning.unwrap_or(0);
            UsageRow {
                provider: r.provider.clone(),
                model: r.model.clone(),
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
                total: r
                    .total
                    .unwrap_or(input + output + cache_read + cache_write + reasoning),
                cost: r.cost.unwrap_or(0.0),
            }
        })
        .collect()
}

// --- puntos de entrada de alto nivel (abren la base real) --------------------

fn state_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|base| base.join("lazysubs-eye"))
}

fn db_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("history.db"))
}

pub fn database_path() -> Option<PathBuf> {
    db_path()
}

/// Abre (creando el fichero y el esquema si hace falta) la base de historial.
pub fn open() -> Result<Connection> {
    let path = db_path().context("sin HOME ni XDG_STATE_HOME")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        crate::cache::set_permissions_restrictive(dir, true)?;
    }
    if !path.exists() {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)?;
    }
    let conn = Connection::open(&path)?;
    crate::cache::set_permissions_restrictive(&path, false)?;
    init_schema(&conn)?;
    Ok(conn)
}

fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

/// Registra las filas de HOY de una fuente y poda el historial. Silencioso: los
/// errores no rompen la UI. `stats.enabled = false` lo desactiva por completo.
pub fn record_source(source: &str, rows: &[UsageRow]) {
    let config = crate::config::get();
    if !config.stats.enabled {
        return;
    }
    if let Err(e) = (|| -> Result<()> {
        let conn = open()?;
        ingest_single_day(&conn, &today_local().to_string(), source, rows)?;
        prune(&conn, config.stats.history_days, today_local())?;
        Ok(())
    })() {
        let message = crate::diagnostics::sanitize_error(format!(
            "no pude actualizar el historial ({source}): {e:#}"
        ));
        crate::diagnostics::record_last_error("E006", &message);
        eprintln!("lazysubs-eye: {message}");
    }
}

/// Escanea las tres fuentes y registra el uso de hoy. Punto de ingesta del
/// camino fresco de main (waybar/--json en cache-miss). Hace el backfill la
/// primera vez.
pub fn ingest_today() {
    let config = crate::config::get();
    if !config.stats.enabled {
        return;
    }
    maybe_backfill();

    record_source(
        SOURCE_CLAUDE,
        &rows_from_claude(&crate::tokens::claude_today()),
    );
    record_source(SOURCE_PI, &rows_from_pi(&crate::pi_tokens::scan_pi_today()));
    if let Some(rows) = opencode_ready_rows(crate::opencode_tokens::scan_opencode_today()) {
        record_source(SOURCE_OPENCODE, &rows_from_opencode(&rows));
    }
}

fn opencode_ready_rows(
    state: crate::opencode_tokens::OpenCodePanelState,
) -> Option<Vec<crate::opencode_tokens::OpenCodeUsageRow>> {
    use crate::opencode_tokens::OpenCodePanelState::*;
    match state {
        Ready(rows) | Stale { rows, .. } => Some(rows),
        Empty => Some(vec![]),
        Loading | Unavailable(_) => None,
    }
}

/// Puebla los días pasados desde las fuentes que aún existan, una sola vez
/// (marcado en la tabla `meta`). Best-effort por fuente.
pub fn maybe_backfill() {
    maybe_backfill_with_progress_cancelled(|_| {}, || false);
}

/// Ejecuta el backfill notificando tras cada día confirmado o fallido. Debe
/// llamarse desde un worker: leer las fuentes nunca bloquea el render de TUI.
/// Variante cancelable usada por la TUI. La cancelación se consulta entre
/// días, que son las unidades transaccionales: nunca deja medio commit y el
/// último día confirmado permite reanudar en la siguiente ejecución.
pub fn maybe_backfill_with_progress_cancelled<F, C>(mut report: F, cancelled: C)
where
    F: FnMut(BackfillProgress),
    C: Fn() -> bool,
{
    let config = crate::config::get();
    if !config.stats.enabled {
        return;
    }
    let Ok(conn) = open() else { return };
    match get_meta(&conn, BACKFILL_META_KEY) {
        Ok(Some(value)) if value == "done" => return,
        Ok(Some(_)) | Ok(None) => {}
        Err(_) => return,
    }

    let today = today_local().to_string();
    // Los días pasados se congelan; el día en curso lo refresca la ingesta
    // normal, así que aquí no lo tocamos para no pisar un escaneo más fresco.
    let mut plans = vec![
        (
            SOURCE_CLAUDE,
            crate::tokens::claude_by_day()
                .into_iter()
                .map(|(d, m)| (d, rows_from_claude(&m)))
                .collect::<Vec<_>>(),
        ),
        (
            SOURCE_PI,
            crate::pi_tokens::scan_pi_all_days()
                .into_iter()
                .map(|(d, m)| (d, rows_from_pi(&m)))
                .collect::<Vec<_>>(),
        ),
        (
            SOURCE_OPENCODE,
            crate::opencode_tokens::scan_opencode_all_days()
                .into_iter()
                .map(|(d, m)| (d, rows_from_opencode(&m)))
                .collect::<Vec<_>>(),
        ),
    ];
    for (source, days) in &mut plans {
        if let Ok(Some(resume_after)) = get_meta(&conn, &backfill_last_key(source)) {
            days.retain(|(day, _)| day > &resume_after);
        }
    }
    let mut progress = BackfillProgress {
        total_days: plans
            .iter()
            .map(|(_, days)| days.iter().filter(|(day, _)| day != &today).count())
            .sum(),
        ..BackfillProgress::default()
    };
    report(progress.clone());
    let mut had_failure = false;
    let mut was_cancelled = false;
    for (source, days) in plans {
        let mut contiguous = true;
        for (date, rows) in days {
            if cancelled() {
                was_cancelled = true;
                break;
            }
            if date == today {
                continue;
            }
            progress.current_day = Some(date.clone());
            if ingest_single_day(&conn, &date, source, &rows).is_err() {
                had_failure = true;
                contiguous = false;
                progress.failed_days += 1;
            } else if contiguous {
                let _ = set_meta(&conn, &backfill_last_key(source), &date);
            }
            progress.completed_days += 1;
            let _ = set_meta(
                &conn,
                BACKFILL_PROGRESS_KEY,
                &serde_json::to_string(&progress).unwrap_or_default(),
            );
            report(progress.clone());
        }
        if was_cancelled {
            break;
        }
    }

    let _ = prune(&conn, config.stats.history_days, today_local());
    let _ = set_meta(
        &conn,
        BACKFILL_META_KEY,
        if had_failure || was_cancelled {
            "partial"
        } else {
            "done"
        },
    );
}

/// Filas agregadas de una fuente para el periodo (para los paneles de la TUI).
/// Vacío ante cualquier error o si el historial está desactivado.
pub fn period_rows(source: &str, period: Period) -> Vec<UsageRow> {
    if !crate::config::get().stats.enabled {
        return vec![];
    }
    let (start, end) = period_bounds(period, today_local());
    open()
        .and_then(|conn| query_period(&conn, source, &start, &end))
        .unwrap_or_default()
}

/// Estado diario más reciente para presentación; los errores se degradan a
/// ausencia para mantener la TUI operativa (doctor sí informa el fallo de DB).
pub fn latest_source_state(source: &str) -> Option<IngestState> {
    open()
        .and_then(|conn| latest_ingest_state(&conn, source))
        .ok()
        .flatten()
}

/// Serie de la gráfica de gasto (todas las fuentes) para la vista dada. Vacío
/// ante error o si el historial está desactivado.
pub fn graph_data(view: GraphView) -> GraphData {
    if !crate::config::get().stats.enabled {
        return GraphData::default();
    }
    let today = today_local();
    match view {
        GraphView::Week => open()
            .map(|conn| week_series(&conn, today))
            .unwrap_or_default(),
        GraphView::Month => open()
            .map(|conn| month_series(&conn, today))
            .unwrap_or_default(),
        GraphView::Hours => hours_series(today_hourly_total()),
    }
}

/// Suma por hora de hoy de las tres fuentes (Claude/Pi/OpenCode).
fn today_hourly_total() -> [u64; 24] {
    let claude = crate::tokens::claude_today_hourly();
    let pi = crate::pi_tokens::scan_pi_today_hourly();
    let opencode = crate::opencode_tokens::scan_opencode_today_hourly();
    let mut out = [0u64; 24];
    for h in 0..24 {
        out[h] = claude[h] + pi[h] + opencode[h];
    }
    out
}

/// Días de la semana en curso (lunes→domingo), total de todas las fuentes.
fn week_series(conn: &Connection, today: NaiveDate) -> GraphData {
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let lookup: std::collections::HashMap<String, u64> = all_daily_totals(
        conn,
        &monday.to_string(),
        &(monday + chrono::Duration::days(6)).to_string(),
    )
    .unwrap_or_default()
    .into_iter()
    .collect();
    let labels = ["L", "M", "X", "J", "V", "S", "D"];
    let values = (0..7)
        .map(|i| {
            let date = (monday + chrono::Duration::days(i)).to_string();
            lookup.get(&date).copied().unwrap_or(0)
        })
        .collect();
    GraphData {
        values,
        labels: labels.iter().map(|s| s.to_string()).collect(),
    }
}

/// Días del mes en curso (1→hoy), total de todas las fuentes.
fn month_series(conn: &Connection, today: NaiveDate) -> GraphData {
    let first = today.with_day(1).unwrap_or(today);
    let lookup: std::collections::HashMap<String, u64> =
        all_daily_totals(conn, &first.to_string(), &today.to_string())
            .unwrap_or_default()
            .into_iter()
            .collect();
    let days = today.day();
    let mut values = Vec::with_capacity(days as usize);
    let mut labels = Vec::with_capacity(days as usize);
    for d in 1..=days {
        let date = first.with_day(d).unwrap_or(first).to_string();
        values.push(lookup.get(&date).copied().unwrap_or(0));
        labels.push(d.to_string());
    }
    GraphData { values, labels }
}

fn hours_series(hourly: [u64; 24]) -> GraphData {
    GraphData {
        values: hourly.to_vec(),
        labels: (0..24).map(|h| format!("{h:02}")).collect(),
    }
}

/// Serie diaria para el sparkline de una fuente. Vacío si está desactivado.
pub fn sparkline(source: &str) -> Vec<u64> {
    let config = crate::config::get();
    if !config.stats.enabled || !config.stats.sparkline {
        return vec![];
    }
    open()
        .and_then(|conn| series_last_n(&conn, source, today_local(), SPARKLINE_DAYS))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    fn row(model: &str, total: u64) -> UsageRow {
        UsageRow {
            provider: "p".into(),
            model: model.into(),
            input: total,
            total,
            ..Default::default()
        }
    }

    #[test]
    fn period_bounds_calendario() {
        // 2026-07-14 es martes → lunes es el 13; mes empieza el 1.
        let today = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        assert_eq!(
            period_bounds(Period::Today, today),
            ("2026-07-14".into(), "2026-07-14".into())
        );
        assert_eq!(
            period_bounds(Period::Week, today),
            ("2026-07-13".into(), "2026-07-14".into())
        );
        assert_eq!(
            period_bounds(Period::Month, today),
            ("2026-07-01".into(), "2026-07-14".into())
        );
    }

    #[test]
    fn periodo_cicla_y_se_parsea() {
        assert_eq!(Period::Today.next(), Period::Week);
        assert_eq!(Period::Week.next(), Period::Month);
        assert_eq!(Period::Month.next(), Period::Today);
        assert_eq!(Period::parse("semana"), Period::Week);
        assert_eq!(Period::parse("mes"), Period::Month);
        assert_eq!(Period::parse("hoy"), Period::Today);
        assert_eq!(Period::parse("otro"), Period::Today);
    }

    #[test]
    fn record_day_es_autoritativo_y_reemplaza() {
        let conn = mem();
        record_day(
            &conn,
            "2026-07-14",
            SOURCE_CLAUDE,
            &[row("a", 10), row("b", 5)],
        )
        .unwrap();
        // segunda pasada con menos modelos: 'b' desaparece
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &[row("a", 20)]).unwrap();
        let rows = query_period(&conn, SOURCE_CLAUDE, "2026-07-14", "2026-07-14").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "a");
        assert_eq!(rows[0].total, 20);
    }

    #[test]
    fn query_period_agrega_por_modelo_entre_dias() {
        let conn = mem();
        record_day(&conn, "2026-07-13", SOURCE_CLAUDE, &[row("a", 10)]).unwrap();
        record_day(
            &conn,
            "2026-07-14",
            SOURCE_CLAUDE,
            &[row("a", 5), row("b", 30)],
        )
        .unwrap();
        let rows = query_period(&conn, SOURCE_CLAUDE, "2026-07-13", "2026-07-14").unwrap();
        // ordenado por total desc: b(30) antes que a(15)
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "b");
        assert_eq!(rows[0].total, 30);
        assert_eq!(rows[1].model, "a");
        assert_eq!(rows[1].total, 15);
    }

    #[test]
    fn query_period_aisla_por_fuente_y_rango() {
        let conn = mem();
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &[row("a", 10)]).unwrap();
        record_day(&conn, "2026-07-14", SOURCE_PI, &[row("a", 99)]).unwrap();
        record_day(&conn, "2026-07-01", SOURCE_CLAUDE, &[row("a", 7)]).unwrap();
        let rows = query_period(&conn, SOURCE_CLAUDE, "2026-07-13", "2026-07-14").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total, 10); // el del 01 queda fuera del rango
    }

    #[test]
    fn series_last_n_rellena_ceros_y_ordena() {
        let conn = mem();
        let today = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &[row("a", 4)]).unwrap();
        record_day(
            &conn,
            "2026-07-12",
            SOURCE_CLAUDE,
            &[row("a", 1), row("b", 1)],
        )
        .unwrap();
        let series = series_last_n(&conn, SOURCE_CLAUDE, today, 4).unwrap();
        // días 11,12,13,14 → 0, 2, 0, 4
        assert_eq!(series, vec![0, 2, 0, 4]);
    }

    #[test]
    fn prune_borra_lo_viejo_y_respeta_cero() {
        let conn = mem();
        let today = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let days = |conn: &Connection| {
            daily_totals(conn, SOURCE_CLAUDE, "2000-01-01", "2100-01-01")
                .unwrap()
                .len()
        };
        record_day(&conn, "2026-01-01", SOURCE_CLAUDE, &[row("a", 1)]).unwrap();
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &[row("a", 1)]).unwrap();
        prune(&conn, 90, today).unwrap();
        assert_eq!(days(&conn), 1, "el día viejo se poda");

        // keep_days = 0 no borra nada
        record_day(&conn, "2026-01-01", SOURCE_CLAUDE, &[row("a", 1)]).unwrap();
        prune(&conn, 0, today).unwrap();
        assert_eq!(days(&conn), 2);
    }

    #[test]
    fn retention_per_source_never_prunes_another_source() {
        let conn = mem();
        let today = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        record_day(&conn, "2026-01-01", SOURCE_CLAUDE, &[row("a", 1)]).unwrap();
        record_day(&conn, "2026-01-01", SOURCE_PI, &[row("a", 2)]).unwrap();
        prune_source(&conn, SOURCE_CLAUDE, RetentionPolicy::KeepDays(90), today).unwrap();
        assert!(
            query_period(&conn, SOURCE_CLAUDE, "2026-01-01", "2026-01-01")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            query_period(&conn, SOURCE_PI, "2026-01-01", "2026-01-01").unwrap()[0].total,
            2
        );
    }

    #[test]
    fn meta_persiste_y_actualiza() {
        let conn = mem();
        assert_eq!(get_meta(&conn, "k").unwrap(), None);
        set_meta(&conn, "k", "1").unwrap();
        assert_eq!(get_meta(&conn, "k").unwrap().as_deref(), Some("1"));
        set_meta(&conn, "k", "2").unwrap();
        assert_eq!(get_meta(&conn, "k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn ingest_state_persists_per_source_and_day_with_the_day_write() {
        let conn = mem();
        record_day(
            &conn,
            "2026-07-14",
            SOURCE_CLAUDE,
            &[row("a", 10), row("b", 5)],
        )
        .unwrap();

        assert!(matches!(
            ingest_state(&conn, SOURCE_CLAUDE, "2026-07-14").unwrap(),
            Some(IngestState::Ingested {
                day,
                record_count: 2,
                last_rowid: None,
                ..
            }) if day == "2026-07-14"
        ));
        assert_eq!(ingest_state(&conn, SOURCE_PI, "2026-07-14").unwrap(), None);
    }

    #[test]
    fn fingerprint_is_stable_and_changes_with_the_source_rows() {
        let base = vec![row("a", 10)];
        assert_eq!(
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-14", &base),
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-14", &base)
        );
        assert_ne!(
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-14", &base),
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-15", &base)
        );
        assert_ne!(
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-14", &base),
            compute_day_fingerprint(SOURCE_CLAUDE, "2026-07-14", &[row("a", 11)])
        );
    }

    #[test]
    fn unchanged_day_is_detected_after_a_successful_ingest() {
        let conn = mem();
        let rows = vec![row("a", 10)];
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &rows).unwrap();
        let before = get_meta(&conn, &ingest_meta_key(SOURCE_CLAUDE, "2026-07-14")).unwrap();
        assert!(has_same_fingerprint(
            &conn,
            SOURCE_CLAUDE,
            "2026-07-14",
            &rows
        ));
        assert!(!has_same_fingerprint(
            &conn,
            SOURCE_CLAUDE,
            "2026-07-14",
            &[row("a", 11)]
        ));
        assert!(matches!(
            ingest_single_day(&conn, "2026-07-14", SOURCE_CLAUDE, &rows).unwrap(),
            IngestState::Skipped { .. }
        ));
        assert_eq!(
            get_meta(&conn, &ingest_meta_key(SOURCE_CLAUDE, "2026-07-14")).unwrap(),
            before
        );
    }

    #[test]
    fn ingest_state_round_trips_partial_and_failed_without_raw_db_errors() {
        let conn = mem();
        let state = IngestState::Partial {
            day: "2026-07-13".into(),
            record_count: 3,
            last_rowid: Some(42),
            ingested_at: 123,
            reason: "línea de origen no válida".into(),
        };
        let tx = conn.unchecked_transaction().unwrap();
        set_ingest_state_tx(&tx, SOURCE_PI, &state).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            ingest_state(&conn, SOURCE_PI, "2026-07-13").unwrap(),
            Some(state)
        );
    }

    #[test]
    fn partial_day_commits_recoverable_rows_and_a_sanitized_state_atomically() {
        let conn = mem();
        let day = "2026-07-13";
        let state = ingest_partial_day(&conn, day, SOURCE_PI, &[row("válido", 7)]).unwrap();

        assert!(matches!(
            state,
            IngestState::Partial {
                record_count: 1,
                reason,
                ..
            } if reason == "algunos registros de origen no eran válidos"
        ));
        assert_eq!(
            query_period(&conn, SOURCE_PI, day, day).unwrap(),
            vec![row("válido", 7)]
        );
        assert!(get_meta(&conn, &fingerprint_meta_key(SOURCE_PI, day))
            .unwrap()
            .is_some());
    }

    #[test]
    fn failed_day_rolls_back_data_and_records_a_sanitized_failed_state() {
        let conn = mem();
        let day = "2026-07-14";
        record_day(&conn, day, SOURCE_CLAUDE, &[row("stable", 10)]).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_broken BEFORE INSERT ON daily_usage
             WHEN NEW.model = 'broken'
             BEGIN SELECT RAISE(ABORT, 'injected database detail'); END;",
        )
        .unwrap();

        assert!(ingest_single_day(&conn, day, SOURCE_CLAUDE, &[row("broken", 99)]).is_err());

        let rows = query_period(&conn, SOURCE_CLAUDE, day, day).unwrap();
        assert_eq!(rows, vec![row("stable", 10)]);
        assert!(matches!(
            ingest_state(&conn, SOURCE_CLAUDE, day).unwrap(),
            Some(IngestState::Failed { reason, .. }) if reason == "no se pudo persistir este día"
        ));
    }

    #[test]
    fn backfill_integration_30_days_continues_after_day_15_and_retries_only_failure() {
        let conn = mem();
        conn.execute_batch(
            "CREATE TRIGGER reject_day_15 BEFORE INSERT ON daily_usage
             WHEN NEW.date = '2026-06-15'
             BEGIN SELECT RAISE(ABORT, 'fallo inyectado'); END;",
        )
        .unwrap();

        for day in 1..=30 {
            let date = format!("2026-06-{day:02}");
            let result = ingest_single_day(&conn, &date, SOURCE_CLAUDE, &[row("m", day)]);
            assert_eq!(result.is_err(), day == 15);
        }
        assert!(matches!(
            ingest_state(&conn, SOURCE_CLAUDE, "2026-06-15").unwrap(),
            Some(IngestState::Failed { .. })
        ));
        assert!(matches!(
            ingest_state(&conn, SOURCE_CLAUDE, "2026-06-30").unwrap(),
            Some(IngestState::Ingested { .. })
        ));
        let stored_days = |connection: &Connection| {
            connection
                .query_row(
                    "SELECT COUNT(DISTINCT date) FROM daily_usage WHERE source = ?1",
                    [SOURCE_CLAUDE],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
        };
        assert_eq!(stored_days(&conn), 29);

        conn.execute_batch("DROP TRIGGER reject_day_15;").unwrap();
        let mut ingested = 0;
        let mut skipped = 0;
        for day in 1..=30 {
            let date = format!("2026-06-{day:02}");
            match ingest_single_day(&conn, &date, SOURCE_CLAUDE, &[row("m", day)]).unwrap() {
                IngestState::Ingested { .. } => ingested += 1,
                IngestState::Skipped { .. } => skipped += 1,
                state => panic!("estado inesperado: {state:?}"),
            }
        }
        assert_eq!((ingested, skipped), (1, 29));
        assert_eq!(stored_days(&conn), 30);
        assert!(matches!(
            latest_ingest_state(&conn, SOURCE_CLAUDE).unwrap(),
            Some(IngestState::Ingested { day, .. }) if day == "2026-06-30"
        ));
    }

    #[test]
    fn graphview_cicla() {
        assert_eq!(GraphView::Week.next(), GraphView::Month);
        assert_eq!(GraphView::Month.next(), GraphView::Hours);
        assert_eq!(GraphView::Hours.next(), GraphView::Week);
    }

    #[test]
    fn all_daily_totals_suma_todas_las_fuentes() {
        let conn = mem();
        record_day(&conn, "2026-07-13", SOURCE_CLAUDE, &[row("a", 10)]).unwrap();
        record_day(&conn, "2026-07-13", SOURCE_PI, &[row("a", 5)]).unwrap();
        record_day(&conn, "2026-07-14", SOURCE_OPENCODE, &[row("a", 20)]).unwrap();
        let totals = all_daily_totals(&conn, "2026-07-13", "2026-07-14").unwrap();
        assert_eq!(
            totals,
            vec![
                ("2026-07-13".to_string(), 15),
                ("2026-07-14".to_string(), 20)
            ]
        );
    }

    #[test]
    fn streaming_query_procesa_10k_en_batches_acotados() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE numbers (value INTEGER NOT NULL)", [])
            .unwrap();
        let tx = conn.transaction().unwrap();
        for value in 0..10_000_i64 {
            tx.execute("INSERT INTO numbers VALUES (?1)", [value])
                .unwrap();
        }
        tx.commit().unwrap();
        let mut count = 0;
        let mut max_batch = 0;
        query_streaming(
            &conn,
            "SELECT value FROM numbers ORDER BY value",
            [],
            128,
            |row| row.get::<_, i64>(0),
            |batch| {
                count += batch.len();
                max_batch = max_batch.max(batch.len());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 10_000);
        assert_eq!(max_batch, 128);

        let mut empty_batches = 0;
        query_streaming(
            &conn,
            "SELECT value FROM numbers WHERE 0",
            [],
            128,
            |row| row.get::<_, i64>(0),
            |_| {
                empty_batches += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(empty_batches, 0);
    }

    #[test]
    fn source_provenance_se_actualiza_en_la_misma_transaccion() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        record_day(
            &conn,
            "2026-07-02",
            SOURCE_CLAUDE,
            &[UsageRow {
                provider: "anthropic".into(),
                model: "a".into(),
                total: 2,
                ..UsageRow::default()
            }],
        )
        .unwrap();
        record_day(
            &conn,
            "2026-07-01",
            SOURCE_CLAUDE,
            &[UsageRow {
                provider: "anthropic".into(),
                model: "b".into(),
                total: 1,
                ..UsageRow::default()
            }],
        )
        .unwrap();
        let raw = get_meta(&conn, &source_meta_key(SOURCE_CLAUDE))
            .unwrap()
            .unwrap();
        let provenance: SourceProvenance = serde_json::from_str(&raw).unwrap();
        assert_eq!(provenance.first_ingest, "2026-07-01");
        assert_eq!(provenance.last_ingest, "2026-07-02");
        assert_eq!(provenance.record_count, 2);
    }

    #[test]
    fn cutoff_historico_es_fin_de_dia_y_hoy_no_es_futuro() {
        let historical = NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
        let cutoff = get_cutoff_timestamp(SOURCE_PI, historical);
        let local = chrono::DateTime::from_timestamp(cutoff, 0)
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(local.date_naive(), historical);
        assert_eq!((local.hour(), local.minute(), local.second()), (23, 59, 59));
        assert!(
            get_cutoff_timestamp(SOURCE_OPENCODE, Local::now().date_naive())
                <= chrono::Utc::now().timestamp()
        );
    }

    #[test]
    fn week_series_lunes_a_domingo_con_ceros() {
        let conn = mem();
        // 2026-07-15 es miércoles; la semana va del lunes 13 al domingo 19.
        let today = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        record_day(&conn, "2026-07-13", SOURCE_CLAUDE, &[row("a", 10)]).unwrap();
        record_day(&conn, "2026-07-13", SOURCE_PI, &[row("a", 5)]).unwrap();
        record_day(&conn, "2026-07-14", SOURCE_CLAUDE, &[row("a", 20)]).unwrap();
        let g = week_series(&conn, today);
        assert_eq!(g.values, vec![15, 20, 0, 0, 0, 0, 0]);
        assert_eq!(g.labels, vec!["L", "M", "X", "J", "V", "S", "D"]);
    }

    #[test]
    fn month_series_del_uno_a_hoy() {
        let conn = mem();
        let today = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        record_day(&conn, "2026-07-01", SOURCE_CLAUDE, &[row("a", 7)]).unwrap();
        record_day(&conn, "2026-07-15", SOURCE_PI, &[row("a", 3)]).unwrap();
        let g = month_series(&conn, today);
        assert_eq!(g.values.len(), 15);
        assert_eq!(g.values[0], 7);
        assert_eq!(g.values[14], 3);
        assert_eq!(g.labels.first().map(String::as_str), Some("1"));
        assert_eq!(g.labels.last().map(String::as_str), Some("15"));
    }

    #[test]
    fn hours_series_pasa_las_24() {
        let mut hourly = [0u64; 24];
        hourly[9] = 42;
        let g = hours_series(hourly);
        assert_eq!(g.values.len(), 24);
        assert_eq!(g.values[9], 42);
        assert_eq!(g.labels[0], "00");
        assert_eq!(g.labels[23], "23");
    }

    #[test]
    fn conversion_opencode_calcula_total_si_falta() {
        let rows = rows_from_opencode(&[crate::opencode_tokens::OpenCodeUsageRow {
            provider: "openai".into(),
            model: "gpt".into(),
            input: Some(3),
            output: Some(4),
            reasoning: Some(1),
            cache_read: Some(2),
            cache_write: Some(0),
            total: None,
            cost: None,
        }]);
        assert_eq!(rows[0].total, 10);
        assert_eq!(rows[0].cost, 0.0);
    }
}
