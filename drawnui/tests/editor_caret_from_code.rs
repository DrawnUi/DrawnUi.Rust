//! A caret placed by code right after the text is set (one step: set_text, set_cursor_position,
//! focus) lands in the new text, not in the old one; `Cx::editor_cursor_at` gives the caret a tap at
//! a point would place.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
}

fn scene(text: &'static str) -> Headless<App> {
    let build = move |app: &mut App| {
        SkiaLayout::new().fill().children(SkiaEditor::new().width_request(300).max_lines(3).font_size(18).text(text).assign(&mut app.editor))
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 800, 600, 2.0);
    host.settle();
    host
}

/// A focused editor blinks: a few frames instead of `settle`.
fn frames(host: &mut Headless<App>) {
    for _ in 0..4 {
        host.frame_after(16.0);
    }
}

fn editor(host: &Headless<App>) -> &SkiaEditor {
    host.ui.tree.find(host.ui.state.editor).unwrap()
}

/// set_text, then the caret, then the focus, in one step (as a reused edit field opening).
fn open_with(host: &mut Headless<App>, text: &str, caret: usize) {
    let handle = host.ui.state.editor;
    let mut cx = host.ui.tree.cx();
    let mut me = cx.get_mut(handle).unwrap();
    me.set_text(text.to_owned());
    me.set_cursor_position(caret);
    drop(me);
    cx.focus(Some(handle));
    frames(host);
}

#[test]
fn the_caret_set_after_the_text_goes_to_the_end_of_the_new_text() {
    // From an empty field (the first edit) and from a shorter text (a field reused).
    for old in ["", "abc"] {
        let mut host = scene(old);
        open_with(&mut host, "HorizontalOptions", usize::MAX);
        assert_eq!(editor(&host).cursor_position(), "HorizontalOptions".chars().count(), "from {old:?}");
    }
    let mut host = scene("");
    open_with(&mut host, "Center", 3);
    assert_eq!(editor(&host).cursor_position(), 3);
}

#[test]
fn select_all_after_the_text_selects_the_new_text() {
    let mut host = scene("old");
    let handle = host.ui.state.editor;
    let mut cx = host.ui.tree.cx();
    let mut me = cx.get_mut(handle).unwrap();
    me.set_text("a longer new text".to_owned());
    me.select_all();
    drop(me);
    host.settle();
    assert_eq!(editor(&host).selected_text(), "a longer new text");
}

#[test]
fn editor_cursor_at_is_where_a_tap_puts_the_caret() {
    let mut host = scene("Hello wonderful world");
    let r = host.rect(host.ui.state.editor);
    // Canvas points (scale 2): a third into the field, on its first line.
    let point = Point::new((r.left + r.width() / 3.0) / 2.0, (r.top + 30.0) / 2.0);
    let handle = host.ui.state.editor;
    let at = host.ui.tree.cx().editor_cursor_at(handle, point).expect("laid out");
    // A tap without `settle` (the focused caret blinks).
    let now = host.time_ms();
    host.ui.pointer(drawnui::PointerKind::Down, point.x * 2.0, point.y * 2.0, now);
    host.ui.pointer(drawnui::PointerKind::Up, point.x * 2.0, point.y * 2.0, now);
    frames(&mut host);
    assert_eq!(editor(&host).cursor_position(), at);
    assert!(at > 0 && at < 21, "{at}");
}
