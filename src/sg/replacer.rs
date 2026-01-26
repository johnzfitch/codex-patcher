use crate::edit::{Edit, EditVerification};
use crate::sg::errors::AstGrepError;
use crate::sg::matcher::{PatternMatch, PatternMatcher};
use std::path::PathBuf;

/// A replacement operation with source and target.
#[derive(Debug, Clone)]
pub struct Replacement {
    /// Byte range to replace
    pub byte_start: usize,
    pub byte_end: usize,
    /// Original text (for verification)
    pub original: String,
    /// New text
    pub replacement: String,
}

impl Replacement {
    /// Convert to an Edit for the given file path.
    pub fn to_edit(&self, file: impl Into<PathBuf>) -> Edit {
        Edit::with_verification(
            file.into(),
            self.byte_start,
            self.byte_end,
            self.replacement.clone(),
            EditVerification::from_text(&self.original),
        )
    }
}

/// Builder for capture-based replacements.
///
/// Allows replacing specific captured metavariables or the entire match.
pub struct CaptureReplacer<'a> {
    matcher: &'a PatternMatcher,
    pattern_match: PatternMatch,
}

impl<'a> CaptureReplacer<'a> {
    /// Create a new capture replacer from a pattern match.
    pub fn new(matcher: &'a PatternMatcher, pattern_match: PatternMatch) -> Self {
        Self {
            matcher,
            pattern_match,
        }
    }

    /// Replace the entire matched region with new text.
    pub fn replace_match(&self, new_text: &str) -> Replacement {
        Replacement {
            byte_start: self.pattern_match.byte_start,
            byte_end: self.pattern_match.byte_end,
            original: self.pattern_match.text.clone(),
            replacement: new_text.to_string(),
        }
    }

    /// Replace a specific captured metavariable with new text.
    ///
    /// Note: This uses string search to find the capture position, which
    /// may not be accurate if the captured text appears multiple times.
    pub fn replace_capture(
        &self,
        capture_name: &str,
        new_text: &str,
    ) -> Result<Replacement, AstGrepError> {
        let capture_text = self
            .pattern_match
            .captures
            .get(capture_name)
            .ok_or_else(|| AstGrepError::MetavarNotFound {
                name: capture_name.to_string(),
            })?;

        let (byte_start, byte_end) = self
            .pattern_match
            .find_capture_span(capture_name)
            .ok_or_else(|| AstGrepError::MetavarNotFound {
                name: capture_name.to_string(),
            })?;

        Ok(Replacement {
            byte_start,
            byte_end,
            original: capture_text.clone(),
            replacement: new_text.to_string(),
        })
    }

    /// Replace using a template that references captures.
    ///
    /// Template syntax: `$NAME` references a captured metavariable.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Pattern: fn $NAME($$$PARAMS) { $$$BODY }
    /// // Template: fn renamed_$NAME($$$PARAMS) { $$$BODY }
    /// ```
    pub fn replace_with_template(&self, template: &str) -> Replacement {
        let mut result = template.to_string();

        // Replace all $NAME references with their captured values
        for (name, capture_text) in &self.pattern_match.captures {
            // Handle both $ and $$$ style placeholders
            let placeholder = format!("${name}");
            result = result.replace(&placeholder, capture_text);

            let variadic_placeholder = format!("$$${name}");
            result = result.replace(&variadic_placeholder, capture_text);
        }

        Replacement {
            byte_start: self.pattern_match.byte_start,
            byte_end: self.pattern_match.byte_end,
            original: self.pattern_match.text.clone(),
            replacement: result,
        }
    }

    /// Get the underlying pattern match.
    pub fn pattern_match(&self) -> &PatternMatch {
        &self.pattern_match
    }

    /// Get the source code.
    pub fn source(&self) -> &str {
        self.matcher.source()
    }
}

/// High-level function to find and replace using ast-grep.
pub fn find_and_replace(
    source: &str,
    pattern: &str,
    replacement: &str,
) -> Result<Vec<Replacement>, AstGrepError> {
    let matcher = PatternMatcher::new(source);
    let matches = matcher.find_all(pattern)?;

    let mut replacements = Vec::new();

    for m in matches {
        let replacer = CaptureReplacer::new(&matcher, m);
        replacements.push(replacer.replace_with_template(replacement));
    }

    Ok(replacements)
}

/// Find unique match and replace, returning an Edit.
pub fn find_unique_and_replace(
    source: &str,
    file: impl Into<PathBuf>,
    pattern: &str,
    replacement: &str,
) -> Result<Edit, AstGrepError> {
    let matcher = PatternMatcher::new(source);
    let m = matcher.find_unique(pattern)?;
    let replacer = CaptureReplacer::new(&matcher, m);
    let repl = replacer.replace_with_template(replacement);
    Ok(repl.to_edit(file))
}

/// Replace a specific capture within a unique match.
pub fn replace_capture_unique(
    source: &str,
    file: impl Into<PathBuf>,
    pattern: &str,
    capture_name: &str,
    new_value: &str,
) -> Result<Edit, AstGrepError> {
    let matcher = PatternMatcher::new(source);
    let m = matcher.find_unique(pattern)?;
    let replacer = CaptureReplacer::new(&matcher, m);
    let repl = replacer.replace_capture(capture_name, new_value)?;
    Ok(repl.to_edit(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_entire_match() {
        let source = "fn old_name() { 42 }";
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("fn old_name() { $$$BODY }").unwrap();
        let replacer = CaptureReplacer::new(&matcher, m);

        let repl = replacer.replace_match("fn new_name() { 42 }");

        assert_eq!(repl.original, "fn old_name() { 42 }");
        assert_eq!(repl.replacement, "fn new_name() { 42 }");
        assert_eq!(repl.byte_start, 0);
    }

    #[test]
    fn replace_with_template() {
        let source = "fn foo() { 42 }";
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("fn $NAME() { $$$BODY }").unwrap();
        let replacer = CaptureReplacer::new(&matcher, m);

        let repl = replacer.replace_with_template("fn renamed_$NAME() { $$$BODY }");

        // Note: The template expansion uses captured values
        assert!(repl.replacement.contains("renamed_foo"));
    }

    #[test]
    fn find_and_replace_all() {
        let source = r#"
fn test() {
    let a = x.clone();
    let b = y.clone();
}
"#;
        let replacements = find_and_replace(source, "$EXPR.clone()", "$EXPR.to_owned()").unwrap();

        assert_eq!(replacements.len(), 2);
        for r in &replacements {
            assert!(r.replacement.contains(".to_owned()"));
        }
    }

    #[test]
    fn to_edit_conversion() {
        let source = "const FOO: i32 = 1;";
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("const $NAME: $TYPE = $VALUE;").unwrap();
        let replacer = CaptureReplacer::new(&matcher, m);

        let repl = replacer.replace_match("const FOO: i32 = 42;");
        let edit = repl.to_edit("test.rs");

        assert_eq!(edit.new_text, "const FOO: i32 = 42;");
        assert_eq!(edit.byte_start, repl.byte_start);
        assert_eq!(edit.byte_end, repl.byte_end);
    }

    #[test]
    fn replace_statsig_example() {
        // Realistic example from codex patcher use case
        // Note: Match arm patterns like `PAT => BODY` cannot be matched directly
        // because they aren't valid standalone Rust code. Instead, match the
        // components (pattern, body) separately or match the whole match expression.
        let source = r#"
match exporter {
    OtelExporter::Statsig => {
        OtelExporter::OtlpHttp {
            endpoint: STATSIG_ENDPOINT.to_string(),
        }
    }
    _ => exporter.clone(),
}
"#;
        let matcher = PatternMatcher::new(source);

        // Find the Statsig variant pattern (left side of match arm)
        let matches = matcher.find_all("OtelExporter::Statsig").unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].text.contains("OtelExporter::Statsig"));

        // Find the struct expression in the body
        let struct_matches = matcher
            .find_all("OtelExporter::OtlpHttp { $$$FIELDS }")
            .unwrap();
        assert_eq!(struct_matches.len(), 1);

        let replacer = CaptureReplacer::new(&matcher, struct_matches[0].clone());
        let repl = replacer.replace_match("OtelExporter::None");

        assert!(repl.replacement.contains("OtelExporter::None"));
    }
}
