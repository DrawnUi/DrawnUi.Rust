//! Desktop text selection as in browsers (drawnui-cross 6cc): a mouse drag that starts outside any
//! text selects from the edge of the label it enters; one drag selects across several selectable
//! labels in page order and copies them joined by new lines; a click elsewhere clears it, Esc does
//! not; touch is unchanged; inside a scroll that holds selectable text a mouse drag selects instead
//! of scrolling and scrolls on while held past the edge, every other scroll still scrolls.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::PointerKind;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const TEXTS: [&str; 3] = ["Hello wonderful world", "Second label here", "Third one at the end"];
const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };

#[derive(Default)]
struct App {
    labels: [Handle<SkiaLabel>; 3],
    scroll: Handle<SkiaScroll>,
}

fn label(text: &'static str, selectable: bool, handle: &mut Handle<SkiaLabel>) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(20).accessibility_text_selectable(selectable).assign(handle)
}

/// Three labels in a column inside a scroll over a tall filler: `selectable` makes them selectable.
fn host(selectable: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let [a, b, c] = &mut app.labels;
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().fill_x().padding(20).spacing(16).children((
            label(TEXTS[0], selectable, a),
            label(TEXTS[1], selectable, b),
            label(TEXTS[2], selectable, c),
            SkiaLayout::new().fill_x().height_request(2000),
        )))
    })
    .font_bytes("Default", FONT)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    host
}

fn selected(host: &Headless<App>, i: usize) -> String {
    host.ui.tree.find::<SkiaLabel>(host.ui.state.labels[i]).unwrap().selected_text()
}

fn all(host: &Headless<App>) -> [String; 3] {
    [selected(host, 0), selected(host, 1), selected(host, 2)]
}

fn offset(host: &Headless<App>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// Canvas pixel at the left edge of character `index` of label `i`, mid-height.
fn at(host: &Headless<App>, i: usize, index: usize) -> (f32, f32) {
    let label = host.ui.tree.find::<SkiaLabel>(host.ui.state.labels[i]).unwrap();
    let line = label.text_lines().next().unwrap();
    let x: f32 = line.advances[..index].iter().sum();
    (line.rect.left + x + 1.0, line.rect.center_y())
}

#[test]
fn a_drag_from_beside_a_label_selects_from_its_edge() {
    let mut host = host(true);
    let rect = host.rect(host.ui.state.labels[0]);
    let (_, y) = at(&host, 0, 0);
    let to = at(&host, 0, 5);
    host.pan((rect.left - 3.0, y), to, 200.0, 6);
    assert_eq!(all(&host), ["Hello".to_owned(), String::new(), String::new()]);
    assert_eq!(offset(&host), 0.0, "the drag selected, it did not scroll");
}

#[test]
fn one_drag_selects_across_labels_and_copies_them_joined() {
    let mut host = host(true);
    let first = host.rect(host.ui.state.labels[0]);
    let to = at(&host, 2, 5);
    host.pan((first.left + 10.0, first.top - 8.0), to, 200.0, 8);
    assert_eq!(all(&host), [TEXTS[0].to_owned(), TEXTS[1].to_owned(), "Third".to_owned()]);
    host.key_down("KeyC", CTRL);
    assert_eq!(host.take_clipboard().as_deref(), Some("Hello wonderful world\nSecond label here\nThird"));
    // Esc keeps it, a click elsewhere clears it.
    host.press_key("Escape");
    assert_eq!(all(&host), [TEXTS[0].to_owned(), TEXTS[1].to_owned(), "Third".to_owned()]);
    host.tap(300.0, 250.0);
    assert_eq!(all(&host), [String::new(), String::new(), String::new()]);
}

#[test]
fn a_drag_starting_on_a_label_goes_on_into_the_next() {
    let mut host = host(true);
    let (from, to) = (at(&host, 0, 6), at(&host, 1, 6));
    host.pan(from, to, 200.0, 8);
    assert_eq!(all(&host), ["wonderful world".to_owned(), "Second".to_owned(), String::new()]);
    // A new press in the second label, dragged up into the first: from there back to the press.
    let (from, to) = (at(&host, 1, 6), at(&host, 0, 16));
    host.pan(from, to, 200.0, 8);
    assert_eq!(all(&host), ["world".to_owned(), "Second".to_owned(), String::new()]);
}

#[test]
fn held_past_the_scroll_edge_the_drag_scrolls_on_and_keeps_selecting() {
    let mut host = host(true);
    let from = at(&host, 0, 0);
    host.ui.pointer(PointerKind::Down, from.0, from.1, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Move, from.0 + 20.0, 200.0, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Move, from.0 + 20.0, 320.0, host.time_ms());
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    let held = offset(&host);
    assert!(held < -20.0, "the scroll went on while the mouse was held below it: {held}");
    host.ui.pointer(PointerKind::Up, from.0 + 20.0, 320.0, host.time_ms());
    host.frame_after(16.0);
    assert_eq!(all(&host), TEXTS.map(str::to_owned), "every label up to the end is selected");
    // Not held: no more scrolling.
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    assert!((offset(&host) - held).abs() < 1.0);
}

#[test]
fn a_scroll_without_selectable_text_still_scrolls_by_mouse_drag() {
    let mut host = host(false);
    host.pan((200.0, 250.0), (200.0, 100.0), 200.0, 8);
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    assert!(offset(&host) < -50.0, "{}", offset(&host));
}

#[test]
fn touch_still_scrolls_over_selectable_text() {
    let mut host = host(true);
    host.use_touch(true);
    host.pan((200.0, 250.0), (200.0, 100.0), 200.0, 8);
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    assert!(offset(&host) < -50.0, "{}", offset(&host));
    assert_eq!(all(&host), [String::new(), String::new(), String::new()]);
}
