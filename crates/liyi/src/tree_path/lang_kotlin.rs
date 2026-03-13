use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for Kotlin nodes.
///
/// Handles `property_declaration` where the name is in a child
/// `variable_declaration` node, and `type_alias` where the name is
/// in an `identifier` child before the `=` (the `type` field is the RHS).
fn kotlin_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "property_declaration" => {
            let mut cursor = node.walk();
            // Name is in the first variable_declaration or identifier child
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declaration" {
                    let name = child.child_by_field_name("name").or_else(|| {
                        let mut c2 = child.walk();
                        child
                            .children(&mut c2)
                            .find(|c| c.kind() == "simple_identifier")
                    })?;
                    return Some(source[name.byte_range()].to_string());
                }
                if child.kind() == "simple_identifier" {
                    return Some(source[child.byte_range()].to_string());
                }
            }
            None
        }
        "type_alias" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier" || c.kind() == "simple_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        _ => None,
    }
}

/// Kotlin language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_kotlin_ng::LANGUAGE.into(),
    extensions: &["kt", "kts"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("object", "object_declaration"),
        ("property", "property_declaration"),
        ("typealias", "type_alias"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body", "class_body"],
    custom_name: Some(kotlin_node_name),
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_KOTLIN: &str = r#"class Calculator {
    fun add(a: Int, b: Int): Int {
        return a + b
    }
}

object Singleton {
    fun instance(): Singleton = this
}

fun standalone(): Int {
    return 42
}

typealias StringList = List<String>
"#;

    #[test]
    fn resolve_kotlin_class() {
        let span = resolve_tree_path(SAMPLE_KOTLIN, "class.Calculator", Language::Kotlin);
        assert!(span.is_some(), "should resolve class.Calculator");
    }

    #[test]
    fn resolve_kotlin_method() {
        let span = resolve_tree_path(SAMPLE_KOTLIN, "class.Calculator::fn.add", Language::Kotlin);
        assert!(span.is_some(), "should resolve class.Calculator::fn.add");
    }

    #[test]
    fn resolve_kotlin_object() {
        let span = resolve_tree_path(SAMPLE_KOTLIN, "object.Singleton", Language::Kotlin);
        assert!(span.is_some(), "should resolve object.Singleton");
    }

    #[test]
    fn resolve_kotlin_function() {
        let span = resolve_tree_path(SAMPLE_KOTLIN, "fn.standalone", Language::Kotlin);
        assert!(span.is_some(), "should resolve fn.standalone");
    }

    #[test]
    fn roundtrip_kotlin() {
        let span = resolve_tree_path(SAMPLE_KOTLIN, "fn.standalone", Language::Kotlin).unwrap();
        let path = compute_tree_path(SAMPLE_KOTLIN, span, Language::Kotlin);
        assert_eq!(path, "fn.standalone");
    }

    #[test]
    fn detect_kotlin_extension() {
        assert_eq!(
            detect_language(Path::new("Main.kt")),
            Some(Language::Kotlin)
        );
        assert_eq!(
            detect_language(Path::new("build.gradle.kts")),
            Some(Language::Kotlin)
        );
    }
}
