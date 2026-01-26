//! Tree-sitter integration for structural Rust code queries.
//!
//! This module provides CST-based span location using tree-sitter, enabling
//! precise byte-span extraction for Rust code constructs without losing
//! comments or formatting.

pub mod errors;
pub mod locator;
pub mod parser;
pub mod query;
pub mod validator;

pub use errors::TreeSitterError;
pub use locator::{LocatorResult, StructuralLocator, StructuralTarget};
pub use parser::{ParsedSource, RustParser};
pub use query::{QueryEngine, QueryMatch};
pub use validator::validate_syntax;
