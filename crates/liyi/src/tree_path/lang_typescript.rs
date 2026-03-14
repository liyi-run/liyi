use super::lang_javascript::js_has_doc_comment;

// TypeScript language configuration.
declare_language! {
    /// TypeScript language configuration.
    pub(super) static CONFIG {
        ts_language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        extensions: ["ts", "mts", "cts"],
        kind_map: [
            ("fn", "function_declaration"),
            ("class", "class_declaration"),
            ("method", "method_definition"),
            ("interface", "interface_declaration"),
            ("type", "type_alias_declaration"),
            ("enum", "enum_declaration"),
        ],
        name_field: "name",
        name_overrides: [],
        body_fields: ["body"],
        custom_name: None,
        doc_comment_detector: Some(js_has_doc_comment),
        transparent_kinds: [],
    }
}

// TSX language configuration.
declare_language! {
    /// TSX language configuration.
    pub(super) static TSX_CONFIG {
        ts_language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        extensions: ["tsx"],
        kind_map: [
            ("fn", "function_declaration"),
            ("class", "class_declaration"),
            ("method", "method_definition"),
            ("interface", "interface_declaration"),
            ("type", "type_alias_declaration"),
            ("enum", "enum_declaration"),
        ],
        name_field: "name",
        name_overrides: [],
        body_fields: ["body"],
        custom_name: None,
        doc_comment_detector: Some(js_has_doc_comment),
        transparent_kinds: [],
    }
}

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

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
        let span = resolve_tree_path(SAMPLE_TS, "interface.User", Language::TypeScript);
        assert!(span.is_some(), "should resolve interface.User");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TS.lines().collect();
        assert!(
            lines[start - 1].contains("interface User"),
            "span should point to User interface"
        );
    }

    #[test]
    fn resolve_ts_type_alias() {
        let span = resolve_tree_path(SAMPLE_TS, "type.UserId", Language::TypeScript);
        assert!(span.is_some(), "should resolve type.UserId");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TS.lines().collect();
        assert!(
            lines[start - 1].contains("type UserId"),
            "span should point to UserId type alias"
        );
    }

    #[test]
    fn resolve_ts_enum() {
        let span = resolve_tree_path(SAMPLE_TS, "enum.UserRole", Language::TypeScript);
        assert!(span.is_some(), "should resolve enum.UserRole");
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
            "class.UserService::method.findById",
            Language::TypeScript,
        );
        assert!(
            span.is_some(),
            "should resolve class.UserService::method.findById"
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
        assert_eq!(path, "interface.User");
    }

    #[test]
    fn roundtrip_ts() {
        let resolved_span =
            resolve_tree_path(SAMPLE_TS, "enum.UserRole", Language::TypeScript).unwrap();

        let computed_path = compute_tree_path(SAMPLE_TS, resolved_span, Language::TypeScript);
        assert_eq!(computed_path, "enum.UserRole");

        let re_resolved =
            resolve_tree_path(SAMPLE_TS, &computed_path, Language::TypeScript).unwrap();
        assert_eq!(re_resolved, resolved_span);
    }

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
        let span = resolve_tree_path(SAMPLE_TSX, "fn.Counter", Language::Tsx);
        assert!(span.is_some(), "should resolve fn.Counter in TSX");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TSX.lines().collect();
        assert!(
            lines[start - 1].contains("function Counter"),
            "span should point to Counter function"
        );
    }

    #[test]
    fn resolve_tsx_class() {
        let span = resolve_tree_path(SAMPLE_TSX, "class.Container", Language::Tsx);
        assert!(span.is_some(), "should resolve class.Container in TSX");
        let [start, _end] = span.unwrap();
        let lines: Vec<&str> = SAMPLE_TSX.lines().collect();
        assert!(
            lines[start - 1].contains("class Container"),
            "span should point to Container class"
        );
    }

    #[test]
    fn resolve_tsx_interface() {
        let span = resolve_tree_path(SAMPLE_TSX, "interface.Props", Language::Tsx);
        assert!(span.is_some(), "should resolve interface.Props in TSX");
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
