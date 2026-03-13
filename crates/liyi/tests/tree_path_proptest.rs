// Property-based roundtrip tests for tree_path parser:
// parse(serialize(path)) == path

use liyi::tree_path::parser::{Pair, TreePath, serialize_name};
use proptest::prelude::*;

/// Strategy for generating a simple identifier (unquoted name).
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,20}"
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
        "array_table",
        "class",
        "struct",
        "enum",
        "trait",
        "impl",
        "key",
        "mod",
        "const",
        "table",
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

/// Strategy for a Pair (kind.name).
fn arb_pair() -> impl Strategy<Value = Pair> {
    (arb_kind(), arb_name()).prop_map(|(k, n)| Pair {
        kind: k,
        name: n,
        index: None,
        injection: None,
    })
}

/// Strategy for a Pair with optional index.
fn arb_pair_indexed() -> impl Strategy<Value = Pair> {
    (arb_kind(), arb_name(), prop::option::of(0..100usize)).prop_map(|(k, n, idx)| Pair {
        kind: k,
        name: n,
        index: idx,
        injection: None,
    })
}

/// Strategy for a complete TreePath (1–4 pairs, no injection).
fn arb_tree_path() -> impl Strategy<Value = TreePath> {
    prop::collection::vec(arb_pair(), 1..=4).prop_map(|pairs| TreePath { pairs })
}

/// Strategy for a complete TreePath with optional indices (1–4 pairs).
fn arb_tree_path_indexed() -> impl Strategy<Value = TreePath> {
    prop::collection::vec(arb_pair_indexed(), 1..=4).prop_map(|pairs| TreePath { pairs })
}

proptest! {
    /// Roundtrip: serialize then parse should yield the same pairs.
    #[test]
    fn roundtrip_serialize_parse(path in arb_tree_path()) {
        let serialized = path.serialize();
        let reparsed = TreePath::parse(&serialized)
            .unwrap_or_else(|e| panic!("Failed to parse serialized path {serialized:?}: {e}"));

        let original: Vec<(&str, &str)> = path.pairs.iter()
            .map(|p| (p.kind.as_str(), p.name.as_str()))
            .collect();
        let reparsed_v: Vec<(&str, &str)> = reparsed.pairs.iter()
            .map(|p| (p.kind.as_str(), p.name.as_str()))
            .collect();

        prop_assert_eq!(original, reparsed_v,
            "Roundtrip failed for serialized form: {:?}", serialized);
    }

    /// Roundtrip for paths with optional array indices.
    #[test]
    fn roundtrip_serialize_parse_indexed(path in arb_tree_path_indexed()) {
        let serialized = path.serialize();
        let reparsed = TreePath::parse(&serialized)
            .unwrap_or_else(|e| panic!("Failed to parse indexed path {serialized:?}: {e}"));

        let original: Vec<(&str, &str, Option<usize>)> = path.pairs.iter()
            .map(|p| (p.kind.as_str(), p.name.as_str(), p.index))
            .collect();
        let reparsed_v: Vec<(&str, &str, Option<usize>)> = reparsed.pairs.iter()
            .map(|p| (p.kind.as_str(), p.name.as_str(), p.index))
            .collect();

        prop_assert_eq!(original, reparsed_v,
            "Indexed roundtrip failed for serialized form: {:?}", serialized);
    }

    /// serialize_name roundtrip: the serialized form should parse back to the original name.
    #[test]
    fn roundtrip_serialize_name(name in arb_name()) {
        let serialized = serialize_name(&name);
        // Build a full path "fn.<serialized_name>" to test via the parser
        let path_str = format!("fn.{serialized}");
        let parsed = TreePath::parse(&path_str)
            .unwrap_or_else(|e| panic!("Failed to parse fn.{serialized}: {e}"));
        let parsed_name = &parsed.pairs[0].name;
        prop_assert_eq!(&name, parsed_name,
            "Name roundtrip failed for serialized form: {:?}", serialized);
    }
}
