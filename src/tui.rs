use crate::output::countdown;
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

struct App {
    status: Option<Status>,
    tokens: Vec<ModelTokens>,
    refreshing: bool,
    tx: mpsc::Sender<(Status, Vec<ModelTokens>)>,
    rx: mpsc::Receiver<(Status, Vec<ModelTokens>)>,
    last_refresh: Instant,
}

impl App {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            status: None,
            tokens: vec![],
            refreshing: false,
            tx,
            rx,
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
            let status = if force { None } else { cache::load(CACHE_TTL_SECS) }.unwrap_or_else(|| {
                let fresh = providers::collect_all();
                cache::save(&fresh);
                fresh
            });
            let toks = tokens::claude_today();
            let _ = tx.send((status, toks));
        });
    }

    fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        self.refresh(false);
        loop {
            while let Ok((status, toks)) = self.rx.try_recv() {
                self.status = Some(status);
                self.tokens = toks;
                self.refreshing = false;
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
                Paragraph::new("cargando…").fg_dim().alignment(Alignment::Center),
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
        constraints.push(Constraint::Min(0)); // relleno
        constraints.push(Constraint::Length(1)); // pie
        let areas = Layout::vertical(constraints).split(f.area());

        self.draw_header(f, areas[0]);
        for (i, p) in status.providers.iter().enumerate() {
            draw_provider(f, areas[i + 1], p);
        }
        if !self.tokens.is_empty() {
            self.draw_tokens(f, areas[status.providers.len() + 1]);
        }
        self.draw_footer(f, areas[areas.len() - 1], status);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" lazysubs ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
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

    fn draw_footer(&self, f: &mut Frame, area: Rect, status: &Status) {
        let [left, right] = Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).areas(area);
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
            Paragraph::new(state).style(Style::new().fg(DIM)).alignment(Alignment::Right),
            right,
        );
    }
}

fn provider_height(p: &ProviderStatus) -> u16 {
    p.windows.len().max(1) as u16 + 2
}

fn bordered<'a>(title: impl Into<std::borrow::Cow<'a, str>>) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(title, Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)))
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
        f.render_widget(Paragraph::new(err.as_str()).style(Style::new().fg(Color::Red)), inner);
        return;
    }

    let rows = Layout::vertical(vec![Constraint::Length(1); p.windows.len()]).split(inner);
    for (row, w) in rows.iter().zip(&p.windows) {
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
        f.render_widget(Paragraph::new(format!(" {}", w.label)).style(label_style), label_a);

        f.render_widget(
            LineGauge::default()
                .ratio((w.used_percent / 100.0).clamp(0.0, 1.0))
                .label(format!("{:>3.0}%", w.used_percent))
                .filled_style(Style::new().fg(percent_color(w.used_percent)))
                .unfilled_style(Style::new().fg(DIM))
                .line_set(symbols::line::THICK),
            gauge_a,
        );

        let reset = w.resets_at.map(|t| format!("→ {} ", countdown(t))).unwrap_or_default();
        f.render_widget(
            Paragraph::new(reset).style(Style::new().fg(DIM)).alignment(Alignment::Right),
            reset_a,
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
