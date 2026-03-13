use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A discovered sidecar and its associated source file.
pub struct SidecarEntry {
    pub sidecar_path: PathBuf,
    pub source_path: PathBuf,
    pub repo_relative_source: String,
}

/// Walk results.
pub struct DiscoveryResult {
    pub sidecars: Vec<SidecarEntry>,
    /// All non-ignored files (for pass 1 marker scanning).
    pub all_files: Vec<PathBuf>,
    /// Ambiguous sidecar warnings, etc.
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Filename suffix for 立意 sidecar files.
///
/// <!-- @立意:有关 liyi-sidecar-naming-convention -->
const SIDECAR_SUFFIX: &str = ".liyi.jsonc";

/// Walk up from `from` looking for a `.git/` directory.
/// Returns the parent of `.git/` (i.e. the repo root).
///
/// Respects `GIT_DISCOVERY_ACROSS_FILESYSTEM`. When the variable is unset or
/// `"0"`, the search stops at filesystem boundaries (different `st_dev`),
/// matching Git's default behaviour. Set the variable to `"1"` to allow
/// crossing mount points.
pub fn find_repo_root(from: &Path) -> Option<PathBuf> {
    let cross_fs = std::env::var("GIT_DISCOVERY_ACROSS_FILESYSTEM")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

    let mut dir = if from.is_file() {
        from.parent()?.to_path_buf()
    } else {
        from.to_path_buf()
    };

    #[cfg(unix)]
    let start_dev = std::fs::metadata(&dir).ok().map(|m| m.dev());

    loop {
        if dir.join(".git").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
        // Stop at filesystem boundaries unless explicitly allowed.
        #[cfg(unix)]
        if !cross_fs
            && let Some(start) = start_dev
            && let Ok(meta) = std::fs::metadata(&dir)
            && meta.dev() != start
        {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Walk the file tree rooted at `root`, collecting sidecar entries and all
/// non-ignored files.
///
/// If `scope_paths` is non-empty, only sidecars whose `source_path` falls
/// under one of the given paths are included (pass 2 scoping).
// @liyi:related liyi-sidecar-naming-convention
pub fn discover(root: &Path, scope_paths: &[PathBuf]) -> DiscoveryResult {
    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut sidecar_paths: Vec<PathBuf> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Build walker: respects .gitignore cascading, add .liyiignore support.
    // Walk hidden directories (e.g. .github/) — the ignore crate skips them
    // by default, but sidecars and source markers can live there.
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .add_custom_ignore_filename(".liyiignore")
        .build();

    for entry in walker.flatten() {
        let path = entry.path().to_path_buf();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.ends_with(SIDECAR_SUFFIX)
        {
            sidecar_paths.push(path);
            // Sidecar files are metadata — do not include them in
            // all_files so the marker scanner does not match literal
            // marker text in JSON string values (source_anchor, intent).
            continue;
        }
        all_files.push(path);
    }

    // Detect ambiguous sidecars: group by directory + base stem.
    // e.g. money.liyi.jsonc (stem "money") vs money.rs.liyi.jsonc (stem "money.rs")
    // A bare stem "X" is ambiguous when "X.<ext>.liyi.jsonc" also exists in
    // the same directory.
    let mut dir_groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for sc in &sidecar_paths {
        if let Some(parent) = sc.parent() {
            dir_groups
                .entry(parent.to_path_buf())
                .or_default()
                .push(sc.clone());
        }
    }

    for (dir, paths) in &dir_groups {
        // Collect bare stems (those without a second extension).
        let mut bare_stems: Vec<String> = Vec::new();
        for p in paths {
            let stem = source_name_from_sidecar(p);
            // A bare stem has no extension itself (e.g. "money" vs "money.rs").
            if !stem.contains('.') {
                bare_stems.push(stem);
            }
        }
        for bare in &bare_stems {
            // Check if any qualified sidecar also matches this bare stem.
            for p in paths {
                let stem = source_name_from_sidecar(p);
                if stem.contains('.') && stem.starts_with(&format!("{bare}.")) {
                    warnings.push(format!(
                        "Ambiguous sidecar: {bare}{SIDECAR_SUFFIX} alongside \
                         {stem}{SIDECAR_SUFFIX} in {}",
                        dir.display()
                    ));
                }
            }
        }
    }

    // Derive SidecarEntry list.
    let sidecars: Vec<SidecarEntry> = sidecar_paths
        .into_iter()
        .filter_map(|sc| {
            let source_name = source_name_from_sidecar(&sc);
            let source_path = sc.parent()?.join(&source_name);
            let repo_relative_source = pathdiff(&source_path, root)?;

            // Scope filtering: if scope_paths is non-empty, only keep
            // sidecars whose source_path falls under one of the scopes.
            // Canonicalise scope paths relative to root so that relative CLI
            // arguments (e.g. "crates/") match the absolute source_path.
            if !scope_paths.is_empty()
                && !scope_paths.iter().any(|sp| {
                    let abs_scope = if sp.is_relative() {
                        root.join(sp)
                    } else {
                        sp.clone()
                    };
                    source_path.starts_with(&abs_scope)
                })
            {
                return None;
            }

            Some(SidecarEntry {
                sidecar_path: sc,
                source_path,
                repo_relative_source,
            })
        })
        .collect();

    DiscoveryResult {
        sidecars,
        all_files,
        warnings,
    }
}

/// Strip the `.liyi.jsonc` suffix from a sidecar filename to recover the
/// source filename component.
///
/// <!-- @立意:有关 liyi-sidecar-naming-convention -->
fn source_name_from_sidecar(sidecar: &Path) -> String {
    let name = sidecar.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.strip_suffix(SIDECAR_SUFFIX)
        .unwrap_or(name)
        .to_string()
}

/// Expand a list of file/directory paths into concrete `.liyi.jsonc` file
/// paths. If a path is a directory, walk it recursively (respecting
/// `.gitignore` and `.liyiignore`) and collect all sidecar files found.
/// If a path is a file, include it directly.
pub fn resolve_sidecar_targets(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut result: Vec<PathBuf> = Vec::new();
    for p in paths {
        if p.is_dir() {
            let walker = WalkBuilder::new(p)
                .hidden(false)
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

/// Compute `path` relative to `base` using pure lexical processing.
fn pathdiff(path: &Path, base: &Path) -> Option<String> {
    path.strip_prefix(base)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn find_repo_root_finds_git_dir() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        assert_eq!(find_repo_root(&nested), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_repo_root_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(find_repo_root(tmp.path()), None);
    }

    #[test]
    fn find_repo_root_works_with_cross_fs_envvar() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();

        // Same filesystem — should find root regardless of envvar value.
        // SAFETY: test binary is single-threaded for this test; no other
        // thread reads GIT_DISCOVERY_ACROSS_FILESYSTEM concurrently.
        unsafe {
            env::set_var("GIT_DISCOVERY_ACROSS_FILESYSTEM", "0");
        }
        assert_eq!(find_repo_root(&nested), Some(tmp.path().to_path_buf()));

        unsafe {
            env::set_var("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1");
        }
        assert_eq!(find_repo_root(&nested), Some(tmp.path().to_path_buf()));

        unsafe {
            env::remove_var("GIT_DISCOVERY_ACROSS_FILESYSTEM");
        }
    }

    #[test]
    fn discover_collects_sidecars_and_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("foo.rs"), "fn main() {}").unwrap();
        fs::write(root.join("foo.rs.liyi.jsonc"), "{}").unwrap();
        fs::write(root.join("bar.txt"), "hello").unwrap();

        let result = discover(root, &[]);
        assert_eq!(result.sidecars.len(), 1);
        assert_eq!(result.sidecars[0].repo_relative_source, "foo.rs");
        // all_files includes every non-ignored file except sidecars
        assert!(result.all_files.len() >= 2);
        // Sidecar files must not appear in all_files
        assert!(
            !result
                .all_files
                .iter()
                .any(|p| p.to_string_lossy().ends_with(".liyi.jsonc"))
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn discover_detects_ambiguous_sidecars() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("money.liyi.jsonc"), "{}").unwrap();
        fs::write(root.join("money.rs.liyi.jsonc"), "{}").unwrap();

        let result = discover(root, &[]);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Ambiguous"));
    }

    #[test]
    fn discover_respects_scope_paths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();

        fs::write(root.join("top.rs"), "").unwrap();
        fs::write(root.join("top.rs.liyi.jsonc"), "{}").unwrap();
        fs::write(sub.join("inner.rs"), "").unwrap();
        fs::write(sub.join("inner.rs.liyi.jsonc"), "{}").unwrap();

        let scoped = discover(root, std::slice::from_ref(&sub));
        assert_eq!(scoped.sidecars.len(), 1);
        assert_eq!(scoped.sidecars[0].repo_relative_source, "sub/inner.rs");

        // all_files is unaffected by scope (but excludes sidecars)
        assert!(scoped.all_files.len() >= 2);
    }

    #[test]
    fn discover_respects_liyiignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        fs::write(root.join(".liyiignore"), "ignored/\n").unwrap();
        let ignored_dir = root.join("ignored");
        fs::create_dir_all(&ignored_dir).unwrap();
        fs::write(ignored_dir.join("skip.rs"), "").unwrap();
        fs::write(ignored_dir.join("skip.rs.liyi.jsonc"), "{}").unwrap();
        fs::write(root.join("keep.rs"), "").unwrap();
        fs::write(root.join("keep.rs.liyi.jsonc"), "{}").unwrap();

        let result = discover(root, &[]);
        assert_eq!(result.sidecars.len(), 1);
        assert_eq!(result.sidecars[0].repo_relative_source, "keep.rs");
        // The ignored files should not appear in all_files
        assert!(!result.all_files.iter().any(|f| f.starts_with(&ignored_dir)));
    }
}
