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

mod lang_bash;
mod lang_c;
mod lang_cpp;
mod lang_csharp;
mod lang_go;
mod lang_java;
mod lang_javascript;
mod lang_kotlin;
mod lang_objc;
mod lang_php;
mod lang_python;
mod lang_rust;
mod lang_swift;
mod lang_typescript;

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
        if let Some(custom) = self.custom_name
            && let Some(name) = custom(node, source)
        {
            return Some(Cow::Owned(name));
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
        node.children(&mut cursor).find(|c| {
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

/// Supported languages for tree_path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Bash,
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
            Language::Bash => &lang_bash::CONFIG,
            Language::Rust => &lang_rust::CONFIG,
            Language::Python => &lang_python::CONFIG,
            Language::Go => &lang_go::CONFIG,
            Language::JavaScript => &lang_javascript::CONFIG,
            Language::TypeScript => &lang_typescript::CONFIG,
            Language::Tsx => &lang_typescript::TSX_CONFIG,
            Language::C => &lang_c::CONFIG,
            Language::Cpp => &lang_cpp::CONFIG,
            Language::Java => &lang_java::CONFIG,
            Language::CSharp => &lang_csharp::CONFIG,
            Language::Php => &lang_php::CONFIG,
            Language::ObjectiveC => &lang_objc::CONFIG,
            Language::Kotlin => &lang_kotlin::CONFIG,
            Language::Swift => &lang_swift::CONFIG,
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
/// Bash → Rust → Python → Go → JavaScript → TypeScript → TSX → C → C++ →
/// Java → C# → PHP → Objective-C → Kotlin → Swift.
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;

    if lang_bash::CONFIG.matches_extension(ext) {
        return Some(Language::Bash);
    }

    if lang_rust::CONFIG.matches_extension(ext) {
        return Some(Language::Rust);
    }

    if lang_python::CONFIG.matches_extension(ext) {
        return Some(Language::Python);
    }

    if lang_go::CONFIG.matches_extension(ext) {
        return Some(Language::Go);
    }

    if lang_javascript::CONFIG.matches_extension(ext) {
        return Some(Language::JavaScript);
    }

    if lang_typescript::CONFIG.matches_extension(ext) {
        return Some(Language::TypeScript);
    }
    if lang_typescript::TSX_CONFIG.matches_extension(ext) {
        return Some(Language::Tsx);
    }

    if lang_c::CONFIG.matches_extension(ext) {
        return Some(Language::C);
    }
    if lang_cpp::CONFIG.matches_extension(ext) {
        return Some(Language::Cpp);
    }
    if lang_java::CONFIG.matches_extension(ext) {
        return Some(Language::Java);
    }
    if lang_csharp::CONFIG.matches_extension(ext) {
        return Some(Language::CSharp);
    }
    if lang_php::CONFIG.matches_extension(ext) {
        return Some(Language::Php);
    }
    if lang_objc::CONFIG.matches_extension(ext) {
        return Some(Language::ObjectiveC);
    }
    if lang_kotlin::CONFIG.matches_extension(ext) {
        return Some(Language::Kotlin);
    }
    if lang_swift::CONFIG.matches_extension(ext) {
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
}
