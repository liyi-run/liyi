use std::fs;
use std::path::{Path, PathBuf};

use crate::sidecar::{ItemSpec, SidecarFile, Spec, write_sidecar};
use crate::tree_path::{detect_language, discover_items};

/// The agent instruction block appended to AGENTS.md by `liyi init`.
/// Note that 立意 directives in the string must be quine-escaped to
/// avoid mis-identification.
/// <!-- @立意:有关 quine-escape -->
const AGENTS_MD_BLOCK: &str = "\n\
## 立意 (Intent Specs)\n\
\n\
When writing or modifying code:\n\
\n\
1. For each non-trivial item, infer what it SHOULD do. Write intent to `<source>.liyi.jsonc`. \
Record `source_span` (start/end lines). Do not write `source_hash` or `source_anchor` \
— the tool fills them in. Use `\"intent\": \"=doc\"` only when the docstring contains \
behavioral requirements. For trivial items, annotate with `\x40liyi:trivial` \
or use `\"intent\": \"=trivial\"` in the sidecar.\n\
2. When module-level invariants are apparent, write an `\x40liyi:module` block.\n\
3. If a source item has `\x40liyi:related <name>`, record `\"related\": {\"<name>\": null}` in the sidecar.\n\
4. For each `\x40liyi:requirement <name>` block, ensure it has a sidecar entry.\n\
5. Only generate adversarial tests from items with `\x40liyi:intent` or `\"reviewed\": true`.\n\
6. Skip items annotated with `\x40liyi:ignore` or `\x40liyi:trivial`, and files matched by `.liyiignore`.\n";

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
        let mut new_content = content;
        new_content.push_str(AGENTS_MD_BLOCK);
        fs::write(&agents_path, new_content)?;
    } else {
        // Create new file
        let content = format!("# AGENTS.md\n{AGENTS_MD_BLOCK}");
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
/// <!-- @立意:有关 liyi-sidecar-naming-convention -->
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

    let specs = if discover {
        if let Some(lang) = detect_language(source_file) {
            let source_content = fs::read_to_string(source_file)?;
            let discovered = discover_items(&source_content, lang);
            discovered
                .into_iter()
                .map(|d| {
                    let mut hints = serde_json::Map::new();
                    if let Some(has_doc) = d.has_doc_comment {
                        hints.insert(
                            "_has_doc".to_string(),
                            serde_json::Value::Bool(has_doc),
                        );
                    }
                    let body_lines = d.span[1] - d.span[0] + 1;
                    hints.insert(
                        "_body_lines".to_string(),
                        serde_json::json!(body_lines),
                    );
                    let likely_trivial =
                        body_lines <= trivial_threshold
                            && d.has_doc_comment != Some(true);
                    if likely_trivial {
                        hints.insert(
                            "_likely_trivial".to_string(),
                            serde_json::Value::Bool(true),
                        );
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

    let sidecar = SidecarFile {
        version: "0.1".to_string(),
        source: source_name,
        specs,
    };

    let content = write_sidecar(&sidecar);
    fs::write(&sidecar_path, content)?;

    Ok(sidecar_path)
}
