use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::{
    CheckFlags, Diagnostic, DiagnosticKind, LiyiExitCode, Severity, compute_exit_code,
};
use crate::discovery::{SidecarEntry, discover};
use crate::hashing::{SpanError, hash_span};
use crate::markers::{SourceMarker, scan_markers};
use crate::schema::validate_version;
use crate::shift::{ShiftResult, detect_shift};
use crate::sidecar::{Spec, parse_sidecar, write_sidecar};

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A requirement discovered during pass 1.
struct RequirementRecord {
    file: PathBuf,
    line: usize,
    hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the two-pass check and optionally apply `--fix` corrections.
///
/// Returns a sorted list of diagnostics and the appropriate exit code.
pub fn run_check(
    root: &Path,
    scope_paths: &[PathBuf],
    fix: bool,
    flags: &CheckFlags,
) -> (Vec<Diagnostic>, LiyiExitCode) {
    let disc = discover(root, scope_paths);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Surface discovery warnings as diagnostics.
    for w in &disc.warnings {
        diagnostics.push(Diagnostic {
            file: root.to_path_buf(),
            item_or_req: String::new(),
            kind: DiagnosticKind::AmbiguousSidecar {
                canonical: w.clone(),
                other: String::new(),
            },
            severity: Severity::Warning,
            message: w.clone(),
        });
    }

    // Shared source-content cache (avoids re-reading the same file).
    let mut source_cache: HashMap<PathBuf, String> = HashMap::new();

    // ------------------------------------------------------------------
    // Pass 1 — Requirement discovery (project-global)
    // ------------------------------------------------------------------
    let mut requirements: HashMap<String, RequirementRecord> = HashMap::new();

    for file_path in &disc.all_files {
        let content = read_cached(&mut source_cache, file_path);
        let content = match content {
            Some(c) => c,
            None => continue,
        };

        let markers = scan_markers(&content);
        for m in &markers {
            if let SourceMarker::Requirement { name, line } = m {
                if let Some(existing) = requirements.get(name) {
                    // Duplicate requirement name — emit error for both sites.
                    diagnostics.push(Diagnostic {
                        file: file_path.clone(),
                        item_or_req: name.clone(),
                        kind: DiagnosticKind::DuplicateEntry,
                        severity: Severity::Error,
                        message: format!(
                            "duplicate \x40liyi:requirement \"{name}\" (also in {}:{})",
                            existing.file.display(),
                            existing.line,
                        ),
                    });
                } else {
                    requirements.insert(
                        name.clone(),
                        RequirementRecord {
                            file: file_path.clone(),
                            line: *line,
                            hash: None, // filled from sidecar below
                        },
                    );
                }
            }
        }
    }

    // Enrich requirement records with hashes from any existing sidecars.
    for entry in &disc.sidecars {
        let sc_content = match fs::read_to_string(&entry.sidecar_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let sidecar = match parse_sidecar(&sc_content) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for spec in &sidecar.specs {
            if let Spec::Requirement(req) = spec {
                if let Some(rec) = requirements.get_mut(&req.requirement) {
                    if rec.hash.is_none() {
                        rec.hash = req.source_hash.clone();
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 2 — Item / requirement checking (scoped to discovered sidecars)
    // ------------------------------------------------------------------
    for entry in &disc.sidecars {
        check_sidecar(
            entry,
            &mut diagnostics,
            &mut source_cache,
            &requirements,
            fix,
        );
    }

    // Sort by file path, then by item/requirement name.
    diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.item_or_req.cmp(&b.item_or_req))
    });

    let exit_code = compute_exit_code(&diagnostics, flags);
    (diagnostics, exit_code)
}

// ---------------------------------------------------------------------------
// Per-sidecar checking
// ---------------------------------------------------------------------------

fn check_sidecar(
    entry: &SidecarEntry,
    diagnostics: &mut Vec<Diagnostic>,
    source_cache: &mut HashMap<PathBuf, String>,
    requirements: &HashMap<String, RequirementRecord>,
    fix: bool,
) {
    let sidecar_path = &entry.sidecar_path;

    // 1. Read & parse sidecar
    let sc_content = match fs::read_to_string(sidecar_path) {
        Ok(c) => c,
        Err(e) => {
            diagnostics.push(Diagnostic {
                file: sidecar_path.clone(),
                item_or_req: String::new(),
                kind: DiagnosticKind::ParseError {
                    detail: e.to_string(),
                },
                severity: Severity::Error,
                message: format!("cannot read sidecar: {e}"),
            });
            return;
        }
    };

    let mut sidecar = match parse_sidecar(&sc_content) {
        Ok(s) => s,
        Err(e) => {
            diagnostics.push(Diagnostic {
                file: sidecar_path.clone(),
                item_or_req: String::new(),
                kind: DiagnosticKind::ParseError { detail: e.clone() },
                severity: Severity::Error,
                message: e,
            });
            return;
        }
    };

    // 2. Validate version
    if let Err(e) = validate_version(&sidecar.version) {
        diagnostics.push(Diagnostic {
            file: sidecar_path.clone(),
            item_or_req: String::new(),
            kind: DiagnosticKind::UnknownVersion {
                version: sidecar.version.clone(),
            },
            severity: Severity::Error,
            message: e,
        });
        return;
    }

    // 3. Check source exists
    if !entry.source_path.is_file() {
        diagnostics.push(Diagnostic {
            file: sidecar_path.clone(),
            item_or_req: entry.repo_relative_source.clone(),
            kind: DiagnosticKind::OrphanedSource,
            severity: Severity::Error,
            message: format!(
                "source file {} not found",
                entry.source_path.display()
            ),
        });
        return;
    }

    // 4. Read source content (cached)
    let source_content = match read_cached(source_cache, &entry.source_path) {
        Some(c) => c,
        None => return,
    };

    let source_markers = scan_markers(&source_content);
    let mut modified = false;

    // 5. Check each spec
    for spec in &mut sidecar.specs {
        match spec {
            Spec::Item(item) => {
                let label = item.item.clone();

                // a. Hash the span
                match hash_span(&source_content, item.source_span) {
                    Ok((computed_hash, computed_anchor)) => {
                        let is_current = item
                            .source_hash
                            .as_ref()
                            .map_or(false, |h| h == &computed_hash);

                        if is_current {
                            // CURRENT
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Current,
                                severity: Severity::Info,
                                message: "hash matches".into(),
                            });
                        } else if item.source_hash.is_none() {
                            // No hash yet — fill it if --fix
                            if fix {
                                item.source_hash = Some(computed_hash.clone());
                                item.source_anchor = Some(computed_anchor.clone());
                                modified = true;
                            }
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Stale,
                                severity: Severity::Warning,
                                message: "missing source_hash".into(),
                            });
                        } else {
                            // Hash mismatch — try shift detection
                            let expected = item.source_hash.as_ref().unwrap();
                            match detect_shift(
                                &source_content,
                                item.source_span,
                                expected,
                            ) {
                                ShiftResult::Shifted { delta, new_span } => {
                                    let old_span = item.source_span;
                                    if fix {
                                        item.source_span = new_span;
                                        // Recompute hash/anchor at new span
                                        if let Ok((h, a)) =
                                            hash_span(&source_content, new_span)
                                        {
                                            item.source_hash = Some(h);
                                            item.source_anchor = Some(a);
                                        }
                                        modified = true;
                                    }
                                    diagnostics.push(Diagnostic {
                                        file: entry.source_path.clone(),
                                        item_or_req: label.clone(),
                                        kind: DiagnosticKind::Shifted {
                                            from: old_span,
                                            to: new_span,
                                        },
                                        severity: Severity::Warning,
                                        message: format!(
                                            "span shifted by {delta:+} → [{}, {}]",
                                            new_span[0], new_span[1]
                                        ),
                                    });
                                }
                                ShiftResult::Stale => {
                                    diagnostics.push(Diagnostic {
                                        file: entry.source_path.clone(),
                                        item_or_req: label.clone(),
                                        kind: DiagnosticKind::Stale,
                                        severity: Severity::Warning,
                                        message: "hash mismatch, could not relocate"
                                            .into(),
                                    });
                                }
                            }
                        }
                    }
                    Err(SpanError::PastEof { end, total }) => {
                        diagnostics.push(Diagnostic {
                            file: entry.source_path.clone(),
                            item_or_req: label.clone(),
                            kind: DiagnosticKind::SpanPastEof {
                                span: item.source_span,
                                file_lines: total,
                            },
                            severity: Severity::Error,
                            message: format!(
                                "span end {end} exceeds file length {total}"
                            ),
                        });
                    }
                    Err(SpanError::Inverted { .. } | SpanError::Empty) => {
                        diagnostics.push(Diagnostic {
                            file: entry.source_path.clone(),
                            item_or_req: label.clone(),
                            kind: DiagnosticKind::InvalidSpan {
                                span: item.source_span,
                            },
                            severity: Severity::Error,
                            message: format!(
                                "invalid span [{}, {}]",
                                item.source_span[0], item.source_span[1]
                            ),
                        });
                    }
                }

                // b. Review status
                let reviewed_in_sidecar = item.reviewed;
                let has_intent_marker = source_markers.iter().any(|m| {
                    if let SourceMarker::Intent { line, .. } = m {
                        *line >= item.source_span[0] && *line <= item.source_span[1]
                    } else {
                        false
                    }
                });
                if !reviewed_in_sidecar && !has_intent_marker {
                    diagnostics.push(Diagnostic {
                        file: entry.source_path.clone(),
                        item_or_req: label.clone(),
                        kind: DiagnosticKind::Unreviewed,
                        severity: Severity::Warning,
                        message: "not reviewed".into(),
                    });
                }

                // c. Trivial / ignore markers within or immediately before span
                let span_start = item.source_span[0];
                let span_end = item.source_span[1];
                for m in &source_markers {
                    match m {
                        SourceMarker::Trivial { line }
                            if *line >= span_start.saturating_sub(1)
                                && *line <= span_end =>
                        {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Trivial,
                                severity: Severity::Info,
                                message: "marked \x40liyi:trivial".into(),
                            });
                        }
                        SourceMarker::Ignore { line, .. }
                            if *line >= span_start.saturating_sub(1)
                                && *line <= span_end =>
                        {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Ignored,
                                severity: Severity::Info,
                                message: "marked \x40liyi:ignore".into(),
                            });
                        }
                        _ => {}
                    }
                }

                // d. Related requirements
                if let Some(ref related) = item.related {
                    for (req_name, stored_hash) in related {
                        match requirements.get(req_name) {
                            None => {
                                diagnostics.push(Diagnostic {
                                    file: entry.source_path.clone(),
                                    item_or_req: label.clone(),
                                    kind: DiagnosticKind::UnknownRequirement {
                                        name: req_name.clone(),
                                    },
                                    severity: Severity::Error,
                                    message: format!(
                                        "related requirement \"{req_name}\" not found"
                                    ),
                                });
                            }
                            Some(rec) => {
                                // If both the sidecar and the requirement record
                                // have hashes, compare them.
                                if let (Some(sh), Some(rh)) =
                                    (stored_hash.as_ref(), rec.hash.as_ref())
                                {
                                    if sh != rh {
                                        diagnostics.push(Diagnostic {
                                            file: entry.source_path.clone(),
                                            item_or_req: label.clone(),
                                            kind: DiagnosticKind::ReqChanged {
                                                requirement: req_name.clone(),
                                            },
                                            severity: Severity::Warning,
                                            message: format!(
                                                "requirement \"{req_name}\" has changed"
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Spec::Requirement(req) => {
                let label = req.requirement.clone();
                match hash_span(&source_content, req.source_span) {
                    Ok((computed_hash, computed_anchor)) => {
                        let is_current = req
                            .source_hash
                            .as_ref()
                            .map_or(false, |h| h == &computed_hash);

                        if is_current {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label,
                                kind: DiagnosticKind::Current,
                                severity: Severity::Info,
                                message: "requirement hash matches".into(),
                            });
                        } else {
                            if fix {
                                req.source_hash = Some(computed_hash);
                                req.source_anchor = Some(computed_anchor);
                                modified = true;
                            }
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label,
                                kind: DiagnosticKind::Stale,
                                severity: Severity::Warning,
                                message: "requirement hash mismatch or missing".into(),
                            });
                        }
                    }
                    Err(SpanError::PastEof { end, total }) => {
                        diagnostics.push(Diagnostic {
                            file: entry.source_path.clone(),
                            item_or_req: label,
                            kind: DiagnosticKind::SpanPastEof {
                                span: req.source_span,
                                file_lines: total,
                            },
                            severity: Severity::Error,
                            message: format!(
                                "span end {end} exceeds file length {total}"
                            ),
                        });
                    }
                    Err(SpanError::Inverted { .. } | SpanError::Empty) => {
                        diagnostics.push(Diagnostic {
                            file: entry.source_path.clone(),
                            item_or_req: label,
                            kind: DiagnosticKind::InvalidSpan {
                                span: req.source_span,
                            },
                            severity: Severity::Error,
                            message: format!(
                                "invalid span [{}, {}]",
                                req.source_span[0], req.source_span[1]
                            ),
                        });
                    }
                }
            }
        }
    }

    // Write back if --fix produced changes.
    if fix && modified {
        let output = write_sidecar(&sidecar);
        let _ = fs::write(sidecar_path, output);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a file into the cache and return a clone of its contents.
fn read_cached(cache: &mut HashMap<PathBuf, String>, path: &Path) -> Option<String> {
    if let Some(content) = cache.get(path) {
        return Some(content.clone());
    }
    match fs::read_to_string(path) {
        Ok(content) => {
            cache.insert(path.to_path_buf(), content.clone());
            Some(content)
        }
        Err(_) => None,
    }
}
