//! Fuzzy matching for patch text search
//!
//! Provides fallback matching when exact text search fails, using
//! normalized Levenshtein distance on a sliding window of lines.

use strsim::normalized_levenshtein;

/// Result of a fuzzy match operation
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    /// Byte offset where the match starts
    pub start: usize,
    /// Byte offset where the match ends
    pub end: usize,
    /// The actual text that was matched
    pub matched_text: String,
    /// Similarity score (0.0 to 1.0, higher is better)
    pub score: f64,
}

/// Find the best fuzzy match for `needle` in `haystack`.
///
/// Uses a sliding window of exactly `needle.lines().count()` lines.
/// Falls back gracefully: returns `None` when no window exceeds `threshold`.
///
/// # Arguments
/// * `needle` - The text pattern to search for
/// * `haystack` - The content to search within
/// * `threshold` - Minimum similarity score (0.0-1.0) to consider a match
pub fn find_best_match(needle: &str, haystack: &str, threshold: f64) -> Option<FuzzyMatch> {
    let window_size = needle.lines().count();
    let (lines, offsets) = build_haystack_info(haystack);
    slide_window(needle, haystack, &lines, &offsets, threshold, window_size)
}

/// Like `find_best_match` but tries window sizes `needle_lines..=needle_lines+max_expansion`.
///
/// Returns the highest-scoring match across all window sizes above `threshold`.
/// This handles cases where N lines were inserted inside the needle's span between
/// version bumps: a window larger than the original needle can still score highly
/// because the inserted lines are counted only once in the edit distance.
///
/// When `max_expansion == 0`, behaviour is identical to [`find_best_match`].
///
/// # Arguments
/// * `needle` - The text pattern to search for
/// * `haystack` - The content to search within
/// * `threshold` - Minimum similarity score (0.0-1.0) to consider a match
/// * `max_expansion` - Maximum extra lines to extend the window (capped at 200 by schema validation)
pub fn find_best_match_elastic(
    needle: &str,
    haystack: &str,
    threshold: f64,
    max_expansion: usize,
) -> Option<FuzzyMatch> {
    let base_lines = needle.lines().count();
    // Pre-compute once; all expansion iterations reuse the same slices.
    let (lines, offsets) = build_haystack_info(haystack);

    let mut best: Option<FuzzyMatch> = None;
    for expansion in 0..=max_expansion {
        let candidate =
            slide_window(needle, haystack, &lines, &offsets, threshold, base_lines + expansion);
        if let Some(c) = candidate {
            if best.as_ref().is_none_or(|b| c.score > b.score) {
                best = Some(c);
            }
        }
    }
    best
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Pre-compute the line slice vector and byte-offset table for `haystack`.
///
/// `line_offsets[i]` is the byte index of the start of line `i`.
/// The vector has one extra sentinel at the end pointing just past the last
/// character, which lets the window-end calculation avoid a bounds check.
fn build_haystack_info(haystack: &str) -> (Vec<&str>, Vec<usize>) {
    let lines: Vec<&str> = haystack.lines().collect();
    // split('\n') rather than lines() so we always produce the +1 sentinel entry
    // even when the file has a trailing newline.
    let offsets: Vec<usize> = std::iter::once(0)
        .chain(haystack.split('\n').scan(0usize, |acc, segment| {
            *acc += segment.len() + 1; // +1 for the '\n' character
            Some(*acc)
        }))
        .collect();
    (lines, offsets)
}

/// Slide a window of exactly `window_size` lines over `haystack_lines`,
/// scoring each window against `needle` with normalized Levenshtein distance.
/// Returns the best-scoring window that is at or above `threshold`.
fn slide_window(
    needle: &str,
    haystack: &str,
    haystack_lines: &[&str],
    line_offsets: &[usize],
    threshold: f64,
    window_size: usize,
) -> Option<FuzzyMatch> {
    if window_size == 0 || haystack_lines.len() < window_size {
        return None;
    }

    let mut best: Option<FuzzyMatch> = None;

    for i in 0..=haystack_lines.len().saturating_sub(window_size) {
        let window = haystack_lines[i..i + window_size].join("\n");
        let score = normalized_levenshtein(needle, &window);

        if score >= threshold && best.as_ref().is_none_or(|b| score > b.score) {
            let start = line_offsets[i];
            // End byte: start of the line *after* our window, minus the preceding '\n'.
            // The sentinel entry in `line_offsets` guarantees this index always exists.
            let end = if i + window_size < line_offsets.len() {
                line_offsets[i + window_size].saturating_sub(1)
            } else {
                haystack.len()
            };

            best = Some(FuzzyMatch {
                start,
                end,
                matched_text: window,
                score,
            });
        }
    }

    best
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── find_best_match ───────────────────────────────────────────────────────

    #[test]
    fn exact_match_scores_one() {
        let src = "fn main() {\n    println!(\"hello\");\n}";
        let m = find_best_match(src, src, 0.9).expect("should find exact match");
        assert!((m.score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn similar_match_above_threshold() {
        let haystack = "fn main() {\n    println!(\"hello world\");\n}";
        let needle = "fn main() {\n    println!(\"hello\");\n}";
        let m = find_best_match(needle, haystack, 0.8).expect("should find similar match");
        assert!(m.score > 0.8);
    }

    #[test]
    fn dissimilar_content_below_threshold() {
        let haystack = "completely different content";
        let needle = "fn main() {}";
        assert!(find_best_match(needle, haystack, 0.8).is_none());
    }

    #[test]
    fn selects_best_match_among_candidates() {
        let haystack = "fn foo() {}\nfn bar() {\n    x\n}\nfn baz() {\n    y\n}";
        let needle = "fn bar() {\n    x\n}";
        let m = find_best_match(needle, haystack, 0.9).expect("should match");
        assert!(m.matched_text.contains("bar"));
    }

    #[test]
    fn byte_positions_are_correct() {
        let haystack = "line1\nline2\nline3";
        let needle = "line2";
        let m = find_best_match(needle, haystack, 0.9).expect("should match");
        assert_eq!(m.start, 6, "\"line1\\n\" is 6 bytes");
        assert_eq!(&haystack[m.start..m.end], "line2");
    }

    #[test]
    fn trailing_newline_does_not_corrupt_offsets() {
        let haystack = "alpha\nbeta\n";
        let needle = "beta";
        let m = find_best_match(needle, haystack, 0.9).expect("should match");
        assert_eq!(&haystack[m.start..m.end], "beta");
    }

    // ── find_best_match_elastic ───────────────────────────────────────────────

    #[test]
    fn elastic_zero_expansion_matches_fixed_window() {
        let needle = "fn foo() {}";
        let haystack = "fn foo() {}\nfn bar() {}";
        let fixed = find_best_match(needle, haystack, 0.9);
        let elastic = find_best_match_elastic(needle, haystack, 0.9, 0);
        match (fixed, elastic) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                assert_eq!(a.start, b.start);
                assert_eq!(a.end, b.end);
                assert!((a.score - b.score).abs() < 1e-9);
            }
            (a, b) => panic!("results differ: fixed={a:?} elastic={b:?}"),
        }
    }

    #[test]
    fn elastic_finds_needle_with_inserted_lines() {
        // When one line is inserted between two anchor lines, no fixed-size window
        // contains both anchors.  An elastic window of size needle_lines+1 does,
        // and scores higher than any fixed window.
        //
        // Verified scores (strsim::normalized_levenshtein):
        //   fixed w2[0..2] = 0.458  ("TOP_ANCHOR\nINSERTED_LINE")
        //   fixed w2[1..3] = 0.519  ("INSERTED_LINE\nBOTTOM_ANCHOR")  ← best fixed
        //   elastic w3[0..3] = 0.632 ("TOP_ANCHOR\nINSERTED_LINE\nBOTTOM_ANCHOR")
        //
        // At threshold=0.55: fixed → None, elastic → Some.
        let needle = "TOP_ANCHOR\nBOTTOM_ANCHOR";
        let haystack = "TOP_ANCHOR\nINSERTED_LINE\nBOTTOM_ANCHOR\nextra";

        assert!(
            find_best_match(needle, haystack, 0.55).is_none(),
            "fixed window (best score 0.519) should miss at threshold 0.55"
        );

        let m = find_best_match_elastic(needle, haystack, 0.55, 1)
            .expect("elastic (best score 0.632) should find match at threshold 0.55");
        assert!(m.matched_text.contains("TOP_ANCHOR"), "match must contain opening anchor");
        assert!(
            m.matched_text.contains("BOTTOM_ANCHOR"),
            "match must contain closing anchor"
        );
    }

    #[test]
    fn elastic_picks_highest_score_across_expansions() {
        // A perfect-match window exists at expansion=0; larger windows should
        // score lower and not displace it.
        let needle = "fn exact() {}";
        let haystack = "preamble\nfn exact() {}\npostamble\nmore stuff\n";
        let m = find_best_match_elastic(needle, haystack, 0.9, 5)
            .expect("should find exact match");
        assert!((m.score - 1.0).abs() < 1e-9, "perfect match should win");
        assert_eq!(&haystack[m.start..m.end], "fn exact() {}");
    }

    #[test]
    fn elastic_returns_none_when_nothing_exceeds_threshold() {
        let needle = "fn absolutely_nothing_like_this() {}";
        let haystack = "let a = 1;\nlet b = 2;\nlet c = 3;\n";
        assert!(find_best_match_elastic(needle, haystack, 0.85, 10).is_none());
    }

    #[test]
    fn elastic_handles_haystack_shorter_than_window() {
        let needle = "a\nb\nc\nd\ne";
        let haystack = "a\nb";
        // haystack has 2 lines, needle has 5 — no window fits even at expansion=0
        assert!(find_best_match_elastic(needle, haystack, 0.5, 20).is_none());
    }
}
