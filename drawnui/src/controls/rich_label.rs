//! SkiaRichLabel: a SkiaLabel whose text is markdown, rendered as spans. The parser is the small
//! one of DrawnUi.React (headings, paragraphs, bullet and numbered lists, fenced code blocks,
//! inline code, bold, italic, strikethrough, links, backslash escapes); the C# engine uses
//! CommonMark.NET.

use std::any::{Any, type_name};

use skia_safe::Color;

use crate::control::{Control, Has};
use crate::controls::label::{LabelBuild, LabelProps, SkiaLabel};
use crate::controls::text_span::TextSpan;
use crate::props;
use crate::tree::{Build, Cx, Mut, Raw};
use crate::types::CacheType;

pub(crate) type LinkTapped = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &str)>;

/// Defaults of DrawnUI SkiaRichLabel (ColorLink, ColorCodeBackground, ColorCodeBlock).
const LINK: Color = Color::from_argb(255, 0x64, 0x95, 0xED);
const CODE_BACKGROUND: Color = Color::from_argb(255, 0x69, 0x69, 0x69);
const CODE_BLOCK: Color = Color::from_argb(255, 0x22, 0x22, 0x22);

props!(RichLabelProps, RichLabelBuild, RichLabelSet {
    /// false = the text is shown as it is, no parsing.
    markdown_enabled / set_markdown_enabled: bool = true, MEASURE;
    /// The color of `[text](url)` links.
    link_color / set_link_color: Color = LINK, MEASURE;
    /// The color of inline code and code blocks.
    code_text_color / set_code_text_color: Color = Color::WHITE, MEASURE;
    /// The color of `#` headings.
    heading_text_color / set_heading_text_color: Color = Color::WHITE, MEASURE;
    /// Behind the lines of a fenced code block.
    code_block_background_color / set_code_block_background_color: Color = CODE_BLOCK, MEASURE;
    /// Behind inline code.
    code_background_color / set_code_background_color: Color = CODE_BACKGROUND, MEASURE;
    /// The line of `~~strikethrough~~`.
    strikeout_color / set_strikeout_color: Color = Color::RED, MEASURE;
    /// Put before a bullet item.
    prefix_bullet / set_prefix_bullet: String = "\u{2022} ".to_owned(), MEASURE;
    /// `{0}` is replaced by the item number.
    prefix_numbered / set_prefix_numbered: String = "{0}. ".to_owned(), MEASURE;
    /// Links are underlined.
    underline_link / set_underline_link: bool = true, MEASURE;
    /// Points, negative = pixels.
    underline_width / set_underline_width: f32 = -1.0, MEASURE;
});

/// A SkiaLabel whose text is markdown, rendered as spans; a tapped link reports its url.
pub struct SkiaRichLabel {
    label: SkiaLabel,
    /// The markdown style properties; the label properties are reached through `Has<LabelProps>`.
    pub p: RichLabelProps,
    /// What the spans were built from: text, font size, style properties.
    parsed: Option<(String, f32, RichLabelProps)>,
    pub(crate) link_tapped: Option<LinkTapped>,
}

impl SkiaRichLabel {
    /// A rich label with markdown text, cached as Operations.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: impl Into<String>) -> Build<SkiaRichLabel> {
        let mut rich = SkiaRichLabel { label: SkiaLabel::default(), p: RichLabelProps::default(), parsed: None, link_tapped: None };
        // C# SkiaRichLabel finds a system font for what the label's fonts lack (MatchCharacter).
        rich.label.p.system_font_fallback = true;
        Build::new(rich).text(text.into()).use_cache(CacheType::Operations)
    }

    /// The label the markdown is rendered into.
    pub fn label(&self) -> &SkiaLabel {
        &self.label
    }

    /// The url and the handler of a tapped link, taken out to run.
    pub(crate) fn take_link_tap(&mut self) -> Option<(String, LinkTapped)> {
        let index = self.label.pending_tap?;
        let tag = self.label.spans.get(index).map(|s| s.tag.as_str()).filter(|t| !t.is_empty())?;
        let handler = self.link_tapped.take()?;
        let url = tag.to_owned();
        self.label.pending_tap = None;
        Some((url, handler))
    }

    fn rebuild(&mut self) {
        let text = &self.label.p.text;
        let font_size = self.label.p.font_size;
        if self.parsed.as_ref().is_some_and(|(t, s, p)| t == text && *s == font_size && *p == self.p) {
            return;
        }
        let mut parser = Parser { p: &self.p, font_size, spans: Vec::new(), had_block: false };
        if !text.is_empty() {
            if self.p.markdown_enabled {
                parser.document(text);
            } else {
                parser.span(text, Inline::default(), |_| {});
            }
        }
        self.label.spans = parser.spans;
        self.parsed = Some((text.clone(), font_size, self.p.clone()));
    }
}

impl Has<LabelProps> for SkiaRichLabel {
    fn part(&self) -> &LabelProps {
        &self.label.p
    }
    fn part_mut(&mut self) -> &mut LabelProps {
        &mut self.label.p
    }
}

impl Has<RichLabelProps> for SkiaRichLabel {
    fn part(&self) -> &RichLabelProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut RichLabelProps {
        &mut self.p
    }
}

impl Build<SkiaRichLabel> {
    /// Runs when a `[text](url)` link is tapped, with the url (C# LinkTapped).
    pub fn on_link_tapped<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaRichLabel>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        self.control_mut().link_tapped = Some(Box::new(move |raw, state, cx, url| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| panic!("handler expects app state `{}`", type_name::<S>()));
            f(&mut raw.typed(), state, cx, url)
        }));
        self
    }
}

impl Control for SkiaRichLabel {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.label)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.label)
    }

    /// The spans are built again when the text, the font size or a style property changed.
    fn measure(&mut self, cx: &mut crate::control::LayoutCx, width: f32, height: f32) -> skia_safe::Size {
        self.rebuild();
        self.label.measure(cx, width, height)
    }
}

// ---------------------------------------------------------------- markdown

/// Inline emphasis while walking the markdown.
#[derive(Clone, Copy, Default)]
struct Inline {
    bold: bool,
    italic: bool,
    strike: bool,
    heading: u8,
}

struct Parser<'a> {
    p: &'a RichLabelProps,
    font_size: f32,
    spans: Vec<TextSpan>,
    /// Blocks are separated by one line break once something was emitted.
    had_block: bool,
}

fn item(line: &str) -> Option<(Option<u32>, &str)> {
    let t = line.trim_start();
    let mut chars = t.chars();
    match chars.next()? {
        '-' | '*' | '+' if chars.next().is_some_and(char::is_whitespace) => Some((None, t[1..].trim_start())),
        c if c.is_ascii_digit() => {
            let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            let rest = &t[digits..];
            let mut chars = rest.chars();
            if !matches!(chars.next(), Some('.' | ')')) || !chars.next().is_some_and(char::is_whitespace) {
                return None;
            }
            Some((t[..digits].parse().ok(), rest[1..].trim_start()))
        }
        _ => None,
    }
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 || !line[hashes..].starts_with(char::is_whitespace) {
        return None;
    }
    let content = line[hashes..].trim().trim_end_matches('#').trim_end();
    Some((hashes.min(3) as u8, content))
}

fn fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

impl Parser<'_> {
    /// C# AddTextSpan + SpanWithAttributes: one span per fragment carrying the inline state.
    fn span(&mut self, text: &str, state: Inline, modify: impl FnOnce(&mut TextSpan)) {
        if text.is_empty() {
            return;
        }
        let mut span = TextSpan::new(text).is_bold(state.bold).is_italic(state.italic).strikeout(state.strike);
        if state.strike {
            span.strikeout_color = self.p.strikeout_color;
        }
        let bigger = match state.heading {
            1 => Some(9.0),
            2 => Some(4.0),
            3 => Some(2.0),
            _ => None,
        };
        if let Some(bigger) = bigger {
            span.is_bold = true;
            span.font_size = Some(self.font_size + bigger);
            span.text_color = Some(self.p.heading_text_color);
        }
        modify(&mut span);
        self.spans.push(span);
    }

    /// A `\n` appended to the last span (or a span of its own).
    fn line_break(&mut self) {
        match self.spans.last_mut() {
            Some(last) => last.text.push('\n'),
            None => self.span("\n", Inline::default(), |_| {}),
        }
    }

    fn begin_block(&mut self) {
        if self.had_block {
            self.line_break();
        }
        self.had_block = true;
    }

    fn document(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<&str> = text.split('\n').collect();
        let mut paragraph: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if fence(line) {
                self.flush(&mut paragraph);
                i += 1;
                self.begin_block();
                let mut n = 0;
                while i < lines.len() && !fence(lines[i]) {
                    if n > 0 {
                        self.line_break();
                    }
                    let (color, background) = (self.p.code_text_color, self.p.code_block_background_color);
                    self.span(lines[i], Inline::default(), |s| {
                        s.text_color = Some(color);
                        s.background_color = Some(background);
                    });
                    n += 1;
                    i += 1;
                }
                i += 1; // closing fence
                continue;
            }
            if let Some((level, content)) = heading(line) {
                self.flush(&mut paragraph);
                self.begin_block();
                self.inlines(content, Inline { heading: level, ..Inline::default() });
                i += 1;
                continue;
            }
            if item(line).is_some() {
                self.flush(&mut paragraph);
                let mut number = 1;
                while i < lines.len() && let Some((numbered, content)) = item(lines[i]) {
                    self.begin_block();
                    let prefix = match numbered {
                        Some(_) => {
                            number += 1;
                            self.p.prefix_numbered.replace("{0}", &(number - 1).to_string())
                        }
                        None => self.p.prefix_bullet.clone(),
                    };
                    self.span(&prefix, Inline::default(), |_| {});
                    self.inlines(content, Inline::default());
                    i += 1;
                    // Continuation lines of the same item: indented, not blank, not a new item.
                    while i < lines.len() && lines[i].starts_with(char::is_whitespace) && !lines[i].trim().is_empty() && item(lines[i]).is_none() {
                        self.line_break();
                        self.inlines(lines[i].trim(), Inline::default());
                        i += 1;
                    }
                }
                continue;
            }
            if line.trim().is_empty() {
                self.flush(&mut paragraph);
                i += 1;
                continue;
            }
            paragraph.push(line);
            i += 1;
        }
        self.flush(&mut paragraph);
    }

    /// The collected paragraph as one block; soft line breaks stay inside it.
    fn flush(&mut self, paragraph: &mut Vec<&str>) {
        if paragraph.is_empty() {
            return;
        }
        self.begin_block();
        let text = paragraph.join("\n");
        self.inlines(&text, Inline::default());
        paragraph.clear();
    }

    /// Inline markdown: `code`, **bold**, __bold__, *italic*, _italic_, ~~strike~~, [text](url),
    /// backslash escapes.
    fn inlines(&mut self, text: &str, state: Inline) {
        let bytes = text.as_bytes();
        let n = bytes.len();
        let mut literal = String::new();
        let closes = |marker: &str, from: usize| -> Option<usize> {
            let mut k = text[from..].find(marker).map(|k| k + from)?;
            while k > 0 && bytes[k - 1] == b'\\' {
                k = text[k + 1..].find(marker).map(|j| j + k + 1)?;
            }
            Some(k)
        };
        let ws = |i: usize| bytes[i].is_ascii_whitespace();
        let mut i = 0;
        while i < n {
            let ch = text[i..].chars().next().expect("inside the text");
            if ch == '\\' && i + 1 < n {
                let next = text[i + 1..].chars().next().expect("inside the text");
                literal.push(next);
                i += 1 + next.len_utf8();
                continue;
            }
            if ch == '`' && let Some(end) = text[i + 1..].find('`').map(|e| e + i + 1) {
                self.flush_literal(&mut literal, state);
                let (color, background) = (self.p.code_text_color, self.p.code_background_color);
                self.span(&text[i + 1..end], state, |s| {
                    s.text_color = Some(color);
                    s.background_color = Some(background);
                });
                i = end + 1;
                continue;
            }
            if ch == '[' && let Some((label, url, len)) = link(&text[i..]) {
                self.flush_literal(&mut literal, state);
                let (color, underline, width) = (self.p.link_color, self.p.underline_link, self.p.underline_width);
                let shown = if label.is_empty() { url } else { label };
                self.span(shown, state, |s| {
                    s.tag = url.to_owned();
                    s.text_color = Some(color);
                    s.underline = underline;
                    s.underline_width = width;
                    s.force_capture_input = true;
                });
                i += len;
                continue;
            }
            if let Some(two) = text.get(i..i + 2)
                && matches!(two, "**" | "__" | "~~")
                && let Some(end) = closes(two, i + 2)
                && end > i + 2
            {
                self.flush_literal(&mut literal, state);
                let inner = if two == "~~" { Inline { strike: true, ..state } } else { Inline { bold: true, ..state } };
                self.inlines(&text[i + 2..end], inner);
                i = end + 2;
                continue;
            }
            if (ch == '*' || ch == '_') && i + 1 < n && !ws(i + 1) {
                let marker = &text[i..i + 1];
                if let Some(end) = closes(marker, i + 1)
                    && end > i + 1
                    && !ws(end - 1)
                {
                    self.flush_literal(&mut literal, state);
                    self.inlines(&text[i + 1..end], Inline { italic: true, ..state });
                    i = end + 1;
                    continue;
                }
            }
            literal.push(ch);
            i += ch.len_utf8();
        }
        self.flush_literal(&mut literal, state);
    }

    fn flush_literal(&mut self, literal: &mut String, state: Inline) {
        if !literal.is_empty() {
            self.span(literal, state, |_| {});
            literal.clear();
        }
    }
}

/// `[text](url "title")` at the start of `text`: the label, the url and the bytes consumed.
fn link(text: &str) -> Option<(&str, &str, usize)> {
    let close = text.find(']')?;
    let label = &text[1..close];
    let rest = &text[close + 1..];
    if !rest.starts_with('(') {
        return None;
    }
    let url_end = rest[1..].find(|c: char| c == ')' || c.is_whitespace()).map(|e| e + 1)?;
    let url = &rest[1..url_end];
    if url.is_empty() {
        return None;
    }
    let after = &rest[url_end..];
    let title = after.trim_start();
    let end = if title.starts_with('"') && after.len() > title.len() {
        let quote = title[1..].find('"')? + 2;
        if !title[quote..].starts_with(')') {
            return None;
        }
        rest.len() - title.len() + quote + 1
    } else if after.starts_with(')') {
        url_end + 1
    } else {
        return None;
    };
    Some((label, url, close + 1 + end))
}
