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

use std::path::Path;

use tree_sitter::{Node, Parser};

/// Map from tree_path kind shorthand to tree-sitter-rust node kind strings.
const KIND_MAP: &[(&str, &str)] = &[
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
];

/// Reverse map: tree-sitter node kind → tree_path shorthand.
fn kind_to_shorthand(ts_kind: &str) -> Option<&'static str> {
    KIND_MAP
        .iter()
        .find(|(_, ts)| *ts == ts_kind)
        .map(|(short, _)| *short)
}

/// Forward map: tree_path shorthand → tree-sitter node kind.
fn shorthand_to_kind(short: &str) -> Option<&'static str> {
    KIND_MAP
        .iter()
        .find(|(s, _)| *s == short)
        .map(|(_, ts)| *ts)
}

/// Detect language from file extension. Returns `None` for unsupported
/// languages (only Rust is supported in 0.1).
pub fn detect_language(path: &Path) -> Option<Language> {
    match path.extension()?.to_str()? {
        "rs" => Some(Language::Rust),
        _ => None,
    }
}

/// Supported languages for tree_path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
}

/// Create a tree-sitter parser for the given language.
fn make_parser(lang: Language) -> Parser {
    let mut parser = Parser::new();
    match lang {
        Language::Rust => {
            parser
                .set_language(&tree_sitter_rust::LANGUAGE.into())
                .expect("tree-sitter-rust grammar should load");
        }
    }
    parser
}

/// Extract the name of a named AST node.
///
/// For most items (fn, struct, enum, mod, trait, const, static, type, macro),
/// the name is in the `name` field. For `impl_item`, the name is the text of
/// the `type` field (the type being implemented).
fn node_name<'a>(node: &Node<'a>, source: &'a str) -> Option<&'a str> {
    let kind = node.kind();
    if kind == "impl_item" {
        // impl blocks: use the `type` field text
        let type_node = node.child_by_field_name("type")?;
        Some(&source[type_node.byte_range()])
    } else {
        let name_node = node.child_by_field_name("name")?;
        Some(&source[name_node.byte_range()])
    }
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
/// or grammar unavailable).
pub fn resolve_tree_path(
    source: &str,
    tree_path: &str,
    lang: Language,
) -> Option<[usize; 2]> {
    if tree_path.is_empty() {
        return None;
    }

    let segments = parse_tree_path(tree_path)?;
    let mut parser = make_parser(lang);
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let node = resolve_segments(&root, &segments, source)?;

    // Return 1-indexed inclusive line range
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    Some([start_line, end_line])
}

/// Walk the tree to find a node matching the given path segments.
fn resolve_segments<'a>(
    parent: &Node<'a>,
    segments: &[PathSegment],
    source: &'a str,
) -> Option<Node<'a>> {
    if segments.is_empty() {
        return Some(*parent);
    }

    let seg = &segments[0];
    let ts_kind = shorthand_to_kind(&seg.kind)?;

    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() != ts_kind {
            continue;
        }
        if let Some(name) = node_name(&child, source) {
            if name == seg.name && segments.len() == 1 {
                return Some(child);
            } else if name == seg.name {
                // Descend — look inside this node's body
                return resolve_in_body(&child, &segments[1..], source);
            }
        }
    }

    None
}

/// Find subsequent segments inside an item's body (e.g., methods inside impl).
fn resolve_in_body<'a>(
    node: &Node<'a>,
    segments: &[PathSegment],
    source: &'a str,
) -> Option<Node<'a>> {
    // For impl/mod/trait blocks, the children are inside the declaration_list
    // or body field. Walk all descendants at the next level.
    let body = node
        .child_by_field_name("body")
        .or_else(|| {
            // Try finding declaration_list child directly
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "declaration_list")
        })?;

    resolve_segments(&body, segments, source)
}

/// Compute the canonical `tree_path` for the AST node at the given span.
///
/// Returns an empty string if no suitable structural path can be determined
/// (e.g., the span doesn't align with a named item, or the language is
/// unsupported).
pub fn compute_tree_path(
    source: &str,
    span: [usize; 2],
    lang: Language,
) -> String {
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
    let node = match find_item_in_range(&root, target_start, target_end) {
        Some(n) => n,
        None => return String::new(),
    };

    // Build path from root to this node
    build_path_to_node(&root, &node, source)
}

/// Find the best item node within [target_start, target_end] (0-indexed rows).
///
/// Attributes in Rust are sibling nodes, not children of the item, so a
/// sidecar span that includes `#[derive(...)]` lines will start before the
/// item node.  We therefore match any item whose start/end rows fall within
/// the target range, preferring the widest match (the outermost item).
fn find_item_in_range<'a>(
    root: &Node<'a>,
    target_start: usize,
    target_end: usize,
) -> Option<Node<'a>> {
    let mut best: Option<Node<'a>> = None;

    fn walk<'a>(
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
        if start >= target_start && end <= target_end && is_item_node(node) {
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
            walk(&child, target_start, target_end, best);
        }
    }

    walk(root, target_start, target_end, &mut best);
    best
}

/// Check if a node is an item type we track in tree_path.
fn is_item_node(node: &Node) -> bool {
    kind_to_shorthand(node.kind()).is_some()
}

/// Build the tree_path string for a given target node by walking from root.
fn build_path_to_node(root: &Node, target: &Node, source: &str) -> String {
    let mut segments: Vec<String> = Vec::new();
    if collect_path(root, target, source, &mut segments) {
        segments.join("::")
    } else {
        String::new()
    }
}

/// Recursively find `target` in the tree and collect path segments.
fn collect_path(
    node: &Node,
    target: &Node,
    source: &str,
    segments: &mut Vec<String>,
) -> bool {
    if node.id() == target.id() {
        // We found the target — add this node's segment if it's an item
        if let (Some(short), Some(name)) =
            (kind_to_shorthand(node.kind()), node_name(node, source))
        {
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
            && collect_path(&child, target, source, segments)
        {
            // If this node is an item node, prepend its segment
            if is_item_node(node)
                && let (Some(short), Some(name)) =
                    (kind_to_shorthand(node.kind()), node_name(node, source))
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
        let span =
            resolve_tree_path(SAMPLE_RUST, "mod::billing::fn::charge", Language::Rust);
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
        let start = lines
            .iter()
            .position(|l| l.contains("pub fn new"))
            .unwrap()
            + 1;
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

        let computed_path =
            compute_tree_path(SAMPLE_RUST, resolved_span, Language::Rust);
        assert_eq!(computed_path, "fn::standalone");

        let re_resolved =
            resolve_tree_path(SAMPLE_RUST, &computed_path, Language::Rust).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

    #[test]
    fn detect_language_rust() {
        assert_eq!(
            detect_language(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            detect_language(Path::new("foo.py")),
            None
        );
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
