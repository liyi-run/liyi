//! Language injection framework.
//!
//! Injection profiles describe where embedded code lives inside a host
//! grammar and what language to sub-parse it as.  This module provides:
//!
//! - `InjectionProfile` / `InjectionRule` — static, data-driven profile types.
//! - A global registry of profiles.
//! - `detect_injection_profiles()` — path-pattern matching.
//! - YAML content extraction helpers (block scalar, flow scalar).
//! - Ancestor-path matching logic.

// @liyi:related injection-profile-isolation
// @liyi:related path-based-dialect-detection
// @liyi:related no-injection-without-profile

pub mod github_actions;

use std::path::Path;

use globset::{Glob, GlobMatcher};

use super::Language;

/// A rule describing one injection point within a host grammar.
// @liyi:related injection-pair-attachment
// @liyi:related ancestor-path-matching
pub struct InjectionRule {
    /// Key name that triggers injection (e.g., "run").
    pub key_name: &'static str,
    /// Language to sub-parse the injected content as.
    pub language: Language,
    /// Optional ancestor key path that must be satisfied.
    /// Each element is a key name; the rule fires only if the
    /// injection-point node has these ancestors in order
    /// (not necessarily immediate parents).
    ///
    /// Example: `&["jobs"]` requires that the `run:` key appears
    /// somewhere under a `jobs:` key.
    pub ancestor_keys: &'static [&'static str],
}

/// An injection profile associates a host language + file-path pattern with a
/// set of injection rules.
// @liyi:related injection-profile-isolation
pub struct InjectionProfile {
    /// Host language this profile applies to.
    pub host: Language,
    /// Glob patterns matched against the repo-relative file path.
    /// If any pattern matches, this profile is active.
    /// Empty slice = never auto-activated (explicit only).
    pub path_patterns: &'static [&'static str],
    /// Injection rules to apply when this profile is active.
    pub rules: &'static [InjectionRule],
}

impl InjectionProfile {
    /// Check whether `repo_path` matches any of this profile's path patterns.
    pub fn matches_path(&self, repo_path: &Path) -> bool {
        if self.path_patterns.is_empty() {
            return false;
        }
        let path_str = repo_path.to_string_lossy();
        self.path_patterns.iter().any(|pattern| {
            Glob::new(pattern)
                .ok()
                .map(|g| g.compile_matcher())
                .is_some_and(|m: GlobMatcher| m.is_match(path_str.as_ref()))
        })
    }

    /// Find the injection rule that matches a given key name and ancestor
    /// chain. Returns `None` if no rule fires.
    // @liyi:related ancestor-path-matching
    pub fn find_rule(&self, key_name: &str, ancestor_keys: &[&str]) -> Option<&InjectionRule> {
        self.rules.iter().find(|rule| {
            rule.key_name == key_name && ancestors_match(rule.ancestor_keys, ancestor_keys)
        })
    }
}

/// Check whether the required ancestor keys appear as an ordered subsequence
/// of the actual ancestor chain (from outermost to innermost).
fn ancestors_match(required: &[&str], actual: &[&str]) -> bool {
    if required.is_empty() {
        return true;
    }
    let mut req_iter = required.iter();
    let mut current = req_iter.next();
    for ancestor in actual {
        if let Some(needed) = current {
            if ancestor == needed {
                current = req_iter.next();
            }
        } else {
            break;
        }
    }
    current.is_none()
}

/// The global registry of all injection profiles.
static REGISTRY: &[&InjectionProfile] = &[&github_actions::PROFILE];

/// Returns all injection profiles whose path patterns match the given
/// repo-relative path.  Returns an empty vec when no profile matches
/// (= no injection, base grammar only).
// @liyi:related no-injection-without-profile
// @liyi:related path-based-dialect-detection
pub fn detect_injection_profiles(path: &Path) -> Vec<&'static InjectionProfile> {
    REGISTRY
        .iter()
        .filter(|p| p.matches_path(path))
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------
// YAML content extraction
// ---------------------------------------------------------------------------

use tree_sitter::Node;

/// Result of extracting injected content from a YAML node.
pub struct ExtractedContent {
    /// The extracted source text, ready for sub-parsing.
    pub text: String,
    /// 0-indexed line offset in the outer file where `text` starts.
    /// Add this to inner-parser line numbers to get outer-file lines.
    pub line_offset: usize,
}

/// Extract the content of a YAML block scalar value node for injection.
///
/// Strips the indicator line (`|`, `>`, `|+`, `|-`, `|2`, etc.) and
/// de-indents the body uniformly based on the first content line.
///
/// `node` should be the `value` field of a `block_mapping_pair` (typically
/// a `block_node` containing a `block_scalar`).
// @liyi:related content-offset-correctness
pub fn extract_block_scalar(node: &Node, source: &str) -> Option<ExtractedContent> {
    // Walk down to the block_scalar node.
    let scalar = find_descendant_by_kind(node, "block_scalar")?;
    let text = &source[scalar.byte_range()];
    let scalar_start_line = scalar.start_position().row;

    // The first line is the indicator (|, >, |+, etc.); content starts after.
    let mut lines = text.lines();
    let _indicator = lines.next()?;
    let content_start_line = scalar_start_line + 1;

    let body_lines: Vec<&str> = lines.collect();
    if body_lines.is_empty() {
        return Some(ExtractedContent {
            text: String::new(),
            line_offset: content_start_line,
        });
    }

    // Determine base indentation from first non-empty content line.
    let base_indent = body_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut result = String::new();
    for line in &body_lines {
        if line.trim().is_empty() {
            result.push('\n');
        } else if line.len() >= base_indent {
            result.push_str(&line[base_indent..]);
            result.push('\n');
        } else {
            result.push_str(line.trim());
            result.push('\n');
        }
    }

    Some(ExtractedContent {
        text: result,
        line_offset: content_start_line,
    })
}

/// Extract the content of a YAML flow scalar (inline string) for injection.
///
/// Handles `double_quote_scalar`, `single_quote_scalar`, and `plain_scalar`.
// @liyi:related content-offset-correctness
pub fn extract_flow_scalar(node: &Node, source: &str) -> Option<ExtractedContent> {
    let scalar = find_flow_scalar(node)?;
    let text = &source[scalar.byte_range()];
    let line_offset = scalar.start_position().row;

    let content = match scalar.kind() {
        "double_quote_scalar" => text
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(text)
            .to_string(),
        "single_quote_scalar" => text
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(text)
            .to_string(),
        _ => text.to_string(), // plain_scalar, string_scalar
    };

    Some(ExtractedContent {
        text: content,
        line_offset,
    })
}

/// Extract injected content from a YAML value node, trying block scalar
/// first, then flow scalar.
pub fn extract_yaml_content(node: &Node, source: &str) -> Option<ExtractedContent> {
    extract_block_scalar(node, source).or_else(|| extract_flow_scalar(node, source))
}

/// Find a descendant node of a specific kind.
fn find_descendant_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(*node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_descendant_by_kind(&child, kind) {
            return Some(found);
        }
    }
    None
}

/// Find a flow scalar descendant (double_quote_scalar, single_quote_scalar,
/// plain_scalar, or string_scalar wrapped in flow_node).
fn find_flow_scalar<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    for kind in &[
        "double_quote_scalar",
        "single_quote_scalar",
        "plain_scalar",
        "string_scalar",
    ] {
        if let Some(found) = find_descendant_by_kind(node, kind) {
            return Some(found);
        }
    }
    None
}

/// Collect ancestor key names from a YAML node upward.
///
/// Starting from `node`, walks up via `node.parent()`, collecting key names
/// from `block_mapping_pair` nodes. Returns keys from outermost to innermost.
pub fn collect_ancestor_keys(node: &Node, source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "block_mapping_pair"
            && let Some(key_node) = n.child_by_field_name("key")
            && let Some(text) = super::lang_yaml::leaf_text_pub(&key_node, source)
        {
            let key = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(text);
            keys.push(key.to_string());
        }
        current = n.parent();
    }
    keys.reverse(); // outermost first
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ---- ancestor matching ----

    #[test]
    fn ancestors_match_empty_required() {
        assert!(ancestors_match(&[], &["jobs", "build"]));
    }

    #[test]
    fn ancestors_match_exact() {
        assert!(ancestors_match(&["jobs"], &["jobs"]));
    }

    #[test]
    fn ancestors_match_subsequence() {
        assert!(ancestors_match(&["jobs"], &["root", "jobs", "build"]));
    }

    #[test]
    fn ancestors_match_ordered_subsequence() {
        assert!(ancestors_match(
            &["jobs", "steps"],
            &["root", "jobs", "build", "steps"]
        ));
    }

    #[test]
    fn ancestors_no_match_wrong_order() {
        assert!(!ancestors_match(
            &["steps", "jobs"],
            &["root", "jobs", "build", "steps"]
        ));
    }

    #[test]
    fn ancestors_no_match_missing() {
        assert!(!ancestors_match(&["jobs"], &["root", "build", "steps"]));
    }

    // ---- profile detection ----

    #[test]
    fn detect_github_actions_workflow() {
        let profiles = detect_injection_profiles(Path::new(".github/workflows/ci.yml"));
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].host, Language::Yaml);
    }

    #[test]
    fn detect_github_actions_yaml_ext() {
        let profiles = detect_injection_profiles(Path::new(".github/workflows/build.yaml"));
        assert_eq!(profiles.len(), 1);
    }

    #[test]
    fn detect_github_actions_action() {
        let profiles = detect_injection_profiles(Path::new(".github/actions/setup/action.yml"));
        assert_eq!(profiles.len(), 1);
    }

    #[test]
    fn detect_no_profile_for_plain_yaml() {
        let profiles = detect_injection_profiles(Path::new("config/settings.yaml"));
        assert!(profiles.is_empty());
    }

    #[test]
    fn detect_no_profile_for_k8s() {
        let profiles = detect_injection_profiles(Path::new("kubernetes/deployment.yaml"));
        assert!(profiles.is_empty());
    }

    // ---- profile find_rule ----

    #[test]
    fn find_rule_matches_with_ancestor() {
        let profile = &github_actions::PROFILE;
        let rule = profile.find_rule("run", &["jobs"]);
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().language, Language::Bash);
    }

    #[test]
    fn find_rule_no_match_wrong_key() {
        let profile = &github_actions::PROFILE;
        assert!(profile.find_rule("uses", &["jobs"]).is_none());
    }

    #[test]
    fn find_rule_no_match_missing_ancestor() {
        let profile = &github_actions::PROFILE;
        assert!(profile.find_rule("run", &[]).is_none());
    }
}
