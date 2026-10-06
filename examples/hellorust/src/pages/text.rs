//! SkiaLabel text engine: wrapping, MaxLines, alignment, spacing, weights, transforms, spans,
//! markdown, stroke / shadow and glyph fallback. Ported from the React demo's TextPage.tsx.

use drawnui::prelude::*;

use super::{column, scrolling};
use crate::{App, hex};

const MARKDOWN: &str = "# Heading 1
## Heading 2
### Heading 3
A paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code` and a [tappable link](https://drawnui.net).
Soft line breaks stay inside the paragraph.

- Bullet item with **bold**
- Second bullet
1. Numbered item
2. Another one, *emphasized*

```
let label = SkiaRichLabel::new(\"# Hello\");
```";

const LOREM: &str = "DrawnUI draws every pixel itself: text is shaped and rasterized by Skia, so a label wraps by words, respects MaxLines with an ellipsis, aligns horizontally and vertically, and never leaves the canvas for a native view. This paragraph is long on purpose so it wraps across several lines at whatever width the layout gives it.";

const BODY: Color = hex(0xDEE2E6);
const MUTED: Color = hex(0xADB5BD);

/// What the page keeps.
pub struct State {
    /// When the link span was tapped last.
    tapped: String,
    /// The url of the markdown link tapped last.
    link: String,
}

impl Default for State {
    fn default() -> Self {
        Self { tapped: "nothing yet".to_owned(), link: "none".to_owned() }
    }
}

/// Builds the page.
pub fn build(_app: &mut App) -> Build<SkiaLayout> {
    // Static text: every card is one bitmap, blitted while the page scrolls.
    scrolling(column().children((
        SkiaLabel::new("SkiaLabel").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        card("Word wrap · HorizontalOptions=Fill", SkiaLabel::new(LOREM).font_size(15).text_color(BODY).fill_x()),
        card(
            "Spans — <TextSpan> children: color, size, bold, italic, underline, strikeout, background, Tapped",
            (
                SkiaLabel::new("").font_size(16).text_color(BODY).fill_x().font_family_fallback("FontSymbols").spans((
                    TextSpan::new("One label, many styles: "),
                    TextSpan::new("bold").is_bold(true),
                    TextSpan::new(", "),
                    TextSpan::new("italic").is_italic(true),
                    TextSpan::new(", "),
                    TextSpan::new("colored").text_color(hex(0xFFC107)),
                    TextSpan::new(", "),
                    TextSpan::new("bigger").font_size(22).text_color(hex(0x20C997)),
                    TextSpan::new(", "),
                    TextSpan::new("underlined").underline(true),
                    TextSpan::new(", "),
                    TextSpan::new("struck out").strikeout(true),
                    (
                        TextSpan::new(", "),
                        TextSpan::new(" highlighted ").background_color(hex(0x6610F2)).text_color(Color::WHITE),
                        TextSpan::new(" and a "),
                        TextSpan::new("tappable link →").text_color(hex(0x6EA8FE)).underline(true).on_tapped(|_me, app: &mut App, _cx| {
                            app.text.tapped = format!("link tapped at {}", clock());
                        }),
                        TextSpan::new(" that wraps with the rest of the paragraph like any other word."),
                    ),
                )),
                SkiaLabel::new("").font_size(13).text_color(MUTED).observe(|me, app: &App| me.set_text(format!("Last span tap: {}", app.text.tapped))),
            ),
        ),
        card(
            "SkiaRichLabel — markdown in Text, rendered as spans",
            (
                SkiaRichLabel::new(MARKDOWN)
                    .font_size(15)
                    .text_color(BODY)
                    .fill_x()
                    .font_family_fallback("FontSymbols,FontSymbols2")
                    .on_link_tapped(|_me, app: &mut App, _cx, url| app.text.link = url.to_owned()),
                SkiaLabel::new("").font_size(13).text_color(MUTED).observe(|me, app: &App| me.set_text(format!("Last link tapped: {}", app.text.link))),
            ),
        ),
        card("MaxLines={2} · TailTruncation (default)", SkiaLabel::new(LOREM).font_size(15).text_color(BODY).fill_x().max_lines(2)),
        card("LineSpacing={1.6}", SkiaLabel::new(LOREM).font_size(14).text_color(BODY).fill_x().line_spacing(1.6).max_lines(3)),
        card(
            "HorizontalTextAlignment Start / Center / End",
            SkiaWrap::new().spacing(12).children((
                aligned("Start aligned text wraps inside its own column", TextAlignment::Start),
                aligned("Center aligned text wraps inside its own column", TextAlignment::Center),
                aligned("End aligned text wraps inside its own column", TextAlignment::End),
            )),
        ),
        card(
            "VerticalTextAlignment in a 90pt box",
            SkiaWrap::new().spacing(12).children((
                boxed("Start", TextAlignment::Start),
                boxed("Center", TextAlignment::Center),
                boxed("End", TextAlignment::End),
            )),
        ),
        card(
            "StrokeColor / StrokeWidth, StrokeGradient, DropShadow* — outline under the fill, shadow below (C# DrawText order)",
            SkiaWrap::new().spacing(16).children((
                big("Outlined").text_color(hex(0x212529)).stroke_color(hex(0xFFC107)).stroke_width(1.5),
                big("Gradient stroke")
                    .text_color(hex(0x212529))
                    .stroke_color(Color::WHITE)
                    .stroke_width(2)
                    .stroke_gradient(SkiaGradient::new(GradientType::Linear, [hex(0x0DCAF0), hex(0xD63384)]).angle(0.0)),
                big("Drop shadow")
                    .text_color(BODY)
                    .drop_shadow_color(Color::BLACK)
                    .drop_shadow_size(2)
                    .drop_shadow_offset_x(3)
                    .drop_shadow_offset_y(3),
                big("Both + gradient fill")
                    .text_color(Color::WHITE)
                    .fill_gradient(SkiaGradient::new(GradientType::Linear, [hex(0xFFC107), hex(0xFD7E14)]).angle(90.0))
                    .stroke_color(hex(0x3D2B00))
                    .stroke_width(1)
                    .drop_shadow_color(Color::new(0x66000000))
                    .drop_shadow_size(3)
                    .drop_shadow_offset_x(2)
                    .drop_shadow_offset_y(4),
            )),
        ),
        card(
            "FontFamilyFallback — symbols and emoji the text font lacks",
            (
                SkiaLabel::new("Arrows ← ↑ → ↓ ⇒ ⇔  math ∑ ∞ ≈ ≠ ≤ ≥ √  misc ♥ ★ ✓ ✗ ⚠ via FontFamilyFallback=\"FontSymbols,FontSymbols2\"")
                    .font_size(16)
                    .text_color(BODY)
                    .font_family_fallback("FontSymbols,FontSymbols2")
                    .fill_x(),
                SkiaLabel::new("Emoji 😀 😎 🤖 😂 👍 🙌 via FontFamilyFallback=\"FontEmoji\" (Noto Color Emoji faces + hands subset)")
                    .font_size(16)
                    .text_color(BODY)
                    .font_family_fallback("FontEmoji")
                    .fill_x(),
                SkiaLabel::new("Without a fallback the same arrow → and emoji 😀 render as tofu").font_size(16).text_color(MUTED).fill_x(),
            ),
        ),
        card(
            "FontAttributes / FontWeight (weights registered via ConfigureFonts)",
            (
                SkiaLabel::new("Regular 400 — the family default").font_size(16).text_color(BODY),
                SkiaLabel::new("FontAttributes=Bold → nearest registered weight (600 Semibold)")
                    .font_size(16)
                    .text_color(BODY)
                    .font_attributes(FontAttributes::Bold)
                    .font_family_fallback("FontSymbols"),
                SkiaLabel::new("FontAttributes=Italic → synthetic skew when no italic face")
                    .font_size(16)
                    .text_color(BODY)
                    .font_attributes(FontAttributes::Italic)
                    .font_family_fallback("FontSymbols"),
                SkiaLabel::new("FontAttributes=BoldItalic").font_size(16).text_color(BODY).font_attributes(FontAttributes::BoldItalic),
                SkiaLabel::new("FontWeight={600} explicit").font_size(16).text_color(BODY).font_weight(600),
            ),
        ),
        // A tuple of children holds twelve at most.
        (
            card(
                "TextTransform · NoWrap · Padding",
                (
                    SkiaLabel::new("uppercase transform applied at layout time").font_size(14).text_color(BODY).text_transform(TextTransform::Uppercase),
                    SkiaLabel::new("Titlecase transform applied at layout time").font_size(14).text_color(BODY).text_transform(TextTransform::Titlecase),
                    SkiaLabel::new("LineBreakMode=NoWrap keeps this on one line even when it is far too long for the card width, so it simply runs past the edge")
                        .font_size(14)
                        .text_color(BODY)
                        .line_break_mode(LineBreakMode::NoWrap)
                        .fill_x(),
                    SkiaLabel::new("Padding={new Thickness(12, 6)} + background")
                        .font_size(14)
                        .text_color(Color::WHITE)
                        .background_color(hex(0x0D6EFD))
                        .padding((12, 6)),
                ),
            ),
            card(
                "Multiline text with explicit line breaks",
                SkiaLabel::new("Line one\nLine two is a bit longer\nLine three")
                    .font_size(14)
                    .text_color(BODY)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .fill_x(),
            ),
        ),
    )))
}

/// A titled card, the React page's Card component: one bitmap each.
fn card(title: &str, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new().corner_radius(8).background_color(hex(0x2B3035)).fill_x().use_cache(CacheType::Image).children(
        SkiaStack::new().spacing(8).padding((16, 12)).children((
            SkiaLabel::new(title)
                .font_size(12)
                .text_color(hex(0x6EA8FE))
                .font_attributes(FontAttributes::Bold)
                .text_transform(TextTransform::Uppercase),
            content,
        )),
    )
}

fn aligned(text: &str, alignment: TextAlignment) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(13)
        .text_color(BODY)
        .width_request(215)
        .horizontal_text_alignment(alignment)
        .background_color(Color::new(0x22FFFFFF))
        .padding(6)
}

fn boxed(text: &str, vertical: TextAlignment) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(14)
        .text_color(BODY)
        .width_request(215)
        .height_request(90)
        .vertical_text_alignment(vertical)
        .horizontal_text_alignment(TextAlignment::Center)
        .background_color(Color::new(0x22FFFFFF))
}

/// The 32 pt bold text of the stroke / shadow card.
fn big(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(32).font_family("FontTextBold")
}

/// The time of day, UTC, as HH:MM:SS.
fn clock() -> String {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}:{:02} UTC", seconds / 3600 % 24, seconds / 60 % 60, seconds % 60)
}
