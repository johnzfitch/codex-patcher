use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TomlError {
    #[error("invalid TOML syntax: {message}")]
    InvalidTomlSyntax { message: String },

    #[error("invalid section path '{input}': {message}")]
    InvalidSectionPath { input: String, message: String },

    #[error("section not found: {path}")]
    SectionNotFound { path: String },

    #[error("key not found: {section}.{key}")]
    KeyNotFound { section: String, key: String },

    #[error("ambiguous match for {kind}: {path}")]
    AmbiguousMatch { kind: String, path: String },

    #[error("invalid positioning: {message}")]
    InvalidPositioning { message: String },

    #[error("unsupported TOML construct: {message}")]
    Unsupported { message: String },

    #[error("toml edit would be a no-op: {reason}")]
    NoOp { reason: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("edit error: {0}")]
    Edit(#[from] crate::edit::EditError),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),
}
