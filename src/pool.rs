//! Thread-local parser pooling for performance optimization.
//!
//! Eliminates redundant parser creation by maintaining a thread-local pool
//! of reusable parsers. Creates new parser on first use per thread, reuses
//! for subsequent operations.

use crate::ts::{RustParser, TreeSitterError};
use std::cell::RefCell;

thread_local! {
    static RUST_PARSER: RefCell<Option<RustParser>> = const { RefCell::new(None) };
}

/// Execute function with pooled parser instance.
///
/// On first call per thread, creates new parser. Subsequent calls reuse
/// the same parser instance, avoiding allocation and initialization overhead.
///
/// # Example
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use codex_patcher::pool::with_parser;
///
/// let result = with_parser(|parser| {
///     parser.parse_with_source("fn main() {}")
/// })?;
/// # Ok(())
/// # }
/// ```
pub fn with_parser<F, R>(f: F) -> Result<R, TreeSitterError>
where
    F: FnOnce(&mut RustParser) -> R,
{
    RUST_PARSER.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(RustParser::new()?);
        }
        Ok(f(opt.as_mut().expect("parser was just initialized above")))
    })
}
