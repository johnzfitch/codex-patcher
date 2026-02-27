//! Thread-local pattern compilation cache for ast-grep patterns.
//!
//! Caches compiled ast-grep patterns to avoid redundant recompilation.
//! Provides 5-10x speedup for repetitive pattern usage.
//! Cache is capped at 256 entries; oldest entries are evicted when full.

use ast_grep_core::Pattern;
use ast_grep_language::SupportLang;
use std::cell::RefCell;
use std::collections::HashMap;

const MAX_CACHE_ENTRIES: usize = 256;

thread_local! {
    // Key is "<lang_debug>:<pattern_str>" so same pattern string for different
    // languages never collides (e.g., Rust vs Python).
    static PATTERN_CACHE: RefCell<HashMap<String, Pattern>> =
        RefCell::new(HashMap::new());
}

/// Get a compiled pattern from cache, or compile and cache it.
///
/// Patterns are cached thread-locally, capped at 256 entries.
/// When the cap is reached, the cache is cleared and rebuilt on demand.
/// Cache hits provide ~10x speedup over recompilation.
pub fn get_or_compile_pattern(pattern_str: &str, lang: SupportLang) -> Pattern {
    // Include lang in key: same pattern string for different languages must not
    // collide (e.g., `$FOO` means different things in Rust vs Python).
    let cache_key = format!("{lang:?}:{pattern_str}");

    PATTERN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();

        // Check if pattern is already compiled
        if let Some(p) = cache.get(&cache_key) {
            return p.clone();
        }

        // Evict all if at capacity (simple but effective for batch workloads)
        if cache.len() >= MAX_CACHE_ENTRIES {
            cache.clear();
        }

        // Compile and cache the pattern
        let compiled = Pattern::new(pattern_str, lang);
        cache.insert(cache_key, compiled.clone());
        compiled
    })
}

/// Clear the pattern cache (mainly for testing).
pub fn clear_cache() {
    PATTERN_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

/// Get cache statistics for monitoring.
pub fn cache_size() -> usize {
    PATTERN_CACHE.with(|cache| cache.borrow().len())
}
