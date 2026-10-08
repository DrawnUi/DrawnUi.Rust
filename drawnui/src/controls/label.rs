//! SkiaLabel: text with its own line breaking (no paragraph library, no ICU). The line layout
//! follows DrawnUi.React SkiaLabel (Tokenize, Segment, LayoutLines, Paint): words are slices of
//! the text, a run is what one font draws, and everything paint needs is built in measure. The
//! Chinese / Japanese break rules and the height limit come from the C# engine.
// ponytail: no shaping (HarfBuzz): emoji sequences, RTL and complex scripts are drawn one glyph
// per code point. No AutoSize, no Fill* alignments.

use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::Range;

use skia_safe::{Color, Contains, Font, GlyphId, Paint, PaintStyle, Point, Rect, Shader, Size, TextBlob, TextBlobBuilder};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx, part_mut};
use crate::controls::rich_label::SkiaRichLabel;
use crate::controls::text_span::{IntoSpans, TextSpan};
use crate::fonts::Fonts;
use crate::gestures::{Gesture, GestureKind};
use crate::keyboard::{Cursor, KeyEvent, KeyKind};
use crate::paint::create_gradient;
use crate::props;
use crate::tree::{Build, ControlId, Cx, Mut, Raw};
use crate::types::{CacheType, Dirty, SkiaGradient, Thickness};

/// Where the lines sit in the label, horizontally or vertically.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextAlignment {
    #[default]
    Start,
    Center,
    End,
}

/// What happens to text wider than the label. As in both upstream engines only `NoWrap` differs
/// from `WordWrap`; the three truncation modes wrap too and put the ellipsis at the end of the
/// last line MaxLines or the height allows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LineBreakMode {
    NoWrap,
    WordWrap,
    CharacterWrap,
    HeadTruncation,
    #[default]
    TailTruncation,
    MiddleTruncation,
}

/// Case applied to the text at layout time; the text itself is kept.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextTransform {
    #[default]
    None,
    Lowercase,
    Uppercase,
    /// The first character of every word.
    Titlecase,
}

/// Bold and italic (C# FontAttributes flags).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FontAttributes {
    #[default]
    None,
    Bold,
    Italic,
    BoldItalic,
}

impl FontAttributes {
    /// Bold or BoldItalic.
    pub fn is_bold(self) -> bool {
        matches!(self, FontAttributes::Bold | FontAttributes::BoldItalic)
    }
    /// Italic or BoldItalic.
    pub fn is_italic(self) -> bool {
        matches!(self, FontAttributes::Italic | FontAttributes::BoldItalic)
    }
}

/// Ends a truncated line.
const ELLIPSIS: &str = "\u{2026}";
/// Weight `is_bold` asks for (CSS bold).
const BOLD: i32 = 700;
/// The strikeout sits at half an estimated x-height (DrawnUi.React, the C# fallback).
const X_HEIGHT: f32 = 0.52;

/// A line never starts with these (closing punctuation, small kana, the long vowel mark): JIS X 4051 kinsoku.
const NO_BREAK_BEFORE: &str = "、。，．・：；？！゛゜ヽヾゝゞ々〻ー」』）〕］｝〉》】〗〙〟｠»’”‐゠–〜～ぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ｡｣､･ｰｧｨｩｪｫｬｭｮｯ)]},.!?:;%";

/// A line never ends with these (opening brackets and quotes).
const NO_BREAK_AFTER: &str = "「『（〔［｛〈《【〖〘〝｟«‘“｢([{";

props!(LabelProps, LabelBuild, LabelSet {
    /// The text; `\n` breaks the line. Spans, when given, replace it.
    text / set_text: String = String::new(), MEASURE;
    /// Points.
    font_size / set_font_size: f32 = 12.0, MEASURE;
    /// A registered font alias; empty = the default font.
    font_family / set_font_family: String = String::new(), MEASURE;
    /// Aliases tried in order, comma separated, for a character the font has no glyph for
    /// (DrawnUi.React chain; C# takes one alias).
    font_family_fallback / set_font_family_fallback: String = String::new(), MEASURE;
    /// 100..900; 0 = the family's regular face. The nearest registered weight is used.
    font_weight / set_font_weight: i32 = 0, MEASURE;
    /// Bold and italic; `font_weight` wins over Bold.
    font_attributes / set_font_attributes: FontAttributes = FontAttributes::None, MEASURE;
    /// GreenYellow by default, as upstream.
    text_color / set_text_color: Color = Color::from_argb(255, 0xAD, 0xFF, 0x2F), DRAW;
    /// -1 = no limit.
    max_lines / set_max_lines: i32 = -1, MEASURE;
    /// Wrapping and truncation (TailTruncation by default).
    line_break_mode / set_line_break_mode: LineBreakMode = LineBreakMode::TailTruncation, MEASURE;
    /// Multiplies the step from one line to the next.
    line_spacing / set_line_spacing: f32 = 1.0, MEASURE;
    /// Multiplies the height of a line (ascent + descent); the text sits at the top of it.
    line_height / set_line_height: f32 = 1.0, MEASURE;
    /// Extra space above a paragraph (a line after `\n`), in line heights with spacing. Upstream
    /// C# defaults to 0.25; DrawnUi.React has none, so 0 here.
    paragraph_spacing / set_paragraph_spacing: f32 = 0.0, MEASURE;
    /// 1 = the font's own advances; every 1.0 above adds a point between characters.
    character_spacing / set_character_spacing: f32 = 1.0, MEASURE;
    /// Upper, lower or title case, applied at layout time.
    text_transform / set_text_transform: TextTransform = TextTransform::None, MEASURE;
    /// Drawn instead of a character no font has a glyph for (C# FallbackCharacter). `None` draws
    /// the font's missing-glyph box, as DrawnUi.React (C# default: a space).
    fallback_character / set_fallback_character: Option<char> = None, MEASURE;
    /// A character that neither the font nor `font_family_fallback` has is drawn with a system
    /// font that has it (C# SkiaRichLabel: `SkiaFontManager.MatchCharacter`; the web has no system
    /// fonts). SkiaRichLabel and SkiaEditor (`use_unicode`) turn it on.
    system_font_fallback / set_system_font_fallback: bool = false, MEASURE;
    /// Where each line sits in the label.
    horizontal_text_alignment / set_horizontal_text_alignment: TextAlignment = TextAlignment::Start, DRAW;
    /// Where the text block sits in the label.
    vertical_text_alignment / set_vertical_text_alignment: TextAlignment = TextAlignment::Start, DRAW;
    /// Outline around the glyphs, drawn under the fill. Transparent = none.
    stroke_color / set_stroke_color: Color = Color::TRANSPARENT, MEASURE;
    /// Points.
    stroke_width / set_stroke_width: f32 = 1.0, MEASURE;
    /// A gradient on the outline instead of `stroke_color`.
    stroke_gradient / set_stroke_gradient: Option<Box<SkiaGradient>> = None, DRAW;
    /// A stroked copy of the glyphs behind them, `drop_shadow_size` points thick, moved by the
    /// offsets. Transparent = none.
    drop_shadow_color / set_drop_shadow_color: Color = Color::TRANSPARENT, MEASURE;
    /// Points.
    drop_shadow_size / set_drop_shadow_size: f32 = 2.0, MEASURE;
    /// Points.
    drop_shadow_offset_x / set_drop_shadow_offset_x: f32 = 2.0, MEASURE;
    /// Points.
    drop_shadow_offset_y / set_drop_shadow_offset_y: f32 = 2.0, MEASURE;
    /// `fill_gradient` and `stroke_gradient` span each line (true) or the whole text box.
    gradient_by_lines / set_gradient_by_lines: bool = true, DRAW;
    /// Every character is laid out as typed, as an editor needs it: runs of spaces are kept,
    /// spaces at a wrap hang at the end of the line, an empty text has its empty line (C#
    /// KeepSpacesOnLineBreaks). Lines break after spaces only, also between spans.
    keep_spaces_on_line_breaks / set_keep_spaces_on_line_breaks: bool = false, MEASURE;
});

/// One run of a laid-out label (`SkiaLabel::runs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelRun<'a> {
    /// The line it is on, from 0.
    pub line: usize,
    /// What it draws, transformed.
    pub text: &'a str,
    /// Pixels, with the character spacing after every glyph.
    pub width: f32,
    /// 0 = the font of its style; k = the k-th alias of `font_family_fallback`.
    pub fallback: usize,
    /// Index in `SkiaLabel::spans`; `None` for the label's own text.
    pub span: Option<usize>,
}

/// A laid-out line as an editor sees it (`SkiaLabel::text_lines`).
pub struct TextLine<'a> {
    /// The line box, pixels in the label's parent space.
    pub rect: Rect,
    /// Index of its first character in the label's text.
    pub start: usize,
    /// Each character's advance, pixels.
    pub advances: &'a [f32],
}

impl TextLine<'_> {
    /// One past its last character.
    pub fn end(&self) -> usize {
        self.start + self.advances.len()
    }

    /// Where the character `index` (of the text) starts, pixels.
    pub fn x_of(&self, index: usize) -> f32 {
        let k = index.clamp(self.start, self.end()) - self.start;
        self.rect.left + self.advances[..k].iter().sum::<f32>()
    }
}

/// A fragment drawn with one font and one style.
struct Run {
    /// Bytes in `Layout::text`.
    text: Range<usize>,
    style: usize,
    /// Index in `Layout::fonts`.
    font: usize,
    /// Pixels, every glyph with its character spacing.
    width: f32,
    /// Glyphs positioned from x = 0 on the baseline.
    blob: Option<TextBlob>,
}

struct Line {
    runs: Range<usize>,
    /// Bytes in `Layout::text`.
    text: Range<usize>,
    /// Pixels, without the spacing after the last glyph.
    width: f32,
    ascent: f32,
    descent: f32,
    /// First line after a `\n`.
    new_paragraph: bool,
    /// Indices in `Layout::advances`: one per character of the line.
    advances: Range<usize>,
}

/// One style resolved for a measure: the label's own or a span's (DrawnUi.React SpanFonts).
struct Style {
    font: usize,
    /// Indices in `Layout::fonts` tried for a glyph the main font lacks.
    fallbacks: Range<usize>,
    ascent: f32,
    descent: f32,
    size_px: f32,
    /// Advance of a space in the main font.
    space: f32,
    span: Option<usize>,
    weight: i32,
    italic: bool,
    /// Characters no font of the style has may take a system font (`system_font_fallback`).
    system: bool,
    /// Indices in `Layout::fonts` of the system fonts found for its text, tried after `fallbacks`.
    system_faces: Vec<usize>,
}

/// A word to wrap: bytes in `Layout::source`.
#[derive(Clone, Copy)]
struct Token {
    start: usize,
    end: usize,
    style: usize,
    space_before: bool,
    trailing_space: bool,
}

/// A line being filled.
struct Open {
    runs: usize,
    text: usize,
    /// Sum of the run widths (each with a spacing after its last glyph).
    width: f32,
    ascent: f32,
    descent: f32,
}

/// The lines of the last measure and the buffers every measure reuses.
#[derive(Default)]
struct Layout {
    lines: Vec<Line>,
    runs: Vec<Run>,
    /// The text of all runs, back to back.
    text: String,
    styles: Vec<Style>,
    fonts: Vec<Font>,
    /// The words of the text and the spans, back to back, transformed.
    source: String,
    tokens: Vec<Token>,
    /// Token ranges, one per `\n` paragraph.
    paragraphs: Vec<Range<usize>>,
    /// The word being placed: its runs (text in `source`) and the advance of each character.
    word_runs: Vec<Run>,
    word_widths: Vec<f32>,
    /// Pixels of the spaces that end the word being placed (they hang past the line's end).
    hang: f32,
    /// Every character's advance with its spacing, line by line.
    advances: Vec<f32>,
    glyphs: Vec<GlyphId>,
    widths: Vec<f32>,
}

/// Gradient shaders rebuilt when the gradients, the layout or the box change; read by paint.
#[derive(Default)]
struct Shaders {
    serial: u32,
    box_size: (f32, f32),
    fill: Option<Box<SkiaGradient>>,
    stroke: Option<Box<SkiaGradient>>,
    /// Per line, or one entry for the whole box.
    lines: Vec<(Option<Shader>, Option<Shader>)>,
}

/// Drawn over selected text (C# SkiaLabel.TextSelectionColor).
pub const TEXT_SELECTION_COLOR: Color = Color::from_argb(90, 13, 110, 253);

/// The caption of the Copy button over a touch selection (C# SkiaLabel.CopyButtonText).
pub const COPY_BUTTON_TEXT: &str = "Copy";

/// The selection of a label with `accessibility_text_selectable` (C# SkiaLabel TEXT SELECTION).
/// Indices count characters of `SkiaLabel::selection_source`.
#[derive(Default)]
struct Selection {
    start: usize,
    length: usize,
    /// The range the press started with: a character, or the double-clicked word.
    anchor: (usize, usize),
    /// A press is selecting: its moves extend the selection.
    selecting: bool,
    /// Made by a long press on touch: the Copy button shows, a tap drops it.
    touch: bool,
    copy_pressed: bool,
    /// Time and place of the last click, for a double click.
    last_click: (f64, Point),
}

/// Text with its own line breaking, spans, per-glyph font fallback, stroke, drop shadow and a
/// gradient on the glyphs (DrawnUI SkiaLabel).
#[derive(Default)]
pub struct SkiaLabel {
    /// The label properties.
    pub p: LabelProps,
    /// Styled fragments; when not empty they replace `text`.
    pub spans: Vec<TextSpan>,
    layout: Layout,
    /// The text block of the last measure in pixels: width, height, the effects included.
    block: (f32, f32),
    /// Pixels the stroke and the shadow add to the block: width, height, and the stroke alone.
    extra: (f32, f32, f32),
    paragraph_space: f32,
    /// Pixels the glyphs can reach above the ascent, below the descent and, slanted, right of
    /// their advance (top, bottom, right).
    overshoot: (f32, f32, f32),
    /// The rect minus the padding and the scale of the last arrange, pixels: where spans are hit.
    hit_area: (Rect, f32),
    /// The drawing rect of the last arrange, pixels.
    arranged: Rect,
    /// The alias an empty `font_family` stood for at the last measure (the selectable text's font).
    default_family: String,
    /// The spans' text joined, the spoken label (DrawnUi.React DefaultAccessibilityLabel).
    spoken: String,
    /// Grows with every measure.
    serial: u32,
    shaders: RefCell<Shaders>,
    /// A tapped span whose handler runs on the next frame.
    pub(crate) pending_tap: Option<usize>,
    selection: Selection,
}

impl SkiaLabel {
    /// A label with a text, cached as Operations (as upstream).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: impl Into<String>) -> Build<SkiaLabel> {
        Build::new(SkiaLabel::default()).text(text.into()).use_cache(CacheType::Operations)
    }

    /// Number of lines the last measure produced.
    pub fn lines_count(&self) -> usize {
        self.layout.lines.len()
    }

    /// The lines of the last measure: text and width in pixels.
    pub fn lines(&self) -> impl Iterator<Item = (&str, f32)> {
        self.layout.lines.iter().map(|l| (&self.layout.text[l.text.clone()], l.width))
    }

    /// The runs of the last measure, line by line: text drawn with one font and one style.
    pub fn runs(&self) -> impl Iterator<Item = LabelRun<'_>> {
        let l = &self.layout;
        l.lines.iter().enumerate().flat_map(move |(line, ln)| {
            l.runs[ln.runs.clone()].iter().map(move |run| {
                let style = &l.styles[run.style];
                let fallback = if style.fallbacks.contains(&run.font) { run.font - style.fallbacks.start + 1 } else { 0 };
                LabelRun { line, text: &l.text[run.text.clone()], width: run.width, fallback, span: style.span }
            })
        })
    }

    /// The lines where the last arrange put them, for an editor: with
    /// `keep_spaces_on_line_breaks` every character of `text` is on a line, and the lines hold
    /// them in order, one `\n` between paragraphs.
    pub fn text_lines(&self) -> impl Iterator<Item = TextLine<'_>> {
        let mut next = 0;
        self.placed(self.hit_area.0).enumerate().map(move |(i, (line, origin, height))| {
            next += (line.new_paragraph && i > 0) as usize;
            let advances = &self.layout.advances[line.advances.clone()];
            let start = next;
            next += advances.len();
            TextLine { rect: Rect::from_xywh(origin.x, origin.y, line.width, height), start, advances }
        })
    }

    /// Height of the first line in pixels, from the last measure.
    pub fn measured_line_height(&self) -> f32 {
        self.layout.lines.first().map_or(0.0, |l| self.line_height(l))
    }

    /// Size of the text block in pixels, padding excluded, from the last measure.
    pub fn content_size(&self) -> Size {
        Size::new(self.block.0.ceil(), self.block.1.ceil())
    }

    /// Whether a line may break before the character at `index` (counted in characters) inside
    /// a run without spaces: next to a Chinese or Japanese character, never before closing
    /// punctuation, small kana or the long vowel mark, never after an opening bracket. Korean
    /// and Latin break at spaces only.
    pub fn can_break_inside_word(text: &str, index: usize) -> bool {
        let mut chars = text.chars().skip(index.wrapping_sub(1));
        index > 0 && matches!((chars.next(), chars.next()), (Some(before), Some(after)) if can_break_between(before, after))
    }

    fn line_height(&self, line: &Line) -> f32 {
        (line.ascent + line.descent) * self.p.line_height
    }

    /// Where the lines go inside `inner` (the rect minus the padding): each with its top-left in
    /// pixels and its height. The same geometry for paint and hit testing.
    fn placed<'a>(&'a self, inner: Rect) -> impl Iterator<Item = (&'a Line, Point, f32)> + 'a {
        let mut y = inner.top + self.extra.2 / 2.0;
        y += match self.p.vertical_text_alignment {
            TextAlignment::Start => 0.0,
            TextAlignment::Center => (inner.height() - self.block.1) / 2.0,
            TextAlignment::End => inner.height() - self.block.1,
        };
        self.layout.lines.iter().enumerate().map(move |(index, line)| {
            if line.new_paragraph && index > 0 {
                y += self.paragraph_space;
            }
            let width = line.width + self.extra.0;
            let x = inner.left + self.extra.2 / 2.0
                + match self.p.horizontal_text_alignment {
                    TextAlignment::Start => 0.0,
                    TextAlignment::Center => (inner.width() - width) / 2.0,
                    TextAlignment::End => inner.width() - width,
                };
            let height = self.line_height(line);
            let top = y;
            y += height * self.p.line_spacing;
            (line, Point::new(x, top), height)
        })
    }

    fn text_area(&self, rect: Rect, padding: Thickness, scale: f32) -> Rect {
        Rect::new(rect.left + padding.left * scale, rect.top + padding.top * scale, rect.right - padding.right * scale, rect.bottom - padding.bottom * scale)
    }

    fn has_stroke(&self) -> bool {
        self.p.stroke_width > 0.0 && self.p.stroke_color.a() > 0
    }

    fn has_shadow(&self) -> bool {
        self.p.drop_shadow_size > 0.0 && self.p.drop_shadow_color.a() > 0
    }

    /// The selected text (`accessibility_text_selectable`), empty when nothing is selected: from
    /// the label's text when the lines are taken from it verbatim, else from the drawn lines.
    pub fn selected_text(&self) -> String {
        let (source, _) = self.selection_source();
        source.chars().skip(self.selection.start).take(self.selection.length).collect()
    }

    /// The drawn lines on a source string, with the character index each line starts at (C#
    /// EnsureSelectionMap): the label's text when every line is found in it in order (the spaces
    /// dropped at wraps are then copied too), else the lines joined, a new line between paragraphs.
    fn selection_source(&self) -> (Cow<'_, str>, Vec<usize>) {
        let l = &self.layout;
        let text = if self.spans.is_empty() { &self.p.text } else { &self.spoken };
        let mut starts = Vec::with_capacity(l.lines.len());
        let (mut at, mut count) = (0, 0);
        for line in &l.lines {
            let value = &l.text[line.text.clone()];
            let Some(found) = text[at..].find(value) else { break };
            count += text[at..at + found].chars().count();
            starts.push(count);
            count += value.chars().count();
            at += found + value.len();
        }
        if starts.len() == l.lines.len() {
            return (Cow::Borrowed(text.as_str()), starts);
        }
        starts.clear();
        let (mut joined, mut count) = (String::new(), 0);
        for (i, line) in l.lines.iter().enumerate() {
            if i > 0 && line.new_paragraph {
                joined.push('\n');
                count += 1;
            }
            starts.push(count);
            let value = &l.text[line.text.clone()];
            joined.push_str(value);
            count += value.chars().count();
        }
        (Cow::Owned(joined), starts)
    }

    /// Where caret slot `slot` of a line is, pixels from the line's left (C# SlotX).
    fn slot_x(&self, line: &Line, slot: usize) -> f32 {
        let advances = &self.layout.advances[line.advances.clone()];
        if slot < advances.len() { advances[..slot].iter().sum() } else { line.width }
    }

    /// The text index under a point of the label's own space, pixels (C# IndexAt).
    fn index_at(&self, point: Point, starts: &[usize]) -> usize {
        let mut found = None;
        for (i, (line, origin, height)) in self.placed(self.hit_area.0).enumerate() {
            found = Some((i, line, origin));
            if point.y <= origin.y + height {
                break;
            }
        }
        let Some((i, line, origin)) = found else { return 0 };
        let advances = &self.layout.advances[line.advances.clone()];
        let slots = advances.len().min(self.layout.text[line.text.clone()].chars().count());
        let (x, mut at, mut best, mut best_distance) = (point.x - origin.x, 0.0, 0, f32::MAX);
        for slot in 0..=slots {
            let distance = (if slot < advances.len() { at } else { line.width } - x).abs();
            if distance < best_distance {
                (best, best_distance) = (slot, distance);
            }
            at += advances.get(slot).copied().unwrap_or(0.0);
        }
        starts.get(i).copied().unwrap_or(0) + best
    }

    /// The selection's rect on each line it covers, pixels, for lines placed in `inner`.
    fn selection_rects(&self, inner: Rect, scale: f32, starts: &[usize]) -> Vec<Rect> {
        let (start, end) = (self.selection.start, self.selection.start + self.selection.length);
        let count = self.layout.lines.len();
        let mut rects = Vec::new();
        for (i, (line, origin, height)) in self.placed(inner).enumerate() {
            let line_start = starts.get(i).copied().unwrap_or(0);
            let line_end = line_start + self.layout.text[line.text.clone()].chars().count();
            let (a, b) = (start.max(line_start), end.min(line_end));
            let continues = end > line_end && i + 1 < count;
            if a > b || (a == b && !continues) {
                continue;
            }
            let x0 = self.slot_x(line, a - line_start);
            let x1 = if continues && b == line_end { line.width } else { self.slot_x(line, b - line_start) };
            rects.push(Rect::new(origin.x + x0, origin.y, origin.x + x1.max(x0 + scale), origin.y + height));
        }
        rects
    }

    /// The Copy button of a touch selection and its font, inside the label's `bounds` (its cache
    /// clips and its hit box is the label): above the selection, else below it, else beside it on
    /// its first line, overlapping it only when none fits (C# DrawTextSelection).
    fn copy_button(&self, rects: &[Rect], bounds: Rect, scale: f32) -> Option<(Rect, Font)> {
        let (first, last) = (rects.first().filter(|_| self.selection.touch)?, rects.last()?);
        let font = self.layout.fonts.first()?.with_size(14.0 * scale)?;
        let text_width = font.measure_str(COPY_BUTTON_TEXT, None).0;
        let (height, width, gap) = (32.0 * scale, text_width + 28.0 * scale, 6.0 * scale);
        let centered = (first.center_x() - width / 2.0).clamp(bounds.left, bounds.left.max(bounds.right - width));
        let middle = (first.center_y() - height / 2.0).clamp(bounds.top, bounds.top.max(bounds.bottom - height));
        let (x, y) = if first.top - gap - height >= bounds.top {
            (centered, first.top - gap - height)
        } else if last.bottom + gap + height <= bounds.bottom {
            (centered, last.bottom + gap)
        } else if first.right + gap + width <= bounds.right {
            (first.right + gap, middle)
        } else if first.left - gap - width >= bounds.left {
            (first.left - gap - width, middle)
        } else {
            (centered, bounds.top.max(bounds.bottom - height))
        };
        Some((Rect::from_xywh(x, y, width, height), font))
    }

    /// The Copy button as the label drew it, where gestures land.
    fn copy_button_at(&self, cx: &GestureCx) -> Option<Rect> {
        let (bounds, (inner, scale)) = (cx.base().rect, self.hit_area);
        let (_, starts) = self.selection_source();
        self.copy_button(&self.selection_rects(inner, scale, &starts), bounds, scale).map(|(r, _)| r)
    }

    fn select(&mut self, cx: &mut GestureCx, start: usize, end: usize) {
        let length = end.saturating_sub(start);
        if (self.selection.start, self.selection.length) != (start, length) {
            (self.selection.start, self.selection.length) = (start, length);
            cx.invalidate(Dirty::DRAW);
        }
    }

    fn clear_selection(&mut self, cx: &mut GestureCx) {
        let s = &mut self.selection;
        (s.selecting, s.copy_pressed) = (false, false);
        if s.length > 0 || s.touch {
            (s.length, s.touch) = (0, false);
            cx.invalidate(Dirty::DRAW);
        }
    }

    /// Puts the selected text on the clipboard. False when nothing is selected.
    fn copy_selection(&self, cx: &mut GestureCx) -> bool {
        let text = self.selected_text();
        if text.is_empty() {
            return false;
        }
        cx.cx().set_clipboard(text);
        true
    }

    /// The index under `point` as an empty range, or the word around it.
    fn anchor_at(&self, point: Point, word: bool) -> (usize, usize) {
        let (source, starts) = self.selection_source();
        let index = self.index_at(point, &starts);
        if word { word_at(&source, index) } else { (index, index) }
    }

    /// Pointer handling of `accessibility_text_selectable` (C# ProcessTextSelection). Mouse: a
    /// press starts a selection, a drag extends it, a double click takes a word. Touch: a long
    /// press takes a word, a drag after it extends it, a Copy button shows. False lets the gesture
    /// go on as usual.
    fn select_text(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> bool {
        let (point, mouse) = (cx.point, !gesture.touch);
        match gesture.kind {
            GestureKind::Down => {
                if self.selection.touch && self.selection.length > 0 && self.copy_button_at(cx).is_some_and(|r| r.contains(point)) {
                    self.selection.copy_pressed = true;
                    return true;
                }
                if !mouse {
                    // Touch: a long press selects, a drag keeps scrolling.
                    return false;
                }
                let (time, at) = self.selection.last_click;
                let second = gesture.time_ms - time < 450.0 && (point - at).length() < 8.0 * cx.base().scale;
                self.selection.last_click = (if second { f64::NEG_INFINITY } else { gesture.time_ms }, point);
                let anchor = self.anchor_at(point, second);
                (self.selection.anchor, self.selection.touch, self.selection.selecting) = (anchor, false, true);
                self.select(cx, anchor.0, anchor.1);
                cx.focus();
                true
            }
            GestureKind::LongPressing => {
                if mouse {
                    return self.selection.selecting;
                }
                let anchor = self.anchor_at(point, true);
                (self.selection.anchor, self.selection.touch, self.selection.selecting) = (anchor, true, true);
                self.select(cx, anchor.0, anchor.1);
                cx.focus();
                true
            }
            GestureKind::Panning => {
                if !self.selection.selecting {
                    return false;
                }
                let (index, _) = self.anchor_at(point, false);
                let (a, b) = self.selection.anchor;
                self.select(cx, a.min(index), b.max(index));
                true
            }
            GestureKind::Up => {
                if std::mem::take(&mut self.selection.copy_pressed) {
                    if self.copy_button_at(cx).is_some_and(|r| r.contains(point)) {
                        self.copy_selection(cx);
                        self.clear_selection(cx);
                    }
                    return true;
                }
                std::mem::take(&mut self.selection.selecting)
            }
            GestureKind::Tapped => {
                if self.selection.length == 0 {
                    // Nothing selected: span links still get their tap.
                    return false;
                }
                if !mouse {
                    self.clear_selection(cx);
                }
                // A click that selected (a double click) is ours: unused, the canvas would read it
                // as a tap on empty space and take the focus, and with it the selection.
                true
            }
            _ => false,
        }
    }

    /// The selection over the lines just drawn, and the Copy button of a touch selection.
    fn paint_selection(&self, canvas: &skia_safe::Canvas, inner: Rect, bounds: Rect, scale: f32) {
        let (_, starts) = self.selection_source();
        let rects = self.selection_rects(inner, scale, &starts);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(TEXT_SELECTION_COLOR);
        for rect in &rects {
            canvas.draw_rect(rect, &paint);
        }
        let Some((button, font)) = self.copy_button(&rects, bounds, scale) else { return };
        let radius = button.height() / 2.0;
        paint.set_color(Color::from_argb(235, 33, 37, 41));
        canvas.draw_round_rect(button, radius, radius, &paint);
        paint.set_color(Color::WHITE);
        let (text_width, metrics) = (font.measure_str(COPY_BUTTON_TEXT, None).0, font.metrics().1);
        let origin = (button.center_x() - text_width / 2.0, button.center_y() - (metrics.ascent + metrics.descent) / 2.0);
        canvas.draw_str(COPY_BUTTON_TEXT, origin, &font, &paint);
    }

    /// The span with a tap handler under a point in the label's own space, pixels.
    fn span_at(&self, point: Point) -> Option<usize> {
        if self.spans.is_empty() {
            return None;
        }
        let (inner, scale) = self.hit_area;
        let spacing = scale * (self.p.character_spacing - 1.0);
        for (line, origin, height) in self.placed(inner) {
            if point.y < origin.y || point.y >= origin.y + height {
                continue;
            }
            let mut x = origin.x;
            for run in &self.layout.runs[line.runs.clone()] {
                let width = run.width - spacing;
                if point.x >= x && point.x < x + width
                    && let Some(span) = self.layout.styles[run.style].span
                    && self.spans[span].has_tap_handler()
                {
                    return Some(span);
                }
                x += run.width;
            }
        }
        None
    }
}

impl Has<LabelProps> for SkiaLabel {
    fn part(&self) -> &LabelProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut LabelProps {
        &mut self.p
    }
}

impl<T: Has<LabelProps>> Build<T> {
    /// Styled fragments (`TextSpan`); when given they replace `text`.
    pub fn spans(mut self, spans: impl IntoSpans) -> Self {
        if let Some(label) = part_mut::<SkiaLabel>(self.control_mut() as &mut dyn Control) {
            spans.push_into(&mut label.spans);
        }
        self
    }
}

impl Mut<'_, SkiaLabel> {
    /// Replaces the spans.
    pub fn set_spans(&mut self, spans: impl IntoSpans) {
        let label = self.control_mut();
        label.spans.clear();
        spans.push_into(&mut label.spans);
        self.mark(Dirty::MEASURE);
    }
}

/// The word around a character index, or the character alone when it is not part of one (C#
/// WordAt): letters, digits, `_` and `'` make words.
fn word_at(source: &str, index: usize) -> (usize, usize) {
    let chars: Vec<char> = source.chars().collect();
    let word = |c: &char| c.is_alphanumeric() || *c == '_' || *c == '\'';
    let Some(mut i) = chars.len().checked_sub(1).map(|last| index.min(last)) else { return (0, 0) };
    if !word(&chars[i]) && i > 0 && word(&chars[i - 1]) {
        i -= 1;
    }
    if !word(&chars[i]) {
        return (i, i + 1);
    }
    let start = chars[..i].iter().rposition(|c| !word(c)).map_or(0, |p| p + 1);
    let end = chars[i..].iter().position(|c| !word(c)).map_or(chars.len(), |p| i + p);
    (start, end)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x312F // CJK punctuation, hiragana, katakana, bopomofo
        | 0x3190..=0x33FF // kanbun, strokes, katakana extension, enclosed and compatibility
        | 0x3400..=0x4DBF // ideographs extension A
        | 0x4E00..=0x9FFF // ideographs
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xFF00..=0xFFEF // full-width and half-width forms
        | 0x20000..=0x3FFFF) // ideographs extensions B and later
}

fn can_break_between(before: char, after: char) -> bool {
    (is_cjk(before) || is_cjk(after)) && !NO_BREAK_BEFORE.contains(after) && !NO_BREAK_AFTER.contains(before)
}

/// Appends a word transformed (DrawnUi.React Transform; Titlecase = the first character of a word).
fn push_transformed(out: &mut String, word: &str, transform: TextTransform) {
    match transform {
        TextTransform::None => out.push_str(word),
        TextTransform::Lowercase => out.extend(word.chars().flat_map(char::to_lowercase)),
        TextTransform::Uppercase => out.extend(word.chars().flat_map(char::to_uppercase)),
        TextTransform::Titlecase => {
            let mut chars = word.chars();
            out.extend(chars.next().into_iter().flat_map(char::to_uppercase));
            out.push_str(chars.as_str());
        }
    }
}

/// Default-ignorable code points: zero width space, (non-)joiner and direction marks, word joiners,
/// variation selectors (U+FE0F after an emoji), tags. Text is not shaped, so where a label's fonts
/// have no glyph for one it draws nothing and takes no room, never the missing-glyph box (C# keeps
/// them in the font of the glyph before them, where shaping hides them).
fn ignorable(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0000}'..='\u{E0FFF}')
}

/// Glyphs and advances of `text` in `font`.
fn glyph_widths(font: &Font, text: &str, glyphs: &mut Vec<GlyphId>, widths: &mut Vec<f32>) {
    let count = font.count_str(text);
    glyphs.resize(count, 0);
    widths.resize(count, 0.0);
    font.str_to_glyphs(text, glyphs);
    font.get_widths(glyphs, widths);
}

/// What one measure wraps with. Widths are pixels.
struct Params {
    wrap: bool,
    truncate: bool,
    max_width: f32,
    max_height: f32,
    max_lines: i32,
    /// Extra space after every glyph (CharacterSpacing).
    spacing: f32,
    line_height: f32,
    line_spacing: f32,
    paragraph_space: f32,
}

impl Layout {
    fn clear(&mut self) {
        self.lines.clear();
        self.runs.clear();
        self.text.clear();
        self.styles.clear();
        self.fonts.clear();
        self.source.clear();
        self.tokens.clear();
        self.paragraphs.clear();
        self.advances.clear();
    }

    /// One style: its font, the fallback fonts and the metrics of the main one.
    #[allow(clippy::too_many_arguments)]
    fn add_style(&mut self, fonts: &Fonts, family: &str, fallbacks: &str, weight: i32, italic: bool, size_px: f32, span: Option<usize>, system: bool) -> Option<usize> {
        let font = fonts.font_for(family, weight, italic, size_px)?;
        let (_, metrics) = font.metrics();
        let space = font.measure_str(" ", None).0;
        let main = self.fonts.len();
        self.fonts.push(font);
        let first = self.fonts.len();
        for alias in fallbacks.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            if let Some(font) = fonts.font_for(alias, weight, italic, size_px) {
                self.fonts.push(font);
            }
        }
        let fallbacks = first..self.fonts.len();
        let (ascent, descent, system_faces) = (-metrics.ascent, metrics.descent, Vec::new());
        let style = Style { font: main, fallbacks, ascent, descent, size_px, space, span, weight, italic, system, system_faces };
        self.styles.push(style);
        Some(self.styles.len() - 1)
    }

    /// DrawnUi.React Tokenize: one paragraph per `\n` (and, as C#, per U+2028), a token per word.
    /// A fragment that does not start with a space glues to the previous word (no break
    /// opportunity between them). A character no font of the style has becomes `missing` (C#
    /// FallbackCharacter) when one is given.
    #[allow(clippy::too_many_arguments)]
    fn add_tokens(&mut self, fonts: &Fonts, text: &str, style: usize, transform: TextTransform, missing: Option<char>, keep: bool, wrap: bool) {
        for (p, part) in text.split(['\n', '\u{2028}']).enumerate() {
            if p > 0 || self.paragraphs.is_empty() {
                let at = self.tokens.len();
                self.paragraphs.push(at..at);
            }
            let para = self.paragraphs.len() - 1;
            if keep {
                // A token is a word with the spaces after it; a paragraph may start with spaces.
                let mut rest = part;
                while !rest.is_empty() {
                    let word = rest.find(' ').unwrap_or(rest.len());
                    // Without wrapping a paragraph is one token: one glyph lookup.
                    let end = if wrap { rest.len() - rest[word..].trim_start_matches(' ').len() } else { rest.len() };
                    let start = self.source.len();
                    push_transformed(&mut self.source, &rest[..end], transform);
                    self.add_system_faces(fonts, start, style);
                    if let Some(missing) = missing {
                        self.replace_missing(start, style, missing);
                    }
                    self.tokens.push(Token { start, end: self.source.len(), style, space_before: false, trailing_space: false });
                    self.paragraphs[para].end = self.tokens.len();
                    rest = &rest[end..];
                }
                continue;
            }
            let leading_space = part.starts_with(' ');
            for (w, word) in part.split(' ').enumerate() {
                if word.is_empty() {
                    continue;
                }
                let start = self.source.len();
                push_transformed(&mut self.source, word, transform);
                self.add_system_faces(fonts, start, style);
                if let Some(missing) = missing {
                    self.replace_missing(start, style, missing);
                }
                let tokens = &mut self.paragraphs[para];
                let space_before = w > 0 || (tokens.end > tokens.start && leading_space);
                self.tokens.push(Token { start, end: self.source.len(), style, space_before, trailing_space: false });
                tokens.end = self.tokens.len();
            }
            let tokens = self.paragraphs[para].clone();
            if part.ends_with(' ') && let Some(last) = self.tokens[tokens].last_mut() {
                last.trailing_space = true;
            }
        }
    }

    /// True when no font of the style has a glyph for `c`.
    fn lacks(&self, style: usize, c: char) -> bool {
        let st = &self.styles[style];
        let mut faces = std::iter::once(st.font).chain(st.fallbacks.clone()).chain(st.system_faces.iter().copied());
        faces.all(|f| self.fonts[f].unichar_to_glyph(c as i32) == 0)
    }

    /// A character from `start` on that no font of the style has gets the system font that has
    /// it, one font per face and style (C# SkiaRichLabel `BuildSpanData`: MatchCharacter).
    fn add_system_faces(&mut self, fonts: &Fonts, start: usize, style: usize) {
        if !self.styles[style].system {
            return;
        }
        let mut at = start;
        while let Some(c) = self.source[at..].chars().next() {
            at += c.len_utf8();
            if c == ' ' || !self.lacks(style, c) {
                continue;
            }
            let Some(typeface) = fonts.match_character(c) else { continue };
            let st = &self.styles[style];
            let font = fonts.font_from(&typeface, st.weight, st.italic, st.size_px);
            self.fonts.push(font);
            let index = self.fonts.len() - 1;
            self.styles[style].system_faces.push(index);
        }
    }

    /// Replaces the characters from `start` on that neither the font of the style nor a fallback
    /// has a glyph for.
    fn replace_missing(&mut self, start: usize, style: usize, missing: char) {
        let lacks = |c: char| !ignorable(c) && self.lacks(style, c);
        if self.source[start..].chars().any(lacks) {
            let word: String = self.source[start..].chars().map(|c| if lacks(c) { missing } else { c }).collect();
            self.source.truncate(start);
            self.source.push_str(&word);
        }
    }

    /// A fragment ending with a space puts the space before the next token.
    fn spread_trailing_spaces(&mut self) {
        for para in &self.paragraphs {
            for i in para.clone() {
                if self.tokens[i].trailing_space && i + 1 < para.end {
                    self.tokens[i + 1].space_before = true;
                }
            }
        }
    }

    /// DrawnUi.React Segment: splits the text of a token into runs by glyph availability, the main
    /// font or the first fallback that has the glyph (spaces always stay on the main font), and
    /// appends them to the word with the advance of every character.
    fn segment(&mut self, token: Token, spacing: f32) {
        let Layout { styles, fonts, source, glyphs, widths, word_runs, word_widths, .. } = self;
        let style = &styles[token.style];
        let word = &source[token.start..token.end];
        let count = fonts[style.font].count_str(word);
        glyphs.resize(count, 0);
        fonts[style.font].str_to_glyphs(word, &mut glyphs[..count]);
        // The fallbacks are asked only when the main font lacks a glyph (DrawnUi.React asks always),
        // the system fonts found for the text after them.
        let aliases = style.fallbacks.len();
        let face_at = |k: usize| if k < aliases { style.fallbacks.start + k } else { style.system_faces[k - aliases] };
        let faces = if glyphs.contains(&0) { 1 + aliases + style.system_faces.len() } else { 1 };
        glyphs.resize(count * faces, 0);
        for k in 0..faces - 1 {
            fonts[face_at(k)].str_to_glyphs(word, &mut glyphs[(k + 1) * count..(k + 2) * count]);
        }
        let font_of = |i: usize, c: char| -> (usize, usize) {
            if c == ' ' || glyphs[i] != 0 {
                return (style.font, 0);
            }
            match (0..faces - 1).find(|k| glyphs[(k + 1) * count + i] != 0) {
                Some(k) => (face_at(k), k + 1),
                None => (style.font, 0),
            }
        };
        let mut run_start = (0usize, 0usize);
        let mut current = None;
        let mut flush = |from: (usize, usize), to: (usize, usize), (font, face): (usize, usize)| {
            let ids = &glyphs[face * count + from.0..face * count + to.0];
            widths.resize(ids.len(), 0.0);
            fonts[font].get_widths(ids, widths);
            for ((c, &id), w) in word[from.1..to.1].chars().zip(ids).zip(widths.iter_mut()) {
                if id == 0 && ignorable(c) {
                    *w = 0.0;
                }
            }
            let mut width = 0.0;
            for w in widths.iter() {
                word_widths.push(w + spacing);
                width += w + spacing;
            }
            let text = token.start + from.1..token.start + to.1;
            word_runs.push(Run { text, style: token.style, font, width, blob: None });
        };
        for (i, (at, c)) in word.char_indices().enumerate() {
            let font = font_of(i, c);
            match current {
                Some(f) if f == font => {}
                Some(f) => {
                    flush(run_start, (i, at), f);
                    run_start = (i, at);
                    current = Some(font);
                }
                None => current = Some(font),
            }
        }
        if let Some(f) = current {
            flush(run_start, (count, word.len()), f);
        }
        let spaces = word.len() - word.trim_end_matches(' ').len();
        self.hang = self.word_widths[self.word_widths.len() - spaces..].iter().sum();
    }

    /// Appends a run to the open line, merged into the last one when both share font and style.
    fn append(&mut self, open: &mut Open, style: usize, font: usize, text: Range<usize>, width: f32) {
        let start = self.text.len();
        self.text.push_str(&self.source[text]);
        let has_runs = self.runs.len() > open.runs;
        match self.runs.last_mut() {
            Some(last) if has_runs && last.font == font && last.style == style => {
                last.text.end = self.text.len();
                last.width += width;
            }
            _ => self.runs.push(Run { text: start..self.text.len(), style, font, width, blob: None }),
        }
        open.width += width;
        let st = &self.styles[style];
        open.ascent = open.ascent.max(st.ascent);
        open.descent = open.descent.max(st.descent);
    }

    /// Appends the characters `chars` of the word.
    fn append_chars(&mut self, open: &mut Open, chars: Range<usize>) {
        let mut seen = 0;
        for r in 0..self.word_runs.len() {
            let run = &self.word_runs[r];
            let text = &self.source[run.text.clone()];
            let count = text.chars().count();
            let (from, to) = (chars.start.max(seen), chars.end.min(seen + count));
            if from < to {
                let byte = |k: usize| text.char_indices().nth(k - seen).map_or(text.len(), |(b, _)| b);
                let (a, b) = (byte(from), byte(to));
                let width: f32 = self.word_widths[from..to].iter().sum();
                let (style, font, start) = (run.style, run.font, run.text.start);
                self.append(open, style, font, start + a..start + b, width);
            }
            seen += count;
        }
    }

    fn append_word(&mut self, open: &mut Open) {
        for r in 0..self.word_runs.len() {
            let run = &self.word_runs[r];
            let (style, font, text, width) = (run.style, run.font, run.text.clone(), run.width);
            self.append(open, style, font, text, width);
        }
    }

    /// Drops the first `count` characters of the word (they went on a line).
    fn drop_chars(&mut self, count: usize) {
        self.word_widths.drain(..count);
        let source = &self.source;
        let mut left = count;
        self.word_runs.retain_mut(|run| {
            let text = &source[run.text.clone()];
            let chars = text.chars().count();
            if left >= chars {
                left -= chars;
                return false;
            }
            if left > 0 {
                run.text.start += text.char_indices().nth(left).map_or(text.len(), |(b, _)| b);
                left = 0;
            }
            true
        });
        let mut k = 0;
        for run in &mut self.word_runs {
            let chars = source[run.text.clone()].chars().count();
            run.width = self.word_widths[k..k + chars].iter().sum();
            k += chars;
        }
    }

    fn word_text_has_cjk(&self) -> bool {
        self.word_runs.iter().any(|r| self.source[r.text.clone()].chars().any(is_cjk))
    }

    /// C# FitAtBreak: the last place inside the word where a Chinese / Japanese line may break
    /// and `prefix` plus the word up to there still fits. Characters.
    fn fit_cjk(&self, prefix: f32, max_width: f32, spacing: f32) -> Option<usize> {
        if !self.word_text_has_cjk() {
            return None;
        }
        let (mut x, mut best, mut before, mut k) = (prefix, None, ' ', 0);
        for run in &self.word_runs {
            for c in self.source[run.text.clone()].chars() {
                if k > 0 && can_break_between(before, c) {
                    if x - spacing > max_width {
                        return best;
                    }
                    best = Some(k);
                }
                x += self.word_widths[k];
                before = c;
                k += 1;
            }
        }
        best
    }

    fn close(&mut self, open: &mut Open, new_paragraph: bool, spacing: f32, empty_style: usize) {
        let has_runs = self.runs.len() > open.runs;
        let (ascent, descent) = if has_runs {
            (open.ascent, open.descent)
        } else {
            let st = &self.styles[empty_style];
            (st.ascent, st.descent)
        };
        let width = if has_runs { open.width - spacing } else { 0.0 };
        let line = Line { runs: open.runs..self.runs.len(), text: open.text..self.text.len(), width, ascent, descent, new_paragraph, advances: 0..0 };
        self.lines.push(line);
        *open = Open { runs: self.runs.len(), text: self.text.len(), width: 0.0, ascent: 0.0, descent: 0.0 };
    }

    /// DrawnUi.React LayoutLines for one paragraph, plus the C# break inside Chinese / Japanese.
    fn layout_paragraph(&mut self, para: Range<usize>, prm: &Params, main: usize, keep: bool) {
        let mut open = Open { runs: self.runs.len(), text: self.text.len(), width: 0.0, ascent: 0.0, descent: 0.0 };
        let mut last_style = main;
        let mut new_paragraph = true;
        let sp = prm.spacing;
        let mut i = para.start;
        while i < para.end {
            let token = self.tokens[i];
            last_style = token.style;
            self.word_runs.clear();
            self.word_widths.clear();
            self.segment(token, sp);
            // Glued fragments (no space between spans) wrap as one word.
            while !keep && i + 1 < para.end && !self.tokens[i + 1].space_before {
                i += 1;
                self.segment(self.tokens[i], sp);
            }
            i += 1;
            loop {
                let has_runs = self.runs.len() > open.runs;
                let word_width: f32 = self.word_runs.iter().map(|r| r.width).sum();
                // Spaces that end the word hang past the line's end (kept spaces only).
                let hang = if keep { self.hang } else { 0.0 };
                let space = token.space_before && has_runs;
                let space_width = if space { self.styles[token.style].space + sp } else { 0.0 };
                let fits = |width: f32| width - sp <= prm.max_width;
                if !prm.wrap || fits(open.width + space_width + word_width - hang) || (!has_runs && fits(word_width - hang)) {
                    if space {
                        let (style, font) = (token.style, self.styles[token.style].font);
                        let at = self.source.len();
                        self.source.push(' ');
                        self.append(&mut open, style, font, at..at + 1, space_width);
                    }
                    self.append_word(&mut open);
                    break;
                }
                if has_runs {
                    // Chinese / Japanese: the word fills the line up to its last break that fits.
                    if let Some(k) = self.fit_cjk(open.width + space_width, prm.max_width, sp).filter(|k| *k > 0) {
                        if space {
                            let (style, font) = (token.style, self.styles[token.style].font);
                            let at = self.source.len();
                            self.source.push(' ');
                            self.append(&mut open, style, font, at..at + 1, space_width);
                        }
                        self.append_chars(&mut open, 0..k);
                        self.close(&mut open, new_paragraph, sp, last_style);
                        new_paragraph = false;
                        self.drop_chars(k);
                        continue;
                    }
                    self.close(&mut open, new_paragraph, sp, last_style);
                    new_paragraph = false;
                    continue;
                }
                if let Some(k) = self.fit_cjk(0.0, prm.max_width, sp).filter(|k| *k > 0) {
                    self.append_chars(&mut open, 0..k);
                    self.close(&mut open, new_paragraph, sp, last_style);
                    new_paragraph = false;
                    self.drop_chars(k);
                    continue;
                }
                // A word wider than the line breaks by characters.
                let count = self.word_widths.len();
                for k in 0..count {
                    let width = self.word_widths[k];
                    if self.runs.len() > open.runs && !fits(open.width + width) {
                        self.close(&mut open, new_paragraph, sp, last_style);
                        new_paragraph = false;
                    }
                    self.append_chars(&mut open, k..k + 1);
                }
                break;
            }
        }
        self.close(&mut open, new_paragraph, sp, last_style);
    }

    /// Cuts the lines to `kept` and ends the last one with the ellipsis when the mode asks for
    /// it (DrawnUi.React LayoutLines, the MaxLines part).
    fn truncate(&mut self, kept: usize, prm: &Params, main: usize) {
        let Some(last) = self.lines.get(kept - 1) else { return };
        let (runs_end, text_end) = (last.runs.end, last.text.end);
        self.lines.truncate(kept);
        self.runs.truncate(runs_end);
        self.text.truncate(text_end);
        if !prm.truncate {
            return;
        }
        let sp = prm.spacing;
        let line = self.lines.len() - 1;
        let tail_style = self.runs.last().map_or(main, |r| r.style);
        self.word_runs.clear();
        self.word_widths.clear();
        let at = self.source.len();
        self.source.push_str(ELLIPSIS);
        let token = Token { start: at, end: self.source.len(), style: tail_style, space_before: false, trailing_space: false };
        self.segment(token, sp);
        let ellipsis: f32 = self.word_runs.iter().map(|r| r.width).sum();
        let mut width: f32 = self.runs[self.lines[line].runs.clone()].iter().map(|r| r.width).sum();
        while self.runs.len() > self.lines[line].runs.start && width + ellipsis - sp > prm.max_width {
            let r = self.runs.len() - 1;
            let start = self.runs[r].text.start;
            width -= self.runs[r].width;
            // One character less, and no space at the end.
            self.text.pop();
            while self.text.len() > start && self.text.ends_with(' ') {
                self.text.pop();
            }
            if self.text.len() <= start {
                self.text.truncate(start);
                self.runs.pop();
                self.lines[line].runs.end -= 1;
                continue;
            }
            self.runs[r].text.end = self.text.len();
            let font = &self.fonts[self.runs[r].font];
            let text = &self.text[start..];
            self.runs[r].width = font.measure_str(text, None).0 + sp * font.count_str(text) as f32;
            width += self.runs[r].width;
        }
        let mut open = Open { runs: self.lines[line].runs.start, text: self.lines[line].text.start, width, ascent: 0.0, descent: 0.0 };
        self.append_word(&mut open);
        let l = &mut self.lines[line];
        l.runs.end = self.runs.len();
        l.text.end = self.text.len();
        l.width = open.width - sp;
        let st = &self.styles[tail_style];
        l.ascent = l.ascent.max(st.ascent);
        l.descent = l.descent.max(st.descent);
    }

    /// One text blob per run: the glyphs on the baseline from x = 0.
    fn build_blobs(&mut self, spacing: f32) {
        let Layout { lines, runs, text, fonts, glyphs, widths, advances, .. } = self;
        for line in lines.iter_mut() {
            let first = advances.len();
            for run in &mut runs[line.runs.clone()] {
                let font = &fonts[run.font];
                glyph_widths(font, &text[run.text.clone()], glyphs, widths);
                // An ignorable code point the font lacks: the space glyph (nothing), no room.
                for (k, c) in text[run.text.clone()].chars().enumerate() {
                    if glyphs[k] == 0 && ignorable(c) {
                        glyphs[k] = font.unichar_to_glyph(' ' as i32);
                        widths[k] = 0.0;
                    }
                }
                advances.extend(widths.iter().map(|w| w + spacing));
                if glyphs.is_empty() {
                    continue;
                }
                let mut builder = TextBlobBuilder::new();
                let (blob_glyphs, positions) = builder.alloc_run_pos_h(font, glyphs.len(), 0.0, None);
                blob_glyphs.copy_from_slice(glyphs);
                let mut x = 0.0;
                for (position, width) in positions.iter_mut().zip(widths.iter()) {
                    *position = x;
                    x += width + spacing;
                }
                run.blob = builder.make();
            }
            line.advances = first..advances.len();
        }
    }
}

/// A frame animator that runs the handler of a tapped span with the app state, then ends.
fn fire_span_tap(id: ControlId, _time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return tick };
    let mut queue = Vec::new();
    if let Some(control) = node.kind.as_deref_mut() {
        // A rich label answers a tap on a link itself.
        let mut link = None;
        if let Some(rich) = part_mut::<SkiaRichLabel>(control) {
            link = rich.take_link_tap();
        }
        let mut span = None;
        if link.is_none()
            && let Some(label) = part_mut::<SkiaLabel>(control)
            && let Some(index) = label.pending_tap.take()
            && let Some(handler) = label.spans.get_mut(index).and_then(|s| s.tapped.take())
        {
            span = Some((index, handler));
        }
        if let Some((url, mut handler)) = link {
            handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, cx, &url);
            tick.state_touched = true;
            if let Some(rich) = part_mut::<SkiaRichLabel>(control) {
                rich.link_tapped = Some(handler);
            }
        } else if let Some((index, mut handler)) = span {
            handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, cx);
            tick.state_touched = true;
            if let Some(span) = part_mut::<SkiaLabel>(control).and_then(|l| l.spans.get_mut(index)) {
                span.tapped = Some(handler);
            }
        }
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    tick
}

impl Control for SkiaLabel {
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let scale = cx.scale;
        let padding = cx.base().p.padding;
        let (px, py) = (padding.horizontal() * scale, padding.vertical() * scale);
        let p = &self.p;
        let layout = &mut self.layout;
        layout.clear();
        self.serial = self.serial.wrapping_add(1);
        self.block = (0.0, 0.0);

        // Styles: the label's own, then one per span.
        let bold = p.font_attributes.is_bold();
        let italic = p.font_attributes.is_italic();
        let weight = if p.font_weight > 0 { p.font_weight } else if bold { BOLD } else { 0 };
        let system = p.system_font_fallback;
        let Some(main) = layout.add_style(cx.fonts, &p.font_family, &p.font_family_fallback, weight, italic, p.font_size * scale, None, system) else {
            return Size::new(px, py);
        };
        let (mut top, mut bottom) = (0f32, 0f32);
        if p.font_family.is_empty() && let Some(alias) = cx.fonts.default_alias() && self.default_family != alias {
            self.default_family = alias.to_owned();
        }
        let keep = p.keep_spaces_on_line_breaks;
        let wrap = p.line_break_mode != LineBreakMode::NoWrap;
        if self.spans.is_empty() {
            layout.add_tokens(cx.fonts, &p.text, main, p.text_transform, p.fallback_character, keep, wrap);
        } else {
            for (index, span) in self.spans.iter().enumerate().filter(|(_, s)| s.is_visible && !s.text.is_empty()) {
                let span_weight = if span.font_weight > 0 {
                    span.font_weight
                } else if span.is_bold {
                    BOLD
                } else {
                    weight
                };
                let family = span.font_family.as_deref().unwrap_or(&p.font_family);
                let size = span.font_size.unwrap_or(p.font_size) * scale;
                let fallback = &p.font_family_fallback;
                let Some(style) = layout.add_style(cx.fonts, family, fallback, span_weight, italic || span.is_italic, size, Some(index), system) else { continue };
                layout.add_tokens(cx.fonts, &span.text, style, p.text_transform, p.fallback_character, keep, wrap);
            }
        }
        let mut right = 0f32;
        for font in &layout.fonts {
            let (_, m) = font.metrics();
            top = top.max(m.ascent - m.top);
            bottom = bottom.max(m.bottom - m.descent);
            // A synthetic italic leans right by its skew times the height above the baseline.
            right = right.max(-font.skew_x() * -m.top);
        }
        self.overshoot = (top, bottom, right);
        self.spoken.clear();
        for span in &self.spans {
            self.spoken.push_str(&span.text);
        }
        if self.spans.is_empty() && p.text.is_empty() && !keep {
            return Size::new(px, py);
        }
        // Text of spaces only, or only hidden spans: one empty line (DrawnUi.React).
        if layout.paragraphs.is_empty() {
            layout.paragraphs.push(0..0);
        }
        layout.spread_trailing_spaces();

        let stroke = if p.stroke_width > 0.0 && p.stroke_color.a() > 0 { p.stroke_width * 2.0 * scale } else { 0.0 };
        let (shadow_w, shadow_h) = if p.drop_shadow_size > 0.0 && p.drop_shadow_color.a() > 0 {
            ((p.drop_shadow_size + p.drop_shadow_offset_x) * scale, (p.drop_shadow_size + p.drop_shadow_offset_y) * scale)
        } else {
            (0.0, 0.0)
        };
        self.extra = (stroke + shadow_w, stroke + shadow_h, stroke);
        let main_style = &layout.styles[main];
        let line = (main_style.ascent + main_style.descent) * p.line_height;
        self.paragraph_space = line * p.line_spacing * p.paragraph_spacing;
        let prm = Params {
            wrap: p.line_break_mode != LineBreakMode::NoWrap,
            truncate: matches!(p.line_break_mode, LineBreakMode::TailTruncation | LineBreakMode::HeadTruncation | LineBreakMode::MiddleTruncation),
            max_width: (width - px - self.extra.0).max(0.0),
            max_height: (height - py - self.extra.1).max(0.0),
            max_lines: p.max_lines,
            spacing: scale * (p.character_spacing - 1.0),
            line_height: p.line_height,
            line_spacing: p.line_spacing,
            paragraph_space: self.paragraph_space,
        };
        for i in 0..layout.paragraphs.len() {
            let para = layout.paragraphs[i].clone();
            layout.layout_paragraph(para, &prm, main, keep);
        }

        // MaxLines and the height limit (C#): lines are kept while they fit, at least one.
        let mut kept = layout.lines.len();
        if prm.max_lines > 0 {
            kept = kept.min(prm.max_lines as usize);
        }
        let (mut used, mut fit) = (0.0, 0);
        for (i, l) in layout.lines.iter().enumerate() {
            used += (l.ascent + l.descent) * prm.line_height * prm.line_spacing;
            if l.new_paragraph && i > 0 {
                used += prm.paragraph_space;
            }
            if used > prm.max_height && fit > 0 {
                break;
            }
            fit += 1;
        }
        kept = kept.min(fit);
        if kept < layout.lines.len() {
            layout.truncate(kept, &prm, main);
        }
        layout.build_blobs(prm.spacing);

        let lines = &layout.lines;
        let text_width = lines.iter().fold(0f32, |w, l| w.max(l.width));
        let mut text_height = 0.0;
        for (i, l) in lines.iter().enumerate() {
            let lh = (l.ascent + l.descent) * prm.line_height;
            text_height += if i + 1 < lines.len() { lh * prm.line_spacing } else { lh };
            if l.new_paragraph && i > 0 {
                text_height += prm.paragraph_space;
            }
        }
        self.block = (text_width + self.extra.0, text_height + self.extra.1);
        Size::new(self.block.0.ceil() + px, self.block.1.ceil() + py)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let base = cx.base();
        self.hit_area = (self.text_area(base.rect, base.p.padding, cx.scale), cx.scale);
        self.arranged = base.rect;
    }

    /// React GetAccessibilityTextLines: every laid-out line in points from the drawing rect's
    /// corner, with the CSS font that lines the selectable text up with the glyphs.
    fn accessibility_text_lines(&self, scale: f32) -> Vec<crate::ui::AccessibilityTextLine> {
        let p = &self.p;
        let main = if p.font_family.is_empty() { self.default_family.as_str() } else { p.font_family.as_str() };
        let fallbacks = p.font_family_fallback.split(',').map(str::trim);
        let family = std::iter::once(main).chain(fallbacks).filter(|f| !f.is_empty()).collect::<Vec<_>>().join(", ");
        let font_weight = if p.font_weight > 0 { p.font_weight } else if p.font_attributes.is_bold() { 600 } else { 400 };
        let (origin, s) = (Point::new(self.arranged.left, self.arranged.top), scale.max(0.1));
        self.placed(self.hit_area.0)
            .map(|(line, at, height)| crate::ui::AccessibilityTextLine {
                text: self.layout.text[line.text.clone()].to_owned(),
                left: (at.x - origin.x) / s,
                top: (at.y - origin.y) / s,
                width: line.width / s,
                height: height / s,
                font_family: family.clone(),
                font_weight,
                font_size: p.font_size,
            })
            .collect()
    }

    /// The hand over a span that takes taps (DrawnUi.React WantsPointerCursor).
    fn cursor(&self, local: Point) -> Option<Cursor> {
        self.span_at(local).map(|_| Cursor::Pointer)
    }

    /// The text, or the text of the spans (C# OnTextInternalChanged).
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        let text = if self.spans.is_empty() { &self.p.text } else { &self.spoken };
        (!text.is_empty()).then(|| text.as_str().into())
    }

    /// Glyph ink past the line box and the shadow beyond the measured band, whole pixels (C#
    /// ComputeEffectsMargin): a cache keeps them.
    fn effects_margin(&self, scale: f32) -> Thickness {
        let (mut left, mut top, mut right, mut bottom) = (0f32, self.overshoot.0.ceil(), self.overshoot.2.ceil(), self.overshoot.1.ceil());
        if self.has_shadow() {
            let p = &self.p;
            let (size, x, y) = (p.drop_shadow_size * scale, p.drop_shadow_offset_x * scale, p.drop_shadow_offset_y * scale);
            left = left.max((size - x).max(0.0));
            top = top.max((size - y).max(0.0));
            right = right.max((size + x).max(0.0));
            bottom = bottom.max((size + y).max(0.0));
        }
        Thickness::new(left, top, right, bottom)
    }

    /// The gradient goes on the glyphs unless there is a background color (C# SetupBackgroundPaint).
    fn paint_background(&self, cx: &mut PaintCx) {
        if cx.base().p.background_color.is_some() {
            crate::paint::paint_background_rect(cx);
        }
    }

    fn paint(&self, cx: &mut PaintCx) {
        if self.layout.lines.is_empty() {
            return;
        }
        let scale = cx.scale;
        let p = &self.p;
        let inner = self.text_area(cx.rect, cx.base().p.padding, scale);
        let by_lines = p.gradient_by_lines;
        let fill_gradient = if cx.base().p.background_color.is_none() { &cx.base().p.fill_gradient } else { &None };
        let stroke_gradient = &p.stroke_gradient;

        // Shaders are built at the origin of the rect they fill; the canvas is moved there.
        let mut shaders = self.shaders.borrow_mut();
        let box_size = (inner.width(), inner.height());
        if shaders.serial != self.serial || shaders.fill != *fill_gradient || shaders.stroke != *stroke_gradient || (!by_lines && shaders.box_size != box_size) {
            shaders.serial = self.serial;
            shaders.fill = fill_gradient.clone();
            shaders.stroke = stroke_gradient.clone();
            shaders.box_size = box_size;
            shaders.lines.clear();
            let build = |rect: Rect| {
                let fill = fill_gradient.as_deref().and_then(|g| create_gradient(g, rect, (0.0, 0.0)));
                let stroke = stroke_gradient.as_deref().and_then(|g| create_gradient(g, rect, (0.0, 0.0)));
                (fill, stroke)
            };
            if by_lines {
                for line in &self.layout.lines {
                    shaders.lines.push(build(Rect::from_wh(line.width, self.line_height(line))));
                }
            } else {
                shaders.lines.push(build(Rect::from_wh(box_size.0, box_size.1)));
            }
        }

        let sp = scale * (p.character_spacing - 1.0);
        let mut fill = Paint::default();
        fill.set_anti_alias(true);
        let mut stroke = Paint::default();
        stroke.set_anti_alias(true);
        stroke.set_style(PaintStyle::Stroke);
        stroke.set_stroke_width(p.stroke_width * 2.0 * scale);
        stroke.set_color(p.stroke_color);
        let mut shadow = Paint::default();
        shadow.set_anti_alias(true);
        shadow.set_style(PaintStyle::Stroke);
        shadow.set_stroke_width(p.drop_shadow_size * 2.0 * scale);
        shadow.set_color(p.drop_shadow_color);
        let shadow_offset = ((p.drop_shadow_offset_x * scale).trunc(), (p.drop_shadow_offset_y * scale).trunc());
        let (has_stroke, has_shadow) = (self.has_stroke(), self.has_shadow());
        let mut deco = Paint::default();
        deco.set_anti_alias(true);
        deco.set_style(PaintStyle::Stroke);
        let mut background = Paint::default();
        background.set_anti_alias(true);
        let canvas = cx.canvas;

        for (index, (line, origin, height)) in self.placed(inner).enumerate() {
            // The shader's origin: the line, or the box.
            let (shader_origin, shader) = if by_lines {
                (origin, shaders.lines.get(index))
            } else {
                (Point::new(inner.left, inner.top), shaders.lines.first())
            };
            let (fill_shader, stroke_shader) = shader.map_or((None, None), |(f, s)| (f.clone(), s.clone()));
            fill.set_shader(fill_shader);
            stroke.set_shader(stroke_shader);
            canvas.save();
            canvas.translate(shader_origin);
            let (mut x, y) = (origin.x - shader_origin.x, origin.y - shader_origin.y);
            let baseline = y + line.ascent;
            for run in &self.layout.runs[line.runs.clone()] {
                let style = &self.layout.styles[run.style];
                let span = style.span.map(|i| &self.spans[i]);
                let width = run.width - sp;
                if let Some(color) = span.and_then(|s| s.background_color) {
                    background.set_color(color);
                    canvas.draw_rect(Rect::new(x, y, x + width, y + height), &background);
                }
                let color = span.and_then(|s| s.text_color).unwrap_or(p.text_color);
                if let Some(blob) = &run.blob {
                    // C# DrawText order: drop shadow, stroke, fill.
                    if has_shadow {
                        canvas.draw_text_blob(blob, (x + shadow_offset.0, baseline + shadow_offset.1), &shadow);
                    }
                    if has_stroke {
                        canvas.draw_text_blob(blob, (x, baseline), &stroke);
                    }
                    fill.set_color(color);
                    canvas.draw_text_blob(blob, (x, baseline), &fill);
                }
                if let Some(span) = span {
                    // DrawnUi.React: the underline 1 scaled px under the baseline, the strikeout at half an x-height.
                    if span.underline && span.underline_width != 0.0 {
                        let w = if span.underline_width > 0.0 { span.underline_width * scale } else { -span.underline_width };
                        let yl = (baseline + scale).round();
                        deco.set_color(color);
                        deco.set_stroke_width(w);
                        canvas.draw_line((x, yl), (x + width, yl), &deco);
                    }
                    if span.strikeout {
                        let yl = (baseline - style.size_px * X_HEIGHT / 2.0).round();
                        deco.set_color(span.strikeout_color);
                        deco.set_stroke_width(span.strikeout_width * scale);
                        canvas.draw_line((x, yl), (x + width, yl), &deco);
                    }
                }
                x += run.width;
            }
            canvas.restore();
        }
        if self.selection.length > 0 && cx.base().p.accessibility_text_selectable {
            self.paint_selection(canvas, inner, cx.rect, scale);
        }
    }

    /// Ctrl+C (Cmd+C) copies the selection, Ctrl+A selects all, while the label holds it.
    fn on_key(&mut self, cx: &mut GestureCx, event: &KeyEvent) -> bool {
        let m = event.modifiers();
        if event.kind != KeyKind::Down || !(m.ctrl || m.meta) || !cx.base().p.accessibility_text_selectable {
            return false;
        }
        match event.key {
            "KeyC" => self.copy_selection(cx),
            "KeyA" => {
                let count = self.selection_source().0.chars().count();
                self.select(cx, 0, count);
                true
            }
            _ => false,
        }
    }

    /// The focus moved elsewhere (a click outside): the selection goes.
    fn on_focus_changed(&mut self, cx: &mut GestureCx, focused: bool) {
        if !focused {
            self.clear_selection(cx);
        }
    }

    /// Selectable text takes the pointer (`select_text`); a tap on a span with a handler: the
    /// handler runs on the next frame with the app state.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if cx.base().p.accessibility_text_selectable && self.select_text(cx, gesture) {
            return Handled::Yes;
        }
        if gesture.kind != GestureKind::Tapped || self.spans.is_empty() {
            return Handled::No;
        }
        let Some(span) = self.span_at(cx.point) else { return Handled::No };
        self.pending_tap = Some(span);
        let (id, point, base) = (cx.id, cx.point, cx.base());
        let (rect, scale, color) = (base.rect, base.scale, base.p.touch_effect_color);
        let (x, y) = ((point.x - rect.left) / scale, (point.y - rect.top) / scale);
        cx.cx().play_ripple(id, color, x, y, 0.0);
        animators::start_frame(cx.tree, id, fire_span_tap);
        Handled::Yes
    }
}
