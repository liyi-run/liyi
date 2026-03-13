//! tree_path parser — formal grammar implementation using nom.
//!
//! This module implements the tree_path grammar spec (v0.2) from
//! `docs/liyi-01x-roadmap.md` Appendix A.

use nom::{
    IResult, Parser as _,
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1, none_of, one_of},
    combinator::{map, recognize},
    multi::many0,
    sequence::{delimited, pair, preceded},
};

/// A segment in a tree_path — either a kind, name, or injection marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Kind shorthand (e.g., "fn", "class", "struct")
    Kind(String),
    /// Item name (e.g., "add", "MyClass", "add function").
    /// The optional index is a 0-based positional child selector
    /// for data-file arrays (e.g., `specs[2]`).
    Name(String, Option<usize>),
    /// Injection marker for M9 (e.g., "//bash")
    Injection(String),
}

/// Parsed tree_path representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreePath {
    pub segments: Vec<Segment>,
}

impl TreePath {
    /// Parse a tree_path string.
    pub fn parse(input: &str) -> Result<Self, String> {
        match parse_tree_path(input) {
            Ok(("", path)) => Ok(path),
            Ok((remainder, _)) => Err(format!("Unexpected trailing input: {remainder:?}")),
            Err(e) => Err(format!("Parse error: {e:?}")),
        }
    }

    /// Serialize a tree_path to string.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for (i, seg) in self.segments.iter().enumerate() {
            // Injection markers attach to the preceding segment without ::
            if i > 0 && !matches!(seg, Segment::Injection(_)) {
                out.push_str("::");
            }
            match seg {
                Segment::Kind(k) => out.push_str(k),
                Segment::Name(n, idx) => {
                    out.push_str(&serialize_name(n));
                    if let Some(i) = idx {
                        out.push('[');
                        out.push_str(&i.to_string());
                        out.push(']');
                    }
                }
                Segment::Injection(lang) => {
                    out.push_str("//");
                    out.push_str(lang);
                }
            }
        }
        out
    }
}

/// Serialize a name, quoting if necessary.
pub fn serialize_name(name: &str) -> String {
    // Check if we need quoting
    let needs_quote = name.is_empty()
        || name.contains('"')
        || name.contains('\\')
        || name.contains("::")
        || name.contains(' ')
        || name.contains('\t')
        || name.contains('\n')
        || name.contains(':')
        || !is_simple_identifier(name);

    if !needs_quote {
        return name.to_string();
    }

    // Escape quotes and backslashes
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Check if a string is a simple identifier (no quoting needed).
///
/// Must stay in sync with `parse_simple_name` — a name is simple iff the
/// parser can round-trip it without quotes.
fn is_simple_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse a complete tree_path.
fn parse_tree_path(input: &str) -> IResult<&str, TreePath> {
    let (input, first) = parse_segment(input)?;
    let (input, rest) = many0(alt((
        // Injection marker directly after a segment (no :: separator): run//bash
        parse_injection_marker,
        // Standard :: separated segment
        preceded(tag("::"), parse_segment),
    )))
    .parse(input)?;
    let mut segments = vec![first];
    segments.extend(rest);
    Ok((input, TreePath { segments }))
}

/// Parse a single segment.
fn parse_segment(input: &str) -> IResult<&str, Segment> {
    alt((
        parse_injection_marker,
        parse_name_with_optional_index(parse_quoted_string),
        map(parse_simple_name_with_index, |(s, idx)| {
            // Heuristic: if it matches common kind patterns and has no index,
            // treat as Kind. Indexed segments are always names.
            if idx.is_none() && is_common_kind(s) {
                Segment::Kind(s.to_string())
            } else {
                Segment::Name(s.to_string(), idx)
            }
        }),
    ))
    .parse(input)
}

/// Parse a quoted string followed by an optional `[index]`.
fn parse_name_with_optional_index(
    inner: fn(&str) -> IResult<&str, String>,
) -> impl FnMut(&str) -> IResult<&str, Segment> {
    move |input: &str| {
        let (input, name) = inner(input)?;
        let (input, idx) = parse_optional_index(input)?;
        Ok((input, Segment::Name(name, idx)))
    }
}

/// Parse a simple name followed by an optional `[index]`.
fn parse_simple_name_with_index(input: &str) -> IResult<&str, (&str, Option<usize>)> {
    let (input, name) = parse_simple_name(input)?;
    let (input, idx) = parse_optional_index(input)?;
    Ok((input, (name, idx)))
}

/// Parse an optional `[N]` index suffix (0-based).
fn parse_optional_index(input: &str) -> IResult<&str, Option<usize>> {
    if input.starts_with('[') {
        let (input, _) = char('[')(input)?;
        let (input, digits) = digit1(input)?;
        let (input, _) = char(']')(input)?;
        let idx: usize = digits.parse().map_err(|_| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        Ok((input, Some(idx)))
    } else {
        Ok((input, None))
    }
}

/// Kind shorthands from all supported language configs.
fn is_common_kind(s: &str) -> bool {
    matches!(
        s,
        "fn" | "annotation"
            | "array_table"
            | "class"
            | "const"
            | "const_constructor"
            | "constructor"
            | "delegate"
            | "enum"
            | "extension"
            | "extension_type"
            | "factory"
            | "factory_redirect"
            | "getter"
            | "impl"
            | "init"
            | "interface"
            | "key"
            | "macro"
            | "method"
            | "method_decl"
            | "mixin"
            | "mod"
            | "module"
            | "namespace"
            | "object"
            | "property"
            | "protocol"
            | "record"
            | "setter"
            | "singleton_method"
            | "static"
            | "struct"
            | "table"
            | "template"
            | "test"
            | "trait"
            | "type"
            | "typealias"
            | "typedef"
            | "using"
            | "var"
    )
}

/// Parse an injection marker (//lang).
fn parse_injection_marker(input: &str) -> IResult<&str, Segment> {
    map(preceded(tag("//"), parse_identifier), |lang| {
        Segment::Injection(lang.to_string())
    })
    .parse(input)
}

/// Parse a quoted string.
fn parse_quoted_string(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(many0(parse_escaped_char), |chars| {
            chars.into_iter().collect()
        }),
        char('"'),
    )
    .parse(input)
}

/// Parse a single character or escaped sequence inside a quoted string.
fn parse_escaped_char(input: &str) -> IResult<&str, char> {
    alt((preceded(char('\\'), one_of("\\\"n:t")), none_of("\""))).parse(input)
}

/// Parse a simple name (unquoted identifier, number, or special values).
fn parse_simple_name(input: &str) -> IResult<&str, &str> {
    recognize(alt((
        parse_identifier,
        parse_number,
        tag("self"),
        tag("Self"),
    )))
    .parse(input)
}

/// Parse an identifier.
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        one_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_"),
        many0(one_of(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_",
        )),
    ))
    .parse(input)
}

/// Parse a number.
fn parse_number(input: &str) -> IResult<&str, &str> {
    digit1(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_fn_path() {
        let path = TreePath::parse("fn::add").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("fn".to_string()),
                Segment::Name("add".to_string(), None)
            ]
        );
    }

    #[test]
    fn parse_class_method_path() {
        let path = TreePath::parse("class::MyClass::fn::do_work").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("class".to_string()),
                Segment::Name("MyClass".to_string(), None),
                Segment::Kind("fn".to_string()),
                Segment::Name("do_work".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_quoted_name_with_spaces() {
        let path = TreePath::parse("test::\"add function\"").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("test".to_string()),
                Segment::Name("add function".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_quoted_name_with_colons() {
        let path = TreePath::parse("fn::\"foo::bar\"").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("fn".to_string()),
                Segment::Name("foo::bar".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_escaped_quote() {
        let path = TreePath::parse("test::\"with \\\"quote\\\"\"").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("test".to_string()),
                Segment::Name("with \"quote\"".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_injection_marker() {
        // Injection appended to preceding segment (canonical M9 syntax)
        let path = TreePath::parse("key::run//bash::fn::setup").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("run".to_string(), None),
                Segment::Injection("bash".to_string()),
                Segment::Kind("fn".to_string()),
                Segment::Name("setup".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_injection_marker_standalone() {
        // Injection as standalone :: separated segment (also accepted)
        let path = TreePath::parse("key::run:://bash::fn::setup").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("run".to_string(), None),
                Segment::Injection("bash".to_string()),
                Segment::Kind("fn".to_string()),
                Segment::Name("setup".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_module_nested_path() {
        let path = TreePath::parse("module::Billing::class::Invoice::fn::total").unwrap();
        assert_eq!(path.segments.len(), 6);
    }

    #[test]
    fn parse_zig_struct_namespace() {
        let path = TreePath::parse("struct::Point::fn::new").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("struct".to_string()),
                Segment::Name("Point".to_string(), None),
                Segment::Kind("fn".to_string()),
                Segment::Name("new".to_string(), None),
            ]
        );
    }

    #[test]
    fn serialize_simple_name() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("fn".to_string()),
                Segment::Name("add".to_string(), None),
            ],
        };
        assert_eq!(path.serialize(), "fn::add");
    }

    #[test]
    fn serialize_name_with_spaces() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("test".to_string()),
                Segment::Name("add function".to_string(), None),
            ],
        };
        assert_eq!(path.serialize(), "test::\"add function\"");
    }

    #[test]
    fn serialize_name_with_double_colons() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("fn".to_string()),
                Segment::Name("foo::bar".to_string(), None),
            ],
        };
        assert_eq!(path.serialize(), "fn::\"foo::bar\"");
    }

    #[test]
    fn serialize_name_with_quote() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("test".to_string()),
                Segment::Name("with \"quote\"".to_string(), None),
            ],
        };
        assert_eq!(path.serialize(), "test::\"with \\\"quote\\\"\"");
    }

    #[test]
    fn roundtrip_simple_path() {
        let original = "class::MyClass::fn::method";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_complex_path() {
        let original = "test::\"add function\"";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_with_escapes() {
        let original = "test::\"with \\\"quote\\\"\"";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_injection_canonical() {
        let original = "key::run//bash::fn::setup";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn standalone_injection_serializes_canonical() {
        // Standalone form (with ::) normalizes to canonical (without ::)
        let path = TreePath::parse("key::run:://bash::fn::setup").unwrap();
        assert_eq!(path.serialize(), "key::run//bash::fn::setup");
    }

    // --- Array index tests ---

    #[test]
    fn parse_simple_index() {
        let path = TreePath::parse("key::specs[2]").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("specs".to_string(), Some(2)),
            ]
        );
    }

    #[test]
    fn parse_nested_index() {
        let path = TreePath::parse("key::specs[2]::key::item").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("specs".to_string(), Some(2)),
                Segment::Kind("key".to_string()),
                Segment::Name("item".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_index_zero() {
        let path = TreePath::parse("key::steps[0]::key::run").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("steps".to_string(), Some(0)),
                Segment::Kind("key".to_string()),
                Segment::Name("run".to_string(), None),
            ]
        );
    }

    #[test]
    fn parse_quoted_name_with_index() {
        let path = TreePath::parse("key::\"my key\"[3]").unwrap();
        assert_eq!(
            path.segments,
            vec![
                Segment::Kind("key".to_string()),
                Segment::Name("my key".to_string(), Some(3)),
            ]
        );
    }

    #[test]
    fn serialize_with_index() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("key".to_string()),
                Segment::Name("specs".to_string(), Some(12)),
            ],
        };
        assert_eq!(path.serialize(), "key::specs[12]");
    }

    #[test]
    fn serialize_nested_with_index() {
        let path = TreePath {
            segments: vec![
                Segment::Kind("key".to_string()),
                Segment::Name("specs".to_string(), Some(2)),
                Segment::Kind("key".to_string()),
                Segment::Name("item".to_string(), None),
            ],
        };
        assert_eq!(path.serialize(), "key::specs[2]::key::item");
    }

    #[test]
    fn roundtrip_with_index() {
        let original = "key::specs[12]::key::item";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_quoted_with_index() {
        let original = "key::\"my key\"[3]::key::value";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_multiple_indices() {
        let original = "key::jobs[0]::key::steps[5]::key::run";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }
}
