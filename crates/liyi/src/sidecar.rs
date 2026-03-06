use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level `.liyi.jsonc` file representation
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarFile {
    pub version: String,
    pub source: String,
    pub specs: Vec<Spec>,
}

/// Represents either an item or a requirement spec
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Spec {
    Item(ItemSpec),
    Requirement(RequirementSpec),
}

/// Details of a code item (function, struct, etc.)
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub item: String,

    #[serde(default)]
    pub reviewed: bool,

    pub intent: String,

    pub source_span: [usize; 2],

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tree_path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<HashMap<String, Option<String>>>,
}

/// Details of a module requirement/invariant
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementSpec {
    pub requirement: String,

    pub source_span: [usize; 2],

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tree_path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<String>,
}

/// Strip `//` line comments and `/* */` block comments from JSONC input,
/// respecting JSON string boundaries (including `\"` escapes).
pub fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_string = false;

    while i < len {
        if in_string {
            // Handle escape sequences inside strings
            if chars[i] == '\\' && i + 1 < len {
                out.push(chars[i]);
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if chars[i] == '"' {
                in_string = false;
            }
            out.push(chars[i]);
            i += 1;
        } else {
            // Not inside a string
            if chars[i] == '"' {
                in_string = true;
                out.push(chars[i]);
                i += 1;
            } else if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
                // Line comment — skip until end of line
                i += 2;
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                // Keep the newline itself
            } else if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
                // Block comment — skip until */
                i += 2;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                if i + 1 < len {
                    i += 2; // skip closing */
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// Parse a `.liyi.jsonc` file: strip JSONC comments then deserialize.
pub fn parse_sidecar(content: &str) -> Result<SidecarFile, String> {
    let json = strip_jsonc_comments(content);
    serde_json::from_str::<SidecarFile>(&json)
        .map_err(|e| format!("failed to parse sidecar file: {e}"))
}

/// Serialize a `SidecarFile` to pretty-printed JSON with a JSONC header comment.
pub fn write_sidecar(sidecar: &SidecarFile) -> String {
    let json =
        serde_json::to_string_pretty(sidecar).expect("SidecarFile serialization should not fail");
    format!("// liyi v0.1 spec file\n{json}\n")
}

/// Validate a parsed `SidecarFile`. Returns a list of error strings (empty = valid).
pub fn validate_sidecar(sidecar: &SidecarFile) -> Vec<String> {
    let mut errors = Vec::new();

    if sidecar.version != "0.1" {
        errors.push(format!(
            "unsupported version \"{}\"; expected \"0.1\"",
            sidecar.version
        ));
    }

    let hash_re = regex::Regex::new(r"^sha256:[0-9a-f]+$").unwrap();

    for (idx, spec) in sidecar.specs.iter().enumerate() {
        match spec {
            Spec::Item(item) => {
                let label = format!("specs[{}] (item \"{}\")", idx, item.item);
                if item.source_span[0] < 1 {
                    errors.push(format!("{label}: source_span start must be >= 1"));
                }
                if item.source_span[0] > item.source_span[1] {
                    errors.push(format!(
                        "{label}: source_span start ({}) > end ({})",
                        item.source_span[0], item.source_span[1]
                    ));
                }
                if let Some(ref h) = item.source_hash
                    && !hash_re.is_match(h)
                {
                    errors.push(format!(
                        "{label}: source_hash \"{}\" does not match sha256:<hex>",
                        h
                    ));
                }
                if let Some(ref related) = item.related {
                    for (name, hash_opt) in related {
                        if let Some(h) = hash_opt
                            && !hash_re.is_match(h)
                        {
                            errors.push(format!(
                                "{label}: related[\"{name}\"] hash \"{h}\" does not match sha256:<hex>"
                            ));
                        }
                    }
                }
            }
            Spec::Requirement(req) => {
                let label = format!("specs[{}] (requirement \"{}\")", idx, req.requirement);
                if req.source_span[0] < 1 {
                    errors.push(format!("{label}: source_span start must be >= 1"));
                }
                if req.source_span[0] > req.source_span[1] {
                    errors.push(format!(
                        "{label}: source_span start ({}) > end ({})",
                        req.source_span[0], req.source_span[1]
                    ));
                }
                if let Some(ref h) = req.source_hash
                    && !hash_re.is_match(h)
                {
                    errors.push(format!(
                        "{label}: source_hash \"{}\" does not match sha256:<hex>",
                        h
                    ));
                }
            }
        }
    }

    errors
}
