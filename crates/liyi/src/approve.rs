use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::discovery::{find_repo_root, resolve_sidecar_targets};
use crate::git::{git_log_revisions, git_show};
use crate::hashing::hash_span;
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

/// Collect all unreviewed items as approval candidates.
pub fn collect_approval_candidates(
    paths: &[PathBuf],
    item_filter: Option<&str>,
) -> Result<Vec<ApprovalCandidate>, ApproveError> {
    let targets = resolve_sidecar_targets(paths).map_err(ApproveError::Parse)?;
    if targets.is_empty() {
        return Err(ApproveError::NoTargets);
    }

    // Try to locate the repo root for Git history lookups.
    let repo_root = targets.first().and_then(|p| find_repo_root(p));

    let mut candidates = Vec::new();
    for sidecar_path in &targets {
        let sc_content = fs::read_to_string(sidecar_path)?;
        let sidecar = parse_sidecar(&sc_content).map_err(ApproveError::Parse)?;

        let source_path = source_path_from_sidecar(sidecar_path)?;
        let source_content = fs::read_to_string(&source_path).unwrap_or_default();
        let all_lines: Vec<&str> = source_content.lines().collect();

        for (spec_index, spec) in sidecar.specs.iter().enumerate() {
            // @liyi:related approve-never-approves-requirements
            if let Spec::Item(item) = spec {
                if let Some(filter) = item_filter
                    && item.item != filter
                {
                    continue;
                }
                if item.reviewed {
                    continue;
                }

                let span_start = item.source_span[0].saturating_sub(1);
                let span_end = item.source_span[1].min(all_lines.len());

                let source_lines: Vec<(usize, String)> = all_lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| (i + 1, line.to_string()))
                    .collect();

                let span_offset = span_start;
                let span_len = span_end.saturating_sub(span_start);

                let prev_intent = lookup_prev_intent(
                    repo_root.as_deref(),
                    sidecar_path,
                    &item.item,
                    &item.tree_path,
                );

                candidates.push(ApprovalCandidate {
                    sidecar_path: sidecar_path.clone(),
                    source_display: sidecar.source.clone(),
                    spec_index,
                    item_name: item.item.clone(),
                    intent: item.intent.clone(),
                    source_span: item.source_span,
                    source_lines,
                    span_offset,
                    span_len,
                    prev_intent,
                });
            }
        }
    }
    Ok(candidates)
}

/// Apply approval decisions to sidecars.
///
/// `decisions` is a slice parallel to the candidates returned by
/// `collect_approval_candidates`.
// @liyi:related reviewed-semantics
// @liyi:related reqchanged-orthogonal-to-reviewed
// @liyi:related reqchanged-demands-human-judgment
pub fn apply_approval_decisions(
    candidates: &[ApprovalCandidate],
    decisions: &[Decision],
    dry_run: bool,
) -> Result<Vec<ApproveResult>, ApproveError> {
    use std::collections::HashMap;

    // Group decisions by sidecar path.
    let mut per_sidecar: HashMap<&Path, Vec<(usize, &Decision)>> = HashMap::new();
    for (candidate, decision) in candidates.iter().zip(decisions.iter()) {
        per_sidecar
            .entry(candidate.sidecar_path.as_path())
            .or_default()
            .push((candidate.spec_index, decision));
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

        for &(spec_index, ref decision) in item_decisions {
            if let Some(Spec::Item(item)) = sidecar.specs.get_mut(spec_index) {
                match decision {
                    Decision::Yes => {
                        if !dry_run {
                            item.reviewed = true;
                            if let Ok((hash, anchor)) = hash_span(&source_content, item.source_span)
                            {
                                item.source_hash = Some(hash);
                                item.source_anchor = Some(anchor);
                            }
                            modified = true;
                        }
                        approved += 1;
                    }
                    Decision::Edit(new_intent) => {
                        if !dry_run {
                            item.intent = new_intent.clone();
                            item.reviewed = true;
                            if let Ok((hash, anchor)) = hash_span(&source_content, item.source_span)
                            {
                                item.source_hash = Some(hash);
                                item.source_anchor = Some(anchor);
                            }
                            modified = true;
                        }
                        approved += 1;
                    }
                    Decision::No => {
                        if !dry_run {
                            item.reviewed = false;
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
