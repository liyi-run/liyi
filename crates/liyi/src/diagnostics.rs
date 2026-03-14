use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    Current,
    Unreviewed,
    Stale,
    Shifted { from: [usize; 2], to: [usize; 2] },
    ReqChanged { requirement: String },
    UnknownRequirement { name: String },
    Untracked,
    ReqNoRelated,
    Trivial,
    Ignored,
    SpanPastEof { span: [usize; 2], file_lines: usize },
    InvalidSpan { span: [usize; 2] },
    MalformedHash,
    DuplicateEntry,
    OrphanedSource,
    ParseError { detail: String },
    UnknownVersion { version: String },
    RequirementCycle { path: Vec<String> },
    AmbiguousSidecar { canonical: String, other: String },
    MissingRelatedEdge { name: String },
    ConflictingTriviality,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub item_or_req: String,
    pub kind: DiagnosticKind,
    pub severity: Severity,
    pub message: String,
    /// Optional fix hint — the exact command to resolve this diagnostic.
    pub fix_hint: Option<String>,
    /// Whether this diagnostic was resolved by `--fix`.
    pub fixed: bool,
    /// 1-indexed start line of the source span, for `file:line` display.
    pub span_start: Option<usize>,
    /// 1-indexed line number of the source annotation, for `--prompt` output.
    pub annotation_line: Option<usize>,
    /// Full text of a requirement block, for `--prompt` output.
    pub requirement_text: Option<String>,
    /// Intent text from the sidecar spec, for CI annotation output.
    pub intent: Option<String>,
}

impl Diagnostic {
    /// Format this diagnostic with a repo-root prefix stripped from the
    /// file path so that output shows repo-relative paths.
    pub fn display_with_root(&self, root: &std::path::Path) -> String {
        let rel = self.file.strip_prefix(root).unwrap_or(&self.file);
        let display_line = self.span_start.or(self.annotation_line);
        let file_loc = match display_line {
            Some(line) => format!("{}:{}", rel.display(), line),
            None => format!("{}", rel.display()),
        };
        let icon = if self.fixed {
            "✓ fixed"
        } else {
            Self::icon(&self.kind, self.severity)
        };
        let main_line = if self.item_or_req.is_empty() {
            format!("{file_loc}: {icon} {}", self.message)
        } else {
            format!("{file_loc}: {}: {icon} {}", self.item_or_req, self.message)
        };
        match &self.fix_hint {
            Some(hint) if !self.fixed => format!("{main_line}\n  fix: {hint}"),
            _ => main_line,
        }
    }

    fn icon(kind: &DiagnosticKind, severity: Severity) -> &'static str {
        match severity {
            Severity::Info => match kind {
                DiagnosticKind::Current => "✓",
                DiagnosticKind::Trivial => "·",
                DiagnosticKind::Ignored => "·",
                DiagnosticKind::ReqNoRelated => "·",
                _ => "·",
            },
            Severity::Warning => match kind {
                DiagnosticKind::Shifted { .. } => "↕",
                _ => "⚠",
            },
            Severity::Error => "✗",
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let icon = if self.fixed {
            "✓ fixed"
        } else {
            Self::icon(&self.kind, self.severity)
        };
        if self.item_or_req.is_empty() {
            write!(f, "{}: {} {}", self.file.display(), icon, self.message)
        } else {
            write!(
                f,
                "{}: {}: {} {}",
                self.file.display(),
                self.item_or_req,
                icon,
                self.message
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckFlags {
    pub fail_on_stale: bool,
    pub fail_on_unreviewed: bool,
    pub fail_on_req_changed: bool,
    pub fail_on_untracked: bool,
}

/// Process exit codes for `liyi check` and related commands.
///
/// <!-- @liyi:related liyi-check-exit-code -->
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiyiExitCode {
    Clean = 0,
    CheckFailure = 1,
    InternalError = 2,
}

// @liyi:related liyi-check-exit-code
pub fn compute_exit_code(diagnostics: &[Diagnostic], flags: &CheckFlags) -> LiyiExitCode {
    let mut has_check_failure = false;

    for d in diagnostics {
        if d.fixed {
            continue;
        }
        match &d.kind {
            DiagnosticKind::ParseError { .. } | DiagnosticKind::UnknownVersion { .. } => {
                return LiyiExitCode::InternalError;
            }
            DiagnosticKind::Stale if flags.fail_on_stale => has_check_failure = true,
            DiagnosticKind::Unreviewed if flags.fail_on_unreviewed => has_check_failure = true,
            DiagnosticKind::ReqChanged { .. } if flags.fail_on_req_changed => {
                has_check_failure = true;
            }
            DiagnosticKind::Untracked if flags.fail_on_untracked => has_check_failure = true,
            // Error-severity diagnostics always trigger check failure
            DiagnosticKind::MalformedHash
            | DiagnosticKind::UnknownRequirement { .. }
            | DiagnosticKind::RequirementCycle { .. }
            | DiagnosticKind::SpanPastEof { .. }
            | DiagnosticKind::InvalidSpan { .. }
            | DiagnosticKind::OrphanedSource
            | DiagnosticKind::DuplicateEntry
            | DiagnosticKind::MissingRelatedEdge { .. }
            | DiagnosticKind::ConflictingTriviality => {
                has_check_failure = true;
            }
            _ => {}
        }
    }

    if has_check_failure {
        LiyiExitCode::CheckFailure
    } else {
        LiyiExitCode::Clean
    }
}

/// Produce a one-line summary of check results.
///
/// Example output: `12 current, 3 stale, 1 unreviewed, 2 errors`
pub fn format_summary(diagnostics: &[Diagnostic]) -> String {
    let mut current = 0usize;
    let mut stale = 0usize;
    let mut shifted = 0usize;
    let mut unreviewed = 0usize;
    let mut errors = 0usize;
    let mut trivial = 0usize;
    let mut ignored = 0usize;
    let mut untracked = 0usize;
    let mut fixed = 0usize;

    for d in diagnostics {
        if d.fixed {
            fixed += 1;
            continue;
        }
        match &d.kind {
            DiagnosticKind::Current => current += 1,
            DiagnosticKind::Stale => stale += 1,
            DiagnosticKind::Shifted { .. } => shifted += 1,
            DiagnosticKind::Unreviewed => unreviewed += 1,
            DiagnosticKind::Trivial => trivial += 1,
            DiagnosticKind::Ignored => ignored += 1,
            DiagnosticKind::Untracked => untracked += 1,
            DiagnosticKind::ReqNoRelated => {} // informational, not counted
            DiagnosticKind::ReqChanged { .. } => stale += 1,
            _ if d.severity == Severity::Error => errors += 1,
            _ => {}
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if current > 0 {
        parts.push(format!("{current} current"));
    }
    if fixed > 0 {
        parts.push(format!("{fixed} fixed"));
    }
    if stale > 0 {
        parts.push(format!("{stale} stale"));
    }
    if shifted > 0 {
        parts.push(format!("{shifted} shifted"));
    }
    if unreviewed > 0 {
        parts.push(format!("{unreviewed} unreviewed"));
    }
    if errors > 0 {
        parts.push(format!(
            "{errors} error{}",
            if errors == 1 { "" } else { "s" }
        ));
    }
    if untracked > 0 {
        parts.push(format!("{untracked} untracked"));
    }
    if trivial > 0 {
        parts.push(format!("{trivial} trivial"));
    }
    if ignored > 0 {
        parts.push(format!("{ignored} ignored"));
    }

    if parts.is_empty() {
        "no specs checked".to_string()
    } else {
        parts.join(", ")
    }
}

/// Format a diagnostic as a GitHub Actions workflow command.
///
/// Emits `::notice`, `::warning`, or `::error` with `file`, `line`, and
/// `title` parameters so that annotations appear inline in PR diffs.
pub fn format_github_actions(d: &Diagnostic, root: &std::path::Path) -> String {
    let level = match d.severity {
        Severity::Info => "notice",
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    let rel = d.file.strip_prefix(root).unwrap_or(&d.file);
    let file = rel.display();

    // Escape special characters per GitHub Actions workflow command spec:
    // https://github.com/actions/toolkit/blob/main/packages/core/src/command.ts
    let escape = |s: &str| {
        s.replace('%', "%25")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
    };
    let message = escape(&d.message);

    // Append intent text to the message when available and non-sentinel.
    let message = match &d.intent {
        Some(intent) if intent != "=doc" && intent != "=trivial" => {
            format!("{message}%0AIntent: {}", escape(intent))
        }
        _ => message,
    };

    let title = if d.item_or_req.is_empty() {
        "立意".to_string()
    } else {
        let icon = if d.fixed {
            "✓"
        } else {
            Diagnostic::icon(&d.kind, d.severity)
        };
        format!("立意 {} {}", icon, d.item_or_req)
    };

    let display_line = d.span_start.or(d.annotation_line);
    match display_line {
        Some(line) => {
            format!("::{level} file={file},line={line},title={title}::{message}")
        }
        None => {
            format!("::{level} file={file},title={title}::{message}")
        }
    }
}
