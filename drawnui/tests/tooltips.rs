//! Tooltips (drawnui-cross 6ar, DrawnUi.Net 5accc053): the innermost control's text after the mouse
//! rests 600 ms, at once when moving to the next control, closed by leaving, a press (until another
//! control), a key or the content moving; never for a finger; a passive popup that keeps no press
//! from the controls below and has no accessibility node.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{KeyKind, Modifiers, PointerKind, TooltipStyle};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    taps: u32,
}

/// Three 100 x 40 boxes in a row at y 20: "Save" and "Open" with tooltips, then an icon with an
/// accessibility label only; below them a label with a hint. `shown` collects every tooltip text.
fn host(from_accessibility: bool) -> (Headless<App>, Rc<RefCell<Vec<String>>>) {
    let shown = Rc::new(RefCell::new(Vec::new()));
    let log = shown.clone();
    let style = TooltipStyle {
        content: Some(Rc::new(move |text: &str| {
            log.borrow_mut().push(text.to_owned());
            SkiaLabel::new(text.to_owned()).into()
        })),
        ..TooltipStyle::default()
    };
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children((
            SkiaLayout::row().spacing(0).margin((20, 20, 0, 0)).children((
                SkiaShape::new().width_request(100).height_request(40).tooltip("Save").on_tapped(|_me, app: &mut App, _cx| app.taps += 1),
                SkiaShape::new().width_request(100).height_request(40).tooltip("Open"),
                SkiaShape::new().width_request(100).height_request(40).accessibility_label("Settings"),
            )),
            SkiaLabel::new("Plain text").margin((20, 100, 0, 0)).accessibility_hint("A hint"),
        ))
    })
    .font_bytes("Default", FONT)
    .tooltip_style(style)
    .tooltips_from_accessibility(from_accessibility);
    let mut host = Headless::new(ui, 600, 300, 1.0);
    host.settle();
    (host, shown)
}

fn hover(host: &mut Headless<App>, x: f32, y: f32) {
    let t = host.time_ms();
    host.ui.pointer(PointerKind::Hover, x, y, t);
    host.frame_after(16.0);
}

fn wait(host: &mut Headless<App>, ms: f64) {
    let end = host.time_ms() + ms;
    while host.time_ms() < end {
        host.frame_after(16.0);
    }
}

fn open(host: &Headless<App>) -> usize {
    host.popup_window_requests().len()
}

#[test]
fn it_shows_after_the_mouse_rests_and_follows_at_once() {
    let (mut host, shown) = host(false);
    host.popup_windows(true);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 300.0);
    assert!(shown.borrow().is_empty(), "not before 600 ms");
    wait(&mut host, 400.0);
    assert_eq!(*shown.borrow(), ["Save"]);
    let request = host.popup_window_requests()[0];
    assert!(request.passive && request.cpu, "{request:?}");
    // At the pointer: the host places it above and right of it (`place_tooltip`).
    assert_eq!((request.anchor.left, request.anchor.top), (60.0, 40.0), "{request:?}");
    // The next control at once.
    hover(&mut host, 160.0, 40.0);
    assert_eq!(*shown.borrow(), ["Save", "Open"]);
    assert_eq!(open(&host), 1);
    // Off every control: closed.
    hover(&mut host, 500.0, 250.0);
    assert_eq!(open(&host), 0);
}

#[test]
fn a_press_closes_it_until_another_control_and_still_reaches_the_control() {
    let (mut host, shown) = host(false);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(shown.borrow().len(), 1);
    host.tap(60.0, 40.0);
    assert_eq!(host.ui.state.taps, 1, "the press was not kept from the button");
    hover(&mut host, 62.0, 41.0);
    wait(&mut host, 1000.0);
    assert_eq!(shown.borrow().len(), 1, "no tooltip again over the same control");
    hover(&mut host, 160.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Save", "Open"]);
}

#[test]
fn a_key_and_leaving_close_it_and_a_finger_never_shows_one() {
    let (mut host, shown) = host(false);
    host.popup_windows(true);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(open(&host), 1);
    host.ui.key(KeyKind::Down, "a", "", Modifiers::default(), false);
    host.frame_after(16.0);
    assert_eq!(open(&host), 0, "a key closes it");
    hover(&mut host, 160.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(open(&host), 1);
    let t = host.time_ms();
    host.ui.pointer(PointerKind::Leave, 160.0, 40.0, t);
    host.frame_after(16.0);
    assert_eq!(open(&host), 0, "leaving the canvas closes it");
    let before = shown.borrow().len();
    let t = host.time_ms();
    host.ui.pointer_touch(PointerKind::Down, 60.0, 40.0, t);
    host.ui.pointer_touch(PointerKind::Up, 60.0, 40.0, t + 50.0);
    wait(&mut host, 1000.0);
    assert_eq!(shown.borrow().len(), before, "a finger shows none");
}

#[test]
fn it_has_no_accessibility_node() {
    let (mut host, _) = host(false);
    host.ui.set_accessibility_enabled(true);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 1200.0);
    assert!(host.ui.accessibility_nodes().iter().all(|n| n.label != "Save"));
}

#[test]
fn from_accessibility_uses_the_hint_else_the_label_of_a_control_without_text() {
    let (mut off, shown) = host(false);
    hover(&mut off, 260.0, 40.0);
    wait(&mut off, 700.0);
    assert!(shown.borrow().is_empty(), "off by default");

    let (mut host, shown) = host(true);
    hover(&mut host, 260.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Settings"], "the icon's label");
    hover(&mut host, 40.0, 108.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Settings", "A hint"], "a hint, also on a control with text");
}

#[test]
fn it_opens_above_and_right_of_the_pointer_or_below_without_room() {
    let bounds = Rect::from_xywh(0.0, 0.0, 600.0, 300.0);
    let size = Size::new(80.0, 24.0);
    let at = |x: f32, y: f32, room: f32| Rect::new(x - room, y - room, x + room, y + room);
    // Bottom-left 12 right and 6 above the pointer (scale 2: 24 and 12 pixels).
    assert_eq!(drawnui::place_tooltip(at(100.0, 200.0, 0.0), 2.0, size, bounds), Rect::from_xywh(124.0, 164.0, 80.0, 24.0));
    // No room above: 20 points below.
    assert_eq!(drawnui::place_tooltip(at(100.0, 10.0, 0.0), 2.0, size, bounds), Rect::from_xywh(124.0, 50.0, 80.0, 24.0));
    // Kept inside at the right edge.
    assert_eq!(drawnui::place_tooltip(at(590.0, 200.0, 0.0), 1.0, size, bounds).right, 600.0);
    // A shadow room of 16 px around the visible box: the visible box keeps the same place.
    let with_room = drawnui::place_tooltip(at(100.0, 200.0, 16.0), 2.0, Size::new(112.0, 56.0), bounds);
    assert_eq!(Rect::new(with_room.left + 16.0, with_room.top + 16.0, with_room.right - 16.0, with_room.bottom - 16.0), Rect::from_xywh(124.0, 164.0, 80.0, 24.0));
}

#[test]
fn on_the_canvas_it_is_drawn_above_and_right_of_the_pointer() {
    let (mut host, _) = host(false);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 700.0);
    // The default bubble is not used here (the test style builds a plain label): the label's ink
    // must be above the pointer's line and right of it.
    let ink = (0..600).flat_map(|x| (0..40).map(move |y| (x, y))).filter(|(x, y)| *x > 72 && host.pixel(*x, *y).r() > 0).count();
    assert!(ink > 0, "drawn above-right of the pointer");
}

#[test]
fn the_default_bubble_wraps_long_text_at_the_max_width() {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children((SkiaShape::new()
            .width_request(100)
            .height_request(40)
            .margin((20, 20, 0, 0))
            .tooltip("A long tooltip text that is much wider than the bubble may be, so it has to wrap over several lines"),))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 900, 400, 1.0);
    host.settle();
    host.popup_windows(true);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 700.0);
    let request = host.popup_window_requests()[0];
    // 320 max width + 12 + 12 padding + 1 + 1 border + 8 + 8 shadow room.
    assert!(request.size.width <= 320.0 + 24.0 + 2.0 + 16.0 + 0.5, "{request:?}");
    assert!(request.size.height > 2.0 * 13.0 + 16.0 + 16.0, "wraps over more than one line: {request:?}");
    let style = TooltipStyle::default();
    assert_eq!((style.background_color, style.text_color, style.border_color, style.shadow_color), (
        Color::from_rgb(0x2B, 0x2D, 0x31),
        Color::from_rgb(0xF2, 0xF3, 0xF5),
        Color::from_argb(0x24, 0xFF, 0xFF, 0xFF),
        Color::from_argb(0x59, 0, 0, 0)
    ));
}
