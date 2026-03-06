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
            _ => {}
        }
    }

    if has_check_failure {
        LiyiExitCode::CheckFailure
    } else {
        LiyiExitCode::Clean
    }
}
