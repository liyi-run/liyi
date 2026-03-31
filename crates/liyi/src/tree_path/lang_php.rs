use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for PHP `const_declaration` nodes.
///
/// PHP `const_declaration` stores names inside `const_element` children.
fn php_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "const_declaration" => {
            let mut cursor = node.walk();
            let elem = node
                .children(&mut cursor)
                .find(|c| c.kind() == "const_element")?;
            let name = elem.child_by_field_name("name")?;
            Some(source[name.byte_range()].to_string())
        }
        _ => None,
    }
}

/// PHP language configuration (PHP-only grammar, no HTML interleaving).
/// Detect PHPDoc comments (`/** ... */`).
///
/// PHP's tree-sitter grammar uses a uniform `comment` kind. The PHP
/// convention for documentation is `/**` (PHPDoc). `attribute_list`
/// and `modifier` siblings are skipped.
fn php_has_doc_comment(node: &Node, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("/**") {
                    return true;
                }
                sibling = s.prev_sibling();
            }
            "attribute_list" | "modifier" | "visibility_modifier" => {
                sibling = s.prev_sibling();
            }
            _ => break,
        }
    }
    false
}
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_php::LANGUAGE_PHP_ONLY.into(),
    extensions: &["php"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_declaration"),
        ("method", "method_declaration"),
        ("interface", "interface_declaration"),
        ("enum", "enum_declaration"),
        ("trait", "trait_declaration"),
        ("namespace", "namespace_definition"),
        ("const", "const_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(php_node_name),
    doc_comment_detector: Some(php_has_doc_comment),
    transparent_kinds: &[],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_PHP: &str = r#"<?php

namespace App\Services;

class UserService {
    public function findUser(int $id): ?User {
        return User::find($id);
    }

    public function deleteUser(int $id): bool {
        return true;
    }
}

interface Repository {
    public function find(int $id);
}

trait Cacheable {
    public function cache(): void {}
}

function helper(): string {
    return "hi";
}

enum Status {
    case Active;
    case Inactive;
}
"#;

    #[test]
    fn resolve_php_class() {
        let span = resolve_tree_path(SAMPLE_PHP, "class.UserService", Language::Php);
        assert!(span.is_some(), "should resolve class.UserService");
    }

    #[test]
    fn resolve_php_method() {
        let span = resolve_tree_path(
            SAMPLE_PHP,
            "class.UserService::method.findUser",
            Language::Php,
        );
        assert!(
            span.is_some(),
            "should resolve class.UserService::method.findUser"
        );
    }

    #[test]
    fn resolve_php_interface() {
        let span = resolve_tree_path(SAMPLE_PHP, "interface.Repository", Language::Php);
        assert!(span.is_some(), "should resolve interface.Repository");
    }

    #[test]
    fn resolve_php_trait() {
        let span = resolve_tree_path(SAMPLE_PHP, "trait.Cacheable", Language::Php);
        assert!(span.is_some(), "should resolve trait.Cacheable");
    }

    #[test]
    fn resolve_php_function() {
        let span = resolve_tree_path(SAMPLE_PHP, "fn.helper", Language::Php);
        assert!(span.is_some(), "should resolve fn.helper");
    }

    #[test]
    fn resolve_php_enum() {
        let span = resolve_tree_path(SAMPLE_PHP, "enum.Status", Language::Php);
        assert!(span.is_some(), "should resolve enum.Status");
    }

    #[test]
    fn roundtrip_php() {
        let span = resolve_tree_path(SAMPLE_PHP, "fn.helper", Language::Php).unwrap();
        let path = compute_tree_path(SAMPLE_PHP, span, Language::Php);
        assert_eq!(path, "fn.helper");
    }

    #[test]
    fn detect_php_extension() {
        assert_eq!(
            detect_language(Path::new("UserService.php")),
            Some(Language::Php)
        );
    }
}
