//! A cached label keeps the ink that reaches past its glyph boxes: the left hook of "j" (a
//! negative left side bearing) and the slant of an italic. With CacheType::Image and Operations
//! the leftmost and rightmost ink columns equal those of the uncached label (C# found "jumps" cut
//! at its margin; React the italic descender).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

/// The leftmost and rightmost columns with ink of a label at margin 50, FontSize 40.
fn ink(text: &'static str, italic: bool, cache: CacheType) -> (i32, i32) {
    let build = move |_: &mut ()| {
        let label = SkiaLabel::new(text).font_size(40).text_color(Color::WHITE).margin((50, 20, 0, 0)).use_cache(cache);
        SkiaLayout::new().fill().children(if italic { label.font_attributes(FontAttributes::Italic) } else { label })
    };
    let ui = Ui::new((), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 120, 1.0);
    host.settle();
    let lit = |host: &mut Headless<()>, x: i32| (0..120).any(|y| host.pixel(x, y).r() > 40);
    let left = (0..400).find(|x| lit(&mut host, *x)).expect("ink");
    let right = (0..400).rev().find(|x| lit(&mut host, *x)).expect("ink");
    (left, right)
}

#[test]
fn a_cached_label_keeps_the_ink_past_its_glyph_boxes() {
    for (text, italic) in [("jumps", false), ("jumps", true), ("Hello world", true)] {
        let live = ink(text, italic, CacheType::None);
        for cache in [CacheType::Image, CacheType::Operations] {
            let cached = ink(text, italic, cache);
            assert_eq!(cached, live, "{text:?} italic {italic} {cache:?}: cached ink {cached:?}, uncached {live:?}");
        }
    }
}
