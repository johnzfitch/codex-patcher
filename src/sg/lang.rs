//! Rust language support via ast-grep-language.
//!
//! We use the built-in `SupportLang::Rust` from ast-grep-language instead of
//! maintaining our own Language implementation. This handles all the metavar
//! preprocessing and tree-sitter integration automatically.

pub use ast_grep_language::SupportLang;

/// Get the Rust language for ast-grep operations.
pub fn rust() -> SupportLang {
    SupportLang::Rust
}

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_core::AstGrep;

    #[test]
    fn rust_lang_parses() {
        let sg = AstGrep::new("fn main() {}", rust());
        assert_eq!(sg.root().kind(), "source_file");
    }

    #[test]
    fn rust_lang_single_metavar() {
        let sg = AstGrep::new("fn foo() { 42 }", rust());
        let root = sg.root();

        // Exact match
        assert!(root.find("fn foo() { 42 }").is_some(), "exact match");

        // Single metavar for name
        assert!(
            root.find("fn $NAME() { 42 }").is_some(),
            "single metavar for name"
        );

        // Single metavar for body (single expression)
        assert!(
            root.find("fn foo() { $BODY }").is_some(),
            "single metavar for body"
        );

        // Anonymous metavar
        assert!(root.find("fn foo() { $_ }").is_some(), "anonymous metavar");
    }

    #[test]
    fn rust_lang_variadic_metavar() {
        // Multi-statement body requires variadic metavar
        let sg = AstGrep::new("fn foo() { let x = 1; x }", rust());
        let root = sg.root();

        // Anonymous variadic ($$$)
        assert!(root.find("fn foo() { $$$ }").is_some(), "anonymous variadic");

        // Captured variadic ($$$NAME)
        assert!(
            root.find("fn foo() { $$$BODY }").is_some(),
            "captured variadic"
        );

        // Combined with single capture for name
        assert!(
            root.find("fn $NAME() { $$$BODY }").is_some(),
            "name + captured variadic"
        );
    }

    #[test]
    fn rust_lang_function_with_return_type() {
        let sg = AstGrep::new("fn foo() -> i32 { 42 }", rust());
        let root = sg.root();

        assert!(
            root.find("fn $NAME() -> $RET { $$$BODY }").is_some(),
            "function with return type"
        );
    }

    #[test]
    fn rust_lang_method_calls() {
        let sg = AstGrep::new("let a = foo.clone(); let b = bar.to_string();", rust());
        let root = sg.root();

        // Match method calls with $EXPR
        let clone_calls: Vec<_> = root.find_all("$EXPR.clone()").collect();
        assert_eq!(clone_calls.len(), 1, "should find one clone() call");

        let to_string_calls: Vec<_> = root.find_all("$EXPR.to_string()").collect();
        assert_eq!(to_string_calls.len(), 1, "should find one to_string() call");
    }

    #[test]
    fn rust_lang_enum_variants() {
        let sg = AstGrep::new("let x = Foo::Bar; let y = Foo::Baz;", rust());
        let root = sg.root();

        let variants: Vec<_> = root.find_all("Foo::$VARIANT").collect();
        assert_eq!(variants.len(), 2, "should find two Foo variants");
    }

    #[test]
    fn rust_lang_struct_expression() {
        let sg = AstGrep::new(
            "let cfg = Config { name: \"test\".into(), value: 42 };",
            rust(),
        );
        let root = sg.root();

        assert!(
            root.find("Config { $$$FIELDS }").is_some(),
            "struct expression with variadic fields"
        );
    }
}
