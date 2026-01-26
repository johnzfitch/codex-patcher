//! ast-grep integration for pattern-based Rust code matching.
//!
//! This module provides high-level pattern matching using ast-grep's
//! metavariable syntax ($NAME, $$$BODY, etc.) for structural code search
//! and replacement.

pub mod errors;
pub mod lang;
pub mod matcher;
pub mod replacer;

pub use errors::AstGrepError;
pub use lang::{rust, SupportLang};
pub use matcher::{PatternMatch, PatternMatcher};
pub use replacer::{CaptureReplacer, Replacement};
