use super::LanguageConfig;

use tree_sitter::Node;

/// Custom name extraction for Rust nodes.
///
/// Handles `impl_item` trait implementations: when an impl has a `trait`
/// field (e.g. `impl Display for Foo`), the name encodes both the trait and
/// the type as `"<trait> for <type>"` so that a trait impl and the inherent
/// `impl Foo` resolve to distinct tree_paths. Inherent impls (no `trait`
/// field) return `None` and fall through to the default `type`-field name
/// (see `name_overrides`), keeping their tree_path as `impl.Foo`.
fn rust_node_name(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "impl_item" {
        return None;
    }
    // Inherent impls have no `trait` field — fall through to the type name.
    let trait_node = node.child_by_field_name("trait")?;
    let type_node = node.child_by_field_name("type")?;
    let trait_name = &source[trait_node.byte_range()];
    let type_name = &source[type_node.byte_range()];
    Some(format!("{trait_name} for {type_name}"))
}

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
    custom_name: Some(rust_node_name),
    doc_comment_detector: Some(rust_has_doc_comment),
    transparent_kinds: &[],
};
