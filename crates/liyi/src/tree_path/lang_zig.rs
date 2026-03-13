use super::LanguageConfig;

use tree_sitter::Node;

/// Find the first child with a given kind.
fn find_child_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

/// Custom name extraction for Zig nodes.
///
/// Handles two Zig-specific patterns:
/// - `variable_declaration` with `const` qualifier holding a `struct_declaration`:
///   emits `struct::Name` instead of `const::Name` to support Zig's struct-as-namespace pattern.
/// - `function_declaration`: extracts name from child `identifier` node.
/// - `test_declaration`: extracts the name from the string literal.
fn zig_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_declaration" => {
            // Find the identifier child which is the function name
            find_child_by_kind(node, "identifier").map(|n| source[n.byte_range()].to_string())
        }
        "variable_declaration" => {
            // Check if this is a `const` declaration
            let is_const = node.children(&mut node.walk()).any(|c| c.kind() == "const");

            if !is_const {
                return None;
            }

            // Check if the value is a struct_declaration
            let has_struct = node
                .children(&mut node.walk())
                .any(|c| c.kind() == "struct_declaration");

            if has_struct {
                // This is `const Name = struct { ... }` — extract just the name
                // (the "struct." prefix is added by compute_tree_path)
                find_child_by_kind(node, "identifier").map(|n| source[n.byte_range()].to_string())
            } else {
                None
            }
        }
        "test_declaration" => {
            // Test declarations have a string child for the name
            // e.g., test "my test" { ... }
            find_child_by_kind(node, "string").map(|n| {
                let raw = &source[n.byte_range()];
                // Remove surrounding quotes
                raw.strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            })
        }
        _ => None,
    }
}

/// Zig language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_zig::LANGUAGE.into(),
    extensions: &["zig"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("struct", "variable_declaration"), // const Name = struct { ... }
        ("test", "test_declaration"),
    ],
    name_field: "", // Not used - we extract names via custom callback
    name_overrides: &[],
    // Zig uses "block" for function bodies, and "struct_declaration" is the
    // container for struct-as-namespace contents (methods, fields).
    body_fields: &["block", "struct_declaration"],
    custom_name: Some(zig_node_name),
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_ZIG: &str = r#"const std = @import("std");

const Point = struct {
    x: i32,
    y: i32,

    pub fn new(x: i32, y: i32) Point {
        return Point{ .x = x, .y = y };
    }
};

const MAX_SIZE = 100;

fn add(a: i32, b: i32) i32 {
    return a + b;
}

test "add function" {
    try std.testing.expectEqual(add(2, 3), 5);
}
"#;

    #[test]
    fn resolve_zig_function() {
        let span = resolve_tree_path(SAMPLE_ZIG, "fn.add", Language::Zig);
        assert!(span.is_some(), "should resolve fn::add");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_ZIG.lines().collect();
        assert!(
            lines[start - 1].contains("fn add("),
            "span should point to add function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_zig_struct_as_namespace() {
        let span = resolve_tree_path(SAMPLE_ZIG, "struct.Point", Language::Zig);
        assert!(span.is_some(), "should resolve struct::Point");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_ZIG.lines().collect();
        assert!(
            lines[start - 1].contains("const Point = struct"),
            "span should point to Point struct definition"
        );
    }

    #[test]
    fn resolve_zig_method_in_struct() {
        let span = resolve_tree_path(SAMPLE_ZIG, "struct.Point::fn.new", Language::Zig);
        assert!(span.is_some(), "should resolve method in struct");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_ZIG.lines().collect();
        assert!(
            lines[start - 1].contains("fn new("),
            "span should point to new method"
        );
    }

    #[test]
    fn resolve_zig_test() {
        let span = resolve_tree_path(SAMPLE_ZIG, "test.\"add function\"", Language::Zig);
        assert!(span.is_some(), "should resolve test declaration");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_ZIG.lines().collect();
        assert!(
            lines[start - 1].contains("test \"add function\""),
            "span should point to test declaration"
        );
    }

    #[test]
    fn compute_zig_function_path() {
        let resolved_span = resolve_tree_path(SAMPLE_ZIG, "fn.add", Language::Zig).unwrap();
        let path = compute_tree_path(SAMPLE_ZIG, resolved_span, Language::Zig);
        assert_eq!(path, "fn.add");
    }

    #[test]
    fn compute_zig_struct_namespace_path() {
        let resolved_span = resolve_tree_path(SAMPLE_ZIG, "struct.Point", Language::Zig).unwrap();
        let path = compute_tree_path(SAMPLE_ZIG, resolved_span, Language::Zig);
        assert_eq!(path, "struct.Point");
    }

    #[test]
    fn roundtrip_zig() {
        let resolved_span = resolve_tree_path(SAMPLE_ZIG, "fn.add", Language::Zig).unwrap();

        let computed_path = compute_tree_path(SAMPLE_ZIG, resolved_span, Language::Zig);
        assert_eq!(computed_path, "fn.add");

        let re_resolved = resolve_tree_path(SAMPLE_ZIG, &computed_path, Language::Zig).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn roundtrip_zig_struct_namespace() {
        let resolved_span =
            resolve_tree_path(SAMPLE_ZIG, "struct.Point::fn.new", Language::Zig).unwrap();

        let computed_path = compute_tree_path(SAMPLE_ZIG, resolved_span, Language::Zig);
        assert_eq!(computed_path, "struct.Point::fn.new");

        let re_resolved = resolve_tree_path(SAMPLE_ZIG, &computed_path, Language::Zig).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn detect_zig_extensions() {
        assert_eq!(detect_language(Path::new("main.zig")), Some(Language::Zig));
        assert_eq!(
            detect_language(Path::new("lib/foo.zig")),
            Some(Language::Zig)
        );
    }
}
