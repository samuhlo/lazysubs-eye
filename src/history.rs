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
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const SOURCE_CLAUDE: &str = "claude";
pub const SOURCE_PI: &str = "pi";
pub const SOURCE_OPENCODE: &str = "opencode";

/// Fuentes con historial, en el orden de los paneles de la TUI.
pub const SOURCES: [&str; 3] = [SOURCE_CLAUDE, SOURCE_PI, SOURCE_OPENCODE];

const BACKFILL_META_KEY: &str = "backfill_v1";
/// Nº de días que abarca el sparkline bajo cada panel.
pub const SPARKLINE_DAYS: usize = 14;

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

/// Total diario sumando **todas** las fuentes en el rango [start, end].
pub fn all_daily_totals(conn: &Connection, start: &str, end: &str) -> Result<Vec<(String, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT date, SUM(total) FROM daily_usage
         WHERE date >= ?1 AND date <= ?2
         GROUP BY date ORDER BY date",
    )?;
    let rows = stmt.query_map(params![start, end], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?.max(0) as u64,
        ))
    })?;
    Ok(rows.filter_map(Result::ok).collect())
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
    if keep_days <= 0 {
        return Ok(());
    }
    let cutoff = (today - chrono::Duration::days(keep_days)).to_string();
    conn.execute("DELETE FROM daily_usage WHERE date < ?1", params![cutoff])?;
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

/// Abre (creando el fichero y el esquema si hace falta) la base de historial.
pub fn open() -> Result<Connection> {
    let path = db_path().context("sin HOME ni XDG_STATE_HOME")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(&path)?;
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
        record_day(&conn, &today_local().to_string(), source, rows)?;
        prune(&conn, config.stats.history_days, today_local())?;
        Ok(())
    })() {
        eprintln!("lazysubs-eye: no pude actualizar el historial ({source}): {e:#}");
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
    let config = crate::config::get();
    if !config.stats.enabled {
        return;
    }
    let Ok(conn) = open() else { return };
    match get_meta(&conn, BACKFILL_META_KEY) {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(_) => return,
    }

    let today = today_local().to_string();
    // Los días pasados se congelan; el día en curso lo refresca la ingesta
    // normal, así que aquí no lo tocamos para no pisar un escaneo más fresco.
    let record_all = |source: &str, by_day: Vec<(String, Vec<UsageRow>)>| {
        for (date, rows) in by_day {
            if date == today {
                continue;
            }
            let _ = record_day(&conn, &date, source, &rows);
        }
    };
    record_all(
        SOURCE_CLAUDE,
        crate::tokens::claude_by_day()
            .into_iter()
            .map(|(d, m)| (d, rows_from_claude(&m)))
            .collect(),
    );
    record_all(
        SOURCE_PI,
        crate::pi_tokens::scan_pi_all_days()
            .into_iter()
            .map(|(d, m)| (d, rows_from_pi(&m)))
            .collect(),
    );
    record_all(
        SOURCE_OPENCODE,
        crate::opencode_tokens::scan_opencode_all_days()
            .into_iter()
            .map(|(d, m)| (d, rows_from_opencode(&m)))
            .collect(),
    );

    let _ = prune(&conn, config.stats.history_days, today_local());
    let _ = set_meta(&conn, BACKFILL_META_KEY, "done");
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
    fn meta_persiste_y_actualiza() {
        let conn = mem();
        assert_eq!(get_meta(&conn, "k").unwrap(), None);
        set_meta(&conn, "k", "1").unwrap();
        assert_eq!(get_meta(&conn, "k").unwrap().as_deref(), Some("1"));
        set_meta(&conn, "k", "2").unwrap();
        assert_eq!(get_meta(&conn, "k").unwrap().as_deref(), Some("2"));
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
