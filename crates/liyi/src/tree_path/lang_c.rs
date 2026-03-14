use tree_sitter::Node;

/// Extract the function name from a C/C++ `function_definition` node.
///
/// C/C++ functions store their name inside the `declarator` field chain:
/// `function_definition` → (field `declarator`) `function_declarator`
/// → (field `declarator`) `identifier` / `field_identifier`.
/// Pointer declarators and other wrappers may appear in the chain;
/// we unwrap them until we find a `function_declarator`.
pub(super) fn c_extract_declarator_name(node: &Node, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    let func_decl = unwrap_to_function_declarator(&declarator)?;
    let name_node = func_decl.child_by_field_name("declarator")?;
    Some(source[name_node.byte_range()].to_string())
}

/// Walk through pointer_declarator / parenthesized_declarator / attributed_declarator
/// wrappers to find the inner `function_declarator`.
fn unwrap_to_function_declarator<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    match node.kind() {
        "function_declarator" => Some(*node),
        "pointer_declarator" | "parenthesized_declarator" | "attributed_declarator" => {
            let inner = node.child_by_field_name("declarator")?;
            unwrap_to_function_declarator(&inner)
        }
        _ => None,
    }
}

/// Custom name extraction for C nodes.
///
/// Handles `function_definition` (name in declarator chain) and
/// `type_definition` (name in declarator field, which is a type_identifier).
fn c_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" => {
            // typedef: the 'declarator' field holds the new type name
            let declarator = node.child_by_field_name("declarator")?;
            Some(source[declarator.byte_range()].to_string())
        }
        _ => None,
    }
}

// C language configuration.
declare_language! {
    /// C language configuration.
    pub(super) static CONFIG {
        ts_language: || tree_sitter_c::LANGUAGE.into(),
        extensions: ["c"],
        kind_map: [
            ("fn", "function_definition"),
            ("struct", "struct_specifier"),
            ("enum", "enum_specifier"),
            ("typedef", "type_definition"),
        ],
        name_field: "name",
        name_overrides: [],
        body_fields: ["body"],
        custom_name: Some(c_node_name),
        doc_comment_detector: None,
        transparent_kinds: [],
    }
}

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_C: &str = r#"#include <stdio.h>

struct Point {
    int x;
    int y;
};

enum Color { RED, GREEN, BLUE };

typedef struct Point Point_t;

void process(int x, int y) {
    printf("hello");
}

static int helper(void) {
    return 42;
}
"#;

    #[test]
    fn resolve_c_function() {
        let span = resolve_tree_path(SAMPLE_C, "fn.process", Language::C);
        assert!(span.is_some(), "should resolve fn.process");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_C.lines().collect();
        assert!(
            lines[start - 1].contains("void process"),
            "span should point to process function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_c_struct() {
        let span = resolve_tree_path(SAMPLE_C, "struct.Point", Language::C);
        assert!(span.is_some(), "should resolve struct.Point");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_C.lines().collect();
        assert!(
            lines[start - 1].contains("struct Point"),
            "span should point to Point struct"
        );
    }

    #[test]
    fn resolve_c_enum() {
        let span = resolve_tree_path(SAMPLE_C, "enum.Color", Language::C);
        assert!(span.is_some(), "should resolve enum.Color");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_C.lines().collect();
        assert!(
            lines[start - 1].contains("enum Color"),
            "span should point to Color enum"
        );
    }

    #[test]
    fn resolve_c_typedef() {
        let span = resolve_tree_path(SAMPLE_C, "typedef.Point_t", Language::C);
        assert!(span.is_some(), "should resolve typedef.Point_t");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_C.lines().collect();
        assert!(
            lines[start - 1].contains("typedef"),
            "span should point to typedef"
        );
    }

    #[test]
    fn compute_c_function_path() {
        let span = resolve_tree_path(SAMPLE_C, "fn.process", Language::C).unwrap();
        let path = compute_tree_path(SAMPLE_C, span, Language::C);
        assert_eq!(path, "fn.process");
    }

    #[test]
    fn roundtrip_c() {
        for tp in &["fn.process", "fn.helper", "struct.Point", "enum.Color"] {
            let span = resolve_tree_path(SAMPLE_C, tp, Language::C).unwrap();
            let path = compute_tree_path(SAMPLE_C, span, Language::C);
            assert_eq!(&path, tp, "roundtrip failed for {tp}");
        }
    }

    #[test]
    fn detect_c_extensions() {
        assert_eq!(detect_language(Path::new("main.c")), Some(Language::C));
    }
}
