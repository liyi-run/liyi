use super::LanguageConfig;

/// C# language configuration.
pub(super) static CONFIG: LanguageConfig = LanguageConfig {
    ts_language: || tree_sitter_c_sharp::LANGUAGE.into(),
    extensions: &["cs"],
    kind_map: &[
        ("fn", "method_declaration"),
        ("class", "class_declaration"),
        ("interface", "interface_declaration"),
        ("enum", "enum_declaration"),
        ("struct", "struct_declaration"),
        ("namespace", "namespace_declaration"),
        ("constructor", "constructor_declaration"),
        ("property", "property_declaration"),
        ("record", "record_declaration"),
        ("delegate", "delegate_declaration"),
    ],
    name_field: "name",
    name_overrides: &[],
    body_fields: &["body"],
    custom_name: None,
};

#[cfg(test)]
mod tests {
    use crate::tree_path::*;
    use std::path::Path;

    const SAMPLE_CSHARP: &str = r#"namespace MyApp {

class Calculator {
    public int Add(int a, int b) {
        return a + b;
    }

    public string Name { get; set; }

    public Calculator() {}
}

interface IComputable {
    int Compute(int x);
}

enum Direction {
    North, South, East, West
}

struct Vector {
    public int X;
    public int Y;
}

record Person(string Name, int Age);

}
"#;

    #[test]
    fn resolve_csharp_class() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::class::Calculator",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve namespace::MyApp::class::Calculator");
    }

    #[test]
    fn resolve_csharp_method() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::class::Calculator::fn::Add",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve method in class in namespace");
    }

    #[test]
    fn resolve_csharp_property() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::class::Calculator::property::Name",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve property::Name");
    }

    #[test]
    fn resolve_csharp_interface() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::interface::IComputable",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve interface::IComputable");
    }

    #[test]
    fn resolve_csharp_struct() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::struct::Vector",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve struct::Vector");
    }

    #[test]
    fn resolve_csharp_enum() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::enum::Direction",
            Language::CSharp,
        );
        assert!(span.is_some(), "should resolve enum::Direction");
    }

    #[test]
    fn roundtrip_csharp() {
        let span = resolve_tree_path(
            SAMPLE_CSHARP,
            "namespace::MyApp::class::Calculator::fn::Add",
            Language::CSharp,
        )
        .unwrap();
        let path = compute_tree_path(SAMPLE_CSHARP, span, Language::CSharp);
        assert_eq!(path, "namespace::MyApp::class::Calculator::fn::Add");
    }

    #[test]
    fn detect_csharp_extension() {
        assert_eq!(
            detect_language(Path::new("Program.cs")),
            Some(Language::CSharp)
        );
    }
}
