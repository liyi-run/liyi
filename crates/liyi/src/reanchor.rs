use std::path::Path;

use crate::hashing::hash_span;
use crate::schema::migrate;
use crate::sidecar::{parse_sidecar, write_sidecar, Spec};

/// Re-hash source spans in a sidecar file.
///
/// If `do_migrate` is set, run schema migration and return.
/// If `target_item` + `target_span` are given, update only that item's span and rehash.
/// Otherwise, recompute hash/anchor for every spec from the current source.
pub fn run_reanchor(
    sidecar_path: &Path,
    target_item: Option<&str>,
    target_span: Option<[usize; 2]>,
    do_migrate: bool,
) -> Result<(), String> {
    let raw = std::fs::read_to_string(sidecar_path)
        .map_err(|e| format!("cannot read sidecar: {e}"))?;

    if do_migrate {
        let mut sidecar = parse_sidecar(&raw)?;
        migrate(&mut sidecar)?;
        let out = write_sidecar(&sidecar);
        std::fs::write(sidecar_path, out)
            .map_err(|e| format!("cannot write sidecar: {e}"))?;
        return Ok(());
    }

    let mut sidecar = parse_sidecar(&raw)?;

    let source_path = sidecar_path
        .to_str()
        .and_then(|s| s.strip_suffix(".liyi.jsonc"))
        .ok_or_else(|| "sidecar path does not end in .liyi.jsonc".to_string())?;

    let source_content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("cannot read source file {source_path}: {e}"))?;

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
                let (hash, anchor) = hash_span(&source_content, item.source_span)
                    .map_err(|e| format!("item \"{}\": {e}", item.item))?;
                item.source_hash = Some(hash);
                item.source_anchor = Some(anchor);
            }
            Spec::Requirement(req) => {
                if target_item.is_some() {
                    continue; // targeted mode only touches items
                }
                let (hash, anchor) = hash_span(&source_content, req.source_span)
                    .map_err(|e| format!("requirement \"{}\": {e}", req.requirement))?;
                req.source_hash = Some(hash);
                req.source_anchor = Some(anchor);
            }
        }
    }

    let out = write_sidecar(&sidecar);
    std::fs::write(sidecar_path, out)
        .map_err(|e| format!("cannot write sidecar: {e}"))?;
    Ok(())
}
