use std::fs;
use std::path::{Path, PathBuf};

use crate::markers::{requirement_spans, scan_markers};
use crate::sidecar::{ItemSpec, RequirementSpec, SidecarFile, Spec, write_sidecar};
use crate::tree_path::{detect_language, discover_items};

/// The full content of the repo's own AGENTS.md, included at compile time
/// so that `liyi init` can extract the portable template block.
const AGENTS_MD_FULL: &str = include_str!("../../../AGENTS.md");

const TEMPLATE_START: &str = "<!-- liyi:template:start -->\n";
const TEMPLATE_END: &str = "\n<!-- liyi:template:end -->";

/// Extract the portable agent instruction block from the repo's AGENTS.md.
///
/// The block is delimited by `<!-- liyi:template:start -->` and
/// `<!-- liyi:template:end -->` HTML comment markers.  Panics at
/// runtime (not compile time) if the markers are missing — but since
/// the content is baked in via `include_str!`, this is effectively a
/// build-time guarantee: any AGENTS.md without markers won't produce
/// a working binary.
fn agents_md_block() -> &'static str {
    let start = AGENTS_MD_FULL
        .find(TEMPLATE_START)
        .expect("AGENTS.md missing <!-- liyi:template:start --> marker")
        + TEMPLATE_START.len();
    let end = start
        + AGENTS_MD_FULL[start..]
            .find(TEMPLATE_END)
            .expect("AGENTS.md missing <!-- liyi:template:end --> marker");
    &AGENTS_MD_FULL[start..end]
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

/// `liyi init` — append agent instruction to `AGENTS.md`.
///
/// Creates the file if it doesn't exist; appends the block if it does.
/// If `force` is true, overwrites the agent instruction block even if
/// a `## 立意` section already exists.
pub fn init_agents_md(root: &Path, force: bool) -> Result<PathBuf, InitError> {
    let agents_path = root.join("AGENTS.md");

    if agents_path.is_file() {
        let content = fs::read_to_string(&agents_path)?;
        if content.contains("## 立意") && !force {
            return Err(InitError::AlreadyExists(agents_path));
        }
        // Append the block
        let block = agents_md_block();
        let mut new_content = content;
        new_content.push('\n');
        new_content.push_str(block);
        fs::write(&agents_path, new_content)?;
    } else {
        // Create new file
        let block = agents_md_block();
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
/// <!-- @立意:有関 liyi-sidecar-naming-convention -->
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

    #[test]
    fn agents_md_block_extracts_valid_template() {
        let block = agents_md_block();

        // Must start with the section heading.
        assert!(
            block.starts_with("## 立意"),
            "extracted block must start with ## 立意 heading"
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
}
