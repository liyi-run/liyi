//! tree_path parser — formal grammar implementation using nom.
//!
//! This module implements the tree_path grammar spec (v0.3) from
//! `docs/liyi-01x-roadmap.md` Appendix A.
//!
//! v0.3 syntax: `kind.name::kind.name` — the `.` binds a kind shorthand
//! to its name within a single pair, while `::` separates successive pairs
//! (descent).  This eliminates the even-pair positional invariant and the
//! `is_common_kind` heuristic of v0.2.

use nom::{
    IResult, Parser as _,
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1, none_of, one_of},
    combinator::{map, recognize},
    multi::many0,
    sequence::{delimited, pair, preceded},
};

/// A kind.name pair in a tree_path.
///
/// Each pair binds a kind shorthand (e.g., `fn`, `class`, `key`) to a name
/// (e.g., `add`, `MyClass`).  The optional index selects a positional child
/// in data-file arrays (e.g., `specs[2]`).  The optional injection marker
/// switches to an embedded language (e.g., `run//bash`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// Kind shorthand (e.g., "fn", "class", "struct", "key").
    pub kind: String,
    /// Item name (e.g., "add", "MyClass", "add function").
    pub name: String,
    /// Optional 0-based positional child selector for data-file arrays.
    pub index: Option<usize>,
    /// Optional injection marker (e.g., "bash") for M9 language injection.
    pub injection: Option<String>,
}

/// Parsed tree_path representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreePath {
    pub pairs: Vec<Pair>,
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
        for (i, p) in self.pairs.iter().enumerate() {
            if i > 0 {
                out.push_str("::");
            }
            out.push_str(&p.kind);
            out.push('.');
            out.push_str(&serialize_name(&p.name));
            if let Some(idx) = p.index {
                out.push('[');
                out.push_str(&idx.to_string());
                out.push(']');
            }
            if let Some(ref lang) = p.injection {
                out.push_str("//");
                out.push_str(lang);
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
        || name.contains('.')
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

/// Parse a complete tree_path: `pair ("::" pair)*`.
fn parse_tree_path(input: &str) -> IResult<&str, TreePath> {
    let (input, first) = parse_pair(input)?;
    let (input, rest) = many0(preceded(tag("::"), parse_pair)).parse(input)?;
    let mut pairs = vec![first];
    pairs.extend(rest);
    Ok((input, TreePath { pairs }))
}

/// Parse a single `kind.name[index?]//injection?` pair.
fn parse_pair(input: &str) -> IResult<&str, Pair> {
    // Kind: always a simple identifier
    let (input, kind) = parse_identifier(input)?;
    // '.' separator between kind and name
    let (input, _) = char('.')(input)?;
    // Name: quoted string or simple name
    let (input, (name, idx)) = parse_name_part(input)?;
    // Optional injection marker
    let (input, injection) = parse_optional_injection(input)?;
    Ok((
        input,
        Pair {
            kind: kind.to_string(),
            name,
            index: idx,
            injection,
        },
    ))
}

/// Parse the name part of a pair: either a quoted string or a simple name,
/// each followed by an optional `[N]` index.
fn parse_name_part(input: &str) -> IResult<&str, (String, Option<usize>)> {
    // Try quoted string first
    if input.starts_with('"') {
        let (input, name) = parse_quoted_string(input)?;
        let (input, idx) = parse_optional_index(input)?;
        return Ok((input, (name, idx)));
    }
    // Otherwise simple name
    let (input, name) = parse_simple_name(input)?;
    let (input, idx) = parse_optional_index(input)?;
    Ok((input, (name.to_string(), idx)))
}

/// Parse an optional `//lang` injection marker.
fn parse_optional_injection(input: &str) -> IResult<&str, Option<String>> {
    if input.starts_with("//") {
        let (input, _) = tag("//")(input)?;
        let (input, lang) = parse_identifier(input)?;
        Ok((input, Some(lang.to_string())))
    } else {
        Ok((input, None))
    }
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

    fn p(kind: &str, name: &str) -> Pair {
        Pair {
            kind: kind.to_string(),
            name: name.to_string(),
            index: None,
            injection: None,
        }
    }

    fn pi(kind: &str, name: &str, idx: usize) -> Pair {
        Pair {
            kind: kind.to_string(),
            name: name.to_string(),
            index: Some(idx),
            injection: None,
        }
    }

    fn pinj(kind: &str, name: &str, lang: &str) -> Pair {
        Pair {
            kind: kind.to_string(),
            name: name.to_string(),
            index: None,
            injection: Some(lang.to_string()),
        }
    }

    #[test]
    fn parse_simple_fn_path() {
        let path = TreePath::parse("fn.add").unwrap();
        assert_eq!(path.pairs, vec![p("fn", "add")]);
    }

    #[test]
    fn parse_class_method_path() {
        let path = TreePath::parse("class.MyClass::fn.do_work").unwrap();
        assert_eq!(path.pairs, vec![p("class", "MyClass"), p("fn", "do_work")]);
    }

    #[test]
    fn parse_quoted_name_with_spaces() {
        let path = TreePath::parse("test.\"add function\"").unwrap();
        assert_eq!(path.pairs, vec![p("test", "add function")]);
    }

    #[test]
    fn parse_quoted_name_with_colons() {
        let path = TreePath::parse("fn.\"foo::bar\"").unwrap();
        assert_eq!(path.pairs, vec![p("fn", "foo::bar")]);
    }

    #[test]
    fn parse_escaped_quote() {
        let path = TreePath::parse("test.\"with \\\"quote\\\"\"").unwrap();
        assert_eq!(path.pairs, vec![p("test", "with \"quote\"")]);
    }

    #[test]
    fn parse_injection_marker() {
        let path = TreePath::parse("key.run//bash::fn.setup").unwrap();
        assert_eq!(
            path.pairs,
            vec![pinj("key", "run", "bash"), p("fn", "setup")]
        );
    }

    #[test]
    fn parse_module_nested_path() {
        let path = TreePath::parse("module.Billing::class.Invoice::fn.total").unwrap();
        assert_eq!(path.pairs.len(), 3);
    }

    #[test]
    fn parse_struct_namespace() {
        let path = TreePath::parse("struct.Point::fn.new").unwrap();
        assert_eq!(path.pairs, vec![p("struct", "Point"), p("fn", "new")]);
    }

    #[test]
    fn parse_quoted_name_with_dot() {
        let path = TreePath::parse("key.\"abc.kubernetes.io\"").unwrap();
        assert_eq!(path.pairs, vec![p("key", "abc.kubernetes.io")]);
    }

    #[test]
    fn serialize_simple_name() {
        let path = TreePath {
            pairs: vec![p("fn", "add")],
        };
        assert_eq!(path.serialize(), "fn.add");
    }

    #[test]
    fn serialize_name_with_spaces() {
        let path = TreePath {
            pairs: vec![p("test", "add function")],
        };
        assert_eq!(path.serialize(), "test.\"add function\"");
    }

    #[test]
    fn serialize_name_with_double_colons() {
        let path = TreePath {
            pairs: vec![p("fn", "foo::bar")],
        };
        assert_eq!(path.serialize(), "fn.\"foo::bar\"");
    }

    #[test]
    fn serialize_name_with_quote() {
        let path = TreePath {
            pairs: vec![p("test", "with \"quote\"")],
        };
        assert_eq!(path.serialize(), "test.\"with \\\"quote\\\"\"");
    }

    #[test]
    fn serialize_name_with_dot() {
        let path = TreePath {
            pairs: vec![p("key", "abc.kubernetes.io")],
        };
        assert_eq!(path.serialize(), "key.\"abc.kubernetes.io\"");
    }

    #[test]
    fn roundtrip_simple_path() {
        let original = "class.MyClass::fn.method";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_complex_path() {
        let original = "test.\"add function\"";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_with_escapes() {
        let original = "test.\"with \\\"quote\\\"\"";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_injection_canonical() {
        let original = "key.run//bash::fn.setup";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    // --- Array index tests ---

    #[test]
    fn parse_simple_index() {
        let path = TreePath::parse("key.specs[2]").unwrap();
        assert_eq!(path.pairs, vec![pi("key", "specs", 2)]);
    }

    #[test]
    fn parse_nested_index() {
        let path = TreePath::parse("key.specs[2]::key.item").unwrap();
        assert_eq!(path.pairs, vec![pi("key", "specs", 2), p("key", "item")]);
    }

    #[test]
    fn parse_index_zero() {
        let path = TreePath::parse("key.steps[0]::key.run").unwrap();
        assert_eq!(path.pairs, vec![pi("key", "steps", 0), p("key", "run")]);
    }

    #[test]
    fn parse_quoted_name_with_index() {
        let path = TreePath::parse("key.\"my key\"[3]").unwrap();
        assert_eq!(path.pairs, vec![pi("key", "my key", 3)]);
    }

    #[test]
    fn serialize_with_index() {
        let path = TreePath {
            pairs: vec![pi("key", "specs", 12)],
        };
        assert_eq!(path.serialize(), "key.specs[12]");
    }

    #[test]
    fn serialize_nested_with_index() {
        let path = TreePath {
            pairs: vec![pi("key", "specs", 2), p("key", "item")],
        };
        assert_eq!(path.serialize(), "key.specs[2]::key.item");
    }

    #[test]
    fn roundtrip_with_index() {
        let original = "key.specs[12]::key.item";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_quoted_with_index() {
        let original = "key.\"my key\"[3]::key.value";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }

    #[test]
    fn roundtrip_multiple_indices() {
        let original = "key.jobs[0]::key.steps[5]::key.run";
        let path = TreePath::parse(original).unwrap();
        assert_eq!(path.serialize(), original);
    }
}
