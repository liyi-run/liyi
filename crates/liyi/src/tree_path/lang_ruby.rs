use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for Ruby nodes.
///
/// Handles `singleton_method` (class methods like `def self.foo`) which encodes
/// the class name in the path: `singleton_method::"ClassName.method_name"`.
fn ruby_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "singleton_method" => {
            let method_name_node = node.child_by_field_name("name")?;
            let method_name = &source[method_name_node.byte_range()];

            // The object field holds the receiver (e.g., `self` or class name)
            // For `def self.foo`, object is `self`
            // For `def ClassName.foo`, object is the class name identifier
            let object = node.child_by_field_name("object")?;
            let receiver = if object.kind() == "self" {
                "self".to_string()
            } else {
                source[object.byte_range()].to_string()
            };

            Some(format!("{receiver}.{method_name}"))
        }
        _ => None,
    }
}

/// Ruby language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_ruby::LANGUAGE.into(),
    extensions: &["rb", "rake", "gemspec"],
    kind_map: &[
        ("fn", "method"),
        ("class", "class"),
        ("module", "module"),
        ("singleton_method", "singleton_method"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body", "statements"],
    custom_name: Some(ruby_node_name),
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_RUBY: &str = r#"# A billing module
module Billing
  class Invoice
    def total
      @items.sum
    end

    def self.calculate_tax(amount)
      amount * 0.1
    end
  end

  def standalone_helper
    "helper"
  end
end

class Order
  def process
    "processing"
  end
end
"#;

    #[test]
    fn resolve_ruby_module() {
        let span = resolve_tree_path(SAMPLE_RUBY, "module::Billing", Language::Ruby);
        assert!(span.is_some(), "should resolve module::Billing");
    }

    #[test]
    fn resolve_ruby_class_in_module() {
        let span = resolve_tree_path(
            SAMPLE_RUBY,
            "module::Billing::class::Invoice",
            Language::Ruby,
        );
        assert!(
            span.is_some(),
            "should resolve module::Billing::class::Invoice"
        );
    }

    #[test]
    fn resolve_ruby_method_in_class() {
        let span = resolve_tree_path(
            SAMPLE_RUBY,
            "module::Billing::class::Invoice::fn::total",
            Language::Ruby,
        );
        assert!(span.is_some(), "should resolve nested method");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUBY.lines().collect();
        assert!(
            lines[start - 1].contains("def total"),
            "span should point to total method"
        );
    }

    #[test]
    fn resolve_ruby_singleton_method() {
        let span = resolve_tree_path(
            SAMPLE_RUBY,
            "module::Billing::class::Invoice::singleton_method::\"self.calculate_tax\"",
            Language::Ruby,
        );
        assert!(span.is_some(), "should resolve singleton method");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUBY.lines().collect();
        assert!(
            lines[start - 1].contains("def self.calculate_tax"),
            "span should point to class method"
        );
    }

    #[test]
    fn resolve_ruby_module_function() {
        // standalone_helper is defined directly in the module body
        let span = resolve_tree_path(
            SAMPLE_RUBY,
            "module::Billing::fn::standalone_helper",
            Language::Ruby,
        );
        assert!(span.is_some(), "should resolve module-level function");
    }

    #[test]
    fn resolve_ruby_top_level_class() {
        let span = resolve_tree_path(SAMPLE_RUBY, "class::Order", Language::Ruby);
        assert!(span.is_some(), "should resolve top-level class");
    }

    #[test]
    fn resolve_ruby_method_in_top_level_class() {
        let span = resolve_tree_path(SAMPLE_RUBY, "class::Order::fn::process", Language::Ruby);
        assert!(span.is_some(), "should resolve method in top-level class");
    }

    #[test]
    fn compute_ruby_method_path() {
        let resolved_span = resolve_tree_path(
            SAMPLE_RUBY,
            "module::Billing::class::Invoice::fn::total",
            Language::Ruby,
        )
        .unwrap();
        let path = compute_tree_path(SAMPLE_RUBY, resolved_span, Language::Ruby);
        assert_eq!(path, "module::Billing::class::Invoice::fn::total");
    }

    #[test]
    fn roundtrip_ruby() {
        let resolved_span =
            resolve_tree_path(SAMPLE_RUBY, "class::Order::fn::process", Language::Ruby).unwrap();

        let computed_path = compute_tree_path(SAMPLE_RUBY, resolved_span, Language::Ruby);
        assert_eq!(computed_path, "class::Order::fn::process");

        let re_resolved = resolve_tree_path(SAMPLE_RUBY, &computed_path, Language::Ruby).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn detect_ruby_extensions() {
        assert_eq!(detect_language(Path::new("app.rb")), Some(Language::Ruby));
        assert_eq!(
            detect_language(Path::new("tasks.rake")),
            Some(Language::Ruby)
        );
        assert_eq!(
            detect_language(Path::new("my_gem.gemspec")),
            Some(Language::Ruby)
        );
    }
}
