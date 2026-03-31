use super::LanguageConfig;

use tree_sitter::Node;

/// Swift language configuration.
/// Detect Swift doc comments (`/// ...` and `/** ... */`).
///
/// Swift's tree-sitter grammar uses `comment` for line comments (both `//`
/// and `///`) and `multiline_comment` for block comments (both `/* */` and
/// `/** */`). We check for `///` prefix in `comment` and `/**` prefix in
/// `multiline_comment`. `modifiers` siblings are skipped.
fn swift_has_doc_comment(node: &Node, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("///") {
                    return true;
                }
                sibling = s.prev_sibling();
            }
            "multiline_comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("/**") {
                    return true;
                }
                sibling = s.prev_sibling();
            }
            "modifiers" => {
                sibling = s.prev_sibling();
            }
            _ => break,
        }
    }
    false
}
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_swift::LANGUAGE.into(),
    extensions: &["swift"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("protocol", "protocol_declaration"),
        ("enum", "enum_entry"),
        ("property", "property_declaration"),
        ("init", "init_declaration"),
        ("typealias", "typealias_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: Some(swift_has_doc_comment),
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_SWIFT: &str = r#"protocol Drawable {
    func draw()
}

class Shape {
    func area() -> Double {
        return 0.0
    }

    init() {}
}

func standalone() -> Int {
    return 42
}

typealias Callback = () -> Void
"#;

    #[test]
    fn resolve_swift_protocol() {
        let span = resolve_tree_path(SAMPLE_SWIFT, "protocol.Drawable", Language::Swift);
        assert!(span.is_some(), "should resolve protocol.Drawable");
    }

    #[test]
    fn resolve_swift_class() {
        let span = resolve_tree_path(SAMPLE_SWIFT, "class.Shape", Language::Swift);
        assert!(span.is_some(), "should resolve class.Shape");
    }

    #[test]
    fn resolve_swift_method() {
        let span = resolve_tree_path(SAMPLE_SWIFT, "class.Shape::fn.area", Language::Swift);
        assert!(span.is_some(), "should resolve class.Shape::fn.area");
    }

    #[test]
    fn resolve_swift_function() {
        let span = resolve_tree_path(SAMPLE_SWIFT, "fn.standalone", Language::Swift);
        assert!(span.is_some(), "should resolve fn.standalone");
    }

    #[test]
    fn roundtrip_swift() {
        let span = resolve_tree_path(SAMPLE_SWIFT, "fn.standalone", Language::Swift).unwrap();
        let path = compute_tree_path(SAMPLE_SWIFT, span, Language::Swift);
        assert_eq!(path, "fn.standalone");
    }

    #[test]
    fn detect_swift_extension() {
        assert_eq!(
            detect_language(Path::new("ViewController.swift")),
            Some(Language::Swift)
        );
    }
}
