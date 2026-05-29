use super::LanguageConfig;
use super::lang_c::c_extract_declarator_name;

use tree_sitter::Node;

/// Custom name extraction for Objective-C nodes.
///
/// ObjC node types like `class_interface`, `class_implementation`,
/// `protocol_declaration`, `method_declaration`, and `method_definition`
/// do not use standard `name` fields. Their names are extracted from
/// specific child node patterns. Categories (`@interface Foo (Cat)`) encode
/// the category into the name as `Foo (Cat)` so they do not collide with the
/// base `@interface Foo` / `@implementation Foo`.
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
            let base = node
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| source[c.byte_range()].to_string())?;
            // Categories (`@interface Foo (Cat)`) carry a `category` field.
            // Encode it as `Foo (Cat)` so a category and the base interface
            // (or implementation) resolve to distinct tree_paths instead of
            // colliding on the class name alone.
            match node.child_by_field_name("category") {
                Some(cat) => Some(format!("{base} ({})", &source[cat.byte_range()])),
                None => Some(base),
            }
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
/// Detect Objective-C doc comments (`/** ... */` and `/// ...`).
///
/// ObjC's tree-sitter grammar uses a uniform `comment` kind. We check for
/// `/**` (HeaderDoc/Javadoc) and `///` (Doxygen) prefixes.
fn objc_has_doc_comment(node: &Node, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = &source[s.byte_range()];
            if text.starts_with("/**") || text.starts_with("///") {
                return true;
            }
            sibling = s.prev_sibling();
        } else {
            break;
        }
    }
    false
}
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
    doc_comment_detector: Some(objc_has_doc_comment),
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
        let span = resolve_tree_path(SAMPLE_OBJC, "fn.helper", Language::ObjectiveC);
        assert!(span.is_some(), "should resolve fn.helper");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_OBJC.lines().collect();
        assert!(
            lines[start - 1].contains("void helper"),
            "span should point to helper function"
        );
    }

    #[test]
    fn resolve_objc_struct() {
        let span = resolve_tree_path(SAMPLE_OBJC, "struct.CGPoint", Language::ObjectiveC);
        assert!(span.is_some(), "should resolve struct.CGPoint");
    }

    #[test]
    fn roundtrip_objc() {
        let span = resolve_tree_path(SAMPLE_OBJC, "fn.helper", Language::ObjectiveC).unwrap();
        let path = compute_tree_path(SAMPLE_OBJC, span, Language::ObjectiveC);
        assert_eq!(path, "fn.helper");
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

    /// A base class and a category on that class — verify the two resolve to
    /// distinct tree_paths instead of colliding on the class name.
    const SAMPLE_OBJC_CATEGORY: &str = r#"@interface Widget : NSObject
- (void)draw;
@end

@implementation Widget
- (void)draw {}
@end

@interface Widget (Layout)
- (void)layout;
@end

@implementation Widget (Layout)
- (void)layout {}
@end
"#;

    #[test]
    fn base_and_category_resolve_to_distinct_blocks() {
        let lines: Vec<&str> = SAMPLE_OBJC_CATEGORY.lines().collect();

        let base = resolve_tree_path(SAMPLE_OBJC_CATEGORY, "class.Widget", Language::ObjectiveC)
            .expect("should resolve base class.Widget");
        assert!(
            lines[base[0] - 1].contains("@interface Widget : NSObject"),
            "base span should start at `@interface Widget : NSObject`, got: {}",
            lines[base[0] - 1]
        );

        let category = resolve_tree_path(
            SAMPLE_OBJC_CATEGORY,
            "class.\"Widget (Layout)\"",
            Language::ObjectiveC,
        )
        .expect("should resolve category by encoded name");
        assert!(
            lines[category[0] - 1].contains("@interface Widget (Layout)"),
            "category span should start at `@interface Widget (Layout)`, got: {}",
            lines[category[0] - 1]
        );

        assert_ne!(
            base, category,
            "base interface and category must resolve to different spans"
        );
    }

    #[test]
    fn compute_category_path_is_distinct_from_base() {
        let lines: Vec<&str> = SAMPLE_OBJC_CATEGORY.lines().collect();

        let base_start = lines
            .iter()
            .position(|l| l.contains("@interface Widget : NSObject"))
            .unwrap()
            + 1;
        let base_end = base_start
            + lines[base_start..]
                .iter()
                .position(|l| l.trim() == "@end")
                .map(|i| i + 1)
                .unwrap();
        let base_path = compute_tree_path(
            SAMPLE_OBJC_CATEGORY,
            [base_start, base_end],
            Language::ObjectiveC,
        );
        assert_eq!(base_path, "class.Widget");

        let cat_start = lines
            .iter()
            .position(|l| l.contains("@interface Widget (Layout)"))
            .unwrap()
            + 1;
        let cat_end = cat_start
            + lines[cat_start..]
                .iter()
                .position(|l| l.trim() == "@end")
                .map(|i| i + 1)
                .unwrap();
        let cat_path = compute_tree_path(
            SAMPLE_OBJC_CATEGORY,
            [cat_start, cat_end],
            Language::ObjectiveC,
        );
        assert_eq!(cat_path, "class.\"Widget (Layout)\"");

        assert_ne!(base_path, cat_path);
    }
}
