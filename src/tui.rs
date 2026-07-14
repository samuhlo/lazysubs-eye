use crate::opencode_tokens::{
    self, OpenCodePanelState, OpenCodeUnavailableReason, OpenCodeUsageRow,
};
use crate::output::countdown;
use crate::pi_tokens::{self, PiUsageRow};
use crate::providers::{ProviderStatus, Status};
use crate::tokens::{self, fmt_count, ModelTokens};
use crate::{cache, providers};
use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, LineGauge, Padding, Paragraph, Row, Table};
use ratatui::Frame;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(60);

const ACCENT: Color = Color::Yellow;
const DIM: Color = Color::DarkGray;

pub fn run() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = App::new().run(&mut terminal);
    ratatui::restore();
    result
}

enum Update {
    Status(Status),
    Tokens(Vec<ModelTokens>),
    PiTokens(Vec<PiUsageRow>),
    OpenCodeTokens(OpenCodePanelState),
}

struct App {
    status: Option<Status>,
    tokens: Vec<ModelTokens>,
    pi_tokens: Vec<PiUsageRow>,
    opencode_tokens: OpenCodePanelState,
    refreshing: bool,
    tx: mpsc::Sender<Update>,
    rx: mpsc::Receiver<Update>,
    tokens_scanning: bool,
    pi_tokens_scanning: bool,
    opencode_scanning: bool,
    last_refresh: Instant,
    /// Cursor del panel de opciones; None = cerrado.
    settings_cursor: Option<usize>,
    settings_error: Option<String>,
}

/// Ítems del panel de opciones. Los índices apuntan a PROVIDERS / PANELS.
#[derive(Clone, Copy, PartialEq)]
enum Setting {
    Section(&'static str),
    Notifications,
    Colors,
    WarningAt,
    CriticalAt,
    Ttl,
    Provider(usize),
    WaybarPercent,
    WaybarProvider(usize),
    TuiProvider(usize),
    TuiPanel(usize),
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

fn settings_items() -> Vec<Setting> {
    let mut items = vec![
        Setting::Section("general"),
        Setting::Notifications,
        Setting::Colors,
        Setting::WarningAt,
        Setting::CriticalAt,
        Setting::Ttl,
        Setting::Section("providers"),
    ];
    items.extend((0..PROVIDERS.len()).map(Setting::Provider));
    items.push(Setting::Section("waybar"));
    items.push(Setting::WaybarPercent);
    items.extend((0..PROVIDERS.len()).map(Setting::WaybarProvider));
    items.push(Setting::Section("tui"));
    items.extend((0..PROVIDERS.len()).map(Setting::TuiProvider));
    items.extend((0..PANELS.len()).map(Setting::TuiPanel));
    items
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
        Self {
            status: None,
            tokens: vec![],
            pi_tokens: vec![],
            opencode_tokens: OpenCodePanelState::Loading,
            refreshing: false,
            tx,
            rx,
            tokens_scanning: false,
            pi_tokens_scanning: false,
            opencode_scanning: false,
            last_refresh: Instant::now(),
            settings_cursor: None,
            settings_error: None,
        }
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
            Setting::WarningAt if dir != 0 => {
                config.warning_at = (config.warning_at + 5.0 * dir as f64).clamp(5.0, 100.0)
            }
            Setting::CriticalAt if dir != 0 => {
                config.critical_at = (config.critical_at + 5.0 * dir as f64).clamp(5.0, 100.0)
            }
            Setting::Ttl if dir != 0 => config.ttl = (config.ttl + 30 * dir).clamp(10, 3600),
            Setting::WarningAt | Setting::CriticalAt | Setting::Ttl => return,
            Setting::Provider(i) => {
                let flag = match PROVIDER_IDS[i] {
                    "claude" => &mut config.providers.claude,
                    "codex" => &mut config.providers.codex,
                    _ => &mut config.providers.minimax,
                };
                *flag = !*flag;
            }
            Setting::WaybarPercent => config.waybar.percent = Some(!config.waybar.percent()),
            Setting::WaybarProvider(i) => crate::config::toggle_id(
                &mut config.waybar.providers,
                &PROVIDER_IDS,
                PROVIDER_IDS[i],
            ),
            Setting::TuiProvider(i) => {
                crate::config::toggle_id(&mut config.tui.providers, &PROVIDER_IDS, PROVIDER_IDS[i])
            }
            Setting::TuiPanel(i) => {
                crate::config::toggle_id(&mut config.tui.panels, &PANEL_IDS, PANEL_IDS[i])
            }
        }
        crate::config::set(config.clone());
        self.settings_error = crate::config::persist(&config)
            .err()
            .map(|e| format!("{e:#}"));
        // Un panel recién activado necesita su escaneo; refresh barato (cache).
        if matches!(items[cursor], Setting::TuiPanel(_)) {
            self.refresh(false);
        }
    }

    fn refresh(&mut self, force: bool) {
        if self.refreshing {
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
                let _ = tx.send(Update::Tokens(tokens::claude_today()));
            });
        }
        if panels.panel("pi_tokens") && self.begin_pi_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Update::PiTokens(pi_tokens::scan_pi_today()));
            });
        }
        if panels.panel("opencode_tokens") && self.begin_opencode_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
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
        true
    }

    fn begin_pi_token_scan(&mut self) -> bool {
        if self.pi_tokens_scanning {
            return false;
        }
        self.pi_tokens_scanning = true;
        true
    }

    fn begin_opencode_token_scan(&mut self) -> bool {
        if self.opencode_scanning {
            return false;
        }
        self.opencode_scanning = true;
        true
    }

    fn apply_update(&mut self, update: Update) {
        match update {
            Update::Status(status) => {
                self.status = Some(status);
                self.refreshing = false;
            }
            Update::Tokens(tokens) => {
                self.tokens = tokens;
                self.tokens_scanning = false;
            }
            Update::PiTokens(tokens) => {
                self.pi_tokens = tokens;
                self.pi_tokens_scanning = false;
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
                self.opencode_scanning = false;
            }
        }
    }

    fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        self.refresh(false);
        loop {
            while let Ok(update) = self.rx.try_recv() {
                self.apply_update(update);
            }
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if self.settings_cursor.is_some() {
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
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                                KeyCode::Char('r') => self.refresh(true),
                                KeyCode::Char('o') => self.settings_cursor = Some(1),
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
        let show_tokens = tui_config.panel("claude_tokens") && !self.tokens.is_empty();
        let show_pi = tui_config.panel("pi_tokens") && !self.pi_tokens.is_empty();
        let show_opencode = tui_config.panel("opencode_tokens");

        let mut constraints = vec![Constraint::Length(1)]; // cabecera
        for p in &providers {
            constraints.push(Constraint::Length(provider_height(p)));
        }
        if show_tokens {
            constraints.push(Constraint::Length(self.tokens.len() as u16 + 3));
        }
        if show_pi {
            constraints.push(Constraint::Length(pi_section_height(self.pi_tokens.len())));
        }
        if show_opencode {
            constraints.push(Constraint::Length(opencode_section_height(
                &self.opencode_tokens,
            )));
        }
        constraints.push(Constraint::Min(0)); // relleno
        constraints.push(Constraint::Length(1)); // pie
        let areas = Layout::vertical(constraints).split(f.area());

        self.draw_header(f, areas[0]);
        for (i, p) in providers.iter().enumerate() {
            draw_provider(f, areas[i + 1], p);
        }
        let mut section = providers.len() + 1;
        if show_tokens {
            self.draw_tokens(f, areas[section]);
            section += 1;
        }
        if show_pi {
            self.draw_pi_tokens(f, areas[section]);
            section += 1;
        }
        if show_opencode {
            self.draw_opencode_tokens(f, areas[section]);
        }
        self.draw_footer(f, areas[areas.len() - 1], status);
        if self.settings_cursor.is_some() {
            self.draw_settings(f);
        }
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
                Setting::Section(name) => {
                    Line::from(Span::styled(format!("── {name} "), Style::new().fg(DIM)))
                }
                _ => {
                    let (label, val) = setting_row(item, &config);
                    let style = if selected {
                        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
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
                Style::new().fg(Color::Red),
            )));
        }

        let block = bordered(" opciones ").title_bottom(Span::styled(
            " ␣ cambiar · ←→ ajustar · o cerrar ",
            Style::new().fg(DIM),
        ));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " lazysubs-eye ",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("· cuotas de IA", Style::new().fg(DIM)),
            ])),
            area,
        );
    }

    fn draw_tokens(&self, f: &mut Frame, area: Rect) {
        let header = Row::new(vec!["modelo", "req", "in", "out", "cache→", "cache+"])
            .style(Style::new().fg(DIM).add_modifier(Modifier::BOLD));
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
        f.render_widget(
            Table::new(rows, widths)
                .header(header)
                .block(bordered(" ✳ tokens hoy ").padding(Padding::horizontal(1))),
            area,
        );
    }

    fn draw_pi_tokens(&self, f: &mut Frame, area: Rect) {
        let header = Row::new(vec![
            "provider", "modelo", "in", "out", "cache→", "cache+", "total", "coste",
        ])
        .style(Style::new().fg(DIM).add_modifier(Modifier::BOLD));
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
        let widths = [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
        ];
        f.render_widget(
            Table::new(rows, widths)
                .header(header)
                .block(bordered(" Pi hoy ").padding(Padding::horizontal(1))),
            area,
        );
    }

    fn draw_opencode_tokens(&self, f: &mut Frame, area: Rect) {
        let header = Row::new(vec![
            "provider", "modelo", "in", "out", "raz", "cache→", "cache+", "total", "coste",
        ])
        .style(Style::new().fg(DIM).add_modifier(Modifier::BOLD));
        let title = match &self.opencode_tokens {
            OpenCodePanelState::Stale { reason, .. } => {
                format!(" OpenCode hoy · {} ", unavailable_text(*reason))
            }
            _ => " OpenCode hoy ".into(),
        };
        // Los estados sin datos van como párrafo, no como fila: las celdas de
        // la tabla truncan el texto al ancho de la primera columna.
        let message = match &self.opencode_tokens {
            OpenCodePanelState::Ready(_) | OpenCodePanelState::Stale { .. } => None,
            OpenCodePanelState::Loading => Some("leyendo OpenCode…"),
            OpenCodePanelState::Empty => Some("sin uso hoy"),
            OpenCodePanelState::Unavailable(reason) => Some(unavailable_text(*reason)),
        };
        if let Some(message) = message {
            f.render_widget(
                Paragraph::new(message)
                    .style(Style::new().fg(DIM))
                    .block(bordered(title).padding(Padding::horizontal(1))),
                area,
            );
            return;
        }
        let rows: Vec<Row> = match &self.opencode_tokens {
            OpenCodePanelState::Ready(rows) | OpenCodePanelState::Stale { rows, .. } => {
                rows.iter().map(opencode_table_row).collect()
            }
            _ => unreachable!("estados sin datos ya renderizados como párrafo"),
        };
        let widths = [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
        ];
        f.render_widget(
            Table::new(rows, widths)
                .header(header)
                .block(bordered(title).padding(Padding::horizontal(1))),
            area,
        );
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, status: &Status) {
        let [left, right] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).areas(area);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" q ", Style::new().fg(ACCENT)),
                Span::styled("salir  ", Style::new().fg(DIM)),
                Span::styled("r ", Style::new().fg(ACCENT)),
                Span::styled("refrescar  ", Style::new().fg(DIM)),
                Span::styled("o ", Style::new().fg(ACCENT)),
                Span::styled("opciones", Style::new().fg(DIM)),
            ])),
            left,
        );
        let state = if self.refreshing {
            "actualizando…".to_string()
        } else {
            let age = chrono::Utc::now().timestamp() - status.fetched_at;
            format!("hace {age}s ")
        };
        f.render_widget(
            Paragraph::new(state)
                .style(Style::new().fg(DIM))
                .alignment(Alignment::Right),
            right,
        );
    }
}

fn pi_section_height(rows: usize) -> u16 {
    rows.saturating_add(3).min(u16::MAX as usize) as u16
}

fn opencode_section_height(state: &OpenCodePanelState) -> u16 {
    match state {
        OpenCodePanelState::Ready(rows) | OpenCodePanelState::Stale { rows, .. } => {
            pi_section_height(rows.len())
        }
        _ => 4,
    }
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
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(
            title,
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
}

/// Etiqueta y valor pintable de un ajuste ("[x]" para toggles, el número
/// para los ajustables con ←/→).
fn setting_row(item: &Setting, config: &crate::config::Config) -> (String, String) {
    let check = |on: bool| if on { "[x]" } else { "[ ]" }.to_string();
    match item {
        Setting::Section(_) => (String::new(), String::new()),
        Setting::Notifications => ("notificaciones".into(), check(config.notifications)),
        Setting::Colors => ("colores de umbral".into(), check(config.colors)),
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
        Setting::WaybarProvider(i) => (
            PROVIDERS[*i].1.into(),
            check(in_list(&config.waybar.providers, PROVIDER_IDS[*i])),
        ),
        Setting::TuiProvider(i) => (
            PROVIDERS[*i].1.into(),
            check(in_list(&config.tui.providers, PROVIDER_IDS[*i])),
        ),
        Setting::TuiPanel(i) => (
            PANELS[*i].1.into(),
            check(in_list(&config.tui.panels, PANEL_IDS[*i])),
        ),
    }
}

fn percent_color(pct: f64) -> Color {
    let config = crate::config::get();
    if !config.colors {
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

fn draw_provider(f: &mut Frame, area: Rect, p: &ProviderStatus) {
    let plan = p.plan.as_deref().unwrap_or("?");
    let plan_title = match p.stale_since {
        Some(since) => format!(" {plan} · datos de hace {} ", crate::output::age(since)),
        None => format!(" {plan} "),
    };
    let block = bordered(format!(" {} {} ", p.icon, p.name))
        .title(Span::styled(plan_title, Style::new().fg(DIM)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(err) = &p.error {
        f.render_widget(
            Paragraph::new(err.as_str()).style(Style::new().fg(Color::Red)),
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
            Style::new().fg(DIM)
        };
        f.render_widget(
            Paragraph::new(format!(" {}", w.label)).style(label_style),
            label_a,
        );

        f.render_widget(
            LineGauge::default()
                .ratio((w.used_percent / 100.0).clamp(0.0, 1.0))
                .label(format!("{:>3.0}%", w.used_percent))
                .filled_style(Style::new().fg(percent_color(w.used_percent)))
                .unfilled_style(Style::new().fg(DIM))
                .line_set(symbols::line::THICK),
            gauge_a,
        );

        let reset = w
            .resets_at
            .map(|t| format!("→ {} ", countdown(t)))
            .unwrap_or_default();
        f.render_widget(
            Paragraph::new(reset)
                .style(Style::new().fg(DIM))
                .alignment(Alignment::Right),
            reset_a,
        );
    }

    if let Some(line) = reset_credits_line {
        f.render_widget(
            Paragraph::new(line).style(Style::new().fg(DIM)),
            rows[window_rows],
        );
    }
}

trait ParagraphExt<'a> {
    fn fg_dim(self) -> Paragraph<'a>;
}
impl<'a> ParagraphExt<'a> for Paragraph<'a> {
    fn fg_dim(self) -> Paragraph<'a> {
        self.style(Style::new().fg(DIM))
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
    fn pi_render_helpers_keep_cost_neutral_and_height_independent() {
        assert_eq!(pi_section_height(2), 5);
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
