//! Find and replace inside one note.
//!
//! The offsets returned here are byte offsets into the original text, so they
//! can be used to splice a buffer without re-walking it. Matching is
//! case-insensitive by default and does not allocate per hit.
//!
//! Case-insensitive search lowercases the haystack and keeps a map from every
//! byte of the lowered text back to a byte offset in the original. A naive
//! `to_lowercase()` would report offsets into the lowered copy, which drift as
//! soon as a character changes length (`İ` lowercases to two characters).

/// One match, as a half-open byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// First byte of the match.
    pub start: usize,
    /// One past the last byte of the match.
    pub end: usize,
}

impl Match {
    /// Length of the match in bytes.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether the match is empty.
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// Every occurrence of `needle` in `haystack`.
///
/// An empty needle matches nothing: a find bar with no query should not
/// highlight the whole buffer. Matches do not overlap.
pub fn find_all(haystack: &str, needle: &str, match_case: bool) -> Vec<Match> {
    if needle.is_empty() {
        return Vec::new();
    }
    if match_case {
        return haystack
            .match_indices(needle)
            .map(|(start, found)| Match {
                start,
                end: start + found.len(),
            })
            .collect();
    }

    let (lowered, map) = lowered_with_map(haystack);
    let needle = needle.to_lowercase();
    lowered
        .match_indices(&needle)
        .filter_map(|(start, found)| {
            let hay_start = *map.get(start)?;
            let hay_end = *map.get(start + found.len())?;
            Some(Match {
                start: hay_start,
                end: hay_end,
            })
        })
        .collect()
}

/// Replace every occurrence, returning the new text and how many were replaced.
pub fn replace_all(
    haystack: &str,
    needle: &str,
    replacement: &str,
    match_case: bool,
) -> (String, usize) {
    let hits = find_all(haystack, needle, match_case);
    if hits.is_empty() {
        return (haystack.to_string(), 0);
    }

    let mut result = String::with_capacity(haystack.len());
    let mut cursor = 0;
    for hit in &hits {
        result.push_str(&haystack[cursor..hit.start]);
        result.push_str(replacement);
        cursor = hit.end;
    }
    result.push_str(&haystack[cursor..]);
    (result, hits.len())
}

/// The hit to select next, given the current caret offset.
///
/// Search wraps: past the last hit comes the first one, and before the first
/// comes the last, which is what users expect from `Ctrl+F` in a text editor.
pub fn next_index(hits: &[Match], cursor: usize, forward: bool) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    if forward {
        hits.iter().position(|hit| hit.start > cursor).or(Some(0))
    } else {
        hits.iter()
            .rposition(|hit| hit.start < cursor)
            .or(Some(hits.len() - 1))
    }
}

/// Lowercase `haystack` and record where every lowered byte came from.
fn lowered_with_map(haystack: &str) -> (String, Vec<usize>) {
    let mut lowered = String::with_capacity(haystack.len());
    let mut map = Vec::with_capacity(haystack.len() + 1);
    for (offset, ch) in haystack.char_indices() {
        for lower in ch.to_lowercase() {
            let before = lowered.len();
            lowered.push(lower);
            for _ in before..lowered.len() {
                map.push(offset);
            }
        }
    }
    map.push(haystack.len());
    (lowered, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(haystack: &str, needle: &str, match_case: bool) -> Vec<(usize, usize)> {
        find_all(haystack, needle, match_case)
            .into_iter()
            .map(|hit| (hit.start, hit.end))
            .collect()
    }

    #[test]
    fn finds_every_occurrence() {
        assert_eq!(ranges("one two one", "one", true), vec![(0, 3), (8, 11)]);
    }

    #[test]
    fn an_empty_needle_matches_nothing() {
        assert!(find_all("anything", "", true).is_empty());
    }

    #[test]
    fn matching_is_case_insensitive_by_default() {
        assert_eq!(
            ranges("Note note NOTE", "note", false),
            vec![(0, 4), (5, 9), (10, 14)]
        );
        assert_eq!(ranges("Note note NOTE", "note", true), vec![(5, 9)]);
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(ranges("aaaa", "aa", true), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn offsets_are_bytes_into_the_original_text() {
        let text = "héllo héllo";
        let hits = find_all(text, "héllo", false);
        assert_eq!(hits.len(), 2);
        assert_eq!(&text[hits[1].start..hits[1].end], "héllo");
    }

    #[test]
    fn a_case_fold_that_changes_length_keeps_offsets_valid() {
        // 'İ' (U+0130) lowercases to two characters.
        let text = "İstanbul and İzmir";
        let hits = find_all(text, "zmir", false);
        assert_eq!(hits.len(), 1);
        assert_eq!(&text[hits[0].start..hits[0].end], "zmir");

        let hits = find_all(text, "İzmir", false);
        assert_eq!(hits.len(), 1, "the multi-character fold still matches");
        assert_eq!(&text[hits[0].start..hits[0].end], "İzmir");
    }

    #[test]
    fn replace_all_rebuilds_the_text_and_counts_hits() {
        let (text, count) = replace_all("a cat and a Cat", "cat", "dog", false);
        assert_eq!(text, "a dog and a dog");
        assert_eq!(count, 2);
    }

    #[test]
    fn replace_all_without_a_match_changes_nothing() {
        let (text, count) = replace_all("a cat", "bird", "", false);
        assert_eq!(text, "a cat");
        assert_eq!(count, 0);
    }

    #[test]
    fn next_index_moves_forward_and_wraps() {
        let hits = find_all("a b a b a", "a", true);
        assert_eq!(next_index(&hits, 0, true), Some(1));
        assert_eq!(next_index(&hits, 9, true), Some(0), "wraps to the first");
        assert_eq!(next_index(&hits, 0, false), Some(2), "wraps backwards");
        assert_eq!(next_index(&hits, 8, false), Some(1));
        assert_eq!(next_index(&[], 0, true), None);
    }
}
