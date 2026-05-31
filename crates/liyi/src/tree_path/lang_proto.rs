use super::LanguageConfig;

use tree_sitter::Node;

/// Find the first direct child with a given kind.
fn find_child_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

/// Custom name extraction for Protobuf nodes.
///
/// The proto grammar does not expose a `name` field on item nodes.  Instead
/// the name is carried by a dedicated child node:
///
/// - `message` → `message_name`
/// - `enum` → `enum_name`
/// - `service` → `service_name`
/// - `rpc` → `rpc_name`
/// - `field` / `enum_field` / `map_field` / `oneof` / `oneof_field` → the
///   first `identifier` child (the declaration's name; the `type` precedes it
///   as a distinct `type` node and is therefore skipped).
fn proto_node_name(node: &Node, source: &str) -> Option<String> {
    let name_kind = match node.kind() {
        "message" => "message_name",
        "enum" => "enum_name",
        "service" => "service_name",
        "rpc" => "rpc_name",
        "field" | "enum_field" | "map_field" | "oneof" | "oneof_field" => "identifier",
        _ => return None,
    };
    let name_node = find_child_by_kind(node, name_kind)?;
    Some(source[name_node.byte_range()].to_string())
}

/// Detect proto doc comments (`//` or `/* */` immediately preceding the item).
fn proto_has_doc_comment(node: &Node, source: &str) -> bool {
    let _ = source;
    matches!(node.prev_sibling().map(|s| s.kind()), Some("comment"))
}

/// Protobuf language configuration.
///
/// Item nodes act as their own body containers (the `"."` sentinel in
/// `body_fields`).  Messages and enums wrap their members in a
/// `message_body` / `enum_body` node, so those kinds are declared
/// `transparent` to let the resolver descend into nested fields, messages,
/// enums, and oneofs.  Services and oneofs hold their members (`rpc`,
/// `oneof_field`) as direct children, so no transparent wrapper is needed.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_proto::LANGUAGE.into(),
    extensions: &["proto"],
    kind_map: &[
        ("message", "message"),
        ("enum", "enum"),
        ("service", "service"),
        ("rpc", "rpc"),
        ("field", "field"),
        ("map", "map_field"),
        ("oneof", "oneof"),
        ("oneof_field", "oneof_field"),
        ("enum_field", "enum_field"),
    ],
    name_field: "",
    name_overrides: &[],
    body_fields: &["."],
    custom_name: Some(proto_node_name),
    doc_comment_detector: Some(proto_has_doc_comment),
    transparent_kinds: &["message_body", "enum_body"],
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_PROTO: &str = r#"syntax = "proto3";

package example.v1;

// User represents an account holder.
message User {
  string id = 1;
  string display_name = 2;

  // Address is a nested message.
  message Address {
    string street = 1;
    string city = 2;
  }

  oneof contact {
    string email = 3;
    string phone = 4;
  }

  map<string, string> labels = 5;
}

// Status enumerates lifecycle states.
enum Status {
  STATUS_UNKNOWN = 0;
  STATUS_ACTIVE = 1;
}

// UserService manages users.
service UserService {
  // GetUser fetches a user by id.
  rpc GetUser(GetUserRequest) returns (User);
  rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);
}
"#;

    #[test]
    fn resolve_message() {
        let span = resolve_tree_path(SAMPLE_PROTO, "message.User", Language::Proto);
        assert!(span.is_some(), "should resolve message.User");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("message User"),
            "span should point to message User, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_nested_message() {
        let span = resolve_tree_path(
            SAMPLE_PROTO,
            "message.User::message.Address",
            Language::Proto,
        );
        assert!(span.is_some(), "should resolve nested message.Address");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("message Address"),
            "span should point to nested message Address, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_field() {
        let span = resolve_tree_path(
            SAMPLE_PROTO,
            "message.User::field.display_name",
            Language::Proto,
        );
        assert!(
            span.is_some(),
            "should resolve message.User::field.display_name"
        );
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("display_name"),
            "span should point to display_name field, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_oneof_field() {
        let span = resolve_tree_path(
            SAMPLE_PROTO,
            "message.User::oneof.contact::oneof_field.email",
            Language::Proto,
        );
        assert!(span.is_some(), "should resolve oneof_field.email");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("email"),
            "span should point to email oneof field, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_map_field() {
        let span = resolve_tree_path(SAMPLE_PROTO, "message.User::map.labels", Language::Proto);
        assert!(span.is_some(), "should resolve map.labels");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("labels"),
            "span should point to labels map field, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_enum() {
        let span = resolve_tree_path(SAMPLE_PROTO, "enum.Status", Language::Proto);
        assert!(span.is_some(), "should resolve enum.Status");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("enum Status"),
            "span should point to enum Status, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_enum_field() {
        let span = resolve_tree_path(
            SAMPLE_PROTO,
            "enum.Status::enum_field.STATUS_ACTIVE",
            Language::Proto,
        );
        assert!(span.is_some(), "should resolve enum_field.STATUS_ACTIVE");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("STATUS_ACTIVE"),
            "span should point to STATUS_ACTIVE, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn resolve_service_rpc() {
        let span = resolve_tree_path(
            SAMPLE_PROTO,
            "service.UserService::rpc.GetUser",
            Language::Proto,
        );
        assert!(span.is_some(), "should resolve rpc.GetUser");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_PROTO.lines().collect();
        assert!(
            lines[start - 1].contains("rpc GetUser"),
            "span should point to GetUser rpc, got: {}",
            lines[start - 1]
        );
    }

    #[test]
    fn detect_proto_extension() {
        assert_eq!(
            detect_language(Path::new("api/user.proto")),
            Some(Language::Proto)
        );
    }

    #[test]
    fn roundtrip_compute_resolve() {
        let resolved = resolve_tree_path(
            SAMPLE_PROTO,
            "service.UserService::rpc.GetUser",
            Language::Proto,
        )
        .unwrap();
        let computed = compute_tree_path(SAMPLE_PROTO, resolved, Language::Proto);
        assert_eq!(computed, "service.UserService::rpc.GetUser");
        let re_resolved = resolve_tree_path(SAMPLE_PROTO, &computed, Language::Proto).unwrap();
        assert_eq!(resolved, re_resolved);
    }

    #[test]
    fn discover_proto_items() {
        let items = discover_items(SAMPLE_PROTO, Language::Proto);
        let paths: Vec<&str> = items.iter().map(|i| i.tree_path.as_str()).collect();
        assert!(paths.contains(&"message.User"), "paths: {paths:?}");
        assert!(
            paths.contains(&"message.User::message.Address"),
            "paths: {paths:?}"
        );
        assert!(paths.contains(&"enum.Status"), "paths: {paths:?}");
        assert!(paths.contains(&"service.UserService"), "paths: {paths:?}");
        assert!(
            paths.contains(&"service.UserService::rpc.GetUser"),
            "paths: {paths:?}"
        );
        assert!(
            paths.contains(&"message.User::map.labels"),
            "paths: {paths:?}"
        );
    }

    #[test]
    fn proto_doc_comment_detection() {
        let items = discover_items(SAMPLE_PROTO, Language::Proto);
        let user = items
            .iter()
            .find(|i| i.tree_path == "message.User")
            .expect("message.User discovered");
        assert_eq!(user.has_doc_comment, Some(true));

        let list_users = items
            .iter()
            .find(|i| i.tree_path == "service.UserService::rpc.ListUsers")
            .expect("rpc.ListUsers discovered");
        assert_eq!(list_users.has_doc_comment, Some(false));
    }
}
