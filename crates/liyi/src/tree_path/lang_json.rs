use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for JSON `pair` nodes.
///
/// JSON keys are `string` nodes with inner `string_content` children.
/// The key field of a `pair` is a `string` whose text includes surrounding
/// quotes; we drill into the `string_content` child to extract the bare name.
fn json_node_name(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "pair" {
        return None;
    }
    let key_node = node.child_by_field_name("key")?;
    // key_node is of kind "string".  Its named child "string_content"
    // carries the unquoted key text.
    let mut cursor = key_node.walk();
    key_node
        .children(&mut cursor)
        .find(|c| c.kind() == "string_content")
        .map(|sc| source[sc.byte_range()].to_string())
}

/// JSON language configuration.
///
/// JSON has a single item kind: `pair` (object key-value entries).
/// Nesting follows the `value` field — when a pair's value is an `object`,
/// the resolver descends into it to find nested pairs.
///
/// The root `document` node wraps the top-level `object` (or `array`);
/// `object` and `array` are listed as transparent kinds so the resolver
/// looks through them to reach `pair` nodes.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_json::LANGUAGE.into(),
    extensions: &["json"],
    kind_map: &[("key", "pair")],
    name_field: "",
    name_overrides: &[],
    body_fields: &["value"],
    custom_name: Some(json_node_name),
    doc_comment_detector: None,
    transparent_kinds: &["object", "array"],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_JSON: &str = r#"{
  "name": "liyi",
  "version": "0.1.0",
  "specs": [
    {"item": "foo", "intent": "do foo"},
    {"item": "bar", "intent": "do bar"}
  ],
  "nested": {
    "deep": {
      "value": 42
    }
  }
}"#;

    #[test]
    fn resolve_json_top_level_key() {
        let span = resolve_tree_path(SAMPLE_JSON, "key::name", Language::Json);
        assert!(span.is_some(), "should resolve key::name");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JSON.lines().collect();
        assert!(
            lines[start - 1].contains("\"name\": \"liyi\""),
            "span should point to name pair, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_json_nested_key() {
        let span = resolve_tree_path(SAMPLE_JSON, "key::nested::key::deep", Language::Json);
        assert!(span.is_some(), "should resolve key::nested::key::deep");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JSON.lines().collect();
        assert!(
            lines[start - 1].contains("\"deep\""),
            "span should point to deep pair, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_json_deeply_nested_key() {
        let span = resolve_tree_path(
            SAMPLE_JSON,
            "key::nested::key::deep::key::value",
            Language::Json,
        );
        assert!(
            span.is_some(),
            "should resolve key::nested::key::deep::key::value"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JSON.lines().collect();
        assert!(
            lines[start - 1].contains("\"value\": 42"),
            "span should point to value pair, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_json_indexed_array_element() {
        // specs[1] → second element → {"item": "bar", ...}
        let span = resolve_tree_path(SAMPLE_JSON, "key::specs[1]::key::item", Language::Json);
        assert!(span.is_some(), "should resolve key::specs[1]::key::item");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JSON.lines().collect();
        assert!(
            lines[start - 1].contains("\"item\": \"bar\""),
            "span should point to item pair in second array element, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn compute_json_top_level_key() {
        let span = resolve_tree_path(SAMPLE_JSON, "key::name", Language::Json).unwrap();
        let path = compute_tree_path(SAMPLE_JSON, span, Language::Json);
        assert_eq!(path, "key::name");
    }

    #[test]
    fn compute_json_nested_key() {
        let span = resolve_tree_path(
            SAMPLE_JSON,
            "key::nested::key::deep::key::value",
            Language::Json,
        )
        .unwrap();
        let path = compute_tree_path(SAMPLE_JSON, span, Language::Json);
        assert_eq!(path, "key::nested::key::deep::key::value");
    }

    #[test]
    fn roundtrip_json_top_level() {
        let span = resolve_tree_path(SAMPLE_JSON, "key::name", Language::Json).unwrap();
        let path = compute_tree_path(SAMPLE_JSON, span, Language::Json);
        assert_eq!(path, "key::name");
        let re_resolved = resolve_tree_path(SAMPLE_JSON, &path, Language::Json).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_json_nested() {
        let span =
            resolve_tree_path(SAMPLE_JSON, "key::nested::key::deep", Language::Json).unwrap();
        let path = compute_tree_path(SAMPLE_JSON, span, Language::Json);
        assert_eq!(path, "key::nested::key::deep");
        let re_resolved = resolve_tree_path(SAMPLE_JSON, &path, Language::Json).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn detect_json_extension() {
        assert_eq!(
            detect_language(Path::new("package.json")),
            Some(Language::Json)
        );
        assert_eq!(
            detect_language(Path::new("schema/liyi.schema.json")),
            Some(Language::Json)
        );
    }
}
