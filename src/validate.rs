//! Validation module for ensuring edit safety.
//!
//! This module provides:
//! - Parse validation (tree-sitter ERROR node detection)
//! - syn validation for generated snippets
//! - Selector uniqueness checks
//!
//! # Hard Rules (Never Violate)
//!
//! 1. **Parse validation**: After editing, re-parse with tree-sitter.
//!    If the file has ERROR nodes that weren't there before, roll back.
//! 2. **Selector uniqueness**: If a structural query matches 0 or >1
//!    locations, refuse to edit. No guessing.

use crate::ts::{ParsedSource, RustParser, TreeSitterError};
use std::path::Path;
use thiserror::Error;

/// Validation errors.
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Parse error introduced: found {count} new ERROR nodes")]
    ParseErrorIntroduced {
        count: usize,
        errors: Vec<ErrorLocation>,
    },

    #[error("Selector matched {count} locations, expected exactly 1")]
    SelectorNotUnique { count: usize, pattern: String },

    #[error("Selector matched 0 locations")]
    NoMatch { pattern: String },

    #[error("syn validation failed: {message}")]
    SynValidationFailed { message: String, code: String },

    #[error("Tree-sitter error: {0}")]
    TreeSitter(#[from] TreeSitterError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Location of an error node in the source.
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: usize,
    pub column: usize,
    pub context: String,
}

/// Parse validator using tree-sitter.
pub struct ParseValidator {
    parser: RustParser,
}

impl ParseValidator {
    /// Create a new parse validator.
    pub fn new() -> Result<Self, TreeSitterError> {
        Ok(Self {
            parser: RustParser::new()?,
        })
    }

    /// Validate that source has no parse errors.
    pub fn validate(&mut self, source: &str) -> Result<(), ValidationError> {
        let parsed = self.parser.parse_with_source(source)?;
        let errors = collect_errors(&parsed, source);

        if !errors.is_empty() {
            return Err(ValidationError::ParseErrorIntroduced {
                count: errors.len(),
                errors,
            });
        }

        Ok(())
    }

    /// Validate a file path.
    pub fn validate_file(&mut self, path: impl AsRef<Path>) -> Result<(), ValidationError> {
        let source = std::fs::read_to_string(path)?;
        self.validate(&source)
    }

    /// Compare two sources and check if new errors were introduced.
    ///
    /// Returns Ok if the edited source doesn't introduce new parse errors
    /// that weren't in the original.
    pub fn validate_edit(&mut self, original: &str, edited: &str) -> Result<(), ValidationError> {
        let original_parsed = self.parser.parse_with_source(original)?;
        let edited_parsed = self.parser.parse_with_source(edited)?;

        let original_errors = collect_error_positions(&original_parsed);
        let edited_errors = collect_errors(&edited_parsed, edited);

        // Filter to only new errors (not present in original)
        let new_errors: Vec<_> = edited_errors
            .into_iter()
            .filter(|e| !original_errors.contains(&(e.byte_start, e.byte_end)))
            .collect();

        if !new_errors.is_empty() {
            return Err(ValidationError::ParseErrorIntroduced {
                count: new_errors.len(),
                errors: new_errors,
            });
        }

        Ok(())
    }
}

impl Default for ParseValidator {
    /// # Panics
    ///
    /// Panics if tree-sitter parser initialization fails (e.g., out of memory).
    fn default() -> Self {
        Self::new().expect("tree-sitter parser initialization failed")
    }
}

/// Pooled validation functions that reuse parsers from thread-local pool.
///
/// These functions provide significant performance improvements for multi-patch
/// workloads by avoiding redundant parser allocation and initialization.
pub mod pooled {
    use super::*;
    use crate::pool;

    /// Validate source code using pooled parser.
    pub fn validate(source: &str) -> Result<(), ValidationError> {
        pool::with_parser(|parser| {
            let parsed = parser.parse_with_source(source)?;
            let errors = collect_errors(&parsed, source);

            if !errors.is_empty() {
                return Err(ValidationError::ParseErrorIntroduced {
                    count: errors.len(),
                    errors,
                });
            }

            Ok(())
        })?
    }

    /// Compare two sources and check if new errors were introduced using pooled parser.
    pub fn validate_edit(original: &str, edited: &str) -> Result<(), ValidationError> {
        pool::with_parser(|parser| {
            let original_parsed = parser.parse_with_source(original)?;
            let original_errors = collect_error_positions(&original_parsed);

            let edited_parsed = parser.parse_with_source(edited)?;
            let edited_errors = collect_error_positions(&edited_parsed);

            // Check if new errors were introduced
            let new_errors: Vec<_> = edited_errors
                .difference(&original_errors)
                .copied()
                .collect();

            if !new_errors.is_empty() {
                let error_details = collect_errors(&edited_parsed, edited);
                return Err(ValidationError::ParseErrorIntroduced {
                    count: error_details.len(),
                    errors: error_details,
                });
            }

            Ok(())
        })?
    }
}

/// Collect all error nodes from a parsed source.
fn collect_errors(parsed: &ParsedSource<'_>, source: &str) -> Vec<ErrorLocation> {
    let mut errors = Vec::new();
    collect_errors_recursive(parsed.root_node(), source, &mut errors);
    errors
}

fn collect_errors_recursive(
    node: tree_sitter::Node<'_>,
    source: &str,
    errors: &mut Vec<ErrorLocation>,
) {
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        let byte_start = node.start_byte();
        let byte_end = node.end_byte();

        // Extract context (up to 50 chars around the error)
        let context_start = byte_start.saturating_sub(20);
        let context_end = (byte_end + 20).min(source.len());
        let context = source
            .get(context_start..context_end)
            .unwrap_or("")
            .replace('\n', "\\n");

        errors.push(ErrorLocation {
            byte_start,
            byte_end,
            line: start.row + 1,
            column: start.column + 1,
            context,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors_recursive(child, source, errors);
    }
}

/// Collect error positions (for comparison).
fn collect_error_positions(parsed: &ParsedSource<'_>) -> std::collections::HashSet<(usize, usize)> {
    let mut positions = std::collections::HashSet::new();
    collect_error_positions_recursive(parsed.root_node(), &mut positions);
    positions
}

fn collect_error_positions_recursive(
    node: tree_sitter::Node<'_>,
    positions: &mut std::collections::HashSet<(usize, usize)>,
) {
    if node.is_error() || node.is_missing() {
        positions.insert((node.start_byte(), node.end_byte()));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_error_positions_recursive(child, positions);
    }
}

/// syn-based validation for generated Rust code snippets.
pub mod syn_validate {
    use super::ValidationError;

    /// Validate that code parses as a valid Rust item (fn, struct, impl, etc.).
    pub fn validate_item(code: &str) -> Result<(), ValidationError> {
        syn::parse_str::<syn::Item>(code).map_err(|e| ValidationError::SynValidationFailed {
            message: e.to_string(),
            code: code.to_string(),
        })?;
        Ok(())
    }

    /// Validate that code parses as a valid Rust expression.
    pub fn validate_expr(code: &str) -> Result<(), ValidationError> {
        syn::parse_str::<syn::Expr>(code).map_err(|e| ValidationError::SynValidationFailed {
            message: e.to_string(),
            code: code.to_string(),
        })?;
        Ok(())
    }

    /// Validate that code parses as a valid Rust statement.
    pub fn validate_stmt(code: &str) -> Result<(), ValidationError> {
        syn::parse_str::<syn::Stmt>(code).map_err(|e| ValidationError::SynValidationFailed {
            message: e.to_string(),
            code: code.to_string(),
        })?;
        Ok(())
    }

    /// Validate that code parses as a valid Rust type.
    pub fn validate_type(code: &str) -> Result<(), ValidationError> {
        syn::parse_str::<syn::Type>(code).map_err(|e| ValidationError::SynValidationFailed {
            message: e.to_string(),
            code: code.to_string(),
        })?;
        Ok(())
    }

    /// Validate that code parses as a complete Rust file.
    pub fn validate_file(code: &str) -> Result<(), ValidationError> {
        syn::parse_file(code).map_err(|e| ValidationError::SynValidationFailed {
            message: e.to_string(),
            code: code.to_string(),
        })?;
        Ok(())
    }

    /// Validate match arm body (expression).
    pub fn validate_match_arm_body(code: &str) -> Result<(), ValidationError> {
        // Match arm bodies are expressions, possibly with a trailing comma
        let trimmed = code.trim().trim_end_matches(',');
        validate_expr(trimmed)
    }

    /// Validate function body (block contents).
    pub fn validate_block(code: &str) -> Result<(), ValidationError> {
        // Try parsing as a block
        let block_code = format!("{{ {} }}", code);
        syn::parse_str::<syn::Block>(&block_code).map_err(|e| {
            ValidationError::SynValidationFailed {
                message: e.to_string(),
                code: code.to_string(),
            }
        })?;
        Ok(())
    }
}

/// Selector uniqueness checker.
pub struct SelectorValidator;

impl SelectorValidator {
    /// Check that a pattern match count is exactly 1.
    pub fn check_unique(count: usize, pattern: &str) -> Result<(), ValidationError> {
        match count {
            0 => Err(ValidationError::NoMatch {
                pattern: pattern.to_string(),
            }),
            1 => Ok(()),
            n => Err(ValidationError::SelectorNotUnique {
                count: n,
                pattern: pattern.to_string(),
            }),
        }
    }

    /// Check that a pattern matched at least once.
    pub fn check_found(count: usize, pattern: &str) -> Result<(), ValidationError> {
        if count == 0 {
            Err(ValidationError::NoMatch {
                pattern: pattern.to_string(),
            })
        } else {
            Ok(())
        }
    }
}

/// Validated edit - wraps Edit with automatic parse validation.
///
/// Ensures that:
/// 1. The edit doesn't introduce new parse errors
/// 2. Generated code snippets are valid according to syn
pub struct ValidatedEdit {
    edit: crate::edit::Edit,
    validate_parse: bool,
}

impl ValidatedEdit {
    /// Create a validated edit from an existing edit.
    pub fn new(edit: crate::edit::Edit) -> Self {
        Self {
            edit,
            validate_parse: true,
        }
    }

    /// Disable parse validation (useful when intentionally editing broken code).
    pub fn skip_parse_validation(mut self) -> Self {
        self.validate_parse = false;
        self
    }

    /// Apply the edit with validation.
    ///
    /// Returns an error if the edit would introduce parse errors.
    pub fn apply(self) -> Result<crate::edit::EditResult, ValidationError> {
        use std::fs;

        if !self.validate_parse {
            return Ok(self.edit.apply()?);
        }

        // Read original content
        let original = fs::read_to_string(&self.edit.file)?;

        // Compute what the edited content would be
        let edited = {
            let mut content = original.clone();
            let before = &content[self.edit.byte_start..self.edit.byte_end];

            // Check verification
            if !self.edit.expected_before.matches(before) {
                return Err(ValidationError::from(
                    crate::edit::EditError::BeforeTextMismatch {
                        file: self.edit.file.clone(),
                        byte_start: self.edit.byte_start,
                        byte_end: self.edit.byte_end,
                        expected: format!("{:?}", self.edit.expected_before),
                        found: before.to_string(),
                    },
                ));
            }

            // Simulate edit
            content.replace_range(
                self.edit.byte_start..self.edit.byte_end,
                &self.edit.new_text,
            );
            content
        };

        // Validate the edited content
        let mut validator = ParseValidator::new()?;
        validator.validate_edit(&original, &edited)?;

        // Now apply for real
        Ok(self.edit.apply()?)
    }
}

impl From<crate::edit::EditError> for ValidationError {
    fn from(e: crate::edit::EditError) -> Self {
        match e {
            crate::edit::EditError::Io(io) => ValidationError::Io(io),
            other => ValidationError::SynValidationFailed {
                message: other.to_string(),
                code: String::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_validator_valid() {
        let mut validator = ParseValidator::new().unwrap();
        let source = "fn main() { println!(\"hello\"); }";
        assert!(validator.validate(source).is_ok());
    }

    #[test]
    fn test_parse_validator_invalid() {
        let mut validator = ParseValidator::new().unwrap();
        let source = "fn main( { }"; // Missing closing paren
        let result = validator.validate(source);
        assert!(matches!(
            result,
            Err(ValidationError::ParseErrorIntroduced { .. })
        ));
    }

    #[test]
    fn test_parse_validator_edit_introduces_error() {
        let mut validator = ParseValidator::new().unwrap();
        let original = "fn main() { let x = 1; }";
        let edited = "fn main( { let x = 1; }"; // Removed closing paren

        let result = validator.validate_edit(original, edited);
        assert!(matches!(
            result,
            Err(ValidationError::ParseErrorIntroduced { .. })
        ));
    }

    #[test]
    fn test_parse_validator_edit_preserves_existing_error() {
        let mut validator = ParseValidator::new().unwrap();
        // Both have the same error
        let original = "fn main( { }";
        let edited = "fn main( { let x = 1; }";

        // This should pass because we're not introducing NEW errors
        // (the error existed in the original)
        let result = validator.validate_edit(original, edited);
        assert!(result.is_ok());
    }

    #[test]
    fn test_syn_validate_item() {
        assert!(syn_validate::validate_item("fn foo() {}").is_ok());
        assert!(syn_validate::validate_item("struct Foo { x: i32 }").is_ok());
        assert!(syn_validate::validate_item("not valid rust").is_err());
    }

    #[test]
    fn test_syn_validate_expr() {
        assert!(syn_validate::validate_expr("1 + 2").is_ok());
        assert!(syn_validate::validate_expr("foo.bar()").is_ok());
        assert!(syn_validate::validate_expr("if x { 1 } else { 2 }").is_ok());
        assert!(syn_validate::validate_expr("fn foo() {}").is_err()); // Not an expr
    }

    #[test]
    fn test_syn_validate_match_arm_body() {
        assert!(syn_validate::validate_match_arm_body("OtelExporter::None").is_ok());
        assert!(syn_validate::validate_match_arm_body("OtelExporter::None,").is_ok());
        assert!(syn_validate::validate_match_arm_body("{ do_something(); result }").is_ok());
    }

    #[test]
    fn test_syn_validate_block() {
        assert!(syn_validate::validate_block("let x = 1; x + 1").is_ok());
        assert!(syn_validate::validate_block("println!(\"hello\");").is_ok());
    }

    #[test]
    fn test_selector_validator_unique() {
        assert!(SelectorValidator::check_unique(1, "test").is_ok());
        assert!(matches!(
            SelectorValidator::check_unique(0, "test"),
            Err(ValidationError::NoMatch { .. })
        ));
        assert!(matches!(
            SelectorValidator::check_unique(2, "test"),
            Err(ValidationError::SelectorNotUnique { count: 2, .. })
        ));
    }

    #[test]
    fn test_selector_validator_found() {
        assert!(SelectorValidator::check_found(1, "test").is_ok());
        assert!(SelectorValidator::check_found(5, "test").is_ok());
        assert!(matches!(
            SelectorValidator::check_found(0, "test"),
            Err(ValidationError::NoMatch { .. })
        ));
    }

    #[test]
    fn test_validated_edit_success() {
        use crate::edit::Edit;
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        fs::write(&file_path, "fn main() { let x = 1; }").unwrap();

        let edit = Edit::new(&file_path, 12, 22, "let y = 2;", "let x = 1;");
        let validated = ValidatedEdit::new(edit);
        let result = validated.apply();

        assert!(result.is_ok());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "fn main() { let y = 2; }");
    }

    #[test]
    fn test_validated_edit_rejects_parse_error() {
        use crate::edit::Edit;
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        fs::write(&file_path, "fn main() { let x = 1; }").unwrap();

        // This edit breaks the syntax (removes closing brace)
        let edit = Edit::new(&file_path, 22, 24, "", " }");
        let validated = ValidatedEdit::new(edit);
        let result = validated.apply();

        assert!(matches!(
            result,
            Err(ValidationError::ParseErrorIntroduced { .. })
        ));

        // File should be unchanged
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "fn main() { let x = 1; }");
    }

    #[test]
    fn test_validated_edit_skip_validation() {
        use crate::edit::Edit;
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        fs::write(&file_path, "fn main() { let x = 1; }").unwrap();

        // This edit breaks syntax, but we skip validation
        let edit = Edit::new(&file_path, 22, 24, "", " }");
        let validated = ValidatedEdit::new(edit).skip_parse_validation();
        let result = validated.apply();

        assert!(result.is_ok());
        // File should be changed (even though it's now invalid)
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "fn main() { let x = 1;");
    }
}
