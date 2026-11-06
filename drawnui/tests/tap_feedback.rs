//! Tap and press animations play for a finger, a pen or the left mouse button only (DrawnUI
//! `IsPrimaryPointer`): a right click that opens a context menu shows no ripple. Its events stay.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{MouseButton, PointerKind};

const BLUE: Color = Color::from_rgb(30, 60, 200);

#[derive(Default)]
struct App {
    taps: u32,
    menus: u32,
}

/// The Agent Signals row: a shape with a ripple, a tap handler and a context menu.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children((SkiaShape::new()
            .width_request(200)
            .height_request(100)
            .background_color(BLUE)
            .animation_tapped(SkiaTouchAnimation::Ripple)
            .touch_effect_color(Color::WHITE)
            .on_tapped(|_me, app: &mut App, _cx| app.taps += 1)
            .on_context_menu(|_me, app: &mut App, _cx, _at| {
                app.menus += 1;
                true
            }),))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 200, 1.0);
    host.settle();
    host
}

/// A press and release with `button` at the shape's center, then 120 ms of the animation.
fn click(host: &mut Headless<App>, button: MouseButton) -> Color {
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Down, button, 100.0, 50.0, t);
    host.frame_after(16.0);
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Up, button, 100.0, 50.0, t);
    if button == MouseButton::Right {
        let t = host.time_ms();
        host.ui.context_menu(100.0, 50.0, drawnui::ContextMenuSource::Mouse, t);
    }
    for _ in 0..8 {
        host.frame_after(16.0);
    }
    host.pixel(100, 50)
}

#[test]
fn a_right_click_plays_no_ripple_and_keeps_its_events() {
    let mut host = host();
    assert_eq!(click(&mut host, MouseButton::Right), BLUE, "no ripple for the right button");
    assert_eq!(host.ui.state.menus, 1, "the context menu still opened");
    host.settle();
    assert_ne!(click(&mut host, MouseButton::Left), BLUE, "the left button ripples");
    assert!(host.ui.state.taps >= 1);
}
