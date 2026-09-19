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

/// Custom name extraction for Python module-level constants.
///
/// A module-level assignment (`NAME = ...` or `NAME: Final = ...`) declares a
/// module constant and maps to the `const` shorthand.  The
/// `expression_statement` wrapper is traversed via `transparent_kinds`.
///
/// Requiring the module scope and a plain-identifier target excludes local
/// bindings (`x = 0`, `x: int = 0`) and attribute assignments (`self.x = 0`)
/// inside function or class bodies, which are not constants.
fn python_node_name(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "assignment" {
        return None;
    }

    // Only assignments directly under the module root qualify.
    let statement = node.parent()?;
    if statement.kind() != "expression_statement" || statement.parent()?.kind() != "module" {
        return None;
    }

    let name = node.child_by_field_name("left")?;
    if name.kind() != "identifier" {
        return None;
    }

    Some(source[name.byte_range()].to_string())
}

/// Python language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_python::LANGUAGE.into(),
    extensions: &["py", "pyi"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_definition"),
        ("const", "assignment"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(python_node_name),
    doc_comment_detector: Some(python_has_doc_comment),
    // Module-level assignments are wrapped in an `expression_statement`, so
    // the resolver must look through it to reach the underlying `assignment`
    // node.
    transparent_kinds: &["expression_statement"],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;

    const SAMPLE_PYTHON: &str = r#"# A simple order processing module

from typing import Final

TOPLEVEL_CONST: Final = [1, 2, 3]


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
        assert!(span.is_some(), "should resolve fn.calculate_total");
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
        assert!(span.is_some(), "should resolve class.Order");
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
        assert!(span.is_some(), "should resolve class.Order::fn.process");
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
        assert!(span.is_some(), "should resolve class.Order::fn.__init__");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("def __init__"),
            "span should point to __init__ method"
        );
    }

    #[test]
    fn resolve_python_toplevel_const() {
        let span = resolve_tree_path(SAMPLE_PYTHON, "const.TOPLEVEL_CONST", Language::Python);
        assert!(span.is_some(), "should resolve const.TOPLEVEL_CONST");
        let [start, end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        assert!(
            lines[start - 1].contains("TOPLEVEL_CONST: Final"),
            "span should point to TOPLEVEL_CONST, got: {}",
            lines[start - 1]
        );
        assert_eq!(start, end, "constant span should be a single line");
    }

    #[test]
    fn compute_python_toplevel_const_path() {
        let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("TOPLEVEL_CONST: Final"))
            .unwrap()
            + 1;

        let path = compute_tree_path(SAMPLE_PYTHON, [start, start], Language::Python);
        assert_eq!(path, "const.TOPLEVEL_CONST");
    }

    #[test]
    fn roundtrip_python_toplevel_const() {
        let resolved_span =
            resolve_tree_path(SAMPLE_PYTHON, "const.TOPLEVEL_CONST", Language::Python).unwrap();

        let computed_path = compute_tree_path(SAMPLE_PYTHON, resolved_span, Language::Python);
        assert_eq!(computed_path, "const.TOPLEVEL_CONST");

        let re_resolved =
            resolve_tree_path(SAMPLE_PYTHON, &computed_path, Language::Python).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    /// Module-level assignments are constants whether annotated or not;
    /// assignments inside class/function bodies are not.
    const SAMPLE_PYTHON_SCOPES: &str = r#"MAX = 10
LIMIT: Final = 20


class Config:
    retries: int = 3

    def load(self):
        local: int = 1
        self.value: int = 2
        return local
"#;

    #[test]
    fn discover_python_only_module_level_consts() {
        let items = discover_items(SAMPLE_PYTHON_SCOPES, Language::Python);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();

        for (name, tree_path) in [
            ("MAX", "const.MAX"),
            ("LIMIT", "const.LIMIT"),
            ("Config", "class.Config"),
            ("Config::load", "class.Config::fn.load"),
        ] {
            let item = items
                .iter()
                .find(|i| i.name == name)
                .unwrap_or_else(|| panic!("{name} should be discovered"));
            assert_eq!(item.tree_path, tree_path);
        }

        for absent in [
            "Config::retries",
            "Config::load::local",
            "Config::load::self.value",
        ] {
            assert!(
                !names.contains(&absent),
                "unexpected item discovered: {absent}"
            );
        }
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
