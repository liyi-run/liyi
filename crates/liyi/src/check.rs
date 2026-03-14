use std::collections::{BTreeMap, HashMap, HashSet};
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
use crate::sidecar::{ItemSpec, RequirementSpec, Spec, parse_sidecar, write_sidecar};
use crate::tree_path::{
    compute_tree_path, detect_language, resolve_tree_path, resolve_tree_path_sibling_scan,
};

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
    emit_discovery_warnings(root, &disc.warnings, &mut diagnostics);

    // Shared source-content cache (avoids re-reading the same file).
    let mut source_cache: HashMap<PathBuf, String> = HashMap::new();

    // ------------------------------------------------------------------
    // Pass 1 — Requirement discovery (project-global)
    // ------------------------------------------------------------------
    let mut requirements =
        discover_requirements(&disc.all_files, &mut source_cache, &mut diagnostics);
    compute_requirement_hashes(&disc.all_files, &mut source_cache, &mut requirements);
    let source_related_refs = collect_source_related_refs(&disc.all_files, &mut source_cache);
    let (requirements_with_sidecar, requirements_referenced, req_dep_graph) =
        enrich_requirements_from_sidecars(&disc.sidecars, &mut requirements);

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
    emit_untracked_requirements(
        &requirements,
        &requirements_with_sidecar,
        &mut source_cache,
        &mut diagnostics,
    );
    emit_unreferenced_requirements(
        &requirements_with_sidecar,
        &requirements_referenced,
        &source_related_refs,
        &requirements,
        &mut diagnostics,
    );
    emit_cycle_diagnostics(&cycles, &requirements, &mut diagnostics);

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
// run_check helpers
// ---------------------------------------------------------------------------

fn emit_discovery_warnings(root: &Path, warnings: &[String], diagnostics: &mut Vec<Diagnostic>) {
    for w in warnings {
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
            span_start: None,
            annotation_line: None,
            requirement_text: None,
            intent: None,
        });
    }
}

/// Pass 1a: scan all source files for `@liyi:requirement` markers and build
/// the initial requirements map (hashes filled later).
fn discover_requirements(
    all_files: &[PathBuf],
    source_cache: &mut HashMap<PathBuf, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, RequirementRecord> {
    let mut requirements: HashMap<String, RequirementRecord> = HashMap::new();

    for file_path in all_files {
        let content = match read_cached(source_cache, file_path) {
            Some(c) => c,
            None => continue,
        };

        let markers = scan_markers(&content);
        for m in &markers {
            if let SourceMarker::Requirement { name, line } = m {
                if let Some(existing) = requirements.get(name) {
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
                        span_start: None,
                        annotation_line: None,
                        requirement_text: None,
                        intent: None,
                    });
                } else {
                    requirements.insert(
                        name.clone(),
                        RequirementRecord {
                            file: file_path.clone(),
                            line: *line,
                            hash: None,
                            computed_hash: None,
                        },
                    );
                }
            }
        }
    }

    requirements
}

/// Pass 1b: compute fresh hashes for requirement blocks from source so that
/// downstream related-edge checks detect cascading staleness in one pass.
fn compute_requirement_hashes(
    all_files: &[PathBuf],
    source_cache: &mut HashMap<PathBuf, String>,
    requirements: &mut HashMap<String, RequirementRecord>,
) {
    for file_path in all_files {
        let content = match read_cached(source_cache, file_path) {
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
}

/// Pass 1c: collect `@liyi:related` marker names from all source files so that
/// source-level references count toward requirement coverage.
fn collect_source_related_refs(
    all_files: &[PathBuf],
    source_cache: &mut HashMap<PathBuf, String>,
) -> HashSet<String> {
    let mut refs: HashSet<String> = HashSet::new();
    for file_path in all_files {
        let content = match read_cached(source_cache, file_path) {
            Some(c) => c,
            None => continue,
        };
        for m in scan_markers(&content) {
            if let SourceMarker::Related { name, .. } = m {
                refs.insert(name);
            }
        }
    }
    refs
}

/// Pass 1d: enrich requirement records with hashes from existing sidecars,
/// track which requirements have sidecar entries and which are referenced
/// by items, and build the dependency graph for cycle detection.
fn enrich_requirements_from_sidecars(
    sidecars: &[SidecarEntry],
    requirements: &mut HashMap<String, RequirementRecord>,
) -> (
    HashSet<String>,
    HashSet<String>,
    HashMap<String, Vec<String>>,
) {
    let mut requirements_with_sidecar: HashSet<String> = HashSet::new();
    let mut requirements_referenced: HashSet<String> = HashSet::new();
    let mut req_dep_graph: HashMap<String, Vec<String>> = HashMap::new();

    for entry in sidecars {
        let sc_content = match fs::read_to_string(&entry.sidecar_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let sidecar = match parse_sidecar(&sc_content) {
            Ok(s) => s,
            Err(_) => continue,
        };

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

    (
        requirements_with_sidecar,
        requirements_referenced,
        req_dep_graph,
    )
}

/// Post-pass: emit `Untracked` for requirements found in source but absent
/// from any sidecar.
fn emit_untracked_requirements(
    requirements: &HashMap<String, RequirementRecord>,
    requirements_with_sidecar: &HashSet<String>,
    source_cache: &mut HashMap<PathBuf, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const MAX_REQ_TEXT_CHARS: usize = 4096;
    for (name, rec) in requirements {
        if !requirements_with_sidecar.contains(name) {
            let req_text = read_cached(source_cache, &rec.file).and_then(|content| {
                let markers = scan_markers(&content);
                let spans = requirement_spans(&markers);
                spans.get(name).map(|span| {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = span[0].saturating_sub(1);
                    let end = span[1].min(lines.len());
                    let text = lines[start..end].join("\n");
                    if text.chars().count() > MAX_REQ_TEXT_CHARS {
                        let boundary = text
                            .char_indices()
                            .nth(MAX_REQ_TEXT_CHARS)
                            .map_or(text.len(), |(i, _)| i);
                        format!("{}\u{2026}[truncated]", &text[..boundary])
                    } else {
                        text
                    }
                })
            });
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
                span_start: None,
                annotation_line: Some(rec.line),
                requirement_text: req_text,
                intent: None,
            });
        }
    }
}

/// Post-pass: emit `ReqNoRelated` for requirements with sidecar entries that
/// no item references.
fn emit_unreferenced_requirements(
    requirements_with_sidecar: &HashSet<String>,
    requirements_referenced: &HashSet<String>,
    source_related_refs: &HashSet<String>,
    requirements: &HashMap<String, RequirementRecord>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for name in requirements_with_sidecar {
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
                span_start: None,
                annotation_line: Some(rec.line),
                requirement_text: None,
                intent: None,
            });
        }
    }
}

/// Post-pass: emit `RequirementCycle` errors for circular dependencies.
fn emit_cycle_diagnostics(
    cycles: &[Vec<String>],
    requirements: &HashMap<String, RequirementRecord>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for cycle in cycles {
        let cycle_display = cycle.join(" → ");
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
            span_start: None,
            annotation_line: None,
            requirement_text: None,
            intent: None,
        });
    }
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
                span_start: None,
                annotation_line: None,
                requirement_text: None,
                intent: None,
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
                span_start: None,
                annotation_line: None,
                requirement_text: None,
                intent: None,
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
            span_start: None,
            annotation_line: None,
            requirement_text: None,
            intent: None,
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
                        span_start: Some(item.source_span[0]),
                        annotation_line: None,
                        requirement_text: None,
                        intent: Some(item.intent.clone()),
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
                                span_start: Some(item.source_span[0]),
                                annotation_line: None,
                                requirement_text: None,
                                intent: Some(item.intent.clone()),
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
                        span_start: Some(req.source_span[0]),
                        annotation_line: None,
                        requirement_text: None,
                        intent: None,
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
            span_start: None,
            annotation_line: None,
            requirement_text: None,
            intent: None,
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

                // a. Hash the span (with tree_path recovery, shift detection)
                if check_item_hash(
                    &entry.source_path,
                    &label,
                    item,
                    &source_content,
                    &source_markers,
                    fix,
                    diagnostics,
                ) {
                    modified = true;
                }

                // b. Review status
                check_review_status(
                    &entry.source_path,
                    &label,
                    item,
                    &source_markers,
                    diagnostics,
                );

                // c. Trivial / ignore markers and sidecar =trivial sentinel
                check_trivial_ignore(
                    &entry.source_path,
                    &label,
                    item,
                    &source_markers,
                    diagnostics,
                );

                // d. Related requirements + source related sync + null hash fill
                let related_modified = check_related_edges(
                    &entry.source_path,
                    &label,
                    item,
                    &source_markers,
                    requirements,
                    fix,
                    diagnostics,
                );
                if related_modified {
                    modified = true;
                }
            }
            Spec::Requirement(req) => {
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

                if check_requirement_hash(
                    &entry.source_path,
                    req,
                    &source_content,
                    fix,
                    diagnostics,
                ) {
                    modified = true;
                }
            }
        }
    }

    // Strip _hints when --fix is active (hints are transient scaffold aids).
    // @liyi:related hints-are-ephemeral
    if fix && !dry_run {
        for spec in &mut sidecar.specs {
            if let Spec::Item(item) = spec
                && item._hints.is_some()
            {
                item._hints = None;
                modified = true;
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
// check_sidecar helpers — requirement hash check
// ---------------------------------------------------------------------------

/// Check a requirement spec's hash freshness with tree_path recovery on
/// PastEof.  Returns true if `--fix` modified the spec.
fn check_requirement_hash(
    file: &Path,
    req: &mut RequirementSpec,
    source_content: &str,
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;
    let label = req.requirement.clone();

    match hash_span(source_content, req.source_span) {
        Ok((computed_hash, computed_anchor)) => {
            let is_current = req.source_hash.as_ref() == Some(&computed_hash);

            if is_current {
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label,
                    kind: DiagnosticKind::Current,
                    severity: Severity::Info,
                    message: "requirement hash matches".into(),
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(req.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: None,
                });
            } else {
                if fix {
                    req.source_hash = Some(computed_hash);
                    req.source_anchor = Some(computed_anchor);
                    modified = true;
                }
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label,
                    kind: DiagnosticKind::Stale,
                    severity: Severity::Warning,
                    message: "requirement hash mismatch or missing".into(),
                    fix_hint: None,
                    fixed: fix,
                    span_start: Some(req.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: None,
                });
            }
        }
        Err(SpanError::PastEof { end, total }) => {
            let lang = detect_language(file);
            let (recovered, tp_note) = if req.tree_path.is_empty() {
                (None, "no tree_path set")
            } else if lang.is_none() {
                (None, "no grammar for source language")
            } else {
                let r = resolve_tree_path(source_content, &req.tree_path, lang.unwrap());
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
                        hash_span(source_content, new_span)
                            .ok()
                            .map(|(h, _)| h == *old_h)
                    })
                    .unwrap_or(false);

                if content_unchanged {
                    let delta = new_span[0] as i64 - old_span[0] as i64;
                    if fix {
                        req.source_span = new_span;
                        if let Ok((h, a)) = hash_span(source_content, new_span) {
                            req.source_hash = Some(h);
                            req.source_anchor = Some(a);
                        }
                        if let Some(l) = lang {
                            let canonical = compute_tree_path(source_content, new_span, l);
                            if !canonical.is_empty() {
                                req.tree_path = canonical;
                            }
                        }
                        modified = true;
                    }
                    diagnostics.push(Diagnostic {
                        file: file.to_path_buf(),
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
                        span_start: Some(req.source_span[0]),
                        annotation_line: None,
                        requirement_text: None,
                        intent: None,
                    });
                } else {
                    if fix {
                        req.source_span = new_span;
                        if let Some(l) = lang {
                            let canonical = compute_tree_path(source_content, new_span, l);
                            if !canonical.is_empty() {
                                req.tree_path = canonical;
                            }
                        }
                        modified = true;
                    }
                    diagnostics.push(Diagnostic {
                        file: file.to_path_buf(),
                        item_or_req: label,
                        kind: DiagnosticKind::Stale,
                        severity: Severity::Warning,
                        message: format!(
                            "span past EOF (end {end} > {total}), tree_path resolved to [{}, {}] but content also changed (not auto-rehashed)",
                            new_span[0], new_span[1]
                        ),
                        fix_hint: None,
                        fixed: false,
                        span_start: Some(req.source_span[0]),
                        annotation_line: None,
                        requirement_text: None,
                        intent: None,
                    });
                }
            } else {
                let detail = if tp_note.is_empty() {
                    format!("span end {end} exceeds file length {total}")
                } else {
                    format!("span end {end} exceeds file length {total} ({tp_note})")
                };
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label,
                    kind: DiagnosticKind::SpanPastEof {
                        span: req.source_span,
                        file_lines: total,
                    },
                    severity: Severity::Error,
                    message: detail,
                    fix_hint: Some("liyi check --fix".into()),
                    fixed: false,
                    span_start: Some(req.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: None,
                });
            }
        }
        Err(SpanError::Inverted { .. } | SpanError::Empty) => {
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
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
                span_start: None,
                annotation_line: None,
                requirement_text: None,
                intent: None,
            });
        }
    }

    modified
}

// ---------------------------------------------------------------------------
// check_sidecar helpers — item hash checking
// ---------------------------------------------------------------------------

/// Hash-check an item spec: verify `source_hash`, attempt tree_path recovery,
/// sibling scan, and shift heuristic when stale.  Returns `true` when the spec
/// was modified (and the sidecar must be rewritten).
#[allow(clippy::too_many_arguments)]
fn check_item_hash(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_content: &str,
    source_markers: &[SourceMarker],
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;

    match hash_span(source_content, item.source_span) {
        Ok((computed_hash, _computed_anchor)) => {
            let is_current = item.source_hash.as_ref() == Some(&computed_hash);

            if is_current {
                // CURRENT
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Current,
                    severity: Severity::Info,
                    message: "hash matches".into(),
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(item.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            } else if item.source_hash.is_none() {
                modified |=
                    handle_missing_hash(file, label, item, source_content, fix, diagnostics);
            } else {
                modified |= handle_hash_mismatch(
                    file,
                    label,
                    item,
                    source_content,
                    source_markers,
                    fix,
                    diagnostics,
                );
            }
        }
        Err(SpanError::PastEof { end, total }) => {
            modified |= handle_past_eof(
                file,
                label,
                item,
                source_content,
                source_markers,
                end,
                total,
                fix,
                diagnostics,
            );
        }
        Err(SpanError::Inverted { .. } | SpanError::Empty) => {
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
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
                span_start: None,
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        }
    }

    modified
}

/// Handle an item with no `source_hash` — try tree_path recovery, then hash.
fn handle_missing_hash(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_content: &str,
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;
    let lang = detect_language(file);
    let recovered_span = if !item.tree_path.is_empty()
        && let Some(l) = lang
        && let Some(tp_span) = resolve_tree_path(source_content, &item.tree_path, l)
    {
        Some(tp_span)
    } else {
        None
    };

    if fix {
        if let Some(tp_span) = recovered_span {
            item.source_span = tp_span;
        }
        if let Ok((fix_hash, fix_anchor)) = hash_span(source_content, item.source_span) {
            item.source_hash = Some(fix_hash);
            item.source_anchor = Some(fix_anchor);
        }
        if let Some(l) = lang {
            let canonical = compute_tree_path(source_content, item.source_span, l);
            if !canonical.is_empty() {
                item.tree_path = canonical;
            }
        }
        // Source changed since last review — clear reviewed so a human re-confirms intent.
        item.reviewed = false;
        modified = true;
    }
    diagnostics.push(Diagnostic {
        file: file.to_path_buf(),
        item_or_req: label.to_string(),
        kind: DiagnosticKind::Stale,
        severity: Severity::Warning,
        message: "missing source_hash".into(),
        fix_hint: Some("liyi check --fix".into()),
        fixed: fix,
        span_start: Some(item.source_span[0]),
        annotation_line: None,
        requirement_text: None,
        intent: Some(item.intent.clone()),
    });
    modified
}

/// Handle a hash mismatch: try tree_path → sibling scan → shift heuristic.
#[allow(clippy::too_many_arguments)]
fn handle_hash_mismatch(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_content: &str,
    source_markers: &[SourceMarker],
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;
    let lang = detect_language(file);

    // Try tree_path-based recovery, tracking why it may not be available.
    let (tree_path_recovered, tree_path_note) = if item.tree_path.is_empty() {
        (None, "no tree_path set")
    } else if lang.is_none() {
        (None, "no grammar for source language")
    } else {
        let resolved = resolve_tree_path(source_content, &item.tree_path, lang.unwrap());
        if resolved.is_some() {
            (resolved, "")
        } else {
            (None, "tree_path resolution failed")
        }
    };

    if let Some(new_span) = tree_path_recovered {
        modified |= handle_tree_path_resolved(
            file,
            label,
            item,
            source_content,
            source_markers,
            new_span,
            lang,
            fix,
            diagnostics,
        );
    } else if let Some(l) = lang
        && let Some(sibling) = resolve_tree_path_sibling_scan(
            source_content,
            &item.tree_path,
            l,
            item.source_hash.as_ref().unwrap(),
        )
    {
        // tree_path resolution failed but sibling scan found the element.
        let old_span = item.source_span;
        let delta = sibling.span[0] as i64 - old_span[0] as i64;
        if fix {
            item.source_span = sibling.span;
            item.tree_path = sibling.updated_tree_path;
            if let Ok((h, a)) = hash_span(source_content, sibling.span) {
                item.source_hash = Some(h);
                item.source_anchor = Some(a);
            }
            modified = true;
        }
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            item_or_req: label.to_string(),
            kind: DiagnosticKind::Shifted {
                from: old_span,
                to: sibling.span,
            },
            severity: Severity::Warning,
            message: format!(
                "sibling scan matched, span shifted by {delta:+} → [{}, {}]",
                sibling.span[0], sibling.span[1]
            ),
            fix_hint: Some("liyi check --fix".into()),
            fixed: fix,
            span_start: Some(item.source_span[0]),
            annotation_line: None,
            requirement_text: None,
            intent: Some(item.intent.clone()),
        });
    } else {
        // Fallback to shift heuristic
        let expected = item.source_hash.as_ref().unwrap();
        match detect_shift(source_content, item.source_span, expected) {
            ShiftResult::Shifted { delta, new_span } => {
                let old_span = item.source_span;
                if fix {
                    item.source_span = new_span;
                    if let Ok((h, a)) = hash_span(source_content, new_span) {
                        item.source_hash = Some(h);
                        item.source_anchor = Some(a);
                    }
                    if let Some(l) = lang {
                        let canonical = compute_tree_path(source_content, new_span, l);
                        if !canonical.is_empty() {
                            item.tree_path = canonical;
                        }
                    }
                    modified = true;
                }
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
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
                    span_start: Some(item.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            }
            ShiftResult::Stale => {
                let detail = if tree_path_note.is_empty() {
                    "hash mismatch, could not relocate".to_string()
                } else {
                    format!("hash mismatch, could not relocate ({tree_path_note})")
                };
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Stale,
                    severity: Severity::Warning,
                    message: detail,
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(item.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            }
        }
    }
    modified
}

/// Handle tree_path-resolved span: pure shift vs content drift vs sibling.
#[allow(clippy::too_many_arguments)]
fn handle_tree_path_resolved(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_content: &str,
    source_markers: &[SourceMarker],
    new_span: [usize; 2],
    lang: Option<crate::tree_path::Language>,
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;
    let old_span = item.source_span;
    let old_hash = item.source_hash.as_ref().unwrap();
    let content_unchanged = hash_span(source_content, new_span)
        .map(|(h, _)| h == *old_hash)
        .unwrap_or(false);

    if new_span != old_span && content_unchanged {
        // Pure shift — content intact, only position changed.
        let delta = new_span[0] as i64 - old_span[0] as i64;
        if fix {
            item.source_span = new_span;
            if let Ok((h, a)) = hash_span(source_content, new_span) {
                item.source_hash = Some(h);
                item.source_anchor = Some(a);
            }
            if let Some(l) = lang {
                let canonical = compute_tree_path(source_content, new_span, l);
                if !canonical.is_empty() {
                    item.tree_path = canonical;
                }
            }
            modified = true;
        }
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            item_or_req: label.to_string(),
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
            span_start: Some(item.source_span[0]),
            annotation_line: None,
            requirement_text: None,
            intent: Some(item.intent.clone()),
        });
    } else if let Some(l) = lang
        && let Some(sibling) =
            resolve_tree_path_sibling_scan(source_content, &item.tree_path, l, old_hash)
    {
        // Hash-based sibling scan found the element at a different array index.
        let delta = sibling.span[0] as i64 - old_span[0] as i64;
        if fix {
            item.source_span = sibling.span;
            item.tree_path = sibling.updated_tree_path;
            if let Ok((h, a)) = hash_span(source_content, sibling.span) {
                item.source_hash = Some(h);
                item.source_anchor = Some(a);
            }
            modified = true;
        }
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            item_or_req: label.to_string(),
            kind: DiagnosticKind::Shifted {
                from: old_span,
                to: sibling.span,
            },
            severity: Severity::Warning,
            message: format!(
                "sibling scan matched, span shifted by {delta:+} → [{}, {}]",
                sibling.span[0], sibling.span[1]
            ),
            fix_hint: Some("liyi check --fix".into()),
            fixed: fix,
            span_start: Some(item.source_span[0]),
            annotation_line: None,
            requirement_text: None,
            intent: Some(item.intent.clone()),
        });
    } else {
        // Content at tree_path location has changed.
        // For reviewed specs (`@liyi:intent` in source): update span but do NOT
        // rehash — the stale hash signals that intent review is needed.
        // For unreviewed specs: rehash is safe.
        let effectively_reviewed = item.reviewed
            || source_markers.iter().any(|m| {
                matches!(m, SourceMarker::Intent { line, .. } if *line >= new_span[0] && *line <= new_span[1])
            });
        if fix {
            if new_span != old_span {
                item.source_span = new_span;
                if let Some(l) = lang {
                    let canonical = compute_tree_path(source_content, new_span, l);
                    if !canonical.is_empty() {
                        item.tree_path = canonical;
                    }
                }
            }
            if !effectively_reviewed && let Ok((h, a)) = hash_span(source_content, new_span) {
                item.source_hash = Some(h);
                item.source_anchor = Some(a);
            }
            modified = true;
        }
        if effectively_reviewed {
            let msg = if new_span != old_span {
                format!(
                    "source changed and shifted → [{}, {}] (tree_path resolved, not auto-rehashed — reviewed)",
                    new_span[0], new_span[1]
                )
            } else {
                "source changed at tree_path location (not auto-rehashed — reviewed)".into()
            };
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
                kind: DiagnosticKind::Stale,
                severity: Severity::Warning,
                message: msg,
                fix_hint: None,
                fixed: false,
                span_start: Some(item.source_span[0]),
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        } else {
            let msg = if new_span != old_span {
                format!(
                    "source changed and shifted → [{}, {}] (tree_path resolved, auto-rehashed — unreviewed)",
                    new_span[0], new_span[1]
                )
            } else {
                "source changed at tree_path location (auto-rehashed — unreviewed)".into()
            };
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
                kind: DiagnosticKind::Stale,
                severity: Severity::Warning,
                message: msg,
                fix_hint: Some("liyi check --fix".into()),
                fixed: fix,
                span_start: Some(item.source_span[0]),
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        }
    }
    modified
}

/// Handle `SpanError::PastEof`: try tree_path recovery, distinguish pure shift
/// from content drift, emit appropriate diagnostics.
#[allow(clippy::too_many_arguments)]
fn handle_past_eof(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_content: &str,
    source_markers: &[SourceMarker],
    end: usize,
    total: usize,
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;
    let lang = detect_language(file);
    let (recovered, tp_note) = if item.tree_path.is_empty() {
        (None, "no tree_path set")
    } else if lang.is_none() {
        (None, "no grammar for source language")
    } else {
        let r = resolve_tree_path(source_content, &item.tree_path, lang.unwrap());
        if r.is_some() {
            (r, "")
        } else {
            (None, "tree_path resolution failed")
        }
    };

    if let Some(new_span) = recovered {
        let old_span = item.source_span;
        let content_unchanged = item
            .source_hash
            .as_ref()
            .and_then(|old_h| {
                hash_span(source_content, new_span)
                    .ok()
                    .map(|(h, _)| h == *old_h)
            })
            .unwrap_or(false);

        if content_unchanged {
            let delta = new_span[0] as i64 - old_span[0] as i64;
            if fix {
                item.source_span = new_span;
                if let Ok((h, a)) = hash_span(source_content, new_span) {
                    item.source_hash = Some(h);
                    item.source_anchor = Some(a);
                }
                if let Some(l) = lang {
                    let canonical = compute_tree_path(source_content, new_span, l);
                    if !canonical.is_empty() {
                        item.tree_path = canonical;
                    }
                }
                modified = true;
            }
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
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
                span_start: Some(item.source_span[0]),
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        } else {
            // Content also changed — relocate span.
            let effectively_reviewed = item.reviewed
                || source_markers.iter().any(|m| {
                    matches!(m, SourceMarker::Intent { line, .. } if *line >= new_span[0] && *line <= new_span[1])
                });
            if fix {
                item.source_span = new_span;
                if let Some(l) = lang {
                    let canonical = compute_tree_path(source_content, new_span, l);
                    if !canonical.is_empty() {
                        item.tree_path = canonical;
                    }
                }
                if !effectively_reviewed && let Ok((h, a)) = hash_span(source_content, new_span) {
                    item.source_hash = Some(h);
                    item.source_anchor = Some(a);
                }
                modified = true;
            }
            if effectively_reviewed {
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Stale,
                    severity: Severity::Warning,
                    message: format!(
                        "span past EOF (end {end} > {total}), tree_path resolved to [{}, {}] but content also changed (not auto-rehashed — reviewed)",
                        new_span[0], new_span[1]
                    ),
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(item.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            } else {
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Stale,
                    severity: Severity::Warning,
                    message: format!(
                        "span past EOF (end {end} > {total}), tree_path resolved to [{}, {}] (auto-rehashed — unreviewed)",
                        new_span[0], new_span[1]
                    ),
                    fix_hint: Some("liyi check --fix".into()),
                    fixed: fix,
                    span_start: Some(item.source_span[0]),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            }
        }
    } else {
        let detail = if tp_note.is_empty() {
            format!("span end {end} exceeds file length {total}")
        } else {
            format!("span end {end} exceeds file length {total} ({tp_note})")
        };
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            item_or_req: label.to_string(),
            kind: DiagnosticKind::SpanPastEof {
                span: item.source_span,
                file_lines: total,
            },
            severity: Severity::Error,
            message: detail,
            fix_hint: Some("liyi check --fix".into()),
            fixed: false,
            span_start: Some(item.source_span[0]),
            annotation_line: None,
            requirement_text: None,
            intent: Some(item.intent.clone()),
        });
    }
    modified
}

// ---------------------------------------------------------------------------
// check_sidecar helpers — item sub-checks
// ---------------------------------------------------------------------------

/// Check review status: emit `Unreviewed` if neither `reviewed: true` in the
/// sidecar nor `@liyi:intent` in source within the item's span.
fn check_review_status(
    file: &Path,
    label: &str,
    item: &ItemSpec,
    source_markers: &[SourceMarker],
    diagnostics: &mut Vec<Diagnostic>,
) {
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
            file: file.to_path_buf(),
            item_or_req: label.to_string(),
            kind: DiagnosticKind::Unreviewed,
            severity: Severity::Warning,
            message: "not reviewed".into(),
            fix_hint: None,
            fixed: false,
            span_start: Some(item.source_span[0]),
            annotation_line: None,
            requirement_text: None,
            intent: Some(item.intent.clone()),
        });
    }
}

/// Check trivial/ignore source markers and the sidecar `=trivial` sentinel.
fn check_trivial_ignore(
    file: &Path,
    label: &str,
    item: &ItemSpec,
    source_markers: &[SourceMarker],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span_start = item.source_span[0];
    let span_end = item.source_span[1];

    for m in source_markers {
        match m {
            SourceMarker::Trivial { line }
                if *line >= span_start.saturating_sub(1) && *line <= span_end =>
            {
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Trivial,
                    severity: Severity::Info,
                    message: "marked \x40liyi:trivial".into(),
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(span_start),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            }
            SourceMarker::Ignore { line, .. }
                if *line >= span_start.saturating_sub(1) && *line <= span_end =>
            {
                diagnostics.push(Diagnostic {
                    file: file.to_path_buf(),
                    item_or_req: label.to_string(),
                    kind: DiagnosticKind::Ignored,
                    severity: Severity::Info,
                    message: "marked \x40liyi:ignore".into(),
                    fix_hint: None,
                    fixed: false,
                    span_start: Some(span_start),
                    annotation_line: None,
                    requirement_text: None,
                    intent: Some(item.intent.clone()),
                });
            }
            _ => {}
        }
    }

    // Sidecar "=trivial" sentinel
    if item.intent == "=trivial" {
        let has_nontrivial = source_markers.iter().any(|m| {
            matches!(m, SourceMarker::Nontrivial { line }
                if *line >= span_start.saturating_sub(1) && *line <= span_end)
        });
        if has_nontrivial {
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
                kind: DiagnosticKind::ConflictingTriviality,
                severity: Severity::Error,
                message: "\x40liyi:nontrivial in source conflicts with \"=trivial\" in sidecar"
                    .into(),
                fix_hint: None,
                fixed: false,
                span_start: Some(span_start),
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        } else {
            diagnostics.push(Diagnostic {
                file: file.to_path_buf(),
                item_or_req: label.to_string(),
                kind: DiagnosticKind::Trivial,
                severity: Severity::Info,
                message: "intent \"=trivial\"".into(),
                fix_hint: None,
                fixed: false,
                span_start: Some(span_start),
                annotation_line: None,
                requirement_text: None,
                intent: Some(item.intent.clone()),
            });
        }
    }
}

/// Check related-requirement edges, sync source `@liyi:related` markers into
/// sidecar, and fill null hashes on existing edges.  Returns true if fixups
/// modified the item.
fn check_related_edges(
    file: &Path,
    label: &str,
    item: &mut ItemSpec,
    source_markers: &[SourceMarker],
    requirements: &HashMap<String, RequirementRecord>,
    fix: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut modified = false;

    // Validate existing related edges
    if let Some(ref related) = item.related {
        for (req_name, stored_hash) in related {
            match requirements.get(req_name) {
                None => {
                    diagnostics.push(Diagnostic {
                        file: file.to_path_buf(),
                        item_or_req: label.to_string(),
                        kind: DiagnosticKind::UnknownRequirement {
                            name: req_name.clone(),
                        },
                        severity: Severity::Error,
                        message: format!("related requirement \"{req_name}\" not found"),
                        fix_hint: None,
                        fixed: false,
                        span_start: Some(item.source_span[0]),
                        annotation_line: None,
                        requirement_text: None,
                        intent: Some(item.intent.clone()),
                    });
                }
                Some(rec) => {
                    // Compare against computed_hash (fresh from source) for
                    // cascading staleness detection; fall back to sidecar hash.
                    let current_req_hash = rec.computed_hash.as_ref().or(rec.hash.as_ref());
                    if let (Some(sh), Some(rh)) = (stored_hash.as_ref(), current_req_hash)
                        && sh != rh
                    {
                        // @liyi:related reqchanged-orthogonal-to-reviewed
                        // @liyi:related reqchanged-demands-human-judgment
                        diagnostics.push(Diagnostic {
                            file: file.to_path_buf(),
                            item_or_req: label.to_string(),
                            kind: DiagnosticKind::ReqChanged {
                                requirement: req_name.clone(),
                            },
                            severity: Severity::Warning,
                            message: format!("requirement \"{req_name}\" has changed"),
                            fix_hint: None,
                            fixed: false,
                            span_start: Some(item.source_span[0]),
                            annotation_line: None,
                            requirement_text: None,
                            intent: Some(item.intent.clone()),
                        });
                    }
                }
            }
        }
    }

    // Source `@liyi:related` markers missing from sidecar
    let span_start = item.source_span[0];
    let span_end = item.source_span[1];
    for m in source_markers {
        if let SourceMarker::Related { name, line } = m {
            // Include doc-comment lines immediately before the span
            if *line >= span_start.saturating_sub(5) && *line <= span_end {
                let has_edge = item.related.as_ref().is_some_and(|r| r.contains_key(name));
                if !has_edge {
                    if fix {
                        let related = item.related.get_or_insert_with(BTreeMap::new);
                        let hash_val = requirements.get(name).and_then(|rec| rec.hash.clone());
                        related.insert(name.clone(), hash_val);
                        modified = true;
                    }
                    diagnostics.push(Diagnostic {
                        file: file.to_path_buf(),
                        item_or_req: label.to_string(),
                        kind: DiagnosticKind::MissingRelatedEdge {
                            name: name.clone(),
                        },
                        severity: Severity::Error,
                        message: format!(
                            "source has \x40liyi:related \"{name}\" but sidecar is missing the related edge"
                        ),
                        fix_hint: None,
                        fixed: fix,
                        span_start: Some(span_start),
                        annotation_line: Some(*line),
                        requirement_text: None,
                        intent: Some(item.intent.clone()),
                    });
                }
            }
        }
    }

    // Fill null hashes on existing related edges
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

    modified
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
