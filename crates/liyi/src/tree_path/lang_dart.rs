use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for Dart nodes.
///
/// Handles two patterns that differ from the standard `name` field lookup:
/// - `extension_type_declaration`: the `name` field yields an
///   `extension_type_name` wrapper node containing the actual `identifier`.
/// - `constructor_signature` / `constant_constructor_signature` /
///   `factory_constructor_signature` / `redirecting_factory_constructor_signature`:
///   the `name` field spans `ClassName.namedPart`; we extract only the full
///   qualified text so the tree_path reads `constructor::ClassName.named`.
fn dart_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "extension_type_declaration" => {
            // name: (extension_type_name (identifier))
            let name_wrapper = node.child_by_field_name("name")?;
            let mut cursor = name_wrapper.walk();
            name_wrapper
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .map(|id| source[id.byte_range()].to_string())
        }
        _ => None,
    }
}

/// Detect Dart doc comments (`///` or `/** ... */` before a declaration).
fn dart_has_doc_comment(node: &Node, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("///") {
                    return true;
                }
            }
            "documentation_block_comment" => return true,
            "annotation" => {
                // Skip annotations (e.g., @override) preceding the item
                sibling = s.prev_sibling();
                continue;
            }
            _ => break,
        }
        sibling = s.prev_sibling();
    }
    false
}

/// Dart language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_dart::LANGUAGE.into(),
    extensions: &["dart"],
    kind_map: &[
        ("fn", "function_signature"),
        ("class", "class_declaration"),
        ("mixin", "mixin_declaration"),
        ("extension", "extension_declaration"),
        ("extension_type", "extension_type_declaration"),
        ("enum", "enum_declaration"),
        ("getter", "getter_signature"),
        ("setter", "setter_signature"),
        ("constructor", "constructor_signature"),
        ("const_constructor", "constant_constructor_signature"),
        ("factory", "factory_constructor_signature"),
        ("factory_redirect", "redirecting_factory_constructor_signature"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(dart_node_name),
    doc_comment_detector: Some(dart_has_doc_comment),
    transparent_kinds: &["class_member", "method_signature", "declaration"],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_DART: &str = r#"/// A point in 2D space.
class Point {
  final double x;
  final double y;

  /// Creates a new point.
  Point(this.x, this.y);

  /// Named constructor from origin.
  Point.origin() : x = 0, y = 0;

  /// Distance from origin.
  double get distanceFromOrigin => (x * x + y * y);

  /// Setter example.
  set label(String value) {}

  /// Move the point.
  void move(double dx, double dy) {}

  /// Static factory.
  static Point zero() {
    return Point(0, 0);
  }
}

mixin Serializable {
  String serialize() {
    return toString();
  }
}

extension StringExt on String {
  bool get isBlank => isEmpty;

  String reverse() {
    return split('').reversed.join('');
  }
}

extension type Meters(double value) implements double {}

enum Color {
  red,
  green,
  blue;

  String get label => name;
}

/// A top-level function.
void main() {
  print('hello');
}

/// A top-level getter.
int get globalCount => 42;
"#;

    #[test]
    fn resolve_dart_class() {
        let span = resolve_tree_path(SAMPLE_DART, "class::Point", Language::Dart);
        assert!(span.is_some(), "should resolve class::Point");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_DART.lines().collect();
        assert!(
            lines[start - 1].contains("class Point"),
            "span should point to Point class, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_dart_method() {
        let span = resolve_tree_path(SAMPLE_DART, "class::Point::fn::move", Language::Dart);
        assert!(span.is_some(), "should resolve class::Point::fn::move");
    }

    #[test]
    fn resolve_dart_getter() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "class::Point::getter::distanceFromOrigin",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve class::Point::getter::distanceFromOrigin"
        );
    }

    #[test]
    fn resolve_dart_setter() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "class::Point::setter::label",
            Language::Dart,
        );
        assert!(span.is_some(), "should resolve class::Point::setter::label");
    }

    #[test]
    fn resolve_dart_constructor() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "class::Point::constructor::Point",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve class::Point::constructor::Point"
        );
    }

    #[test]
    fn resolve_dart_mixin() {
        let span = resolve_tree_path(SAMPLE_DART, "mixin::Serializable", Language::Dart);
        assert!(span.is_some(), "should resolve mixin::Serializable");
    }

    #[test]
    fn resolve_dart_mixin_method() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "mixin::Serializable::fn::serialize",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve mixin::Serializable::fn::serialize"
        );
    }

    #[test]
    fn resolve_dart_extension() {
        let span = resolve_tree_path(SAMPLE_DART, "extension::StringExt", Language::Dart);
        assert!(span.is_some(), "should resolve extension::StringExt");
    }

    #[test]
    fn resolve_dart_extension_method() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "extension::StringExt::fn::reverse",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve extension::StringExt::fn::reverse"
        );
    }

    #[test]
    fn resolve_dart_extension_getter() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "extension::StringExt::getter::isBlank",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve extension::StringExt::getter::isBlank"
        );
    }

    #[test]
    fn resolve_dart_extension_type() {
        let span = resolve_tree_path(SAMPLE_DART, "extension_type::Meters", Language::Dart);
        assert!(span.is_some(), "should resolve extension_type::Meters");
    }

    #[test]
    fn resolve_dart_enum() {
        let span = resolve_tree_path(SAMPLE_DART, "enum::Color", Language::Dart);
        assert!(span.is_some(), "should resolve enum::Color");
    }

    #[test]
    fn resolve_dart_enum_method() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "enum::Color::getter::label",
            Language::Dart,
        );
        assert!(
            span.is_some(),
            "should resolve enum::Color::getter::label"
        );
    }

    #[test]
    fn resolve_dart_top_level_function() {
        let span = resolve_tree_path(SAMPLE_DART, "fn::main", Language::Dart);
        assert!(span.is_some(), "should resolve fn::main");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_DART.lines().collect();
        assert!(
            lines[start - 1].contains("void main()"),
            "span should point to main function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_dart_top_level_getter() {
        let span = resolve_tree_path(SAMPLE_DART, "getter::globalCount", Language::Dart);
        assert!(span.is_some(), "should resolve getter::globalCount");
    }

    #[test]
    fn roundtrip_dart_class() {
        let span = resolve_tree_path(SAMPLE_DART, "class::Point", Language::Dart).unwrap();
        let path = compute_tree_path(SAMPLE_DART, span, Language::Dart);
        assert_eq!(path, "class::Point");

        let re_resolved = resolve_tree_path(SAMPLE_DART, &path, Language::Dart).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_dart_method() {
        let span =
            resolve_tree_path(SAMPLE_DART, "class::Point::fn::move", Language::Dart).unwrap();
        let path = compute_tree_path(SAMPLE_DART, span, Language::Dart);
        assert_eq!(path, "class::Point::fn::move");

        let re_resolved = resolve_tree_path(SAMPLE_DART, &path, Language::Dart).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_dart_top_level_fn() {
        let span = resolve_tree_path(SAMPLE_DART, "fn::main", Language::Dart).unwrap();
        let path = compute_tree_path(SAMPLE_DART, span, Language::Dart);
        assert_eq!(path, "fn::main");

        let re_resolved = resolve_tree_path(SAMPLE_DART, &path, Language::Dart).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn roundtrip_dart_extension_method() {
        let span = resolve_tree_path(
            SAMPLE_DART,
            "extension::StringExt::fn::reverse",
            Language::Dart,
        )
        .unwrap();
        let path = compute_tree_path(SAMPLE_DART, span, Language::Dart);
        assert_eq!(path, "extension::StringExt::fn::reverse");

        let re_resolved = resolve_tree_path(SAMPLE_DART, &path, Language::Dart).unwrap();
        assert_eq!(re_resolved, span);
    }

    #[test]
    fn detect_dart_extension() {
        assert_eq!(
            detect_language(Path::new("main.dart")),
            Some(Language::Dart)
        );
        assert_eq!(
            detect_language(Path::new("lib/src/widget.dart")),
            Some(Language::Dart)
        );
    }
}
