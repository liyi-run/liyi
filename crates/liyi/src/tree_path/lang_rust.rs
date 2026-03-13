use super::LanguageConfig;

use tree_sitter::Node;

/// Detect Rust doc comments (`///`, `//!`, `/** */`).
fn rust_has_doc_comment(node: &Node, source: &str) -> bool {
    // Check previous siblings for doc comments
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("///") || text.starts_with("//!") {
                    return true;
                }
                // Regular comment, keep looking
                sibling = s.prev_sibling();
            }
            "block_comment" => {
                let text = &source[s.byte_range()];
                if text.starts_with("/**") {
                    return true;
                }
                sibling = s.prev_sibling();
            }
            "attribute_item" | "attribute" => {
                // Attributes like #[derive(...)] may precede doc comments
                sibling = s.prev_sibling();
            }
            _ => break,
        }
    }
    false
}

/// Rust language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_rust::LANGUAGE.into(),
    extensions: &["rs"],
    kind_map: &[
        ("fn", "function_item"),
        ("struct", "struct_item"),
        ("enum", "enum_item"),
        ("impl", "impl_item"),
        ("trait", "trait_item"),
        ("mod", "mod_item"),
        ("const", "const_item"),
        ("static", "static_item"),
        ("type", "type_item"),
        ("macro", "macro_definition"),
    ],
    name_field: "name",
    name_overrides: &[("impl_item", "type")],
    body_fields: &["body"],
    custom_name: None,
    doc_comment_detector: Some(rust_has_doc_comment),
};
