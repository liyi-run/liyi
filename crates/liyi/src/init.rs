use std::fs;
use std::path::{Path, PathBuf};

use crate::markers::{requirement_spans, scan_markers};
use crate::sidecar::{ItemSpec, RequirementSpec, SidecarFile, Spec, write_sidecar};
use crate::tree_path::{detect_language, discover_items};

/// The full content of the repo's own AGENTS.md, included at compile time
/// so that `liyi init` can extract the portable template block.
const AGENTS_MD_FULL: &str = include_str!("../../../AGENTS.md");

/// Current revision of the portable agent-instruction template.
///
/// Bumped when the template content changes; `liyi migrate` rewrites an
/// adopter's block to this revision.
pub const TEMPLATE_REVISION: u32 = 1;

/// Opening words of the `START` pragma; the revision number follows.
const PRAGMA_START_PREFIX: &str = "<!-- START liyi agent instructions rev. ";
/// Closing words of the `START` pragma line.
const PRAGMA_SUFFIX: &str = " -->";
/// Reminder line that follows the `START` pragma.
const REMINDER: &str =
    "<!-- DON'T EDIT THIS BLOCK. REFRESH WITH `liyi migrate` FROM A NEWER LIYI. -->";
/// Closing marker line.
const END_MARKER: &str = "<!-- END liyi agent instructions -->";

/// The `START liyi agent instructions rev. N` pragma line for `rev`.
fn pragma_line(rev: u32) -> String {
    format!("{PRAGMA_START_PREFIX}{rev}{PRAGMA_SUFFIX}")
}

/// Extract the portable agent instruction block from the repo's AGENTS.md,
/// re-rendered at [`TEMPLATE_REVISION`] and including the pragma, reminder,
/// and closing marker — i.e. exactly what `liyi init` ships and `liyi migrate`
/// writes.
///
/// Panics if the pragma or end marker is missing — but since the content is
/// baked in via `include_str!`, this is effectively a build-time guarantee:
/// any AGENTS.md without the pragma won't produce a working binary.
fn agents_md_block() -> String {
    let start = format!("{}\n{REMINDER}\n", pragma_line(TEMPLATE_REVISION));
    let start_idx = AGENTS_MD_FULL
        .find(&start)
        .expect("AGENTS.md missing START liyi agent instructions pragma")
        + start.len();
    let end_idx = start_idx
        + AGENTS_MD_FULL[start_idx..]
            .find(&format!("\n{END_MARKER}"))
            .expect("AGENTS.md missing END liyi agent instructions marker");
    format!(
        "{start}{}\n{END_MARKER}",
        &AGENTS_MD_FULL[start_idx..end_idx]
    )
}

/// Locate the portable agent-instruction block in `text`.
///
/// Returns the byte range from the `START` pragma through the `END` marker,
/// or `None` if no block is present. The revision is not inspected, so a block
/// written by an older `liyi` is still found.
pub fn find_agents_md_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(PRAGMA_START_PREFIX)?;
    let end = start + text[start..].find(END_MARKER)? + END_MARKER.len();
    Some((start, end))
}

/// Rewrite the portable agent-instruction block in `path` to
/// [`TEMPLATE_REVISION`].
///
/// Returns `Ok(true)` when the file contains a block (rewritten in place,
/// idempotently) and `Ok(false)` when it does not.
pub fn migrate_agents_md(path: &Path) -> Result<bool, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let Some((start, end)) = find_agents_md_block(&content) else {
        return Ok(false);
    };
    let block = agents_md_block();
    if content[start..end] == block {
        return Ok(true);
    }
    let mut new_content = String::with_capacity(content.len());
    new_content.push_str(&content[..start]);
    new_content.push_str(&block);
    new_content.push_str(&content[end..]);
    fs::write(path, new_content).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(true)
}

/// Error type for init operations.
#[derive(Debug)]
pub enum InitError {
    /// The target file already exists and `--force` was not set.
    AlreadyExists(PathBuf),
    /// An I/O error occurred.
    Io(std::io::Error),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(p) => write!(
                f,
                "{} already exists (use --force to overwrite)",
                p.display()
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for InitError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// `liyi init` — create or update the agent instruction section in `AGENTS.md`.
///
/// Creates the file if it doesn't exist and appends the block if it does. If a
/// portable block is already present, returns [`InitError::AlreadyExists`]
/// unless `force` is set, in which case the existing block is replaced with the
/// current revision.
pub fn init_agents_md(root: &Path, force: bool) -> Result<PathBuf, InitError> {
    let agents_path = root.join("AGENTS.md");
    let block = agents_md_block();

    if agents_path.is_file() {
        let content = fs::read_to_string(&agents_path)?;
        if let Some((start, end)) = find_agents_md_block(&content) {
            if !force {
                return Err(InitError::AlreadyExists(agents_path));
            }
            let mut new_content = String::with_capacity(content.len());
            new_content.push_str(&content[..start]);
            new_content.push_str(&block);
            new_content.push_str(&content[end..]);
            fs::write(&agents_path, new_content)?;
        } else {
            let mut new_content = content;
            new_content.push('\n');
            new_content.push_str(&block);
            fs::write(&agents_path, new_content)?;
        }
    } else {
        let content = format!("# AGENTS.md\n\n{block}");
        fs::write(&agents_path, content)?;
    }

    Ok(agents_path)
}

/// `liyi init <source-file>` — create a `.liyi.jsonc` sidecar.
///
/// When `discover` is true and the language is supported, pre-populates the
/// sidecar `specs` array with items discovered via tree-sitter. Otherwise
/// emits an empty `"specs": []` skeleton.
///
/// `trivial_threshold` controls the line-count cutoff for `_likely_trivial`:
/// items with `_body_lines <= trivial_threshold` and no doc comment are
/// marked `_likely_trivial: true`.
///
/// The sidecar path is `<source-file>.liyi.jsonc`.
/// If the sidecar already exists and `force` is false, returns an error.
///
/// <!-- @liyi:related liyi-sidecar-naming-convention -->
// @liyi:related exhaustive-inclusion
// @liyi:related graceful-degradation
// @liyi:related hints-are-ephemeral
// @liyi:related hints-intentionally-unstructured
// @liyi:related tree-sitter-signals-always-present
pub fn init_sidecar(
    source_file: &Path,
    force: bool,
    discover: bool,
    trivial_threshold: usize,
) -> Result<PathBuf, InitError> {
    let sidecar_name = format!(
        "{}.liyi.jsonc",
        source_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let sidecar_path = source_file.with_file_name(&sidecar_name);

    if sidecar_path.is_file() && !force {
        return Err(InitError::AlreadyExists(sidecar_path));
    }

    let source_name = source_file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let source_content = fs::read_to_string(source_file)?;

    // Item discovery via tree-sitter (when language is supported).
    let mut specs: Vec<Spec> = if discover {
        if let Some(lang) = detect_language(source_file) {
            let discovered = discover_items(&source_content, lang);
            discovered
                .into_iter()
                .map(|d| {
                    let mut hints = serde_json::Map::new();
                    if let Some(has_doc) = d.has_doc_comment {
                        hints.insert("_has_doc".to_string(), serde_json::Value::Bool(has_doc));
                    }
                    let body_lines = d.span[1] - d.span[0] + 1;
                    hints.insert("_body_lines".to_string(), serde_json::json!(body_lines));
                    let likely_trivial =
                        body_lines <= trivial_threshold && d.has_doc_comment != Some(true);
                    if likely_trivial {
                        hints.insert("_likely_trivial".to_string(), serde_json::Value::Bool(true));
                    }
                    let _hints = Some(serde_json::Value::Object(hints));
                    Spec::Item(ItemSpec {
                        item: d.name,
                        reviewed: false,
                        intent: String::new(),
                        source_span: d.span,
                        tree_path: d.tree_path,
                        source_hash: None,
                        source_anchor: None,
                        confidence: None,
                        related: None,
                        _hints,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Requirement discovery via marker scanning (any file type).
    let markers = scan_markers(&source_content);
    let req_spans = requirement_spans(&markers);
    for (name, span) in &req_spans {
        let anchor_line = source_content
            .lines()
            .nth(span[0] - 1)
            .unwrap_or("")
            .trim()
            .to_string();
        specs.push(Spec::Requirement(RequirementSpec {
            requirement: name.clone(),
            source_span: *span,
            tree_path: String::new(),
            source_hash: None,
            source_anchor: Some(anchor_line),
        }));
    }

    let sidecar = SidecarFile {
        version: "0.1".to_string(),
        source: source_name,
        specs,
    };

    let content = write_sidecar(&sidecar);
    fs::write(&sidecar_path, content)?;

    Ok(sidecar_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn agents_md_block_extracts_valid_template() {
        let block = agents_md_block();

        // The shipped block carries the pragma, reminder, content, and marker.
        assert!(
            block.starts_with(PRAGMA_START_PREFIX),
            "block must start with the START pragma"
        );
        assert!(
            block.contains(&pragma_line(TEMPLATE_REVISION)),
            "block must carry the current revision"
        );
        assert!(block.contains(REMINDER), "block must carry the reminder");
        assert!(
            block.ends_with(END_MARKER),
            "block must end with the END marker"
        );
        assert!(
            block.contains("## The 立意"),
            "block must include the section heading"
        );

        // Key invariants: the block contains the sidecar schema and
        // the triage schema — ensuring both rules and schemas are
        // included in the portable template.
        assert!(
            block.contains(".liyi.jsonc"),
            "template must reference .liyi.jsonc"
        );
        assert!(
            block.contains("source_span"),
            "template must reference source_span"
        );
        assert!(
            block.contains("liyi.schema.json"),
            "template must include the sidecar JSON schema"
        );
        assert!(
            block.contains("triage.schema.json"),
            "template must include the triage JSON schema"
        );
    }

    #[test]
    fn find_agents_md_block_locates_a_block_at_any_revision() {
        let text = "before\n<!-- START liyi agent instructions rev. 7 -->\n<!-- DON'T EDIT -->\nbody\n<!-- END liyi agent instructions -->\nafter\n";
        let (start, end) = find_agents_md_block(text).expect("block must be found");
        assert_eq!(
            &text[start..end],
            "<!-- START liyi agent instructions rev. 7 -->\n<!-- DON'T EDIT -->\nbody\n<!-- END liyi agent instructions -->"
        );
        assert!(find_agents_md_block("no pragma here").is_none());
    }

    #[test]
    fn migrate_agents_md_rewrites_an_older_block_idempotently() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("AGENTS.md");
        fs::write(
            &path,
            "# AGENTS.md\n\n<!-- START liyi agent instructions rev. 0 -->\n<!-- DON'T EDIT -->\nstale body\n<!-- END liyi agent instructions -->\n",
        )
        .unwrap();

        assert!(migrate_agents_md(&path).unwrap());
        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains(&pragma_line(TEMPLATE_REVISION)));
        assert!(migrated.contains("## The 立意"));
        assert!(!migrated.contains("stale body"));

        // Idempotent: a second run leaves the file byte-identical.
        assert!(migrate_agents_md(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), migrated);
    }

    #[test]
    fn migrate_agents_md_reports_absent_blocks() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("AGENTS.md");
        fs::write(&path, "# AGENTS.md\n\nNo pragma here.\n").unwrap();
        assert!(!migrate_agents_md(&path).unwrap());
    }
}
