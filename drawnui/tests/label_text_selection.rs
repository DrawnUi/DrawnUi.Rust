//! Port of DrawnUi.Net.Tests LabelTextSelectionTests. `accessibility_text_selectable`: a mouse drag and
//! a double click select, a touch long press selects a word, the selection copies to the clipboard
//! (Ctrl+C), a click elsewhere drops it, and a label without the property ignores all of it.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{LONG_PRESS_MS, PointerKind};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const TEXT: &str = "Hello wonderful world, this text wraps onto a second line here";
const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };

fn host(selectable: bool) -> Headless<Handle<SkiaLabel>> {
    let build = move |label: &mut Handle<SkiaLabel>| {
        SkiaLayer::new().fill().children(
            SkiaLabel::new(TEXT).font_size(20).width_request(300).margin(20).accessibility_text_selectable(selectable).assign(label),
        )
    };
    let ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 420, 300, 1.0);
    host.settle();
    host
}

fn label(host: &Headless<Handle<SkiaLabel>>) -> &SkiaLabel {
    host.ui.tree.find::<SkiaLabel>(host.ui.state).expect("label")
}

/// Canvas pixel at the left edge of a character of TEXT, mid-height of its line.
fn at(host: &Headless<Handle<SkiaLabel>>, index: usize) -> (f32, f32) {
    let label = label(host);
    let mut from = 0;
    for ((value, _), line) in label.lines().zip(label.text_lines()) {
        let start = from + TEXT[from..].find(value).expect("a line of the text");
        if index <= start + value.len() {
            let slot = index - start;
            let x = if slot < line.advances.len() { line.advances[..slot].iter().sum() } else { line.rect.width() };
            return (line.rect.left + x + 1.0, line.rect.center_y());
        }
        from = start + value.len();
    }
    panic!("index {index} is past the text");
}

#[test]
fn mouse_drag_selects_the_dragged_range() {
    let mut host = host(true);
    let (from, to) = (at(&host, 6), at(&host, 15));
    host.pan(from, to, 100.0, 2);
    assert_eq!(label(&host).selected_text(), "wonderful");
}

#[test]
fn mouse_drag_across_the_wrap_copies_the_space_it_dropped() {
    let mut host = host(true);
    let second = label(&host).lines().nth(1).map(|(v, _)| TEXT.find(v).unwrap()).expect("two lines");
    let (from, to) = (at(&host, second - 6), at(&host, second + 4));
    host.pan(from, to, 100.0, 2);
    assert_eq!(label(&host).selected_text(), TEXT[second - 6..second + 4]);
}

#[test]
fn double_click_selects_the_word() {
    let mut host = host(true);
    let (x, y) = at(&host, 9);
    host.tap(x, y);
    host.tap(x, y);
    assert_eq!(label(&host).selected_text(), "wonderful");
}

#[test]
fn ctrl_c_copies_ctrl_a_selects_all() {
    let mut host = host(true);
    let (x, y) = at(&host, 2);
    host.tap(x, y);
    host.tap(x, y);
    assert!(host.key_down("KeyC", CTRL));
    assert_eq!(host.take_clipboard().as_deref(), Some("Hello"));
    assert!(host.key_down("KeyA", CTRL));
    assert_eq!(label(&host).selected_text(), TEXT);
}

#[test]
fn click_on_empty_space_drops_the_selection() {
    let mut host = host(true);
    let (x, y) = at(&host, 9);
    host.tap(x, y);
    host.tap(x, y);
    assert_eq!(label(&host).selected_text(), "wonderful");
    host.tap(400.0, 280.0);
    assert_eq!(label(&host).selected_text(), "");
}

#[test]
fn touch_long_press_selects_the_word_touch_drag_does_not() {
    let mut host = host(true);
    host.use_touch(true);
    let (from, to) = (at(&host, 6), at(&host, 15));
    host.pan(from, to, 100.0, 2);
    assert_eq!(label(&host).selected_text(), "");
    // As upstream: the long press alone, still held.
    let (x, y) = at(&host, 9);
    host.ui.pointer_touch(PointerKind::Down, x, y, host.time_ms());
    host.frame();
    host.frame_after(LONG_PRESS_MS);
    assert_eq!(label(&host).selected_text(), "wonderful");
    // Lifting the finger keeps it: a release after a long press is no tap.
    host.ui.pointer_touch(PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
    assert_eq!(label(&host).selected_text(), "wonderful");
}

#[test]
fn not_selectable_ignores_the_pointer() {
    let mut host = host(false);
    let (from, to) = (at(&host, 6), at(&host, 15));
    host.pan(from, to, 100.0, 2);
    assert_eq!(label(&host).selected_text(), "");
}

#[test]
fn the_selection_is_drawn() {
    let mut host = host(true);
    let (x, y) = at(&host, 9);
    let before = host.pixel(x as i32 + 2, y as i32 - 8);
    host.tap(x, y);
    host.tap(x, y);
    let after = host.pixel(x as i32 + 2, y as i32 - 8);
    assert_ne!(before, after, "the highlight is over the word");
    assert!(after.b() > after.r(), "drawn in the selection blue: {after:?}");
}
