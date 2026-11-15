//! Fuzzy matching for the quick switcher.
//!
//! Typing `mtn` should find `Meeting Notes.md`. The matcher walks the
//! candidate for the query's characters in order, rewarding characters that
//! follow the previous match and characters that start a word, and rewarding
//! matches that begin early in the name. Scores are only comparable within one
//! query; the absolute value means nothing.
//!
//! It is deliberately small: no allocation inside the hot loop, no dependency,
//! and deterministic tie-breaking by path so the list does not jitter between
//! frames.

use std::path::{Path, PathBuf};

/// Score for a match at the start of a word or path segment.
const BOUNDARY_BONUS: i32 = 10;
/// Score for a character that directly follows the previous match.
const CONSECUTIVE_BONUS: i32 = 6;
/// Score for any matched character.
const MATCH_SCORE: i32 = 2;
/// Cost per character skipped before the first match.
const FIRST_MATCH_PENALTY: i32 = 1;
/// Cost per unmatched character skipped inside a match run.
const GAP_PENALTY: i32 = 1;

/// Score `query` against `candidate`, or `None` when it does not match.
///
/// Matching is case-insensitive and works on characters, not bytes, so a
/// non-ASCII note name cannot split a code point. An empty query matches
/// everything with a score of zero.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let candidate_chars: Vec<char> = candidate.chars().collect();
    let lowered: Vec<char> = candidate_chars
        .iter()
        .flat_map(|c| c.to_lowercase())
        .collect();

    // The lowered string can be longer than the original when a character
    // lowercases into several, so walk the lowered form and map matches back
    // by position in that form; for the boundary check the original is enough.
    let mut needle = 0;
    let mut first_match: Option<usize> = None;
    let mut previous_match: Option<usize> = None;
    let mut score = 0;
    let mut gaps: i32 = 0;

    for (index, ch) in lowered.iter().enumerate() {
        if needle == query.len() {
            break;
        }
        if *ch != query[needle] {
            if first_match.is_some() {
                gaps += 1;
            }
            continue;
        }

        if first_match.is_none() {
            first_match = Some(index);
        }
        score += MATCH_SCORE;
        if previous_match.is_some_and(|previous| previous + 1 == index) {
            score += CONSECUTIVE_BONUS;
        }
        if is_boundary(&candidate_chars, index) {
            score += BOUNDARY_BONUS;
        }
        previous_match = Some(index);
        needle += 1;
    }

    if needle < query.len() {
        return None;
    }

    let leading = first_match.map_or(0, |index| i32::try_from(index).unwrap_or(i32::MAX / 4));
    score -= leading * FIRST_MATCH_PENALTY;
    score -= gaps * GAP_PENALTY;
    Some(score)
}

/// Whether the character at `index` starts a word, path segment, or the name.
fn is_boundary(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    matches!(chars[index - 1], '/' | '\\' | ' ' | '-' | '_' | '.')
}

/// A note and its match score, newest query first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    /// Vault-relative path.
    pub path: PathBuf,
    /// Match score; higher is better.
    pub score: i32,
}

/// Rank `paths` against `query`, best first.
///
/// Returns at most `limit` results. Ties break on the path so the order is
/// stable, and duplicates are collapsed.
pub fn rank<'a>(
    query: &str,
    paths: impl IntoIterator<Item = &'a Path>,
    limit: usize,
) -> Vec<Ranked> {
    let mut ranked: Vec<Ranked> = Vec::new();
    for path in paths {
        let Some(display) = path.to_str() else {
            continue;
        };
        if let Some(score) = score(query, display) {
            ranked.push(Ranked {
                path: path.to_path_buf(),
                score,
            });
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
    });
    ranked.dedup_by(|left, right| left.path == right.path);
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "Meeting Notes.md"), Some(0));
    }

    #[test]
    fn subsequence_matches_case_insensitively() {
        assert!(score("mtn", "Meeting Notes.md").is_some());
        assert!(score("MTN", "meeting notes.md").is_some());
        assert!(score("mtn", "Meeting Notes.md").is_some());
    }

    #[test]
    fn out_of_order_characters_do_not_match() {
        assert_eq!(score("ntx", "Meeting Notes.md"), None);
        assert_eq!(score("xyz", "Meeting Notes.md"), None);
    }

    #[test]
    fn a_missing_character_fails_the_match() {
        assert_eq!(score("meetingnotesx", "Meeting Notes.md"), None);
    }

    #[test]
    fn a_word_boundary_match_beats_a_scattered_one() {
        let boundary = score("mn", "Meeting Notes.md").expect("match");
        let scattered = score("mn", "amounted.md").expect("match");
        assert!(
            boundary > scattered,
            "boundary {boundary} should beat scattered {scattered}"
        );
    }

    #[test]
    fn an_early_match_beats_a_late_one() {
        let early = score("note", "note.md").expect("match");
        let late = score("note", "archive/old-note.md").expect("match");
        assert!(early > late, "early {early} should beat late {late}");
    }

    #[test]
    fn ranking_puts_the_exact_prefix_first() {
        let paths = [
            Path::new("archive/weekly-plan.md"),
            Path::new("plan.md"),
            Path::new("notes/planning.md"),
        ];
        let ranked = rank("plan", paths, 10);
        assert_eq!(ranked[0].path, PathBuf::from("plan.md"));
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn ranking_is_limited_and_stable() {
        let paths = [Path::new("b.md"), Path::new("a.md"), Path::new("c.md")];
        let ranked = rank("", paths, 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].path, PathBuf::from("a.md"), "ties break on path");
        assert_eq!(ranked[1].path, PathBuf::from("b.md"));
    }

    #[test]
    fn non_ascii_names_match_without_panicking() {
        assert!(score("café", "notes/café.md").is_some());
        assert!(score("日本", "日本/メモ.md").is_some());
        assert_eq!(score("zzz", "café.md"), None);
    }
}
