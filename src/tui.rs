use crate::history::{self, GraphData, GraphView, Period};
use crate::opencode_tokens::{
    self, OpenCodePanelState, OpenCodeUnavailableReason, OpenCodeUsageRow,
};
use crate::output::countdown;
use crate::pi_tokens::{self, PiUsageRow};
use crate::providers::{ProviderStatus, Status};
use crate::tokens::{self, fmt_count, ModelTokens};
use crate::{cache, providers};
use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{
    Block, BorderType, Clear, LineGauge, Padding, Paragraph, Row, Sparkline, Table,
};
use ratatui::Frame;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(60);
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);

const ACCENT: Color = Color::Yellow;
const DIM: Color = Color::DarkGray;

fn env_flag(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

fn should_use_color_for(force: Option<&str>, no_color: Option<&str>, term: Option<&str>) -> bool {
    env_flag(force) || (!env_flag(no_color) && term != Some("dumb"))
}

fn should_use_color() -> bool {
    should_use_color_for(
        std::env::var("FORCE_COLOR").ok().as_deref(),
        std::env::var("NO_COLOR").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
}

fn is_interrupt_key(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn accent_color() -> Color {
    if should_use_color() {
        ACCENT
    } else {
        Color::Reset
    }
}

fn dim_color() -> Color {
    if should_use_color() {
        DIM
    } else {
        Color::Reset
    }
}

fn supports_utf8_for(lang: Option<&str>) -> bool {
    lang.is_some_and(|lang| {
        let lang = lang.to_ascii_uppercase();
        lang.contains("UTF-8") || lang.contains("UTF8")
    })
}

fn supports_utf8() -> bool {
    supports_utf8_for(std::env::var("LANG").ok().as_deref())
}

fn ascii_icon(icon: &str) -> &str {
    if supports_utf8() {
        return icon;
    }
    match icon {
        "✳" => "*",
        "⬡" => "#",
        "◆" => "+",
        "⚠" => "!",
        "✓" => "v",
        "✗" => "x",
        _ => "?",
    }
}

fn state_marker(state: PanelState, utf8: bool) -> &'static str {
    match (state, utf8) {
        (PanelState::Ready, true) => "[✓]",
        (PanelState::Ready, false) => "[v]",
        (PanelState::Partial | PanelState::Stale, _) => "[!]",
        (PanelState::Unavailable | PanelState::NotConfigured, true) => "[✗]",
        (PanelState::Unavailable | PanelState::NotConfigured, false) => "[x]",
        (PanelState::Loading, _) => "[…]",
        (PanelState::Empty, _) => "[-]",
    }
}

/// Recorta líneas en tiempo de render: un resize no deja offsets ni alturas
/// obsoletos en el estado de la aplicación.
fn layout_with_scroll<'a>(area: Rect, content: &'a [Line<'a>]) -> Vec<Line<'a>> {
    layout_with_scroll_offset(area, content, 0)
}

fn layout_with_scroll_offset<'a>(
    area: Rect,
    content: &'a [Line<'a>],
    offset: usize,
) -> Vec<Line<'a>> {
    let capacity = area.height as usize;
    if capacity == 0 {
        return Vec::new();
    }
    let start = offset.min(content.len().saturating_sub(1));
    let remaining = content.len().saturating_sub(start);
    if remaining <= capacity {
        return content[start..].to_vec();
    }
    let visible = capacity.saturating_sub(1);
    let mut lines = content[start..start + visible].to_vec();
    lines.push(Line::from(format!(
        "↓ {} línea(s) más",
        remaining.saturating_sub(visible)
    )));
    lines
}

pub fn run() -> Result<()> {
    let mut terminal = ratatui::init();
    let _restore = RestoreTerminal(ratatui::restore);
    App::new().run(&mut terminal)
}

struct RestoreTerminal(fn());

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        (self.0)();
    }
}

enum Update {
    Status(Status),
    Tokens(Vec<ModelTokens>),
    PiTokens(Vec<PiUsageRow>),
    OpenCodeTokens(OpenCodePanelState),
    BackfillProgress(history::BackfillProgress),
    /// El backfill del historial terminó; toca recargar los agregados.
    Backfilled,
    /// Datos de la gráfica de gasto (para la vista pedida).
    Graph(GraphView, GraphData),
    ClearLoading {
        source: ScanSource,
        timed_out: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanSource {
    Claude,
    Pi,
    OpenCode,
}

/// El worker posee este guard. Tanto un retorno normal como un panic liberan
/// el estado loading enviando un mensaje al hilo de UI.
struct LoadingGuard {
    source: ScanSource,
    expire_at: Instant,
    tx: mpsc::Sender<Update>,
}

/// Coalesce refreshes sin bloquear el hilo de UI. AtomicBool basta porque el
/// dato solo expresa ownership del worker; los resultados siguen por mpsc.
#[derive(Default)]
struct RefreshScheduler {
    active: Arc<AtomicBool>,
    pending: AtomicBool,
    pending_force: AtomicBool,
}

impl RefreshScheduler {
    fn maybe_schedule_refresh(&self, force: bool) -> bool {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return true;
        }
        self.pending.store(true, Ordering::Release);
        if force {
            self.pending_force.store(true, Ordering::Release);
        }
        false
    }

    fn finished(&self) -> Option<bool> {
        self.active.store(false, Ordering::Release);
        self.pending
            .swap(false, Ordering::AcqRel)
            .then(|| self.pending_force.swap(false, Ordering::AcqRel))
    }
}

impl LoadingGuard {
    fn new(source: ScanSource, tx: mpsc::Sender<Update>, timeout: Duration) -> Self {
        Self {
            source,
            expire_at: Instant::now() + timeout,
            tx,
        }
    }

    fn expired(&self, now: Instant) -> bool {
        now >= self.expire_at
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        let _ = self.tx.send(Update::ClearLoading {
            source: self.source,
            timed_out: self.expired(Instant::now()),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)] // Partial/NotConfigured se activan al integrar diagnósticos por panel.
enum PanelState {
    Loading,
    Ready,
    Empty,
    Partial,
    Unavailable,
    Stale,
    NotConfigured,
}

struct App {
    status: Option<Status>,
    tokens: Vec<ModelTokens>,
    pi_tokens: Vec<PiUsageRow>,
    opencode_tokens: OpenCodePanelState,
    refreshing: bool,
    refresh_scheduler: RefreshScheduler,
    tx: mpsc::Sender<Update>,
    rx: mpsc::Receiver<Update>,
    tokens_scanning: bool,
    pi_tokens_scanning: bool,
    opencode_scanning: bool,
    scan_deadlines: [Option<Instant>; 3],
    last_refresh: Instant,
    /// Periodo de los paneles de tokens (hoy/semana/mes).
    period: Period,
    /// Agregados del historial por fuente para el periodo actual.
    history_rows: HashMap<&'static str, Vec<history::UsageRow>>,
    history_states: HashMap<&'static str, history::IngestState>,
    /// Serie diaria (sparkline) por fuente.
    history_spark: HashMap<&'static str, Vec<u64>>,
    /// Gráfica de gasto: abierta o no, vista actual y datos (None = cargando).
    graph_open: bool,
    graph_view: GraphView,
    graph_data: Option<GraphData>,
    /// Cursor del panel de opciones; None = cerrado.
    settings_cursor: Option<usize>,
    settings_error: Option<String>,
    backfill_progress: Option<history::BackfillProgress>,
    backfill_active: bool,
    backfill_cancelled: Arc<AtomicBool>,
    show_help: bool,
    scroll_offset: usize,
}

/// Ítems del panel de opciones. Los índices apuntan a PROVIDERS / PANELS.
#[derive(Clone, Copy, PartialEq)]
enum Setting {
    Section(&'static str),
    Notifications,
    Cooldown,
    Colors,
    ShowAccount,
    WarningAt,
    CriticalAt,
    Ttl,
    Provider(usize),
    WaybarPercent,
    WaybarProvider(usize),
    WaybarWindow(usize),
    TuiProvider(usize),
    TuiPanel(usize),
    StatsEnabled,
    StatsPeriod,
    StatsHistoryDays,
    StatsSparkline,
}

const PROVIDERS: [(&str, &str); 3] = [
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("minimax", "MiniMax"),
];
const PANELS: [(&str, &str); 3] = [
    ("claude_tokens", "tokens Claude"),
    ("pi_tokens", "tokens Pi"),
    ("opencode_tokens", "tokens OpenCode"),
];
const PROVIDER_IDS: [&str; 3] = ["claude", "codex", "minimax"];
const PANEL_IDS: [&str; 3] = ["claude_tokens", "pi_tokens", "opencode_tokens"];

/// Fuente de historial de cada panel de tokens (mismo orden que PANEL_IDS).
const PANEL_SOURCES: [&str; 3] = [
    history::SOURCE_CLAUDE,
    history::SOURCE_PI,
    history::SOURCE_OPENCODE,
];

fn settings_items() -> Vec<Setting> {
    let mut items = vec![
        Setting::Section("general"),
        Setting::Notifications,
        Setting::Cooldown,
        Setting::Colors,
        Setting::ShowAccount,
        Setting::WarningAt,
        Setting::CriticalAt,
        Setting::Ttl,
        Setting::Section("providers"),
    ];
    items.extend((0..PROVIDERS.len()).map(Setting::Provider));
    // Las filas de visibilidad por superficie salen de las cuentas
    // configuradas (incluye ids compuestos como "claude:trabajo").
    let surface = surface_providers().len();
    items.push(Setting::Section("waybar"));
    items.push(Setting::WaybarPercent);
    items.extend((0..surface).map(Setting::WaybarProvider));
    items.push(Setting::Section("ventana en la barra"));
    items.extend((0..surface).map(Setting::WaybarWindow));
    items.push(Setting::Section("tui"));
    items.extend((0..surface).map(Setting::TuiProvider));
    items.extend((0..PANELS.len()).map(Setting::TuiPanel));
    items.push(Setting::Section("historial"));
    items.push(Setting::StatsEnabled);
    items.push(Setting::StatsPeriod);
    items.push(Setting::StatsHistoryDays);
    items.push(Setting::StatsSparkline);
    items
}

/// Providers por cuenta (id, nombre) para las filas de visibilidad del panel.
fn surface_providers() -> Vec<(String, String)> {
    providers::configured_providers()
}

fn in_list(list: &Option<Vec<String>>, id: &str) -> bool {
    match list {
        None => true,
        Some(items) => items.iter().any(|x| x == id),
    }
}

impl App {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let config = crate::config::get();
        let period = if config.stats.enabled {
            Period::parse(&config.stats.default_period)
        } else {
            Period::Today
        };
        Self {
            status: None,
            tokens: vec![],
            pi_tokens: vec![],
            opencode_tokens: OpenCodePanelState::Loading,
            refreshing: false,
            refresh_scheduler: RefreshScheduler::default(),
            tx,
            rx,
            tokens_scanning: false,
            pi_tokens_scanning: false,
            opencode_scanning: false,
            scan_deadlines: [None; 3],
            last_refresh: Instant::now(),
            period,
            history_rows: HashMap::new(),
            history_states: HashMap::new(),
            history_spark: HashMap::new(),
            graph_open: false,
            graph_view: GraphView::Week,
            graph_data: None,
            settings_cursor: None,
            settings_error: None,
            backfill_progress: None,
            backfill_active: false,
            backfill_cancelled: Arc::new(AtomicBool::new(false)),
            show_help: false,
            scroll_offset: 0,
        }
    }

    fn token_panel_state(&self, idx: usize) -> PanelState {
        match idx {
            0 if self.tokens_scanning => PanelState::Loading,
            0 if self.tokens.is_empty() => PanelState::Empty,
            0 => PanelState::Ready,
            1 if self.pi_tokens_scanning => PanelState::Loading,
            1 if self.pi_tokens.is_empty() => PanelState::Empty,
            1 => PanelState::Ready,
            _ => match &self.opencode_tokens {
                OpenCodePanelState::Loading => PanelState::Loading,
                OpenCodePanelState::Ready(_) => PanelState::Ready,
                OpenCodePanelState::Empty => PanelState::Empty,
                OpenCodePanelState::Stale { .. } => PanelState::Stale,
                OpenCodePanelState::Unavailable(_) => PanelState::Unavailable,
            },
        }
    }

    fn history_panel_state(&self, source: &str) -> PanelState {
        if !crate::config::get().stats.enabled {
            return PanelState::NotConfigured;
        }
        if self.backfill_active {
            return if self
                .backfill_progress
                .as_ref()
                .is_some_and(|progress| progress.failed_days > 0)
            {
                PanelState::Partial
            } else {
                PanelState::Loading
            };
        }
        if let Some(state) = self.history_states.get(source) {
            match state {
                history::IngestState::Partial { .. } => return PanelState::Partial,
                history::IngestState::InProgress { .. } | history::IngestState::Pending { .. } => {
                    return PanelState::Loading
                }
                history::IngestState::Failed { .. } => return PanelState::Unavailable,
                history::IngestState::Ingested { .. } | history::IngestState::Skipped { .. } => {}
            }
        }
        match self.history_rows.get(source) {
            Some(rows) if !rows.is_empty() => PanelState::Ready,
            Some(_) => PanelState::Empty,
            None => PanelState::Unavailable,
        }
    }

    /// Recarga (en segundo plano) los datos de la gráfica para la vista actual.
    fn reload_graph(&mut self) {
        self.graph_data = None; // "cargando…" hasta que llegue
        let view = self.graph_view;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Update::Graph(view, history::graph_data(view)));
        });
    }

    /// Recarga los agregados del historial (filas del periodo + sparkline) de
    /// una fuente. Barato: consultas SQLite indexadas.
    fn reload_source_history(&mut self, source: &'static str) {
        if !crate::config::get().stats.enabled {
            self.history_rows.remove(source);
            self.history_spark.remove(source);
            self.history_states.remove(source);
            return;
        }
        self.history_rows
            .insert(source, history::period_rows(source, self.period));
        self.history_spark
            .insert(source, history::sparkline(source));
        match history::latest_source_state(source) {
            Some(state) => {
                self.history_states.insert(source, state);
            }
            None => {
                self.history_states.remove(source);
            }
        }
    }

    fn reload_history(&mut self) {
        for source in history::SOURCES {
            self.reload_source_history(source);
        }
    }

    /// Etiquetas de las ventanas de un provider (de la última consulta), para
    /// el selector "ventana en la barra" del panel de opciones.
    fn provider_window_labels(&self, id: &str) -> Vec<String> {
        self.status
            .as_ref()
            .and_then(|s| s.providers.iter().find(|p| p.id == id))
            .map(|p| p.windows.iter().map(|w| w.label.clone()).collect())
            .unwrap_or_default()
    }

    fn settings_move(&mut self, delta: i64) {
        let items = settings_items();
        let Some(mut cursor) = self.settings_cursor else {
            return;
        };
        loop {
            cursor = (cursor as i64 + delta).rem_euclid(items.len() as i64) as usize;
            if !matches!(items[cursor], Setting::Section(_)) {
                break;
            }
        }
        self.settings_cursor = Some(cursor);
    }

    /// Aplica un cambio: `dir` 0 = toggle (espacio/enter), ±1 = ajustar (←/→).
    fn settings_apply(&mut self, dir: i64) {
        let items = settings_items();
        let Some(cursor) = self.settings_cursor else {
            return;
        };
        let mut config = crate::config::get();
        match items[cursor] {
            Setting::Section(_) => return,
            Setting::Notifications => config.notifications = !config.notifications,
            Setting::Colors => config.colors = !config.colors,
            Setting::ShowAccount => config.show_account = !config.show_account,
            Setting::WarningAt if dir != 0 => {
                config.warning_at = (config.warning_at + 5.0 * dir as f64).clamp(5.0, 100.0)
            }
            Setting::CriticalAt if dir != 0 => {
                config.critical_at = (config.critical_at + 5.0 * dir as f64).clamp(5.0, 100.0)
            }
            Setting::Ttl if dir != 0 => config.ttl = (config.ttl + 30 * dir).clamp(10, 3600),
            Setting::Cooldown if dir != 0 => {
                config.notification_cooldown =
                    (config.notification_cooldown + 300 * dir).clamp(0, 6 * 3600)
            }
            Setting::WarningAt | Setting::CriticalAt | Setting::Ttl | Setting::Cooldown => return,
            Setting::Provider(i) => {
                let flag = match PROVIDER_IDS[i] {
                    "claude" => &mut config.providers.claude,
                    "codex" => &mut config.providers.codex,
                    _ => &mut config.providers.minimax,
                };
                *flag = !*flag;
            }
            Setting::WaybarPercent => config.waybar.percent = Some(!config.waybar.percent()),
            Setting::WaybarProvider(i) => {
                let surface = surface_providers();
                let all: Vec<&str> = surface.iter().map(|(id, _)| id.as_str()).collect();
                crate::config::toggle_id(&mut config.waybar.providers, &all, &surface[i].0)
            }
            Setting::WaybarWindow(i) => {
                let surface = surface_providers();
                let Some((id, _)) = surface.get(i).cloned() else {
                    return;
                };
                // Opciones: "auto" (worst) + una por ventana del provider.
                let mut options: Vec<Option<String>> = vec![None];
                options.extend(self.provider_window_labels(&id).into_iter().map(Some));
                let current = config
                    .waybar
                    .window
                    .as_ref()
                    .and_then(|m| m.get(&id))
                    .cloned();
                let cur = options.iter().position(|o| o == &current).unwrap_or(0);
                let step = if dir == 0 { 1 } else { dir };
                let next = (cur as i64 + step).rem_euclid(options.len() as i64) as usize;
                let map = config.waybar.window.get_or_insert_with(Default::default);
                match &options[next] {
                    Some(label) => {
                        map.insert(id.clone(), label.clone());
                    }
                    None => {
                        map.remove(&id);
                    }
                }
                if map.is_empty() {
                    config.waybar.window = None;
                }
            }
            Setting::TuiProvider(i) => {
                let surface = surface_providers();
                let all: Vec<&str> = surface.iter().map(|(id, _)| id.as_str()).collect();
                crate::config::toggle_id(&mut config.tui.providers, &all, &surface[i].0)
            }
            Setting::TuiPanel(i) => {
                crate::config::toggle_id(&mut config.tui.panels, &PANEL_IDS, PANEL_IDS[i])
            }
            Setting::StatsEnabled => config.stats.enabled = !config.stats.enabled,
            Setting::StatsSparkline => config.stats.sparkline = !config.stats.sparkline,
            Setting::StatsPeriod if dir != 0 => {
                let period = Period::parse(&config.stats.default_period).next();
                config.stats.default_period = period.label().to_string();
            }
            Setting::StatsHistoryDays if dir != 0 => {
                config.stats.history_days = (config.stats.history_days + 30 * dir).clamp(0, 3650)
            }
            Setting::StatsPeriod | Setting::StatsHistoryDays => return,
        }
        crate::config::set(config.clone());
        self.settings_error = crate::config::persist(&config)
            .err()
            .map(|e| format!("{e:#}"));
        // Un panel recién activado necesita su escaneo; refresh barato (cache).
        if matches!(items[cursor], Setting::TuiPanel(_)) {
            self.refresh(false);
        }
        // Cambios que afectan al historial: sincronizar periodo y recargar.
        match items[cursor] {
            Setting::StatsPeriod => {
                self.period = Period::parse(&config.stats.default_period);
                self.reload_history();
            }
            Setting::StatsEnabled => {
                if config.stats.enabled {
                    self.spawn_backfill();
                }
                self.reload_history();
            }
            Setting::StatsSparkline => self.reload_history(),
            _ => {}
        }
    }

    /// Lanza el backfill del historial en segundo plano (una sola vez de por
    /// vida); al terminar avisa para recargar los agregados.
    fn spawn_backfill(&mut self) {
        if self.backfill_active {
            return;
        }
        self.backfill_active = true;
        self.backfill_cancelled.store(false, Ordering::Release);
        let tx = self.tx.clone();
        let cancelled = Arc::clone(&self.backfill_cancelled);
        std::thread::spawn(move || {
            history::maybe_backfill_with_progress_cancelled(
                |progress| {
                    let _ = tx.send(Update::BackfillProgress(progress));
                },
                || cancelled.load(Ordering::Acquire),
            );
            let _ = tx.send(Update::Backfilled);
        });
    }

    fn refresh(&mut self, force: bool) {
        if !self.refresh_scheduler.maybe_schedule_refresh(force) {
            return;
        }
        self.refreshing = true;
        self.last_refresh = Instant::now();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let status = if force {
                None
            } else {
                cache::load(crate::config::get().ttl)
            }
            .unwrap_or_else(|| {
                let fresh = providers::collect_all();
                cache::save(&fresh);
                crate::notify::check(&fresh);
                fresh
            });
            let _ = tx.send(Update::Status(status));
        });
        let panels = &crate::config::get().tui;
        if panels.panel("claude_tokens") && self.begin_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _guard = LoadingGuard::new(ScanSource::Claude, tx.clone(), SCAN_TIMEOUT);
                let _ = tx.send(Update::Tokens(tokens::claude_today()));
            });
        }
        if panels.panel("pi_tokens") && self.begin_pi_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _guard = LoadingGuard::new(ScanSource::Pi, tx.clone(), SCAN_TIMEOUT);
                let _ = tx.send(Update::PiTokens(pi_tokens::scan_pi_today()));
            });
        }
        if panels.panel("opencode_tokens") && self.begin_opencode_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _guard = LoadingGuard::new(ScanSource::OpenCode, tx.clone(), SCAN_TIMEOUT);
                let _ = tx.send(Update::OpenCodeTokens(
                    opencode_tokens::scan_opencode_today(),
                ));
            });
        }
    }

    fn begin_token_scan(&mut self) -> bool {
        if self.tokens_scanning {
            return false;
        }
        self.tokens_scanning = true;
        self.scan_deadlines[0] = Some(Instant::now() + SCAN_TIMEOUT);
        true
    }

    fn begin_pi_token_scan(&mut self) -> bool {
        if self.pi_tokens_scanning {
            return false;
        }
        self.pi_tokens_scanning = true;
        self.scan_deadlines[1] = Some(Instant::now() + SCAN_TIMEOUT);
        true
    }

    fn begin_opencode_token_scan(&mut self) -> bool {
        if self.opencode_scanning {
            return false;
        }
        self.opencode_scanning = true;
        self.scan_deadlines[2] = Some(Instant::now() + SCAN_TIMEOUT);
        true
    }

    fn clear_loading(&mut self, source: ScanSource) {
        let (flag, deadline) = match source {
            ScanSource::Claude => (&mut self.tokens_scanning, &mut self.scan_deadlines[0]),
            ScanSource::Pi => (&mut self.pi_tokens_scanning, &mut self.scan_deadlines[1]),
            ScanSource::OpenCode => (&mut self.opencode_scanning, &mut self.scan_deadlines[2]),
        };
        *flag = false;
        *deadline = None;
    }

    fn clear_expired_loading(&mut self, now: Instant) {
        for (idx, source) in [ScanSource::Claude, ScanSource::Pi, ScanSource::OpenCode]
            .into_iter()
            .enumerate()
        {
            if self.scan_deadlines[idx].is_some_and(|deadline| now >= deadline) {
                self.clear_loading(source);
            }
        }
    }

    fn apply_update(&mut self, update: Update) {
        match update {
            Update::Status(status) => {
                self.status = Some(status);
                self.refreshing = false;
                if let Some(force) = self.refresh_scheduler.finished() {
                    self.refresh(force);
                }
            }
            Update::Tokens(tokens) => {
                self.tokens = tokens;
                self.clear_loading(ScanSource::Claude);
                history::record_source(
                    history::SOURCE_CLAUDE,
                    &history::rows_from_claude(&self.tokens),
                );
                self.reload_source_history(history::SOURCE_CLAUDE);
            }
            Update::PiTokens(tokens) => {
                self.pi_tokens = tokens;
                self.clear_loading(ScanSource::Pi);
                history::record_source(history::SOURCE_PI, &history::rows_from_pi(&self.pi_tokens));
                self.reload_source_history(history::SOURCE_PI);
            }
            Update::OpenCodeTokens(tokens) => {
                self.opencode_tokens = match (self.opencode_tokens.clone(), tokens) {
                    (OpenCodePanelState::Ready(rows), OpenCodePanelState::Unavailable(reason))
                    | (
                        OpenCodePanelState::Stale { rows, .. },
                        OpenCodePanelState::Unavailable(reason),
                    ) => OpenCodePanelState::Stale { rows, reason },
                    (_, state) => state,
                };
                self.clear_loading(ScanSource::OpenCode);
                // Solo se ingiere cuando hay datos reales (no en fallos: eso
                // sobrescribiría el día con cero y perdería lo ya registrado).
                if let OpenCodePanelState::Ready(rows) | OpenCodePanelState::Stale { rows, .. } =
                    &self.opencode_tokens
                {
                    history::record_source(
                        history::SOURCE_OPENCODE,
                        &history::rows_from_opencode(rows),
                    );
                }
                self.reload_source_history(history::SOURCE_OPENCODE);
            }
            Update::Backfilled => {
                self.backfill_progress = None;
                self.backfill_active = false;
                self.reload_history();
                if self.graph_open {
                    self.reload_graph();
                }
            }
            Update::BackfillProgress(progress) => self.backfill_progress = Some(progress),
            Update::Graph(view, data) => {
                // Ignora respuestas de una vista ya cambiada.
                if view == self.graph_view {
                    self.graph_data = Some(data);
                }
            }
            Update::ClearLoading { source, timed_out } => {
                // El deadline también queda disponible para diagnósticos; la
                // recuperación es idempotente en ambos casos.
                let _ = timed_out;
                self.clear_loading(source);
            }
        }
    }

    fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        self.refresh(false);
        if crate::config::get().stats.enabled {
            self.spawn_backfill();
            self.reload_history();
        }
        loop {
            self.clear_expired_loading(Instant::now());
            while let Ok(update) = self.rx.try_recv() {
                self.apply_update(update);
            }
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if is_interrupt_key(key) {
                            return Ok(());
                        } else if self.show_help {
                            self.show_help = false;
                        } else if self.settings_cursor.is_some() {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('o') | KeyCode::Esc => {
                                    self.settings_cursor = None
                                }
                                KeyCode::Up | KeyCode::Char('k') => self.settings_move(-1),
                                KeyCode::Down | KeyCode::Char('j') => self.settings_move(1),
                                KeyCode::Char(' ') | KeyCode::Enter => self.settings_apply(0),
                                KeyCode::Left | KeyCode::Char('h') => self.settings_apply(-1),
                                KeyCode::Right | KeyCode::Char('l') => self.settings_apply(1),
                                _ => {}
                            }
                        } else {
                            // El banner es informativo: cualquier interacción
                            // lo descarta; el worker puede publicar progreso nuevo.
                            self.backfill_progress = None;
                            let stats_on = crate::config::get().stats.enabled;
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                                KeyCode::Char('?') => self.show_help = true,
                                KeyCode::Char('r') => self.refresh(true),
                                KeyCode::Char('o') => self.settings_cursor = Some(1),
                                KeyCode::Char('j') | KeyCode::Down => {
                                    self.scroll_offset = self.scroll_offset.saturating_add(1)
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    self.scroll_offset = self.scroll_offset.saturating_sub(1)
                                }
                                KeyCode::Char('g') if stats_on => {
                                    self.graph_open = !self.graph_open;
                                    if self.graph_open {
                                        self.reload_graph();
                                    }
                                }
                                KeyCode::Char('v') | KeyCode::Left | KeyCode::Right
                                    if self.graph_open =>
                                {
                                    self.graph_view = self.graph_view.next();
                                    self.reload_graph();
                                }
                                KeyCode::Char('t') | KeyCode::Tab
                                    if stats_on && !self.graph_open =>
                                {
                                    self.period = self.period.next();
                                    self.reload_history();
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if !self.refreshing && self.last_refresh.elapsed() >= AUTO_REFRESH {
                self.refresh(false);
            }
        }
    }

    fn draw(&self, f: &mut Frame) {
        if f.area().width < 80 || f.area().height < 24 {
            f.render_widget(
                Paragraph::new(format!(
                    "terminal demasiado pequeño ({}×{}); mínimo recomendado: 80×24",
                    f.area().width,
                    f.area().height
                ))
                .alignment(Alignment::Center),
                f.area(),
            );
            if self.show_help {
                self.draw_help(f);
            }
            return;
        }
        let Some(status) = &self.status else {
            f.render_widget(
                Paragraph::new("cargando…")
                    .fg_dim()
                    .alignment(Alignment::Center),
                f.area(),
            );
            return;
        };

        let tui_config = &crate::config::get().tui;
        let providers = providers::select(&status.providers, &tui_config.providers);
        // Con la gráfica abierta, ocupa el sitio de los paneles de tokens.
        let panels: Vec<usize> = if self.graph_open {
            vec![]
        } else {
            (0..PANELS.len())
                .filter(|&idx| self.token_panel_shown(idx, tui_config))
                .collect()
        };

        let mut sections: Vec<(Option<&ProviderStatus>, Option<usize>, u16)> = providers
            .iter()
            .map(|provider| (Some(*provider), None, provider_height(provider)))
            .collect();
        sections.extend(
            panels
                .iter()
                .map(|idx| (None, Some(*idx), self.token_panel_height(*idx))),
        );
        let start = self.scroll_offset.min(sections.len().saturating_sub(1));
        let available = f.area().height.saturating_sub(2);
        let mut end = start;
        let mut used = 0u16;
        while end < sections.len() && (end == start || used + sections[end].2 <= available) {
            used = used.saturating_add(sections[end].2);
            end += 1;
        }
        let visible = &sections[start..end];
        let mut constraints = vec![Constraint::Length(1)]; // cabecera
        for (_, _, height) in visible {
            constraints.push(Constraint::Length(*height));
        }
        constraints.push(Constraint::Min(0)); // relleno (o la gráfica)
        constraints.push(Constraint::Length(1)); // pie
        let areas = Layout::vertical(constraints).split(f.area());

        self.draw_header(f, areas[0]);
        for (i, (provider, panel, _)) in visible.iter().enumerate() {
            if let Some(provider) = provider {
                draw_provider(f, areas[i + 1], provider);
            } else if let Some(panel) = panel {
                self.draw_token_panel(f, areas[i + 1], *panel);
            }
        }
        if self.graph_open {
            // El penúltimo área es el relleno Min(0): ahí va la gráfica.
            self.draw_graph(f, areas[areas.len() - 2]);
        }
        self.draw_footer(f, areas[areas.len() - 1], status);
        if self.settings_cursor.is_some() {
            self.draw_settings(f);
        }
        if self.show_help {
            self.draw_help(f);
        }
    }

    fn draw_help(&self, f: &mut Frame) {
        let width = 54.min(f.area().width.saturating_sub(2));
        let height = 10.min(f.area().height.saturating_sub(2));
        let area = Rect {
            x: (f.area().width - width) / 2,
            y: (f.area().height - height) / 2,
            width,
            height,
        };
        f.render_widget(Clear, area);
        let lines = vec![
            Line::from("q / Esc  salir"),
            Line::from("r        refrescar"),
            Line::from("o        opciones"),
            Line::from("j / k    desplazar paneles"),
            Line::from("t / Tab  periodo de tokens"),
            Line::from("g / v    gráfica / vista"),
            Line::from("?        cerrar ayuda"),
            Line::from("Estados: [✓] listo · [!] aviso · [✗] crítico"),
        ];
        let lines = layout_with_scroll(
            Rect {
                height: area.height.saturating_sub(2),
                ..area
            },
            &lines,
        );
        f.render_widget(
            Paragraph::new(lines).block(bordered(" ayuda ").padding(Padding::horizontal(1))),
            area,
        );
    }

    fn draw_graph(&self, f: &mut Frame, area: Rect) {
        // Suma de las tres fuentes (Claude + Pi + OpenCode): no es solo Claude.
        let title = format!(" tokens totales · {} ", self.graph_view.label());
        let block = bordered(title).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let Some(data) = &self.graph_data else {
            f.render_widget(Paragraph::new("cargando…").fg_dim(), inner);
            return;
        };
        if data.values.iter().all(|&v| v == 0) {
            f.render_widget(Paragraph::new("sin gasto en este periodo").fg_dim(), inner);
            return;
        }
        if inner.height < 2 {
            return;
        }
        let [chart, labels] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        draw_braille_bars(f, chart, &data.values);
        draw_graph_labels(f, labels, &data.labels);
    }

    fn draw_settings(&self, f: &mut Frame) {
        let items = settings_items();
        let config = crate::config::get();
        let cursor = self.settings_cursor.unwrap_or(1);

        let extra = if self.settings_error.is_some() { 1 } else { 0 };
        let height = (items.len() as u16 + 2 + extra).min(f.area().height.saturating_sub(2));
        let width = 46.min(f.area().width.saturating_sub(2));
        let area = Rect {
            x: (f.area().width.saturating_sub(width)) / 2,
            y: (f.area().height.saturating_sub(height)) / 2,
            width,
            height,
        };
        f.render_widget(Clear, area);

        // Ventana visible con scroll alrededor del cursor si no cabe todo.
        let inner_rows = (height - 2 - extra) as usize;
        let offset = cursor.saturating_sub(inner_rows.saturating_sub(1));
        let mut lines: Vec<Line> = Vec::new();
        for (i, item) in items.iter().enumerate().skip(offset).take(inner_rows) {
            let selected = i == cursor;
            let line = match item {
                Setting::Section(name) => Line::from(Span::styled(
                    format!("── {name} "),
                    Style::new().fg(dim_color()),
                )),
                _ => {
                    let (label, val) = setting_row(item, &config);
                    let style = if selected {
                        Style::new().fg(accent_color()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new()
                    };
                    Line::from(Span::styled(format!(" {val} {label}"), style))
                }
            };
            lines.push(line);
        }
        if let Some(err) = &self.settings_error {
            lines.push(Line::from(Span::styled(
                format!(" ⚠ {err}"),
                Style::new().fg(if should_use_color() {
                    Color::Red
                } else {
                    Color::Reset
                }),
            )));
        }

        let block = bordered(" opciones ").title_bottom(Span::styled(
            " ␣ cambiar · ←→ ajustar · o cerrar ",
            Style::new().fg(dim_color()),
        ));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " lazysubs-eye ",
                    Style::new().fg(accent_color()).add_modifier(Modifier::BOLD),
                ),
                Span::styled("· cuotas de IA", Style::new().fg(dim_color())),
            ])),
            area,
        );
    }

    /// Un panel de tokens se muestra si su panel está activo. Vacío ⇒ "sin uso
    /// hoy" (los tres paneles se comportan igual, para que Pi/Claude no
    /// desaparezcan en días sin uso).
    fn token_panel_shown(&self, idx: usize, tui: &crate::config::Tui) -> bool {
        tui.panel(PANEL_IDS[idx])
    }

    /// Nº de filas de cuerpo (cabecera + datos, o una línea de mensaje si vacío).
    fn body_lines(&self, idx: usize) -> u16 {
        if self.period != Period::Today {
            let rows = self
                .history_rows
                .get(PANEL_SOURCES[idx])
                .map(|r| r.len())
                .unwrap_or(0);
            return 1 + rows.max(1) as u16; // cabecera + al menos una fila/mensaje
        }
        match idx {
            0 if self.tokens.is_empty() => 1,
            0 => 1 + self.tokens.len() as u16,
            1 if self.pi_tokens.is_empty() => 1,
            1 => 1 + self.pi_tokens.len() as u16,
            _ => match &self.opencode_tokens {
                OpenCodePanelState::Ready(rows) | OpenCodePanelState::Stale { rows, .. } => {
                    1 + rows.len() as u16
                }
                _ => 1, // mensaje de estado, sin cabecera
            },
        }
    }

    /// Serie del sparkline si hay datos con algún valor positivo.
    fn panel_spark(&self, idx: usize) -> Option<&Vec<u64>> {
        self.history_spark
            .get(PANEL_SOURCES[idx])
            .filter(|s| s.iter().any(|&v| v > 0))
    }

    fn token_panel_height(&self, idx: usize) -> u16 {
        let spark = u16::from(self.panel_spark(idx).is_some());
        self.body_lines(idx) + 2 + spark
    }

    fn panel_title(&self, idx: usize) -> String {
        // Doble espacio tras el ✳ porque en muchas fuentes es un glifo ancho.
        let base = match idx {
            0 => format!("{}  Claude Code", ascii_icon("✳")),
            1 => "Pi".into(),
            _ => "OpenCode".into(),
        };
        // Nota de datos rancios solo en el panel OpenCode de hoy.
        if idx == 2 && self.period == Period::Today {
            if let OpenCodePanelState::Stale { reason, .. } = &self.opencode_tokens {
                return format!(" {base} hoy · {} ", unavailable_text(*reason));
            }
        }
        format!(" {base} {} ", self.period.label())
    }

    fn draw_token_panel(&self, f: &mut Frame, area: Rect, idx: usize) {
        let block = bordered(self.panel_title(idx)).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let (body, spark_area) = match self.panel_spark(idx) {
            Some(_) => {
                let [b, s] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
                (b, Some(s))
            }
            None => (inner, None),
        };

        if self.period == Period::Today {
            self.draw_today_body(f, body, idx);
        } else {
            self.draw_history_body(f, body, PANEL_SOURCES[idx]);
        }

        if let (Some(area), Some(series)) = (spark_area, self.panel_spark(idx)) {
            f.render_widget(
                Sparkline::default()
                    .data(series.as_slice())
                    .style(
                        Style::new().fg(if crate::config::get().colors && should_use_color() {
                            ACCENT
                        } else {
                            Color::Reset
                        }),
                    ),
                area,
            );
        }
    }

    fn draw_today_body(&self, f: &mut Frame, area: Rect, idx: usize) {
        let _state = self.token_panel_state(idx);
        // Vacío hoy → mensaje (como OpenCode), para que el panel no desaparezca.
        if (idx == 0 && self.tokens.is_empty()) || (idx == 1 && self.pi_tokens.is_empty()) {
            f.render_widget(
                Paragraph::new("sin uso hoy").style(Style::new().fg(dim_color())),
                area,
            );
            return;
        }
        match idx {
            0 => {
                let header = Row::new(vec!["modelo", "req", "in", "out", "cache→", "cache+"])
                    .style(Style::new().fg(dim_color()).add_modifier(Modifier::BOLD));
                let rows: Vec<Row> = self
                    .tokens
                    .iter()
                    .map(|m| {
                        Row::new(vec![
                            m.model.clone(),
                            fmt_count(m.requests),
                            fmt_count(m.input),
                            fmt_count(m.output),
                            fmt_count(m.cache_read),
                            fmt_count(m.cache_creation),
                        ])
                    })
                    .collect();
                let widths = [
                    Constraint::Fill(1),
                    Constraint::Length(6),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                ];
                f.render_widget(Table::new(rows, widths).header(header), area);
            }
            1 => {
                let header = Row::new(vec![
                    "provider", "modelo", "in", "out", "cache→", "cache+", "total", "coste",
                ])
                .style(Style::new().fg(dim_color()).add_modifier(Modifier::BOLD));
                let rows: Vec<Row> = self
                    .pi_tokens
                    .iter()
                    .map(|row| {
                        Row::new(vec![
                            row.provider.clone(),
                            row.model.clone(),
                            fmt_count(row.totals.input),
                            fmt_count(row.totals.output),
                            fmt_count(row.totals.cache_read),
                            fmt_count(row.totals.cache_write),
                            fmt_count(row.totals.total_tokens),
                            fmt_cost(row.totals.cost_total),
                        ])
                    })
                    .collect();
                f.render_widget(Table::new(rows, pi_widths()).header(header), area);
            }
            _ => {
                let message = match &self.opencode_tokens {
                    OpenCodePanelState::Ready(_) | OpenCodePanelState::Stale { .. } => None,
                    OpenCodePanelState::Loading => Some("leyendo OpenCode…"),
                    OpenCodePanelState::Empty => Some("sin uso hoy"),
                    OpenCodePanelState::Unavailable(reason) => Some(unavailable_text(*reason)),
                };
                if let Some(message) = message {
                    f.render_widget(
                        Paragraph::new(message).style(Style::new().fg(dim_color())),
                        area,
                    );
                    return;
                }
                let header = Row::new(vec![
                    "provider", "modelo", "in", "out", "raz", "cache→", "cache+", "total", "coste",
                ])
                .style(Style::new().fg(dim_color()).add_modifier(Modifier::BOLD));
                let rows: Vec<Row> = match &self.opencode_tokens {
                    OpenCodePanelState::Ready(rows) | OpenCodePanelState::Stale { rows, .. } => {
                        rows.iter().map(opencode_table_row).collect()
                    }
                    _ => unreachable!("estados sin datos ya renderizados como párrafo"),
                };
                f.render_widget(Table::new(rows, opencode_widths()).header(header), area);
            }
        }
    }

    /// Tabla agregada del historial (semana/mes) para una fuente: una fila por
    /// (provider, modelo) con totales y coste del periodo.
    fn draw_history_body(&self, f: &mut Frame, area: Rect, source: &'static str) {
        let _state = self.history_panel_state(source);
        let rows = self.history_rows.get(source);
        if rows.map(|r| r.is_empty()).unwrap_or(true) {
            f.render_widget(
                Paragraph::new("sin datos del periodo").style(Style::new().fg(dim_color())),
                area,
            );
            return;
        }
        let header = Row::new(vec![
            "provider", "modelo", "in", "out", "cache", "total", "coste",
        ])
        .style(Style::new().fg(dim_color()).add_modifier(Modifier::BOLD));
        let table_rows: Vec<Row> = rows
            .unwrap()
            .iter()
            .map(|r| {
                Row::new(vec![
                    r.provider.clone(),
                    r.model.clone(),
                    fmt_count(r.input),
                    fmt_count(r.output),
                    fmt_count(r.cache_read + r.cache_write),
                    fmt_count(r.total),
                    if r.cost > 0.0 {
                        fmt_cost(r.cost)
                    } else {
                        "—".into()
                    },
                ])
            })
            .collect();
        let widths = [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(9),
        ];
        f.render_widget(Table::new(table_rows, widths).header(header), area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, status: &Status) {
        let [left, right] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).areas(area);
        let mut spans = vec![
            Span::styled(" q ", Style::new().fg(accent_color())),
            Span::styled("salir  ", Style::new().fg(dim_color())),
            Span::styled("r ", Style::new().fg(accent_color())),
            Span::styled("refrescar  ", Style::new().fg(dim_color())),
            Span::styled("o ", Style::new().fg(accent_color())),
            Span::styled("opciones", Style::new().fg(dim_color())),
            Span::styled("  j/k ", Style::new().fg(accent_color())),
            Span::styled("scroll", Style::new().fg(dim_color())),
        ];
        if crate::config::get().stats.enabled {
            if self.graph_open {
                spans.push(Span::styled("  g ", Style::new().fg(accent_color())));
                spans.push(Span::styled("cerrar  ", Style::new().fg(dim_color())));
                spans.push(Span::styled("v ", Style::new().fg(accent_color())));
                spans.push(Span::styled(
                    format!("vista · {}", self.graph_view.label()),
                    Style::new().fg(dim_color()),
                ));
            } else {
                spans.push(Span::styled("  t ", Style::new().fg(accent_color())));
                spans.push(Span::styled(
                    format!("periodo · {}  ", self.period.label()),
                    Style::new().fg(dim_color()),
                ));
                spans.push(Span::styled("g ", Style::new().fg(accent_color())));
                spans.push(Span::styled("gráfica", Style::new().fg(dim_color())));
            }
        }
        f.render_widget(Paragraph::new(Line::from(spans)), left);
        let state = if let Some(progress) = &self.backfill_progress {
            format!(
                "backfill {}/{} · {} fallo(s)",
                progress.completed_days, progress.total_days, progress.failed_days
            )
        } else if self.refreshing {
            "actualizando…".to_string()
        } else {
            let age = chrono::Utc::now().timestamp() - status.fetched_at;
            format!("hace {age}s ")
        };
        f.render_widget(
            Paragraph::new(state)
                .style(Style::new().fg(dim_color()))
                .alignment(Alignment::Right),
            right,
        );
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.backfill_cancelled.store(true, Ordering::Release);
    }
}

fn pi_widths() -> [Constraint; 8] {
    [
        Constraint::Length(10),
        Constraint::Fill(1),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(9),
    ]
}

fn opencode_widths() -> [Constraint; 9] {
    [
        Constraint::Length(10),
        Constraint::Fill(1),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(9),
    ]
}

fn opencode_table_row(row: &OpenCodeUsageRow) -> Row<'static> {
    Row::new(vec![
        row.provider.clone(),
        row.model.clone(),
        fmt_optional_count(row.input),
        fmt_optional_count(row.output),
        fmt_optional_count(row.reasoning),
        fmt_optional_count(row.cache_read),
        fmt_optional_count(row.cache_write),
        fmt_optional_count(row.total),
        row.cost.map(fmt_cost).unwrap_or_else(|| "—".into()),
    ])
}

fn fmt_optional_count(value: Option<u64>) -> String {
    value.map(fmt_count).unwrap_or_else(|| "—".into())
}

fn unavailable_text(reason: OpenCodeUnavailableReason) -> &'static str {
    match reason {
        OpenCodeUnavailableReason::Missing => "OpenCode no disponible",
        OpenCodeUnavailableReason::PermissionDenied => "sin permisos para OpenCode",
        OpenCodeUnavailableReason::Busy => "OpenCode ocupado temporalmente",
        OpenCodeUnavailableReason::EphemeralDatabase => "base efímera no disponible",
        OpenCodeUnavailableReason::SchemaIncompatible => "esquema OpenCode incompatible",
        OpenCodeUnavailableReason::InvalidUsage => "uso OpenCode inválido",
        OpenCodeUnavailableReason::CacheWriteFailed => "no se pudo guardar la caché OpenCode",
        OpenCodeUnavailableReason::ReadFailed => "lectura OpenCode fallida",
    }
}

fn fmt_cost(cost: f64) -> String {
    let value = format!("{cost:.4}");
    value.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Barras verticales en braille (estilo btop) de una serie de totales.
fn draw_braille_bars(f: &mut Frame, area: Rect, values: &[u64]) {
    let n = values.len();
    if n == 0 {
        return;
    }
    let max = values.iter().copied().max().unwrap_or(1).max(1) as f64;
    let color = if crate::config::get().colors && should_use_color() {
        ACCENT
    } else {
        Color::Reset
    };
    // Ancho de un punto braille en unidades de x, para rellenar cada barra con
    // columnas contiguas (barras llenas estilo btop en vez de una línea fina).
    let dot_dx = (n as f64 / (area.width.max(1) as f64 * 2.0)).max(f64::EPSILON);
    let canvas = Canvas::default()
        .marker(symbols::Marker::Braille)
        .x_bounds([0.0, n as f64])
        .y_bounds([0.0, max])
        .paint(move |ctx| {
            for (i, &v) in values.iter().enumerate() {
                if v == 0 {
                    continue;
                }
                // La barra ocupa el 70% central del bucket.
                let mut x = i as f64 + 0.15;
                let end = i as f64 + 0.85;
                while x <= end {
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: 0.0,
                        x2: x,
                        y2: v as f64,
                        color,
                    });
                    x += dot_dx;
                }
            }
        });
    f.render_widget(canvas, area);
}

/// Fila de etiquetas del eje x, centradas bajo cada barra y sin solaparse
/// (así las vistas densas —mes, horas— muestran solo las que caben).
fn draw_graph_labels(f: &mut Frame, area: Rect, labels: &[String]) {
    let w = area.width as usize;
    let n = labels.len();
    if w == 0 || n == 0 {
        return;
    }
    let mut buf = vec![' '; w];
    let mut last_end = 0usize;
    for (i, label) in labels.iter().enumerate() {
        let len = label.chars().count();
        let center = ((i as f64 + 0.5) * w as f64 / n as f64) as usize;
        let start = center.saturating_sub(len / 2).min(w.saturating_sub(len));
        if start >= last_end && start + len <= w {
            for (k, ch) in label.chars().enumerate() {
                buf[start + k] = ch;
            }
            last_end = start + len + 1; // un espacio de separación mínimo
        }
    }
    let text: String = buf.into_iter().collect();
    f.render_widget(
        Paragraph::new(text).style(Style::new().fg(dim_color())),
        area,
    );
}

/// Recorta cuentas largas (emails) para que quepan en el título del panel.
fn truncate_account(account: &str) -> String {
    const MAX: usize = 22;
    let chars: Vec<char> = account.chars().collect();
    if chars.len() <= MAX {
        return account.to_string();
    }
    let head: String = chars.into_iter().take(MAX - 1).collect();
    format!("{head}…")
}

fn codex_reset_credits_line(p: &ProviderStatus) -> Option<String> {
    (p.id == "codex" && p.error.is_none())
        .then_some(p.reset_credits_available)
        .flatten()
        .map(|credits| format!("Créditos de reinicio disponibles: {credits}"))
}

fn provider_height(p: &ProviderStatus) -> u16 {
    p.windows.len().max(1) as u16 + 2 + u16::from(codex_reset_credits_line(p).is_some())
}

fn bordered<'a>(title: impl Into<std::borrow::Cow<'a, str>>) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(dim_color()))
        .title(Span::styled(
            title,
            Style::new().fg(accent_color()).add_modifier(Modifier::BOLD),
        ))
}

/// Etiqueta y valor pintable de un ajuste ("[x]" para toggles, el número
/// para los ajustables con ←/→).
fn setting_row(item: &Setting, config: &crate::config::Config) -> (String, String) {
    let check = |on: bool| if on { "[x]" } else { "[ ]" }.to_string();
    match item {
        Setting::Section(_) => (String::new(), String::new()),
        Setting::Notifications => ("notificaciones".into(), check(config.notifications)),
        Setting::Cooldown => (
            "cooldown avisos (min)".into(),
            format!("◂{:>4}▸", config.notification_cooldown / 60),
        ),
        Setting::Colors => ("colores de umbral".into(), check(config.colors)),
        Setting::ShowAccount => ("mostrar cuenta".into(), check(config.show_account)),
        Setting::WarningAt => (
            "umbral warning".into(),
            format!("◂{:>3.0}%▸", config.warning_at),
        ),
        Setting::CriticalAt => (
            "umbral critical".into(),
            format!("◂{:>3.0}%▸", config.critical_at),
        ),
        Setting::Ttl => ("cache ttl (s)".into(), format!("◂{:>4}▸", config.ttl)),
        Setting::Provider(i) => {
            let on = match PROVIDER_IDS[*i] {
                "claude" => config.providers.claude,
                "codex" => config.providers.codex,
                _ => config.providers.minimax,
            };
            (PROVIDERS[*i].1.into(), check(on))
        }
        Setting::WaybarPercent => (
            "porcentaje en la barra".into(),
            check(config.waybar.percent()),
        ),
        Setting::WaybarProvider(i) => {
            let surface = surface_providers();
            match surface.get(*i) {
                Some((id, name)) => (name.clone(), check(in_list(&config.waybar.providers, id))),
                None => (String::new(), String::new()),
            }
        }
        Setting::WaybarWindow(i) => {
            let surface = surface_providers();
            match surface.get(*i) {
                Some((id, name)) => {
                    let sel = config
                        .waybar
                        .window
                        .as_ref()
                        .and_then(|m| m.get(id))
                        .cloned()
                        .unwrap_or_else(|| "auto".into());
                    (name.clone(), format!("◂{:^14}▸", truncate_account(&sel)))
                }
                None => (String::new(), String::new()),
            }
        }
        Setting::TuiProvider(i) => {
            let surface = surface_providers();
            match surface.get(*i) {
                Some((id, name)) => (name.clone(), check(in_list(&config.tui.providers, id))),
                None => (String::new(), String::new()),
            }
        }
        Setting::TuiPanel(i) => (
            PANELS[*i].1.into(),
            check(in_list(&config.tui.panels, PANEL_IDS[*i])),
        ),
        Setting::StatsEnabled => ("historial de gasto".into(), check(config.stats.enabled)),
        Setting::StatsPeriod => (
            "periodo inicial".into(),
            format!("◂{:>6}▸", config.stats.default_period),
        ),
        Setting::StatsHistoryDays => (
            "retención (días)".into(),
            if config.stats.history_days == 0 {
                "◂   ∞▸".into()
            } else {
                format!("◂{:>4}▸", config.stats.history_days)
            },
        ),
        Setting::StatsSparkline => ("sparkline".into(), check(config.stats.sparkline)),
    }
}

fn percent_color(pct: f64) -> Color {
    let config = crate::config::get();
    if !config.colors || !should_use_color() {
        return Color::Reset; // color del terminal, sin semáforo
    }
    if pct >= config.critical_at {
        Color::Red
    } else if pct >= config.warning_at {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn quota_marker(pct: f64) -> &'static str {
    let config = crate::config::get();
    let state = if pct >= config.critical_at {
        PanelState::Unavailable
    } else if pct >= config.warning_at {
        PanelState::Partial
    } else {
        PanelState::Ready
    };
    state_marker(state, supports_utf8())
}

fn draw_provider(f: &mut Frame, area: Rect, p: &ProviderStatus) {
    let plan = p.plan.as_deref().unwrap_or("?");
    let mut bits = vec![plan.to_string()];
    if crate::config::get().show_account {
        if let Some(account) = &p.account {
            bits.push(truncate_account(account));
        }
    }
    if let Some(since) = p.stale_since {
        bits.push(format!("datos de hace {}", crate::output::age(since)));
    }
    let plan_title = format!(" {} ", bits.join(" · "));
    // Doble espacio tras el icono: glifos como ✳ son anchos en muchas fuentes.
    let block = bordered(format!(" {}  {} ", ascii_icon(&p.icon), p.name))
        .title(Span::styled(plan_title, Style::new().fg(dim_color())));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(err) = &p.error {
        f.render_widget(
            Paragraph::new(err.as_str()).style(Style::new().fg(if should_use_color() {
                Color::Red
            } else {
                Color::Reset
            })),
            inner,
        );
        return;
    }

    let reset_credits_line = codex_reset_credits_line(p);
    let window_rows = p.windows.len().max(1);
    let mut constraints = vec![Constraint::Length(1); window_rows];
    if reset_credits_line.is_some() {
        constraints.push(Constraint::Length(1));
    }
    let rows = Layout::vertical(constraints).split(inner);
    for (row, w) in rows.iter().take(window_rows).zip(&p.windows) {
        let [label_a, gauge_a, reset_a] = Layout::horizontal([
            Constraint::Length(18),
            Constraint::Min(10),
            Constraint::Length(12),
        ])
        .areas(*row);

        let label_style = if w.active {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(dim_color())
        };
        f.render_widget(
            Paragraph::new(format!(" {}", w.label)).style(label_style),
            label_a,
        );

        f.render_widget(
            LineGauge::default()
                .ratio((w.used_percent / 100.0).clamp(0.0, 1.0))
                .label(format!(
                    "{} {:>3.0}%",
                    quota_marker(w.used_percent),
                    w.used_percent
                ))
                .filled_style(Style::new().fg(percent_color(w.used_percent)))
                .unfilled_style(Style::new().fg(dim_color()))
                .line_set(symbols::line::THICK),
            gauge_a,
        );

        let reset = w
            .resets_at
            .map(|t| format!("→ {} ", countdown(t)))
            .unwrap_or_default();
        f.render_widget(
            Paragraph::new(reset)
                .style(Style::new().fg(dim_color()))
                .alignment(Alignment::Right),
            reset_a,
        );
    }

    if let Some(line) = reset_credits_line {
        f.render_widget(
            Paragraph::new(line).style(Style::new().fg(dim_color())),
            rows[window_rows],
        );
    }
}

trait ParagraphExt<'a> {
    fn fg_dim(self) -> Paragraph<'a>;
}
impl<'a> ParagraphExt<'a> for Paragraph<'a> {
    fn fg_dim(self) -> Paragraph<'a> {
        self.style(Style::new().fg(dim_color()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_tokens::{PiUsageRow, PiUsageTotals};
    use crate::providers::Window;

    fn provider(id: &str, credits: Option<u64>, error: Option<&str>) -> ProviderStatus {
        ProviderStatus {
            id: id.into(),
            name: "Provider".into(),
            icon: "*".into(),
            plan: None,
            account: None,
            windows: vec![
                Window {
                    label: "5h".into(),
                    used_percent: 10.0,
                    resets_at: None,
                    active: true,
                },
                Window {
                    label: "semana".into(),
                    used_percent: 20.0,
                    resets_at: None,
                    active: true,
                },
            ],
            reset_credits_available: credits,
            stale_since: None,
            error: error.map(str::to_owned),
        }
    }

    #[test]
    fn status_updates_are_applied_while_tokens_are_scanning() {
        let mut app = App::new();
        app.refreshing = true;
        app.tokens_scanning = true;
        let status = Status {
            fetched_at: 1,
            providers: vec![],
        };
        app.tx.send(Update::Status(status)).unwrap();
        while let Ok(update) = app.rx.try_recv() {
            app.apply_update(update);
        }
        assert!(app.status.is_some());
        assert!(app.tokens_scanning);
        assert!(!app.refreshing);
    }

    #[test]
    fn opencode_state_is_independent_and_suppresses_duplicate_scans() {
        let mut app = App::new();
        assert!(app.begin_opencode_token_scan());
        assert!(!app.begin_opencode_token_scan());
        app.apply_update(Update::OpenCodeTokens(
            crate::opencode_tokens::OpenCodePanelState::Empty,
        ));
        assert!(!app.opencode_scanning);
        app.opencode_scanning = true;
        app.apply_update(Update::Status(Status {
            fetched_at: 1,
            providers: vec![],
        }));
        assert!(app.status.is_some());
        assert!(app.opencode_scanning);
    }

    #[test]
    fn opencode_failure_keeps_previous_rows_as_stale() {
        let mut app = App::new();
        let rows = vec![OpenCodeUsageRow {
            provider: "openai".into(),
            model: "gpt-test".into(),
            input: Some(1),
            output: Some(2),
            reasoning: Some(3),
            cache_read: Some(4),
            cache_write: Some(5),
            total: Some(15),
            cost: Some(0.01),
        }];
        app.apply_update(Update::OpenCodeTokens(OpenCodePanelState::Ready(
            rows.clone(),
        )));
        app.apply_update(Update::OpenCodeTokens(OpenCodePanelState::Unavailable(
            OpenCodeUnavailableReason::Busy,
        )));
        assert_eq!(
            app.opencode_tokens,
            OpenCodePanelState::Stale {
                rows,
                reason: OpenCodeUnavailableReason::Busy,
            }
        );
    }

    #[test]
    fn pi_state_independence() {
        let mut app = App::new();
        assert!(app.begin_pi_token_scan());
        assert!(!app.begin_pi_token_scan());
        app.pi_tokens_scanning = true;
        app.tx
            .send(Update::PiTokens(vec![PiUsageRow {
                provider: "p".into(),
                model: "m".into(),
                totals: PiUsageTotals::default(),
            }]))
            .unwrap();
        while let Ok(update) = app.rx.try_recv() {
            app.apply_update(update);
        }
        assert!(!app.pi_tokens_scanning);
        assert_eq!(app.pi_tokens.len(), 1);
    }

    #[test]
    fn prevents_duplicate_token_scans_while_one_is_active() {
        let mut app = App::new();
        assert!(app.begin_token_scan());
        assert!(!app.begin_token_scan());
    }

    #[test]
    fn status_updates_apply_while_pi_scan_is_active() {
        let mut app = App::new();
        app.pi_tokens_scanning = true;
        app.apply_update(Update::Status(Status {
            fetched_at: 1,
            providers: vec![],
        }));
        assert!(app.status.is_some());
        assert!(app.pi_tokens_scanning);
    }

    #[test]
    fn backfill_progress_is_observable_without_waiting_for_completion() {
        let mut app = App::new();
        app.apply_update(Update::BackfillProgress(history::BackfillProgress {
            current_day: Some("2026-07-14".into()),
            completed_days: 4,
            total_days: 12,
            failed_days: 1,
        }));
        assert_eq!(app.backfill_progress.as_ref().unwrap().completed_days, 4);
    }

    #[test]
    fn dropping_app_requests_cooperative_backfill_cancellation() {
        let app = App::new();
        let cancelled = Arc::clone(&app.backfill_cancelled);
        assert!(!cancelled.load(Ordering::Acquire));
        drop(app);
        assert!(cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn panel_state_serializa_y_cubre_los_paneles() {
        assert_eq!(
            serde_json::to_string(&PanelState::Stale).unwrap(),
            "\"Stale\""
        );
        let mut app = App::new();
        assert_eq!(app.token_panel_state(0), PanelState::Empty);
        app.tokens_scanning = true;
        assert_eq!(app.token_panel_state(0), PanelState::Loading);
        app.opencode_tokens = OpenCodePanelState::Unavailable(OpenCodeUnavailableReason::Busy);
        assert_eq!(app.token_panel_state(2), PanelState::Unavailable);
        assert_eq!(
            app.history_panel_state(history::SOURCE_PI),
            PanelState::Unavailable
        );
        app.history_rows.insert(history::SOURCE_PI, vec![]);
        assert_eq!(
            app.history_panel_state(history::SOURCE_PI),
            PanelState::Empty
        );
        app.history_states.insert(
            history::SOURCE_PI,
            history::IngestState::Failed {
                day: "2026-07-14".into(),
                attempted_at: 1,
                reason: "fallo saneado".into(),
            },
        );
        assert_eq!(
            app.history_panel_state(history::SOURCE_PI),
            PanelState::Unavailable
        );
        app.backfill_active = true;
        app.backfill_progress = Some(history::BackfillProgress {
            failed_days: 1,
            ..history::BackfillProgress::default()
        });
        assert_eq!(
            app.history_panel_state(history::SOURCE_PI),
            PanelState::Partial
        );
    }

    #[test]
    fn scroll_recorta_indica_y_respeta_offset_y_resize() {
        let content: Vec<Line> = (0..30).map(|n| Line::from(format!("línea {n}"))).collect();
        let first = layout_with_scroll(Rect::new(0, 0, 40, 10), &content);
        assert_eq!(first.len(), 10);
        assert_eq!(first[0].to_string(), "línea 0");
        assert!(first[9].to_string().contains("21 línea(s) más"));

        let end = layout_with_scroll_offset(Rect::new(0, 0, 40, 10), &content, 25);
        assert_eq!(end.len(), 5);
        assert_eq!(end[0].to_string(), "línea 25");

        let resized = layout_with_scroll_offset(Rect::new(0, 0, 40, 3), &content, 28);
        assert_eq!(resized.len(), 2);
        assert_eq!(resized[1].to_string(), "línea 29");
    }

    #[test]
    fn loading_guard_libera_en_drop_y_el_timeout_recupera_el_estado() {
        let mut app = App::new();
        assert!(app.begin_token_scan());
        {
            let guard =
                LoadingGuard::new(ScanSource::Claude, app.tx.clone(), Duration::from_millis(1));
            assert!(!guard.expired(Instant::now()));
        }
        app.apply_update(app.rx.recv().unwrap());
        assert!(!app.tokens_scanning);

        assert!(app.begin_pi_token_scan());
        app.scan_deadlines[1] = Some(Instant::now() - Duration::from_millis(1));
        app.clear_expired_loading(Instant::now());
        assert!(!app.pi_tokens_scanning);
    }

    #[test]
    fn color_utf8_y_estados_tienen_fallbacks_deterministas() {
        assert!(!should_use_color_for(None, Some("1"), Some("xterm")));
        assert!(!should_use_color_for(None, None, Some("dumb")));
        assert!(should_use_color_for(Some("1"), Some("1"), Some("dumb")));
        assert!(should_use_color_for(None, Some("0"), Some("xterm")));

        assert!(!supports_utf8_for(Some("C")));
        assert!(!supports_utf8_for(None));
        assert!(supports_utf8_for(Some("en_US.UTF-8")));
        assert!(supports_utf8_for(Some("UTF8")));
        assert_eq!(state_marker(PanelState::Ready, true), "[✓]");
        assert_eq!(state_marker(PanelState::Partial, false), "[!]");
        assert_eq!(state_marker(PanelState::Unavailable, true), "[✗]");
        assert_eq!(state_marker(PanelState::Unavailable, false), "[x]");
    }

    #[test]
    fn refresh_scheduler_coalesce_y_conserva_force_pendiente() {
        let scheduler = RefreshScheduler::default();
        assert!(scheduler.maybe_schedule_refresh(false));
        assert!(!scheduler.maybe_schedule_refresh(false));
        assert!(!scheduler.maybe_schedule_refresh(true));
        assert_eq!(scheduler.finished(), Some(true));
        assert!(scheduler.maybe_schedule_refresh(true));
        assert_eq!(scheduler.finished(), None);
    }

    #[test]
    fn ctrl_c_es_interrupcion_incluso_con_help_abierto() {
        assert!(is_interrupt_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_interrupt_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
    }

    #[test]
    fn terminal_restore_guard_ejecuta_drop_durante_unwind() {
        static RESTORED: AtomicBool = AtomicBool::new(false);
        fn restore_for_test() {
            RESTORED.store(true, Ordering::SeqCst);
        }
        RESTORED.store(false, Ordering::SeqCst);
        let _ = std::panic::catch_unwind(|| {
            let _guard = RestoreTerminal(restore_for_test);
            panic!("panic simulado");
        });
        assert!(RESTORED.load(Ordering::SeqCst));
    }

    #[test]
    fn fmt_cost_es_neutral_y_recorta_ceros() {
        assert_eq!(fmt_cost(0.0), "0");
        assert_eq!(fmt_cost(1234.56789), "1234.5679");
    }

    #[test]
    fn shows_reset_credits_for_healthy_codex_only() {
        let codex = provider("codex", Some(3), None);
        assert_eq!(
            codex_reset_credits_line(&codex).as_deref(),
            Some("Créditos de reinicio disponibles: 3")
        );

        let zero = provider("codex", Some(0), None);
        assert_eq!(
            codex_reset_credits_line(&zero).as_deref(),
            Some("Créditos de reinicio disponibles: 0")
        );
        assert_eq!(
            codex_reset_credits_line(&provider("codex", None, None)),
            None
        );
        assert_eq!(
            codex_reset_credits_line(&provider("claude", Some(3), None)),
            None
        );
        assert_eq!(
            codex_reset_credits_line(&provider("codex", Some(3), Some("falló"))),
            None
        );
    }

    #[test]
    fn provider_height_uses_the_same_reset_credit_condition() {
        assert_eq!(provider_height(&provider("codex", Some(3), None)), 5);
        assert_eq!(provider_height(&provider("codex", Some(0), None)), 5);
        assert_eq!(provider_height(&provider("codex", None, None)), 4);
        assert_eq!(provider_height(&provider("claude", Some(3), None)), 4);
        assert_eq!(
            provider_height(&provider("codex", Some(3), Some("falló"))),
            4
        );
    }
}
