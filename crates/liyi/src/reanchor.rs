use std::path::{Path, PathBuf};

use crate::hashing::hash_span;
use crate::schema::migrate;
use crate::sidecar::{Spec, parse_sidecar, write_sidecar};
use crate::tree_path::{compute_tree_path, detect_language, resolve_tree_path};

const SIDECAR_SUFFIX: &str = ".liyi.jsonc";

/// Expand a list of file/directory paths into concrete `.liyi.jsonc` file
/// paths. If a path is a directory, walk it recursively (respecting
/// `.gitignore` and `.liyiignore`) and collect all sidecar files found.
/// If a path is a file, include it directly.
pub fn resolve_reanchor_targets(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut result: Vec<PathBuf> = Vec::new();
    for p in paths {
        if p.is_dir() {
            let walker = ignore::WalkBuilder::new(p)
                .add_custom_ignore_filename(".liyiignore")
                .build();
            for entry in walker {
                let entry = entry.map_err(|e| format!("walk error: {e}"))?;
                if entry.file_type().is_some_and(|ft| ft.is_file())
                    && let Some(name) = entry.path().file_name().and_then(|n| n.to_str())
                    && name.ends_with(SIDECAR_SUFFIX)
                {
                    result.push(entry.into_path());
                }
            }
        } else if p.is_file() {
            result.push(p.clone());
        } else {
            return Err(format!("path does not exist: {}", p.display()));
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

/// Re-hash source spans in a sidecar file.
///
/// If `do_migrate` is set, run schema migration and return.
/// If `target_item` + `target_span` are given, update only that item's span and rehash.
/// Otherwise, for every spec: if `tree_path` is non-empty and a tree-sitter
/// grammar is available, locate the item by structural identity and update the
/// span. Then recompute hash/anchor. When tree_path is empty or the language is
/// unsupported, fall back to re-hashing at the recorded span.
// @liyi:related tool-managed-fields
// @liyi:related tree-path-reanchor-behavior
// @liyi:related tree-path-empty-fallback
// @liyi:related fix-never-modifies-human-fields
// @liyi:related liyi-sidecar-naming-convention
pub fn run_reanchor(
    sidecar_path: &Path,
    target_item: Option<&str>,
    target_span: Option<[usize; 2]>,
    do_migrate: bool,
) -> Result<(), String> {
    let raw =
        std::fs::read_to_string(sidecar_path).map_err(|e| format!("cannot read sidecar: {e}"))?;

    if do_migrate {
        let mut sidecar = parse_sidecar(&raw)?;
        migrate(&mut sidecar)?;
        let out = write_sidecar(&sidecar);
        std::fs::write(sidecar_path, out).map_err(|e| format!("cannot write sidecar: {e}"))?;
        return Ok(());
    }

    let mut sidecar = parse_sidecar(&raw)?;

    let source_path = sidecar_path
        .to_str()
        .and_then(|s| s.strip_suffix(".liyi.jsonc"))
        .ok_or_else(|| "sidecar path does not end in .liyi.jsonc".to_string())?;

    let source_content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("cannot read source file {source_path}: {e}"))?;

    let lang = detect_language(Path::new(source_path));

    for spec in &mut sidecar.specs {
        match spec {
            Spec::Item(item) => {
                if let (Some(name), Some(span)) = (target_item, target_span) {
                    if item.item != name {
                        continue;
                    }
                    item.source_span = span;
                } else if target_item.is_some() || target_span.is_some() {
                    return Err("both --item and --span must be provided together".into());
                }

                // Tree-sitter span recovery: if tree_path is non-empty and
                // language is supported, locate item by structural identity.
                // If resolution fails (item renamed/deleted), keep the
                // existing span — hash_span below will detect the mismatch.
                if let (false, Some(l)) = (item.tree_path.is_empty(), lang)
                    && let Some(new_span) = resolve_tree_path(&source_content, &item.tree_path, l)
                {
                    item.source_span = new_span;
                }

                // Compute or update tree_path from the (possibly updated) span.
                if let Some(l) = lang {
                    let canonical = compute_tree_path(&source_content, item.source_span, l);
                    item.tree_path = canonical;
                }

                let (hash, anchor) = hash_span(&source_content, item.source_span)
                    .map_err(|e| format!("item \"{}\": {e}", item.item))?;
                item.source_hash = Some(hash);
                item.source_anchor = Some(anchor);
            }
            Spec::Requirement(req) => {
                if target_item.is_some() {
                    continue; // targeted mode only touches items
                }

                // Tree-sitter span recovery for requirements
                if let (false, Some(l)) = (req.tree_path.is_empty(), lang)
                    && let Some(new_span) = resolve_tree_path(&source_content, &req.tree_path, l)
                {
                    req.source_span = new_span;
                }

                if let Some(l) = lang {
                    let canonical = compute_tree_path(&source_content, req.source_span, l);
                    req.tree_path = canonical;
                }

                let (hash, anchor) = hash_span(&source_content, req.source_span)
                    .map_err(|e| format!("requirement \"{}\": {e}", req.requirement))?;
                req.source_hash = Some(hash);
                req.source_anchor = Some(anchor);
            }
        }
    }

    let out = write_sidecar(&sidecar);
    std::fs::write(sidecar_path, out).map_err(|e| format!("cannot write sidecar: {e}"))?;
    Ok(())
}
