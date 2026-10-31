//! Text shaping (harfrust) and bidirectional order (unicode-bidi) in SkiaLabel: Arabic letters
//! join and run right to left, a right-to-left paragraph puts its Latin words on the left, an
//! emoji with a skin tone is one glyph, and plain Latin text keeps its plain glyphs. The DrawnUI
//! .NET demo (C# SkiaLabel DrawShapedText, HarfBuzzSharp) is the reference picture.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));
const ARABIC: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoSansArabic-Subset.ttf"));
const EMOJI: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoColorEmoji-Subset.ttf"));

type Host = Headless<Handle<SkiaLabel>>;

fn host(label: Build<SkiaLabel>) -> Host {
    let build = move |handle: &mut Handle<SkiaLabel>| SkiaLayout::new().fill().children(label.font_size(30).text_color(Color::WHITE).assign(handle));
    let ui = Ui::new(Handle::default(), build)
        .font_bytes("FontText", FONT)
        .font_bytes("FontArabic", ARABIC)
        .font_bytes("FontEmoji", EMOJI)
        .background(Color::BLACK);
    let mut host = Headless::new(ui, 600, 200, 1.0);
    host.settle();
    host
}

fn label(host: &Host) -> &SkiaLabel {
    host.ui.tree.find(host.ui.state).unwrap()
}

/// Width of `text` as the label draws it, pixels.
fn width(text: &str, fallback: &str) -> f32 {
    let host = host(SkiaLabel::new(text).font_family_fallback(fallback));
    label(&host).runs().map(|r| r.width).sum()
}

/// Lam + alef is one ligature glyph in an Arabic font: narrower than the two letters apart
/// (each measured alone, in its isolated form). Without shaping the two isolated letters are drawn.
#[test]
fn arabic_letters_join() {
    let (lam, alef, joined) = (width("\u{0644}", "FontArabic"), width("\u{0627}", "FontArabic"), width("\u{0644}\u{0627}", "FontArabic"));
    assert!(lam > 0.0 && alef > 0.0, "the Arabic font draws nothing: {lam} {alef}");
    assert!(joined < lam + alef - 1.0, "lam-alef is not a ligature: {joined} vs {lam} + {alef}");
}

/// Columns of the screen where a color shows (red or blue text on black).
fn columns(host: &mut Host, color: fn(Color) -> bool) -> Option<(i32, i32)> {
    let mut found: Option<(i32, i32)> = None;
    for x in 0..600 {
        if (0..120).any(|y| color(host.pixel(x, y))) {
            found = Some(found.map_or((x, x), |(a, _)| (a, x)));
        }
    }
    found
}

fn red(c: Color) -> bool {
    c.r() > 150 && c.g() < 80 && c.b() < 80
}

fn blue(c: Color) -> bool {
    c.b() > 150 && c.r() < 80 && c.g() < 80
}

/// A paragraph that starts with an Arabic letter is right to left: its Latin word comes after the
/// Arabic one in reading order, so it is drawn on its left. A paragraph that starts with a Latin
/// letter keeps the Latin word first, on the left.
#[test]
fn a_right_to_left_paragraph_puts_its_latin_word_on_the_left() {
    let spans = |first_arabic: bool| {
        let arabic = TextSpan::new("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} ").text_color(Color::RED);
        let latin = TextSpan::new("Hello ").text_color(Color::BLUE);
        let label = SkiaLabel::new("").font_family_fallback("FontArabic");
        if first_arabic { label.spans((arabic, latin)) } else { label.spans((latin, arabic)) }
    };
    let mut rtl = host(spans(true));
    let (arabic, latin) = (columns(&mut rtl, red).expect("Arabic drawn"), columns(&mut rtl, blue).expect("Latin drawn"));
    assert!(latin.1 < arabic.0, "right to left: Latin {latin:?} should be left of Arabic {arabic:?}");
    let mut ltr = host(spans(false));
    let (arabic, latin) = (columns(&mut ltr, red).expect("Arabic drawn"), columns(&mut ltr, blue).expect("Latin drawn"));
    assert!(latin.1 < arabic.0, "left to right: Latin {latin:?} should be left of Arabic {arabic:?}");
}

/// In a right-to-left paragraph the first Arabic word is on the right: the five Arabic letters of
/// "مرحبا" end right of "بكم", which follows it.
#[test]
fn arabic_words_run_right_to_left() {
    let mut host = host(SkiaLabel::new("").font_family_fallback("FontArabic").spans((
        TextSpan::new("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} ").text_color(Color::RED),
        TextSpan::new("\u{0628}\u{0643}\u{0645}").text_color(Color::BLUE),
    )));
    let (first, second) = (columns(&mut host, red).expect("first word"), columns(&mut host, blue).expect("second word"));
    assert!(second.1 < first.0, "the second word {second:?} should be left of the first {first:?}");
}

/// A person with a skin tone is one emoji: as wide as the person alone, not the person plus a
/// color swatch.
#[test]
fn an_emoji_with_a_skin_tone_is_one_glyph() {
    let alone = width("\u{1F64E}", "FontEmoji");
    let toned = width("\u{1F64E}\u{1F3FD}", "FontEmoji");
    assert!(alone > 10.0, "the emoji font draws nothing: {alone}");
    assert!((toned - alone).abs() < 1.0, "skin tone drawn apart: {toned} vs {alone}");
}

/// Latin text needs no shaping: the same widths as before (the plain path, one glyph per character).
#[test]
fn latin_text_keeps_its_plain_widths() {
    let host = host(SkiaLabel::new("Hello world"));
    let mut font = drawnui::skia::Font::from_typeface(drawnui::skia::FontMgr::new().new_from_data(drawnui::skia::Data::new_copy(FONT), None).unwrap(), 30.0);
    font.set_linear_metrics(true);
    let expected = font.measure_str("Hello world", None).0;
    let got: f32 = label(&host).runs().map(|r| r.width).sum();
    assert!((got - expected).abs() < 0.5, "{got} vs {expected}");
}

/// A space keeps the style of the span it was written in (C# SkiaLabel): a highlighted span
/// " BiG " after "as " has its own spaces inside the highlight, and the space of "as " before it
/// stays plain. The DrawnUI .NET demo's "Format text as  BiG  and small!".
#[test]
fn a_space_keeps_the_style_of_its_span() {
    let mut host = host(SkiaLabel::new("").spans((
        TextSpan::new("as ").text_color(Color::WHITE),
        TextSpan::new(" BiG ").background_color(Color::YELLOW).text_color(Color::BLACK),
        TextSpan::new("and").text_color(Color::WHITE),
    )));
    let runs: Vec<(String, Option<usize>)> = label(&host).runs().map(|r| (r.text.to_owned(), r.span)).collect();
    assert_eq!(runs, [("as ".to_owned(), Some(0)), (" BiG ".to_owned(), Some(1)), ("and".to_owned(), Some(2))]);
    // The yellow starts after the plain space of "as " and ends after the space of " BiG ".
    let yellow = |c: Color| c.r() > 200 && c.g() > 200 && c.b() < 80;
    let (from, to) = columns(&mut host, yellow).expect("the highlight is drawn");
    let space = label(&host).runs().next().map(|r| r.width).unwrap();
    assert!(from as f32 >= space - 1.0, "the highlight covers the space of \"as \": starts at {from}, \"as \" ends at {space}");
    let big: f32 = label(&host).runs().take(2).map(|r| r.width).sum();
    assert!((to as f32 - big).abs() <= 2.0, "the highlight ends at {to}, \" BiG \" ends at {big}");
}

/// A word whose leftmost glyph carries a dot mark (ث, ي, ن) keeps its full advance and the space
/// next to it: the run width comes from pen advances, not from where the glyphs are drawn
/// (DrawnUi.React found "حيثالدقة" with a glyph-position width).
#[test]
fn a_dotted_letter_keeps_the_space_next_to_it() {
    let (first, second) = ("\u{062D}\u{064A}\u{062B}", "\u{0627}\u{0644}\u{062F}\u{0642}\u{0629}");
    let both = width(&format!("{first} {second}"), "FontArabic");
    let space = width("\u{062D} \u{062D}", "FontArabic") - 2.0 * width("\u{062D}", "FontArabic");
    let (a, b) = (width(first, "FontArabic"), width(second, "FontArabic"));
    assert!(space > 2.0, "no space width: {space}");
    assert!((both - (a + space + b)).abs() < 1.0, "\"{first} {second}\" is {both}, the words and the space {a} + {space} + {b}");
}

/// Ink columns of white text in the label's row band.
fn ink(host: &mut Host) -> (i32, i32) {
    columns(host, |c| c.r() > 150 && c.g() > 150 && c.b() > 150).expect("text drawn")
}

fn aligned(text: &'static str, alignment: Option<TextAlignment>) -> Host {
    let label = SkiaLabel::new(text).font_family_fallback("FontArabic").fill_x().padding(10);
    host(match alignment {
        Some(a) => label.horizontal_text_alignment(a),
        None => label,
    })
}

/// Start (the default) follows the paragraph's direction: a right-to-left paragraph sits at the
/// right edge, inside the padding; End puts it at the left (drawnui-cross 6am).
#[test]
fn a_right_to_left_paragraph_starts_at_the_right() {
    let mut start = aligned("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} \u{0628}\u{0643}\u{0645}", None);
    let (_, right) = ink(&mut start);
    assert!((right - (600 - 10 - 1)).abs() <= 3, "Start: the ink ends at {right}, the label's right inside the padding is 589");
    let mut end = aligned("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} \u{0628}\u{0643}\u{0645}", Some(TextAlignment::End));
    let (left, _) = ink(&mut end);
    assert!((left - 10).abs() <= 3, "End: the ink starts at {left}, the label's left inside the padding is 10");
    // Left to right text keeps Start on the left.
    let mut latin = aligned("Hello", None);
    let (left, _) = ink(&mut latin);
    assert!((left - 10).abs() <= 3, "Latin Start: {left}");
}

/// Each paragraph aligns by its own direction: an Arabic paragraph at the right, a Latin one at the
/// left, in one label.
#[test]
fn each_paragraph_aligns_by_its_own_direction() {
    let host = aligned("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}\nHello", None);
    let lines: Vec<Rect> = label(&host).text_lines().map(|l| l.rect).collect();
    assert_eq!(lines.len(), 2);
    assert!((lines[0].right - 590.0).abs() < 1.0, "the Arabic line ends at {}, not the right edge 590", lines[0].right);
    assert!((lines[1].left - 10.0).abs() < 1.0, "the Latin line starts at {}, not the left edge 10", lines[1].left);
}
