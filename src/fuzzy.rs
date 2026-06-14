//! Matching for the jump picker and dashboard `/` filter.
//!
//! A query matches when it is either a contiguous (case-insensitive)
//! substring of the haystack, or an acronym — a subsequence of the
//! haystack's word-initial letters. Plain scattered subsequence is
//! deliberately *not* a match: it's too loose (e.g. "clu" is a subsequence
//! of "claude", so every Claude pane would match a "clu" search).

/// Returns `Some(0)` when `needle` matches `haystack`, `None` otherwise.
/// Empty needle matches everything. The score value is unused (membership
/// only); callers order results themselves.
pub fn score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let needle = needle.to_lowercase();
    if haystack.to_lowercase().contains(&needle) {
        return Some(0);
    }
    if acronym_match(haystack, &needle) {
        return Some(0);
    }
    None
}

/// Lenient match for the Overview tab: a case-insensitive substring of the
/// project's full text. Looser than the strict name matcher above (it
/// searches the whole description, not just names), but still anchored — a
/// scattered subsequence over a long paragraph would match almost anything
/// ("pine" appears in-order in most prose).
pub fn loose_match(haystack: &str, needle: &str) -> bool {
    needle.is_empty() || haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// `needle` (already lowercased) is a subsequence of the haystack's
/// word-initial letters — e.g. "ho" matches "helion-orbit".
fn acronym_match(haystack: &str, needle: &str) -> bool {
    let chars: Vec<char> = haystack.chars().collect();
    let mut initials = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() && is_word_start(&chars, i) {
            initials.extend(c.to_lowercase());
        }
    }
    is_subsequence(&initials, needle)
}

fn is_subsequence(hay: &str, needle: &str) -> bool {
    let mut chars = hay.chars();
    needle.chars().all(|nc| chars.any(|hc| hc == nc))
}

/// A char is a word start if it's the first char, follows a separator, or
/// is an uppercase char after a lowercase one (camelCase boundary).
fn is_word_start(hay: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = hay[i - 1];
    if matches!(prev, '/' | '-' | '_' | ' ' | '.' | ':') {
        return true;
    }
    hay[i].is_uppercase() && prev.is_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle_matches() {
        assert!(score("anything", "").is_some());
    }

    #[test]
    fn substring_matches_but_scattered_does_not() {
        assert!(score("clublasanta", "clu").is_some());
        // The regression: "clu" is a scattered subsequence of "claude" but
        // not a substring, so it must NOT match.
        assert!(score("claude", "clu").is_none());
    }

    #[test]
    fn case_insensitive_substring() {
        assert!(score("HelionOrbit", "helion").is_some());
        assert!(score("helionorbit", "ORBIT").is_some());
    }

    #[test]
    fn acronym_of_word_initials() {
        assert!(score("helion-orbit", "ho").is_some());
        assert!(score("cc-canary-llm main", "clm").is_some());
        assert!(score("helion-orbit", "hx").is_none());
    }

    #[test]
    fn plain_miss() {
        assert!(score("helion-orbit", "zzz").is_none());
    }

    #[test]
    fn loose_is_substring_not_scattered() {
        assert!(loose_match("pine-strategy-tests runner parity", "pine"));
        assert!(loose_match("Postgres Proposed PGlite quarantine", "post"));
        // The regression: "pine" is a scattered subsequence of this prose
        // but not a substring, so it must NOT match.
        assert!(!loose_match("Postgres Proposed PGlite quarantine", "pine"));
        assert!(loose_match("anything", ""));
    }
}
