use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::discovery::{find_repo_root, resolve_sidecar_targets};
use crate::git::{git_log_revisions, git_show};
use crate::hashing::hash_span;
use crate::markers::{SourceMarker, scan_markers};
use crate::sidecar::{Spec, parse_sidecar, write_sidecar};

/// Result of an approve operation on a single sidecar.
#[derive(Debug)]
pub struct ApproveResult {
    pub sidecar_path: PathBuf,
    pub approved: usize,
    pub skipped: usize,
    pub rejected: usize,
}

/// Error type for approve operations.
#[derive(Debug)]
pub enum ApproveError {
    Io(io::Error),
    Parse(String),
    NoTargets,
}

impl std::fmt::Display for ApproveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(e) => write!(f, "{e}"),
            Self::NoTargets => write!(f, "no .liyi.jsonc files found"),
        }
    }
}

impl From<io::Error> for ApproveError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// User decision for an approval candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Yes,
    No,
    Skip,
    /// Approve with edited intent text.
    Edit(String),
}

/// What kind of review this candidate requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateKind {
    /// Agent-inferred intent; no human confirmation yet.
    Unreviewed,
    /// Reviewed item whose source code changed (source_hash mismatch).
    StaleReviewed,
    /// Reviewed item whose related requirement text changed.
    ReqChanged {
        /// Name of the requirement that changed.
        requirement: String,
    },
}

/// Filter for which candidate kinds to collect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproveFilter {
    /// All candidate kinds.
    All,
    /// Only unreviewed items.
    UnreviewedOnly,
    /// Only stale-reviewed items.
    StaleOnly,
    /// Only requirement-changed items.
    ReqOnly,
}

/// A single item pending approval, with all context needed for display.
#[derive(Debug)]
pub struct ApprovalCandidate {
    /// Path to the sidecar file.
    pub sidecar_path: PathBuf,
    /// Display name of the source file (from the sidecar's `source` field).
    pub source_display: String,
    /// Index of this spec in the sidecar's specs array.
    pub spec_index: usize,
    /// Item name.
    pub item_name: String,
    /// Inferred intent text.
    pub intent: String,
    /// Source span [start, end] (1-indexed).
    pub source_span: [usize; 2],
    /// All lines of the source file: (line_number, line_content).
    pub source_lines: Vec<(usize, String)>,
    /// Index into `source_lines` where the reviewed span begins.
    pub span_offset: usize,
    /// Number of lines in the reviewed span.
    pub span_len: usize,
    /// Previously approved intent text from Git history, if available.
    pub prev_intent: Option<String>,
    /// What kind of review this candidate requires.
    pub kind: CandidateKind,
    /// Previous source text at the span (for StaleReviewed diffs).
    pub prev_source: Option<String>,
    /// Current source text at the span (for StaleReviewed diffs).
    pub current_source: Option<String>,
    /// Requirement name that changed (for ReqChanged display).
    pub changed_requirement: Option<String>,
    /// Old requirement text (for ReqChanged diff).
    pub old_requirement_text: Option<String>,
    /// New requirement text (for ReqChanged diff).
    pub new_requirement_text: Option<String>,
}

/// Look up the previously approved intent for an item by walking Git
/// history to find the most recent commit where the item was reviewed.
///
/// Walks up to 20 commits that touched the sidecar file (via `git log`),
/// checking each for a version where the item had `reviewed: true`.
fn lookup_prev_intent(
    repo_root: Option<&Path>,
    sidecar_path: &Path,
    item_name: &str,
    tree_path: &str,
) -> Option<String> {
    let root = repo_root?;
    let rel = sidecar_path.strip_prefix(root).ok()?;
    let rel_str = rel.to_str()?;
    let revisions = git_log_revisions(root, rel_str, 20);

    for rev in &revisions {
        let content = match git_show(root, rel_str, rev) {
            Some(c) => c,
            None => continue,
        };
        let sidecar = match parse_sidecar(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for spec in &sidecar.specs {
            if let Spec::Item(old_item) = spec {
                let matched = if !tree_path.is_empty() && !old_item.tree_path.is_empty() {
                    old_item.tree_path == tree_path
                } else {
                    old_item.item == item_name
                };
                if matched && old_item.reviewed {
                    return Some(old_item.intent.clone());
                }
            }
        }
    }
    None
}

/// Look up the source text at the time when `source_hash` was last valid,
/// by walking Git history on the source file.
///
/// Uses a two-pass strategy:
/// 1. **Fast path:** walk source history with the current span. Works when
///    the span hasn't been shifted by `check --fix`.
/// 2. **Slow path:** walk sidecar history to recover the pre-shift span,
///    then read the source at that revision. Handles the case where
///    `check --fix` shifted the span after the function moved.
///
/// Returns the source lines at the span from the historical version,
/// or `None` if git history is unavailable or no matching commit is found.
fn lookup_prev_source(
    repo_root: Option<&Path>,
    source_path: &Path,
    sidecar_path: &Path,
    item_name: &str,
    tree_path: &str,
    source_hash: &str,
    span: [usize; 2],
) -> Option<String> {
    let root = repo_root?;
    let source_rel = source_path.strip_prefix(root).ok()?.to_str()?;

    // Fast path: try source history with the current span.
    let source_revisions = git_log_revisions(root, source_rel, 20);
    for rev in &source_revisions {
        let content = match git_show(root, source_rel, rev) {
            Some(c) => c,
            None => continue,
        };
        if let Ok((hash, _)) = hash_span(&content, span)
            && hash == source_hash
        {
            let lines: Vec<&str> = content.lines().collect();
            let start = span[0].saturating_sub(1);
            let end = span[1].min(lines.len());
            return Some(lines[start..end].join("\n"));
        }
    }

    // Slow path: walk sidecar history to find the old span (before check --fix
    // shifted it), then read the source at that revision.
    let sidecar_rel = sidecar_path.strip_prefix(root).ok()?.to_str()?;
    let sidecar_revisions = git_log_revisions(root, sidecar_rel, 20);
    for rev in &sidecar_revisions {
        let sidecar_content = match git_show(root, sidecar_rel, rev) {
            Some(c) => c,
            None => continue,
        };
        let sidecar = match parse_sidecar(&sidecar_content) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for spec in &sidecar.specs {
            if let Spec::Item(item) = spec {
                let matched = if !tree_path.is_empty() && !item.tree_path.is_empty() {
                    item.tree_path == tree_path
                } else {
                    item.item == item_name
                };
                if !matched {
                    continue;
                }
                let old_span = item.source_span;
                if old_span == span {
                    continue; // same span — already tried in fast path
                }
                // Read source at this sidecar revision with the old span.
                let source_content = match git_show(root, source_rel, rev) {
                    Some(c) => c,
                    None => continue,
                };
                if let Ok((hash, _)) = hash_span(&source_content, old_span)
                    && hash == source_hash
                {
                    let lines: Vec<&str> = source_content.lines().collect();
                    let start = old_span[0].saturating_sub(1);
                    let end = old_span[1].min(lines.len());
                    return Some(lines[start..end].join("\n"));
                }
            }
        }
    }

    None
}

/// Collect approval candidates matching the given filter.
///
/// When `kind_filter` is `All`, collects unreviewed, stale-reviewed, and
/// req-changed items (in that order). Specific filters restrict to one kind.
// @liyi:related stale-reviewed-demands-human-judgment
pub fn collect_approval_candidates(
    paths: &[PathBuf],
    item_filter: Option<&str>,
    kind_filter: ApproveFilter,
) -> Result<Vec<ApprovalCandidate>, ApproveError> {
    let targets = resolve_sidecar_targets(paths).map_err(ApproveError::Parse)?;
    if targets.is_empty() {
        return Err(ApproveError::NoTargets);
    }

    // Try to locate the repo root for Git history lookups.
    let repo_root = targets.first().and_then(|p| find_repo_root(p));

    let collect_unreviewed =
        kind_filter == ApproveFilter::All || kind_filter == ApproveFilter::UnreviewedOnly;
    let collect_stale =
        kind_filter == ApproveFilter::All || kind_filter == ApproveFilter::StaleOnly;
    let collect_req = kind_filter == ApproveFilter::All || kind_filter == ApproveFilter::ReqOnly;

    let mut unreviewed_candidates = Vec::new();
    let mut stale_candidates = Vec::new();
    let mut req_candidates = Vec::new();

    for sidecar_path in &targets {
        let sc_content = fs::read_to_string(sidecar_path)?;
        let sidecar = parse_sidecar(&sc_content).map_err(ApproveError::Parse)?;

        let source_path = source_path_from_sidecar(sidecar_path)?;
        let source_content = fs::read_to_string(&source_path).unwrap_or_default();
        let all_lines: Vec<&str> = source_content.lines().collect();
        let source_markers = scan_markers(&source_content);

        // Build source_lines once per file (shared across candidates).
        let source_lines: Vec<(usize, String)> = all_lines
            .iter()
            .enumerate()
            .map(|(i, line)| (i + 1, line.to_string()))
            .collect();

        // Build requirement hash registry from the same sidecar set for
        // ReqChanged detection. Map requirement name → current source_hash.
        let req_hashes: std::collections::HashMap<&str, Option<&str>> = if collect_req {
            sidecar
                .specs
                .iter()
                .filter_map(|s| {
                    if let Spec::Requirement(r) = s {
                        Some((r.requirement.as_str(), r.source_hash.as_deref()))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        for (spec_index, spec) in sidecar.specs.iter().enumerate() {
            // @liyi:related approve-never-approves-requirements
            if let Spec::Item(item) = spec {
                if let Some(filter) = item_filter
                    && item.item != filter
                {
                    continue;
                }

                let span_start = item.source_span[0].saturating_sub(1);
                let span_end = item.source_span[1].min(all_lines.len());
                let span_offset = span_start;
                let span_len = span_end.saturating_sub(span_start);

                // Determine if this item is effectively reviewed.
                let has_source_intent = source_markers.iter().any(|m| {
                    matches!(m, SourceMarker::Intent { line, .. }
                        if *line >= item.source_span[0] && *line <= item.source_span[1])
                });
                let effectively_reviewed = item.reviewed || has_source_intent;

                if !effectively_reviewed && collect_unreviewed {
                    let prev_intent = lookup_prev_intent(
                        repo_root.as_deref(),
                        sidecar_path,
                        &item.item,
                        &item.tree_path,
                    );

                    unreviewed_candidates.push(ApprovalCandidate {
                        sidecar_path: sidecar_path.clone(),
                        source_display: sidecar.source.clone(),
                        spec_index,
                        item_name: item.item.clone(),
                        intent: item.intent.clone(),
                        source_span: item.source_span,
                        source_lines: source_lines.clone(),
                        span_offset,
                        span_len,
                        prev_intent,
                        kind: CandidateKind::Unreviewed,
                        prev_source: None,
                        current_source: None,
                        changed_requirement: None,
                        old_requirement_text: None,
                        new_requirement_text: None,
                    });
                }

                if effectively_reviewed
                    && collect_stale
                    && let Some(stored_hash) = &item.source_hash
                    && let Ok((current_hash, _)) = hash_span(&source_content, item.source_span)
                    && *stored_hash != current_hash
                {
                    // Extract current and previous source at the span.
                    let cur_src = all_lines[span_start..span_end.min(all_lines.len())].join("\n");
                    let prev_src = lookup_prev_source(
                        repo_root.as_deref(),
                        &source_path,
                        sidecar_path,
                        &item.item,
                        &item.tree_path,
                        stored_hash,
                        item.source_span,
                    );

                    stale_candidates.push(ApprovalCandidate {
                        sidecar_path: sidecar_path.clone(),
                        source_display: sidecar.source.clone(),
                        spec_index,
                        item_name: item.item.clone(),
                        intent: item.intent.clone(),
                        source_span: item.source_span,
                        source_lines: source_lines.clone(),
                        span_offset,
                        span_len,
                        prev_intent: None,
                        kind: CandidateKind::StaleReviewed,
                        prev_source: prev_src,
                        current_source: Some(cur_src),
                        changed_requirement: None,
                        old_requirement_text: None,
                        new_requirement_text: None,
                    });
                }

                // ReqChanged: check related edges against requirement registry.
                if effectively_reviewed
                    && collect_req
                    && let Some(related) = &item.related
                {
                    for (req_name, stored_hash) in related {
                        if let Some(stored_h) = stored_hash
                            && let Some(Some(cur)) = req_hashes.get(req_name.as_str())
                            && stored_h != cur
                        {
                            req_candidates.push(ApprovalCandidate {
                                sidecar_path: sidecar_path.clone(),
                                source_display: sidecar.source.clone(),
                                spec_index,
                                item_name: item.item.clone(),
                                intent: item.intent.clone(),
                                source_span: item.source_span,
                                source_lines: source_lines.clone(),
                                span_offset,
                                span_len,
                                prev_intent: None,
                                kind: CandidateKind::ReqChanged {
                                    requirement: req_name.clone(),
                                },
                                prev_source: None,
                                current_source: None,
                                changed_requirement: Some(req_name.clone()),
                                old_requirement_text: None,
                                new_requirement_text: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Ordering: unreviewed first, then stale-reviewed by file, then req-changed.
    let mut candidates = Vec::new();
    candidates.append(&mut unreviewed_candidates);
    candidates.append(&mut stale_candidates);
    candidates.append(&mut req_candidates);
    Ok(candidates)
}

/// Apply approval decisions to sidecars.
///
/// `decisions` is a slice parallel to the candidates returned by
/// `collect_approval_candidates`.
// @liyi:related reviewed-semantics
// @liyi:related reqchanged-orthogonal-to-reviewed
// @liyi:related reqchanged-demands-human-judgment
// @liyi:related stale-reviewed-demands-human-judgment
pub fn apply_approval_decisions(
    candidates: &[ApprovalCandidate],
    decisions: &[Decision],
    dry_run: bool,
) -> Result<Vec<ApproveResult>, ApproveError> {
    use std::collections::HashMap;

    // Group decisions by sidecar path, carrying the candidate kind.
    let mut per_sidecar: HashMap<&Path, Vec<(usize, &Decision, &CandidateKind)>> = HashMap::new();
    for (candidate, decision) in candidates.iter().zip(decisions.iter()) {
        per_sidecar
            .entry(candidate.sidecar_path.as_path())
            .or_default()
            .push((candidate.spec_index, decision, &candidate.kind));
    }

    let mut results = Vec::new();
    for (sidecar_path, item_decisions) in &per_sidecar {
        let sc_content = fs::read_to_string(sidecar_path)?;
        let mut sidecar = parse_sidecar(&sc_content).map_err(ApproveError::Parse)?;
        let source_path = source_path_from_sidecar(sidecar_path)?;
        let source_content = fs::read_to_string(&source_path).unwrap_or_default();

        let mut approved = 0usize;
        let mut skipped = 0usize;
        let mut rejected = 0usize;
        let mut modified = false;

        for &(spec_index, ref decision, ref kind) in item_decisions {
            if let Some(Spec::Item(item)) = sidecar.specs.get_mut(spec_index) {
                match decision {
                    Decision::Yes => {
                        if !dry_run {
                            match kind {
                                CandidateKind::Unreviewed | CandidateKind::StaleReviewed => {
                                    // Set reviewed and rehash.
                                    item.reviewed = true;
                                    if let Ok((hash, anchor)) =
                                        hash_span(&source_content, item.source_span)
                                    {
                                        item.source_hash = Some(hash);
                                        item.source_anchor = Some(anchor);
                                    }
                                }
                                CandidateKind::ReqChanged { requirement } => {
                                    // Refresh the related edge hash only.
                                    // Do not touch reviewed, intent, or source_hash.
                                    if let Some(related) = &mut item.related {
                                        // Set to null so `liyi check --fix` fills
                                        // in the current requirement hash.
                                        related.insert(requirement.clone(), None);
                                    }
                                }
                            }
                            modified = true;
                        }
                        approved += 1;
                    }
                    Decision::Edit(new_intent) => {
                        if !dry_run {
                            item.intent = new_intent.clone();
                            match kind {
                                CandidateKind::Unreviewed | CandidateKind::StaleReviewed => {
                                    item.reviewed = true;
                                    if let Ok((hash, anchor)) =
                                        hash_span(&source_content, item.source_span)
                                    {
                                        item.source_hash = Some(hash);
                                        item.source_anchor = Some(anchor);
                                    }
                                }
                                CandidateKind::ReqChanged { requirement } => {
                                    // Update intent and refresh the related edge.
                                    item.reviewed = true;
                                    if let Ok((hash, anchor)) =
                                        hash_span(&source_content, item.source_span)
                                    {
                                        item.source_hash = Some(hash);
                                        item.source_anchor = Some(anchor);
                                    }
                                    if let Some(related) = &mut item.related {
                                        related.insert(requirement.clone(), None);
                                    }
                                }
                            }
                            modified = true;
                        }
                        approved += 1;
                    }
                    Decision::No => {
                        if !dry_run {
                            match kind {
                                CandidateKind::Unreviewed => {
                                    // Explicitly mark as not reviewed.
                                    item.reviewed = false;
                                    modified = true;
                                }
                                CandidateKind::StaleReviewed | CandidateKind::ReqChanged { .. } => {
                                    // Leave unchanged — item stays stale/req-changed
                                    // as a todo marker.
                                }
                            }
                        }
                        rejected += 1;
                    }
                    Decision::Skip => {
                        skipped += 1;
                    }
                }
            }
        }

        if modified && !dry_run {
            let output = write_sidecar(&sidecar);
            fs::write(sidecar_path, output)?;
        }

        results.push(ApproveResult {
            sidecar_path: sidecar_path.to_path_buf(),
            approved,
            skipped,
            rejected,
        });
    }
    Ok(results)
}

/// Derive the source file path by stripping the `.liyi.jsonc` suffix.
fn source_path_from_sidecar(sidecar_path: &Path) -> Result<PathBuf, ApproveError> {
    let s = sidecar_path
        .to_str()
        .and_then(|s| s.strip_suffix(".liyi.jsonc"))
        .ok_or_else(|| {
            ApproveError::Parse(format!(
                "sidecar path does not end in .liyi.jsonc: {}",
                sidecar_path.display()
            ))
        })?;
    Ok(PathBuf::from(s))
}
