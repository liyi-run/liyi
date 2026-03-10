use super::LanguageConfig;
use super::lang_c::c_extract_declarator_name;

use tree_sitter::Node;

/// Custom name extraction for C++ nodes.
///
/// Extends `c_node_name` with C++-specific patterns:
/// - `template_declaration`: transparent wrapper — extracts name from inner decl.
/// - `namespace_definition`: name is in a `namespace_identifier` child (no "name" field).
fn cpp_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" | "alias_declaration" => {
            let name_node = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("declarator"))?;
            Some(source[name_node.byte_range()].to_string())
        }
        "template_declaration" => {
            // template_declaration wraps an inner declaration — find it and
            // extract the name from the inner node.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_definition" => return c_extract_declarator_name(&child, source),
                    "class_specifier" | "struct_specifier" | "enum_specifier"
                    | "concept_definition" | "alias_declaration" => {
                        let n = child.child_by_field_name("name")?;
                        return Some(source[n.byte_range()].to_string());
                    }
                    // A template can also wrap another template_declaration (nested)
                    "template_declaration" => return cpp_node_name(&child, source),
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

/// C++ language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_cpp::LANGUAGE.into(),
    extensions: &["cpp", "cc", "cxx", "h", "hpp", "hh", "hxx", "h++", "c++"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_specifier"),
        ("struct", "struct_specifier"),
        ("namespace", "namespace_definition"),
        ("enum", "enum_specifier"),
        ("template", "template_declaration"),
        ("typedef", "type_definition"),
        ("using", "alias_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body", "declaration_list"],
    custom_name: Some(cpp_node_name),
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_CPP: &str = r#"namespace math {

class Calculator {
public:
    int add(int a, int b) {
        return a + b;
    }
};

struct Point {
    int x, y;
};

enum class Color { Red, Green, Blue };

}

void standalone() {}
"#;

    #[test]
    fn resolve_cpp_namespace() {
        let span = resolve_tree_path(SAMPLE_CPP, "namespace::math", Language::Cpp);
        assert!(span.is_some(), "should resolve namespace::math");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_CPP.lines().collect();
        assert!(
            lines[start - 1].contains("namespace math"),
            "span should point to namespace math, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_cpp_class_in_namespace() {
        let span = resolve_tree_path(
            SAMPLE_CPP,
            "namespace::math::class::Calculator",
            Language::Cpp,
        );
        assert!(
            span.is_some(),
            "should resolve namespace::math::class::Calculator"
        );
    }

    #[test]
    fn resolve_cpp_method_in_class() {
        let span = resolve_tree_path(
            SAMPLE_CPP,
            "namespace::math::class::Calculator::fn::add",
            Language::Cpp,
        );
        assert!(span.is_some(), "should resolve nested method");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_CPP.lines().collect();
        assert!(
            lines[start - 1].contains("add"),
            "span should point to add method"
        );
    }

    #[test]
    fn resolve_cpp_standalone() {
        let span = resolve_tree_path(SAMPLE_CPP, "fn::standalone", Language::Cpp);
        assert!(span.is_some(), "should resolve fn::standalone");
    }

    #[test]
    fn resolve_cpp_enum() {
        let span = resolve_tree_path(SAMPLE_CPP, "namespace::math::enum::Color", Language::Cpp);
        assert!(span.is_some(), "should resolve enum in namespace");
    }

    #[test]
    fn roundtrip_cpp() {
        let span = resolve_tree_path(SAMPLE_CPP, "fn::standalone", Language::Cpp).unwrap();
        let path = compute_tree_path(SAMPLE_CPP, span, Language::Cpp);
        assert_eq!(path, "fn::standalone");
    }

    #[test]
    fn detect_cpp_extensions() {
        assert_eq!(detect_language(Path::new("main.cpp")), Some(Language::Cpp));
        assert_eq!(detect_language(Path::new("main.cc")), Some(Language::Cpp));
        assert_eq!(
            detect_language(Path::new("header.hpp")),
            Some(Language::Cpp)
        );
        assert_eq!(detect_language(Path::new("header.h")), Some(Language::Cpp));
    }
}
