use thiserror::Error;

#[derive(Error, Debug)]
pub enum AstGrepError {
    #[error("invalid pattern: {message}")]
    InvalidPattern { message: String },

    #[error("pattern matched {count} locations, expected exactly 1")]
    AmbiguousMatch { count: usize },

    #[error("pattern matched 0 locations")]
    NoMatch,

    #[error("metavariable '{name}' not found in match")]
    MetavarNotFound { name: String },

    #[error("replacement would create invalid syntax")]
    InvalidReplacement,

    #[error("context constraint not satisfied: {message}")]
    ContextNotSatisfied { message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
