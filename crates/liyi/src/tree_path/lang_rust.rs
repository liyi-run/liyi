use super::LanguageConfig;

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
};
