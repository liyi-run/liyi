use super::LanguageConfig;

use tree_sitter::Node;

/// Detect Javadoc comments (`/** ... */` before a declaration).
fn java_has_doc_comment(node: &Node, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "block_comment" {
            let text = &source[s.byte_range()];
            if text.starts_with("/**") {
                return true;
            }
        }
        // Skip modifiers (public, static, etc.)
        if s.kind() == "modifiers" {
            sibling = s.prev_sibling();
            continue;
        }
        break;
    }
    false
}

/// Java language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_java::LANGUAGE.into(),
    extensions: &["java"],
    kind_map: &[
        ("fn", "method_declaration"),
        ("class", "class_declaration"),
        ("interface", "interface_declaration"),
        ("enum", "enum_declaration"),
        ("constructor", "constructor_declaration"),
        ("record", "record_declaration"),
        ("annotation", "annotation_type_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: Some(java_has_doc_comment),
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_JAVA: &str = r#"package com.example;

public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public Calculator() {
        // constructor
    }
}

interface Computable {
    int compute(int x);
}

enum Direction {
    NORTH, SOUTH, EAST, WEST
}

record Point(int x, int y) {}
"#;

    #[test]
    fn resolve_java_class() {
        let span = resolve_tree_path(SAMPLE_JAVA, "class::Calculator", Language::Java);
        assert!(span.is_some(), "should resolve class::Calculator");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JAVA.lines().collect();
        assert!(
            lines[start - 1].contains("class Calculator"),
            "span should point to Calculator class"
        );
    }

    #[test]
    fn resolve_java_method() {
        let span = resolve_tree_path(SAMPLE_JAVA, "class::Calculator::fn::add", Language::Java);
        assert!(span.is_some(), "should resolve class::Calculator::fn::add");
    }

    #[test]
    fn resolve_java_constructor() {
        let span = resolve_tree_path(
            SAMPLE_JAVA,
            "class::Calculator::constructor::Calculator",
            Language::Java,
        );
        assert!(span.is_some(), "should resolve constructor");
    }

    #[test]
    fn resolve_java_interface() {
        let span = resolve_tree_path(SAMPLE_JAVA, "interface::Computable", Language::Java);
        assert!(span.is_some(), "should resolve interface::Computable");
    }

    #[test]
    fn resolve_java_enum() {
        let span = resolve_tree_path(SAMPLE_JAVA, "enum::Direction", Language::Java);
        assert!(span.is_some(), "should resolve enum::Direction");
    }

    #[test]
    fn resolve_java_record() {
        let span = resolve_tree_path(SAMPLE_JAVA, "record::Point", Language::Java);
        assert!(span.is_some(), "should resolve record::Point");
    }

    #[test]
    fn roundtrip_java() {
        let span =
            resolve_tree_path(SAMPLE_JAVA, "class::Calculator::fn::add", Language::Java).unwrap();
        let path = compute_tree_path(SAMPLE_JAVA, span, Language::Java);
        assert_eq!(path, "class::Calculator::fn::add");
    }

    #[test]
    fn detect_java_extension() {
        assert_eq!(
            detect_language(Path::new("Main.java")),
            Some(Language::Java)
        );
    }
}
