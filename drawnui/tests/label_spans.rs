//! SkiaLabel spans, weights, per-glyph font fallback, gradient / stroke / drop shadow on the
//! glyphs, span taps, and SkiaRichLabel layout. Lines, runs, the font of every run, widths and
//! sizes are those of the DrawnUi.React engine: a node probe over its `dist` measured the same
//! labels with the same font files (`SkiaLabel.MeasureAbsolute`, faces registered as its demo
//! does). The probe turned FreeType hinting off: CanvasKit in the browser hints advances to whole
//! pixels, DirectWrite (desktop here, and C# on Windows) does not; with hinting off both engines
//! agree to the third decimal.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::PointerKind;
use drawnui::controls::label::LabelRun;
use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/");
const REGULAR: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));
const SEMIBOLD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Semibold.ttf"));
const SYMBOLS: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoSansMathSymbols-Subset.ttf"));
const SYMBOLS2: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoSansSymbols2-Subset.ttf"));

/// The emoji subset of the HelloRust demo.
fn emoji_font() -> Option<Vec<u8>> {
    std::fs::read(format!("{ASSETS}NotoColorEmoji-Subset.ttf")).ok()
}

/// The spans of the React TextPage.
fn page_spans() -> Vec<TextSpan> {
    vec![
        TextSpan::new("One label, many styles: "),
        TextSpan::new("bold").is_bold(true),
        TextSpan::new(", "),
        TextSpan::new("italic").is_italic(true),
        TextSpan::new(", "),
        TextSpan::new("colored").text_color(Color::from_rgb(0xFF, 0xC1, 0x07)),
        TextSpan::new(", "),
        TextSpan::new("bigger").font_size(22).text_color(Color::from_rgb(0x20, 0xC9, 0x97)),
        TextSpan::new(", "),
        TextSpan::new("underlined").underline(true),
        TextSpan::new(", "),
        TextSpan::new("struck out").strikeout(true),
        TextSpan::new(", "),
        TextSpan::new(" highlighted ").background_color(Color::from_rgb(0x66, 0x10, 0xF2)).text_color(Color::WHITE),
        TextSpan::new(" and a "),
        TextSpan::new("tappable link \u{2192}").text_color(Color::from_rgb(0x6E, 0xA8, 0xFE)).underline(true),
        TextSpan::new(" that wraps with the rest of the paragraph like any other word."),
    ]
}

const SYMBOLS_TEXT: &str = "Arrows \u{2190} \u{2191} \u{2192} \u{2193} \u{21D2} \u{21D4}  math \u{2211} \u{221E} \u{2248} \u{2260} \u{2264} \u{2265} \u{221A}  misc \u{2665} \u{2605} \u{2713} \u{2717} \u{26A0} via FontFamilyFallback=\"FontSymbols,FontSymbols2\"";
const EMOJI_TEXT: &str = "Emoji \u{1F600} \u{1F60E} \u{1F916} \u{1F602} \u{1F44D} \u{1F64C} via FontFamilyFallback=\"FontEmoji\" (Noto Color Emoji faces + hands subset)";
const LOREM: &str = "DrawnUI draws every pixel itself: text is shaped and rasterized by Skia, so a label wraps by words, respects MaxLines with an ellipsis, aligns horizontally and vertically, and never leaves the canvas for a native view. This paragraph is long on purpose so it wraps across several lines at whatever width the layout gives it.";
const MARKDOWN: &str = "# Heading 1\n## Heading 2\n### Heading 3\nA paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code` and a [tappable link](https://drawnui.net).\nSoft line breaks stay inside the paragraph.\n\n- Bullet item with **bold**\n- Second bullet\n1. Numbered item\n2. Another one, *emphasised*\n\n```\nconst label = new SkiaRichLabel();\nlabel.Text = \"# Hello\";\n```";
const WEIGHT_TEXT: &str = "Regular 400 the family default";

/// (name, content size in pixels, lines of runs: text, width, font (0 its own, k the k-th
/// fallback alias), span (-1 none)), from the React probe.
type Case = (&'static str, (f32, f32), &'static [&'static [(&'static str, f32, usize, i32)]]);
const REACT: &[Case] = &[
    ("spans_656_1", (655.0, 52.0), &[&[("One label, many styles:", 171.344, 0, 0), (" bold", 38.328, 0, 1), (",", 3.922, 0, 2), (" italic", 38.461, 0, 3), (",", 3.922, 0, 4), (" colored", 60.461, 0, 5), (",", 3.922, 0, 6), (" bigger", 70.189, 0, 7), (",", 3.922, 0, 8), (" underlined", 85.805, 0, 9), (",", 3.922, 0, 10), (" struck out", 79.094, 0, 11), (",", 3.922, 0, 12), (" highlighted", 87.719, 0, 13)], &[("and a", 41.578, 0, 14), (" tappable link ", 104.664, 0, 15), ("\u{2192}", 16.192, 1, 15), (" that wraps with the rest of the paragraph like any other word.", 463.477, 0, 16)]]),
    ("symbols_656_1", (453.0, 44.0), &[&[("Arrows ", 57.086, 0, -1), ("\u{2190}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{2191}", 8.48, 1, -1), (" ", 4.156, 0, -1), ("\u{2192}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{2193}", 8.48, 1, -1), (" ", 4.156, 0, -1), ("\u{21D2}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{21D4}", 20.192, 1, -1), (" math \u{2211} \u{221E} \u{2248} \u{2260} \u{2264} \u{2265} \u{221A} misc ", 181.742, 0, -1), ("\u{2665}", 11.84, 2, -1), (" ", 4.156, 0, -1), ("\u{2605}", 15.36, 2, -1), (" ", 4.156, 0, -1), ("\u{2713}", 13.264, 2, -1), (" ", 4.156, 0, -1), ("\u{2717}", 10.624, 2, -1), (" ", 4.156, 0, -1), ("\u{26A0}", 14.816, 2, -1), (" via", 25.117, 0, -1)], &[("FontFamilyFallback=\"FontSymbols,FontSymbols2\"", 369.133, 0, -1)]]),
    ("emoji_656_1", (646.0, 44.0), &[&[("Emoji ", 45.695, 0, -1), ("\u{1F600}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F60E}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F916}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F602}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F44D}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F64C}", 19.922, 1, -1), (" via FontFamilyFallback=\"FontEmoji\" (Noto Color Emoji faces +", 459.523, 0, -1)], &[("hands subset)", 104.383, 0, -1)]]),
    ("rich_656_1", (524.0, 246.0), &[&[("Heading 1", 115.934, 0, 0)], &[("Heading 2", 91.781, 0, 1)], &[("Heading 3", 82.12, 0, 2)], &[("A paragraph with", 120.33, 0, 3), (" bold", 35.933, 0, 4), (",", 3.677, 0, 5), (" italic", 36.057, 0, 6), (",", 3.677, 0, 7), (" strikethrough", 98.87, 0, 8), (",", 3.677, 0, 9), (" inline code", 79.812, 0, 10), (" and a", 42.876, 0, 11), (" tappable link", 94.226, 0, 12), (".", 3.992, 0, 13)], &[("Soft line breaks stay inside the paragraph.", 293.262, 0, 13)], &[("\u{2022}", 5.64, 0, 14), (" Bullet item with", 113.335, 0, 15), (" bold", 35.933, 0, 16)], &[("\u{2022}", 5.64, 0, 17), (" Second bullet", 98.738, 0, 18)], &[("1.", 12.568, 0, 19), (" Numbered item", 115.056, 0, 20)], &[("2.", 12.568, 0, 21), (" Another one,", 94.951, 0, 22), (" emphasised", 88.718, 0, 23)], &[("const label = new SkiaRichLabel();", 232.903, 0, 24)], &[("label.Text = \"# Hello\";", 149.502, 0, 25)]]),
    ("lorem2_656_1", (654.0, 41.0), &[&[("DrawnUI draws every pixel itself: text is shaped and rasterized by Skia, so a label wraps by", 626.741, 0, -1)], &[("words, respects MaxLines with an ellipsis, aligns horizontally and vertically, and never leaves\u{2026}", 653.679, 0, -1)]]),
    ("spans_656_2", (1310.0, 104.0), &[&[("One label, many styles:", 342.688, 0, 0), (" bold", 76.656, 0, 1), (",", 7.844, 0, 2), (" italic", 76.922, 0, 3), (",", 7.844, 0, 4), (" colored", 120.922, 0, 5), (",", 7.844, 0, 6), (" bigger", 140.379, 0, 7), (",", 7.844, 0, 8), (" underlined", 171.609, 0, 9), (",", 7.844, 0, 10), (" struck out", 158.188, 0, 11), (",", 7.844, 0, 12), (" highlighted", 175.438, 0, 13)], &[("and a", 83.156, 0, 14), (" tappable link ", 209.328, 0, 15), ("\u{2192}", 32.384, 1, 15), (" that wraps with the rest of the paragraph like any other word.", 926.953, 0, 16)]]),
    ("symbols_656_2", (906.0, 88.0), &[&[("Arrows ", 114.172, 0, -1), ("\u{2190}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{2191}", 16.96, 1, -1), (" ", 8.313, 0, -1), ("\u{2192}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{2193}", 16.96, 1, -1), (" ", 8.313, 0, -1), ("\u{21D2}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{21D4}", 40.384, 1, -1), (" math \u{2211} \u{221E} \u{2248} \u{2260} \u{2264} \u{2265} \u{221A} misc ", 363.484, 0, -1), ("\u{2665}", 23.68, 2, -1), (" ", 8.313, 0, -1), ("\u{2605}", 30.72, 2, -1), (" ", 8.313, 0, -1), ("\u{2713}", 26.528, 2, -1), (" ", 8.313, 0, -1), ("\u{2717}", 21.248, 2, -1), (" ", 8.313, 0, -1), ("\u{26A0}", 29.632, 2, -1), (" via", 50.234, 0, -1)], &[("FontFamilyFallback=\"FontSymbols,FontSymbols2\"", 738.266, 0, -1)]]),
    ("emoji_656_2", (1292.0, 88.0), &[&[("Emoji ", 91.391, 0, -1), ("\u{1F600}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F60E}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F916}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F602}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F44D}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F64C}", 39.844, 1, -1), (" via FontFamilyFallback=\"FontEmoji\" (Noto Color Emoji faces +", 919.047, 0, -1)], &[("hands subset)", 208.766, 0, -1)]]),
    ("rich_656_2", (1047.0, 491.0), &[&[("Heading 1", 231.867, 0, 0)], &[("Heading 2", 183.562, 0, 1)], &[("Heading 3", 164.239, 0, 2)], &[("A paragraph with", 240.659, 0, 3), (" bold", 71.865, 0, 4), (",", 7.354, 0, 5), (" italic", 72.114, 0, 6), (",", 7.354, 0, 7), (" strikethrough", 197.739, 0, 8), (",", 7.354, 0, 9), (" inline code", 159.624, 0, 10), (" and a", 85.752, 0, 11), (" tappable link", 188.452, 0, 12), (".", 7.983, 0, 13)], &[("Soft line breaks stay inside the paragraph.", 586.523, 0, 13)], &[("\u{2022}", 11.279, 0, 14), (" Bullet item with", 226.67, 0, 15), (" bold", 71.865, 0, 16)], &[("\u{2022}", 11.279, 0, 17), (" Second bullet", 197.476, 0, 18)], &[("1.", 25.137, 0, 19), (" Numbered item", 230.112, 0, 20)], &[("2.", 25.137, 0, 21), (" Another one,", 189.902, 0, 22), (" emphasised", 177.437, 0, 23)], &[("const label = new SkiaRichLabel();", 465.806, 0, 24)], &[("label.Text = \"# Hello\";", 299.004, 0, 25)]]),
    ("lorem2_656_2", (1308.0, 82.0), &[&[("DrawnUI draws every pixel itself: text is shaped and rasterized by Skia, so a label wraps by", 1253.481, 0, -1)], &[("words, respects MaxLines with an ellipsis, aligns horizontally and vertically, and never leaves\u{2026}", 1307.358, 0, -1)]]),
    ("spans_300_1", (284.0, 118.0), &[&[("One label, many styles:", 171.344, 0, 0), (" bold", 38.328, 0, 1), (",", 3.922, 0, 2), (" italic", 38.461, 0, 3), (",", 3.922, 0, 4)], &[("colored", 56.305, 0, 5), (",", 3.922, 0, 6), (" bigger", 70.189, 0, 7), (",", 3.922, 0, 8), (" underlined", 85.805, 0, 9), (",", 3.922, 0, 10), (" struck", 49.805, 0, 11)], &[("out", 25.133, 0, 11), (",", 3.922, 0, 12), (" highlighted", 87.719, 0, 13), (" and a", 45.734, 0, 14), (" tappable link ", 104.664, 0, 15), ("\u{2192}", 16.192, 1, 15)], &[("that wraps with the rest of the", 224.992, 0, 16)], &[("paragraph like any other word.", 230.172, 0, 16)]]),
    ("symbols_300_1", (300.0, 88.0), &[&[("Arrows ", 57.086, 0, -1), ("\u{2190}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{2191}", 8.48, 1, -1), (" ", 4.156, 0, -1), ("\u{2192}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{2193}", 8.48, 1, -1), (" ", 4.156, 0, -1), ("\u{21D2}", 16.192, 1, -1), (" ", 4.156, 0, -1), ("\u{21D4}", 20.192, 1, -1), (" math \u{2211} \u{221E} \u{2248} \u{2260} \u{2264} \u{2265}", 126.313, 0, -1)], &[("\u{221A} misc ", 51.273, 0, -1), ("\u{2665}", 11.84, 2, -1), (" ", 4.156, 0, -1), ("\u{2605}", 15.36, 2, -1), (" ", 4.156, 0, -1), ("\u{2713}", 13.264, 2, -1), (" ", 4.156, 0, -1), ("\u{2717}", 10.624, 2, -1), (" ", 4.156, 0, -1), ("\u{26A0}", 14.816, 2, -1), (" via", 25.117, 0, -1)], &[("FontFamilyFallback=\"FontSymbols,FontS", 299.477, 0, -1)], &[("ymbols2\"", 69.656, 0, -1)]]),
    ("emoji_300_1", (285.0, 66.0), &[&[("Emoji ", 45.695, 0, -1), ("\u{1F600}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F60E}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F916}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F602}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F44D}", 19.922, 1, -1), (" ", 4.156, 0, -1), ("\u{1F64C}", 19.922, 1, -1), (" via", 25.117, 0, -1)], &[("FontFamilyFallback=\"FontEmoji\" (Noto", 284.391, 0, -1)], &[("Color Emoji faces + hands subset)", 250.242, 0, -1)]]),
    ("rich_300_1", (294.0, 286.0), &[&[("Heading 1", 115.934, 0, 0)], &[("Heading 2", 91.781, 0, 1)], &[("Heading 3", 82.12, 0, 2)], &[("A paragraph with", 120.33, 0, 3), (" bold", 35.933, 0, 4), (",", 3.677, 0, 5), (" italic", 36.057, 0, 6), (",", 3.677, 0, 7)], &[("strikethrough", 94.973, 0, 8), (",", 3.677, 0, 9), (" inline code", 79.812, 0, 10), (" and a", 42.876, 0, 11), (" tappable", 65.662, 0, 12)], &[("link", 24.668, 0, 12), (".", 3.992, 0, 13)], &[("Soft line breaks stay inside the paragraph.", 293.262, 0, 13)], &[("\u{2022}", 5.64, 0, 14), (" Bullet item with", 113.335, 0, 15), (" bold", 35.933, 0, 16)], &[("\u{2022}", 5.64, 0, 17), (" Second bullet", 98.738, 0, 18)], &[("1.", 12.568, 0, 19), (" Numbered item", 115.056, 0, 20)], &[("2.", 12.568, 0, 21), (" Another one,", 94.951, 0, 22), (" emphasised", 88.718, 0, 23)], &[("const label = new SkiaRichLabel();", 232.903, 0, 24)], &[("label.Text = \"# Hello\";", 149.502, 0, 25)]]),
    ("lorem2_300_1", (294.0, 41.0), &[&[("DrawnUI draws every pixel itself: text is", 273.662, 0, -1)], &[("shaped and rasterized by Skia, so a label\u{2026}", 293.921, 0, -1)]]),
    ("spans_300_2", (567.0, 235.0), &[&[("One label, many styles:", 342.688, 0, 0), (" bold", 76.656, 0, 1), (",", 7.844, 0, 2), (" italic", 76.922, 0, 3), (",", 7.844, 0, 4)], &[("colored", 112.609, 0, 5), (",", 7.844, 0, 6), (" bigger", 140.379, 0, 7), (",", 7.844, 0, 8), (" underlined", 171.609, 0, 9), (",", 7.844, 0, 10), (" struck", 99.609, 0, 11)], &[("out", 50.266, 0, 11), (",", 7.844, 0, 12), (" highlighted", 175.438, 0, 13), (" and a", 91.469, 0, 14), (" tappable link ", 209.328, 0, 15), ("\u{2192}", 32.384, 1, 15)], &[("that wraps with the rest of the", 449.984, 0, 16)], &[("paragraph like any other word.", 460.344, 0, 16)]]),
    ("symbols_300_2", (599.0, 175.0), &[&[("Arrows ", 114.172, 0, -1), ("\u{2190}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{2191}", 16.96, 1, -1), (" ", 8.313, 0, -1), ("\u{2192}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{2193}", 16.96, 1, -1), (" ", 8.313, 0, -1), ("\u{21D2}", 32.384, 1, -1), (" ", 8.313, 0, -1), ("\u{21D4}", 40.384, 1, -1), (" math \u{2211} \u{221E} \u{2248} \u{2260} \u{2264} \u{2265}", 252.625, 0, -1)], &[("\u{221A} misc ", 102.547, 0, -1), ("\u{2665}", 23.68, 2, -1), (" ", 8.313, 0, -1), ("\u{2605}", 30.72, 2, -1), (" ", 8.313, 0, -1), ("\u{2713}", 26.528, 2, -1), (" ", 8.313, 0, -1), ("\u{2717}", 21.248, 2, -1), (" ", 8.313, 0, -1), ("\u{26A0}", 29.632, 2, -1), (" via", 50.234, 0, -1)], &[("FontFamilyFallback=\"FontSymbols,FontS", 598.953, 0, -1)], &[("ymbols2\"", 139.313, 0, -1)]]),
    ("emoji_300_2", (569.0, 131.0), &[&[("Emoji ", 91.391, 0, -1), ("\u{1F600}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F60E}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F916}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F602}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F44D}", 39.844, 1, -1), (" ", 8.313, 0, -1), ("\u{1F64C}", 39.844, 1, -1), (" via", 50.234, 0, -1)], &[("FontFamilyFallback=\"FontEmoji\" (Noto", 568.781, 0, -1)], &[("Color Emoji faces + hands subset)", 500.484, 0, -1)]]),
    ("rich_300_2", (587.0, 572.0), &[&[("Heading 1", 231.867, 0, 0)], &[("Heading 2", 183.562, 0, 1)], &[("Heading 3", 164.239, 0, 2)], &[("A paragraph with", 240.659, 0, 3), (" bold", 71.865, 0, 4), (",", 7.354, 0, 5), (" italic", 72.114, 0, 6), (",", 7.354, 0, 7)], &[("strikethrough", 189.946, 0, 8), (",", 7.354, 0, 9), (" inline code", 159.624, 0, 10), (" and a", 85.752, 0, 11), (" tappable", 131.323, 0, 12)], &[("link", 49.336, 0, 12), (".", 7.983, 0, 13)], &[("Soft line breaks stay inside the paragraph.", 586.523, 0, 13)], &[("\u{2022}", 11.279, 0, 14), (" Bullet item with", 226.67, 0, 15), (" bold", 71.865, 0, 16)], &[("\u{2022}", 11.279, 0, 17), (" Second bullet", 197.476, 0, 18)], &[("1.", 25.137, 0, 19), (" Numbered item", 230.112, 0, 20)], &[("2.", 25.137, 0, 21), (" Another one,", 189.902, 0, 22), (" emphasised", 177.437, 0, 23)], &[("const label = new SkiaRichLabel();", 465.806, 0, 24)], &[("label.Text = \"# Hello\";", 299.004, 0, 25)]]),
    ("lorem2_300_2", (588.0, 82.0), &[&[("DrawnUI draws every pixel itself: text is", 547.324, 0, -1)], &[("shaped and rasterized by Skia, so a label\u{2026}", 587.842, 0, -1)]]),
    ("weight_regular", (224.0, 22.0), &[&[("Regular 400 the family default", 223.422, 0, -1)]]),
    ("weight_bold", (232.0, 22.0), &[&[("Regular 400 the family default", 231.891, 0, -1)]]),
    ("weight_italic", (224.0, 22.0), &[&[("Regular 400 the family default", 223.422, 0, -1)]]),
    ("weight_bolditalic", (232.0, 22.0), &[&[("Regular 400 the family default", 231.891, 0, -1)]]),
    ("weight_w600", (232.0, 22.0), &[&[("Regular 400 the family default", 231.891, 0, -1)]]),
    ("weight_w800", (232.0, 22.0), &[&[("Regular 400 the family default", 231.891, 0, -1)]]),
    ("weight_boldtextbold", (232.0, 22.0), &[&[("Regular 400 the family default", 231.891, 0, -1)]]),
    ("titlecase", (283.0, 20.0), &[&[("Titlecase Transform Applied At Layout Time", 282.844, 0, -1)]]),
    ("multiline", (147.0, 58.0), &[&[("Line one", 55.795, 0, -1)], &[("Line two is a bit longer", 146.624, 0, -1)], &[("Line three", 65.851, 0, -1)]]),
];

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
    rich: Handle<SkiaRichLabel>,
    tapped: Vec<String>,
}

type Host = Headless<App>;

/// Faces as the React demo registers them: FontText regular and 600, FontTextBold, the symbols
/// and the emoji.
fn host_of(build: impl FnOnce(&mut App) -> Build<SkiaLayout>, width: f32, height: f32, scale: f32) -> Host {
    let mut ui = Ui::new(App::default(), build).font_bytes("FontText", REGULAR).background(Color::BLACK);
    ui.fonts.add_weight("FontText", 600, SEMIBOLD);
    ui.fonts.add("FontTextBold", SEMIBOLD);
    ui.fonts.add("FontSymbols", SYMBOLS);
    ui.fonts.add("FontSymbols2", SYMBOLS2);
    if let Some(emoji) = emoji_font() {
        ui.fonts.add("FontEmoji", &emoji);
    }
    let mut host = Headless::new(ui, (width * scale) as i32, (height * scale) as i32, scale);
    host.settle();
    host
}

/// A label `width` points wide at the top left.
fn host(label: Build<SkiaLabel>, width: f32, scale: f32) -> Host {
    host_of(|app| SkiaLayout::new().fill().children(label.width_request(width).assign(&mut app.label)), width + 8.0, 300.0, scale)
}

fn label(host: &Host) -> &SkiaLabel {
    host.ui.tree.find(host.ui.state.label).unwrap()
}

/// The label a case of the React probe measured.
fn case_label(name: &str) -> Option<(Build<SkiaLabel>, f32, f32)> {
    let mut parts = name.split('_');
    let (kind, a, b) = (parts.next()?, parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (width, scale) = (a.parse().unwrap_or(1000.0), b.parse().unwrap_or(1.0));
    let label = match kind {
        "spans" => SkiaLabel::new("").font_size(16).font_family_fallback("FontSymbols").spans(page_spans()),
        "symbols" => SkiaLabel::new(SYMBOLS_TEXT).font_size(16).font_family_fallback("FontSymbols,FontSymbols2"),
        "emoji" => SkiaLabel::new(EMOJI_TEXT).font_size(16).font_family_fallback("FontEmoji"),
        "lorem2" => SkiaLabel::new(LOREM).font_size(15).max_lines(2),
        "titlecase" => SkiaLabel::new("Titlecase transform applied at layout time").font_size(14).text_transform(TextTransform::Titlecase),
        "multiline" => return Some((SkiaLabel::new("Line one\nLine two is a bit longer\nLine three").font_size(14), 400.0, 1.0)),
        "weight" => {
            let label = SkiaLabel::new(WEIGHT_TEXT).font_size(16);
            let label = match a {
                "regular" => label,
                "bold" => label.font_attributes(FontAttributes::Bold),
                "italic" => label.font_attributes(FontAttributes::Italic),
                "bolditalic" => label.font_attributes(FontAttributes::BoldItalic),
                "w600" => label.font_weight(600),
                "w800" => label.font_weight(800),
                "boldtextbold" => label.font_attributes(FontAttributes::Bold).font_family("FontTextBold"),
                _ => return None,
            };
            return Some((label, 1000.0, 1.0));
        }
        _ => return None,
    };
    Some((label, width, scale))
}

/// Compares the runs of a laid-out label with a case of the React probe.
fn assert_case(case: &Case, l: &SkiaLabel) {
    let (name, size, lines) = *case;
    let runs: Vec<LabelRun> = l.runs().collect();
    let expected: Vec<(usize, &(&str, f32, usize, i32))> = lines.iter().enumerate().flat_map(|(i, line)| line.iter().map(move |r| (i, r))).collect();
    let show = || runs.iter().map(|r| format!("{} {:?} {} {} {:?}", r.line, r.text, r.width, r.fallback, r.span)).collect::<Vec<_>>().join("\n");
    assert_eq!(runs.len(), expected.len(), "{name}: runs\n{}", show());
    for (run, (line, (text, width, font, span))) in runs.iter().zip(expected) {
        assert_eq!((run.line, run.text, run.fallback), (line, *text, *font), "{name}\n{}", show());
        assert_eq!(run.span.map_or(-1, |s| s as i32), *span, "{name}: span of {text:?}");
        assert!((run.width - width).abs() < 0.002, "{name}: width of {text:?}: {} != {width}", run.width);
    }
    let content = l.content_size();
    assert_eq!((content.width, content.height), size, "{name}: size");
}

#[test]
fn labels_lay_out_as_the_react_engine() {
    let emoji = emoji_font().is_some();
    let mut checked = 0;
    for case in REACT.iter().filter(|c| !c.0.starts_with("rich")) {
        if case.0.starts_with("emoji") && !emoji {
            continue;
        }
        let (build, width, scale) = case_label(case.0).unwrap_or_else(|| panic!("no label for {}", case.0));
        let host = host(build, width, scale);
        assert_case(case, label(&host));
        checked += 1;
    }
    assert!(checked >= 20, "{checked} cases");
}

#[test]
fn rich_labels_lay_out_as_the_react_engine() {
    for case in REACT.iter().filter(|c| c.0.starts_with("rich")) {
        let mut parts = case.0.split('_').skip(1);
        let width: f32 = parts.next().unwrap().parse().unwrap();
        let scale: f32 = parts.next().unwrap().parse().unwrap();
        let rich = SkiaRichLabel::new(MARKDOWN).font_size(15).font_family_fallback("FontSymbols,FontSymbols2");
        let host = host_of(|app| SkiaLayout::new().fill().children(rich.width_request(width).assign(&mut app.rich)), width + 8.0, 300.0, scale);
        let rich: &SkiaRichLabel = host.ui.tree.find(host.ui.state.rich).unwrap();
        assert_case(case, rich.label());
    }
}

/// Ink pixels (not the black background) inside a rect of the frame.
fn ink(host: &mut Host, rect: Rect, of: impl Fn(Color) -> bool) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in rect.top as i32..rect.bottom as i32 {
        for x in rect.left as i32..rect.right as i32 {
            if of(host.pixel(x, y)) {
                out.push((x, y));
            }
        }
    }
    out
}

fn not_black(c: Color) -> bool {
    c != Color::BLACK
}

#[test]
fn bold_without_a_bolder_face_is_emboldened() {
    let draw = |label: Build<SkiaLabel>| {
        let mut host = host(label.font_size(30).text_color(Color::WHITE), 400.0, 1.0);
        ink(&mut host, Rect::from_wh(400.0, 60.0), not_black)
    };
    // FontText has a 600 face: Bold picks it as it is; FontTextBold has one face only (the same
    // file at 400): Bold emboldens it (DrawnUi.React GetFont, C# Embolden).
    let semibold = draw(SkiaLabel::new("Bold").font_family("FontTextBold"));
    let picked = draw(SkiaLabel::new("Bold").font_attributes(FontAttributes::Bold));
    let emboldened = draw(SkiaLabel::new("Bold").font_family("FontTextBold").font_attributes(FontAttributes::Bold));
    assert_eq!(picked, semibold);
    assert!(emboldened.len() > semibold.len() + 40, "{} vs {}", emboldened.len(), semibold.len());
}

#[test]
fn a_fallback_draws_the_glyph_the_main_font_lacks() {
    let draw = |label: Build<SkiaLabel>| {
        let mut host = host(label.font_size(40).text_color(Color::WHITE), 200.0, 1.0);
        ink(&mut host, Rect::from_wh(200.0, 60.0), not_black)
    };
    let alone = draw(SkiaLabel::new("\u{2665}").font_family("FontSymbols2"));
    let fallback = draw(SkiaLabel::new("\u{2665}").font_family_fallback("FontSymbols,FontSymbols2"));
    let tofu = draw(SkiaLabel::new("\u{2665}"));
    assert_eq!(fallback, alone);
    assert_ne!(tofu, alone);
    // C# FallbackCharacter, opt in: no glyph anywhere becomes the character.
    let host = host(SkiaLabel::new("a\u{2665}b").fallback_character(Some('?')), 200.0, 1.0);
    assert_eq!(label(&host).lines().next().unwrap().0, "a?b");
    let host = self::host(SkiaLabel::new("a\u{2665}b").fallback_character(Some('?')).font_family_fallback("FontSymbols2"), 200.0, 1.0);
    assert_eq!(label(&host).lines().next().unwrap().0, "a\u{2665}b");
}

#[test]
fn color_emoji_keep_their_colors() {
    if emoji_font().is_none() {
        return;
    }
    let mut host = host(SkiaLabel::new("\u{1F600}").font_size(40).font_family_fallback("FontEmoji").text_color(Color::WHITE), 200.0, 1.0);
    // A yellow face, whatever the text color.
    let yellow = ink(&mut host, Rect::from_wh(200.0, 60.0), |c| c.r() > 200 && c.g() > 150 && c.b() < 100);
    assert!(yellow.len() > 200, "{} yellow pixels", yellow.len());
}

#[test]
fn the_gradient_goes_on_the_glyphs_unless_there_is_a_background() {
    let red_to_blue = SkiaGradient::new(GradientType::Linear, vec![Color::RED, Color::BLUE]).angle(90.0);
    let text = || SkiaLabel::new("IIIIIIIIII").font_size(40).text_color(Color::WHITE).fill_gradient(red_to_blue.clone());
    let mut host = host(text(), 300.0, 1.0);
    let ink = ink(&mut host, Rect::from_wh(300.0, 60.0), not_black);
    let (first, last) = (ink.iter().min_by_key(|p| p.0).unwrap(), ink.iter().max_by_key(|p| p.0).unwrap());
    let (left, right) = (host.pixel(first.0 + 1, first.1), host.pixel(last.0 - 1, last.1));
    assert!(left.r() > left.b() && right.b() > right.r(), "{left:?} {right:?}");
    assert_eq!(host.pixel(first.0 + 1, 55), Color::BLACK, "no background");

    // A background color takes the gradient (C# SetupBackgroundPaint); the glyphs keep their color.
    let mut host = self::host(text().background_color(Color::from_rgb(0, 80, 0)), 300.0, 1.0);
    let (bg_left, bg_right) = (host.pixel(1, 2), host.pixel(290, 2));
    assert!(bg_left.r() > bg_left.b() && bg_right.b() > bg_right.r(), "{bg_left:?} {bg_right:?}");
    assert_eq!(host.pixel(first.0 + 3, first.1 + 10), Color::WHITE);
}

#[test]
fn stroke_and_drop_shadow_draw_under_the_fill() {
    let label = SkiaLabel::new("I").font_size(60).text_color(Color::WHITE).stroke_color(Color::GREEN).stroke_width(2);
    let label = label.drop_shadow_color(Color::RED).drop_shadow_size(1).drop_shadow_offset_x(12).drop_shadow_offset_y(12);
    let mut host = host(label.margin(20), 200.0, 1.0);
    let area = Rect::from_wh(200.0, 140.0);
    let white = ink(&mut host, area, |c| c == Color::WHITE);
    let green = ink(&mut host, area, |c| c.g() > 200 && c.r() < 60);
    let red = ink(&mut host, area, |c| c.r() > 200 && c.g() < 60);
    assert!(!white.is_empty() && !green.is_empty() && !red.is_empty());
    let right = |ps: &[(i32, i32)]| ps.iter().map(|p| p.0).max().unwrap();
    let bottom = |ps: &[(i32, i32)]| ps.iter().map(|p| p.1).max().unwrap();
    // The outline around the glyph, the shadow 12 px right and down of it.
    assert!(right(&green) > right(&white) && bottom(&green) > bottom(&white));
    assert!((right(&red) - right(&green) - 12).abs() <= 3 && (bottom(&red) - bottom(&green) - 12).abs() <= 3, "{} {}", right(&red), right(&green));
}

#[test]
fn span_taps_run_their_handler_and_show_the_hand() {
    let spans = (
        TextSpan::new("Tap "),
        TextSpan::new("here").on_tapped(|me: &mut Mut<SkiaLabel>, app: &mut App, _cx| app.tapped.push(format!("span {}", me.spans.len()))),
        TextSpan::new(" or not"),
    );
    let mut host = host(SkiaLabel::new("").font_size(20).spans(spans), 300.0, 1.0);
    let runs: Vec<LabelRun> = label(&host).runs().collect();
    let x_of = |i: usize| runs[..i].iter().map(|r| r.width).sum::<f32>();
    let (here, plain) = (x_of(1) + runs[1].width / 2.0, x_of(0) + 5.0);
    host.tap(plain, 12.0);
    assert!(host.ui.state.tapped.is_empty());
    host.tap(here, 12.0);
    assert_eq!(host.ui.state.tapped, ["span 3"]);

    // The hand over the span, not over the rest (DrawnUi.React WantsPointerCursor).
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Hover, here, 12.0, now);
    host.frame_after(16.0);
    assert_eq!(host.ui.cursor(), Cursor::Pointer);
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Hover, plain, 12.0, now);
    host.frame_after(16.0);
    assert_eq!(host.ui.cursor(), Cursor::Default);
}

#[test]
fn a_tapped_link_reports_its_url() {
    let rich = SkiaRichLabel::new("[DrawnUI](https://drawnui.net) and text").font_size(20);
    let rich = rich.on_link_tapped(|_me: &mut Mut<SkiaRichLabel>, app: &mut App, _cx, url: &str| app.tapped.push(url.to_owned()));
    let mut host = host_of(|app| SkiaLayout::new().fill().children(rich.assign(&mut app.rich)), 300.0, 100.0, 1.0);
    host.tap(150.0, 12.0);
    assert!(host.ui.state.tapped.is_empty());
    host.tap(10.0, 12.0);
    assert_eq!(host.ui.state.tapped, ["https://drawnui.net"]);
}

#[test]
fn spaces_only_or_hidden_spans_measure_one_empty_line() {
    let line = label(&host(SkiaLabel::new("a").font_size(16), 200.0, 1.0)).content_size().height;
    let spaces = host(SkiaLabel::new("   ").font_size(16), 200.0, 1.0);
    assert_eq!((label(&spaces).lines_count(), label(&spaces).content_size().height), (1, line));
    let hidden = host(SkiaLabel::new("").font_size(16).spans(TextSpan::new("gone").is_visible(false)), 200.0, 1.0);
    assert_eq!((label(&hidden).lines_count(), label(&hidden).content_size().height), (1, line));
}

#[test]
fn the_spoken_label_is_the_text_of_the_spans() {
    let host = host(SkiaLabel::new("").spans((TextSpan::new("One "), TextSpan::new("two").is_bold(true))), 200.0, 1.0);
    assert_eq!(label(&host).accessibility_label().as_deref(), Some("One two"));
    let host = self::host(SkiaLabel::new("Plain"), 200.0, 1.0);
    assert_eq!(label(&host).accessibility_label().as_deref(), Some("Plain"));
}

#[test]
fn a_frame_with_nothing_changed_allocates_nothing() {
    let spans = SkiaLabel::new("").font_size(16).font_family_fallback("FontSymbols").spans(page_spans()).use_cache(CacheType::None);
    let mut host = host(spans, 300.0, 1.0);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    let before = ALLOCATIONS.with(Cell::get);
    for _ in 0..20 {
        host.frame_after(16.0);
    }
    assert_eq!(ALLOCATIONS.with(Cell::get) - before, 0);
}

#[test]
fn slanted_glyphs_lean_into_the_effects_margin() {
    // A synthetic italic leans right by a quarter of the glyph height: caches keep it.
    let upright = host(SkiaLabel::new("If").font_size(40), 200.0, 1.0);
    let italic = host(SkiaLabel::new("If").font_size(40).font_attributes(FontAttributes::Italic), 200.0, 1.0);
    assert_eq!(label(&upright).effects_margin(1.0).right, 0.0);
    let right = label(&italic).effects_margin(1.0).right;
    assert!((9.0..=13.0).contains(&right), "{right}");
}

#[test]
fn span_background_underline_and_strikeout() {
    let blue = Color::from_rgb(0, 0, 255);
    let spans = (TextSpan::new("ab "), TextSpan::new("cd").background_color(blue).underline(true).underline_width(2), TextSpan::new(" ef").strikeout(true).strikeout_width(2));
    let mut host = host(SkiaLabel::new("").font_size(40).text_color(Color::WHITE).spans(spans), 300.0, 1.0);
    let runs: Vec<LabelRun> = label(&host).runs().collect();
    let x0 = runs[0].width as i32;
    let (cd, ef) = ((x0 + 2, x0 + runs[1].width as i32 - 2), (x0 + runs[1].width as i32 + 12, (x0 as f32 + runs[1].width + runs[2].width) as i32 - 2));
    let row = |host: &mut Host, y: i32, (from, to): (i32, i32), of: &dyn Fn(Color) -> bool| (from..to).all(|x| of(host.pixel(x, y)));
    // The background fills the run's line box, the underline crosses it under the baseline.
    assert!(row(&mut host, 1, cd, &|c| c == blue));
    assert!((30..56).any(|y| row(&mut host, y, cd, &|c| c == Color::WHITE)), "no underline");
    // The strikeout, red by default, crosses the run at half an x-height above the baseline.
    assert!((20..40).any(|y| row(&mut host, y, ef, &|c| c.r() > 200 && c.g() < 60)), "no strikeout");
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn cost_of_spans_and_fallback() {
    use std::time::Instant;
    const FRAMES: u32 = 200;
    let cases: [(&str, fn() -> Build<SkiaLabel>); 4] = [
        ("plain text, 1 style", || SkiaLabel::new(LOREM).font_size(15)),
        ("plain text + 2 fallbacks asked", || SkiaLabel::new(LOREM).font_size(15).font_family_fallback("FontSymbols,FontSymbols2")),
        ("17 spans + fallback", || SkiaLabel::new("").font_size(16).font_family_fallback("FontSymbols").spans(page_spans())),
        ("symbols, 2 fallbacks used", || SkiaLabel::new(SYMBOLS_TEXT).font_size(16).font_family_fallback("FontSymbols,FontSymbols2")),
    ];
    for scale in [1.0f32, 2.0] {
        for (name, build) in cases {
            // A measure every frame (MaxLines flips between two values that cut nothing).
            let mut host = host(build(), 656.0, scale);
            let id = host.ui.state.label;
            let start = Instant::now();
            for i in 0..FRAMES {
                host.ui.tree.find_mut::<SkiaLabel>(id).unwrap().set_max_lines(if i % 2 == 0 { 50 } else { 51 });
                host.frame_after(16.0);
            }
            let measured = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
            // Nothing changed, no cache: paint only.
            let mut host = self::host(build().use_cache(CacheType::None), 656.0, scale);
            let start = Instant::now();
            for _ in 0..FRAMES {
                host.frame_after(16.0);
            }
            let painted = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
            println!("scale {scale}, {name}: {measured:.0} us per frame with a measure, {painted:.0} us per uncached paint");
        }
        let rich = || SkiaRichLabel::new(MARKDOWN).font_size(15).font_family_fallback("FontSymbols,FontSymbols2");
        let mut host = host_of(|app| SkiaLayout::new().fill().children(rich().width_request(656).assign(&mut app.rich)), 664.0, 300.0, scale);
        let id = host.ui.state.rich;
        let start = Instant::now();
        for i in 0..FRAMES {
            host.ui.tree.find_mut::<SkiaRichLabel>(id).unwrap().set_text(if i % 2 == 0 { format!("{MARKDOWN} ") } else { MARKDOWN.to_owned() });
            host.frame_after(16.0);
        }
        let parsed = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
        println!("scale {scale}, rich label, TextPage markdown: {parsed:.0} us per frame with a parse and a measure");
    }
}

#[test]
fn selectable_text_gets_the_lines_and_their_css_font() {
    let build = SkiaLabel::new("First line\nSecond").font_size(16).font_attributes(FontAttributes::Bold).font_family_fallback("FontSymbols, FontSymbols2");
    let build = build.padding(10).accessibility_role(Aria::TEXT).accessibility_text_selectable(true);
    let mut ui = Ui::new(App::default(), |app| SkiaLayout::new().fill().children(build.assign(&mut app.label))).font_bytes("FontText", REGULAR);
    ui.fonts.add("FontSymbols", SYMBOLS);
    let mut host = Headless::new(ui, 400, 200, 2.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let lines = &host.ui.accessibility_nodes()[0].text_lines;
    let expected: Vec<(String, f32)> = label(&host).lines().map(|(t, w)| (t.to_owned(), w / 2.0)).collect();
    let line = label(&host).measured_line_height() / 2.0;
    assert_eq!(lines.iter().map(|l| (l.text.clone(), l.width)).collect::<Vec<_>>(), expected);
    assert_eq!(lines.iter().map(|l| (l.left, l.top)).collect::<Vec<_>>(), [(10.0, 10.0), (10.0, 10.0 + line)]);
    let l = &lines[0];
    assert_eq!((l.font_family.as_str(), l.font_weight, l.font_size, l.height), ("FontText, FontSymbols, FontSymbols2", 600, 16.0, line));
}
