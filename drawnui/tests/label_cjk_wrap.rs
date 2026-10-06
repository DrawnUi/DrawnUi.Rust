//! Port of DrawnUi.Net.Tests LabelCjkWrapTests. Japanese and Chinese have no spaces: WordWrap must break between
//! ideographs and kana (never before 、。ー」 or small kana, never after 「), and a Latin word wider than the line
//! (a URL) breaks by characters instead of overflowing.

use drawnui::controls::label::LineBreakMode;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

/// A Japanese font (the repo's, else Windows'). Without one the Japanese tests pass and prove nothing.
const JAPANESE_FONTS: [&str; 2] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/IBMPlexSansJP-Regular.ttf"),
    r"C:\Windows\Fonts\NotoSansJP-VF.ttf",
];

const JAPANESE: &str = "背景の部屋をどう見せるかを選びます。今のプリセットは下のスイッチでオフにできます。";

fn japanese_font() -> Option<Vec<u8>> {
    let font = JAPANESE_FONTS.iter().find_map(|path| std::fs::read(path).ok());
    if font.is_none() {
        eprintln!("no Japanese font on this machine: nothing proved");
    }
    font
}

/// The lines (text, width in pixels) of a 16 pt label `width` points wide.
fn layout(text: &str, mode: LineBreakMode, max_lines: i32, width: f32, japanese: Option<&[u8]>) -> Vec<(String, f32)> {
    let build = |label: &mut Handle<SkiaLabel>| {
        SkiaLayout::new().fill().children(
            SkiaLabel::new(text)
                .font_size(16)
                .width_request(width)
                .line_break_mode(mode)
                .max_lines(max_lines)
                .font_family(if japanese.is_some() { "FontJapaneseTest" } else { "" })
                .assign(label),
        )
    };
    let mut ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    if let Some(bytes) = japanese {
        ui = ui.font_bytes("FontJapaneseTest", bytes);
    }
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    let label = host.ui.tree.find::<SkiaLabel>(host.ui.state).unwrap();
    let lines: Vec<_> = label.lines().map(|(text, width)| (text.to_owned(), width)).collect();
    for (text, width) in &lines {
        println!("{width:6} '{text}'");
    }
    lines
}

fn assert_fit(lines: &[(String, f32)], limit: f32) {
    for (text, width) in lines {
        assert!(*width <= limit, "line wider than the label: {width} '{text}'");
    }
}

#[test]
fn japanese_word_wrap_breaks_between_characters_with_kinsoku() {
    let Some(font) = japanese_font() else { return };
    let lines = layout(JAPANESE, LineBreakMode::WordWrap, -1, 300.0, Some(&font));

    assert!((2..=4).contains(&lines.len()), "{} lines", lines.len());
    assert_fit(&lines, 301.0);
    for (text, _) in &lines[1..] {
        assert!(!"、。ー」』）っゃゅょッャュョ".contains(text.chars().next().unwrap()), "line starts with '{text}'");
    }
    assert_eq!(lines.iter().map(|l| l.0.as_str()).collect::<String>(), JAPANESE);
}

#[test]
fn japanese_after_a_latin_word_fills_the_line() {
    let Some(font) = japanese_font() else { return };
    let text = format!("DrawnCamera {JAPANESE}");
    let lines = layout(&text, LineBreakMode::WordWrap, -1, 300.0, Some(&font));

    assert!(lines[0].0.starts_with("DrawnCamera 背景"), "first line '{}'", lines[0].0);
    assert_fit(&lines, 301.0);
}

#[test]
fn japanese_tail_truncation_keeps_max_lines() {
    let Some(font) = japanese_font() else { return };
    let text = format!("{JAPANESE}{JAPANESE}");
    let lines = layout(&text, LineBreakMode::TailTruncation, 2, 300.0, Some(&font));

    assert_eq!(lines.len(), 2);
    assert_fit(&lines, 301.0);
}

#[test]
fn long_latin_word_word_wrap_breaks_by_characters() {
    const URL: &str = "https://drawnui.net/articles/a/very/long/path/without/any/space/that/cannot/fit/on/one/line";
    let lines = layout(&format!("See {URL}"), LineBreakMode::WordWrap, -1, 200.0, None);

    assert_eq!(lines[0].0.trim(), "See");
    assert!(lines.len() > 2, "the URL was not broken");
    assert_fit(&lines, 201.0);
    assert_eq!(lines[1..].iter().map(|l| l.0.as_str()).collect::<String>(), URL);
}

#[test]
fn break_opportunities() {
    let cases = [
        ("背景", 1, true),
        ("部屋。", 2, false), // never before 。
        ("「今", 1, false),   // never after 「
        ("ャッ", 1, false),   // never before small kana
        ("ab", 1, false),     // Latin keeps breaking at spaces
        ("aの", 1, true),
        ("한국", 1, false), // not upstream: Korean keeps breaking at spaces
    ];
    for (text, index, expected) in cases {
        assert_eq!(SkiaLabel::can_break_inside_word(text, index), expected, "'{text}' at {index}");
    }
}
