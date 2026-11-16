//! A phone's soft keyboard opens over the lower part of the canvas (drawnui-cross 6cj): the
//! scroll around the focused text field moves it into the part left visible, 16 points clear of
//! the keyboard. A field already visible does not move; a field that is not focused does not move.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    low: Handle<SkiaEditor>,
    high: Handle<SkiaEditor>,
}

type Host = Headless<App>;

/// A 400 x 800 point page (800 x 1600 pixels), a field near its top and one near its bottom, in a
/// scroll with room below.
fn scene() -> Host {
    let build = |app: &mut App| {
        SkiaScroll::new().fill().adapt_to_keyboard_auto(true).adapt_to_keyboard_gap(16).assign(&mut app.scroll).content(SkiaLayout::column().fill_x().spacing(0).children((
            SkiaEditor::new().fill_x().height_request(44).font_size(16).assign(&mut app.high),
            SkiaLayout::new().fill_x().height_request(640),
            SkiaEditor::new().fill_x().height_request(44).font_size(16).assign(&mut app.low),
            SkiaLayout::new().fill_x().height_request(600),
        )))
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 800, 1600, 2.0);
    host.settle();
    host
}

fn focus(host: &mut Host, editor: Handle<SkiaEditor>) {
    host.ui.focus(Some(editor.id()));
    // The caret blinks: a few frames, not a settle.
    for _ in 0..4 {
        host.frame_after(16.0);
    }
}

/// Where a field is on screen, pixels: its place in the content, moved by the scroll.
fn on_screen(host: &Host, editor: Handle<SkiaEditor>) -> (f32, f32) {
    let offset = host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y() * 2.0;
    let r = host.rect(editor);
    (r.top + offset, r.bottom + offset)
}

/// The keyboard leaves the canvas visible down to `bottom` (pixels).
fn keyboard(host: &mut Host, bottom: f32) {
    host.ui.keyboard_cover(0.0, bottom);
    for _ in 0..30 {
        host.frame_after(16.0);
    }
}

#[test]
fn the_focused_field_moves_above_the_keyboard() {
    let mut host = scene();
    let low = host.ui.state.low;
    focus(&mut host, low);
    let before = on_screen(&host, low);
    assert!(before.1 > 900.0, "the field starts low: {before:?}");
    keyboard(&mut host, 900.0);
    let after = on_screen(&host, low);
    assert!((after.1 - (900.0 - 32.0)).abs() < 2.0, "16 points (32 px) above the keyboard: {after:?}");
}

#[test]
fn a_visible_field_or_no_focus_does_not_move() {
    let mut host = scene();
    let (high, low) = (host.ui.state.high, host.ui.state.low);
    let start = on_screen(&host, low);
    keyboard(&mut host, 900.0);
    assert_eq!(on_screen(&host, low), start, "nothing focused: the page stays");
    focus(&mut host, high);
    let start = on_screen(&host, high);
    keyboard(&mut host, 900.0);
    assert_eq!(on_screen(&host, high), start, "the field is above the keyboard: the page stays");
}

#[test]
fn the_text_input_area_is_where_the_field_is_drawn() {
    let mut host = scene();
    let low = host.ui.state.low;
    focus(&mut host, low);
    keyboard(&mut host, 900.0);
    let (top, bottom) = on_screen(&host, low);
    let area = host.ui.text_input().expect("open");
    assert!((area.top * 2.0 - top).abs() < 1.0 && (area.bottom * 2.0 - bottom).abs() < 1.0, "the host's text input follows the scroll: {area:?} vs {top}..{bottom} px");
}
