//! Tree-sitter structural identity for span recovery.
//!
//! `tree_path` provides format-invariant item identity by encoding an item's
//! position in the AST as a `::` delimited path of (kind, name) pairs.
//! For example, `fn.add_money` or `impl.Money::fn.new`.
//!
//! When `tree_path` is populated and a tree-sitter grammar is available for
//! the source language, `liyi check --fix` uses it to
//! locate items by structural identity, making span recovery deterministic
//! across formatting changes, import additions, and line reflows.

pub mod inject;
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

/// Convert a tree-sitter node's exclusive end-position row to a 0-indexed
/// *inclusive* end row.  Tree-sitter end positions point one byte past the
/// last byte of the node.  When that byte is at column 0, the node's last
/// content is on the *previous* row (the trailing `\n` pushed the cursor
/// forward).  This function returns the row that actually contains the
/// node's last non-newline content.
#[inline]
fn node_end_row_inclusive(node: &Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

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
///
/// When the tree_path contains an injection marker (e.g., `key.run//bash`),
/// the resolver extracts the injected content from the host node, sub-parses
/// it with the injected language, and resolves remaining segments against the
/// inner tree. Spans are translated back to outer-file coordinates.
// @liyi:related injection-pair-attachment
// @liyi:related content-offset-correctness
pub fn resolve_tree_path(source: &str, tree_path: &str, lang: Language) -> Option<[usize; 2]> {
    if tree_path.is_empty() {
        return None;
    }

    let parsed = parser::TreePath::parse(tree_path).ok()?;

    if parsed.pairs.is_empty() {
        return None;
    }

    // Check for an injection marker in the parsed pairs.
    let injection_idx = parsed.pairs.iter().position(|p| p.injection.is_some());

    if let Some(idx) = injection_idx {
        return resolve_with_injection(source, &parsed, idx, lang);
    }

    let pairs: Vec<PathPair<'_>> = parsed
        .pairs
        .iter()
        .map(|p| PathPair {
            kind: &p.kind,
            name: &p.name,
            index: p.index,
        })
        .collect();

    let config = lang.config();
    let mut parser = make_parser(lang);
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let node = resolve_segments(config, &root, &pairs, source)?;

    // Return 1-indexed inclusive line range
    let start_line = node.start_position().row + 1;
    let end_line = node_end_row_inclusive(&node) + 1;
    Some([start_line, end_line])
}

/// Resolve a tree_path that contains an injection marker.
///
/// 1. Resolve host-side segments up to (and including) the injection pair.
/// 2. Extract the injected content from the host node's value.
/// 3. Sub-parse the content with the injected language.
/// 4. Resolve remaining segments against the inner parse tree.
/// 5. Translate inner spans back to outer-file coordinates.
fn resolve_with_injection(
    source: &str,
    parsed: &parser::TreePath,
    injection_idx: usize,
    host_lang: Language,
) -> Option<[usize; 2]> {
    // Build host-side pairs (up to and including the injection pair, but
    // without the injection marker — we need the host node).
    let host_pairs: Vec<PathPair<'_>> = parsed.pairs[..=injection_idx]
        .iter()
        .map(|p| PathPair {
            kind: &p.kind,
            name: &p.name,
            index: p.index,
        })
        .collect();

    let config = host_lang.config();
    let mut ts_parser = make_parser(host_lang);
    let tree = ts_parser.parse(source, None)?;
    let root = tree.root_node();

    // Resolve to the host node (e.g., the `run:` block_mapping_pair).
    let host_node = resolve_segments(config, &root, &host_pairs, source)?;

    // Get the value child of the host node for content extraction.
    let value_node = host_node.child_by_field_name("value")?;

    // Extract the content to sub-parse.
    let extracted = inject::extract_yaml_content(&value_node, source)?;

    // Determine the injected language from the marker.
    let injection_lang_str = parsed.pairs[injection_idx].injection.as_deref()?;
    let injection_lang = language_from_name(injection_lang_str)?;

    // If there are no remaining segments after the injection, return the
    // span of the entire injected content in outer-file coordinates.
    if injection_idx + 1 >= parsed.pairs.len() {
        let start = extracted.line_offset + 1; // 1-indexed
        let num_lines = extracted.text.lines().count().max(1);
        let end = extracted.line_offset + num_lines;
        return Some([start, end]);
    }

    // Build inner-side pairs from segments after the injection marker.
    let inner_pairs: Vec<PathPair<'_>> = parsed.pairs[injection_idx + 1..]
        .iter()
        .map(|p| PathPair {
            kind: &p.kind,
            name: &p.name,
            index: p.index,
        })
        .collect();

    // Sub-parse the extracted content with the injected language.
    let inner_config = injection_lang.config();
    let mut inner_parser = make_parser(injection_lang);
    let inner_tree = inner_parser.parse(&extracted.text, None)?;
    let inner_root = inner_tree.root_node();

    // Resolve the remaining segments in the inner tree.
    let inner_node = resolve_segments(inner_config, &inner_root, &inner_pairs, &extracted.text)?;

    // Translate inner spans back to outer-file coordinates.
    let start_line = inner_node.start_position().row + extracted.line_offset + 1;
    let end_line = node_end_row_inclusive(&inner_node) + extracted.line_offset + 1;
    Some([start_line, end_line])
}

/// Map a language name string (from `//lang` injection markers) to a `Language`.
fn language_from_name(name: &str) -> Option<Language> {
    match name {
        "bash" | "sh" => Some(Language::Bash),
        "rust" | "rs" => Some(Language::Rust),
        "python" | "py" => Some(Language::Python),
        "javascript" | "js" => Some(Language::JavaScript),
        "typescript" | "ts" => Some(Language::TypeScript),
        "tsx" => Some(Language::Tsx),
        "go" => Some(Language::Go),
        "ruby" | "rb" => Some(Language::Ruby),
        "c" => Some(Language::C),
        "cpp" | "cxx" => Some(Language::Cpp),
        "java" => Some(Language::Java),
        "csharp" | "cs" => Some(Language::CSharp),
        "php" => Some(Language::Php),
        "objc" => Some(Language::ObjectiveC),
        "kotlin" | "kt" => Some(Language::Kotlin),
        "swift" => Some(Language::Swift),
        "dart" => Some(Language::Dart),
        "zig" => Some(Language::Zig),
        "toml" => Some(Language::Toml),
        "json" => Some(Language::Json),
        "yaml" | "yml" => Some(Language::Yaml),
        _ => None,
    }
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
/// When a tree_path like `key.steps[2]` resolves to an element whose hash
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

    let pairs: Vec<PathPair<'_>> = parsed
        .pairs
        .iter()
        .map(|p| PathPair {
            kind: &p.kind,
            name: &p.name,
            index: p.index,
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
        let end_line = node_end_row_inclusive(&target_node) + 1;
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
    let mut new_pairs = parsed.pairs.clone();
    new_pairs[indexed_pos].index = Some(new_index);

    let updated_tree_path = parser::TreePath { pairs: new_pairs }.serialize();

    Some(SiblingScanResult {
        span,
        updated_tree_path,
    })
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
    while config.transparent_kinds.contains(&body.kind())
        && body.kind() != "array"
        && body.kind() != "block_sequence"
    {
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

/// Compute the canonical `tree_path` for the AST node at the given span,
/// with injection profile detection.
///
/// When `repo_path` matches an active injection profile and `span` falls inside
/// an injection zone (e.g., a `run:` block scalar in GitHub Actions YAML), the
/// returned path includes the `//lang` injection marker and inner-language path
/// segments.
///
/// Falls back to `compute_tree_path` when no injection applies.
// @liyi:related injection-pair-attachment
// @liyi:related content-offset-correctness
pub fn compute_tree_path_injected(
    source: &str,
    span: [usize; 2],
    lang: Language,
    repo_path: &Path,
) -> String {
    let profiles = inject::detect_injection_profiles(repo_path);
    if profiles.is_empty() {
        return compute_tree_path(source, span, lang);
    }

    let config = lang.config();
    let mut ts_parser = make_parser(lang);
    let tree = match ts_parser.parse(source, None) {
        Some(t) => t,
        None => return String::new(),
    };
    let root = tree.root_node();

    let target_start = span[0].saturating_sub(1);
    let target_end = span[1].saturating_sub(1);

    // Try to find an injection zone containing the target span.
    if let Some(result) =
        find_injection_zone(config, &root, source, target_start, target_end, &profiles)
    {
        return result;
    }

    // No injection zone — fall back to base-language compute.
    let node = match find_item_in_range(config, &root, target_start, target_end) {
        Some(n) => n,
        None => return String::new(),
    };
    build_path_to_node(config, &root, &node, source)
}

/// Search for an injection zone containing the target span.
///
/// Walks the host AST looking for `block_mapping_pair` nodes whose value
/// contains the target span and whose key name matches an active injection
/// rule. When found, extracts the content, sub-parses it, computes the
/// inner tree_path, and returns the composite `host_path//lang::inner_path`.
fn find_injection_zone(
    config: &LanguageConfig,
    root: &Node,
    source: &str,
    target_start: usize,
    target_end: usize,
    profiles: &[&inject::InjectionProfile],
) -> Option<String> {
    // Find all block_mapping_pair nodes that contain the target span.
    let mut candidate = find_injection_candidate(root, source, target_start, target_end)?;

    // Walk up from the candidate to find one matching an injection rule.
    loop {
        let key_name = {
            let key_node = candidate.child_by_field_name("key")?;
            lang_yaml::leaf_text_pub(&key_node, source)?.to_string()
        };

        // Collect ancestor keys for the candidate node.
        let ancestor_keys_owned = inject::collect_ancestor_keys(&candidate, source);
        let ancestor_keys: Vec<&str> = ancestor_keys_owned.iter().map(|s| s.as_str()).collect();

        // Check each active profile for a matching rule.
        for profile in profiles {
            if let Some(rule) = profile.find_rule(&key_name, &ancestor_keys) {
                // Found a matching injection rule.
                let value_node = candidate.child_by_field_name("value")?;
                let extracted = inject::extract_yaml_content(&value_node, source)?;

                // Check that the target span falls within the extracted content.
                let content_start = extracted.line_offset;
                let content_end = content_start + extracted.text.lines().count().max(1) - 1;
                if target_start < content_start || target_end > content_end {
                    continue;
                }

                // Build the host-side path to this injection-point node.
                let host_path = build_path_to_node(config, root, &candidate, source);
                if host_path.is_empty() {
                    return None;
                }

                // Map the injection language to its name string.
                let lang_name = language_to_name(rule.language);

                // Translate target span to inner coordinates.
                let inner_start = target_start - extracted.line_offset;
                let inner_end = target_end - extracted.line_offset;
                let inner_span = [inner_start + 1, inner_end + 1]; // back to 1-indexed

                // Compute the inner tree_path.
                let inner_path = compute_tree_path(&extracted.text, inner_span, rule.language);

                if inner_path.is_empty() {
                    // Target is inside injection zone but no structural item found.
                    return Some(format!("{host_path}//{lang_name}"));
                }

                return Some(format!("{host_path}//{lang_name}::{inner_path}"));
            }
        }

        // Move up to parent block_mapping_pair.
        candidate = find_ancestor_pair(&candidate)?;
    }
}

/// Find the innermost `block_mapping_pair` whose value range contains
/// [target_start, target_end] (0-indexed rows).
fn find_injection_candidate<'a>(
    root: &Node<'a>,
    _source: &str,
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
        if node.kind() == "block_mapping_pair"
            && let Some(value) = node.child_by_field_name("value")
        {
            let v_start = value.start_position().row;
            let v_end = value.end_position().row;
            if v_start <= target_start && v_end >= target_end {
                // This pair's value contains the target — prefer the
                // innermost (smallest) match.
                if let Some(b) = best {
                    let b_size = b.end_position().row - b.start_position().row;
                    let n_size = node.end_position().row - node.start_position().row;
                    if n_size < b_size {
                        *best = Some(*node);
                    }
                } else {
                    *best = Some(*node);
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let c_start = child.start_position().row;
            let c_end = child.end_position().row;
            if c_start <= target_end && c_end >= target_start {
                walk(&child, target_start, target_end, best);
            }
        }
    }

    walk(root, target_start, target_end, &mut best);
    best
}

/// Walk up from a node to find the nearest ancestor `block_mapping_pair`.
fn find_ancestor_pair<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "block_mapping_pair" {
            return Some(n);
        }
        current = n.parent();
    }
    None
}

/// Map a `Language` to its canonical injection marker name.
fn language_to_name(lang: Language) -> &'static str {
    match lang {
        Language::Bash => "bash",
        Language::Rust => "rust",
        Language::Python => "python",
        Language::JavaScript => "javascript",
        Language::TypeScript => "typescript",
        Language::Tsx => "tsx",
        Language::Go => "go",
        Language::Ruby => "ruby",
        Language::C => "c",
        Language::Cpp => "cpp",
        Language::Java => "java",
        Language::CSharp => "csharp",
        Language::Php => "php",
        Language::ObjectiveC => "objc",
        Language::Kotlin => "kotlin",
        Language::Swift => "swift",
        Language::Dart => "dart",
        Language::Zig => "zig",
        Language::Toml => "toml",
        Language::Json => "json",
        Language::Yaml => "yaml",
    }
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
        let end = node_end_row_inclusive(node);

        // Skip nodes that don't overlap our target
        if start > target_end || end < target_start {
            return;
        }

        // Check if this is a named item node within the target range
        if start >= target_start && end <= target_end && is_item_node(config, node) {
            // Prefer the widest (outermost) match
            if let Some(b) = best {
                let b_size = node_end_row_inclusive(b) - b.start_position().row;
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

fn format_segment(short: &str, name: &str, index: Option<usize>) -> String {
    let mut segment = format!("{short}.{}", parser::serialize_name(name));
    if let Some(idx) = index {
        segment.push('[');
        segment.push_str(&idx.to_string());
        segment.push(']');
    }
    segment
}

fn sequence_children<'a>(config: &LanguageConfig, node: &Node<'a>) -> Option<Vec<Node<'a>>> {
    let mut body = config.find_body(node)?;
    while config.transparent_kinds.contains(&body.kind())
        && body.kind() != "array"
        && body.kind() != "block_sequence"
    {
        let mut cursor = body.walk();
        let named: Vec<Node<'_>> = body
            .children(&mut cursor)
            .filter(|c| c.is_named())
            .collect();
        if named.len() == 1 {
            body = named[0];
        } else {
            break;
        }
    }

    if body.kind() != "array" && body.kind() != "block_sequence" {
        return None;
    }

    let mut cursor = body.walk();
    Some(
        body.children(&mut cursor)
            .filter(|c| c.is_named())
            .collect(),
    )
}

fn is_descendant_of(target: &Node, ancestor: &Node) -> bool {
    let mut current = Some(*target);
    while let Some(node) = current {
        if node.id() == ancestor.id() {
            return true;
        }
        current = node.parent();
    }
    false
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
            segments.push(format_segment(short, &name, None));
            return true;
        }
        return false;
    }

    if is_item_node(config, node)
        && let Some(children) = sequence_children(config, node)
    {
        for (idx, child) in children.into_iter().enumerate() {
            if !is_descendant_of(target, &child) {
                continue;
            }

            let mut nested_segments = Vec::new();
            if collect_path(config, &child, target, source, &mut nested_segments) {
                if let (Some(short), Some(name)) = (
                    config.kind_to_shorthand(node.kind()),
                    config.node_name(node, source),
                ) {
                    segments.push(format_segment(short, &name, Some(idx)));
                }
                segments.extend(nested_segments);
                return true;
            }
        }
    }

    // Check children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_start = child.start_byte();
        let child_end = child.end_byte();
        let target_start = target.start_byte();
        let target_end = target.end_byte();

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
                segments.insert(0, format_segment(short, &name, None));
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
    /// Canonical tree_path (e.g., "impl.Money::fn.new").
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
        let end_line = node_end_row_inclusive(&child) + 1;
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
        let span = resolve_tree_path(SAMPLE_RUST, "fn.standalone", Language::Rust);
        assert!(span.is_some(), "should resolve fn.standalone");
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
        let span = resolve_tree_path(SAMPLE_RUST, "struct.Money", Language::Rust);
        assert!(span.is_some(), "should resolve struct.Money");
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
        let span = resolve_tree_path(SAMPLE_RUST, "impl.Money::fn.new", Language::Rust);
        assert!(span.is_some(), "should resolve impl.Money::fn.new");
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
        let span = resolve_tree_path(SAMPLE_RUST, "impl.Money::fn.add", Language::Rust);
        assert!(span.is_some(), "should resolve impl.Money::fn.add");
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
        let span = resolve_tree_path(SAMPLE_RUST, "mod.billing::fn.charge", Language::Rust);
        assert!(span.is_some(), "should resolve mod.billing::fn.charge");
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
        let span = resolve_tree_path(SAMPLE_RUST, "impl.Money", Language::Rust);
        assert!(span.is_some(), "should resolve impl.Money");
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
        let span = resolve_tree_path(SAMPLE_RUST, "fn.nonexistent", Language::Rust);
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
        assert_eq!(path, "fn.standalone");
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
        assert_eq!(path, "impl.Money::fn.new");
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
        assert_eq!(path, "struct.Money");
    }

    #[test]
    fn roundtrip_resolve_compute() {
        // Compute path for fn.standalone, then resolve it — spans should match
        // Use tree-sitter to find exact span
        let resolved_span =
            resolve_tree_path(SAMPLE_RUST, "fn.standalone", Language::Rust).unwrap();

        let computed_path = compute_tree_path(SAMPLE_RUST, resolved_span, Language::Rust);
        assert_eq!(computed_path, "fn.standalone");

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
            "fn.standalone",
            "struct.Money",
            "impl.Money",
            "impl.Money::fn.new",
            "impl.Money::fn.add",
            "mod.billing::fn.charge",
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
        // In SIBLING_BEFORE, impl.Money[1] points to `fn second`.
        // Compute hash of `fn second` from the original source.
        let original_span =
            resolve_tree_path(SIBLING_BEFORE, "impl.Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        // In SIBLING_AFTER, `fn zeroth` was inserted before `fn first`,
        // so impl.Money[1] now points to `fn first` (wrong).
        // Sibling scan should find `fn second` at index 2.
        let result = resolve_tree_path_sibling_scan(
            SIBLING_AFTER,
            "impl.Money[1]",
            Language::Rust,
            &original_hash,
        );

        let result = result.expect("sibling scan should find shifted element");
        assert_eq!(result.updated_tree_path, "impl.Money[2]");

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
            "impl.Money",
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
            "impl.Money[1]",
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
            resolve_tree_path(SIBLING_BEFORE, "impl.Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        let result =
            resolve_tree_path_sibling_scan(shrunk, "impl.Money[1]", Language::Rust, &original_hash);

        let result = result.expect("sibling scan should find element at new index");
        assert_eq!(result.updated_tree_path, "impl.Money[0]");
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
            resolve_tree_path(SIBLING_BEFORE, "impl.Money[1]", Language::Rust).unwrap();
        let (original_hash, _) = hash_span(SIBLING_BEFORE, original_span).unwrap();

        let result = resolve_tree_path_sibling_scan(
            rewritten,
            "impl.Money[1]",
            Language::Rust,
            &original_hash,
        );
        assert!(
            result.is_none(),
            "should return None when no sibling matches"
        );
    }

    // -----------------------------------------------------------------------
    // Injection resolver tests
    // -----------------------------------------------------------------------

    const SAMPLE_GHA: &str = r#"name: CI

on: push

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: |
          set -euo pipefail
          cargo build --release
      - name: Test
        run: cargo test
"#;

    const SAMPLE_GHA_FUNC: &str = r#"name: CI

on: push

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Setup
        run: |
          setup() {
            echo "setting up"
          }
          setup
"#;

    #[test]
    fn resolve_injection_bash_function() {
        let span = resolve_tree_path(
            SAMPLE_GHA_FUNC,
            "key.jobs::key.build::key.steps[0]::key.run//bash::fn.setup",
            Language::Yaml,
        );
        assert!(span.is_some(), "should resolve injected bash function");
        let [start, end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_GHA_FUNC.lines().collect();
        assert!(
            lines[start - 1].contains("setup()"),
            "span start should point to setup function, got: {}",
            lines[start - 1]
        );
        assert!(end >= start, "span end should be >= start");
    }

    #[test]
    fn resolve_injection_no_inner_segments() {
        // When the path ends at the injection marker (no inner segments),
        // the resolver should return the span of the injected content.
        // Step 1 (0-indexed) is the "Build" step with `run: |` block.
        let span = resolve_tree_path(
            SAMPLE_GHA,
            "key.jobs::key.build::key.steps[1]::key.run//bash",
            Language::Yaml,
        );
        assert!(
            span.is_some(),
            "should resolve injection without inner path"
        );
        let [start, end] = span.unwrap();
        assert!(start > 0);
        assert!(end >= start);
    }

    #[test]
    fn resolve_injection_returns_none_for_bad_inner_path() {
        let span = resolve_tree_path(
            SAMPLE_GHA_FUNC,
            "key.jobs::key.build::key.steps[0]::key.run//bash::fn.nonexistent",
            Language::Yaml,
        );
        assert!(
            span.is_none(),
            "should return None for nonexistent inner function"
        );
    }

    #[test]
    fn resolve_injection_returns_none_for_unknown_language() {
        let span = resolve_tree_path(
            SAMPLE_GHA,
            "key.jobs::key.build::key.steps[0]::key.run//unknown_lang",
            Language::Yaml,
        );
        assert!(
            span.is_none(),
            "should return None for unknown injected language"
        );
    }

    // -----------------------------------------------------------------------
    // Compute injection tests
    // -----------------------------------------------------------------------

    #[test]
    fn compute_injection_bash_function() {
        // The setup() function in SAMPLE_GHA_FUNC starts at line 11
        // (0-indexed row 10). Given the block scalar starts at row 10
        // (content after indicator), setup() { is on line 11.
        let lines: Vec<&str> = SAMPLE_GHA_FUNC.lines().collect();
        // Find the line with "setup() {"
        let setup_line = lines
            .iter()
            .position(|l| l.contains("setup()"))
            .expect("should find setup() line")
            + 1; // 1-indexed
        let end_line = lines
            .iter()
            .rposition(|l| l.trim() == "}")
            .expect("should find closing brace")
            + 1;

        let path = compute_tree_path_injected(
            SAMPLE_GHA_FUNC,
            [setup_line, end_line],
            Language::Yaml,
            Path::new(".github/workflows/ci.yml"),
        );

        assert!(
            path.contains("//bash"),
            "path should contain //bash injection marker, got: {path}"
        );
        assert!(
            path.contains("fn.setup"),
            "path should contain fn.setup, got: {path}"
        );
        assert!(
            path.contains("key.run"),
            "path should contain key.run, got: {path}"
        );
    }

    #[test]
    fn compute_injection_roundtrip() {
        let lines: Vec<&str> = SAMPLE_GHA_FUNC.lines().collect();
        let setup_line = lines
            .iter()
            .position(|l| l.contains("setup()"))
            .expect("should find setup() line")
            + 1;
        let closing_line = lines
            .iter()
            .enumerate()
            .skip(setup_line)
            .find(|(_, l)| l.trim() == "}")
            .map(|(i, _)| i + 1)
            .expect("should find closing brace");

        let computed_path = compute_tree_path_injected(
            SAMPLE_GHA_FUNC,
            [setup_line, closing_line],
            Language::Yaml,
            Path::new(".github/workflows/ci.yml"),
        );

        assert!(
            !computed_path.is_empty(),
            "computed path should not be empty"
        );
        assert!(
            computed_path.contains("key.steps[0]::key.run"),
            "computed path should preserve the sequence index, got: {computed_path}"
        );
        assert!(
            computed_path.contains("//bash"),
            "computed path should contain //bash, got: {computed_path}"
        );

        let resolved_span = resolve_tree_path(SAMPLE_GHA_FUNC, &computed_path, Language::Yaml);
        assert!(
            resolved_span.is_some(),
            "resolve should succeed for computed path: {computed_path}"
        );
    }

    #[test]
    fn compute_no_injection_for_plain_yaml() {
        // When the repo path doesn't match any injection profile,
        // compute_tree_path_injected should behave like compute_tree_path.
        let yaml = "name: test\nversion: 1\n";
        let base = compute_tree_path(yaml, [1, 1], Language::Yaml);
        let injected = compute_tree_path_injected(
            yaml,
            [1, 1],
            Language::Yaml,
            Path::new("config/settings.yaml"),
        );
        assert_eq!(base, injected);
    }

    // ---------------------------------------------------------------
    // Doc comment detection tests
    // ---------------------------------------------------------------

    /// Helper: parse source, find the item-level node at the resolved
    /// tree_path span, and call `has_doc_comment`.
    fn check_doc_comment(source: &str, tree_path: &str, lang: Language) -> Option<bool> {
        let span = resolve_tree_path(source, tree_path, lang)?;
        let config = lang.config();
        let mut parser = make_parser(lang);
        let tree = parser.parse(source, None)?;
        let root = tree.root_node();
        fn find_item_node<'a>(
            node: tree_sitter::Node<'a>,
            span: [usize; 2],
            config: &LanguageConfig,
        ) -> Option<tree_sitter::Node<'a>> {
            let node_start = node.start_position().row + 1;
            let node_end = node.end_position().row + 1;
            if node_start == span[0]
                && node_end == span[1]
                && config.kind_to_shorthand(node.kind()).is_some()
            {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_item_node(child, span, config) {
                    return Some(found);
                }
            }
            None
        }
        let node = find_item_node(root, span, config)?;
        config.has_doc_comment(&node, source)
    }

    #[test]
    fn doc_comment_kotlin_kdoc_block() {
        let src = "/** Adds two numbers */\nfun add(a: Int, b: Int): Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Kotlin),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_kotlin_triple_slash() {
        let src = "/// Adds two numbers\nfun add(a: Int, b: Int): Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Kotlin),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_kotlin_regular_not_doc() {
        let src = "// regular comment\nfun add(a: Int, b: Int): Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Kotlin),
            Some(false)
        );
    }

    #[test]
    fn doc_comment_swift_triple_slash() {
        let src = "/// Adds two numbers\nfunc add(a: Int, b: Int) -> Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Swift),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_swift_multiline_doc() {
        let src = "/** Adds two numbers */\nfunc add(a: Int, b: Int) -> Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Swift),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_swift_regular_not_doc() {
        let src = "// regular\nfunc add(a: Int, b: Int) -> Int { return a + b }\n";
        assert_eq!(
            check_doc_comment(src, "fn.add", Language::Swift),
            Some(false)
        );
    }

    #[test]
    fn doc_comment_c_block_doc() {
        let src = "/** Process input */\nvoid process(int x) { return; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.process", Language::C),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_c_triple_slash() {
        let src = "/// Process input\nvoid process(int x) { return; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.process", Language::C),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_c_regular_not_doc() {
        let src = "// regular\nvoid process(int x) { return; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.process", Language::C),
            Some(false)
        );
    }

    #[test]
    fn doc_comment_cpp_block_doc() {
        let src = "/** Standalone function */\nvoid standalone() {}\n";
        assert_eq!(
            check_doc_comment(src, "fn.standalone", Language::Cpp),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_cpp_triple_slash() {
        let src = "/// Standalone function\nvoid standalone() {}\n";
        assert_eq!(
            check_doc_comment(src, "fn.standalone", Language::Cpp),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_csharp_triple_slash() {
        let src = "class Foo {\n    /// Adds two numbers\n    int Add(int a, int b) { return a + b; }\n}\n";
        assert_eq!(
            check_doc_comment(src, "class.Foo::fn.Add", Language::CSharp),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_csharp_regular_not_doc() {
        let src = "class Foo {\n    // regular\n    int Add(int a, int b) { return a + b; }\n}\n";
        assert_eq!(
            check_doc_comment(src, "class.Foo::fn.Add", Language::CSharp),
            Some(false)
        );
    }

    #[test]
    fn doc_comment_php_phpdoc() {
        let src = "/** Find a user */\nfunction findUser(int $id): ?User { return null; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.findUser", Language::Php),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_php_regular_not_doc() {
        let src = "// regular\nfunction findUser(int $id): ?User { return null; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.findUser", Language::Php),
            Some(false)
        );
    }

    #[test]
    fn doc_comment_objc_block_doc() {
        let src = "/** Helper function */\nvoid helper(void) { return; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.helper", Language::ObjectiveC),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_objc_triple_slash() {
        let src = "/// Helper function\nvoid helper(void) { return; }\n";
        assert_eq!(
            check_doc_comment(src, "fn.helper", Language::ObjectiveC),
            Some(true)
        );
    }

    #[test]
    fn doc_comment_zig_triple_slash() {
        let src = "/// Add two numbers\nfn add(a: i32, b: i32) i32 { return a + b; }\n";
        assert_eq!(check_doc_comment(src, "fn.add", Language::Zig), Some(true));
    }

    #[test]
    fn doc_comment_zig_regular_not_doc() {
        let src = "// regular comment\nfn add(a: i32, b: i32) i32 { return a + b; }\n";
        assert_eq!(check_doc_comment(src, "fn.add", Language::Zig), Some(false));
    }
}
