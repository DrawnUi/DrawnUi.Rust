//! Default-ignorable code points (U+FE0F after an emoji, the zero width joiner, tags) that no font
//! of a label has draw nothing and take no room. Labels are not shaped; C# keeps them in the font of
//! the glyph before them, where shaping hides them.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
/// Noto Color Emoji (COLRv1) cut to 181 characters: it has U+2699 but not U+FE0F.
const EMOJI: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoColorEmoji-Subset.ttf"));

/// The pixels and the measured width of a label with `text`.
fn render(text: &str, label: impl Fn(Build<SkiaLabel>) -> Build<SkiaLabel>) -> (Vec<Color>, f32) {
    let text = text.to_owned();
    let build = move |handle: &mut Handle<SkiaLabel>| {
        let built = label(SkiaLabel::new(text.clone()).font_size(40).text_color(Color::WHITE));
        SkiaLayout::new().fill().children(built.assign(handle))
    };
    let ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).font_bytes("FontEmoji", EMOJI).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 80, 1.0);
    host.settle();
    let width = host.rect(host.ui.state).width();
    let pixels = (0..80).flat_map(|y| (0..300).map(move |x| (x, y))).map(|(x, y)| host.pixel(x, y)).collect();
    (pixels, width)
}

#[test]
fn a_variation_selector_no_font_has_draws_nothing() {
    let emoji = |l: Build<SkiaLabel>| l.font_family_fallback("FontEmoji");
    let (gear, width) = render("\u{2699} 4m", emoji);
    let (selected, selected_width) = render("\u{2699}\u{FE0F} 4m", emoji);
    assert!(gear.iter().any(|&c| c != Color::BLACK && c != Color::WHITE), "the gear is drawn in color");
    assert_eq!(selected_width, width);
    assert!(selected == gear, "U+FE0F draws no box after the gear");
}

#[test]
fn a_joiner_or_tag_no_font_has_draws_nothing() {
    let (plain, width) = render("AB", |l| l);
    for text in ["A\u{200D}B", "A\u{FE0F}B", "A\u{E0067}B", "A\u{2060}B"] {
        let (with, with_width) = render(text, |l| l);
        assert_eq!(with_width, width, "{text:?}");
        assert!(with == plain, "{text:?} draws as AB");
    }
}

#[test]
fn the_fallback_character_does_not_replace_an_ignorable() {
    let (plain, _) = render("AB", |l| l.fallback_character(Some('?')));
    let (with, _) = render("A\u{FE0F}B", |l| l.fallback_character(Some('?')));
    assert!(with == plain);
}
