use crate::ts::errors::TreeSitterError;
use crate::ts::parser::RustParser;
use crate::ts::query::{queries, QueryEngine, QueryMatch};
use std::path::Path;

/// High-level structural target for locating Rust code constructs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralTarget {
    /// A function by name (top-level or in module)
    Function { name: String },

    /// A method in an impl block
    Method {
        type_name: String,
        method_name: String,
    },

    /// A struct by name
    Struct { name: String },

    /// An enum by name
    Enum { name: String },

    /// A const item by name
    Const { name: String },

    /// Const items matching a regex pattern
    ConstMatching { pattern: String },

    /// A static item by name
    Static { name: String },

    /// An impl block for a type
    Impl { type_name: String },

    /// An impl block for a trait on a type
    ImplTrait {
        trait_name: String,
        type_name: String,
    },

    /// A use declaration matching a path pattern
    Use { path_pattern: String },

    /// Custom tree-sitter query
    Custom { query: String },
}

impl StructuralTarget {
    /// Convert to a tree-sitter query string.
    pub fn to_query(&self) -> String {
        match self {
            StructuralTarget::Function { name } => queries::function_by_name(name),
            StructuralTarget::Method {
                type_name,
                method_name,
            } => queries::method_by_name(type_name, method_name),
            StructuralTarget::Struct { name } => queries::struct_by_name(name),
            StructuralTarget::Enum { name } => queries::enum_by_name(name),
            StructuralTarget::Const { name } => queries::const_by_name(name),
            StructuralTarget::ConstMatching { pattern } => queries::const_matching(pattern),
            StructuralTarget::Static { name } => queries::static_by_name(name),
            StructuralTarget::Impl { type_name } => queries::impl_by_type(type_name),
            StructuralTarget::ImplTrait {
                trait_name,
                type_name,
            } => queries::impl_trait_for_type(trait_name, type_name),
            StructuralTarget::Use { path_pattern } => queries::use_declaration(path_pattern),
            StructuralTarget::Custom { query } => query.clone(),
        }
    }
}

/// Result of locating a structural target.
#[derive(Debug, Clone)]
pub struct LocatorResult {
    /// Byte range of the entire matched construct
    pub byte_start: usize,
    pub byte_end: usize,
    /// The matched text
    pub text: String,
    /// Named captures from the query
    pub captures: std::collections::HashMap<String, CaptureInfo>,
}

#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
}

impl From<QueryMatch> for LocatorResult {
    fn from(m: QueryMatch) -> Self {
        LocatorResult {
            byte_start: m.byte_start,
            byte_end: m.byte_end,
            text: String::new(), // Will be filled in by locator
            captures: m
                .captures
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        CaptureInfo {
                            byte_start: v.byte_start,
                            byte_end: v.byte_end,
                            text: v.text,
                        },
                    )
                })
                .collect(),
        }
    }
}

/// Structural code locator using tree-sitter queries.
pub struct StructuralLocator {
    parser: RustParser,
}

impl StructuralLocator {
    /// Create a new structural locator.
    pub fn new() -> Result<Self, TreeSitterError> {
        Ok(Self {
            parser: RustParser::new()?,
        })
    }

    /// Locate a structural target in source code, expecting exactly one match.
    pub fn locate(&mut self, source: &str, target: &StructuralTarget) -> Result<LocatorResult, TreeSitterError> {
        let parsed = self.parser.parse_with_source(source)?;
        let query_str = target.to_query();
        let engine = QueryEngine::new(&query_str)?;

        let m = engine.find_unique(&parsed)?;
        let mut result = LocatorResult::from(m);
        result.text = source[result.byte_start..result.byte_end].to_string();

        Ok(result)
    }

    /// Locate all matches for a structural target.
    pub fn locate_all(
        &mut self,
        source: &str,
        target: &StructuralTarget,
    ) -> Result<Vec<LocatorResult>, TreeSitterError> {
        let parsed = self.parser.parse_with_source(source)?;
        let query_str = target.to_query();
        let engine = QueryEngine::new(&query_str)?;

        let matches = engine.find_all(&parsed);
        let results = matches
            .into_iter()
            .map(|m| {
                let mut result = LocatorResult::from(m);
                result.text = source[result.byte_start..result.byte_end].to_string();
                result
            })
            .collect();

        Ok(results)
    }

    /// Locate a target in a file.
    pub fn locate_in_file(
        &mut self,
        path: &Path,
        target: &StructuralTarget,
    ) -> Result<LocatorResult, TreeSitterError> {
        let source = std::fs::read_to_string(path).map_err(|e| TreeSitterError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        self.locate(&source, target)
    }

    /// Check if source has syntax errors.
    pub fn has_errors(&mut self, source: &str) -> Result<bool, TreeSitterError> {
        let parsed = self.parser.parse_with_source(source)?;
        Ok(parsed.has_errors())
    }

    /// Get the underlying parser for direct tree-sitter access.
    pub fn parser_mut(&mut self) -> &mut RustParser {
        &mut self.parser
    }
}

impl Default for StructuralLocator {
    fn default() -> Self {
        Self::new().expect("failed to create default StructuralLocator")
    }
}

/// Convenience functions for common operations.
impl StructuralLocator {
    /// Find a function by name.
    pub fn find_function(&mut self, source: &str, name: &str) -> Result<LocatorResult, TreeSitterError> {
        self.locate(source, &StructuralTarget::Function { name: name.to_string() })
    }

    /// Find a struct by name.
    pub fn find_struct(&mut self, source: &str, name: &str) -> Result<LocatorResult, TreeSitterError> {
        self.locate(source, &StructuralTarget::Struct { name: name.to_string() })
    }

    /// Find a const by name.
    pub fn find_const(&mut self, source: &str, name: &str) -> Result<LocatorResult, TreeSitterError> {
        self.locate(source, &StructuralTarget::Const { name: name.to_string() })
    }

    /// Find all consts matching a pattern.
    pub fn find_consts_matching(
        &mut self,
        source: &str,
        pattern: &str,
    ) -> Result<Vec<LocatorResult>, TreeSitterError> {
        self.locate_all(
            source,
            &StructuralTarget::ConstMatching {
                pattern: pattern.to_string(),
            },
        )
    }

    /// Find an impl block for a type.
    pub fn find_impl(&mut self, source: &str, type_name: &str) -> Result<LocatorResult, TreeSitterError> {
        self.locate(
            source,
            &StructuralTarget::Impl {
                type_name: type_name.to_string(),
            },
        )
    }

    /// Find a method in an impl block.
    pub fn find_method(
        &mut self,
        source: &str,
        type_name: &str,
        method_name: &str,
    ) -> Result<LocatorResult, TreeSitterError> {
        self.locate(
            source,
            &StructuralTarget::Method {
                type_name: type_name.to_string(),
                method_name: method_name.to_string(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_function() {
        let mut locator = StructuralLocator::new().unwrap();
        let source = r#"
fn helper() -> i32 {
    42
}

fn main() {
    let x = helper();
    println!("{}", x);
}
"#;

        let result = locator.find_function(source, "main").unwrap();
        assert!(result.text.contains("fn main()"));
        assert!(result.text.contains("println!"));
    }

    #[test]
    fn locate_struct() {
        let mut locator = StructuralLocator::new().unwrap();
        let source = r#"
/// A configuration struct
#[derive(Debug)]
struct Config {
    name: String,
    value: i32,
}
"#;

        let result = locator.find_struct(source, "Config").unwrap();
        assert!(result.text.contains("struct Config"));
        assert!(result.text.contains("name: String"));
    }

    #[test]
    fn locate_consts_by_pattern() {
        let mut locator = StructuralLocator::new().unwrap();
        let source = r#"
const STATSIG_API_KEY: &str = "key123";
const STATSIG_ENDPOINT: &str = "https://api.statsig.com";
const OTEL_ENABLED: bool = true;
"#;

        let results = locator.find_consts_matching(source, "^STATSIG_").unwrap();
        assert_eq!(results.len(), 2);

        let names: Vec<_> = results
            .iter()
            .map(|r| r.captures["name"].text.as_str())
            .collect();
        assert!(names.contains(&"STATSIG_API_KEY"));
        assert!(names.contains(&"STATSIG_ENDPOINT"));
    }

    #[test]
    fn locate_impl_block() {
        let mut locator = StructuralLocator::new().unwrap();
        let source = r#"
struct Foo;

impl Foo {
    fn new() -> Self {
        Foo
    }

    fn method(&self) -> i32 {
        42
    }
}
"#;

        let result = locator.find_impl(source, "Foo").unwrap();
        assert!(result.text.contains("impl Foo"));
        assert!(result.text.contains("fn new()"));
        assert!(result.text.contains("fn method(&self)"));
    }

    #[test]
    fn byte_span_accuracy() {
        let mut locator = StructuralLocator::new().unwrap();
        let source = "fn foo() {}\nfn bar() {}";

        let result = locator.find_function(source, "bar").unwrap();

        // Verify the byte span extracts exactly the function
        let extracted = &source[result.byte_start..result.byte_end];
        assert_eq!(extracted, "fn bar() {}");
    }
}
