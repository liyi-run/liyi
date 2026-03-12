use std::io;
use std::io::Write;
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use similar::{ChangeTag, TextDiff};
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
        let scroll = Self::initial_scroll(&candidates[0]);
        Self {
            candidates,
            decisions: vec![Decision::Skip; candidates.len()],
            current: 0,
            scroll,
            quit_all: false,
            highlighter: Highlighter {
                syntax_set: SyntaxSet::load_defaults_newlines(),
                theme,
            },
        }
    }

    /// Compute an initial scroll offset that centres the span in the
    /// source pane (assuming ~20 visible lines as a reasonable default).
    fn initial_scroll(candidate: &ApprovalCandidate) -> u16 {
        let visible_estimate: usize = 20;
        candidate.span_offset.saturating_sub(visible_estimate / 4) as u16
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
        if !self.done() {
            self.scroll = Self::initial_scroll(self.candidate());
        }
    }

    fn go_back(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.scroll = Self::initial_scroll(self.candidate());
        }
    }

    fn go_forward(&mut self) {
        if self.current + 1 < self.candidates.len() {
            self.current += 1;
            self.scroll = Self::initial_scroll(self.candidate());
        }
    }
}

/// Open `$EDITOR` (or `$VISUAL`, falling back to `vi`) on a tempfile
/// pre-populated with the current intent.  Previous intent, item name,
/// and source location are presented as comment lines (stripped on read-back),
/// following the Git commit-message editing convention.
///
/// Returns `Some(edited_text)` if the user saved a non-empty result, or
/// `None` if the file was emptied or the editor exited with an error.
fn edit_intent_in_editor(candidate: &ApprovalCandidate) -> Option<String> {
    use std::process::Command;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let dir = std::env::temp_dir();
    let path = dir.join("liyi-approve-intent.txt");

    // Build tempfile content.
    let mut content = String::new();
    // Editable intent — lines above the comment separator.
    content.push_str(&candidate.intent);
    content.push('\n');

    // Comment block with context (stripped on read-back).
    content.push_str("\n# --- Do not edit below this line ---\n");
    content.push_str(&format!(
        "# Item: {}  ({}:{}-{})\n",
        candidate.item_name,
        candidate.source_display,
        candidate.source_span[0],
        candidate.source_span[1],
    ));
    if let Some(prev) = &candidate.prev_intent {
        content.push_str("#\n# Previously approved intent:\n");
        for line in prev.lines() {
            content.push_str(&format!("#   {line}\n"));
        }
    }
    content.push_str("#\n# Lines starting with '#' are ignored.\n");
    content.push_str("# An empty result (after stripping comments) cancels the edit.\n");

    // Write tempfile.
    {
        let mut f = std::fs::File::create(&path).ok()?;
        f.write_all(content.as_bytes()).ok()?;
    }

    let status = Command::new(&editor).arg(&path).status().ok()?;

    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    let raw = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);

    // Strip comment lines and trailing whitespace.
    let result: String = raw
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if result.is_empty() { None } else { Some(result) }
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
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    // Leave TUI to run $EDITOR.
                    disable_raw_mode()?;
                    crossterm::execute!(io::stderr(), LeaveAlternateScreen)?;

                    let candidate = app.candidate();
                    if let Some(edited) = edit_intent_in_editor(candidate) {
                        app.decide(Decision::Edit(edited));
                    }
                    // else: edit cancelled — stay on the same item.

                    // Re-enter TUI.
                    enable_raw_mode()?;
                    crossterm::execute!(io::stderr(), EnterAlternateScreen)?;
                    terminal = Terminal::new(CrosstermBackend::new(io::stderr()))?;
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
                KeyCode::Right => {
                    app.go_forward();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.scroll = app.scroll.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.scroll = app.scroll.saturating_sub(1);
                }
                KeyCode::PageDown => {
                    app.scroll = app.scroll.saturating_add(15);
                }
                KeyCode::PageUp => {
                    app.scroll = app.scroll.saturating_sub(15);
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
        Constraint::Min(4),    // intent
        Constraint::Min(6),    // source
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

/// Expand a `TextDiff` into styled `Span`s for one side of the diff.
///
/// `side` selects which changes to include:
/// - `ChangeTag::Delete` → previous (removed text highlighted)
/// - `ChangeTag::Insert` → current (inserted text highlighted)
///
/// Equal segments use `base_style`; changed segments use `highlight_style`.
fn diff_spans<'a>(
    old: &str,
    new: &str,
    side: ChangeTag,
    base_style: Style,
    highlight_style: Style,
) -> Vec<Span<'a>> {
    let diff = TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .diff_words(old, new);
    let mut spans = Vec::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                spans.push(Span::styled(change.value().to_owned(), base_style));
            }
            tag if tag == side => {
                spans.push(Span::styled(change.value().to_owned(), highlight_style));
            }
            _ => {
                // Other side's change — skip.
            }
        }
    }
    spans
}

/// Append styled `Span`s as indented display lines, splitting on embedded
/// newlines.  Each logical line is prefixed with two-space indentation.
fn push_span_lines<'a>(spans: Vec<Span<'a>>, lines: &mut Vec<Line<'a>>) {
    let mut current_line_spans: Vec<Span<'a>> = vec![Span::raw("  ")];
    for span in spans {
        let text = span.content.to_string();
        let style = span.style;
        let mut parts = text.split('\n');
        if let Some(first) = parts.next()
            && !first.is_empty()
        {
            current_line_spans.push(Span::styled(first.to_owned(), style));
        }
        for part in parts {
            lines.push(Line::from(std::mem::replace(
                &mut current_line_spans,
                vec![Span::raw("  ")],
            )));
            if !part.is_empty() {
                current_line_spans.push(Span::styled(part.to_owned(), style));
            }
        }
    }
    if current_line_spans.len() > 1 {
        lines.push(Line::from(current_line_spans));
    }
}

fn draw_intent(f: &mut ratatui::Frame, area: Rect, candidate: &ApprovalCandidate) {
    let current_text = if candidate.intent == "=doc" {
        "(intent delegated to source docstring)".to_string()
    } else {
        candidate.intent.clone()
    };

    match &candidate.prev_intent {
        None => {
            // First-time item — no prior approval.
            let paragraph = Paragraph::new(current_text)
                .block(
                    Block::default()
                        .title(" Intent (new) ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::White));
            f.render_widget(paragraph, area);
        }
        Some(prev) => {
            let prev_text = if prev == "=doc" {
                "(intent delegated to source docstring)".to_string()
            } else {
                prev.clone()
            };

            if prev_text == current_text {
                // Intent unchanged — just show it with a note.
                let paragraph = Paragraph::new(current_text)
                    .block(
                        Block::default()
                            .title(" Intent (unchanged from last approval) ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    )
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::White));
                f.render_widget(paragraph, area);
            } else {
                // Intent changed — show previous and current with
                // word-level diff highlighting.
                let mut lines: Vec<Line> = Vec::new();

                lines.push(Line::from(Span::styled(
                    "▼ Previously approved:",
                    Style::default().fg(Color::DarkGray).bold(),
                )));

                let prev_spans = diff_spans(
                    &prev_text,
                    &current_text,
                    ChangeTag::Delete,
                    Style::default().fg(Color::Red),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                );
                push_span_lines(prev_spans, &mut lines);

                lines.push(Line::from(""));

                lines.push(Line::from(Span::styled(
                    "▲ Current (proposed):",
                    Style::default().fg(Color::DarkGray).bold(),
                )));

                let cur_spans = diff_spans(
                    &prev_text,
                    &current_text,
                    ChangeTag::Insert,
                    Style::default().fg(Color::Green),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
                push_span_lines(cur_spans, &mut lines);

                let paragraph = Paragraph::new(Text::from(lines))
                    .block(
                        Block::default()
                            .title(" Intent (changed) ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    )
                    .wrap(Wrap { trim: false });
                f.render_widget(paragraph, area);
            }
        }
    }
}

/// Convert a syntect `Color` to a ratatui `Color`.
fn to_ratatui_color(c: highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Maximum file size (in lines) for which syntax highlighting is enabled.
/// Beyond this threshold the highlighter is skipped to avoid UI lag.
const MAX_HIGHLIGHT_LINES: usize = 10_000;

/// Maximum byte length of a single line for which syntax highlighting is
/// applied.  Longer lines fall back to plain text to prevent the regex
/// engine from stalling.
const MAX_LINE_LEN: usize = 4_096;

/// Returns `true` if the file is small enough for syntax highlighting.
fn file_highlight_enabled(candidate: &ApprovalCandidate) -> bool {
    candidate.source_lines.len() <= MAX_HIGHLIGHT_LINES
}

/// Returns `true` if an individual line is short enough for syntax
/// highlighting.
fn line_highlight_enabled(line: &str) -> bool {
    line.len() <= MAX_LINE_LEN
}

fn draw_source(
    f: &mut ratatui::Frame,
    area: Rect,
    candidate: &ApprovalCandidate,
    scroll: u16,
    hl: &Highlighter,
) {
    let use_highlighting = file_highlight_enabled(candidate);

    let syntax = if use_highlighting {
        hl.syntax_set
            .find_syntax_by_extension(
                Path::new(&candidate.source_display)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or(""),
            )
            .unwrap_or_else(|| hl.syntax_set.find_syntax_plain_text())
    } else {
        hl.syntax_set.find_syntax_plain_text()
    };

    let mut h = syntect::easy::HighlightLines::new(syntax, &hl.theme);

    let span_start = candidate.span_offset;
    let span_end = candidate.span_offset + candidate.span_len;

    /// Subtle background colour applied to span lines to distinguish them
    /// from surrounding context.  All lines get full syntax highlighting.
    const SPAN_BG: Color = Color::Rgb(50, 60, 75);

    let lines: Vec<Line> = candidate
        .source_lines
        .iter()
        .enumerate()
        .map(|(idx, (lineno, content))| {
            let in_span = idx >= span_start && idx < span_end;

            let mut spans: Vec<Span> = Vec::new();

            let gutter_style = Style::default().fg(Color::DarkGray);
            let gutter_style = if in_span {
                gutter_style.bg(SPAN_BG)
            } else {
                gutter_style
            };
            spans.push(Span::styled(format!(" {lineno:>4} │ "), gutter_style));

            if use_highlighting && line_highlight_enabled(content) {
                let ranges = h
                    .highlight_line(content, &hl.syntax_set)
                    .unwrap_or_default();

                for (style, text) in &ranges {
                    let mut s = Style::default().fg(to_ratatui_color(style.foreground));
                    if style.font_style.contains(highlighting::FontStyle::BOLD) {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if style.font_style.contains(highlighting::FontStyle::ITALIC) {
                        s = s.add_modifier(Modifier::ITALIC);
                    }
                    if style
                        .font_style
                        .contains(highlighting::FontStyle::UNDERLINE)
                    {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    if in_span {
                        s = s.bg(SPAN_BG);
                    }
                    spans.push(Span::styled((*text).to_string(), s));
                }
            } else {
                let s = if in_span {
                    Style::default().bg(SPAN_BG)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(content.as_str(), s));
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
        Span::styled("e", Style::default().fg(Color::Cyan).bold()),
        Span::raw(" edit  "),
        Span::styled("s", Style::default().fg(Color::Yellow).bold()),
        Span::raw("/"),
        Span::styled("↵", Style::default().fg(Color::Yellow).bold()),
        Span::raw(" skip  "),
        Span::styled("a", Style::default().fg(Color::Cyan).bold()),
        Span::raw(" all  "),
        Span::styled("b", Style::default().fg(Color::Blue).bold()),
        Span::raw("/"),
        Span::styled("←→", Style::default().fg(Color::Blue).bold()),
        Span::raw(" nav  "),
        Span::styled("j/k", Style::default().fg(Color::DarkGray).bold()),
        Span::raw(" scroll  "),
        Span::styled("q", Style::default().fg(Color::Red).bold()),
        Span::raw("/"),
        Span::styled("esc", Style::default().fg(Color::Red).bold()),
        Span::raw(" quit"),
    ]);
    f.render_widget(Paragraph::new(keys), area);
}
