//! Smart lists for the Markdown editor.
//!
//! Markdown lists are plain text, so "smart" behaviour is just knowing where
//! the caret is relative to a marker and returning a text edit:
//!
//! - `Enter` continues a bullet, a task, or an ordered item.
//! - `Enter` on an empty item ends the list (and outdents a nested one).
//! - `Tab` / `Shift+Tab` indent and outdent an item.
//!
//! The public functions take and return character offsets, the unit egui uses
//! for its text cursor, so the GUI can apply an edit without translating
//! offsets. Everything is pure and tested without egui.

/// Indentation added by one `Tab`, in spaces.
pub const INDENT: &str = "  ";

/// What kind of marker a list item has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    /// `- `, `* `, or `+ `.
    Bullet(char),
    /// `1. ` or `1) `.
    Ordered {
        /// The number as written.
        number: u64,
        /// `.` or `)`.
        delimiter: char,
    },
    /// `- [ ] ` or `- [x] `.
    Task {
        /// The bullet that opens the task.
        bullet: char,
        /// Whether the box is ticked.
        checked: bool,
    },
}

impl Marker {
    /// Marker text for the item that continues this one.
    fn continuation(&self) -> String {
        match self {
            Self::Bullet(bullet) => format!("{bullet} "),
            Self::Ordered { number, delimiter } => format!("{}{delimiter} ", number + 1),
            // A new task starts unchecked: that is what a checklist means.
            Self::Task { bullet, .. } => format!("{bullet} [ ] "),
        }
    }
}

/// One list item line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Leading whitespace, as written.
    pub indent: String,
    /// The marker of the item.
    pub marker: Marker,
    /// Text after the marker, without the line break.
    pub content: String,
    /// Character offset where the line starts.
    pub start: usize,
    /// Character offset where the line ends (before `\n`).
    pub end: usize,
}

impl Item {
    /// Whether the item has no text after its marker.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Character offset just past the indentation.
    fn content_start(&self) -> usize {
        self.start + self.indent.chars().count()
    }
}

/// A replacement of `start..end` (character offsets) with `insert`.
///
/// The caller applies it to the text and parks the caret at `caret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEdit {
    /// First character replaced.
    pub start: usize,
    /// One past the last character replaced.
    pub end: usize,
    /// Text that takes its place.
    pub insert: String,
    /// Caret position after the edit, in characters.
    pub caret: usize,
}

/// Parse the list item on the line that contains `caret`.
///
/// Returns `None` when the line is not a list item, so the caller can leave
/// the key to the text editor's normal behaviour.
#[must_use]
pub fn item_at(text: &str, caret: usize) -> Option<Item> {
    let line_start = text[..byte_of_char(text, caret)]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = text[byte_of_char(text, caret)..]
        .find('\n')
        .map_or(text.len(), |index| byte_of_char(text, caret) + index);
    let line = &text[line_start..line_end];

    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent_len..];
    let (marker, marker_len) = parse_marker(rest)?;

    Some(Item {
        indent: line[..indent_len].to_string(),
        marker,
        content: rest[marker_len..].to_string(),
        start: char_of_byte(text, line_start),
        end: char_of_byte(text, line_end),
    })
}

/// Continue the list when the user presses `Enter` at the end of the line.
///
/// `None` means the caret is not at the end of a list item line, so the
/// editor's normal line break is the right behaviour. Trailing spaces on the
/// finished item are dropped as part of the edit.
#[must_use]
pub fn continue_list(text: &str, caret: usize) -> Option<ListEdit> {
    let item = item_at(text, caret)?;
    let trimmed_end = item.end - trailing_whitespace(&item.content);
    if caret != item.end && caret != trimmed_end {
        return None;
    }

    if item.is_empty() {
        let indent_chars = item.indent.chars().count();
        // A nested empty item steps back out one level first, so repeated
        // Enter walks the user out of the list.
        if indent_chars >= INDENT.chars().count() {
            let removed = INDENT.chars().count();
            return Some(ListEdit {
                start: item.start,
                end: item.start + removed,
                insert: String::new(),
                caret: caret - removed,
            });
        }
        // At the outermost level the item ends: drop the marker and content,
        // keeping the indentation as plain text.
        let content_start = item.content_start();
        return Some(ListEdit {
            start: content_start,
            end: item.end,
            insert: String::new(),
            caret: content_start,
        });
    }

    let insert = format!("\n{}{}", item.indent, item.marker.continuation());
    Some(ListEdit {
        start: trimmed_end,
        end: item.end,
        insert: insert.clone(),
        caret: trimmed_end + insert.chars().count(),
    })
}

/// Indent a list item by one level.
#[must_use]
pub fn indent_item(text: &str, caret: usize) -> Option<ListEdit> {
    let item = item_at(text, caret)?;
    Some(ListEdit {
        start: item.start,
        end: item.start,
        insert: INDENT.to_string(),
        caret: caret + INDENT.chars().count(),
    })
}

/// Remove one level of indentation from a list item.
///
/// `None` for an item already at column zero, so `Shift+Tab` has nothing to
/// do and the key can fall through.
#[must_use]
pub fn outdent_item(text: &str, caret: usize) -> Option<ListEdit> {
    let item = item_at(text, caret)?;
    if item.indent.is_empty() {
        return None;
    }
    let removed = item.indent.len().min(INDENT.len());
    Some(ListEdit {
        start: item.start,
        end: item.start + removed,
        insert: String::new(),
        caret: caret.saturating_sub(removed),
    })
}

/// Split one line's marker from its text.
fn parse_marker(rest: &str) -> Option<(Marker, usize)> {
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let after = rest.get(first.len_utf8()..)?;
        let after_bullet = after.strip_prefix(' ')?;
        if let Some((consumed, checked)) = parse_task(after_bullet) {
            return Some((
                Marker::Task {
                    bullet: first,
                    checked,
                },
                first.len_utf8() + 1 + consumed,
            ));
        }
        return Some((Marker::Bullet(first), first.len_utf8() + 1));
    }

    if first.is_ascii_digit() {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let delimiter = rest[digits..].chars().next()?;
        if matches!(delimiter, '.' | ')') && rest[digits + 1..].starts_with(' ') {
            let number = rest[..digits].parse().ok()?;
            return Some((Marker::Ordered { number, delimiter }, digits + 2));
        }
    }

    None
}

/// Parse a task box at the start of the text after a bullet.
///
/// Returns the bytes it spans and whether it is ticked.
fn parse_task(text: &str) -> Option<(usize, bool)> {
    let mut chars = text.chars();
    if chars.next()? != '[' {
        return None;
    }
    let checked = match chars.next()? {
        ' ' => false,
        'x' | 'X' => true,
        _ => return None,
    };
    if chars.next()? != ']' || chars.next()? != ' ' {
        return None;
    }
    Some((4, checked))
}

/// Number of trailing whitespace characters in `text`.
fn trailing_whitespace(text: &str) -> usize {
    text.chars()
        .rev()
        .take_while(|ch| ch.is_whitespace())
        .count()
}

/// Byte offset of a character index, clamped to the text length.
fn byte_of_char(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

/// Character index of a byte offset that sits on a character boundary.
fn char_of_byte(text: &str, bytes: usize) -> usize {
    text[..bytes].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edit: &ListEdit) -> String {
        let mut chars: Vec<char> = text.chars().collect();
        let insert: Vec<char> = edit.insert.chars().collect();
        chars.splice(edit.start..edit.end, insert);
        chars.into_iter().collect()
    }

    #[test]
    fn item_at_reads_bullets_and_content() {
        let item = item_at("- one\n", 0).expect("item");
        assert_eq!(item.indent, "");
        assert_eq!(item.marker, Marker::Bullet('-'));
        assert_eq!(item.content, "one");
        assert_eq!((item.start, item.end), (0, 5));
    }

    #[test]
    fn item_at_reads_nested_ordered_items() {
        let item = item_at("  intro\n  12) twelve\n", 12).expect("item");
        assert_eq!(item.indent, "  ");
        assert_eq!(
            item.marker,
            Marker::Ordered {
                number: 12,
                delimiter: ')'
            }
        );
        assert_eq!(item.content, "twelve");
    }

    #[test]
    fn item_at_reads_tasks() {
        let item = item_at("- [x] done\n", 3).expect("item");
        assert_eq!(
            item.marker,
            Marker::Task {
                bullet: '-',
                checked: true
            }
        );
        assert_eq!(item.content, "done");
    }

    #[test]
    fn item_at_rejects_plain_lines() {
        assert!(item_at("plain text\n", 2).is_none());
        assert!(item_at("# heading\n", 2).is_none());
        assert!(item_at("-nospace\n", 2).is_none());
        assert!(item_at("10.missing space\n", 2).is_none());
    }

    #[test]
    fn item_at_is_only_the_line_under_the_caret() {
        let text = "- first\n- second\n";
        assert_eq!(item_at(text, 0).expect("first").content, "first");
        assert_eq!(item_at(text, 10).expect("second").content, "second");
    }

    #[test]
    fn enter_continues_a_bullet() {
        let edit = continue_list("- one", 5).expect("continue");
        assert_eq!(apply("- one", &edit), "- one\n- ");
        assert_eq!(edit.caret, 8);
    }

    #[test]
    fn enter_keeps_the_indent_of_a_nested_item() {
        let edit = continue_list("  - one", 7).expect("continue");
        assert_eq!(apply("  - one", &edit), "  - one\n  - ");
        assert_eq!(edit.caret, 12);
    }

    #[test]
    fn enter_increments_ordered_items() {
        let dot = continue_list("1. one", 6).expect("continue");
        assert_eq!(apply("1. one", &dot), "1. one\n2. ");
        let paren = continue_list("9) nine", 7).expect("continue");
        assert_eq!(apply("9) nine", &paren), "9) nine\n10) ");
    }

    #[test]
    fn enter_starts_tasks_unchecked() {
        let edit = continue_list("- [x] done", 10).expect("continue");
        assert_eq!(apply("- [x] done", &edit), "- [x] done\n- [ ] ");
    }

    #[test]
    fn enter_drops_trailing_spaces_before_continuing() {
        let edit = continue_list("- one   ", 8).expect("continue");
        assert_eq!(apply("- one   ", &edit), "- one\n- ");
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list() {
        let edit = continue_list("- ", 2).expect("end");
        assert_eq!(apply("- ", &edit), "");
        assert_eq!(edit.caret, 0);
    }

    #[test]
    fn enter_on_an_empty_nested_item_outdents_first() {
        let edit = continue_list("  - ", 4).expect("outdent");
        assert_eq!(apply("  - ", &edit), "- ");
        assert_eq!(edit.caret, 2);
    }

    #[test]
    fn enter_inside_a_line_is_left_to_the_editor() {
        assert!(continue_list("- one", 2).is_none());
        assert!(continue_list("plain", 5).is_none());
    }

    #[test]
    fn tab_indents_an_item_and_moves_the_caret() {
        let text = "- one";
        let edit = indent_item(text, 5).expect("indent");
        assert_eq!(apply(text, &edit), "  - one");
        assert_eq!(edit.caret, 7);
    }

    #[test]
    fn tab_only_indents_list_items() {
        assert!(indent_item("plain", 5).is_none());
    }

    #[test]
    fn shift_tab_outdents_one_level() {
        let text = "    - one";
        let edit = outdent_item(text, 9).expect("outdent");
        assert_eq!(apply(text, &edit), "  - one");
        assert_eq!(edit.caret, 7);
    }

    #[test]
    fn shift_tab_at_column_zero_does_nothing() {
        assert!(outdent_item("- one", 5).is_none());
        assert!(outdent_item("plain", 5).is_none());
    }

    #[test]
    fn non_ascii_text_keeps_character_offsets() {
        let text = "- héllo";
        let edit = continue_list(text, 7).expect("continue");
        assert_eq!(apply(text, &edit), "- héllo\n- ");
        assert_eq!(edit.caret, 10);
    }
}
