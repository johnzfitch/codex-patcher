use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TreeSitterError {
    #[error("failed to initialize tree-sitter parser")]
    ParserInit,

    #[error("failed to set language for parser")]
    LanguageSet,

    #[error("failed to parse source code")]
    ParseFailed,

    #[error("invalid tree-sitter query: {message}")]
    InvalidQuery { message: String },

    #[error("query matched {count} locations, expected exactly 1")]
    AmbiguousMatch { count: usize },

    #[error("query matched 0 locations")]
    NoMatch,

    #[error("target not found: {target}")]
    TargetNotFound { target: String },

    #[error("syntax error detected at byte {byte_start}..{byte_end}")]
    SyntaxError { byte_start: usize, byte_end: usize },

    #[error("multiple syntax errors detected: {count} ERROR nodes")]
    MultipleSyntaxErrors { count: usize },

    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("capture '{name}' not found in query matches")]
    CaptureNotFound { name: String },
}
