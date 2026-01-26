use crate::ts::errors::TreeSitterError;
use ast_grep_language::{LanguageExt, SupportLang};
use tree_sitter::{Parser, Tree};

/// Rust edition for grammar compatibility checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RustEdition {
    E2015,
    E2018,
    #[default]
    E2021,
    E2024,
}

impl RustEdition {
    /// Parse edition from Cargo.toml edition string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "2015" => Some(RustEdition::E2015),
            "2018" => Some(RustEdition::E2018),
            "2021" => Some(RustEdition::E2021),
            "2024" => Some(RustEdition::E2024),
            _ => None,
        }
    }
}

/// Tree-sitter parser wrapper for Rust source code.
pub struct RustParser {
    parser: Parser,
    edition: RustEdition,
}

impl RustParser {
    /// Create a new Rust parser with the default edition (2021).
    pub fn new() -> Result<Self, TreeSitterError> {
        Self::with_edition(RustEdition::default())
    }

    /// Create a new Rust parser targeting a specific edition.
    pub fn with_edition(edition: RustEdition) -> Result<Self, TreeSitterError> {
        let mut parser = Parser::new();
        // Get the tree-sitter Language from ast-grep-language
        let ts_lang = SupportLang::Rust.get_ts_language();
        parser
            .set_language(&ts_lang)
            .map_err(|_| TreeSitterError::LanguageSet)?;

        Ok(Self { parser, edition })
    }

    /// Get the configured edition.
    pub fn edition(&self) -> RustEdition {
        self.edition
    }

    /// Parse source code into a tree-sitter Tree.
    pub fn parse(&mut self, source: &str) -> Result<Tree, TreeSitterError> {
        self.parser
            .parse(source, None)
            .ok_or(TreeSitterError::ParseFailed)
    }

    /// Parse source code and return the tree along with the source.
    pub fn parse_with_source<'a>(
        &mut self,
        source: &'a str,
    ) -> Result<ParsedSource<'a>, TreeSitterError> {
        let tree = self.parse(source)?;
        Ok(ParsedSource { source, tree })
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new().expect("failed to create default RustParser")
    }
}

/// A parsed source file with its tree-sitter tree.
pub struct ParsedSource<'a> {
    pub source: &'a str,
    pub tree: Tree,
}

impl<'a> ParsedSource<'a> {
    /// Get the root node of the tree.
    pub fn root_node(&self) -> tree_sitter::Node<'_> {
        self.tree.root_node()
    }

    /// Check if the tree contains any ERROR nodes.
    pub fn has_errors(&self) -> bool {
        has_error_nodes(self.tree.root_node())
    }

    /// Get all ERROR nodes in the tree.
    pub fn error_nodes(&self) -> Vec<ErrorNode> {
        let mut errors = Vec::new();
        collect_error_nodes(self.tree.root_node(), &mut errors);
        errors
    }

    /// Extract text for a node's byte range.
    pub fn node_text(&self, node: tree_sitter::Node<'_>) -> &'a str {
        &self.source[node.byte_range()]
    }
}

/// Information about an ERROR node in the parse tree.
#[derive(Debug, Clone)]
pub struct ErrorNode {
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_point: tree_sitter::Point,
    pub end_point: tree_sitter::Point,
}

fn has_error_nodes(node: tree_sitter::Node<'_>) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(child) {
            return true;
        }
    }

    false
}

fn collect_error_nodes(node: tree_sitter::Node<'_>, errors: &mut Vec<ErrorNode>) {
    if node.is_error() || node.is_missing() {
        errors.push(ErrorNode {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_point: node.start_position(),
            end_point: node.end_position(),
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_error_nodes(child, errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_rust() {
        let mut parser = RustParser::new().unwrap();
        let source = "fn main() { println!(\"hello\"); }";
        let parsed = parser.parse_with_source(source).unwrap();

        assert!(!parsed.has_errors());
        assert_eq!(parsed.root_node().kind(), "source_file");
    }

    #[test]
    fn parse_invalid_rust() {
        let mut parser = RustParser::new().unwrap();
        let source = "fn main( { }";
        let parsed = parser.parse_with_source(source).unwrap();

        assert!(parsed.has_errors());
        assert!(!parsed.error_nodes().is_empty());
    }

    #[test]
    fn edition_parsing() {
        assert_eq!(RustEdition::parse("2021"), Some(RustEdition::E2021));
        assert_eq!(RustEdition::parse("2024"), Some(RustEdition::E2024));
        assert_eq!(RustEdition::parse("invalid"), None);
    }
}
