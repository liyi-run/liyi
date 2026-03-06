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
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub item_or_req: String,
    pub kind: DiagnosticKind,
    pub severity: Severity,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let icon = match self.severity {
            Severity::Info => match self.kind {
                DiagnosticKind::Current => "✓",
                DiagnosticKind::Trivial => "·",
                DiagnosticKind::Ignored => "·",
                DiagnosticKind::ReqNoRelated => "·",
                _ => "·",
            },
            Severity::Warning => match self.kind {
                DiagnosticKind::Shifted { .. } => "↕",
                _ => "⚠",
            },
            Severity::Error => "✗",
        };
        write!(f, "{}: {} {}", self.item_or_req, icon, self.message)
    }
}

#[derive(Debug, Clone)]
pub struct CheckFlags {
    pub fail_on_stale: bool,
    pub fail_on_unreviewed: bool,
    pub fail_on_req_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiyiExitCode {
    Clean = 0,
    CheckFailure = 1,
    InternalError = 2,
}

pub fn compute_exit_code(diagnostics: &[Diagnostic], flags: &CheckFlags) -> LiyiExitCode {
    let mut has_check_failure = false;

    for d in diagnostics {
        match &d.kind {
            DiagnosticKind::ParseError { .. } | DiagnosticKind::UnknownVersion { .. } => {
                return LiyiExitCode::InternalError;
            }
            DiagnosticKind::Stale if flags.fail_on_stale => has_check_failure = true,
            DiagnosticKind::Unreviewed if flags.fail_on_unreviewed => has_check_failure = true,
            DiagnosticKind::ReqChanged { .. } if flags.fail_on_req_changed => {
                has_check_failure = true;
            }
            // Error-severity diagnostics always trigger check failure
            DiagnosticKind::MalformedHash
            | DiagnosticKind::UnknownRequirement { .. }
            | DiagnosticKind::RequirementCycle { .. }
            | DiagnosticKind::SpanPastEof { .. }
            | DiagnosticKind::InvalidSpan { .. }
            | DiagnosticKind::OrphanedSource
            | DiagnosticKind::DuplicateEntry => {
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

    for d in diagnostics {
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
