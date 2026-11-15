//! Headings for the outline panel.
//!
//! The outline only needs the ATX headings (`# Title`) with their level and
//! position, so this is a line scan rather than a Markdown parse. It skips
//! YAML frontmatter and fenced code blocks, because a `#` in a shell snippet is
//! a comment, not a heading, and it follows the `CommonMark` details that matter
//! in practice: up to three spaces of indent, one to six `#`, a space after the
//! marker, and an optional closing run of `#`.
//!
//! Setext headings (a `===` or `---` underline) are deliberately not detected:
//! `---` is also frontmatter and a horizontal rule, and a false heading is
//! worse in an outline than a missing one.

/// One heading in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Heading level, 1 to 6.
    pub level: u8,
    /// Heading text, without the `#` markers or closing run.
    pub text: String,
    /// Zero-based line index.
    pub line: usize,
    /// Byte offset of the start of the heading line.
    pub offset: usize,
}

impl Heading {
    /// The heading text, or a placeholder when the heading is empty.
    pub fn label(&self) -> &str {
        if self.text.is_empty() {
            "(untitled)"
        } else {
            &self.text
        }
    }
}

/// Every heading in `markdown`, in document order.
pub fn outline(markdown: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut in_frontmatter = false;
    let mut offset = 0;

    for (line_number, line) in markdown.split_inclusive('\n').enumerate() {
        let line_start = offset;
        offset += line.len();
        let text = line.trim_end_matches(['\r', '\n']);

        if line_number == 0 && text.trim_end() == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if text.trim_end() == "---" {
                in_frontmatter = false;
            }
            continue;
        }

        if let Some((marker, length)) = fence {
            if let Some((closing, closing_length)) = fence_marker(text)
                && closing == marker
                && closing_length >= length
            {
                fence = None;
            }
            continue;
        }
        if let Some(opening) = fence_marker(text) {
            fence = Some(opening);
            continue;
        }

        if let Some((level, heading)) = parse_atx(text) {
            headings.push(Heading {
                level,
                text: heading.to_string(),
                line: line_number,
                offset: line_start,
            });
        }
    }

    headings
}

/// Index of the heading that contains `offset`, i.e. the last heading at or
/// before it.
///
/// This is what drives the active-heading highlight: the caret sits under one
/// heading until it passes the next one.
pub fn heading_at(headings: &[Heading], offset: usize) -> Option<usize> {
    headings
        .iter()
        .rposition(|heading| heading.offset <= offset)
}

/// Parse a single ATX heading line, returning its level and text.
fn parse_atx(line: &str) -> Option<(u8, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|byte| *byte == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    // `#tag` is not a heading; CommonMark requires a space or the line end.
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "hashes is 1..=6, so u8 is exact"
    )]
    Some((hashes as u8, strip_closing_hashes(after.trim())))
}

/// Drop a trailing run of `#` that is separated from the text by whitespace.
fn strip_closing_hashes(text: &str) -> &str {
    if !text.ends_with('#') {
        return text;
    }
    let without = text.trim_end_matches('#');
    if without.trim_end().is_empty() {
        // A heading made only of markers (`## ##`) has no text.
        return "";
    }
    if without.ends_with([' ', '\t']) {
        without.trim_end()
    } else {
        // `# C#` keeps its hash: a closing run must follow whitespace.
        text
    }
}

/// Opening or closing code-fence marker and its run length.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let length = rest.chars().take_while(|c| *c == marker).count();
    if length < 3 {
        return None;
    }
    Some((marker, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(markdown: &str) -> Vec<(u8, String, usize)> {
        outline(markdown)
            .into_iter()
            .map(|heading| (heading.level, heading.text, heading.line))
            .collect()
    }

    #[test]
    fn extracts_levels_and_text() {
        let markdown = "# One\n\n## Two\n\ntext\n\n###### Six\n";
        assert_eq!(
            texts(markdown),
            vec![
                (1, "One".to_string(), 0),
                (2, "Two".to_string(), 2),
                (6, "Six".to_string(), 6),
            ]
        );
    }

    #[test]
    fn offsets_point_at_the_start_of_each_line() {
        let markdown = "# One\n\n## Two\n";
        let headings = outline(markdown);
        assert_eq!(&markdown[headings[0].offset..], "# One\n\n## Two\n");
        assert_eq!(&markdown[headings[1].offset..], "## Two\n");
    }

    #[test]
    fn a_hash_without_a_space_is_a_tag_not_a_heading() {
        assert!(outline("#tag\n").is_empty());
        assert!(outline("####### seven\n").is_empty());
    }

    #[test]
    fn closing_hashes_are_stripped_only_after_whitespace() {
        assert_eq!(texts("## Title ##\n")[0].1, "Title");
        assert_eq!(texts("# C#\n")[0].1, "C#");
        assert_eq!(texts("## ##\n")[0].1, "");
        assert_eq!(outline("## ##\n")[0].label(), "(untitled)");
    }

    #[test]
    fn up_to_three_spaces_of_indent_are_allowed() {
        assert_eq!(texts("   # Three\n")[0].1, "Three");
        assert!(outline("    # Four\n").is_empty());
    }

    #[test]
    fn fenced_code_blocks_are_skipped() {
        let markdown = "# Real\n\n```sh\n# not a heading\n```\n\n## Also real\n";
        assert_eq!(
            texts(markdown),
            vec![(1, "Real".to_string(), 0), (2, "Also real".to_string(), 6),]
        );
    }

    #[test]
    fn tilde_fences_close_on_their_own_marker() {
        let markdown = "~~~\n# hidden\n~~~\n# shown\n";
        assert_eq!(texts(markdown), vec![(1, "shown".to_string(), 3)]);
    }

    #[test]
    fn a_longer_closing_fence_still_closes() {
        let markdown = "````\n# hidden\n`````\n# shown\n";
        assert_eq!(texts(markdown), vec![(1, "shown".to_string(), 3)]);
    }

    #[test]
    fn an_unclosed_fence_hides_the_rest() {
        let markdown = "# before\n```\n# after\n";
        assert_eq!(texts(markdown), vec![(1, "before".to_string(), 0)]);
    }

    #[test]
    fn frontmatter_is_skipped() {
        let markdown = "---\ntitle: Notes\n# not a heading\n---\n\n# Real\n";
        assert_eq!(texts(markdown), vec![(1, "Real".to_string(), 5)]);
    }

    #[test]
    fn a_document_that_starts_with_a_rule_is_not_frontmatter() {
        // No closing `---`, so the first line opens frontmatter and the scan
        // stops there rather than inventing a heading.
        assert!(outline("---\n# nope\n").is_empty());
    }

    #[test]
    fn crlf_lines_do_not_leak_a_carriage_return() {
        assert_eq!(texts("# One\r\n\r\n## Two\r\n")[0].1, "One");
        assert_eq!(texts("# One\r\n\r\n## Two\r\n")[1].1, "Two");
    }

    #[test]
    fn heading_at_returns_the_section_containing_an_offset() {
        let markdown = "# One\n\nbody\n\n## Two\n\nmore\n";
        let headings = outline(markdown);
        assert_eq!(heading_at(&headings, 0), Some(0));
        assert_eq!(heading_at(&headings, 6), Some(0));
        assert_eq!(
            heading_at(&headings, markdown.find("## Two").unwrap()),
            Some(1)
        );
        assert_eq!(heading_at(&headings, markdown.len()), Some(1));
        assert_eq!(heading_at(&[], 0), None);
    }
}
