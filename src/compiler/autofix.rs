//! Auto-fix strategies for common compiler errors.
//!
//! This module provides automatic fixes for well-understood error patterns,
//! particularly E0063 (missing struct fields) which commonly occurs when
//! upstream dependencies add new required fields.

use crate::compiler::diagnostic::{CompileDiagnostic, Suggestion};
use crate::edit::Edit;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutofixError {
    #[error("Cannot auto-fix: {0}")]
    CannotFix(String),

    #[error("Edit error: {0}")]
    EditError(#[from] crate::edit::EditError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result of attempting to auto-fix a diagnostic.
#[derive(Debug)]
pub enum AutofixResult {
    /// Successfully generated fixes
    Fixed(Vec<Edit>),
    /// Cannot auto-fix this diagnostic
    CannotFix { reason: String },
}

/// Attempt to auto-fix a diagnostic.
///
/// Returns `AutofixResult::Fixed` with edits if fixable, or `CannotFix` with reason.
#[must_use]
pub fn try_autofix(diag: &CompileDiagnostic, workspace: &Path) -> AutofixResult {
    // 1. Check for MachineApplicable suggestions first (compiler knows best)
    let machine_fixes = diag.machine_applicable_suggestions();
    if !machine_fixes.is_empty() {
        let edits = machine_fixes
            .iter()
            .filter_map(|s| suggestion_to_edit(s, workspace))
            .collect::<Vec<_>>();

        if !edits.is_empty() {
            return AutofixResult::Fixed(edits);
        }
    }

    // 2. Pattern-match on error codes for custom fixes
    match diag.code.as_deref() {
        Some("E0063") => fix_missing_field(diag, workspace),
        _ => AutofixResult::CannotFix {
            reason: format!(
                "No auto-fix strategy for error code {:?}",
                diag.code
            ),
        },
    }
}

/// Convert a compiler suggestion to an Edit.
fn suggestion_to_edit(suggestion: &Suggestion, workspace: &Path) -> Option<Edit> {
    // Ensure file is within workspace
    if !suggestion.file.starts_with(workspace) {
        return None;
    }

    // Read the file to get the expected text
    let content = std::fs::read_to_string(&suggestion.file).ok()?;

    // Validate byte range
    if suggestion.byte_end > content.len() {
        return None;
    }

    let expected = &content[suggestion.byte_start..suggestion.byte_end];

    Some(Edit::new(
        suggestion.file.clone(),
        suggestion.byte_start,
        suggestion.byte_end,
        suggestion.replacement.clone(),
        expected,
    ))
}

/// Fix E0063: missing field in struct initializer.
///
/// Parses the error message to extract field name and struct type,
/// then generates a sensible default value.
fn fix_missing_field(diag: &CompileDiagnostic, workspace: &Path) -> AutofixResult {
    // Parse error message: "missing field `field_name` in initializer of `StructName`"
    let Some((field_name, _struct_name)) = parse_missing_field_message(&diag.message) else {
        return AutofixResult::CannotFix {
            reason: format!("Could not parse E0063 message: {}", diag.message),
        };
    };

    // Find the primary span (the struct initializer location)
    let Some(span) = diag.spans.first() else {
        return AutofixResult::CannotFix {
            reason: "No source span in diagnostic".to_string(),
        };
    };

    // Skip macro expansions - too risky
    if span.is_macro_expansion {
        return AutofixResult::CannotFix {
            reason: "Cannot auto-fix inside macro expansion".to_string(),
        };
    }

    // Read the file to find the struct initializer
    let content = match std::fs::read_to_string(&span.file) {
        Ok(c) => c,
        Err(e) => {
            return AutofixResult::CannotFix {
                reason: format!("Cannot read file: {}", e),
            };
        }
    };

    // Find the closing brace of the struct initializer
    // We'll insert our new field before it
    let Some(insert_info) = find_struct_initializer_insert_point(&content, span.byte_start, span.byte_end) else {
        return AutofixResult::CannotFix {
            reason: "Cannot find struct initializer closing brace".to_string(),
        };
    };

    // Generate default value based on field name patterns
    let default_value = infer_default_value(&field_name);

    // Build the field initialization text with proper indentation
    let field_init = if insert_info.needs_comma_before {
        format!(
            ",\n{}{}: {}",
            insert_info.field_indent, field_name, default_value
        )
    } else if insert_info.is_empty_struct {
        format!(
            "\n{}{}: {},\n{}",
            insert_info.field_indent, field_name, default_value, insert_info.closing_brace_indent
        )
    } else {
        format!(
            "\n{}{}: {},",
            insert_info.field_indent, field_name, default_value
        )
    };

    // Create the edit - insert before the closing brace
    let expected = &content[insert_info.insert_at..insert_info.insert_at];

    let edit = Edit::new(
        span.file.clone(),
        insert_info.insert_at,
        insert_info.insert_at,
        field_init,
        expected,
    );

    // Verify the file is in workspace
    if !span.file.starts_with(workspace) {
        return AutofixResult::CannotFix {
            reason: "File is outside workspace".to_string(),
        };
    }

    AutofixResult::Fixed(vec![edit])
}

/// Parse E0063 error message to extract field name and struct name.
///
/// Example: "missing field `windows_sandbox_level` in initializer of `SandboxPolicy`"
fn parse_missing_field_message(message: &str) -> Option<(String, String)> {
    // Pattern: "missing field `FIELD` in initializer of `STRUCT`"
    let field_start = message.find("missing field `")? + "missing field `".len();
    let field_end = message[field_start..].find('`')? + field_start;
    let field_name = message[field_start..field_end].to_string();

    let struct_start = message.find("in initializer of `")? + "in initializer of `".len();
    let struct_end = message[struct_start..].find('`')? + struct_start;
    let struct_name = message[struct_start..struct_end].to_string();

    Some((field_name, struct_name))
}

/// Information about where to insert a new field in a struct initializer.
#[derive(Debug)]
struct InsertPoint {
    /// Byte offset where to insert
    insert_at: usize,
    /// Whether we need a comma before our new field
    needs_comma_before: bool,
    /// Whether the struct initializer is currently empty (just `{}`)
    is_empty_struct: bool,
    /// Indentation string for fields (detected from existing fields)
    field_indent: String,
    /// Indentation string for the closing brace
    closing_brace_indent: String,
}

/// Find the insertion point for a new field in a struct initializer.
///
/// Scans backwards from the closing brace to find the right spot.
fn find_struct_initializer_insert_point(
    content: &str,
    span_start: usize,
    span_end: usize,
) -> Option<InsertPoint> {
    // The span typically points to the struct initializer expression
    // We need to find the closing brace `}`
    let search_start = span_start.saturating_sub(50); // Look a bit before in case span is partial
    let search_end = (span_end + 500).min(content.len()); // Look ahead for the closing brace

    let search_region = &content[search_start..search_end];

    // Find matching braces to locate the struct body
    let mut brace_depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut last_closing_brace = None;
    let mut first_opening_brace = None;

    for (i, c) in search_region.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => {
                if first_opening_brace.is_none() {
                    first_opening_brace = Some(search_start + i);
                }
                brace_depth += 1;
            }
            '}' if !in_string => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    last_closing_brace = Some(search_start + i);
                    break;
                }
            }
            _ => {}
        }
    }

    let closing_brace = last_closing_brace?;
    let opening_brace = first_opening_brace?;

    // Check if struct is empty (only whitespace between braces)
    let between_braces = &content[opening_brace + 1..closing_brace];
    let is_empty = between_braces.trim().is_empty();

    // Find the last non-whitespace character before the closing brace
    let before_brace = &content[opening_brace + 1..closing_brace];
    let last_content_char = before_brace.trim_end().chars().last();

    // Determine if we need a comma (last char is not a comma and struct isn't empty)
    let needs_comma = !is_empty && last_content_char != Some(',');

    // Detect indentation from existing fields or closing brace
    let (field_indent, closing_brace_indent) = detect_indentation(content, opening_brace, closing_brace);

    Some(InsertPoint {
        insert_at: closing_brace,
        needs_comma_before: needs_comma,
        is_empty_struct: is_empty,
        field_indent,
        closing_brace_indent,
    })
}

/// Detect the indentation used in a struct initializer.
///
/// Returns (field_indent, closing_brace_indent).
fn detect_indentation(content: &str, opening_brace: usize, closing_brace: usize) -> (String, String) {
    // Find the indentation of the closing brace by looking at the line it's on
    let closing_brace_indent = get_line_indent(content, closing_brace);

    // Try to find an existing field's indentation
    let between_braces = &content[opening_brace + 1..closing_brace];

    // Look for lines that contain a colon (field: value pattern)
    for line in between_braces.lines() {
        if line.contains(':') && !line.trim().is_empty() {
            // Extract leading whitespace
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            if !indent.is_empty() {
                return (indent, closing_brace_indent);
            }
        }
    }

    // No existing fields found - use closing brace indent + 4 spaces
    let field_indent = format!("{}    ", closing_brace_indent);
    (field_indent, closing_brace_indent)
}

/// Get the indentation (leading whitespace) of the line containing the given byte offset.
fn get_line_indent(content: &str, byte_offset: usize) -> String {
    // Find the start of the line
    let line_start = content[..byte_offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    // Extract leading whitespace from line start to first non-whitespace
    content[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Infer a sensible default value for a field based on its name.
///
/// This is a heuristic - we can't know the actual type without more analysis.
/// We err on the side of `None` for Option-like fields.
fn infer_default_value(field_name: &str) -> &'static str {
    // Common naming patterns that suggest Option<T>
    if field_name.ends_with("_level")
        || field_name.ends_with("_limit")
        || field_name.ends_with("_timeout")
        || field_name.ends_with("_override")
        || field_name.ends_with("_config")
        || field_name.ends_with("_policy")
        || field_name.starts_with("optional_")
        || field_name.starts_with("maybe_")
        || field_name.contains("sandbox")
    {
        return "None";
    }

    // Boolean-like fields
    if field_name.starts_with("is_")
        || field_name.starts_with("has_")
        || field_name.starts_with("can_")
        || field_name.starts_with("should_")
        || field_name.starts_with("enable")
        || field_name.starts_with("disable")
        || field_name.ends_with("_enabled")
        || field_name.ends_with("_disabled")
        || field_name.ends_with("_allowed")
    {
        return "false";
    }

    // Collection-like fields
    if field_name.ends_with("s") && !field_name.ends_with("ss") {
        // Plural names often indicate Vec/HashSet
        return "Vec::new()";
    }

    // Count/size fields
    if field_name.ends_with("_count")
        || field_name.ends_with("_size")
        || field_name.ends_with("_index")
    {
        return "0";
    }

    // String-like fields
    if field_name.ends_with("_name")
        || field_name.ends_with("_path")
        || field_name.ends_with("_url")
        || field_name.ends_with("_message")
    {
        return "String::new()";
    }

    // Default to None - safest for most new optional fields
    // Upstream additions are often optional
    "None"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_missing_field_message() {
        let msg = "missing field `windows_sandbox_level` in initializer of `SandboxPolicy`";
        let (field, struct_name) = parse_missing_field_message(msg).unwrap();
        assert_eq!(field, "windows_sandbox_level");
        assert_eq!(struct_name, "SandboxPolicy");
    }

    #[test]
    fn test_parse_missing_field_message_complex() {
        let msg = "missing field `foo_bar` in initializer of `some::module::MyStruct`";
        let (field, struct_name) = parse_missing_field_message(msg).unwrap();
        assert_eq!(field, "foo_bar");
        assert_eq!(struct_name, "some::module::MyStruct");
    }

    #[test]
    fn test_infer_default_value() {
        assert_eq!(infer_default_value("windows_sandbox_level"), "None");
        assert_eq!(infer_default_value("is_enabled"), "false");
        assert_eq!(infer_default_value("items"), "Vec::new()");
        assert_eq!(infer_default_value("retry_count"), "0");
        assert_eq!(infer_default_value("file_name"), "String::new()");
        assert_eq!(infer_default_value("unknown_field"), "None");
    }

    #[test]
    fn test_find_insert_point_simple() {
        let content = r#"let x = MyStruct {
        field1: 1,
        field2: 2,
    };"#;

        let insert = find_struct_initializer_insert_point(content, 8, 60).unwrap();
        assert!(!insert.is_empty_struct);
        assert!(!insert.needs_comma_before); // Last field has comma
        assert_eq!(insert.field_indent, "        "); // 8 spaces
        assert_eq!(insert.closing_brace_indent, "    "); // 4 spaces
    }

    #[test]
    fn test_find_insert_point_no_trailing_comma() {
        let content = r#"let x = MyStruct {
        field1: 1,
        field2: 2
    };"#;

        let insert = find_struct_initializer_insert_point(content, 8, 60).unwrap();
        assert!(!insert.is_empty_struct);
        assert!(insert.needs_comma_before); // Last field missing comma
        assert_eq!(insert.field_indent, "        "); // 8 spaces
    }

    #[test]
    fn test_find_insert_point_empty() {
        let content = "let x = MyStruct { };";
        let insert = find_struct_initializer_insert_point(content, 8, 20).unwrap();
        assert!(insert.is_empty_struct);
    }

    #[test]
    fn test_detect_indentation_tabs() {
        let content = "let x = MyStruct {\n\t\tfield1: 1,\n\t}";
        let insert = find_struct_initializer_insert_point(content, 8, 35).unwrap();
        assert_eq!(insert.field_indent, "\t\t"); // 2 tabs
        assert_eq!(insert.closing_brace_indent, "\t"); // 1 tab
    }

    #[test]
    fn test_detect_indentation_real_world() {
        // Simulates the real-world case that was failing
        let content = r#"        self.app_event_tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: self.model.clone(),
        }));"#;

        let insert = find_struct_initializer_insert_point(content, 50, 180).unwrap();
        assert_eq!(insert.field_indent, "            "); // 12 spaces
        assert_eq!(insert.closing_brace_indent, "        "); // 8 spaces
    }
}
