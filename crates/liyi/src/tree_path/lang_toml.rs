use super::LanguageConfig;

use tree_sitter::Node;

/// Find the first child with a given kind.
fn find_child_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

/// Custom name extraction for TOML nodes.
///
/// TOML `table`, `table_array_element`, and `pair` nodes do not have a
/// standard `name` field.  Instead, the key is carried as a `bare_key` or
/// `quoted_key` child.
fn toml_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "table" | "table_array_element" => {
            // The table name is the first bare_key or quoted_key child.
            find_child_by_kind(node, "bare_key")
                .or_else(|| find_child_by_kind(node, "quoted_key"))
                .map(|n| {
                    let raw = &source[n.byte_range()];
                    // quoted_key: strip surrounding quotes
                    raw.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(raw)
                        .to_string()
                })
        }
        "pair" => {
            // The key in a pair is the first bare_key or quoted_key child.
            find_child_by_kind(node, "bare_key")
                .or_else(|| find_child_by_kind(node, "quoted_key"))
                .map(|n| {
                    let raw = &source[n.byte_range()];
                    raw.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(raw)
                        .to_string()
                })
        }
        _ => None,
    }
}

/// TOML language configuration.
///
/// Tables and table array elements act as their own body containers —
/// pairs are direct children of the table node.  The `"."` sentinel
/// in `body_fields` tells `find_body` to return the node itself.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_toml_ng::LANGUAGE.into(),
    extensions: &["toml"],
    kind_map: &[
        ("table", "table"),
        ("key", "pair"),
        ("array_table", "table_array_element"),
    ],
    name_field: "",
    name_overrides: &[],
    body_fields: &["."],
    custom_name: Some(toml_node_name),
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_TOML: &str = r#"[package]
name = "liyi"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"

[[test]]
name = "golden"
path = "tests/golden.rs"

[[test]]
name = "other"
"#;

    #[test]
    fn resolve_toml_table() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.package", Language::Toml);
        assert!(span.is_some(), "should resolve table::package");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("[package]"),
            "span should point to [package], got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_toml_key_in_table() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.package::key.name", Language::Toml);
        assert!(span.is_some(), "should resolve table::package::key.name");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("name = \"liyi\""),
            "span should point to name pair, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_toml_key_version() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.package::key.version", Language::Toml);
        assert!(span.is_some(), "should resolve table::package::key.version");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("version = \"0.1.0\""),
            "span should point to version pair, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_toml_dependency_key() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.dependencies::key.serde", Language::Toml);
        assert!(
            span.is_some(),
            "should resolve table::dependencies::key.serde"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("serde = \"1.0\""),
            "span should point to serde dependency, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_toml_array_table() {
        let span = resolve_tree_path(SAMPLE_TOML, "array_table.test", Language::Toml);
        assert!(span.is_some(), "should resolve array_table::test");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("[[test]]"),
            "span should point to [[test]], got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_toml_array_table_key() {
        let span = resolve_tree_path(SAMPLE_TOML, "array_table.test::key.name", Language::Toml);
        assert!(span.is_some(), "should resolve array_table::test.key::name");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TOML.lines().collect();
        assert!(
            lines[start - 1].contains("name = \"golden\""),
            "span should point to name pair in first [[test]], got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn compute_toml_table_path() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.package", Language::Toml).unwrap();
        let path = compute_tree_path(SAMPLE_TOML, span, Language::Toml);
        assert_eq!(path, "table.package");
    }

    #[test]
    fn compute_toml_key_path() {
        let span =
            resolve_tree_path(SAMPLE_TOML, "table.package::key.name", Language::Toml).unwrap();
        let path = compute_tree_path(SAMPLE_TOML, span, Language::Toml);
        assert_eq!(path, "table.package::key.name");
    }

    #[test]
    fn roundtrip_toml_table() {
        let span = resolve_tree_path(SAMPLE_TOML, "table.package", Language::Toml).unwrap();
        let path = compute_tree_path(SAMPLE_TOML, span, Language::Toml);
        assert_eq!(path, "table.package");
        let re_resolved = resolve_tree_path(SAMPLE_TOML, &path, Language::Toml).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_toml_key_in_table() {
        let span =
            resolve_tree_path(SAMPLE_TOML, "table.package::key.name", Language::Toml).unwrap();
        let path = compute_tree_path(SAMPLE_TOML, span, Language::Toml);
        assert_eq!(path, "table.package::key.name");
        let re_resolved = resolve_tree_path(SAMPLE_TOML, &path, Language::Toml).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_toml_array_table() {
        let span = resolve_tree_path(SAMPLE_TOML, "array_table.test", Language::Toml).unwrap();
        let path = compute_tree_path(SAMPLE_TOML, span, Language::Toml);
        assert_eq!(path, "array_table.test");
        let re_resolved = resolve_tree_path(SAMPLE_TOML, &path, Language::Toml).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn detect_toml_extension() {
        assert_eq!(
            detect_language(Path::new("Cargo.toml")),
            Some(Language::Toml)
        );
        assert_eq!(
            detect_language(Path::new("pyproject.toml")),
            Some(Language::Toml)
        );
    }
}
