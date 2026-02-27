use crate::cache;
use crate::sg::errors::AstGrepError;
use crate::sg::lang::rust;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, NodeMatch};
use ast_grep_language::SupportLang;
use std::collections::HashMap;

/// A match from an ast-grep pattern with captured metavariables.
#[derive(Debug, Clone)]
pub struct PatternMatch {
    /// Byte range of the entire match
    pub byte_start: usize,
    pub byte_end: usize,
    /// The matched text
    pub text: String,
    /// Captured metavariables: name -> text
    /// Note: For byte spans of captures, use find_capture_span()
    pub captures: HashMap<String, String>,
}

impl PatternMatch {
    /// Find the byte span of a capture within the matched text.
    ///
    /// This is an approximation that finds the first occurrence of the
    /// captured text within the matched region.
    pub fn find_capture_span(&self, name: &str) -> Option<(usize, usize)> {
        let capture_text = self.captures.get(name)?;
        // Find the capture text within the match
        let offset = self.text.find(capture_text)?;
        let start = self.byte_start + offset;
        let end = start + capture_text.len();
        Some((start, end))
    }
}

/// Pattern matcher using ast-grep's metavariable syntax.
///
/// # Metavariable Syntax
///
/// - `$NAME` - Matches a single node and captures it
/// - `$$$NAME` - Matches zero or more nodes (variadic)
/// - `$_` - Matches any single node (anonymous)
///
/// # Example Patterns
///
/// ```text
/// fn $NAME($$$PARAMS) { $$$BODY }     // Match function definition
/// struct $NAME { $$$FIELDS }           // Match struct definition
/// $EXPR.clone()                        // Match .clone() calls
/// OtelExporter::$VARIANT               // Match enum variants
/// ```
pub struct PatternMatcher {
    source: String,
    sg: AstGrep<StrDoc<SupportLang>>,
}

impl PatternMatcher {
    /// Create a new pattern matcher for the given source code.
    pub fn new(source: &str) -> Self {
        let sg = AstGrep::new(source, rust());
        Self {
            source: source.to_string(),
            sg,
        }
    }

    /// Find all matches for a pattern.
    pub fn find_all(&self, pattern: &str) -> Result<Vec<PatternMatch>, AstGrepError> {
        let pat = cache::get_or_compile_pattern(pattern, rust());
        let root = self.sg.root();
        let matches: Vec<_> = root.find_all(&pat).collect();

        let results = matches
            .into_iter()
            .map(|m| self.node_match_to_pattern_match(m))
            .collect();

        Ok(results)
    }

    /// Find exactly one match for a pattern.
    pub fn find_unique(&self, pattern: &str) -> Result<PatternMatch, AstGrepError> {
        let matches = self.find_all(pattern)?;

        match matches.len() {
            0 => Err(AstGrepError::NoMatch),
            1 => Ok(matches.into_iter().next().expect("len checked == 1")),
            n => Err(AstGrepError::AmbiguousMatch { count: n }),
        }
    }

    /// Check if a pattern has any matches.
    pub fn has_match(&self, pattern: &str) -> bool {
        let pat = cache::get_or_compile_pattern(pattern, rust());
        self.sg.root().find(&pat).is_some()
    }

    /// Find matches within a specific byte range (for context constraints).
    pub fn find_in_range(
        &self,
        pattern: &str,
        start: usize,
        end: usize,
    ) -> Result<Vec<PatternMatch>, AstGrepError> {
        let matches = self.find_all(pattern)?;

        let filtered: Vec<_> = matches
            .into_iter()
            .filter(|m| m.byte_start >= start && m.byte_end <= end)
            .collect();

        Ok(filtered)
    }

    /// Find matches that are inside a function with the given name.
    pub fn find_in_function(
        &self,
        pattern: &str,
        function_name: &str,
    ) -> Result<Vec<PatternMatch>, AstGrepError> {
        // First, find the function
        let func_pattern = format!("fn {function_name}($$$PARAMS) {{ $$$BODY }}");
        let func_matches = self.find_all(&func_pattern)?;

        let mut results = Vec::new();

        for func_match in func_matches {
            // Find pattern matches within the function body
            let inner_matches =
                self.find_in_range(pattern, func_match.byte_start, func_match.byte_end)?;
            results.extend(inner_matches);
        }

        Ok(results)
    }

    /// Get the source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Find all nodes of a specific kind, optionally filtering by a pattern on a field.
    ///
    /// This is useful for constructs that aren't valid standalone Rust syntax,
    /// like match arms (`PAT => BODY`). Since match arms can't be parsed in
    /// isolation, we find them by kind and optionally filter by matching a
    /// pattern against a specific field.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Find all match arms where the pattern is OtelExporter::Statsig
    /// let arms = matcher.find_by_kind_with_field(
    ///     "match_arm",
    ///     Some(("pattern", "OtelExporter::Statsig")),
    /// )?;
    /// ```
    pub fn find_by_kind_with_field(
        &self,
        kind: &str,
        field_filter: Option<(&str, &str)>,
    ) -> Result<Vec<PatternMatch>, AstGrepError> {
        let root = self.sg.root();
        let mut results = Vec::new();

        // Use depth-first traversal to find all nodes
        for node in root.dfs() {
            if node.kind() != kind {
                continue;
            }

            // If we have a field filter, check it
            if let Some((field_name, pattern)) = field_filter {
                let pat = cache::get_or_compile_pattern(pattern, rust());
                let field_node = node.field(field_name);

                if let Some(field) = field_node {
                    if field.find(&pat).is_none() {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Node matches - convert to PatternMatch
            let range = node.range();
            let byte_start = range.start;
            let byte_end = range.end;
            let text = self.source[byte_start..byte_end].to_string();

            results.push(PatternMatch {
                byte_start,
                byte_end,
                text,
                captures: HashMap::new(), // No captures for kind-based matching
            });
        }

        Ok(results)
    }

    /// Find match arms by their pattern.
    ///
    /// Convenience method for finding match arms since they can't be matched
    /// directly with patterns (not valid standalone Rust syntax).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let arms = matcher.find_match_arms("OtelExporter::Statsig")?;
    /// for arm in arms {
    ///     println!("Found arm: {}", arm.text);
    /// }
    /// ```
    pub fn find_match_arms(&self, pattern: &str) -> Result<Vec<PatternMatch>, AstGrepError> {
        self.find_by_kind_with_field("match_arm", Some(("pattern", pattern)))
    }

    fn node_match_to_pattern_match(&self, m: NodeMatch<StrDoc<SupportLang>>) -> PatternMatch {
        let node = m.get_node();
        let range = node.range();
        let byte_start = range.start;
        let byte_end = range.end;
        let text = self.source[byte_start..byte_end].to_string();

        // Convert MetaVarEnv to HashMap<String, String>
        let env = m.get_env().clone();
        let captures: HashMap<String, String> = env.into();

        PatternMatch {
            byte_start,
            byte_end,
            text,
            captures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_function_by_pattern() {
        let source = r#"
fn helper() -> i32 { 42 }

fn main() {
    let x = helper();
    println!("{}", x);
}
"#;
        let matcher = PatternMatcher::new(source);
        let matches = matcher.find_all("fn main() { $$$BODY }").unwrap();

        assert_eq!(matches.len(), 1);
        assert!(matches[0].text.contains("fn main()"));
        assert!(matches[0].captures.contains_key("BODY"));
    }

    #[test]
    fn find_struct_fields() {
        let source = r#"
struct Config {
    name: String,
    value: i32,
}
"#;
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("struct Config { $$$FIELDS }").unwrap();

        assert!(m.captures.contains_key("FIELDS"));
    }

    #[test]
    fn find_method_calls() {
        let source = r#"
fn test() {
    let a = foo.clone();
    let b = bar.clone();
    let c = baz.to_string();
}
"#;
        let matcher = PatternMatcher::new(source);
        let matches = matcher.find_all("$EXPR.clone()").unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn find_enum_variants() {
        let source = r#"
match exporter {
    OtelExporter::Statsig => do_statsig(),
    OtelExporter::None => do_nothing(),
    _ => other(),
}
"#;
        let matcher = PatternMatcher::new(source);
        let matches = matcher.find_all("OtelExporter::$VARIANT").unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn find_unique_success() {
        let source = "fn main() { println!(\"hello\"); }";
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("fn main() { $$$BODY }").unwrap();

        assert!(m.text.contains("fn main()"));
    }

    #[test]
    fn find_unique_no_match() {
        let source = "fn main() {}";
        let matcher = PatternMatcher::new(source);
        let result = matcher.find_unique("fn nonexistent() { $$$BODY }");

        assert!(matches!(result, Err(AstGrepError::NoMatch)));
    }

    #[test]
    fn find_unique_ambiguous() {
        let source = r#"
fn foo() {}
fn bar() {}
"#;
        let matcher = PatternMatcher::new(source);
        let result = matcher.find_unique("fn $NAME() {}");

        assert!(matches!(
            result,
            Err(AstGrepError::AmbiguousMatch { count: 2 })
        ));
    }

    #[test]
    fn find_in_function_context() {
        let source = r#"
fn outer() {
    let x = foo.clone();
}

fn inner() {
    let y = bar.clone();
}
"#;
        let matcher = PatternMatcher::new(source);
        let matches = matcher.find_in_function("$EXPR.clone()", "inner").unwrap();

        assert_eq!(matches.len(), 1);
        assert!(matches[0].captures.contains_key("EXPR"));
    }

    #[test]
    fn byte_spans_accurate() {
        let source = "fn foo() { let x = 1; }";
        let matcher = PatternMatcher::new(source);
        let m = matcher.find_unique("fn $NAME() { $$$BODY }").unwrap();

        // Verify we can extract exact text using byte spans
        let extracted = &source[m.byte_start..m.byte_end];
        assert_eq!(extracted, source);
    }

    #[test]
    fn find_match_arms_by_pattern() {
        let source = r#"
match exporter {
    OtelExporter::Statsig => {
        OtelExporter::OtlpHttp { endpoint: url }
    }
    OtelExporter::None => None,
    _ => exporter.clone(),
}
"#;
        let matcher = PatternMatcher::new(source);

        // Find the Statsig match arm
        let arms = matcher.find_match_arms("OtelExporter::Statsig").unwrap();
        assert_eq!(arms.len(), 1);
        assert!(arms[0].text.contains("OtelExporter::Statsig"));
        assert!(arms[0].text.contains("OtlpHttp"));

        // Find the None match arm
        let none_arms = matcher.find_match_arms("OtelExporter::None").unwrap();
        assert_eq!(none_arms.len(), 1);

        // Find by variant pattern
        let variant_arms = matcher.find_match_arms("OtelExporter::$VARIANT").unwrap();
        assert_eq!(variant_arms.len(), 2); // Statsig and None
    }

    #[test]
    fn find_by_kind_generic() {
        let source = r#"
struct Foo { x: i32 }
struct Bar { y: String }
"#;
        let matcher = PatternMatcher::new(source);

        // Find all struct items
        let structs = matcher
            .find_by_kind_with_field("struct_item", None)
            .unwrap();
        assert_eq!(structs.len(), 2);

        // Find struct with specific name - use metavar to match the type_identifier
        // Note: The "name" field of struct_item is a type_identifier node
        let foo = matcher
            .find_by_kind_with_field("struct_item", Some(("name", "$NAME")))
            .unwrap();
        // $NAME matches any identifier, so both structs match
        assert_eq!(foo.len(), 2);

        // To filter by a specific name, check the text of the match
        let foo_structs: Vec<_> = foo.iter().filter(|m| m.text.contains("Foo")).collect();
        assert_eq!(foo_structs.len(), 1);
    }
}
