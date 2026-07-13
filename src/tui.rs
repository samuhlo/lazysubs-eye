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
use ratatui::widgets::{Block, BorderType, LineGauge, Padding, Paragraph, Row, Table};
use ratatui::Frame;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(60);
const CACHE_TTL_SECS: i64 = 60;

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
}

struct App {
    status: Option<Status>,
    tokens: Vec<ModelTokens>,
    pi_tokens: Vec<PiUsageRow>,
    refreshing: bool,
    tx: mpsc::Sender<Update>,
    rx: mpsc::Receiver<Update>,
    tokens_scanning: bool,
    pi_tokens_scanning: bool,
    last_refresh: Instant,
}

impl App {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            status: None,
            tokens: vec![],
            pi_tokens: vec![],
            refreshing: false,
            tx,
            rx,
            tokens_scanning: false,
            pi_tokens_scanning: false,
            last_refresh: Instant::now(),
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
                cache::load(CACHE_TTL_SECS)
            }
            .unwrap_or_else(|| {
                let fresh = providers::collect_all();
                cache::save(&fresh);
                fresh
            });
            let _ = tx.send(Update::Status(status));
        });
        if self.begin_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Update::Tokens(tokens::claude_today()));
            });
        }
        if self.begin_pi_token_scan() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Update::PiTokens(pi_tokens::scan_pi_today()));
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
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('r') => self.refresh(true),
                            _ => {}
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

        let mut constraints = vec![Constraint::Length(1)]; // cabecera
        for p in &status.providers {
            constraints.push(Constraint::Length(provider_height(p)));
        }
        if !self.tokens.is_empty() {
            constraints.push(Constraint::Length(self.tokens.len() as u16 + 3));
        }
        if !self.pi_tokens.is_empty() {
            constraints.push(Constraint::Length(pi_section_height(self.pi_tokens.len())));
        }
        constraints.push(Constraint::Min(0)); // relleno
        constraints.push(Constraint::Length(1)); // pie
        let areas = Layout::vertical(constraints).split(f.area());

        self.draw_header(f, areas[0]);
        for (i, p) in status.providers.iter().enumerate() {
            draw_provider(f, areas[i + 1], p);
        }
        let mut section = status.providers.len() + 1;
        if !self.tokens.is_empty() {
            self.draw_tokens(f, areas[section]);
            section += 1;
        }
        if !self.pi_tokens.is_empty() {
            self.draw_pi_tokens(f, areas[section]);
        }
        self.draw_footer(f, areas[areas.len() - 1], status);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " lazysubs ",
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

    fn draw_footer(&self, f: &mut Frame, area: Rect, status: &Status) {
        let [left, right] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).areas(area);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" q ", Style::new().fg(ACCENT)),
                Span::styled("salir  ", Style::new().fg(DIM)),
                Span::styled("r ", Style::new().fg(ACCENT)),
                Span::styled("refrescar", Style::new().fg(DIM)),
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

fn percent_color(pct: f64) -> Color {
    if pct >= 95.0 {
        Color::Red
    } else if pct >= 80.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn draw_provider(f: &mut Frame, area: Rect, p: &ProviderStatus) {
    let plan = p.plan.as_deref().unwrap_or("?");
    let block = bordered(format!(" {} {} ", p.icon, p.name))
        .title(Span::styled(format!(" {plan} "), Style::new().fg(DIM)));
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
