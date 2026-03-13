//! Tree-sitter structural identity for span recovery.
//!
//! `tree_path` provides format-invariant item identity by encoding an item's
//! position in the AST as a `::` delimited path of (kind, name) segments.
//! For example, `fn::add_money` or `impl::Money::fn::new`.
//!
//! When `tree_path` is populated and a tree-sitter grammar is available for
//! the source language, `liyi check --fix` uses it to
//! locate items by structural identity, making span recovery deterministic
//! across formatting changes, import additions, and line reflows.

mod lang_bash;
mod lang_c;
mod lang_cpp;
mod lang_csharp;
mod lang_dart;
mod lang_go;
mod lang_java;
mod lang_javascript;
mod lang_json;
mod lang_kotlin;
mod lang_objc;
mod lang_php;
mod lang_python;
mod lang_ruby;
mod lang_rust;
mod lang_swift;
mod lang_toml;
mod lang_typescript;
mod lang_yaml;
mod lang_zig;
pub mod parser;

use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Language as TSLanguage, Node, Parser};

use crate::hashing::hash_span;

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
    /// Optional callback to detect whether a doc comment is attached to an item node.
    /// Returns true if a doc comment immediately precedes or is inside the item.
    doc_comment_detector: Option<fn(&Node, &str) -> bool>,
    /// Node kinds that `resolve_segments` should look through transparently.
    ///
    /// Some grammars wrap item nodes in intermediate containers (e.g., Dart's
    /// `class_member` → `method_signature` → `function_signature`).  When
    /// resolving a path segment inside a body, nodes whose kind appears in
    /// this list are recursed into automatically so that the inner item nodes
    /// become visible to the resolver.
    transparent_kinds: &'static [&'static str],
}

impl LanguageConfig {
    /// Map tree-sitter node kind → tree_path shorthand.
    // @liyi:related reuse-kind-map-as-item-definition
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
    ///
    /// The sentinel `"."` in `body_fields` means the node itself acts as
    /// its own body container (e.g., TOML tables whose pairs are direct
    /// children).
    fn find_body<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        for field in self.body_fields {
            if *field == "." {
                return Some(*node);
            }
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

    /// Detect whether a doc comment is attached to an item node.
    /// Returns `None` if the language has no doc comment detector.
    // @liyi:related tree-sitter-signals-always-present
    pub fn has_doc_comment(&self, node: &Node, source: &str) -> Option<bool> {
        self.doc_comment_detector.map(|f| f(node, source))
    }
}

/// Supported languages for tree_path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Bash,
    Dart,
    Rust,
    Ruby,
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
    Zig,
    Toml,
    Json,
    Yaml,
}

impl Language {
    /// Get the language configuration for this language.
    fn config(&self) -> &'static LanguageConfig {
        match self {
            Language::Bash => &lang_bash::CONFIG,
            Language::Dart => &lang_dart::CONFIG,
            Language::Rust => &lang_rust::CONFIG,
            Language::Ruby => &lang_ruby::CONFIG,
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
            Language::Zig => &lang_zig::CONFIG,
            Language::Toml => &lang_toml::CONFIG,
            Language::Json => &lang_json::CONFIG,
            Language::Yaml => &lang_yaml::CONFIG,
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
/// Bash → Dart → Rust → Ruby → Python → Go → JavaScript → TypeScript → TSX → C → C++ →
/// Java → C# → PHP → Objective-C → Kotlin → Swift → Zig → TOML → JSON → YAML.
// @liyi:related graceful-degradation
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;

    if lang_bash::CONFIG.matches_extension(ext) {
        return Some(Language::Bash);
    }

    if lang_dart::CONFIG.matches_extension(ext) {
        return Some(Language::Dart);
    }

    if lang_rust::CONFIG.matches_extension(ext) {
        return Some(Language::Rust);
    }

    if lang_ruby::CONFIG.matches_extension(ext) {
        return Some(Language::Ruby);
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
    if lang_zig::CONFIG.matches_extension(ext) {
        return Some(Language::Zig);
    }

    if lang_toml::CONFIG.matches_extension(ext) {
        return Some(Language::Toml);
    }
    if lang_json::CONFIG.matches_extension(ext) {
        return Some(Language::Json);
    }
    if lang_yaml::CONFIG.matches_extension(ext) {
        return Some(Language::Yaml);
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

/// Resolve a `tree_path` to a source span `[start_line, end_line]` (1-indexed,
/// inclusive).
///
/// Returns `None` if the tree_path cannot be resolved (item renamed, deleted,
/// grammar unavailable, or language not supported).
pub fn resolve_tree_path(source: &str, tree_path: &str, lang: Language) -> Option<[usize; 2]> {
    if tree_path.is_empty() {
        return None;
    }

    let parsed = parser::TreePath::parse(tree_path).ok()?;

    // Collect non-injection segments into (kind, name, optional_index) triples.
    let mut flat: Vec<FlatSegment<'_>> = Vec::new();
    for s in &parsed.segments {
        match s {
            parser::Segment::Kind(k) => flat.push(FlatSegment::KindOrName(k.as_str(), None)),
            parser::Segment::Name(n, idx) => flat.push(FlatSegment::KindOrName(n.as_str(), *idx)),
            parser::Segment::Injection(_) => {}
        }
    }

    if !flat.len().is_multiple_of(2) || flat.is_empty() {
        return None;
    }

    let pairs: Vec<PathPair<'_>> = flat
        .chunks(2)
        .map(|c| {
            let kind = c[0].text();
            let (name, idx) = c[1].text_and_index();
            PathPair {
                kind,
                name,
                index: idx,
            }
        })
        .collect();

    let config = lang.config();
    let mut parser = make_parser(lang);
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let node = resolve_segments(config, &root, &pairs, source)?;

    // Return 1-indexed inclusive line range
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    Some([start_line, end_line])
}

/// Result of a hash-based sibling scan within an array.
pub struct SiblingScanResult {
    /// The 1-indexed inclusive span of the matched sibling element.
    pub span: [usize; 2],
    /// The updated tree_path with the corrected array index.
    pub updated_tree_path: String,
}

/// Attempt to find an array element matching `expected_hash` by scanning
/// siblings of an indexed tree_path segment.
///
/// When a tree_path like `key::steps[2]` resolves to an element whose hash
/// doesn't match (e.g., because a new element was inserted before index 2),
/// this function scans all sibling elements in the same array container to
/// find one whose content matches `expected_hash`.
///
/// Returns `Some(SiblingScanResult)` if exactly one element matches.
/// Returns `None` if the tree_path has no indexed segment, the parent array
/// can't be resolved, or zero/multiple elements match (ambiguous).
// @liyi:related sibling-scan-recovery
pub fn resolve_tree_path_sibling_scan(
    source: &str,
    tree_path: &str,
    lang: Language,
    expected_hash: &str,
) -> Option<SiblingScanResult> {
    if tree_path.is_empty() {
        return None;
    }

    let parsed = parser::TreePath::parse(tree_path).ok()?;

    // Flatten to (kind, name, optional_index) pairs, skipping injections.
    let mut flat: Vec<FlatSegment<'_>> = Vec::new();
    for s in &parsed.segments {
        match s {
            parser::Segment::Kind(k) => flat.push(FlatSegment::KindOrName(k.as_str(), None)),
            parser::Segment::Name(n, idx) => flat.push(FlatSegment::KindOrName(n.as_str(), *idx)),
            parser::Segment::Injection(_) => {}
        }
    }

    if !flat.len().is_multiple_of(2) || flat.is_empty() {
        return None;
    }

    let pairs: Vec<PathPair<'_>> = flat
        .chunks(2)
        .map(|c| {
            let kind = c[0].text();
            let (name, idx) = c[1].text_and_index();
            PathPair {
                kind,
                name,
                index: idx,
            }
        })
        .collect();

    // Find the last pair with an index — that's our array access point.
    let indexed_pos = pairs.iter().rposition(|p| p.index.is_some())?;

    let config = lang.config();
    let mut ts_parser = make_parser(lang);
    let tree = ts_parser.parse(source, None)?;
    let root = tree.root_node();

    // Resolve the parent context to reach the container holding the key node.
    let container = if indexed_pos == 0 {
        root
    } else {
        let parent_node = resolve_segments(config, &root, &pairs[..indexed_pos], source)?;
        config.find_body(&parent_node)?
    };

    // Find the key node for the indexed pair within the container.
    let indexed_pair = &pairs[indexed_pos];
    let ts_kind = config.shorthand_to_kind(indexed_pair.kind)?;
    let key_node = {
        let mut cursor = container.walk();
        container.children(&mut cursor).find(|c| {
            c.kind() == ts_kind
                && config
                    .node_name(c, source)
                    .is_some_and(|n| *n == *indexed_pair.name)
        })?
    };

    // Get the array body (the value of this key — typically an array node).
    let array_body = config.find_body(&key_node)?;

    let suffix = &pairs[indexed_pos + 1..];

    // Iterate all named children and find those matching expected_hash.
    let mut cursor = array_body.walk();
    let children: Vec<Node<'_>> = array_body
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();

    let mut candidates: Vec<(usize, [usize; 2])> = Vec::new();

    for (i, child) in children.iter().enumerate() {
        // If there's a suffix path, resolve it within this child.
        let target_node = if suffix.is_empty() {
            *child
        } else {
            match resolve_segments(config, child, suffix, source) {
                Some(n) => n,
                None => continue,
            }
        };

        let start_line = target_node.start_position().row + 1;
        let end_line = target_node.end_position().row + 1;
        let span = [start_line, end_line];

        if let Ok((hash, _)) = hash_span(source, span)
            && hash == expected_hash
        {
            candidates.push((i, span));
        }
    }

    // Exactly one match → unambiguous.
    if candidates.len() != 1 {
        return None;
    }

    let (new_index, span) = candidates[0];

    // Build updated tree_path with the corrected index.
    let mut new_segments = parsed.segments.clone();
    // Map pair position back to segment position: the Name segment for
    // pair[indexed_pos] is the (indexed_pos * 2 + 1)th non-injection segment.
    let target_non_injection_idx = indexed_pos * 2 + 1;
    let mut non_injection_count = 0;
    for seg in &mut new_segments {
        if matches!(seg, parser::Segment::Injection(_)) {
            continue;
        }
        if non_injection_count == target_non_injection_idx {
            if let parser::Segment::Name(_, idx) = seg {
                *idx = Some(new_index);
            }
            break;
        }
        non_injection_count += 1;
    }

    let updated_tree_path = parser::TreePath {
        segments: new_segments,
    }
    .serialize();

    Some(SiblingScanResult {
        span,
        updated_tree_path,
    })
}

/// Intermediate representation for flattened segments.
enum FlatSegment<'a> {
    KindOrName(&'a str, Option<usize>),
}

impl<'a> FlatSegment<'a> {
    fn text(&self) -> &'a str {
        let FlatSegment::KindOrName(s, _) = self;
        s
    }

    fn text_and_index(&self) -> (&'a str, Option<usize>) {
        let FlatSegment::KindOrName(s, idx) = self;
        (s, *idx)
    }
}

/// A (kind, name, optional_index) triple for resolution.
struct PathPair<'a> {
    kind: &'a str,
    name: &'a str,
    index: Option<usize>,
}

/// Walk the tree to find a node matching the given path segments.
fn resolve_segments<'a>(
    config: &LanguageConfig,
    parent: &Node<'a>,
    segments: &[PathPair<'_>],
    source: &'a str,
) -> Option<Node<'a>> {
    if segments.is_empty() {
        return Some(*parent);
    }

    let pair = &segments[0];
    let ts_kind = config.shorthand_to_kind(pair.kind)?;

    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() == ts_kind {
            if let Some(node_name) = config.node_name(&child, source)
                && *node_name == *pair.name
            {
                let resolved = if let Some(idx) = pair.index {
                    // Index into the Nth positional child of this node's body
                    resolve_indexed_child(config, &child, idx, &segments[1..], source)
                } else if segments.len() == 1 {
                    Some(child)
                } else {
                    // Descend — look inside this node's body
                    resolve_in_body(config, &child, &segments[1..], source)
                };
                if resolved.is_some() {
                    return resolved;
                }
            }
        } else if config.transparent_kinds.contains(&child.kind()) {
            // Look through transparent wrapper nodes (e.g., Dart class_member)
            if let Some(found) = resolve_segments(config, &child, segments, source) {
                return Some(found);
            }
        }
    }

    None
}

/// Resolve the Nth positional child of a node's value (for data-file arrays).
///
/// After finding the named key node, this looks for its value child (typically
/// an array node) and selects the child at the given 0-based index. If there
/// are subsequent path segments, resolution continues from the indexed child.
fn resolve_indexed_child<'a>(
    config: &LanguageConfig,
    node: &Node<'a>,
    index: usize,
    remaining: &[PathPair<'_>],
    source: &'a str,
) -> Option<Node<'a>> {
    // For data-file grammars, the "body" of a key-value pair is its value
    // node (an array or object). Find it, then select the Nth child.
    let mut body = config.find_body(node)?;
    // Walk through transparent wrapper nodes to reach the actual array
    // container (e.g., YAML block_node → block_sequence).
    while config.transparent_kinds.contains(&body.kind()) {
        let mut cursor = body.walk();
        let named: Vec<Node<'a>> = body
            .children(&mut cursor)
            .filter(|c| c.is_named())
            .collect();
        if named.len() == 1 {
            body = named[0];
        } else {
            break;
        }
    }
    let mut cursor = body.walk();
    let child = body
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .nth(index)?;

    if remaining.is_empty() {
        Some(child)
    } else {
        resolve_segments(config, &child, remaining, source)
    }
}

/// Find subsequent segments inside an item's body (e.g., methods inside impl).
fn resolve_in_body<'a>(
    config: &LanguageConfig,
    node: &Node<'a>,
    segments: &[PathPair<'_>],
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
// @liyi:related reuse-kind-map-as-item-definition
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
            segments.push(format!("{short}::{}", parser::serialize_name(&name)));
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
                segments.insert(0, format!("{short}::{}", parser::serialize_name(&name)));
            }
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Item discovery for `liyi init` scaffold
// ---------------------------------------------------------------------------

/// A discovered item from tree-sitter AST traversal.
pub struct DiscoveredItem {
    /// Display name: leaf name for top-level items, container-qualified for nested.
    pub name: String,
    /// 1-indexed inclusive span [start, end].
    pub span: [usize; 2],
    /// Canonical tree_path (e.g., "impl::Money::fn::new").
    pub tree_path: String,
    /// Whether a doc comment was detected (None if detector unavailable).
    pub has_doc_comment: Option<bool>,
}

/// Discover all items in a source file using tree-sitter AST traversal.
///
/// Returns one `DiscoveredItem` per item node found (functions, structs,
/// classes, methods, etc.) as defined by the language's `kind_map`.
// @liyi:related exhaustive-inclusion
pub fn discover_items(source: &str, lang: Language) -> Vec<DiscoveredItem> {
    let config = lang.config();
    let mut parser = make_parser(lang);
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let root = tree.root_node();
    let mut items = Vec::new();
    discover_walk(config, &root, &root, source, &mut items, None);
    items
}

/// Recursive depth-first walk collecting discovered items.
///
/// `container_name` is `Some("ContainerName")` when inside a container
/// item (class, impl, etc.) so nested items get qualified names.
// @liyi:related exhaustive-inclusion
// @liyi:related item-naming-uses-leaf-name
// @liyi:related reuse-kind-map-as-item-definition
fn discover_walk(
    config: &LanguageConfig,
    root: &Node,
    node: &Node,
    source: &str,
    items: &mut Vec<DiscoveredItem>,
    container_name: Option<&str>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !is_item_node(config, &child) {
            // Not an item — still recurse in case there are items deeper
            discover_walk(config, root, &child, source, items, container_name);
            continue;
        }

        let name = match config.node_name(&child, source) {
            Some(n) => n.into_owned(),
            None => continue,
        };

        let start_line = child.start_position().row + 1;
        let end_line = child.end_position().row + 1;
        let span = [start_line, end_line];

        let tree_path = build_path_to_node(config, root, &child, source);

        // Display name: qualified if nested, leaf if top-level
        let display_name = match container_name {
            Some(container) => format!("{container}::{name}"),
            None => name.clone(),
        };

        items.push(DiscoveredItem {
            name: display_name,
            span,
            tree_path,
            has_doc_comment: config.has_doc_comment(&child, source),
        });

        // If this item has a body, recurse into it for nested items
        if let Some(body) = config.find_body(&child) {
            discover_walk(config, root, &body, source, items, Some(&name));
        }
    }
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

    #[test]
    fn discover_items_rust() {
        let items = discover_items(SAMPLE_RUST, Language::Rust);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();

        // Should find all top-level and nested items
        assert!(names.contains(&"Money"), "should discover struct Money");
        assert!(names.contains(&"Money"), "should discover impl Money");
        assert!(
            names.contains(&"Money::new"),
            "should discover nested method new as Money::new"
        );
        assert!(
            names.contains(&"Money::add"),
            "should discover nested method add as Money::add"
        );
        assert!(names.contains(&"billing"), "should discover mod billing");
        assert!(
            names.contains(&"billing::charge"),
            "should discover nested fn charge as billing::charge"
        );
        assert!(
            names.contains(&"standalone"),
            "should discover fn standalone"
        );

        // Verify tree_paths are populated
        for item in &items {
            assert!(
                !item.tree_path.is_empty(),
                "tree_path should be populated for {}",
                item.name
            );
        }

        // Verify spans are valid
        for item in &items {
            assert!(
                item.span[0] >= 1,
                "span start should be >= 1 for {}",
                item.name
            );
            assert!(
                item.span[1] >= item.span[0],
                "span end should be >= start for {}",
                item.name
            );
        }
    }

    #[test]
    fn discover_items_python() {
        let source = r#"class Order:
    def __init__(self, amount):
        self.amount = amount

    def process(self):
        return self.amount > 0

def calculate_total(items):
    return sum(items)
"#;

        let items = discover_items(source, Language::Python);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();

        assert!(names.contains(&"Order"), "should discover class Order");
        assert!(
            names.contains(&"Order::__init__"),
            "should discover nested __init__"
        );
        assert!(
            names.contains(&"Order::process"),
            "should discover nested process"
        );
        assert!(
            names.contains(&"calculate_total"),
            "should discover top-level function"
        );
    }

    // -- sibling scan tests -----------------------------------------------

    /// Source with an impl block whose methods we can index positionally.
    const SIBLING_BEFORE: &str = r#"pub struct Money { amount: i64 }

impl Money {
    fn first(&self) -> i64 { 1 }
    fn second(&self) -> i64 { 2 }
    fn third(&self) -> i64 { 3 }
}
"#;

    /// Same source with a new method inserted at the beginning.
    const SIBLING_AFTER: &str = r#"pub struct Money { amount: i64 }

impl Money {
    fn zeroth(&self) -> i64 { 0 }
    fn first(&self) -> i64 { 1 }
    fn second(&self) -> i64 { 2 }
    fn third(&self) -> i64 { 3 }
}
"#;

    #[test]
    fn sibling_scan_finds_shifted_element() {
        // In SIBLING_BEFORE, impl::Money[1] points to `fn second`.
        // Compute hash of `fn second` from the original source.
        let original_span =
            resolve_tree_path(SIBLING_BEFORE, "impl::Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        // In SIBLING_AFTER, `fn zeroth` was inserted before `fn first`,
        // so impl::Money[1] now points to `fn first` (wrong).
        // Sibling scan should find `fn second` at index 2.
        let result = resolve_tree_path_sibling_scan(
            SIBLING_AFTER,
            "impl::Money[1]",
            Language::Rust,
            &original_hash,
        );

        let result = result.expect("sibling scan should find shifted element");
        assert_eq!(result.updated_tree_path, "impl::Money[2]");

        // Verify the span points to `fn second` in the new source.
        let lines: Vec<&str> = SIBLING_AFTER.lines().collect();
        assert!(
            lines[result.span[0] - 1].contains("fn second"),
            "span should point to fn second, got: {}",
            lines[result.span[0] - 1]
        );
    }

    #[test]
    fn sibling_scan_returns_none_when_no_index() {
        // tree_path without index → Nothing to scan.
        let result = resolve_tree_path_sibling_scan(
            SIBLING_BEFORE,
            "impl::Money",
            Language::Rust,
            "sha256:0000",
        );
        assert!(result.is_none());
    }

    #[test]
    fn sibling_scan_returns_none_when_content_changed() {
        // When no sibling has the expected hash, returns None.
        let result = resolve_tree_path_sibling_scan(
            SIBLING_AFTER,
            "impl::Money[1]",
            Language::Rust,
            "sha256:no_such_hash",
        );
        assert!(result.is_none());
    }

    #[test]
    fn sibling_scan_returns_none_for_empty_path() {
        let result =
            resolve_tree_path_sibling_scan(SIBLING_BEFORE, "", Language::Rust, "sha256:0000");
        assert!(result.is_none());
    }

    #[test]
    fn sibling_scan_handles_deletion_before() {
        // Source with the first method removed — `fn second` shifts from
        // index 1 to index 0.
        let shrunk = r#"pub struct Money { amount: i64 }

impl Money {
    fn second(&self) -> i64 { 2 }
    fn third(&self) -> i64 { 3 }
}
"#;

        let original_span =
            resolve_tree_path(SIBLING_BEFORE, "impl::Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        let result = resolve_tree_path_sibling_scan(
            shrunk,
            "impl::Money[1]",
            Language::Rust,
            &original_hash,
        );

        let result = result.expect("sibling scan should find element at new index");
        assert_eq!(result.updated_tree_path, "impl::Money[0]");
        let lines: Vec<&str> = shrunk.lines().collect();
        assert!(
            lines[result.span[0] - 1].contains("fn second"),
            "span should point to fn second, got: {}",
            lines[result.span[0] - 1]
        );
    }

    #[test]
    fn sibling_scan_no_match_when_all_content_changed() {
        // All methods are different from the original — no sibling matches.
        let rewritten = r#"pub struct Money { amount: i64 }

impl Money {
    fn alpha(&self) -> i64 { 10 }
    fn beta(&self) -> i64 { 20 }
    fn gamma(&self) -> i64 { 30 }
}
"#;

        // Hash from a method that doesn't exist in `rewritten` at all.
        let original_span =
            resolve_tree_path(SIBLING_BEFORE, "impl::Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        let result = resolve_tree_path_sibling_scan(
            rewritten,
            "impl::Money[1]",
            Language::Rust,
            &original_hash,
        );
        assert!(
            result.is_none(),
            "should return None when no sibling matches"
        );
    }
}
