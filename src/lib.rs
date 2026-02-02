//! Codex Patcher: Automated code patching system for Rust
//!
//! A robust patching system built on byte-span replacement primitives with
//! tree-sitter and ast-grep integration for structural code queries.
//!
//! # Architecture
//!
//! All edit operations compile down to a single primitive: [`Edit`], which
//! represents a verified byte-span replacement. Intelligence lives in span
//! acquisition (via tree-sitter, ast-grep, compiler diagnostics), not in
//! the application logic.
//!
//! # Safety
//!
//! - All edits verify expected before-text before applying
//! - Atomic file writes (tempfile + fsync + rename)
//! - Workspace boundary enforcement
//! - UTF-8 validation
//! - Idempotent operations
//!
//! # Example
//!
//! ```no_run
//! use codex_patcher::{Edit, EditVerification};
//! use std::path::PathBuf;
//!
//! let edit = Edit::new(
//!     PathBuf::from("src/main.rs"),
//!     0,
//!     5,
//!     "HELLO",
//!     "hello",
//! );
//!
//! match edit.apply() {
//!     Ok(result) => println!("Edit applied: {:?}", result),
//!     Err(e) => eprintln!("Edit failed: {}", e),
//! }
//! ```

pub mod cache;
pub mod compiler;
pub mod config;
pub mod edit;
pub mod pool;
pub mod safety;
pub mod sg;
pub mod toml;
pub mod ts;
pub mod validate;

// Re-exports
pub use config::{
    apply_patches, load_from_path, load_from_str, matches_requirement, ApplicationError,
    ConfigError, PatchConfig, PatchResult, VersionError,
};
pub use edit::{Edit, EditError, EditResult, EditVerification};
pub use safety::{SafetyError, WorkspaceGuard};
pub use ts::{
    LocatorResult, QueryEngine, QueryMatch, RustParser, StructuralLocator, StructuralTarget,
    TreeSitterError,
};
pub use validate::{
    syn_validate, ErrorLocation, ParseValidator, SelectorValidator, ValidatedEdit,
    ValidationError,
};
