use super::LanguageConfig;

/// Bash language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_bash::LANGUAGE.into(),
    extensions: &["sh", "bash"],
    kind_map: &[("fn", "function_definition")],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: None,
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_BASH: &str = r#"#!/bin/bash

# Helper function
function helper() {
    echo "helping"
}

# Main function with alternate syntax
main_func() {
    echo "main"
}

# Function with no parens style (some shells)
another_func {
    echo "another"
}
"#;

    #[test]
    fn resolve_bash_function_with_function_keyword() {
        let span = resolve_tree_path(SAMPLE_BASH, "fn::helper", Language::Bash);
        assert!(span.is_some(), "should resolve fn::helper");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_BASH.lines().collect();
        assert!(
            lines[start - 1].contains("function helper"),
            "span should point to helper function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_bash_function_with_parens_syntax() {
        let span = resolve_tree_path(SAMPLE_BASH, "fn::main_func", Language::Bash);
        assert!(span.is_some(), "should resolve fn::main_func");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_BASH.lines().collect();
        assert!(
            lines[start - 1].contains("main_func()"),
            "span should point to main_func function, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn compute_bash_function_path() {
        // Use resolve to get the span, then verify compute produces the same path
        let resolved_span = resolve_tree_path(SAMPLE_BASH, "fn::helper", Language::Bash).unwrap();
        let path = compute_tree_path(SAMPLE_BASH, resolved_span, Language::Bash);
        assert_eq!(path, "fn::helper");
    }

    #[test]
    fn roundtrip_bash() {
        let resolved_span = resolve_tree_path(SAMPLE_BASH, "fn::helper", Language::Bash).unwrap();

        let computed_path = compute_tree_path(SAMPLE_BASH, resolved_span, Language::Bash);
        assert_eq!(computed_path, "fn::helper");

        let re_resolved = resolve_tree_path(SAMPLE_BASH, &computed_path, Language::Bash).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn detect_bash_extensions() {
        assert_eq!(
            detect_language(Path::new("script.sh")),
            Some(Language::Bash)
        );
        assert_eq!(
            detect_language(Path::new("script.bash")),
            Some(Language::Bash)
        );
    }
}
