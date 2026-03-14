use tree_sitter::Node;

/// Custom name extraction for Go nodes.
///
/// Handles three Go-specific patterns:
/// - `method_declaration`: encodes receiver type into the name, producing
///   `ReceiverType.MethodName` or `(*ReceiverType).MethodName`.
/// - `type_declaration`: navigates to the inner `type_spec` for the name.
/// - `const_declaration` / `var_declaration`: navigates to the inner spec.
fn go_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "method_declaration" => {
            let method_name_node = node.child_by_field_name("name")?;
            let method_name = &source[method_name_node.byte_range()];

            let receiver = node.child_by_field_name("receiver")?;
            let mut cursor = receiver.walk();
            let param = receiver
                .children(&mut cursor)
                .find(|c| c.kind() == "parameter_declaration")?;

            let type_node = param.child_by_field_name("type")?;
            let receiver_type = if type_node.kind() == "pointer_type" {
                let mut cursor2 = type_node.walk();
                let inner = type_node
                    .children(&mut cursor2)
                    .find(|c| c.kind() == "type_identifier")?;
                format!("(*{})", &source[inner.byte_range()])
            } else {
                source[type_node.byte_range()].to_string()
            };

            Some(format!("{receiver_type}.{method_name}"))
        }
        "type_declaration" => {
            let mut cursor = node.walk();
            let type_spec = node
                .children(&mut cursor)
                .find(|c| c.kind() == "type_spec")?;
            let name_node = type_spec.child_by_field_name("name")?;
            Some(source[name_node.byte_range()].to_string())
        }
        "const_declaration" => {
            let mut cursor = node.walk();
            let spec = node
                .children(&mut cursor)
                .find(|c| c.kind() == "const_spec")?;
            let name_node = spec.child_by_field_name("name")?;
            Some(source[name_node.byte_range()].to_string())
        }
        "var_declaration" => {
            let mut cursor = node.walk();
            let spec = node
                .children(&mut cursor)
                .find(|c| c.kind() == "var_spec")?;
            let name_node = spec.child_by_field_name("name")?;
            Some(source[name_node.byte_range()].to_string())
        }
        _ => None,
    }
}

/// Detect Go doc comments (`// Comment` before a declaration).
fn go_has_doc_comment(node: &Node, source: &str) -> bool {
    let _ = source;
    if let Some(s) = node.prev_sibling()
        && s.kind() == "comment"
    {
        return true;
    }
    false
}

// Go language configuration.
declare_language! {
    /// Go language configuration.
    pub(super) static CONFIG {
        ts_language: || tree_sitter_go::LANGUAGE.into(),
        extensions: ["go"],
        kind_map: [
            ("fn", "function_declaration"),
            ("method", "method_declaration"),
            ("type", "type_declaration"),
            ("const", "const_declaration"),
            ("var", "var_declaration"),
        ],
        name_field: "name",
        name_overrides: [],
        body_fields: ["body"],
        custom_name: Some(go_node_name),
        doc_comment_detector: Some(go_has_doc_comment),
        transparent_kinds: [],
    }
}

#[cfg(test)]
mod tests {
    use crate::tree_path::*;

    const SAMPLE_GO: &str = r#"package main

import "fmt"

// Calculator performs arithmetic operations
type Calculator struct {
    value int
}

// Reader is an interface
type Reader interface {
    Read(p []byte) (n int, err error)
}

// MaxRetries is a constant
const MaxRetries = 3

// DefaultTimeout is a var
var DefaultTimeout = 30

// Add adds a number to the calculator's value
func (c *Calculator) Add(n int) {
    c.value += n
}

// Value returns the current value
func (c Calculator) Value() int {
    return c.value
}

// Add is a standalone function
func Add(a, b int) int {
    return a + b
}
"#;

    #[test]
    fn resolve_go_function() {
        let span = resolve_tree_path(SAMPLE_GO, "fn.Add", Language::Go);
        assert!(span.is_some(), "should resolve fn.Add");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("func Add("),
            "span should point to Add function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_pointer_method() {
        let span = resolve_tree_path(SAMPLE_GO, "method.\"(*Calculator).Add\"", Language::Go);
        assert!(
            span.is_some(),
            "should resolve method.\"(*Calculator).Add\""
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("func (c *Calculator) Add"),
            "span should point to Add method, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_value_method() {
        let span = resolve_tree_path(SAMPLE_GO, "method.\"Calculator.Value\"", Language::Go);
        assert!(span.is_some(), "should resolve method.\"Calculator.Value\"");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("func (c Calculator) Value"),
            "span should point to Value method, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_type_struct() {
        let span = resolve_tree_path(SAMPLE_GO, "type.Calculator", Language::Go);
        assert!(span.is_some(), "should resolve type.Calculator");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("type Calculator struct"),
            "span should point to Calculator struct, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_type_interface() {
        let span = resolve_tree_path(SAMPLE_GO, "type.Reader", Language::Go);
        assert!(span.is_some(), "should resolve type.Reader");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("type Reader interface"),
            "span should point to Reader interface, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_const() {
        let span = resolve_tree_path(SAMPLE_GO, "const.MaxRetries", Language::Go);
        assert!(span.is_some(), "should resolve const.MaxRetries");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("const MaxRetries"),
            "span should point to MaxRetries const, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_go_var() {
        let span = resolve_tree_path(SAMPLE_GO, "var.DefaultTimeout", Language::Go);
        assert!(span.is_some(), "should resolve var.DefaultTimeout");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        assert!(
            lines[start - 1].contains("var DefaultTimeout"),
            "span should point to DefaultTimeout var, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn compute_go_function_path() {
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        let start = lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| l.contains("func Add("))
            .unwrap()
            .0
            + 1;
        let end = lines.len();

        let path = compute_tree_path(SAMPLE_GO, [start, end], Language::Go);
        assert_eq!(path, "fn.Add");
    }

    #[test]
    fn compute_go_pointer_method_path() {
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("func (c *Calculator) Add"))
            .unwrap()
            + 1;
        let end = lines
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, l)| l.starts_with('}'))
            .map(|(i, _)| i + 1)
            .unwrap_or(lines.len());

        let path = compute_tree_path(SAMPLE_GO, [start, end], Language::Go);
        assert_eq!(path, "method.\"(*Calculator).Add\"");
    }

    #[test]
    fn compute_go_value_method_path() {
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("func (c Calculator) Value"))
            .unwrap()
            + 1;
        let end = lines
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, l)| l.starts_with('}'))
            .map(|(i, _)| i + 1)
            .unwrap_or(lines.len());

        let path = compute_tree_path(SAMPLE_GO, [start, end], Language::Go);
        assert_eq!(path, "method.\"Calculator.Value\"");
    }

    #[test]
    fn compute_go_type_path() {
        let lines: Vec<&str> = SAMPLE_GO.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("type Calculator struct"))
            .unwrap()
            + 1;
        let end = lines
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, l)| l.starts_with('}'))
            .map(|(i, _)| i + 1)
            .unwrap_or(lines.len());

        let path = compute_tree_path(SAMPLE_GO, [start, end], Language::Go);
        assert_eq!(path, "type.Calculator");
    }

    #[test]
    fn roundtrip_go() {
        let resolved_span = resolve_tree_path(SAMPLE_GO, "fn.Add", Language::Go).unwrap();

        let computed_path = compute_tree_path(SAMPLE_GO, resolved_span, Language::Go);
        assert_eq!(computed_path, "fn.Add");

        let re_resolved = resolve_tree_path(SAMPLE_GO, &computed_path, Language::Go).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn roundtrip_go_method() {
        let resolved_span =
            resolve_tree_path(SAMPLE_GO, "method.\"(*Calculator).Add\"", Language::Go).unwrap();

        let computed_path = compute_tree_path(SAMPLE_GO, resolved_span, Language::Go);
        assert_eq!(computed_path, "method.\"(*Calculator).Add\"");

        let re_resolved = resolve_tree_path(SAMPLE_GO, &computed_path, Language::Go).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }
}
