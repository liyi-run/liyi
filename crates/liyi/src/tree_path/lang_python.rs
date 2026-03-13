use super::LanguageConfig;

use tree_sitter::Node;

/// Detect Python docstrings (`"""..."""`/`'''...'''` as first statement of body).
fn python_has_doc_comment(node: &Node, source: &str) -> bool {
    // Python docstrings are the first expression_statement in the body
    // containing a string literal.
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "expression_statement" {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "string" {
                        let text = &source[inner.byte_range()];
                        if text.starts_with("\"\"\"") || text.starts_with("'''") {
                            return true;
                        }
                    }
                }
                // Only check the first statement
                break;
            }
            // Skip comments/decorators, only check the first non-decorator statement
            if child.kind() != "comment" && child.kind() != "decorator" {
                break;
            }
        }
    }
    false
}

/// Python language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_python::LANGUAGE.into(),
    extensions: &["py", "pyi"],
    kind_map: &[("fn", "function_definition"), ("class", "class_definition")],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: Some(python_has_doc_comment),
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;

    const SAMPLE_PYTHON: &str = r#"# A simple order processing module

class Order:
    def __init__(self, amount):
        self.amount = amount

    def process(self):
        return self.amount > 0

def calculate_total(items):
    return sum(items)
"#;

    #[test]
    fn resolve_python_function() {
        let span = resolve_tree_path(SAMPLE_PYTHON, "fn.calculate_total", Language::Python);
        assert!(span.is_some(), "should resolve fn::calculate_total");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("def calculate_total"),
            "span should point to calculate_total function"
        );
    }

    #[test]
    fn resolve_python_class() {
        let span = resolve_tree_path(SAMPLE_PYTHON, "class.Order", Language::Python);
        assert!(span.is_some(), "should resolve class::Order");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("class Order"),
            "span should point to Order class"
        );
    }

    #[test]
    fn resolve_python_class_method() {
        let span = resolve_tree_path(SAMPLE_PYTHON, "class.Order::fn.process", Language::Python);
        assert!(span.is_some(), "should resolve class::Order::fn.process");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("def process"),
            "span should point to process method"
        );
    }

    #[test]
    fn resolve_python_init_method() {
        let span = resolve_tree_path(SAMPLE_PYTHON, "class.Order::fn.__init__", Language::Python);
        assert!(span.is_some(), "should resolve class::Order::fn.__init__");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("def __init__"),
            "span should point to __init__ method"
        );
    }

    #[test]
    fn compute_python_function_path() {
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("def calculate_total"))
            .unwrap()
            + 1;
        let end = lines.len();

        let path = compute_tree_path(SAMPLE_PYTHON, [start, end], Language::Python);
        assert_eq!(path, "fn.calculate_total");
    }

    #[test]
    fn compute_python_class_method_path() {
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("def process"))
            .unwrap()
            + 1;
        // Find end of method (next line with same or less indentation)
        let end = start + 1; // Single-line body for this test

        let path = compute_tree_path(SAMPLE_PYTHON, [start, end], Language::Python);
        assert_eq!(path, "class.Order::fn.process");
    }

    #[test]
    fn roundtrip_python() {
        // Compute path for fn::calculate_total, then resolve it
        let resolved_span =
            resolve_tree_path(SAMPLE_PYTHON, "fn.calculate_total", Language::Python).unwrap();

        let computed_path = compute_tree_path(SAMPLE_PYTHON, resolved_span, Language::Python);
        assert_eq!(computed_path, "fn.calculate_total");

        let re_resolved =
            resolve_tree_path(SAMPLE_PYTHON, &computed_path, Language::Python).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }
}
