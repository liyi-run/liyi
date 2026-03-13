use super::LanguageConfig;
use super::lang_c::c_extract_declarator_name;

use tree_sitter::Node;

/// Custom name extraction for Objective-C nodes.
///
/// ObjC node types like `class_interface`, `class_implementation`,
/// `protocol_declaration`, `method_declaration`, and `method_definition`
/// do not use standard `name` fields. Their names are extracted from
/// specific child node patterns.
fn objc_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        // C function definitions use the same declarator chain as C.
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" => {
            let declarator = node.child_by_field_name("declarator")?;
            Some(source[declarator.byte_range()].to_string())
        }
        // @interface ClassName or @interface ClassName (Category)
        "class_interface" | "class_implementation" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        // @protocol ProtocolName
        "protocol_declaration" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        // - (ReturnType)methodName or - (ReturnType)methodName:(Type)arg
        // + (ReturnType)classMethodName
        "method_declaration" | "method_definition" => {
            let mut cursor = node.walk();
            // The selector is composed of keyword_declarator children or
            // a single identifier (for zero-argument methods).
            let mut parts: Vec<String> = Vec::new();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "identifier" | "field_identifier" if parts.is_empty() => {
                        // Single-part selector (no arguments)
                        parts.push(source[child.byte_range()].to_string());
                    }
                    "keyword_declarator" => {
                        // Each keyword_declarator has a keyword child
                        let mut kw_cursor = child.walk();
                        if let Some(kw) = child
                            .children(&mut kw_cursor)
                            .find(|c| c.kind() == "keyword_selector" || c.kind() == "identifier")
                        {
                            parts.push(format!("{}:", &source[kw.byte_range()]));
                        }
                    }
                    _ => {}
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(""))
            }
        }
        _ => None,
    }
}

/// Objective-C language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_objc::LANGUAGE.into(),
    extensions: &["m", "mm"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_interface"),
        ("impl", "class_implementation"),
        ("protocol", "protocol_declaration"),
        ("method", "method_definition"),
        ("method_decl", "method_declaration"),
        ("struct", "struct_specifier"),
        ("enum", "enum_specifier"),
        ("typedef", "type_definition"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(objc_node_name),
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_OBJC: &str = r#"#import <Foundation/Foundation.h>

struct CGPoint {
    float x;
    float y;
};

void helper(void) {
    NSLog(@"hello");
}
"#;

    #[test]
    fn resolve_objc_function() {
        let span = resolve_tree_path(SAMPLE_OBJC, "fn::helper", Language::ObjectiveC);
        assert!(span.is_some(), "should resolve fn::helper");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_OBJC.lines().collect();
        assert!(
            lines[start - 1].contains("void helper"),
            "span should point to helper function"
        );
    }

    #[test]
    fn resolve_objc_struct() {
        let span = resolve_tree_path(SAMPLE_OBJC, "struct::CGPoint", Language::ObjectiveC);
        assert!(span.is_some(), "should resolve struct::CGPoint");
    }

    #[test]
    fn roundtrip_objc() {
        let span = resolve_tree_path(SAMPLE_OBJC, "fn::helper", Language::ObjectiveC).unwrap();
        let path = compute_tree_path(SAMPLE_OBJC, span, Language::ObjectiveC);
        assert_eq!(path, "fn::helper");
    }

    #[test]
    fn detect_objc_extensions() {
        assert_eq!(
            detect_language(Path::new("AppDelegate.m")),
            Some(Language::ObjectiveC)
        );
        assert_eq!(
            detect_language(Path::new("mixed.mm")),
            Some(Language::ObjectiveC)
        );
    }
}
