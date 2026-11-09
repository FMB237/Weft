//! Syntax highlighting for markdown.
//!
//! Produces `egui::TextFormat` spans for inline markdown constructs so the
//! editor widget can render rich text.

use egui::{Color32, FontFamily, FontId, Stroke, TextFormat};

pub struct SyntaxHighlighter {
    heading_format: TextFormat,
    bold_format: TextFormat,
    italic_format: TextFormat,
    code_format: TextFormat,
    link_format: TextFormat,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            heading_format: TextFormat {
                font_id: FontId {
                    size: 24.0,
                    family: FontFamily::Proportional,
                },
                ..Default::default()
            },
            bold_format: TextFormat {
                font_id: FontId {
                    size: 14.0,
                    family: FontFamily::Proportional,
                },
                ..Default::default()
            },
            italic_format: TextFormat {
                italics: true,
                ..Default::default()
            },
            code_format: TextFormat {
                color: Color32::from_rgb(200, 100, 50),
                ..Default::default()
            },
            link_format: TextFormat {
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
        if level >= 1 && level <= 6 && line.as_bytes().get(level) == Some(&b' ') {
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

            let candidates: Vec<usize> = [
                next_code,
                next_bold,
                next_italic,
                next_link,
            ]
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
                if rest.starts_with('`') {
                    let end = rest[1..].find('`').map(|i| i + 1).unwrap_or(rest.len());
                    spans.push((self.code_format.clone(), &rest[..end + 1]));
                    remaining = &rest[end + 1..];
                    continue;
                }

                // Bold.
                if rest.starts_with("**") {
                    let end = rest[2..].find("**").map(|i| i + 2).unwrap_or(rest.len());
                    spans.push((self.bold_format.clone(), &rest[..end + 2]));
                    remaining = &rest[end + 2..];
                    continue;
                }

                // Italic.
                if rest.starts_with('*') {
                    let end = rest[1..].find('*').map(|i| i + 1).unwrap_or(rest.len());
                    spans.push((self.italic_format.clone(), &rest[..end + 1]));
                    remaining = &rest[end + 1..];
                    continue;
                }

                // Link.
                if rest.starts_with('[') {
                    if let Some(close_bracket) = rest[1..].find(']') {
                        // close_bracket is relative to rest[1..]; add 1 for absolute position.
                        let link_text = &rest[0..=1 + close_bracket];
                        let after = &rest[1 + close_bracket + 1..];
                        if after.starts_with('(') && after[1..].find(')').is_some() {
                            spans.push((self.link_format.clone(), link_text));
                            remaining = after;
                            continue;
                        }
                    }
                    spans.push((TextFormat::default(), rest));
                    remaining = "";
                    continue;
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
        assert_eq!(spans[0].0.font_id.size, 32.0);
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
        assert_eq!(spans[0].0.font_id.size, 14.0);
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
