//! The soft keyboard and a scroll that follows it (drawnui-cross 6cj, opt-in): with
//! `adapt_to_keyboard_auto` the focused field's scroll moves it `adapt_to_keyboard_gap` above the
//! keyboard, and while the keyboard is up the scroll gets the part of its viewport the keyboard
//! covers (plus the gap) as room at its content end, so even the LAST field rises and a touch does
//! not bounce it back. When the keyboard closes, the room goes and a scroll past the new end
//! settles at the end, animated. Without the setting nothing moves (.NET ScrollKeyboardRoomTests).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    last: Handle<SkiaEditor>,
}

type Host = Headless<App>;

/// A 400 x 800 point page (800 x 1600 pixels) whose content is 1000 points: a filler, then the
/// last field at the very end.
fn scene(auto: bool) -> Host {
    let build = move |app: &mut App| {
        SkiaScroll::new()
            .fill()
            .adapt_to_keyboard_auto(auto)
            .adapt_to_keyboard_gap(16)
            .assign(&mut app.scroll)
            .content(SkiaLayout::column().fill_x().spacing(0).children((
                SkiaLayout::new().fill_x().height_request(956),
                SkiaEditor::new().fill_x().height_request(44).font_size(16).assign(&mut app.last),
            )))
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 800, 1600, 2.0);
    host.settle();
    host
}

fn frames(host: &mut Host, n: usize) {
    for _ in 0..n {
        host.frame_after(16.0);
    }
}

fn offset(host: &Host) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// The last field's bottom on screen, pixels.
fn field_bottom(host: &Host) -> f32 {
    host.rect(host.ui.state.last).bottom + offset(host) * 2.0
}

/// At the end with the field focused, then the keyboard leaves the top 1000 of 1600 pixels.
fn focused_at_end_keyboard_up(auto: bool) -> Host {
    let mut host = scene(auto);
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().set_viewport_offset_y(scroll, -10_000.0);
    frames(&mut host, 4);
    let last = host.ui.state.last.id();
    host.ui.focus(Some(last));
    frames(&mut host, 4);
    host.ui.keyboard_cover(0.0, 1000.0);
    frames(&mut host, 40);
    host
}

#[test]
fn the_last_field_rises_above_the_keyboard_and_stays_after_a_tap() {
    let mut host = focused_at_end_keyboard_up(true);
    let bottom = field_bottom(&host);
    assert!((bottom - (1000.0 - 32.0)).abs() < 2.0, "16 points (32 px) above the keyboard: {bottom}");
    // A tap on the page: the scroll stays where it is (no bounce back past its end).
    host.tap(400.0, 300.0);
    frames(&mut host, 60);
    let after = field_bottom(&host);
    assert!((after - bottom).abs() < 2.0, "a tap keeps the field above the keyboard: {bottom} -> {after}");
}

#[test]
fn when_the_keyboard_closes_the_scroll_settles_at_its_end_animated() {
    let mut host = focused_at_end_keyboard_up(true);
    let up = offset(&host);
    // The keyboard closes: the canvas is all visible again.
    host.ui.keyboard_cover(0.0, 1600.0);
    host.frame_after(16.0);
    let moving = offset(&host);
    assert!(moving < up + 300.0 - 1.0 || moving > up, "starts toward the end");
    frames(&mut host, 60);
    let end = -(1000.0 - 800.0);
    assert!((offset(&host) - end).abs() < 1.0, "settled at the content end: {} vs {end}", offset(&host));
    assert!((moving - end).abs() > 1.0, "animated, not a jump: first frame at {moving}");
}

#[test]
fn without_the_setting_nothing_moves() {
    let host = focused_at_end_keyboard_up(false);
    assert!((offset(&host) - (-200.0)).abs() < 1.0, "the page stays at its end: {}", offset(&host));
}

#[test]
fn between_two_fields_the_room_stays() {
    let mut host = focused_at_end_keyboard_up(true);
    let up = offset(&host);
    // The field gives up the focus while the keyboard stays up (the next field takes it after).
    host.ui.focus(None::<ControlId>);
    host.ui.keyboard_cover(0.0, 1000.0);
    frames(&mut host, 60);
    assert!((offset(&host) - up).abs() < 1.0, "the page stays risen: {up} -> {}", offset(&host));
}
