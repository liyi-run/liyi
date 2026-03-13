// Property-based roundtrip tests for tree_path parser:
// parse(serialize(path)) == path

use liyi::tree_path::parser::{Segment, TreePath, serialize_name};
use proptest::prelude::*;

/// Strategy for generating a simple identifier (unquoted name).
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,20}".prop_filter("not a kind keyword", |s| !is_kind(s))
}

/// Strategy for a name that requires quoting (contains spaces, colons, dots, etc.).
fn arb_complex_name() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            "[a-zA-Z0-9_]".prop_map(|s| s),
            Just(" ".to_string()),
            Just(".".to_string()),
            Just("::".to_string()),
            Just("(".to_string()),
            Just(")".to_string()),
            Just("*".to_string()),
        ],
        1..=10,
    )
    .prop_map(|parts| parts.join(""))
    .prop_filter("must not be empty", |s| !s.is_empty())
}

/// Strategy for any name (simple or complex).
fn arb_name() -> impl Strategy<Value = String> {
    prop_oneof![arb_identifier(), arb_complex_name(),]
}

/// Strategy for a kind shorthand.
fn arb_kind() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "fn",
        "class",
        "struct",
        "enum",
        "trait",
        "impl",
        "mod",
        "const",
        "type",
        "test",
        "method",
        "namespace",
        "interface",
        "protocol",
        "module",
        "var",
        "property",
        "object",
        "typedef",
        "init",
        "singleton_method",
        "record",
        "constructor",
    ])
    .prop_map(|s| s.to_string())
}

/// Strategy for a (Kind, Name) pair.
fn arb_segment_pair() -> impl Strategy<Value = (Segment, Segment)> {
    (arb_kind(), arb_name()).prop_map(|(k, n)| (Segment::Kind(k), Segment::Name(n)))
}

/// Strategy for a complete TreePath (1–4 segment pairs, no injection).
fn arb_tree_path() -> impl Strategy<Value = TreePath> {
    prop::collection::vec(arb_segment_pair(), 1..=4).prop_map(|pairs| {
        let segments: Vec<Segment> = pairs.into_iter().flat_map(|(k, n)| vec![k, n]).collect();
        TreePath { segments }
    })
}

/// Check if a string is a kind keyword in our expanded list.
fn is_kind(s: &str) -> bool {
    matches!(
        s,
        "fn" | "annotation"
            | "class"
            | "const"
            | "constructor"
            | "delegate"
            | "enum"
            | "impl"
            | "init"
            | "interface"
            | "macro"
            | "method"
            | "method_decl"
            | "mod"
            | "module"
            | "namespace"
            | "object"
            | "property"
            | "protocol"
            | "record"
            | "singleton_method"
            | "static"
            | "struct"
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

proptest! {
    /// Roundtrip: serialize then parse should yield the same segments.
    #[test]
    fn roundtrip_serialize_parse(path in arb_tree_path()) {
        let serialized = path.serialize();
        let reparsed = TreePath::parse(&serialized)
            .unwrap_or_else(|e| panic!("Failed to parse serialized path {serialized:?}: {e}"));

        // Extract (kind, name) pairs from both, ignoring Kind/Name classification
        let original_pairs: Vec<(&str, &str)> = path.segments.chunks(2).map(|pair| {
            let k = match &pair[0] { Segment::Kind(s) | Segment::Name(s) => s.as_str(), _ => panic!() };
            let n = match &pair[1] { Segment::Kind(s) | Segment::Name(s) => s.as_str(), _ => panic!() };
            (k, n)
        }).collect();

        let reparsed_pairs: Vec<(&str, &str)> = reparsed.segments.chunks(2).map(|pair| {
            let k = match &pair[0] { Segment::Kind(s) | Segment::Name(s) => s.as_str(), _ => panic!() };
            let n = match &pair[1] { Segment::Kind(s) | Segment::Name(s) => s.as_str(), _ => panic!() };
            (k, n)
        }).collect();

        prop_assert_eq!(original_pairs, reparsed_pairs,
            "Roundtrip failed for serialized form: {:?}", serialized);
    }

    /// serialize_name roundtrip: the serialized form should parse back to the original name.
    #[test]
    fn roundtrip_serialize_name(name in arb_name()) {
        let serialized = serialize_name(&name);
        // Build a full path "fn::<serialized_name>" to test via the parser
        let path_str = format!("fn::{serialized}");
        let parsed = TreePath::parse(&path_str)
            .unwrap_or_else(|e| panic!("Failed to parse fn::{serialized}: {e}"));
        let parsed_name = match &parsed.segments[1] {
            Segment::Kind(s) | Segment::Name(s) => s.clone(),
            _ => panic!("Expected Kind or Name segment"),
        };
        prop_assert_eq!(name, parsed_name,
            "Name roundtrip failed for serialized form: {:?}", serialized);
    }
}
