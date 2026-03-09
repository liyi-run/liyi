use std::io;
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use ratatui::Terminal;
use syntect::highlighting::{self, ThemeSet};
use syntect::parsing::SyntaxSet;

use liyi::approve::{ApprovalCandidate, Decision};

/// Shared syntax-highlighting resources, initialised once per TUI session.
struct Highlighter {
    syntax_set: SyntaxSet,
    theme: highlighting::Theme,
}

/// TUI state for the approval workflow.
struct ApproveTui<'a> {
    candidates: &'a [ApprovalCandidate],
    decisions: Vec<Decision>,
    current: usize,
    /// Vertical scroll offset for the source code pane.
    scroll: u16,
    quit_all: bool,
    highlighter: Highlighter,
}

impl<'a> ApproveTui<'a> {
    fn new(candidates: &'a [ApprovalCandidate]) -> Self {
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-eighties.dark"].clone();
        Self {
            candidates,
            decisions: vec![Decision::Skip; candidates.len()],
            current: 0,
            scroll: 0,
            quit_all: false,
            highlighter: Highlighter {
                syntax_set: SyntaxSet::load_defaults_newlines(),
                theme,
            },
        }
    }

    fn candidate(&self) -> &ApprovalCandidate {
        &self.candidates[self.current]
    }

    fn done(&self) -> bool {
        self.current >= self.candidates.len() || self.quit_all
    }

    fn decide(&mut self, d: Decision) {
        self.decisions[self.current] = d;
        self.current += 1;
        self.scroll = 0;
    }

    fn go_back(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.scroll = 0;
        }
    }
}

/// Run the interactive TUI approval flow. Returns decisions parallel to
/// the candidates slice.
pub fn run_tui(candidates: &[ApprovalCandidate]) -> io::Result<Vec<Decision>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    enable_raw_mode()?;
    crossterm::execute!(io::stderr(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let mut app = ApproveTui::new(candidates);

    while !app.done() {
        terminal.draw(|f| draw(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => app.decide(Decision::Yes),
                KeyCode::Char('n') | KeyCode::Char('N') => app.decide(Decision::No),
                KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                    app.decide(Decision::Skip)
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    // Approve all remaining.
                    for i in app.current..candidates.len() {
                        app.decisions[i] = Decision::Yes;
                    }
                    app.current = candidates.len();
                }
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    app.quit_all = true;
                }
                KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Left => {
                    app.go_back();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.scroll = app.scroll.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.scroll = app.scroll.saturating_sub(1);
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    crossterm::execute!(io::stderr(), LeaveAlternateScreen)?;

    Ok(app.decisions)
}

fn draw(f: &mut ratatui::Frame, app: &ApproveTui) {
    let area = f.area();
    let candidate = app.candidate();
    let total = app.candidates.len();
    let current = app.current + 1; // 1-indexed for display

    // Layout: header (3), intent block (flexible), source block (flexible),
    // progress bar (3), keybindings (3).
    let chunks = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(4),   // intent
        Constraint::Min(6),   // source
        Constraint::Length(3), // progress
        Constraint::Length(2), // keybindings
    ])
    .split(area);

    draw_header(f, chunks[0], candidate, current, total);
    draw_intent(f, chunks[1], candidate);
    draw_source(f, chunks[2], candidate, app.scroll, &app.highlighter);
    draw_progress(f, chunks[3], current, total);
    draw_keys(f, chunks[4]);
}

fn draw_header(
    f: &mut ratatui::Frame,
    area: Rect,
    candidate: &ApprovalCandidate,
    current: usize,
    total: usize,
) {
    let title = format!(
        " Item {current}/{total} │ {} │ {}:{}-{} ",
        candidate.source_display,
        candidate.item_name,
        candidate.source_span[0],
        candidate.source_span[1],
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);
}

fn draw_intent(f: &mut ratatui::Frame, area: Rect, candidate: &ApprovalCandidate) {
    let intent_text = if candidate.intent == "=doc" {
        "(intent delegated to source docstring)".to_string()
    } else {
        candidate.intent.clone()
    };

    let paragraph = Paragraph::new(intent_text)
        .block(
            Block::default()
                .title(" Intent ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(paragraph, area);
}

/// Convert a syntect `Color` to a ratatui `Color`.
fn to_ratatui_color(c: highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn draw_source(
    f: &mut ratatui::Frame,
    area: Rect,
    candidate: &ApprovalCandidate,
    scroll: u16,
    hl: &Highlighter,
) {
    let syntax = hl
        .syntax_set
        .find_syntax_by_extension(
            Path::new(&candidate.source_display)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or(""),
        )
        .unwrap_or_else(|| hl.syntax_set.find_syntax_plain_text());

    let mut h = syntect::easy::HighlightLines::new(syntax, &hl.theme);

    let lines: Vec<Line> = candidate
        .source_lines
        .iter()
        .map(|(lineno, content)| {
            let ranges = h
                .highlight_line(content, &hl.syntax_set)
                .unwrap_or_default();

            let mut spans: Vec<Span> = Vec::with_capacity(ranges.len() + 1);
            spans.push(Span::styled(
                format!(" {lineno:>4} │ "),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));

            for (style, text) in &ranges {
                let mut ratatui_style = Style::default().fg(to_ratatui_color(style.foreground));
                if style.font_style.contains(highlighting::FontStyle::BOLD) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(highlighting::FontStyle::ITALIC) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                if style.font_style.contains(highlighting::FontStyle::UNDERLINE) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                }
                spans.push(Span::styled((*text).to_string(), ratatui_style));
            }

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(format!(" {} ", candidate.source_display))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

fn draw_progress(f: &mut ratatui::Frame, area: Rect, current: usize, total: usize) {
    let ratio = current as f64 / total as f64;
    let label = format!("{current}/{total}");
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
        .ratio(ratio.min(1.0))
        .label(label);
    f.render_widget(gauge, area);
}

fn draw_keys(f: &mut ratatui::Frame, area: Rect) {
    let keys = Line::from(vec![
        Span::styled(" y", Style::default().fg(Color::Green).bold()),
        Span::raw(" approve  "),
        Span::styled("n", Style::default().fg(Color::Red).bold()),
        Span::raw(" reject  "),
        Span::styled("s", Style::default().fg(Color::Yellow).bold()),
        Span::raw("/"),
        Span::styled("↵", Style::default().fg(Color::Yellow).bold()),
        Span::raw(" skip  "),
        Span::styled("a", Style::default().fg(Color::Cyan).bold()),
        Span::raw(" approve all  "),
        Span::styled("b", Style::default().fg(Color::Blue).bold()),
        Span::raw("/"),
        Span::styled("←", Style::default().fg(Color::Blue).bold()),
        Span::raw(" back  "),
        Span::styled("j/k", Style::default().fg(Color::DarkGray).bold()),
        Span::raw(" scroll  "),
        Span::styled("q", Style::default().fg(Color::Red).bold()),
        Span::raw("/"),
        Span::styled("esc", Style::default().fg(Color::Red).bold()),
        Span::raw(" quit"),
    ]);
    f.render_widget(Paragraph::new(keys), area);
}
