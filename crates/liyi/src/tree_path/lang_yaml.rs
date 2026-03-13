use super::LanguageConfig;

use tree_sitter::Node;

/// Walk down wrapper nodes to find the leaf scalar text.
///
/// YAML key nodes are wrapped in `flow_node → plain_scalar → string_scalar`
/// (or `double_quote_scalar`, `single_quote_scalar`).  This function
/// recursively descends through named children until it finds a leaf.
fn leaf_text<'a>(node: &Node<'a>, source: &'a str) -> Option<&'a str> {
    if node.named_child_count() == 0 {
        return Some(&source[node.byte_range()]);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(t) = leaf_text(&child, source) {
            return Some(t);
        }
    }
    None
}

/// Custom name extraction for YAML `block_mapping_pair` nodes.
///
/// The key field is a `flow_node` wrapping `plain_scalar → string_scalar`
/// (for unquoted keys) or a `double_quote_scalar` / `single_quote_scalar`
/// for quoted keys.  We drill down to the leaf scalar text.
fn yaml_node_name(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "block_mapping_pair" {
        return None;
    }
    let key_node = node.child_by_field_name("key")?;
    leaf_text(&key_node, source).map(|s| {
        // Strip surrounding quotes from quoted scalars.
        s.strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .or_else(|| {
                s.strip_prefix('\'')
                    .and_then(|inner| inner.strip_suffix('\''))
            })
            .unwrap_or(s)
            .to_string()
    })
}

/// YAML language configuration (without injection support).
///
/// YAML nesting follows `block_mapping_pair → value (block_node) →
/// block_mapping → block_mapping_pair`.  The intermediate `document`,
/// `block_node`, `block_mapping`, and `block_sequence` nodes are marked
/// transparent so the resolver looks through them to reach the actual
/// `block_mapping_pair` items.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_yaml::LANGUAGE.into(),
    extensions: &["yml", "yaml"],
    kind_map: &[("key", "block_mapping_pair")],
    name_field: "",
    name_overrides: &[],
    body_fields: &["value"],
    custom_name: Some(yaml_node_name),
    doc_comment_detector: None,
    transparent_kinds: &["document", "block_node", "block_mapping", "block_sequence"],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_YAML: &str = r#"name: liyi
version: 0.1.0

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build
      - name: Test
        run: cargo test

metadata:
  authors:
    - alice
    - bob
"#;

    #[test]
    fn resolve_yaml_top_level_key() {
        let span = resolve_tree_path(SAMPLE_YAML, "key.name", Language::Yaml);
        assert!(span.is_some(), "should resolve key.name");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_YAML.lines().collect();
        assert!(
            lines[start - 1].contains("name: liyi"),
            "span should point to name key, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_yaml_nested_key() {
        let span = resolve_tree_path(SAMPLE_YAML, "key.jobs::key.build", Language::Yaml);
        assert!(span.is_some(), "should resolve key.jobs::key.build");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_YAML.lines().collect();
        assert!(
            lines[start - 1].contains("build:"),
            "span should point to build key, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_yaml_deeply_nested_key() {
        let span = resolve_tree_path(
            SAMPLE_YAML,
            "key.jobs::key.build::key.\"runs-on\"",
            Language::Yaml,
        );
        assert!(
            span.is_some(),
            "should resolve key.jobs::key.build::key.\"runs-on\""
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_YAML.lines().collect();
        assert!(
            lines[start - 1].contains("runs-on: ubuntu-latest"),
            "span should point to runs-on key, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_yaml_indexed_sequence_item() {
        // steps[1] → second step → {name: Build, run: cargo build}
        let span = resolve_tree_path(
            SAMPLE_YAML,
            "key.jobs::key.build::key.steps[1]::key.name",
            Language::Yaml,
        );
        assert!(
            span.is_some(),
            "should resolve key.jobs::key.build::key.steps[1]::key.name"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_YAML.lines().collect();
        assert!(
            lines[start - 1].contains("name: Build"),
            "span should point to name key in second step, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_yaml_indexed_step_run() {
        let span = resolve_tree_path(
            SAMPLE_YAML,
            "key.jobs::key.build::key.steps[2]::key.run",
            Language::Yaml,
        );
        assert!(
            span.is_some(),
            "should resolve key.jobs::key.build::key.steps[2]::key.run"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_YAML.lines().collect();
        assert!(
            lines[start - 1].contains("run: cargo test"),
            "span should point to run key in third step, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn compute_yaml_top_level_key() {
        let span = resolve_tree_path(SAMPLE_YAML, "key.name", Language::Yaml).unwrap();
        let path = compute_tree_path(SAMPLE_YAML, span, Language::Yaml);
        assert_eq!(path, "key.name");
    }

    #[test]
    fn compute_yaml_nested_key() {
        let span = resolve_tree_path(
            SAMPLE_YAML,
            "key.jobs::key.build::key.\"runs-on\"",
            Language::Yaml,
        )
        .unwrap();
        let path = compute_tree_path(SAMPLE_YAML, span, Language::Yaml);
        assert_eq!(path, "key.jobs::key.build::key.\"runs-on\"");
    }

    #[test]
    fn roundtrip_yaml_top_level() {
        let span = resolve_tree_path(SAMPLE_YAML, "key.name", Language::Yaml).unwrap();
        let path = compute_tree_path(SAMPLE_YAML, span, Language::Yaml);
        assert_eq!(path, "key.name");
        let re_resolved = resolve_tree_path(SAMPLE_YAML, &path, Language::Yaml).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_yaml_nested() {
        let span = resolve_tree_path(SAMPLE_YAML, "key.jobs::key.build", Language::Yaml).unwrap();
        let path = compute_tree_path(SAMPLE_YAML, span, Language::Yaml);
        assert_eq!(path, "key.jobs::key.build");
        let re_resolved = resolve_tree_path(SAMPLE_YAML, &path, Language::Yaml).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn detect_yaml_extensions() {
        assert_eq!(
            detect_language(Path::new("config.yml")),
            Some(Language::Yaml)
        );
        assert_eq!(
            detect_language(Path::new("config.yaml")),
            Some(Language::Yaml)
        );
        assert_eq!(
            detect_language(Path::new(".github/workflows/ci.yml")),
            Some(Language::Yaml)
        );
    }
}
