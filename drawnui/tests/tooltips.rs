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
    // At the control: the host places it centered above it (`place_tooltip`).
    assert_eq!(request.anchor, Rect::new(20.0, 20.0, 120.0, 60.0), "{request:?}");
    // The pointer moving over the same control leaves it where it is.
    hover(&mut host, 110.0, 55.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Save"]);
    assert_eq!(host.popup_window_requests()[0].anchor, request.anchor);
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
fn from_accessibility_turned_on_at_run_time_applies_from_the_next_tooltip() {
    let (mut host, shown) = host(false);
    host.ui.tree.cx().set_tooltips_from_accessibility(true);
    hover(&mut host, 260.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Settings"]);
    host.ui.tree.cx().set_tooltips_from_accessibility(false);
    hover(&mut host, 500.0, 250.0);
    hover(&mut host, 260.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(shown.borrow().len(), 1, "off again: the icon shows none");
}

#[test]
fn it_opens_centered_above_the_control_or_below_without_room() {
    let bounds = Rect::from_xywh(0.0, 0.0, 600.0, 300.0);
    let size = Size::new(80.0, 24.0);
    // The control's rect, top and bottom moved in by the room around the visible box.
    let control = |l: f32, t: f32, r: f32, b: f32, room: f32| Rect::new(l, t + room, r, b - room);
    // Centered, 6 points above (scale 2: 12 pixels).
    assert_eq!(drawnui::place_tooltip(control(100.0, 200.0, 200.0, 240.0, 0.0), 2.0, size, bounds), Rect::from_xywh(110.0, 164.0, 80.0, 24.0));
    // No room above: 6 points below.
    assert_eq!(drawnui::place_tooltip(control(100.0, 10.0, 200.0, 50.0, 0.0), 2.0, size, bounds), Rect::from_xywh(110.0, 62.0, 80.0, 24.0));
    // Kept inside at the right edge.
    assert_eq!(drawnui::place_tooltip(control(560.0, 200.0, 600.0, 240.0, 0.0), 1.0, size, bounds).right, 600.0);
    // A shadow room of 16 px around the visible box: the visible box keeps the same place, also
    // over a control lower than twice the room.
    let visible = |r: Rect| Rect::new(r.left + 16.0, r.top + 16.0, r.right - 16.0, r.bottom - 16.0);
    let with_room = drawnui::place_tooltip(control(100.0, 200.0, 200.0, 240.0, 16.0), 2.0, Size::new(112.0, 56.0), bounds);
    assert_eq!(visible(with_room), Rect::from_xywh(110.0, 164.0, 80.0, 24.0));
    let low = drawnui::place_tooltip(control(100.0, 200.0, 200.0, 210.0, 16.0), 2.0, Size::new(112.0, 56.0), bounds);
    assert_eq!(visible(low), Rect::from_xywh(110.0, 164.0, 80.0, 24.0));
    let below = drawnui::place_tooltip(control(100.0, 10.0, 200.0, 20.0, 16.0), 2.0, Size::new(112.0, 56.0), bounds);
    assert_eq!(visible(below), Rect::from_xywh(110.0, 32.0, 80.0, 24.0));
}

#[test]
fn on_the_canvas_it_is_drawn_centered_on_the_control() {
    let (mut host, _) = host(false);
    hover(&mut host, 30.0, 40.0);
    wait(&mut host, 700.0);
    // The test style builds a plain label; "Save" (box 20..120 at y 20..60) has no room above, so
    // it shows 6 points below, centered on the box, not at the pointer near its left edge.
    let ink: Vec<(i32, i32)> = (0..600).flat_map(|x| (60..98).map(move |y| (x, y))).filter(|(x, y)| host.pixel(*x, *y).r() > 0).collect();
    assert!(!ink.is_empty(), "drawn below the box");
    let (min, max) = (ink.iter().map(|p| p.0).min().unwrap(), ink.iter().map(|p| p.0).max().unwrap());
    assert!(((min + max) as f32 / 2.0 - 70.0).abs() <= 2.0, "centered on the box: ink {min}..{max}");
    assert!(ink.iter().all(|p| p.1 >= 66), "6 points below the box");
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

#[test]
fn a_style_set_at_run_time_applies_from_the_next_tooltip() {
    let (mut host, shown) = host(false);
    hover(&mut host, 60.0, 40.0);
    wait(&mut host, 700.0);
    assert_eq!(*shown.borrow(), ["Save"]);
    let log = shown.clone();
    let mut cx = host.ui.tree.cx();
    let mut style = cx.tooltip_style().clone();
    style.content = Some(Rc::new(move |text: &str| {
        log.borrow_mut().push(format!("themed {text}"));
        SkiaLabel::new(text.to_owned()).into()
    }));
    cx.set_tooltip_style(style);
    host.frame_after(16.0);
    assert_eq!(shown.borrow().len(), 1, "the open one keeps its look");
    hover(&mut host, 160.0, 40.0);
    assert_eq!(*shown.borrow(), ["Save", "themed Open"]);
}
