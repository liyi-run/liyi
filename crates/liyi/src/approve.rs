use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::hashing::hash_span;
use crate::reanchor::resolve_reanchor_targets;
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

/// Run `liyi approve` in batch mode (`--yes`).
///
/// Approves all (or filtered) items in the given sidecar files, setting
/// `reviewed: true` and reanchoring hashes.
pub fn approve_batch(
    paths: &[PathBuf],
    item_filter: Option<&str>,
    dry_run: bool,
) -> Result<Vec<ApproveResult>, ApproveError> {
    let targets = resolve_reanchor_targets(paths).map_err(|e| ApproveError::Parse(e))?;
    if targets.is_empty() {
        return Err(ApproveError::NoTargets);
    }

    let mut results = Vec::new();
    for sidecar_path in &targets {
        let result = approve_sidecar(sidecar_path, item_filter, dry_run, false)?;
        results.push(result);
    }
    Ok(results)
}

/// Run `liyi approve` in interactive mode.
///
/// For each unreviewed item, displays context and prompts the user.
pub fn approve_interactive(
    paths: &[PathBuf],
    item_filter: Option<&str>,
    dry_run: bool,
) -> Result<Vec<ApproveResult>, ApproveError> {
    let targets = resolve_reanchor_targets(paths).map_err(|e| ApproveError::Parse(e))?;
    if targets.is_empty() {
        return Err(ApproveError::NoTargets);
    }

    let mut results = Vec::new();
    for sidecar_path in &targets {
        let result = approve_sidecar(sidecar_path, item_filter, dry_run, true)?;
        results.push(result);
    }
    Ok(results)
}

/// Approve items in a single sidecar file.
fn approve_sidecar(
    sidecar_path: &Path,
    item_filter: Option<&str>,
    dry_run: bool,
    interactive: bool,
) -> Result<ApproveResult, ApproveError> {
    let sc_content = fs::read_to_string(sidecar_path)?;
    let mut sidecar = parse_sidecar(&sc_content).map_err(ApproveError::Parse)?;

    // Read source file for context display and hashing.
    let source_path = sidecar_path.with_file_name(&sidecar.source);
    let source_content = fs::read_to_string(&source_path).unwrap_or_default();

    let mut approved = 0usize;
    let mut skipped = 0usize;
    let mut rejected = 0usize;
    let mut modified = false;

    for spec in &mut sidecar.specs {
        if let Spec::Item(item) = spec {
            // Apply item name filter if specified.
            if let Some(filter) = item_filter {
                if item.item != filter {
                    continue;
                }
            }

            // Skip already reviewed items.
            if item.reviewed {
                skipped += 1;
                continue;
            }

            let decision = if interactive {
                // Display context and prompt.
                println!("─── {} ───", item.item);
                println!("Intent: {}", item.intent);
                println!(
                    "Span:   [{}, {}]",
                    item.source_span[0], item.source_span[1]
                );

                // Show source lines.
                let lines: Vec<&str> = source_content.lines().collect();
                let start = item.source_span[0].saturating_sub(1);
                let end = item.source_span[1].min(lines.len());
                if start < end {
                    println!();
                    for (i, line) in lines[start..end].iter().enumerate() {
                        println!("  {:>4} │ {}", start + i + 1, line);
                    }
                    println!();
                }

                prompt_user()
            } else {
                // Batch mode: auto-approve.
                Decision::Yes
            };

            match decision {
                Decision::Yes => {
                    if dry_run {
                        println!("would approve: {}", item.item);
                    } else {
                        item.reviewed = true;
                        // Reanchor: fill source_hash and source_anchor.
                        if let Ok((hash, anchor)) =
                            hash_span(&source_content, item.source_span)
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

    // Write back if modified.
    if modified && !dry_run {
        let output = write_sidecar(&sidecar);
        fs::write(sidecar_path, output)?;
    }

    Ok(ApproveResult {
        sidecar_path: sidecar_path.to_path_buf(),
        approved,
        skipped,
        rejected,
    })
}

/// User decision for interactive mode.
enum Decision {
    Yes,
    No,
    Skip,
}

/// Prompt the user for a decision.
fn prompt_user() -> Decision {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("approve? [y]es / [n]o / [s]kip: ");
        stdout.flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            return Decision::Skip;
        }

        match line.trim().to_lowercase().as_str() {
            "y" | "yes" => return Decision::Yes,
            "n" | "no" => return Decision::No,
            "s" | "skip" | "" => return Decision::Skip,
            _ => {
                println!("  (enter y, n, or s)");
            }
        }
    }
}
