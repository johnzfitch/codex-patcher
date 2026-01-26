use crate::ts::errors::TreeSitterError;
use crate::ts::parser::ParsedSource;
use ast_grep_language::{LanguageExt, SupportLang};
use std::collections::HashMap;
use tree_sitter::{Query, QueryCursor, StreamingIterator};

/// A match from a tree-sitter query with captured nodes.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The full match byte range
    pub byte_start: usize,
    pub byte_end: usize,
    /// Named captures: capture_name -> (byte_start, byte_end, text)
    pub captures: HashMap<String, CapturedNode>,
}

#[derive(Debug, Clone)]
pub struct CapturedNode {
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
    pub kind: String,
}

/// Engine for executing tree-sitter queries against parsed Rust source.
pub struct QueryEngine {
    query: Query,
    capture_names: Vec<String>,
}

impl QueryEngine {
    /// Create a new query engine from a tree-sitter query string.
    ///
    /// # Query Syntax
    ///
    /// Tree-sitter queries use S-expression syntax:
    /// ```text
    /// (function_item
    ///   name: (identifier) @func_name
    ///   body: (block) @body)
    /// ```
    ///
    /// Captures are prefixed with `@` and can be referenced by name.
    pub fn new(query_str: &str) -> Result<Self, TreeSitterError> {
        let language = SupportLang::Rust.get_ts_language();
        let query = Query::new(&language, query_str).map_err(|e| TreeSitterError::InvalidQuery {
            message: e.to_string(),
        })?;

        let capture_names = query.capture_names().iter().map(|s| s.to_string()).collect();

        Ok(Self {
            query,
            capture_names,
        })
    }

    /// Execute the query against parsed source and return all matches.
    pub fn find_all<'a>(&self, parsed: &'a ParsedSource<'a>) -> Vec<QueryMatch> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, parsed.root_node(), parsed.source.as_bytes());

        let mut results = Vec::new();

        // tree-sitter 0.25+ uses StreamingIterator
        while let Some(m) = matches.next() {
            let mut captures = HashMap::new();
            let mut overall_start = usize::MAX;
            let mut overall_end = 0usize;

            for capture in m.captures {
                let node = capture.node;
                let name = &self.capture_names[capture.index as usize];
                let text = parsed.node_text(node).to_string();

                overall_start = overall_start.min(node.start_byte());
                overall_end = overall_end.max(node.end_byte());

                captures.insert(
                    name.clone(),
                    CapturedNode {
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        text,
                        kind: node.kind().to_string(),
                    },
                );
            }

            if overall_start != usize::MAX {
                results.push(QueryMatch {
                    byte_start: overall_start,
                    byte_end: overall_end,
                    captures,
                });
            }
        }

        results
    }

    /// Execute the query and expect exactly one match.
    pub fn find_unique<'a>(
        &self,
        parsed: &'a ParsedSource<'a>,
    ) -> Result<QueryMatch, TreeSitterError> {
        let matches = self.find_all(parsed);

        match matches.len() {
            0 => Err(TreeSitterError::NoMatch),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => Err(TreeSitterError::AmbiguousMatch { count: n }),
        }
    }

    /// Get capture names defined in the query.
    pub fn capture_names(&self) -> &[String] {
        &self.capture_names
    }
}

/// Common tree-sitter queries for Rust constructs.
pub mod queries {
    /// Query for a function by name.
    pub fn function_by_name(name: &str) -> String {
        format!(
            r#"(function_item
                name: (identifier) @name
                (#eq? @name "{name}")
            ) @function"#
        )
    }

    /// Query for a function in an impl block.
    pub fn method_by_name(type_name: &str, method_name: &str) -> String {
        format!(
            r#"(impl_item
                type: (_) @type
                (#match? @type "{type_name}")
                body: (declaration_list
                    (function_item
                        name: (identifier) @method_name
                        (#eq? @method_name "{method_name}")
                    ) @method
                )
            )"#
        )
    }

    /// Query for a struct by name.
    pub fn struct_by_name(name: &str) -> String {
        format!(
            r#"(struct_item
                name: (type_identifier) @name
                (#eq? @name "{name}")
            ) @struct"#
        )
    }

    /// Query for an enum by name.
    pub fn enum_by_name(name: &str) -> String {
        format!(
            r#"(enum_item
                name: (type_identifier) @name
                (#eq? @name "{name}")
            ) @enum"#
        )
    }

    /// Query for a const item by name.
    pub fn const_by_name(name: &str) -> String {
        format!(
            r#"(const_item
                name: (identifier) @name
                (#eq? @name "{name}")
            ) @const"#
        )
    }

    /// Query for a static item by name.
    pub fn static_by_name(name: &str) -> String {
        format!(
            r#"(static_item
                name: (identifier) @name
                (#eq? @name "{name}")
            ) @static"#
        )
    }

    /// Query for an impl block by type name.
    pub fn impl_by_type(type_name: &str) -> String {
        format!(
            r#"(impl_item
                type: (type_identifier) @type
                (#eq? @type "{type_name}")
            ) @impl"#
        )
    }

    /// Query for an impl block with a trait.
    pub fn impl_trait_for_type(trait_name: &str, type_name: &str) -> String {
        format!(
            r#"(impl_item
                trait: (type_identifier) @trait
                (#eq? @trait "{trait_name}")
                type: (type_identifier) @type
                (#eq? @type "{type_name}")
            ) @impl"#
        )
    }

    /// Query for use statements matching a path pattern.
    pub fn use_declaration(path_pattern: &str) -> String {
        format!(
            r#"(use_declaration
                argument: (_) @path
                (#match? @path "{path_pattern}")
            ) @use"#
        )
    }

    /// Query for all functions in file.
    pub const ALL_FUNCTIONS: &str = r#"(function_item
        name: (identifier) @name
    ) @function"#;

    /// Query for all structs in file.
    pub const ALL_STRUCTS: &str = r#"(struct_item
        name: (type_identifier) @name
    ) @struct"#;

    /// Query for all impl blocks in file.
    pub const ALL_IMPLS: &str = r#"(impl_item
        type: (_) @type
    ) @impl"#;

    /// Query for const items matching a name pattern.
    pub fn const_matching(pattern: &str) -> String {
        format!(
            r#"(const_item
                name: (identifier) @name
                (#match? @name "{pattern}")
            ) @const"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ts::parser::RustParser;

    #[test]
    fn find_function_by_name() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
fn helper() {}

fn main() {
    helper();
}

fn other() {}
"#;
        let parsed = parser.parse_with_source(source).unwrap();
        let engine = QueryEngine::new(&queries::function_by_name("main")).unwrap();

        let matches = engine.find_all(&parsed);
        assert_eq!(matches.len(), 1);

        let m = &matches[0];
        assert!(m.captures.contains_key("name"));
        assert_eq!(m.captures["name"].text, "main");
    }

    #[test]
    fn find_struct_by_name() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
struct Foo {
    x: i32,
}

struct Bar;
"#;
        let parsed = parser.parse_with_source(source).unwrap();
        let engine = QueryEngine::new(&queries::struct_by_name("Foo")).unwrap();

        let m = engine.find_unique(&parsed).unwrap();
        assert_eq!(m.captures["name"].text, "Foo");
    }

    #[test]
    fn find_const_by_pattern() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
const STATSIG_API_KEY: &str = "secret";
const STATSIG_ENDPOINT: &str = "https://example.com";
const OTHER_CONST: i32 = 42;
"#;
        let parsed = parser.parse_with_source(source).unwrap();
        let engine = QueryEngine::new(&queries::const_matching("^STATSIG_")).unwrap();

        let matches = engine.find_all(&parsed);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn ambiguous_match_error() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
fn test() {}
fn test() {}
"#;
        let parsed = parser.parse_with_source(source).unwrap();
        let engine = QueryEngine::new(&queries::function_by_name("test")).unwrap();

        let result = engine.find_unique(&parsed);
        assert!(matches!(
            result,
            Err(TreeSitterError::AmbiguousMatch { count: 2 })
        ));
    }

    #[test]
    fn no_match_error() {
        let mut parser = RustParser::new().unwrap();
        let source = "fn main() {}";
        let parsed = parser.parse_with_source(source).unwrap();
        let engine = QueryEngine::new(&queries::function_by_name("nonexistent")).unwrap();

        let result = engine.find_unique(&parsed);
        assert!(matches!(result, Err(TreeSitterError::NoMatch)));
    }
}
