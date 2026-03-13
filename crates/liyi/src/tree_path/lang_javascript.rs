use super::LanguageConfig;

use tree_sitter::Node;

/// Detect JSDoc comments (`/** ... */` before a declaration).
pub(super) fn js_has_doc_comment(node: &Node, source: &str) -> bool {
    let sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = &source[s.byte_range()];
            if text.starts_with("/**") {
                return true;
            }
        }
        break;
    }
    false
}

/// JavaScript language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_javascript::LANGUAGE.into(),
    extensions: &["js", "mjs", "cjs", "jsx"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("method", "method_definition"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: Some(js_has_doc_comment),
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;

    const SAMPLE_JS: &str = r#"// A simple counter module

class Counter {
    constructor(initial = 0) {
        this.count = initial;
    }

    increment() {
        this.count++;
    }

    getValue() {
        return this.count;
    }
}

function createCounter(initial) {
    return new Counter(initial);
}

const utils = {
    formatCount: (n) => `${n} items`
};
"#;

    #[test]
    fn resolve_js_function() {
        let span = resolve_tree_path(SAMPLE_JS, "fn::createCounter", Language::JavaScript);
        assert!(span.is_some(), "should resolve fn::createCounter");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JS.lines().collect();
        assert!(
            lines[start - 1].contains("function createCounter"),
            "span should point to createCounter function"
        );
    }

    #[test]
    fn resolve_js_class() {
        let span = resolve_tree_path(SAMPLE_JS, "class::Counter", Language::JavaScript);
        assert!(span.is_some(), "should resolve class::Counter");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JS.lines().collect();
        assert!(
            lines[start - 1].contains("class Counter"),
            "span should point to Counter class"
        );
    }

    #[test]
    fn resolve_js_method() {
        let span = resolve_tree_path(
            SAMPLE_JS,
            "class::Counter::method::increment",
            Language::JavaScript,
        );
        assert!(
            span.is_some(),
            "should resolve class::Counter::method::increment"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_JS.lines().collect();
        assert!(
            lines[start - 1].contains("increment()"),
            "span should point to increment method"
        );
    }

    #[test]
    fn compute_js_function_path() {
        let lines: Vec<&str> = SAMPLE_JS.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("function createCounter"))
            .unwrap()
            + 1;
        let end = lines.len() - 3; // Rough end

        let path = compute_tree_path(SAMPLE_JS, [start, end], Language::JavaScript);
        assert_eq!(path, "fn::createCounter");
    }

    #[test]
    fn roundtrip_js() {
        let resolved_span = resolve_tree_path(
            SAMPLE_JS,
            "class::Counter::method::getValue",
            Language::JavaScript,
        )
        .unwrap();

        let computed_path = compute_tree_path(SAMPLE_JS, resolved_span, Language::JavaScript);
        assert_eq!(computed_path, "class::Counter::method::getValue");

        let re_resolved =
            resolve_tree_path(SAMPLE_JS, &computed_path, Language::JavaScript).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }
}
