use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::{
    CheckFlags, Diagnostic, DiagnosticKind, LiyiExitCode, Severity, compute_exit_code,
};
use crate::discovery::{SidecarEntry, discover};
use crate::hashing::{SpanError, hash_span};
use crate::markers::{SourceMarker, requirement_spans, scan_markers};
use crate::schema::validate_version;
use crate::shift::{ShiftResult, detect_shift};
use crate::sidecar::{Spec, parse_sidecar, write_sidecar};
use crate::tree_path::{compute_tree_path, detect_language, resolve_tree_path};

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A requirement discovered during pass 1.
struct RequirementRecord {
    file: PathBuf,
    line: usize,
    /// Hash stored in the sidecar (may be stale).
    hash: Option<String>,
    /// Hash freshly computed from the current source span.
    computed_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the two-pass check and optionally apply `--fix` corrections.
///
/// Returns a sorted list of diagnostics and the appropriate exit code.
// @liyi:related requirement-name-uniqueness
// @liyi:related requirement-discovery-global
// @liyi:related cycle-detection
pub fn run_check(
    root: &Path,
    scope_paths: &[PathBuf],
    fix: bool,
    dry_run: bool,
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
            fix_hint: None,
            fixed: false,
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
                        fix_hint: None,
                        fixed: false,
                    });
                } else {
                    requirements.insert(
                        name.clone(),
                        RequirementRecord {
                            file: file_path.clone(),
                            line: *line,
                            hash: None,          // filled from sidecar below
                            computed_hash: None, // filled from source below
                        },
                    );
                }
            }
        }
    }

    // Compute fresh hashes for requirement blocks from source markers so
    // that downstream related-edge checks can detect cascading staleness
    // in a single `liyi check` run, without needing `--fix` first.
    for file_path in &disc.all_files {
        let content = match read_cached(&mut source_cache, file_path) {
            Some(c) => c,
            None => continue,
        };
        let markers = scan_markers(&content);
        let spans = requirement_spans(&markers);
        for (name, span) in &spans {
            if let Some(rec) = requirements.get_mut(name)
                && let Ok((h, _)) = hash_span(&content, *span)
            {
                rec.computed_hash = Some(h);
            }
        }
    }

    // Collect related markers (`\x40liyi:related`) from all source files so that
    // source-level references count toward requirement coverage even
    // when the sidecar `related` edge has not been written yet.
    let mut source_related_refs: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for file_path in &disc.all_files {
        let content = match read_cached(&mut source_cache, file_path) {
            Some(c) => c,
            None => continue,
        };
        for m in scan_markers(&content) {
            if let SourceMarker::Related { name, .. } = m {
                source_related_refs.insert(name);
            }
        }
    }

    // Enrich requirement records with hashes from any existing sidecars.
    // Also track which requirements have a Spec::Requirement sidecar entry
    // and which requirement names are referenced by any Spec::Item via `related`.
    // Build a requirement dependency graph for cycle detection:
    //   edge A → B means: a sidecar defines requirement A AND contains an
    //   item that references requirement B.
    let mut requirements_with_sidecar: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut requirements_referenced: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut req_dep_graph: HashMap<String, Vec<String>> = HashMap::new();

    for entry in &disc.sidecars {
        let sc_content = match fs::read_to_string(&entry.sidecar_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let sidecar = match parse_sidecar(&sc_content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Collect requirements defined in this sidecar and requirements
        // referenced by items in this sidecar.
        let mut defined_in_sidecar: Vec<String> = Vec::new();
        let mut referenced_in_sidecar: Vec<String> = Vec::new();

        for spec in &sidecar.specs {
            match spec {
                Spec::Requirement(req) => {
                    requirements_with_sidecar.insert(req.requirement.clone());
                    defined_in_sidecar.push(req.requirement.clone());
                    if let Some(rec) = requirements.get_mut(&req.requirement)
                        && rec.hash.is_none()
                    {
                        rec.hash = req.source_hash.clone();
                    }
                }
                Spec::Item(item) => {
                    if let Some(ref related) = item.related {
                        for name in related.keys() {
                            requirements_referenced.insert(name.clone());
                            referenced_in_sidecar.push(name.clone());
                        }
                    }
                }
            }
        }

        // Build graph edges: defined req → referenced reqs in same sidecar.
        for def in &defined_in_sidecar {
            for reff in &referenced_in_sidecar {
                if def != reff {
                    req_dep_graph
                        .entry(def.clone())
                        .or_default()
                        .push(reff.clone());
                }
            }
        }
    }

    // Detect cycles in the requirement dependency graph using DFS.
    let cycles = detect_requirement_cycles(&req_dep_graph);

    // ------------------------------------------------------------------
    // Pass 2 — Item / requirement checking (scoped to discovered sidecars)
    // ------------------------------------------------------------------
    for entry in &disc.sidecars {
        check_sidecar(
            entry,
            &mut diagnostics,
            &mut source_cache,
            &requirements,
            root,
            fix,
            dry_run,
        );
    }

    // ------------------------------------------------------------------
    // Post-pass diagnostics
    // ------------------------------------------------------------------

    // Untracked: requirements found in source markers but absent from any sidecar.
    for (name, rec) in &requirements {
        if !requirements_with_sidecar.contains(name) {
            diagnostics.push(Diagnostic {
                file: rec.file.clone(),
                item_or_req: name.clone(),
                kind: DiagnosticKind::Untracked,
                severity: Severity::Warning,
                message: format!(
                    "\x40liyi:requirement \"{name}\" at line {} has no sidecar entry",
                    rec.line,
                ),
                fix_hint: None,
                fixed: false,
            });
        }
    }

    // ReqNoRelated: requirements with sidecar entries that no item references
    // (neither via sidecar `related` edges nor via source related markers).
    for name in &requirements_with_sidecar {
        if !requirements_referenced.contains(name)
            && !source_related_refs.contains(name)
            && let Some(rec) = requirements.get(name)
        {
            diagnostics.push(Diagnostic {
                file: rec.file.clone(),
                item_or_req: name.clone(),
                kind: DiagnosticKind::ReqNoRelated,
                severity: Severity::Info,
                message: format!("requirement \"{name}\" is not referenced by any item"),
                fix_hint: None,
                fixed: false,
            });
        }
    }

    // RequirementCycle: circular dependencies among requirements.
    for cycle in &cycles {
        let cycle_display = cycle.join(" → ");
        // Report from the first requirement in the cycle.
        let first = &cycle[0];
        let file = requirements
            .get(first)
            .map(|r| r.file.clone())
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            file,
            item_or_req: first.clone(),
            kind: DiagnosticKind::RequirementCycle {
                path: cycle.clone(),
            },
            severity: Severity::Error,
            message: format!("requirement cycle detected: {cycle_display}"),
            fix_hint: None,
            fixed: false,
        });
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

// @liyi:related reviewed-semantics
// @liyi:related fix-semantic-drift-protection
// @liyi:related fix-never-modifies-human-fields
fn check_sidecar(
    entry: &SidecarEntry,
    diagnostics: &mut Vec<Diagnostic>,
    source_cache: &mut HashMap<PathBuf, String>,
    requirements: &HashMap<String, RequirementRecord>,
    root: &Path,
    fix: bool,
    dry_run: bool,
) {
    let sidecar_path = &entry.sidecar_path;
    let rel_sidecar = sidecar_path
        .strip_prefix(root)
        .unwrap_or(sidecar_path)
        .display()
        .to_string();

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
                fix_hint: None,
                fixed: false,
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
                fix_hint: None,
                fixed: false,
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
            fix_hint: Some(format!("liyi migrate {rel_sidecar}")),
            fixed: false,
        });
        return;
    }

    // 2b. Validate source_hash format on all specs
    let hash_re = regex::Regex::new(r"^sha256:[0-9a-f]+$").unwrap();
    for spec in &sidecar.specs {
        match spec {
            Spec::Item(item) => {
                if let Some(ref h) = item.source_hash
                    && !hash_re.is_match(h)
                {
                    diagnostics.push(Diagnostic {
                        file: sidecar_path.clone(),
                        item_or_req: item.item.clone(),
                        kind: DiagnosticKind::MalformedHash,
                        severity: Severity::Error,
                        message: format!("source_hash \"{h}\" does not match sha256:<hex>"),
                        fix_hint: None,
                        fixed: false,
                    });
                }
                if let Some(ref related) = item.related {
                    for (name, hash_opt) in related {
                        if let Some(h) = hash_opt
                            && !hash_re.is_match(h)
                        {
                            diagnostics.push(Diagnostic {
                                file: sidecar_path.clone(),
                                item_or_req: item.item.clone(),
                                kind: DiagnosticKind::MalformedHash,
                                severity: Severity::Error,
                                message: format!(
                                    "related[\"{name}\"] hash \"{h}\" does not match sha256:<hex>"
                                ),
                                fix_hint: None,
                                fixed: false,
                            });
                        }
                    }
                }
            }
            Spec::Requirement(req) => {
                if let Some(ref h) = req.source_hash
                    && !hash_re.is_match(h)
                {
                    diagnostics.push(Diagnostic {
                        file: sidecar_path.clone(),
                        item_or_req: req.requirement.clone(),
                        kind: DiagnosticKind::MalformedHash,
                        severity: Severity::Error,
                        message: format!("source_hash \"{h}\" does not match sha256:<hex>"),
                        fix_hint: None,
                        fixed: false,
                    });
                }
            }
        }
    }

    // 3. Check source exists
    if !entry.source_path.is_file() {
        diagnostics.push(Diagnostic {
            file: sidecar_path.clone(),
            item_or_req: entry.repo_relative_source.clone(),
            kind: DiagnosticKind::OrphanedSource,
            severity: Severity::Error,
            message: format!("source file {} not found", entry.source_path.display()),
            fix_hint: None,
            fixed: false,
        });
        return;
    }

    // 4. Read source content (cached)
    let source_content = match read_cached(source_cache, &entry.source_path) {
        Some(c) => c,
        None => return,
    };

    let source_markers = scan_markers(&source_content);
    let marker_span_map = requirement_spans(&source_markers);
    let mut modified = false;

    // 5. Check each spec
    for spec in &mut sidecar.specs {
        match spec {
            Spec::Item(item) => {
                let label = item.item.clone();

                // a. Hash the span
                match hash_span(&source_content, item.source_span) {
                    Ok((computed_hash, computed_anchor)) => {
                        let is_current = item.source_hash.as_ref() == Some(&computed_hash);

                        if is_current {
                            // CURRENT
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Current,
                                severity: Severity::Info,
                                message: "hash matches".into(),
                                fix_hint: None,
                                fixed: false,
                            });
                        } else if item.source_hash.is_none() {
                            // No hash yet — fill it if --fix
                            if fix {
                                item.source_hash = Some(computed_hash.clone());
                                item.source_anchor = Some(computed_anchor.clone());
                                let lang = detect_language(&entry.source_path);
                                if let Some(l) = lang {
                                    item.tree_path =
                                        compute_tree_path(&source_content, item.source_span, l);
                                }
                                // Source changed since last review — clear
                                // reviewed so a human re-confirms intent.
                                item.reviewed = false;
                                modified = true;
                            }
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Stale,
                                severity: Severity::Warning,
                                message: "missing source_hash".into(),
                                fix_hint: Some("liyi check --fix".into()),
                                fixed: fix,
                            });
                        } else {
                            // Hash mismatch — try tree_path first, then shift
                            let lang = detect_language(&entry.source_path);

                            // Try tree_path-based recovery, tracking why it
                            // may not be available for diagnostic clarity.
                            let (tree_path_recovered, tree_path_note) = if item.tree_path.is_empty()
                            {
                                (None, "no tree_path set")
                            } else if lang.is_none() {
                                (None, "no grammar for source language")
                            } else {
                                let resolved = resolve_tree_path(
                                    &source_content,
                                    &item.tree_path,
                                    lang.unwrap(),
                                );
                                if resolved.is_some() {
                                    (resolved, "")
                                } else {
                                    (None, "tree_path resolution failed")
                                }
                            };

                            if let Some(new_span) = tree_path_recovered {
                                // tree_path resolved — check whether the
                                // content at new_span is unchanged (pure
                                // shift) or also changed (semantic drift).
                                let old_span = item.source_span;
                                let old_hash = item.source_hash.as_ref().unwrap();
                                let content_unchanged = hash_span(&source_content, new_span)
                                    .map(|(h, _)| h == *old_hash)
                                    .unwrap_or(false);

                                if new_span != old_span && content_unchanged {
                                    // Pure shift — content intact, only
                                    // position changed.  Safe to auto-fix.
                                    let delta = new_span[0] as i64 - old_span[0] as i64;
                                    if fix {
                                        item.source_span = new_span;
                                        if let Ok((h, a)) = hash_span(&source_content, new_span) {
                                            item.source_hash = Some(h);
                                            item.source_anchor = Some(a);
                                        }
                                        if let Some(l) = lang {
                                            item.tree_path =
                                                compute_tree_path(&source_content, new_span, l);
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
                                            "tree_path resolved, span shifted by {delta:+} → [{}, {}]",
                                            new_span[0], new_span[1]
                                        ),
                                    fix_hint: Some("liyi check --fix".into()),
                                    fixed: fix,
                                    });
                                } else {
                                    // Content changed (semantic drift).
                                    // Update span to track the item's
                                    // current location but do NOT rehash —
                                    // the stale hash is the signal that
                                    // intent review is needed.
                                    if fix && new_span != old_span {
                                        item.source_span = new_span;
                                        // Intentionally NOT updating hash —
                                        // leaves the spec stale so the next
                                        // `liyi check` flags it.
                                        if let Some(l) = lang {
                                            item.tree_path =
                                                compute_tree_path(&source_content, new_span, l);
                                        }
                                        modified = true;
                                    }
                                    let msg = if new_span != old_span {
                                        format!(
                                            "source changed and shifted → [{}, {}] (tree_path resolved, not auto-rehashed)",
                                            new_span[0], new_span[1]
                                        )
                                    } else {
                                        "source changed at tree_path location".into()
                                    };
                                    diagnostics.push(Diagnostic {
                                        file: entry.source_path.clone(),
                                        item_or_req: label.clone(),
                                        kind: DiagnosticKind::Stale,
                                        severity: Severity::Warning,
                                        message: msg,
                                        fix_hint: None,
                                        fixed: false,
                                    });
                                }
                            } else {
                                // Fallback to shift heuristic
                                let expected = item.source_hash.as_ref().unwrap();
                                match detect_shift(&source_content, item.source_span, expected) {
                                    ShiftResult::Shifted { delta, new_span } => {
                                        let old_span = item.source_span;
                                        if fix {
                                            item.source_span = new_span;
                                            // Recompute hash/anchor at new span
                                            if let Ok((h, a)) = hash_span(&source_content, new_span)
                                            {
                                                item.source_hash = Some(h);
                                                item.source_anchor = Some(a);
                                            }
                                            let lang = detect_language(&entry.source_path);
                                            if let Some(l) = lang {
                                                item.tree_path =
                                                    compute_tree_path(&source_content, new_span, l);
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
                                            fix_hint: Some("liyi check --fix".into()),
                                            fixed: fix,
                                        });
                                    }
                                    ShiftResult::Stale => {
                                        let detail = if tree_path_note.is_empty() {
                                            "hash mismatch, could not relocate".to_string()
                                        } else {
                                            format!(
                                                "hash mismatch, could not relocate ({tree_path_note})"
                                            )
                                        };
                                        diagnostics.push(Diagnostic {
                                            file: entry.source_path.clone(),
                                            item_or_req: label.clone(),
                                            kind: DiagnosticKind::Stale,
                                            severity: Severity::Warning,
                                            message: detail,
                                            fix_hint: None,
                                            fixed: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(SpanError::PastEof { end, total }) => {
                        // Try tree-path recovery before giving up
                        let lang = detect_language(&entry.source_path);
                        let (recovered, tp_note) = if item.tree_path.is_empty() {
                            (None, "no tree_path set")
                        } else if lang.is_none() {
                            (None, "no grammar for source language")
                        } else {
                            let r =
                                resolve_tree_path(&source_content, &item.tree_path, lang.unwrap());
                            if r.is_some() {
                                (r, "")
                            } else {
                                (None, "tree_path resolution failed")
                            }
                        };

                        if let Some(new_span) = recovered {
                            // PastEof means old hash is unreliable (can't
                            // hash a span past the file end).  Check
                            // whether the content at the resolved span
                            // matches the *recorded* hash to distinguish
                            // pure shift from semantic drift.
                            let old_span = item.source_span;
                            let content_unchanged = item
                                .source_hash
                                .as_ref()
                                .and_then(|old_h| {
                                    hash_span(&source_content, new_span)
                                        .ok()
                                        .map(|(h, _)| h == *old_h)
                                })
                                .unwrap_or(false);

                            if content_unchanged {
                                let delta = new_span[0] as i64 - old_span[0] as i64;
                                if fix {
                                    item.source_span = new_span;
                                    if let Ok((h, a)) = hash_span(&source_content, new_span) {
                                        item.source_hash = Some(h);
                                        item.source_anchor = Some(a);
                                    }
                                    if let Some(l) = lang {
                                        item.tree_path =
                                            compute_tree_path(&source_content, new_span, l);
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
                                        "span past EOF (end {end} > {total}), tree_path resolved, shifted by {delta:+} → [{}, {}]",
                                        new_span[0], new_span[1]
                                    ),
                                fix_hint: Some("liyi check --fix".into()),
                                fixed: fix,
                                });
                            } else {
                                // Content also changed — relocate span
                                // but leave hash stale.
                                if fix {
                                    item.source_span = new_span;
                                    if let Some(l) = lang {
                                        item.tree_path =
                                            compute_tree_path(&source_content, new_span, l);
                                    }
                                    modified = true;
                                }
                                diagnostics.push(Diagnostic {
                                    file: entry.source_path.clone(),
                                    item_or_req: label.clone(),
                                    kind: DiagnosticKind::Stale,
                                    severity: Severity::Warning,
                                    message: format!(
                                        "span past EOF (end {end} > {total}), tree_path resolved to [{}, {}] but content also changed (not auto-rehashed)",
                                        new_span[0], new_span[1]
                                    ),
                                fix_hint: None,
                                fixed: false,
                                });
                            }
                        } else {
                            let detail = if tp_note.is_empty() {
                                format!("span end {end} exceeds file length {total}")
                            } else {
                                format!("span end {end} exceeds file length {total} ({tp_note})")
                            };
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::SpanPastEof {
                                    span: item.source_span,
                                    file_lines: total,
                                },
                                severity: Severity::Error,
                                message: detail,
                                fix_hint: Some("liyi check --fix".into()),
                                fixed: false,
                            });
                        }
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
                            fix_hint: None,
                            fixed: false,
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
                        fix_hint: None,
                        fixed: false,
                    });
                }

                // c. Trivial / ignore markers within or immediately before span
                let span_start = item.source_span[0];
                let span_end = item.source_span[1];
                for m in &source_markers {
                    match m {
                        SourceMarker::Trivial { line }
                            if *line >= span_start.saturating_sub(1) && *line <= span_end =>
                        {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Trivial,
                                severity: Severity::Info,
                                message: "marked \x40liyi:trivial".into(),
                                fix_hint: None,
                                fixed: false,
                            });
                        }
                        SourceMarker::Ignore { line, .. }
                            if *line >= span_start.saturating_sub(1) && *line <= span_end =>
                        {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label.clone(),
                                kind: DiagnosticKind::Ignored,
                                severity: Severity::Info,
                                message: "marked \x40liyi:ignore".into(),
                                fix_hint: None,
                                fixed: false,
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
                                    fix_hint: None,
                                    fixed: false,
                                });
                            }
                            Some(rec) => {
                                // Compare the item's stored related hash
                                // against the requirement's *current* source
                                // hash (computed_hash).  This surfaces
                                // cascading staleness in one pass — even
                                // before `--fix` updates the requirement's
                                // sidecar hash.  Fall back to the sidecar
                                // hash when computed_hash is unavailable.
                                let current_req_hash =
                                    rec.computed_hash.as_ref().or(rec.hash.as_ref());
                                if let (Some(sh), Some(rh)) =
                                    (stored_hash.as_ref(), current_req_hash)
                                    && sh != rh
                                {
                                    diagnostics.push(Diagnostic {
                                        file: entry.source_path.clone(),
                                        item_or_req: label.clone(),
                                        kind: DiagnosticKind::ReqChanged {
                                            requirement: req_name.clone(),
                                        },
                                        severity: Severity::Warning,
                                        message: format!("requirement \"{req_name}\" has changed"),
                                        fix_hint: None,
                                        fixed: false,
                                    });
                                }
                            }
                        }
                    }
                }

                // e. Source related markers missing from sidecar
                let span_start = item.source_span[0];
                let span_end = item.source_span[1];
                for m in &source_markers {
                    if let SourceMarker::Related { name, line } = m {
                        // Include doc-comment lines immediately before the span
                        if *line >= span_start.saturating_sub(5) && *line <= span_end {
                            let has_edge =
                                item.related.as_ref().is_some_and(|r| r.contains_key(name));
                            if !has_edge {
                                if fix {
                                    let related = item.related.get_or_insert_with(BTreeMap::new);
                                    let hash_val =
                                        requirements.get(name).and_then(|rec| rec.hash.clone());
                                    related.insert(name.clone(), hash_val);
                                    modified = true;
                                }
                                diagnostics.push(Diagnostic {
                                    file: entry.source_path.clone(),
                                    item_or_req: label.clone(),
                                    kind: DiagnosticKind::MissingRelatedEdge {
                                        name: name.clone(),
                                    },
                                    severity: Severity::Error,
                                    message: format!(
                                        "source has \x40liyi:related \"{name}\" but sidecar is missing the related edge"
                                    ),
                                fix_hint: None,
                                fixed: fix,
                                });
                            }
                        }
                    }
                }

                // f. Fill in null hashes on existing related edges
                if fix && let Some(ref mut related) = item.related {
                    for (req_name, hash_val) in related.iter_mut() {
                        if hash_val.is_none()
                            && let Some(rec) = requirements.get(req_name)
                            && let Some(ref h) = rec.hash
                        {
                            *hash_val = Some(h.clone());
                            modified = true;
                        }
                    }
                }
            }
            Spec::Requirement(req) => {
                let label = req.requirement.clone();

                // Try marker-based span recovery first: if the file has
                // @liyi:end-requirement markers, use those for span.
                if let Some(&marker_span) = marker_span_map.get(&req.requirement)
                    && marker_span != req.source_span
                {
                    req.source_span = marker_span;
                    if fix {
                        modified = true;
                    }
                }

                match hash_span(&source_content, req.source_span) {
                    Ok((computed_hash, computed_anchor)) => {
                        let is_current = req.source_hash.as_ref() == Some(&computed_hash);

                        if is_current {
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label,
                                kind: DiagnosticKind::Current,
                                severity: Severity::Info,
                                message: "requirement hash matches".into(),
                                fix_hint: None,
                                fixed: false,
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
                                fix_hint: None,
                                fixed: fix,
                            });
                        }
                    }
                    Err(SpanError::PastEof { end, total }) => {
                        // Try tree-path recovery before giving up
                        let lang = detect_language(&entry.source_path);
                        let (recovered, tp_note) = if req.tree_path.is_empty() {
                            (None, "no tree_path set")
                        } else if lang.is_none() {
                            (None, "no grammar for source language")
                        } else {
                            let r =
                                resolve_tree_path(&source_content, &req.tree_path, lang.unwrap());
                            if r.is_some() {
                                (r, "")
                            } else {
                                (None, "tree_path resolution failed")
                            }
                        };

                        if let Some(new_span) = recovered {
                            let old_span = req.source_span;
                            let content_unchanged = req
                                .source_hash
                                .as_ref()
                                .and_then(|old_h| {
                                    hash_span(&source_content, new_span)
                                        .ok()
                                        .map(|(h, _)| h == *old_h)
                                })
                                .unwrap_or(false);

                            if content_unchanged {
                                let delta = new_span[0] as i64 - old_span[0] as i64;
                                if fix {
                                    req.source_span = new_span;
                                    if let Ok((h, a)) = hash_span(&source_content, new_span) {
                                        req.source_hash = Some(h);
                                        req.source_anchor = Some(a);
                                    }
                                    if let Some(l) = lang {
                                        req.tree_path =
                                            compute_tree_path(&source_content, new_span, l);
                                    }
                                    modified = true;
                                }
                                diagnostics.push(Diagnostic {
                                    file: entry.source_path.clone(),
                                    item_or_req: label,
                                    kind: DiagnosticKind::Shifted {
                                        from: old_span,
                                        to: new_span,
                                    },
                                    severity: Severity::Warning,
                                    message: format!(
                                        "span past EOF (end {end} > {total}), tree_path resolved, shifted by {delta:+} → [{}, {}]",
                                        new_span[0], new_span[1]
                                    ),
                                fix_hint: Some("liyi check --fix".into()),
                                fixed: fix,
                                });
                            } else {
                                if fix {
                                    req.source_span = new_span;
                                    if let Some(l) = lang {
                                        req.tree_path =
                                            compute_tree_path(&source_content, new_span, l);
                                    }
                                    modified = true;
                                }
                                diagnostics.push(Diagnostic {
                                    file: entry.source_path.clone(),
                                    item_or_req: label,
                                    kind: DiagnosticKind::Stale,
                                    severity: Severity::Warning,
                                    message: format!(
                                        "span past EOF (end {end} > {total}), tree_path resolved to [{}, {}] but content also changed (not auto-rehashed)",
                                        new_span[0], new_span[1]
                                    ),
                                fix_hint: None,
                                fixed: false,
                                });
                            }
                        } else {
                            let detail = if tp_note.is_empty() {
                                format!("span end {end} exceeds file length {total}")
                            } else {
                                format!("span end {end} exceeds file length {total} ({tp_note})")
                            };
                            diagnostics.push(Diagnostic {
                                file: entry.source_path.clone(),
                                item_or_req: label,
                                kind: DiagnosticKind::SpanPastEof {
                                    span: req.source_span,
                                    file_lines: total,
                                },
                                severity: Severity::Error,
                                message: detail,
                                fix_hint: Some("liyi check --fix".into()),
                                fixed: false,
                            });
                        }
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
                            fix_hint: None,
                            fixed: false,
                        });
                    }
                }
            }
        }
    }

    // Write back if --fix produced changes (skip if --dry-run).
    if fix && modified && !dry_run {
        let output = write_sidecar(&sidecar);
        let _ = fs::write(sidecar_path, output);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Detect cycles in a directed graph of requirement dependencies.
///
/// Returns a list of cycles, where each cycle is a Vec of requirement names
/// forming the cycle (e.g., `["A", "B", "A"]`).
fn detect_requirement_cycles(graph: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    use std::collections::HashSet;

    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = Vec::new();

    fn dfs(
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        on_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        on_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for next in neighbors {
                if !visited.contains(next.as_str()) {
                    dfs(next, graph, visited, on_stack, path, cycles);
                } else if on_stack.contains(next.as_str()) {
                    // Found a cycle — extract just the cycle portion.
                    let start_idx = path.iter().position(|n| n == next).unwrap();
                    let mut cycle: Vec<String> = path[start_idx..].to_vec();
                    cycle.push(next.clone()); // Close the cycle
                    cycles.push(cycle);
                }
            }
        }

        on_stack.remove(node);
        path.pop();
    }

    for node in graph.keys() {
        if !visited.contains(node.as_str()) {
            dfs(
                node,
                graph,
                &mut visited,
                &mut on_stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

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
