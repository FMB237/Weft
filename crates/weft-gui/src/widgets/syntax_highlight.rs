//! Syntax highlighting for markdown.
//!
//! Produces `egui::TextFormat` spans for inline markdown constructs so the
//! editor widget can render rich text.

use egui::{Color32, FontFamily, FontId, Stroke, TextFormat};

#[derive(Debug)]
pub struct SyntaxHighlighter {
    bold: TextFormat,
    italic: TextFormat,
    code: TextFormat,
    link: TextFormat,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            bold: TextFormat {
                font_id: FontId {
                    size: 14.0,
                    family: FontFamily::Proportional,
                },
                ..Default::default()
            },
            italic: TextFormat {
                italics: true,
                ..Default::default()
            },
            code: TextFormat {
                color: Color32::from_rgb(200, 100, 50),
                ..Default::default()
            },
            link: TextFormat {
                color: Color32::from_rgb(86, 156, 214),
                underline: Stroke::new(1.0, Color32::from_rgb(86, 156, 214)),
                ..Default::default()
            },
        }
    }

    /// Highlight a single line of markdown, returning a list of (format, text) spans.
    pub fn highlight<'a>(&self, line: &'a str) -> Vec<(TextFormat, &'a str)> {
        // Heading.
        let level = line.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
            return vec![(
                TextFormat {
                    font_id: FontId {
                        size: match level {
                            1 => 32.0,
                            2 => 24.0,
                            3 => 20.0,
                            _ => 18.0,
                        },
                        family: FontFamily::Proportional,
                    },
                    ..Default::default()
                },
                line,
            )];
        }

        // Inline: code spans first (`code`), then bold (**text**), italic (*text*), links [text](url).
        let mut spans: Vec<(TextFormat, &str)> = Vec::new();
        let mut remaining = line;

        while !remaining.is_empty() {
            let next_code = remaining.find('`');
            let next_bold = remaining.find("**");
            let next_italic = remaining.find('*');
            let next_link = remaining.find('[');

            let candidates: Vec<usize> = [next_code, next_bold, next_italic, next_link]
                .into_iter()
                .flatten()
                .collect();

            if let Some(first) = candidates.iter().min() {
                let before = &remaining[..*first];
                if !before.is_empty() {
                    spans.push((TextFormat::default(), before));
                }

                let rest = &remaining[*first..];

                // Code span.
                if let Some(after) = rest.strip_prefix('`') {
                    let end = after.find('`').map_or(rest.len(), |i| i + 2);
                    spans.push((self.code.clone(), &rest[..end]));
                    remaining = &rest[end..];
                    continue;
                }

                // Bold.
                if let Some(after) = rest.strip_prefix("**") {
                    let end = after.find("**").map_or(rest.len(), |i| i + 4);
                    spans.push((self.bold.clone(), &rest[..end]));
                    remaining = &rest[end..];
                    continue;
                }

                // Italic.
                if let Some(after) = rest.strip_prefix('*') {
                    let end = after.find('*').map_or(rest.len(), |i| i + 2);
                    spans.push((self.italic.clone(), &rest[..end]));
                    remaining = &rest[end..];
                    continue;
                }

                // Link.
                if let Some(after_open) = rest.strip_prefix('[') {
                    if let Some(close_bracket) = after_open.find(']') {
                        // close_bracket is relative to after_open; add 1 for the '['.
                        let link_text = &rest[..=1 + close_bracket];
                        let after = &rest[1 + close_bracket + 1..];
                        if after.starts_with('(') && after[1..].find(')').is_some() {
                            spans.push((self.link.clone(), link_text));
                            remaining = after;
                            continue;
                        }
                    }
                    spans.push((TextFormat::default(), rest));
                    remaining = "";
                }
            } else {
                spans.push((TextFormat::default(), remaining));
                remaining = "";
            }
        }

        spans
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_heading() {
        let h = SyntaxHighlighter::new();
        let spans = h.highlight("# Title");
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0.font_id.size - 32.0).abs() < f32::EPSILON);
        assert_eq!(spans[0].1, "# Title");
    }

    #[test]
    fn highlight_code_span() {
        let h = SyntaxHighlighter::new();
        let spans = h.highlight("code `x + 1` here");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].1, "code ");
        assert_eq!(spans[1].0.color, egui::Color32::from_rgb(200, 100, 50));
        assert_eq!(spans[1].1, "`x + 1`");
        assert_eq!(spans[2].1, " here");
    }

    #[test]
    fn highlight_bold() {
        let h = SyntaxHighlighter::new();
        let spans = h.highlight("**bold** text");
        assert_eq!(spans.len(), 2);
        assert!((spans[0].0.font_id.size - 14.0).abs() < f32::EPSILON);
        assert_eq!(spans[0].1, "**bold**");
        assert_eq!(spans[1].1, " text");
    }

    #[test]
    fn highlight_link() {
        let h = SyntaxHighlighter::new();
        let spans = h.highlight("[click](https://example.com) here");
        assert_eq!(spans.len(), 2);
        assert!(spans[0].0.underline.width > 0.0);
        assert_eq!(spans[0].1, "[click]");
        assert_eq!(spans[1].1, "(https://example.com) here");
    }
}
