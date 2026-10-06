//! The default font is the first one registered, whatever order the files arrive in.

use drawnui::App as _;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

fn build(_app: &mut ()) -> Build<SkiaLayout> {
    SkiaLayout::column().children((SkiaLabel::new("Hello"),))
}

#[test]
fn default_font_is_the_first_registered_even_when_it_arrives_last() {
    let ui = Ui::new((), build).font("First", "fonts/first.ttf").font("Second", "fonts/second.ttf");
    let mut host = Headless::new(ui, 200, 100, 1.0);

    // The browser delivers in any order: the second file first.
    host.ui.asset(1, FONT.to_vec());
    host.frame();
    host.ui.asset(0, FONT.to_vec());
    host.settle();

    let fonts = &host.ui.fonts;
    let id = |family: &str| fonts.typeface(family).map(|t| t.unique_id());
    assert_ne!(id("First"), id("Second"));
    assert_eq!(id(""), id("First"));
    assert_eq!(id("not registered"), id("First"));
}

#[test]
fn a_default_font_that_failed_to_load_falls_back_without_taking_another_alias() {
    let ui = Ui::new((), build).font("First", "fonts/missing.ttf").font("Second", "fonts/second.ttf");
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.ui.asset(0, Vec::new());
    host.ui.asset(1, FONT.to_vec());
    host.settle();

    let fonts = &host.ui.fonts;
    // "Second" is still only reachable by its name; the default is the system font, if any.
    let second = fonts.typeface("Second").map(|t| t.unique_id());
    assert!(second.is_some());
    assert_ne!(fonts.typeface("").map(|t| t.unique_id()), second);
}
