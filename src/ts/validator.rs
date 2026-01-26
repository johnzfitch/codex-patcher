use crate::ts::errors::TreeSitterError;
use crate::ts::parser::{ErrorNode, RustParser};

/// Validate that Rust source code has no syntax errors.
///
/// Returns Ok(()) if the code parses without ERROR nodes.
pub fn validate_syntax(source: &str) -> Result<(), TreeSitterError> {
    let mut parser = RustParser::new()?;
    let parsed = parser.parse_with_source(source)?;

    let errors = parsed.error_nodes();
    match errors.len() {
        0 => Ok(()),
        1 => Err(TreeSitterError::SyntaxError {
            byte_start: errors[0].byte_start,
            byte_end: errors[0].byte_end,
        }),
        n => Err(TreeSitterError::MultipleSyntaxErrors { count: n }),
    }
}

/// Validate that an edit doesn't introduce syntax errors.
///
/// Takes the original source and the proposed edit, applies it virtually,
/// and checks for new ERROR nodes.
pub fn validate_edit(
    source: &str,
    byte_start: usize,
    byte_end: usize,
    new_text: &str,
) -> Result<(), TreeSitterError> {
    // Build the new source
    let mut new_source = String::with_capacity(source.len() + new_text.len() - (byte_end - byte_start));
    new_source.push_str(&source[..byte_start]);
    new_source.push_str(new_text);
    new_source.push_str(&source[byte_end..]);

    // Count errors before and after
    let mut parser = RustParser::new()?;

    let original_parsed = parser.parse_with_source(source)?;
    let original_errors = original_parsed.error_nodes();

    let new_parsed = parser.parse_with_source(&new_source)?;
    let new_errors = new_parsed.error_nodes();

    // Find errors that weren't in the original
    let introduced_errors: Vec<&ErrorNode> = new_errors
        .iter()
        .filter(|e| {
            // Error is "new" if it's not in the original error set
            // (comparing by position is imperfect but reasonable)
            !original_errors
                .iter()
                .any(|o| o.byte_start == e.byte_start && o.byte_end == e.byte_end)
        })
        .collect();

    match introduced_errors.len() {
        0 => Ok(()),
        1 => Err(TreeSitterError::SyntaxError {
            byte_start: introduced_errors[0].byte_start,
            byte_end: introduced_errors[0].byte_end,
        }),
        n => Err(TreeSitterError::MultipleSyntaxErrors { count: n }),
    }
}

/// Check if a code snippet is valid as a specific syntactic category.
pub fn validate_snippet(snippet: &str, category: SnippetCategory) -> Result<(), TreeSitterError> {
    let wrapped = match category {
        SnippetCategory::Item => snippet.to_string(),
        SnippetCategory::Statement => format!("fn __wrapper__() {{ {} }}", snippet),
        SnippetCategory::Expression => format!("fn __wrapper__() {{ let _ = {}; }}", snippet),
        SnippetCategory::FunctionBody => format!("fn __wrapper__() {}", snippet),
    };

    validate_syntax(&wrapped)
}

/// Category of code snippet for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetCategory {
    /// A top-level item (fn, struct, impl, etc.)
    Item,
    /// A statement (let, expression with semicolon, etc.)
    Statement,
    /// An expression
    Expression,
    /// A function body (including braces)
    FunctionBody,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_syntax() {
        let source = r#"
fn main() {
    println!("hello");
}
"#;
        assert!(validate_syntax(source).is_ok());
    }

    #[test]
    fn validate_invalid_syntax() {
        let source = "fn main( { }";
        let result = validate_syntax(source);
        assert!(result.is_err());
    }

    #[test]
    fn validate_edit_introduces_error() {
        let source = "fn foo() { let x = 1; }";
        // Replace valid code with invalid code
        let result = validate_edit(source, 11, 21, "let x =");
        assert!(result.is_err());
    }

    #[test]
    fn validate_edit_no_new_errors() {
        let source = "fn foo() { let x = 1; }";
        // Replace valid code with different valid code
        let result = validate_edit(source, 11, 21, "let y = 2;");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_edit_on_already_broken_code() {
        let source = "fn foo( { let x = 1; }"; // Already broken
        // Make a change that doesn't add new errors
        let result = validate_edit(source, 10, 20, "let y = 2;");
        assert!(result.is_ok()); // Should be ok since we didn't add NEW errors
    }

    #[test]
    fn validate_item_snippet() {
        assert!(validate_snippet("fn test() {}", SnippetCategory::Item).is_ok());
        assert!(validate_snippet("struct Foo;", SnippetCategory::Item).is_ok());
        assert!(validate_snippet("fn test(", SnippetCategory::Item).is_err());
    }

    #[test]
    fn validate_expression_snippet() {
        assert!(validate_snippet("1 + 2", SnippetCategory::Expression).is_ok());
        assert!(validate_snippet("foo.bar()", SnippetCategory::Expression).is_ok());
        assert!(validate_snippet("1 +", SnippetCategory::Expression).is_err());
    }

    #[test]
    fn validate_function_body_snippet() {
        assert!(validate_snippet("{ let x = 1; x }", SnippetCategory::FunctionBody).is_ok());
        assert!(validate_snippet("{ x }", SnippetCategory::FunctionBody).is_ok());
    }
}
