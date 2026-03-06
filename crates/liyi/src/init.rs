use std::fs;
use std::path::{Path, PathBuf};

/// The agent instruction block appended to AGENTS.md by `liyi init`.
const AGENTS_MD_BLOCK: &str = r#"
## 立意 (Intent Specs)

When writing or modifying code:

1. For each non-trivial item, infer what it SHOULD do. Write intent to `<source>.liyi.jsonc`. Record `source_span` (start/end lines). Do not write `source_hash` or `source_anchor` — the tool fills them in. Use `"intent": "=doc"` only when the docstring contains behavioral requirements. For trivial items, annotate with `@liyi:trivial`.
2. When module-level invariants are apparent, write an `@liyi:module` block.
3. If a source item has `@liyi:related <name>`, record `"related": {"<name>": null}` in the sidecar.
4. For each `@liyi:requirement <name>` block, ensure it has a sidecar entry.
5. Only generate adversarial tests from items with `@liyi:intent` or `"reviewed": true`.
6. Skip items annotated with `@liyi:ignore` or `@liyi:trivial`, and files matched by `.liyiignore`.
"#;

/// Skeleton sidecar content for a source file.
fn skeleton_sidecar(source_relative: &str) -> String {
    format!(
        r#"// liyi v0.1 spec file
{{
  "version": "0.1",
  "source": "{source_relative}",
  "specs": []
}}
"#
    )
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
            Self::AlreadyExists(p) => write!(f, "{} already exists (use --force to overwrite)", p.display()),
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

/// `liyi init <source-file>` — create a skeleton `.liyi.jsonc` sidecar.
///
/// The sidecar path is `<source-file>.liyi.jsonc`.
/// If the sidecar already exists and `force` is false, returns an error.
pub fn init_sidecar(source_file: &Path, force: bool) -> Result<PathBuf, InitError> {
    let sidecar_name = format!(
        "{}.liyi.jsonc",
        source_file.file_name().unwrap_or_default().to_string_lossy()
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

    let content = skeleton_sidecar(&source_name);
    fs::write(&sidecar_path, content)?;

    Ok(sidecar_path)
}
