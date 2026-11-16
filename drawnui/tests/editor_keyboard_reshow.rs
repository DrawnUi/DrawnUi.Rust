//! A tap on the text field that already has the focus asks its soft keyboard to show again: Back
//! (Android) closes the keyboard but leaves the field focused, and after the app was in the
//! background the same; nothing else would bring the keyboard back. A tap elsewhere does not ask.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::PointerKind;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    field: Handle<SkiaEditor>,
}

/// A tap (down and up) and a few frames; no settle: a focused caret blinks for ever.
fn tap(host: &mut Headless<App>) {
    let t = host.time_ms();
    host.ui.pointer_touch(PointerKind::Down, 100.0, 20.0, t);
    host.ui.pointer_touch(PointerKind::Up, 100.0, 20.0, t + 40.0);
    for _ in 0..4 {
        host.frame_after(16.0);
    }
}

#[test]
fn a_tap_on_the_focused_field_asks_for_the_keyboard_again() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::column().fill().children((SkiaEditor::new().width_request(300).height_request(44).font_size(16).assign(&mut app.field),))
    })
    .font_bytes("FontText", FONT);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    // A tap beside the field: nothing to show.
    let t = host.time_ms();
    host.ui.pointer_touch(PointerKind::Down, 380.0, 250.0, t);
    host.ui.pointer_touch(PointerKind::Up, 380.0, 250.0, t + 40.0);
    host.frame_after(16.0);
    assert!(!host.ui.keyboard_reshow_requested(), "a tap elsewhere asks for no keyboard");
    tap(&mut host);
    let focused = host.ui.tree.find::<SkiaEditor>(host.ui.state.field).is_some_and(|e| e.is_focused());
    assert!(focused, "the tap focuses the field");
    // Back closed the keyboard; the field kept the focus. A tap on it:
    tap(&mut host);
    assert!(host.ui.keyboard_reshow_requested(), "a tap on the focused field asks for the keyboard again");
}
