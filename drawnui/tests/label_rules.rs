//! The line layout rules of SkiaLabel that the tests ported from DrawnUi.Net.Tests do not reach. Not upstream
//! tests: the expectations come from reading C# SkiaLabel (DecomposeText, OnMeasuring, DrawLines).

use drawnui::controls::label::{LineBreakMode, TextTransform};
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const WORDS: &str = "aaa bbb ccc ddd eee fff ggg hhh";

type Host = Headless<Handle<SkiaLabel>>;

fn host(label: Build<SkiaLabel>) -> Host {
    let build = |handle: &mut Handle<SkiaLabel>| SkiaLayout::new().fill().children(label.font_size(16).assign(handle));
    let ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    host
}

fn label(host: &Host) -> &SkiaLabel {
    host.ui.tree.find(host.ui.state).unwrap()
}

fn lines(label: Build<SkiaLabel>) -> Vec<String> {
    let host = host(label);
    self::label(&host).lines().map(|l| l.0.to_owned()).collect()
}

/// Left, top, right, bottom of the pixels that are not background.
fn ink(host: &mut Host) -> (i32, i32, i32, i32) {
    let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..400 {
        for x in 0..400 {
            if host.pixel(x, y) != Color::BLACK {
                bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x + 1), bounds.3.max(y + 1));
            }
        }
    }
    bounds
}

#[test]
fn words_wrap_at_spaces() {
    let host = host(SkiaLabel::new(WORDS).width_request(100).line_break_mode(LineBreakMode::WordWrap));
    let lines: Vec<_> = label(&host).lines().collect();
    assert!(lines.len() > 2);
    for (text, width) in &lines {
        assert!(*width <= 101.0 && !text.starts_with(' ') && !text.ends_with(' '), "{width} '{text}'");
    }
    assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>().join(" "), WORDS);
    // The default mode wraps the same way while MaxLines and the height allow.
    assert_eq!(self::lines(SkiaLabel::new(WORDS).width_request(100)).len(), lines.len());
}

#[test]
fn no_wrap_keeps_one_line() {
    let one = SkiaLabel::new(WORDS).width_request(100).line_break_mode(LineBreakMode::NoWrap).max_lines(1);
    assert_eq!(lines(one), [WORDS]);
    // DrawnUi.React: one line runs past the edge also without MaxLines (C#: the word that
    // overflows closes the line).
    let open = lines(SkiaLabel::new(WORDS).width_request(100).line_break_mode(LineBreakMode::NoWrap));
    assert_eq!(open, [WORDS]);
}

#[test]
fn max_lines_cuts_and_only_tail_truncation_adds_the_trail() {
    let wrapped = lines(SkiaLabel::new(WORDS).width_request(100).line_break_mode(LineBreakMode::WordWrap).max_lines(2));
    assert_eq!(wrapped.len(), 2);
    assert!(WORDS.starts_with(&wrapped.join(" ")), "{wrapped:?}");

    let host = host(SkiaLabel::new(WORDS).width_request(100).max_lines(2));
    let cut: Vec<_> = label(&host).lines().collect();
    assert_eq!(cut.len(), 2);
    assert_eq!(cut[0].0, wrapped[0]);
    // DrawnUi.React: the ellipsis character, cut by advance (C#: "..", cut by ink).
    assert!(cut[1].0.ends_with('\u{2026}') && cut[1].1 <= 100.0, "{cut:?}");
}

#[test]
fn the_height_limits_the_lines() {
    // Room for one line and a half: the first line is the last one and gets the trail.
    let cut = lines(SkiaLabel::new(WORDS).width_request(100).height_request(34));
    assert_eq!(cut.len(), 1);
    assert!(cut[0].ends_with('\u{2026}'), "{cut:?}");
}

#[test]
fn paragraphs_spacing_and_line_height() {
    let one = host(SkiaLabel::new("a"));
    let line = label(&one).measured_line_height();
    assert_eq!(label(&one).content_size().height, line.ceil());

    // OpenSans has no leading: LineSpacing adds `line * (spacing - 1)` between lines, and a paragraph
    // adds ParagraphSpacing of a line with its spacing: none by default, as DrawnUi.React (C#: 0.25).
    let two = host(SkiaLabel::new("a\nb"));
    assert_eq!(label(&two).content_size().height, (line * 2.0).ceil());
    let upstream = host(SkiaLabel::new("a\nb").paragraph_spacing(0.25));
    assert_eq!(label(&upstream).content_size().height, (line * 2.0 + line * 0.25).ceil());
    let spaced = host(SkiaLabel::new("a\nb").line_spacing(1.5).paragraph_spacing(1));
    assert_eq!(label(&spaced).content_size().height, (line * 2.0 + line * 0.5 + line * 1.5).ceil());

    assert_eq!(lines(SkiaLabel::new("a\n\nb")), ["a", "", "b"]);
    assert_eq!(lines(SkiaLabel::new("a\u{2028}b")), ["a", "b"]);
    // DrawnUi.React: a run of spaces is one (C#: it loses one).
    assert_eq!(lines(SkiaLabel::new("a   b")), ["a b"]);

    let tall = host(SkiaLabel::new("a").line_height(2));
    assert!((label(&tall).measured_line_height() - line * 2.0).abs() <= 1.0);
}

#[test]
fn character_spacing_widens_the_line() {
    let plain = host(SkiaLabel::new("abcd"));
    let wide = host(SkiaLabel::new("abcd").character_spacing(3));
    // 2 px more between each of 4 glyphs; line widths are rounded.
    let grown = label(&wide).lines().next().unwrap().1 - label(&plain).lines().next().unwrap().1;
    assert!((grown - 6.0).abs() <= 1.0, "{grown}");
}

#[test]
fn text_is_aligned_inside_the_label() {
    let sized = || SkiaLabel::new("H").width_request(200).height_request(100).text_color(Color::WHITE);
    let start = ink(&mut host(sized()));
    let (width, height) = (start.2 - start.0, start.3 - start.1);
    assert!(start.0 < 4 && start.1 < 12, "{start:?}");

    let center = ink(&mut host(sized().horizontal_text_alignment(TextAlignment::Center).vertical_text_alignment(TextAlignment::Center)));
    assert!((center.0 + center.2 - 200).abs() <= 2 && (center.1 + center.3 - 100).abs() <= 6, "{center:?}");
    assert_eq!((center.2 - center.0, center.3 - center.1), (width, height));

    let end = ink(&mut host(sized().horizontal_text_alignment(TextAlignment::End).vertical_text_alignment(TextAlignment::End)));
    assert!(200 - end.2 < 4 && 100 - end.3 < 8 && end.3 <= 100, "{end:?}");
}

#[test]
fn transform_fallback_and_empty_text() {
    assert_eq!(lines(SkiaLabel::new("Abc").text_transform(TextTransform::Uppercase)), ["ABC"]);
    assert_eq!(lines(SkiaLabel::new("Abc").text_transform(TextTransform::Lowercase)), ["abc"]);
    // OpenSans has no glyph for 背: drawn as the missing-glyph box, as DrawnUi.React, or replaced by
    // the fallback character when one is given (C#, where the default is a space).
    assert_eq!(lines(SkiaLabel::new("a背b")), ["a背b"]);
    assert_eq!(lines(SkiaLabel::new("a背b").fallback_character(Some('?'))), ["a?b"]);

    let empty = host(SkiaLabel::new(""));
    assert_eq!(label(&empty).lines_count(), 0);
    assert!(empty.rect(empty.ui.state).is_empty());
}

#[test]
fn a_system_font_draws_what_the_fonts_lack() {
    // C# SkiaRichLabel and SkiaEditor (UseUnicode): MatchCharacter. Only where the system has a
    // font for 背 (not on the web, nor on a machine without CJK fonts).
    if drawnui::Fonts::default().match_character('背').is_none() {
        return;
    }
    assert_eq!(lines(SkiaLabel::new("a背b").fallback_character(Some('?')).system_font_fallback(true)), ["a背b"]);
    // A plain SkiaLabel (C#) keeps to its own fonts.
    assert_eq!(lines(SkiaLabel::new("a背b").fallback_character(Some('?'))), ["a?b"]);
    // Drawn with the system font's glyph, not the missing-glyph box.
    let lit = |host: &mut Host| (0..60).flat_map(|y| (0..60).map(move |x| (x, y))).filter(|&(x, y)| host.pixel(x, y) != Color::BLACK).count();
    let boxed = lit(&mut host(SkiaLabel::new("背").font_size(40)));
    let drawn = lit(&mut host(SkiaLabel::new("背").font_size(40).system_font_fallback(true)));
    assert!(drawn != boxed && drawn > 0, "glyph {drawn} px, box {boxed} px");
}
