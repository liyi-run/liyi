//! Tree-sitter structural identity for span recovery.
//!
//! `tree_path` provides format-invariant item identity by encoding an item's
//! position in the AST as a `::` delimited path of (kind, name) segments.
//! For example, `fn::add_money` or `impl::Money::fn::new`.
//!
//! When `tree_path` is populated and a tree-sitter grammar is available for
//! the source language, `liyi reanchor` and `liyi check --fix` use it to
//! locate items by structural identity, making span recovery deterministic
//! across formatting changes, import additions, and line reflows.

use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Language as TSLanguage, Node, Parser};

/// Language-specific configuration for tree_path resolution.
///
/// Each supported language provides a static `LanguageConfig` that defines
/// how to parse it and map between tree-sitter node kinds and tree_path
/// shorthands.
pub struct LanguageConfig {
    /// Function to get the tree-sitter language grammar (lazy initialization).
    ts_language: fn() -> TSLanguage,
    /// File extensions associated with this language.
    extensions: &'static [&'static str],
    /// Map from tree_path kind shorthand to tree-sitter node kind.
    kind_map: &'static [(&'static str, &'static str)],
    /// Field name to extract the node's name (usually "name").
    name_field: &'static str,
    /// Overrides for special cases: (node_kind, field_name) pairs.
    name_overrides: &'static [(&'static str, &'static str)],
    /// Field names to traverse to find a node's body/declaration_list.
    body_fields: &'static [&'static str],
    /// Custom name extraction for node kinds that need special handling
    /// (e.g., Go methods with receiver types, Go type_declaration wrapping type_spec).
    /// Returns `Some(name)` for handled kinds, `None` to fall through to default.
    custom_name: Option<fn(&Node, &str) -> Option<String>>,
}

impl LanguageConfig {
    /// Map tree-sitter node kind → tree_path shorthand.
    fn kind_to_shorthand(&self, ts_kind: &str) -> Option<&'static str> {
        self.kind_map
            .iter()
            .find(|(_, ts)| *ts == ts_kind)
            .map(|(short, _)| *short)
    }

    /// Map tree_path shorthand → tree-sitter node kind.
    fn shorthand_to_kind(&self, short: &str) -> Option<&'static str> {
        self.kind_map
            .iter()
            .find(|(s, _)| *s == short)
            .map(|(_, ts)| *ts)
    }

    /// Extract the name of a named AST node.
    ///
    /// Returns a `Cow<str>` — borrowed from `source` in the common case,
    /// owned when the name is constructed (e.g., Go method receiver encoding).
    fn node_name<'a>(&self, node: &Node<'a>, source: &'a str) -> Option<Cow<'a, str>> {
        // Check custom_name callback first (e.g., Go method receivers)
        if let Some(custom) = self.custom_name {
            if let Some(name) = custom(node, source) {
                return Some(Cow::Owned(name));
            }
        }

        let kind = node.kind();

        // Check for name field override (e.g., impl_item uses "type" field)
        let field_name = self
            .name_overrides
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, f)| *f)
            .unwrap_or(self.name_field);

        let name_node = node.child_by_field_name(field_name)?;
        Some(Cow::Borrowed(&source[name_node.byte_range()]))
    }

    /// Find a body/declaration_list child for descending into containers.
    fn find_body<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        for field in self.body_fields {
            if let Some(body) = node.child_by_field_name(field) {
                return Some(body);
            }
        }
        // Fallback: search for body_fields or declaration_list as direct
        // (unnamed) children. Needed for languages where the body is a
        // positional child rather than a named field (e.g., Kotlin class_body,
        // C++ field_declaration_list).
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|c| {
                self.body_fields.contains(&c.kind())
                    || c.kind() == "declaration_list"
                    || c.kind() == "field_declaration_list"
            })
    }

    /// Check if the given file extension is associated with this language.
    pub fn matches_extension(&self, ext: &str) -> bool {
        self.extensions.contains(&ext)
    }
}

/// Rust language configuration.
static RUST_CONFIG: LanguageConfig = LanguageConfig {
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

/// Python language configuration.
static PYTHON_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_python::LANGUAGE.into(),
    extensions: &["py", "pyi"],
    kind_map: &[("fn", "function_definition"), ("class", "class_definition")],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// Extract the function name from a C/C++ `function_definition` node.
///
/// C/C++ functions store their name inside the `declarator` field chain:
/// `function_definition` → (field `declarator`) `function_declarator`
/// → (field `declarator`) `identifier` / `field_identifier`.
/// Pointer declarators and other wrappers may appear in the chain;
/// we unwrap them until we find a `function_declarator`.
fn c_extract_declarator_name(node: &Node, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    let func_decl = unwrap_to_function_declarator(&declarator)?;
    let name_node = func_decl.child_by_field_name("declarator")?;
    Some(source[name_node.byte_range()].to_string())
}

/// Walk through pointer_declarator / parenthesized_declarator / attributed_declarator
/// wrappers to find the inner `function_declarator`.
fn unwrap_to_function_declarator<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    match node.kind() {
        "function_declarator" => Some(*node),
        "pointer_declarator" | "parenthesized_declarator" | "attributed_declarator" => {
            let inner = node.child_by_field_name("declarator")?;
            unwrap_to_function_declarator(&inner)
        }
        _ => None,
    }
}

/// Custom name extraction for C nodes.
///
/// Handles `function_definition` (name in declarator chain) and
/// `type_definition` (name in declarator field, which is a type_identifier).
fn c_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" => {
            // typedef: the 'declarator' field holds the new type name
            let declarator = node.child_by_field_name("declarator")?;
            Some(source[declarator.byte_range()].to_string())
        }
        _ => None,
    }
}

/// Custom name extraction for C++ nodes.
///
/// Extends `c_node_name` with C++-specific patterns:
/// - `template_declaration`: transparent wrapper — extracts name from inner decl.
/// - `namespace_definition`: name is in a `namespace_identifier` child (no "name" field).
fn cpp_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" | "alias_declaration" => {
            let name_node = node.child_by_field_name("name")
                .or_else(|| node.child_by_field_name("declarator"))?;
            Some(source[name_node.byte_range()].to_string())
        }
        "template_declaration" => {
            // template_declaration wraps an inner declaration — find it and
            // extract the name from the inner node.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_definition" => return c_extract_declarator_name(&child, source),
                    "class_specifier" | "struct_specifier" | "enum_specifier"
                    | "concept_definition" | "alias_declaration" => {
                        let n = child.child_by_field_name("name")?;
                        return Some(source[n.byte_range()].to_string());
                    }
                    // A template can also wrap another template_declaration (nested)
                    "template_declaration" => return cpp_node_name(&child, source),
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

/// Custom name extraction for Objective-C nodes.
///
/// ObjC node types like `class_interface`, `class_implementation`,
/// `protocol_declaration`, `method_declaration`, and `method_definition`
/// do not use standard `name` fields. Their names are extracted from
/// specific child node patterns.
fn objc_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        // C function definitions use the same declarator chain as C.
        "function_definition" => c_extract_declarator_name(node, source),
        "type_definition" => {
            let declarator = node.child_by_field_name("declarator")?;
            Some(source[declarator.byte_range()].to_string())
        }
        // @interface ClassName or @interface ClassName (Category)
        "class_interface" | "class_implementation" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        // @protocol ProtocolName
        "protocol_declaration" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        // - (ReturnType)methodName or - (ReturnType)methodName:(Type)arg
        // + (ReturnType)classMethodName
        "method_declaration" | "method_definition" => {
            let mut cursor = node.walk();
            // The selector is composed of keyword_declarator children or
            // a single identifier (for zero-argument methods).
            let mut parts: Vec<String> = Vec::new();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "identifier" | "field_identifier" if parts.is_empty() => {
                        // Single-part selector (no arguments)
                        parts.push(source[child.byte_range()].to_string());
                    }
                    "keyword_declarator" => {
                        // Each keyword_declarator has a keyword child
                        let mut kw_cursor = child.walk();
                        if let Some(kw) = child.children(&mut kw_cursor)
                            .find(|c| c.kind() == "keyword_selector" || c.kind() == "identifier")
                        {
                            parts.push(format!("{}:", &source[kw.byte_range()]));
                        }
                    }
                    _ => {}
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(""))
            }
        }
        _ => None,
    }
}

/// Custom name extraction for Kotlin nodes.
///
/// Handles `property_declaration` where the name is in a child
/// `variable_declaration` node, and `type_alias` where the name is
/// in an `identifier` child before the `=` (the `type` field is the RHS).
fn kotlin_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "property_declaration" => {
            let mut cursor = node.walk();
            // Name is in the first variable_declaration or identifier child
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declaration" {
                    let name = child.child_by_field_name("name")
                        .or_else(|| {
                            let mut c2 = child.walk();
                            child.children(&mut c2).find(|c| c.kind() == "simple_identifier")
                        })?;
                    return Some(source[name.byte_range()].to_string());
                }
                if child.kind() == "simple_identifier" {
                    return Some(source[child.byte_range()].to_string());
                }
            }
            None
        }
        "type_alias" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier" || c.kind() == "simple_identifier")
                .map(|c| source[c.byte_range()].to_string())
        }
        _ => None,
    }
}

/// Custom name extraction for PHP `const_declaration` nodes.
///
/// PHP `const_declaration` stores names inside `const_element` children.
fn php_node_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "const_declaration" => {
            let mut cursor = node.walk();
            let elem = node.children(&mut cursor)
                .find(|c| c.kind() == "const_element")?;
            let name = elem.child_by_field_name("name")?;
            Some(source[name.byte_range()].to_string())
        }
        _ => None,
    }
}

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

/// Go language configuration.
static GO_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_go::LANGUAGE.into(),
    extensions: &["go"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("method", "method_declaration"),
        ("type", "type_declaration"),
        ("const", "const_declaration"),
        ("var", "var_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(go_node_name),
};

/// JavaScript language configuration.
static JAVASCRIPT_CONFIG: LanguageConfig = LanguageConfig {
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
};

/// TypeScript language configuration.
static TYPESCRIPT_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    extensions: &["ts", "mts", "cts"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("method", "method_definition"),
        ("interface", "interface_declaration"),
        ("type", "type_alias_declaration"),
        ("enum", "enum_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// TSX language configuration.
static TSX_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
    extensions: &["tsx"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("method", "method_definition"),
        ("interface", "interface_declaration"),
        ("type", "type_alias_declaration"),
        ("enum", "enum_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// C language configuration.
static C_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_c::LANGUAGE.into(),
    extensions: &["c", "h"],
    kind_map: &[
        ("fn", "function_definition"),
        ("struct", "struct_specifier"),
        ("enum", "enum_specifier"),
        ("typedef", "type_definition"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(c_node_name),
};

/// C++ language configuration.
static CPP_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_cpp::LANGUAGE.into(),
    extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx", "h++", "c++"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_specifier"),
        ("struct", "struct_specifier"),
        ("namespace", "namespace_definition"),
        ("enum", "enum_specifier"),
        ("template", "template_declaration"),
        ("typedef", "type_definition"),
        ("using", "alias_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body", "declaration_list"],
    custom_name: Some(cpp_node_name),
};

/// Java language configuration.
static JAVA_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_java::LANGUAGE.into(),
    extensions: &["java"],
    kind_map: &[
        ("fn", "method_declaration"),
        ("class", "class_declaration"),
        ("interface", "interface_declaration"),
        ("enum", "enum_declaration"),
        ("constructor", "constructor_declaration"),
        ("record", "record_declaration"),
        ("annotation", "annotation_type_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// C# language configuration.
static CSHARP_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_c_sharp::LANGUAGE.into(),
    extensions: &["cs"],
    kind_map: &[
        ("fn", "method_declaration"),
        ("class", "class_declaration"),
        ("interface", "interface_declaration"),
        ("enum", "enum_declaration"),
        ("struct", "struct_declaration"),
        ("namespace", "namespace_declaration"),
        ("constructor", "constructor_declaration"),
        ("property", "property_declaration"),
        ("record", "record_declaration"),
        ("delegate", "delegate_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// PHP language configuration (PHP-only grammar, no HTML interleaving).
static PHP_CONFIG: LanguageConfig = LanguageConfig {
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
};

/// Objective-C language configuration.
static OBJC_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_objc::LANGUAGE.into(),
    extensions: &["m", "mm"],
    kind_map: &[
        ("fn", "function_definition"),
        ("class", "class_interface"),
        ("impl", "class_implementation"),
        ("protocol", "protocol_declaration"),
        ("method", "method_definition"),
        ("method_decl", "method_declaration"),
        ("struct", "struct_specifier"),
        ("enum", "enum_specifier"),
        ("typedef", "type_definition"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: Some(objc_node_name),
};

/// Kotlin language configuration.
static KOTLIN_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_kotlin_ng::LANGUAGE.into(),
    extensions: &["kt", "kts"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("object", "object_declaration"),
        ("property", "property_declaration"),
        ("typealias", "type_alias"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body", "class_body"],
    custom_name: Some(kotlin_node_name),
};

/// Swift language configuration.
static SWIFT_CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_swift::LANGUAGE.into(),
    extensions: &["swift"],
    kind_map: &[
        ("fn", "function_declaration"),
        ("class", "class_declaration"),
        ("protocol", "protocol_declaration"),
        ("enum", "enum_entry"),
        ("property", "property_declaration"),
        ("init", "init_declaration"),
        ("typealias", "typealias_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

/// Supported languages for tree_path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    Go,
    JavaScript,
    TypeScript,
    Tsx,
    C,
    Cpp,
    Java,
    CSharp,
    Php,
    ObjectiveC,
    Kotlin,
    Swift,
}

impl Language {
    /// Get the language configuration for this language.
    fn config(&self) -> &'static LanguageConfig {
        match self {
            Language::Rust => &RUST_CONFIG,
            Language::Python => &PYTHON_CONFIG,
            Language::Go => &GO_CONFIG,
            Language::JavaScript => &JAVASCRIPT_CONFIG,
            Language::TypeScript => &TYPESCRIPT_CONFIG,
            Language::Tsx => &TSX_CONFIG,
            Language::C => &C_CONFIG,
            Language::Cpp => &CPP_CONFIG,
            Language::Java => &JAVA_CONFIG,
            Language::CSharp => &CSHARP_CONFIG,
            Language::Php => &PHP_CONFIG,
            Language::ObjectiveC => &OBJC_CONFIG,
            Language::Kotlin => &KOTLIN_CONFIG,
            Language::Swift => &SWIFT_CONFIG,
        }
    }

    /// Get the tree-sitter language grammar.
    fn ts_language(&self) -> TSLanguage {
        (self.config().ts_language)()
    }
}

/// Detect language from file extension. Returns `None` for unsupported
/// languages (unknown extension).
///
/// # Extension Collision
///
/// `.h` files are ambiguous (C, C++, or Objective-C). We map them to C
/// by default. Users can override via future configuration if needed.
///
/// If two languages share an extension (unlikely with built-in languages),
/// the first match in the following order is returned:
/// Rust → Python → Go → JavaScript → TypeScript → TSX → C → C++ →
/// Java → C# → PHP → Objective-C → Kotlin → Swift.
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;

    if RUST_CONFIG.matches_extension(ext) {
        return Some(Language::Rust);
    }

    if PYTHON_CONFIG.matches_extension(ext) {
        return Some(Language::Python);
    }

    if GO_CONFIG.matches_extension(ext) {
        return Some(Language::Go);
    }

    if JAVASCRIPT_CONFIG.matches_extension(ext) {
        return Some(Language::JavaScript);
    }

    if TYPESCRIPT_CONFIG.matches_extension(ext) {
        return Some(Language::TypeScript);
    }
    if TSX_CONFIG.matches_extension(ext) {
        return Some(Language::Tsx);
    }

    if C_CONFIG.matches_extension(ext) {
        return Some(Language::C);
    }
    if CPP_CONFIG.matches_extension(ext) {
        return Some(Language::Cpp);
    }
    if JAVA_CONFIG.matches_extension(ext) {
        return Some(Language::Java);
    }
    if CSHARP_CONFIG.matches_extension(ext) {
        return Some(Language::CSharp);
    }
    if PHP_CONFIG.matches_extension(ext) {
        return Some(Language::Php);
    }
    if OBJC_CONFIG.matches_extension(ext) {
        return Some(Language::ObjectiveC);
    }
    if KOTLIN_CONFIG.matches_extension(ext) {
        return Some(Language::Kotlin);
    }
    if SWIFT_CONFIG.matches_extension(ext) {
        return Some(Language::Swift);
    }

    None
}

/// Create a tree-sitter parser for the given language.
fn make_parser(lang: Language) -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&lang.ts_language())
        .expect("tree-sitter grammar should load");
    parser
}

/// A parsed tree_path segment: (kind_shorthand, name).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathSegment {
    kind: String,
    name: String,
}

/// Parse a tree_path string into segments.
///
/// `"fn::add_money"` → `[PathSegment { kind: "fn", name: "add_money" }]`
/// `"impl::Money::fn::new"` → `[impl/Money, fn/new]`
fn parse_tree_path(tree_path: &str) -> Option<Vec<PathSegment>> {
    let parts: Vec<&str> = tree_path.split("::").collect();
    if !parts.len().is_multiple_of(2) {
        return None; // must be pairs
    }
    let segments: Vec<PathSegment> = parts
        .chunks(2)
        .map(|pair| PathSegment {
            kind: pair[0].to_string(),
            name: pair[1].to_string(),
        })
        .collect();
    if segments.is_empty() {
        return None;
    }
    Some(segments)
}

/// Resolve a `tree_path` to a source span `[start_line, end_line]` (1-indexed,
/// inclusive).
///
/// Returns `None` if the tree_path cannot be resolved (item renamed, deleted,
/// grammar unavailable, or language not supported).
pub fn resolve_tree_path(source: &str, tree_path: &str, lang: Language) -> Option<[usize; 2]> {
    if tree_path.is_empty() {
        return None;
    }

    let config = lang.config();
    let segments = parse_tree_path(tree_path)?;
    let mut parser = make_parser(lang);
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let node = resolve_segments(config, &root, &segments, source)?;

    // Return 1-indexed inclusive line range
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    Some([start_line, end_line])
}

/// Walk the tree to find a node matching the given path segments.
fn resolve_segments<'a>(
    config: &LanguageConfig,
    parent: &Node<'a>,
    segments: &[PathSegment],
    source: &'a str,
) -> Option<Node<'a>> {
    if segments.is_empty() {
        return Some(*parent);
    }

    let seg = &segments[0];
    let ts_kind = config.shorthand_to_kind(&seg.kind)?;

    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() != ts_kind {
            continue;
        }
        if let Some(name) = config.node_name(&child, source) {
            if *name == seg.name && segments.len() == 1 {
                return Some(child);
            } else if *name == seg.name {
                // Descend — look inside this node's body
                return resolve_in_body(config, &child, &segments[1..], source);
            }
        }
    }

    None
}

/// Find subsequent segments inside an item's body (e.g., methods inside impl).
fn resolve_in_body<'a>(
    config: &LanguageConfig,
    node: &Node<'a>,
    segments: &[PathSegment],
    source: &'a str,
) -> Option<Node<'a>> {
    let body = config.find_body(node)?;
    resolve_segments(config, &body, segments, source)
}

/// Compute the canonical `tree_path` for the AST node at the given span.
///
/// Returns an empty string if no suitable structural path can be determined
/// (e.g., the span doesn't align with a named item, or the language is
/// unsupported).
pub fn compute_tree_path(source: &str, span: [usize; 2], lang: Language) -> String {
    let config = lang.config();
    let mut parser = make_parser(lang);
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return String::new(),
    };

    let root = tree.root_node();
    // Convert 1-indexed inclusive span to 0-indexed row
    let target_start = span[0].saturating_sub(1);
    let target_end = span[1].saturating_sub(1);

    // Find the best item node within the target range
    let node = match find_item_in_range(config, &root, target_start, target_end) {
        Some(n) => n,
        None => return String::new(),
    };

    // Build path from root to this node
    build_path_to_node(config, &root, &node, source)
}

/// Find the best item node within [target_start, target_end] (0-indexed rows).
///
/// Attributes in Rust are sibling nodes, not children of the item, so a
/// sidecar span that includes `#[derive(...)]` lines will start before the
/// item node.  We therefore match any item whose start/end rows fall within
/// the target range, preferring the widest match (the outermost item).
fn find_item_in_range<'a>(
    config: &LanguageConfig,
    root: &Node<'a>,
    target_start: usize,
    target_end: usize,
) -> Option<Node<'a>> {
    let mut best: Option<Node<'a>> = None;

    fn walk<'a>(
        config: &LanguageConfig,
        node: &Node<'a>,
        target_start: usize,
        target_end: usize,
        best: &mut Option<Node<'a>>,
    ) {
        let start = node.start_position().row;
        let end = node.end_position().row;

        // Skip nodes that don't overlap our target
        if start > target_end || end < target_start {
            return;
        }

        // Check if this is a named item node within the target range
        if start >= target_start && end <= target_end && is_item_node(config, node) {
            // Prefer the widest (outermost) match
            if let Some(b) = best {
                let b_size = b.end_position().row - b.start_position().row;
                let n_size = end - start;
                if n_size >= b_size {
                    *best = Some(*node);
                }
            } else {
                *best = Some(*node);
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(config, &child, target_start, target_end, best);
        }
    }

    walk(config, root, target_start, target_end, &mut best);
    best
}

/// Check if a node is an item type we track in tree_path.
fn is_item_node(config: &LanguageConfig, node: &Node) -> bool {
    config.kind_to_shorthand(node.kind()).is_some()
}

/// Build the tree_path string for a given target node by walking from root.
fn build_path_to_node(config: &LanguageConfig, root: &Node, target: &Node, source: &str) -> String {
    let mut segments: Vec<String> = Vec::new();
    if collect_path(config, root, target, source, &mut segments) {
        segments.join("::")
    } else {
        String::new()
    }
}

/// Recursively find `target` in the tree and collect path segments.
fn collect_path(
    config: &LanguageConfig,
    node: &Node,
    target: &Node,
    source: &str,
    segments: &mut Vec<String>,
) -> bool {
    if node.id() == target.id() {
        // We found the target — add this node's segment if it's an item
        if let (Some(short), Some(name)) = (
            config.kind_to_shorthand(node.kind()),
            config.node_name(node, source),
        ) {
            segments.push(format!("{short}::{name}"));
            return true;
        }
        return false;
    }

    // Check children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_start = child.start_position().row;
        let child_end = child.end_position().row;
        let target_start = target.start_position().row;
        let target_end = target.end_position().row;

        // Only descend into nodes that contain the target
        if child_start <= target_start
            && child_end >= target_end
            && collect_path(config, &child, target, source, segments)
        {
            // If this node is an item node, prepend its segment
            if is_item_node(config, node)
                && let (Some(short), Some(name)) = (
                    config.kind_to_shorthand(node.kind()),
                    config.node_name(node, source),
                )
            {
                segments.insert(0, format!("{short}::{name}"));
            }
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RUST: &str = r#"use std::collections::HashMap;

/// A monetary amount
pub struct Money {
    amount: i64,
    currency: String,
}

impl Money {
    pub fn new(amount: i64, currency: String) -> Self {
        Self { amount, currency }
    }

    pub fn add(&self, other: &Money) -> Result<Money, &'static str> {
        if self.currency != other.currency {
            return Err("mismatched currencies");
        }
        Ok(Money {
            amount: self.amount + other.amount,
            currency: self.currency.clone(),
        })
    }
}

mod billing {
    pub fn charge(amount: i64) -> bool {
        amount > 0
    }
}

fn standalone() -> i32 {
    42
}
"#;

    #[test]
    fn resolve_top_level_fn() {
        let span = resolve_tree_path(SAMPLE_RUST, "fn::standalone", Language::Rust);
        assert!(span.is_some(), "should resolve fn::standalone");
        let [start, end] = span.unwrap();
        assert!(start > 0);
        assert!(end >= start);
        // Verify the span contains the function
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("fn standalone"),
            "span start should point to fn standalone, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_struct() {
        let span = resolve_tree_path(SAMPLE_RUST, "struct::Money", Language::Rust);
        assert!(span.is_some(), "should resolve struct::Money");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("struct Money"),
            "span start should point to struct Money, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_impl_method() {
        let span = resolve_tree_path(SAMPLE_RUST, "impl::Money::fn::new", Language::Rust);
        assert!(span.is_some(), "should resolve impl::Money::fn::new");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("fn new"),
            "span start should point to fn new, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_impl_method_add() {
        let span = resolve_tree_path(SAMPLE_RUST, "impl::Money::fn::add", Language::Rust);
        assert!(span.is_some(), "should resolve impl::Money::fn::add");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("fn add"),
            "span start should point to fn add, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_mod_fn() {
        let span = resolve_tree_path(SAMPLE_RUST, "mod::billing::fn::charge", Language::Rust);
        assert!(span.is_some(), "should resolve mod::billing::fn::charge");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("fn charge"),
            "span start should point to fn charge, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_impl_block() {
        let span = resolve_tree_path(SAMPLE_RUST, "impl::Money", Language::Rust);
        assert!(span.is_some(), "should resolve impl::Money");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        assert!(
            lines[start - 1].contains("impl Money"),
            "span start should point to impl Money, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_nonexistent_returns_none() {
        let span = resolve_tree_path(SAMPLE_RUST, "fn::nonexistent", Language::Rust);
        assert!(span.is_none());
    }

    #[test]
    fn resolve_empty_returns_none() {
        let span = resolve_tree_path(SAMPLE_RUST, "", Language::Rust);
        assert!(span.is_none());
    }

    #[test]
    fn compute_fn_path() {
        // Find standalone function line
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("fn standalone"))
            .unwrap()
            + 1;
        let end = lines
            .iter()
            .enumerate()
            .skip(start - 1)
            .find(|(_, l)| l.contains('}'))
            .unwrap()
            .0
            + 1;

        let path = compute_tree_path(SAMPLE_RUST, [start, end], Language::Rust);
        assert_eq!(path, "fn::standalone");
    }

    #[test]
    fn compute_impl_method_path() {
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        let start = lines.iter().position(|l| l.contains("pub fn new")).unwrap() + 1;
        // fn new spans from its line to the closing }
        let mut brace_depth = 0i32;
        let mut end = start;
        for (i, line) in lines.iter().enumerate().skip(start - 1) {
            for ch in line.chars() {
                if ch == '{' {
                    brace_depth += 1;
                }
                if ch == '}' {
                    brace_depth -= 1;
                }
            }
            if brace_depth == 0 {
                end = i + 1;
                break;
            }
        }

        let path = compute_tree_path(SAMPLE_RUST, [start, end], Language::Rust);
        assert_eq!(path, "impl::Money::fn::new");
    }

    #[test]
    fn compute_struct_path() {
        let lines: Vec<&str> = SAMPLE_RUST.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains("pub struct Money"))
            .unwrap()
            + 1;
        let end = lines
            .iter()
            .enumerate()
            .skip(start - 1)
            .find(|(_, l)| l.trim() == "}")
            .unwrap()
            .0
            + 1;

        let path = compute_tree_path(SAMPLE_RUST, [start, end], Language::Rust);
        assert_eq!(path, "struct::Money");
    }

    #[test]
    fn roundtrip_resolve_compute() {
        // Compute path for fn::standalone, then resolve it — spans should match
        // Use tree-sitter to find exact span
        let resolved_span =
            resolve_tree_path(SAMPLE_RUST, "fn::standalone", Language::Rust).unwrap();

        let computed_path = compute_tree_path(SAMPLE_RUST, resolved_span, Language::Rust);
        assert_eq!(computed_path, "fn::standalone");

        let re_resolved = resolve_tree_path(SAMPLE_RUST, &computed_path, Language::Rust).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn detect_language_rust() {
        assert_eq!(
            detect_language(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(detect_language(Path::new("foo.py")), Some(Language::Python));
    }

    #[test]
    fn resilient_to_formatting() {
        // Same code reformatted differently — tree_path should still resolve
        let reformatted = r#"use std::collections::HashMap;

/// A monetary amount
pub struct Money { amount: i64, currency: String }

impl Money {
    pub fn new(amount: i64, currency: String) -> Self { Self { amount, currency } }

    pub fn add(&self, other: &Money) -> Result<Money, &'static str> {
        if self.currency != other.currency { return Err("mismatched currencies"); }
        Ok(Money { amount: self.amount + other.amount, currency: self.currency.clone() })
    }
}

mod billing {
    pub fn charge(amount: i64) -> bool { amount > 0 }
}

fn standalone() -> i32 { 42 }
"#;

        // All tree_paths from the original should resolve in the reformatted version
        for tp in &[
            "fn::standalone",
            "struct::Money",
            "impl::Money",
            "impl::Money::fn::new",
            "impl::Money::fn::add",
            "mod::billing::fn::charge",
        ] {
            let span = resolve_tree_path(reformatted, tp, Language::Rust);
            assert!(span.is_some(), "should resolve {tp} in reformatted code");
        }
    }

    mod python_tests {
        use super::*;

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
            let span = resolve_tree_path(SAMPLE_PYTHON, "fn::calculate_total", Language::Python);
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
            let span = resolve_tree_path(SAMPLE_PYTHON, "class::Order", Language::Python);
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
            let span =
                resolve_tree_path(SAMPLE_PYTHON, "class::Order::fn::process", Language::Python);
            assert!(span.is_some(), "should resolve class::Order::fn::process");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_PYTHON.lines().collect();
            assert!(
                lines[start - 1].contains("def process"),
                "span should point to process method"
            );
        }

        #[test]
        fn resolve_python_init_method() {
            let span = resolve_tree_path(
                SAMPLE_PYTHON,
                "class::Order::fn::__init__",
                Language::Python,
            );
            assert!(span.is_some(), "should resolve class::Order::fn::__init__");
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
            assert_eq!(path, "fn::calculate_total");
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
            assert_eq!(path, "class::Order::fn::process");
        }

        #[test]
        fn roundtrip_python() {
            // Compute path for fn::calculate_total, then resolve it
            let resolved_span =
                resolve_tree_path(SAMPLE_PYTHON, "fn::calculate_total", Language::Python).unwrap();

            let computed_path = compute_tree_path(SAMPLE_PYTHON, resolved_span, Language::Python);
            assert_eq!(computed_path, "fn::calculate_total");

            let re_resolved =
                resolve_tree_path(SAMPLE_PYTHON, &computed_path, Language::Python).unwrap();
            assert_eq!(re_resolved, resolved_span);
        }
    }

    mod go_tests {
        use super::*;

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
            let span = resolve_tree_path(SAMPLE_GO, "fn::Add", Language::Go);
            assert!(span.is_some(), "should resolve fn::Add");
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
            let span =
                resolve_tree_path(SAMPLE_GO, "method::(*Calculator).Add", Language::Go);
            assert!(span.is_some(), "should resolve method::(*Calculator).Add");
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
            let span =
                resolve_tree_path(SAMPLE_GO, "method::Calculator.Value", Language::Go);
            assert!(span.is_some(), "should resolve method::Calculator.Value");
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
            let span = resolve_tree_path(SAMPLE_GO, "type::Calculator", Language::Go);
            assert!(span.is_some(), "should resolve type::Calculator");
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
            let span = resolve_tree_path(SAMPLE_GO, "type::Reader", Language::Go);
            assert!(span.is_some(), "should resolve type::Reader");
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
            let span = resolve_tree_path(SAMPLE_GO, "const::MaxRetries", Language::Go);
            assert!(span.is_some(), "should resolve const::MaxRetries");
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
            let span = resolve_tree_path(SAMPLE_GO, "var::DefaultTimeout", Language::Go);
            assert!(span.is_some(), "should resolve var::DefaultTimeout");
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
            assert_eq!(path, "fn::Add");
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
            assert_eq!(path, "method::(*Calculator).Add");
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
            assert_eq!(path, "method::Calculator.Value");
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
            assert_eq!(path, "type::Calculator");
        }

        #[test]
        fn roundtrip_go() {
            let resolved_span = resolve_tree_path(SAMPLE_GO, "fn::Add", Language::Go).unwrap();

            let computed_path = compute_tree_path(SAMPLE_GO, resolved_span, Language::Go);
            assert_eq!(computed_path, "fn::Add");

            let re_resolved = resolve_tree_path(SAMPLE_GO, &computed_path, Language::Go).unwrap();
            assert_eq!(re_resolved, resolved_span);
        }

        #[test]
        fn roundtrip_go_method() {
            let resolved_span =
                resolve_tree_path(SAMPLE_GO, "method::(*Calculator).Add", Language::Go).unwrap();

            let computed_path = compute_tree_path(SAMPLE_GO, resolved_span, Language::Go);
            assert_eq!(computed_path, "method::(*Calculator).Add");

            let re_resolved = resolve_tree_path(SAMPLE_GO, &computed_path, Language::Go).unwrap();
            assert_eq!(re_resolved, resolved_span);
        }
    }

    mod javascript_tests {
        use super::*;

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

    mod typescript_tests {
        use super::*;

        const SAMPLE_TS: &str = r#"// A typed user service

interface User {
    id: number;
    name: string;
}

type UserId = number;

enum UserRole {
    Admin,
    User,
    Guest
}

class UserService {
    private users: User[] = [];

    addUser(user: User): void {
        this.users.push(user);
    }

    findById(id: UserId): User | undefined {
        return this.users.find(u => u.id === id);
    }
}

function createUser(name: string): User {
    return { id: Date.now(), name };
}
"#;

        #[test]
        fn resolve_ts_interface() {
            let span = resolve_tree_path(SAMPLE_TS, "interface::User", Language::TypeScript);
            assert!(span.is_some(), "should resolve interface::User");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TS.lines().collect();
            assert!(
                lines[start - 1].contains("interface User"),
                "span should point to User interface"
            );
        }

        #[test]
        fn resolve_ts_type_alias() {
            let span = resolve_tree_path(SAMPLE_TS, "type::UserId", Language::TypeScript);
            assert!(span.is_some(), "should resolve type::UserId");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TS.lines().collect();
            assert!(
                lines[start - 1].contains("type UserId"),
                "span should point to UserId type alias"
            );
        }

        #[test]
        fn resolve_ts_enum() {
            let span = resolve_tree_path(SAMPLE_TS, "enum::UserRole", Language::TypeScript);
            assert!(span.is_some(), "should resolve enum::UserRole");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TS.lines().collect();
            assert!(
                lines[start - 1].contains("enum UserRole"),
                "span should point to UserRole enum"
            );
        }

        #[test]
        fn resolve_ts_class_method() {
            let span = resolve_tree_path(
                SAMPLE_TS,
                "class::UserService::method::findById",
                Language::TypeScript,
            );
            assert!(
                span.is_some(),
                "should resolve class::UserService::method::findById"
            );
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TS.lines().collect();
            assert!(
                lines[start - 1].contains("findById("),
                "span should point to findById method"
            );
        }

        #[test]
        fn compute_ts_interface_path() {
            let lines: Vec<&str> = SAMPLE_TS.lines().collect();
            let start = lines
                .iter()
                .position(|l| l.contains("interface User"))
                .unwrap()
                + 1;
            let end = start + 3;

            let path = compute_tree_path(SAMPLE_TS, [start, end], Language::TypeScript);
            assert_eq!(path, "interface::User");
        }

        #[test]
        fn roundtrip_ts() {
            let resolved_span =
                resolve_tree_path(SAMPLE_TS, "enum::UserRole", Language::TypeScript).unwrap();

            let computed_path = compute_tree_path(SAMPLE_TS, resolved_span, Language::TypeScript);
            assert_eq!(computed_path, "enum::UserRole");

            let re_resolved =
                resolve_tree_path(SAMPLE_TS, &computed_path, Language::TypeScript).unwrap();
            assert_eq!(re_resolved, resolved_span);
        }
    }

    mod tsx_tests {
        use super::*;

        const SAMPLE_TSX: &str = r#"// A React component

interface Props {
    title: string;
    count: number;
}

function Counter({ title, count }: Props) {
    return (
        <div>
            <h1>{title}</h1>
            <p>Count: {count}</p>
        </div>
    );
}

class Container extends React.Component<Props> {
    render() {
        return <div>{this.props.title}</div>;
    }
}
"#;

        #[test]
        fn resolve_tsx_function() {
            let span = resolve_tree_path(SAMPLE_TSX, "fn::Counter", Language::Tsx);
            assert!(span.is_some(), "should resolve fn::Counter in TSX");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TSX.lines().collect();
            assert!(
                lines[start - 1].contains("function Counter"),
                "span should point to Counter function"
            );
        }

        #[test]
        fn resolve_tsx_class() {
            let span = resolve_tree_path(SAMPLE_TSX, "class::Container", Language::Tsx);
            assert!(span.is_some(), "should resolve class::Container in TSX");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TSX.lines().collect();
            assert!(
                lines[start - 1].contains("class Container"),
                "span should point to Container class"
            );
        }

        #[test]
        fn resolve_tsx_interface() {
            let span = resolve_tree_path(SAMPLE_TSX, "interface::Props", Language::Tsx);
            assert!(span.is_some(), "should resolve interface::Props in TSX");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_TSX.lines().collect();
            assert!(
                lines[start - 1].contains("interface Props"),
                "span should point to Props interface"
            );
        }

        #[test]
        fn detect_tsx_extension() {
            assert_eq!(
                detect_language(Path::new("component.tsx")),
                Some(Language::Tsx)
            );
        }
    }

    mod c_tests {
        use super::*;

        const SAMPLE_C: &str = r#"#include <stdio.h>

struct Point {
    int x;
    int y;
};

enum Color { RED, GREEN, BLUE };

typedef struct Point Point_t;

void process(int x, int y) {
    printf("hello");
}

static int helper(void) {
    return 42;
}
"#;

        #[test]
        fn resolve_c_function() {
            let span = resolve_tree_path(SAMPLE_C, "fn::process", Language::C);
            assert!(span.is_some(), "should resolve fn::process");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_C.lines().collect();
            assert!(
                lines[start - 1].contains("void process"),
                "span should point to process function, got: {}",
                lines[start - 1]
            );
        }

        #[test]
        fn resolve_c_struct() {
            let span = resolve_tree_path(SAMPLE_C, "struct::Point", Language::C);
            assert!(span.is_some(), "should resolve struct::Point");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_C.lines().collect();
            assert!(
                lines[start - 1].contains("struct Point"),
                "span should point to Point struct"
            );
        }

        #[test]
        fn resolve_c_enum() {
            let span = resolve_tree_path(SAMPLE_C, "enum::Color", Language::C);
            assert!(span.is_some(), "should resolve enum::Color");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_C.lines().collect();
            assert!(
                lines[start - 1].contains("enum Color"),
                "span should point to Color enum"
            );
        }

        #[test]
        fn resolve_c_typedef() {
            let span = resolve_tree_path(SAMPLE_C, "typedef::Point_t", Language::C);
            assert!(span.is_some(), "should resolve typedef::Point_t");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_C.lines().collect();
            assert!(
                lines[start - 1].contains("typedef"),
                "span should point to typedef"
            );
        }

        #[test]
        fn compute_c_function_path() {
            let span = resolve_tree_path(SAMPLE_C, "fn::process", Language::C).unwrap();
            let path = compute_tree_path(SAMPLE_C, span, Language::C);
            assert_eq!(path, "fn::process");
        }

        #[test]
        fn roundtrip_c() {
            for tp in &["fn::process", "fn::helper", "struct::Point", "enum::Color"] {
                let span = resolve_tree_path(SAMPLE_C, tp, Language::C).unwrap();
                let path = compute_tree_path(SAMPLE_C, span, Language::C);
                assert_eq!(&path, tp, "roundtrip failed for {tp}");
            }
        }

        #[test]
        fn detect_c_extensions() {
            assert_eq!(detect_language(Path::new("main.c")), Some(Language::C));
            assert_eq!(detect_language(Path::new("header.h")), Some(Language::C));
        }
    }

    mod cpp_tests {
        use super::*;

        const SAMPLE_CPP: &str = r#"namespace math {

class Calculator {
public:
    int add(int a, int b) {
        return a + b;
    }
};

struct Point {
    int x, y;
};

enum class Color { Red, Green, Blue };

}

void standalone() {}
"#;

        #[test]
        fn resolve_cpp_namespace() {
            let span = resolve_tree_path(SAMPLE_CPP, "namespace::math", Language::Cpp);
            assert!(span.is_some(), "should resolve namespace::math");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_CPP.lines().collect();
            assert!(
                lines[start - 1].contains("namespace math"),
                "span should point to namespace math, got: {}",
                lines[start - 1]
            );
        }

        #[test]
        fn resolve_cpp_class_in_namespace() {
            let span = resolve_tree_path(
                SAMPLE_CPP,
                "namespace::math::class::Calculator",
                Language::Cpp,
            );
            assert!(span.is_some(), "should resolve namespace::math::class::Calculator");
        }

        #[test]
        fn resolve_cpp_method_in_class() {
            let span = resolve_tree_path(
                SAMPLE_CPP,
                "namespace::math::class::Calculator::fn::add",
                Language::Cpp,
            );
            assert!(span.is_some(), "should resolve nested method");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_CPP.lines().collect();
            assert!(
                lines[start - 1].contains("add"),
                "span should point to add method"
            );
        }

        #[test]
        fn resolve_cpp_standalone() {
            let span = resolve_tree_path(SAMPLE_CPP, "fn::standalone", Language::Cpp);
            assert!(span.is_some(), "should resolve fn::standalone");
        }

        #[test]
        fn resolve_cpp_enum() {
            let span = resolve_tree_path(
                SAMPLE_CPP,
                "namespace::math::enum::Color",
                Language::Cpp,
            );
            assert!(span.is_some(), "should resolve enum in namespace");
        }

        #[test]
        fn roundtrip_cpp() {
            let span = resolve_tree_path(SAMPLE_CPP, "fn::standalone", Language::Cpp).unwrap();
            let path = compute_tree_path(SAMPLE_CPP, span, Language::Cpp);
            assert_eq!(path, "fn::standalone");
        }

        #[test]
        fn detect_cpp_extensions() {
            assert_eq!(detect_language(Path::new("main.cpp")), Some(Language::Cpp));
            assert_eq!(detect_language(Path::new("main.cc")), Some(Language::Cpp));
            assert_eq!(detect_language(Path::new("header.hpp")), Some(Language::Cpp));
        }
    }

    mod java_tests {
        use super::*;

        const SAMPLE_JAVA: &str = r#"package com.example;

public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public Calculator() {
        // constructor
    }
}

interface Computable {
    int compute(int x);
}

enum Direction {
    NORTH, SOUTH, EAST, WEST
}

record Point(int x, int y) {}
"#;

        #[test]
        fn resolve_java_class() {
            let span = resolve_tree_path(SAMPLE_JAVA, "class::Calculator", Language::Java);
            assert!(span.is_some(), "should resolve class::Calculator");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_JAVA.lines().collect();
            assert!(
                lines[start - 1].contains("class Calculator"),
                "span should point to Calculator class"
            );
        }

        #[test]
        fn resolve_java_method() {
            let span = resolve_tree_path(
                SAMPLE_JAVA,
                "class::Calculator::fn::add",
                Language::Java,
            );
            assert!(span.is_some(), "should resolve class::Calculator::fn::add");
        }

        #[test]
        fn resolve_java_constructor() {
            let span = resolve_tree_path(
                SAMPLE_JAVA,
                "class::Calculator::constructor::Calculator",
                Language::Java,
            );
            assert!(span.is_some(), "should resolve constructor");
        }

        #[test]
        fn resolve_java_interface() {
            let span = resolve_tree_path(SAMPLE_JAVA, "interface::Computable", Language::Java);
            assert!(span.is_some(), "should resolve interface::Computable");
        }

        #[test]
        fn resolve_java_enum() {
            let span = resolve_tree_path(SAMPLE_JAVA, "enum::Direction", Language::Java);
            assert!(span.is_some(), "should resolve enum::Direction");
        }

        #[test]
        fn resolve_java_record() {
            let span = resolve_tree_path(SAMPLE_JAVA, "record::Point", Language::Java);
            assert!(span.is_some(), "should resolve record::Point");
        }

        #[test]
        fn roundtrip_java() {
            let span = resolve_tree_path(
                SAMPLE_JAVA,
                "class::Calculator::fn::add",
                Language::Java,
            )
            .unwrap();
            let path = compute_tree_path(SAMPLE_JAVA, span, Language::Java);
            assert_eq!(path, "class::Calculator::fn::add");
        }

        #[test]
        fn detect_java_extension() {
            assert_eq!(
                detect_language(Path::new("Main.java")),
                Some(Language::Java)
            );
        }
    }

    mod csharp_tests {
        use super::*;

        const SAMPLE_CSHARP: &str = r#"namespace MyApp {

class Calculator {
    public int Add(int a, int b) {
        return a + b;
    }

    public string Name { get; set; }

    public Calculator() {}
}

interface IComputable {
    int Compute(int x);
}

enum Direction {
    North, South, East, West
}

struct Vector {
    public int X;
    public int Y;
}

record Person(string Name, int Age);

}
"#;

        #[test]
        fn resolve_csharp_class() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::class::Calculator",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve namespace::MyApp::class::Calculator");
        }

        #[test]
        fn resolve_csharp_method() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::class::Calculator::fn::Add",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve method in class in namespace");
        }

        #[test]
        fn resolve_csharp_property() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::class::Calculator::property::Name",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve property::Name");
        }

        #[test]
        fn resolve_csharp_interface() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::interface::IComputable",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve interface::IComputable");
        }

        #[test]
        fn resolve_csharp_struct() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::struct::Vector",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve struct::Vector");
        }

        #[test]
        fn resolve_csharp_enum() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::enum::Direction",
                Language::CSharp,
            );
            assert!(span.is_some(), "should resolve enum::Direction");
        }

        #[test]
        fn roundtrip_csharp() {
            let span = resolve_tree_path(
                SAMPLE_CSHARP,
                "namespace::MyApp::class::Calculator::fn::Add",
                Language::CSharp,
            )
            .unwrap();
            let path = compute_tree_path(SAMPLE_CSHARP, span, Language::CSharp);
            assert_eq!(path, "namespace::MyApp::class::Calculator::fn::Add");
        }

        #[test]
        fn detect_csharp_extension() {
            assert_eq!(
                detect_language(Path::new("Program.cs")),
                Some(Language::CSharp)
            );
        }
    }

    mod php_tests {
        use super::*;

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
            let span = resolve_tree_path(SAMPLE_PHP, "class::UserService", Language::Php);
            assert!(span.is_some(), "should resolve class::UserService");
        }

        #[test]
        fn resolve_php_method() {
            let span = resolve_tree_path(
                SAMPLE_PHP,
                "class::UserService::method::findUser",
                Language::Php,
            );
            assert!(span.is_some(), "should resolve class::UserService::method::findUser");
        }

        #[test]
        fn resolve_php_interface() {
            let span = resolve_tree_path(SAMPLE_PHP, "interface::Repository", Language::Php);
            assert!(span.is_some(), "should resolve interface::Repository");
        }

        #[test]
        fn resolve_php_trait() {
            let span = resolve_tree_path(SAMPLE_PHP, "trait::Cacheable", Language::Php);
            assert!(span.is_some(), "should resolve trait::Cacheable");
        }

        #[test]
        fn resolve_php_function() {
            let span = resolve_tree_path(SAMPLE_PHP, "fn::helper", Language::Php);
            assert!(span.is_some(), "should resolve fn::helper");
        }

        #[test]
        fn resolve_php_enum() {
            let span = resolve_tree_path(SAMPLE_PHP, "enum::Status", Language::Php);
            assert!(span.is_some(), "should resolve enum::Status");
        }

        #[test]
        fn roundtrip_php() {
            let span = resolve_tree_path(SAMPLE_PHP, "fn::helper", Language::Php).unwrap();
            let path = compute_tree_path(SAMPLE_PHP, span, Language::Php);
            assert_eq!(path, "fn::helper");
        }

        #[test]
        fn detect_php_extension() {
            assert_eq!(
                detect_language(Path::new("UserService.php")),
                Some(Language::Php)
            );
        }
    }

    mod kotlin_tests {
        use super::*;

        const SAMPLE_KOTLIN: &str = r#"class Calculator {
    fun add(a: Int, b: Int): Int {
        return a + b
    }
}

object Singleton {
    fun instance(): Singleton = this
}

fun standalone(): Int {
    return 42
}

typealias StringList = List<String>
"#;

        #[test]
        fn resolve_kotlin_class() {
            let span = resolve_tree_path(SAMPLE_KOTLIN, "class::Calculator", Language::Kotlin);
            assert!(span.is_some(), "should resolve class::Calculator");
        }

        #[test]
        fn resolve_kotlin_method() {
            let span = resolve_tree_path(
                SAMPLE_KOTLIN,
                "class::Calculator::fn::add",
                Language::Kotlin,
            );
            assert!(span.is_some(), "should resolve class::Calculator::fn::add");
        }

        #[test]
        fn resolve_kotlin_object() {
            let span = resolve_tree_path(SAMPLE_KOTLIN, "object::Singleton", Language::Kotlin);
            assert!(span.is_some(), "should resolve object::Singleton");
        }

        #[test]
        fn resolve_kotlin_function() {
            let span = resolve_tree_path(SAMPLE_KOTLIN, "fn::standalone", Language::Kotlin);
            assert!(span.is_some(), "should resolve fn::standalone");
        }

        #[test]
        fn roundtrip_kotlin() {
            let span =
                resolve_tree_path(SAMPLE_KOTLIN, "fn::standalone", Language::Kotlin).unwrap();
            let path = compute_tree_path(SAMPLE_KOTLIN, span, Language::Kotlin);
            assert_eq!(path, "fn::standalone");
        }

        #[test]
        fn detect_kotlin_extension() {
            assert_eq!(
                detect_language(Path::new("Main.kt")),
                Some(Language::Kotlin)
            );
            assert_eq!(
                detect_language(Path::new("build.gradle.kts")),
                Some(Language::Kotlin)
            );
        }
    }

    mod swift_tests {
        use super::*;

        const SAMPLE_SWIFT: &str = r#"protocol Drawable {
    func draw()
}

class Shape {
    func area() -> Double {
        return 0.0
    }

    init() {}
}

func standalone() -> Int {
    return 42
}

typealias Callback = () -> Void
"#;

        #[test]
        fn resolve_swift_protocol() {
            let span = resolve_tree_path(SAMPLE_SWIFT, "protocol::Drawable", Language::Swift);
            assert!(span.is_some(), "should resolve protocol::Drawable");
        }

        #[test]
        fn resolve_swift_class() {
            let span = resolve_tree_path(SAMPLE_SWIFT, "class::Shape", Language::Swift);
            assert!(span.is_some(), "should resolve class::Shape");
        }

        #[test]
        fn resolve_swift_method() {
            let span = resolve_tree_path(
                SAMPLE_SWIFT,
                "class::Shape::fn::area",
                Language::Swift,
            );
            assert!(span.is_some(), "should resolve class::Shape::fn::area");
        }

        #[test]
        fn resolve_swift_function() {
            let span = resolve_tree_path(SAMPLE_SWIFT, "fn::standalone", Language::Swift);
            assert!(span.is_some(), "should resolve fn::standalone");
        }

        #[test]
        fn roundtrip_swift() {
            let span =
                resolve_tree_path(SAMPLE_SWIFT, "fn::standalone", Language::Swift).unwrap();
            let path = compute_tree_path(SAMPLE_SWIFT, span, Language::Swift);
            assert_eq!(path, "fn::standalone");
        }

        #[test]
        fn detect_swift_extension() {
            assert_eq!(
                detect_language(Path::new("ViewController.swift")),
                Some(Language::Swift)
            );
        }
    }

    mod objc_tests {
        use super::*;

        const SAMPLE_OBJC: &str = r#"#import <Foundation/Foundation.h>

struct CGPoint {
    float x;
    float y;
};

void helper(void) {
    NSLog(@"hello");
}
"#;

        #[test]
        fn resolve_objc_function() {
            let span = resolve_tree_path(SAMPLE_OBJC, "fn::helper", Language::ObjectiveC);
            assert!(span.is_some(), "should resolve fn::helper");
            let [start, _end] = span.unwrap();
            let lines: Vec<&str> = SAMPLE_OBJC.lines().collect();
            assert!(
                lines[start - 1].contains("void helper"),
                "span should point to helper function"
            );
        }

        #[test]
        fn resolve_objc_struct() {
            let span = resolve_tree_path(SAMPLE_OBJC, "struct::CGPoint", Language::ObjectiveC);
            assert!(span.is_some(), "should resolve struct::CGPoint");
        }

        #[test]
        fn roundtrip_objc() {
            let span =
                resolve_tree_path(SAMPLE_OBJC, "fn::helper", Language::ObjectiveC).unwrap();
            let path = compute_tree_path(SAMPLE_OBJC, span, Language::ObjectiveC);
            assert_eq!(path, "fn::helper");
        }

        #[test]
        fn detect_objc_extensions() {
            assert_eq!(
                detect_language(Path::new("AppDelegate.m")),
                Some(Language::ObjectiveC)
            );
            assert_eq!(
                detect_language(Path::new("mixed.mm")),
                Some(Language::ObjectiveC)
            );
        }
    }
}
