//! A small Markdown preview (Phase C) and the editor's formatting commands. It handles headings,
//! paragraphs, bullet and numbered lists, quotes, code blocks, rules, and bold, italic and code
//! spans. It's drawn with egui, so the app needs no Markdown crate. egui has no bold typeface here,
//! so bold text shows in the accent colour.

use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, RichText, Ui};
use gridcraft_ui_egui::theme::Tokens;

/// One block of a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Paragraph(String),
    Bullet(String),
    Numbered(String, String),
    Quote(String),
    Code(String),
    Rule,
    Blank,
}

/// A run of text with its inline style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

/// Splits a document into blocks.
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(lines) => blocks.push(Block::Code(lines.join("\n"))),
                None => code = Some(Vec::new()),
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line);
            continue;
        }
        let line = line.trim_end();
        let trimmed = line.trim_start();
        blocks.push(if trimmed.is_empty() {
            Block::Blank
        } else if trimmed == "---" || trimmed == "***" {
            Block::Rule
        } else if let Some(level) = heading_level(trimmed) {
            Block::Heading(level, trimmed[level as usize..].trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
            Block::Bullet(rest.to_string())
        } else if let Some((n, rest)) = numbered(trimmed) {
            Block::Numbered(n, rest)
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            Block::Quote(rest.to_string())
        } else {
            Block::Paragraph(trimmed.to_string())
        });
    }
    if let Some(lines) = code {
        blocks.push(Block::Code(lines.join("\n")));
    }
    blocks
}

/// `#`..`######` followed by a space: the heading level.
fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let rest = line.get(hashes..)?;
    ((1..=6).contains(&hashes) && rest.starts_with(' ')).then_some(hashes as u8)
}

/// `1. text`: the number and the text.
fn numbered(line: &str) -> Option<(String, String)> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 6 {
        return None;
    }
    let rest = line.get(digits..)?.strip_prefix(". ")?;
    Some((line[..digits].to_string(), rest.to_string()))
}

/// Splits a line into styled runs: `**bold**`, `*italic*` or `_italic_`, and `` `code` ``.
/// An unclosed marker styles the rest of the line.
pub fn spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut cur = Span::default();
    let mut chars = text.chars().peekable();
    let flush = |cur: &mut Span, out: &mut Vec<Span>| {
        if !cur.text.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                flush(&mut cur, &mut out);
                cur.code = !cur.code;
                out.push(Span { text: String::new(), code: cur.code, ..Span::default() });
                cur = Span { code: cur.code, ..Span::default() };
            }
            '*' | '_' if !cur.code => {
                if c == '*' && chars.peek() == Some(&'*') {
                    chars.next();
                    flush(&mut cur, &mut out);
                    cur.bold = !cur.bold;
                } else {
                    flush(&mut cur, &mut out);
                    cur.italic = !cur.italic;
                }
            }
            _ => cur.text.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out.retain(|s| !s.text.is_empty());
    out
}

fn job(text: &str, size: f32, color: egui::Color32, t: &Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
    for s in spans(text) {
        let format = TextFormat {
            font_id: if s.code { FontId::monospace(size - 1.0) } else { FontId::proportional(size) },
            color: if s.bold { t.accent } else { color },
            italics: s.italic,
            background: if s.code { t.header_bg } else { egui::Color32::TRANSPARENT },
            ..Default::default()
        };
        job.append(&s.text, 0.0, format);
    }
    job
}

/// Draws a document's preview into the Ui.
pub fn show(ui: &mut Ui, t: &Tokens, text: &str) {
    let body = t.text;
    for block in parse(text) {
        match block {
            Block::Blank => ui.add_space(6.0),
            Block::Rule => {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
            }
            Block::Heading(level, text) => {
                let size = match level {
                    1 => 26.0,
                    2 => 21.0,
                    3 => 18.0,
                    _ => 15.0,
                };
                ui.add_space(4.0);
                ui.label(RichText::new(text).size(size).strong().color(body));
            }
            Block::Paragraph(text) => {
                ui.add(egui::Label::new(job(&text, 14.0, body, t)).wrap());
            }
            Block::Bullet(text) => {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("•").color(body));
                    ui.add(egui::Label::new(job(&text, 14.0, body, t)).wrap());
                });
            }
            Block::Numbered(n, text) => {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(format!("{n}.")).color(body));
                    ui.add(egui::Label::new(job(&text, 14.0, body, t)).wrap());
                });
            }
            Block::Quote(text) => {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("▌").color(t.accent_soft));
                    ui.add(egui::Label::new(job(&text, 14.0, t.text_dim, t)).wrap());
                });
            }
            Block::Code(code) => {
                egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::same(6)).corner_radius(4.0).show(ui, |ui| {
                    ui.label(RichText::new(code).monospace().color(body));
                });
            }
        }
    }
}

/// An editor command: how a selection (or the cursor) is changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fmt {
    Bold,
    Italic,
    Code,
    Link,
    Heading(u8),
    Bullet,
    Numbered,
    Quote,
    CodeBlock,
    Rule,
}

/// Applies a command to `text`, where the selection runs from char `start` to char `end`.
/// Returns the new text and the new selection (in chars).
pub fn format(text: &str, start: usize, end: usize, fmt: Fmt) -> (String, usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let start = start.min(chars.len());
    let end = end.clamp(start, chars.len());
    let (open, close, placeholder): (&str, &str, &str) = match fmt {
        Fmt::Bold => ("**", "**", "bold text"),
        Fmt::Italic => ("*", "*", "italic text"),
        Fmt::Code => ("`", "`", "code"),
        Fmt::Link => ("[", "](https://)", "link text"),
        Fmt::CodeBlock => ("```\n", "\n```", "code"),
        Fmt::Rule => ("\n---\n", "", ""),
        Fmt::Heading(_) | Fmt::Bullet | Fmt::Numbered | Fmt::Quote => ("", "", ""),
    };
    match fmt {
        Fmt::Heading(_) | Fmt::Bullet | Fmt::Numbered | Fmt::Quote => {
            let prefix = match fmt {
                Fmt::Heading(level) => format!("{} ", "#".repeat(level.clamp(1, 6) as usize)),
                Fmt::Bullet => "- ".to_string(),
                Fmt::Numbered => "1. ".to_string(),
                _ => "> ".to_string(),
            };
            prefix_lines(&chars, start, end, &prefix)
        }
        Fmt::Rule => {
            let out: String = chars[..start].iter().collect::<String>() + open + &chars[end..].iter().collect::<String>();
            let at = start + open.chars().count();
            (out, at, at)
        }
        _ => {
            let open_len = open.chars().count();
            if start == end {
                let inner: String = placeholder.to_string();
                let out: String = chars[..start].iter().collect::<String>() + open + &inner + close + &chars[end..].iter().collect::<String>();
                let a = start + open_len;
                (out, a, a + inner.chars().count())
            } else {
                let out: String = chars[..start].iter().collect::<String>()
                    + open
                    + &chars[start..end].iter().collect::<String>()
                    + close
                    + &chars[end..].iter().collect::<String>();
                (out, start + open_len, end + open_len)
            }
        }
    }
}

/// Puts `prefix` at the start of each line the selection touches.
fn prefix_lines(chars: &[char], start: usize, end: usize, prefix: &str) -> (String, usize, usize) {
    let line_start = chars[..start].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let line_end = chars[end..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| end + i);
    let block: String = chars[line_start..line_end].iter().collect();
    let plen = prefix.chars().count();
    let mut out = String::new();
    let mut added = 0usize;
    for (i, line) in block.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(prefix);
        out.push_str(line);
        added += plen;
    }
    let text: String = chars[..line_start].iter().collect::<String>() + &out + &chars[line_end..].iter().collect::<String>();
    (text, line_start + plen, line_end + added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_parse_headings_lists_quotes_and_code() {
        let blocks = parse("# Title\n\nSome **bold** text\n- one\n2. two\n> note\n```\nlet x = 1;\n```\n---");
        assert_eq!(blocks[0], Block::Heading(1, "Title".into()));
        assert!(blocks.contains(&Block::Paragraph("Some **bold** text".into())));
        assert!(blocks.contains(&Block::Bullet("one".into())));
        assert!(blocks.contains(&Block::Numbered("2".into(), "two".into())));
        assert!(blocks.contains(&Block::Quote("note".into())));
        assert!(blocks.contains(&Block::Code("let x = 1;".into())));
        assert_eq!(blocks.last(), Some(&Block::Rule));
    }

    #[test]
    fn inline_spans_switch_bold_italic_and_code() {
        let s = spans("a **b** *c* `d` e");
        let styled: Vec<(&str, bool, bool, bool)> = s.iter().map(|x| (x.text.as_str(), x.bold, x.italic, x.code)).collect();
        assert!(styled.contains(&("b", true, false, false)));
        assert!(styled.contains(&("c", false, true, false)));
        assert!(styled.contains(&("d", false, false, true)));
        assert!(styled.contains(&("a ", false, false, false)));
    }

    #[test]
    fn bold_wraps_the_selection_and_keeps_it_selected() {
        assert_eq!(format("say hi", 4, 6, Fmt::Bold), ("say **hi**".into(), 6, 8));
    }

    #[test]
    fn with_no_selection_bold_inserts_a_placeholder_selected() {
        assert_eq!(format("ab", 1, 1, Fmt::Bold), ("a**bold text**b".into(), 3, 12));
    }

    #[test]
    fn heading_and_bullets_prefix_every_selected_line() {
        let (t, _, _) = format("one\ntwo", 0, 7, Fmt::Bullet);
        assert_eq!(t, "- one\n- two");
        let (t, _, _) = format("Plan\nmore", 0, 0, Fmt::Heading(2));
        assert_eq!(t, "## Plan\nmore");
    }

    #[test]
    fn code_block_wraps_the_selection() {
        let (t, _, _) = format("x", 0, 1, Fmt::CodeBlock);
        assert_eq!(t, "```\nx\n```");
    }
}
