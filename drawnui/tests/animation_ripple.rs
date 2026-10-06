//! The ripple: an overlay effect above the button, from the press point, clipped to its frame.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const CRIMSON: Color = Color::from_argb(255, 0xDC, 0x14, 0x3C);

#[derive(Default)]
struct App {
    button: Handle<SkiaButton>,
    plain: Handle<SkiaButton>,
    dark: Handle<SkiaButton>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        // No caption, so every pixel inside the frame is the fill.
        SkiaButton::new("").assign(&mut app.button),
        SkiaButton::new("").apply_effect(SkiaTouchAnimation::None).assign(&mut app.plain),
        SkiaButton::new("").touch_effect_color(Color::BLACK).assign(&mut app.dark),
    ))
}

fn host(scale: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, (400.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    host
}

#[test]
fn a_press_plays_a_ripple_clipped_to_the_frame_and_it_ends() {
    let mut host = host(1.0);
    let r = host.rect(host.ui.state.button);
    let (x, y) = (r.center_x() as i32, r.center_y() as i32);
    assert_eq!(host.pixel(x, y), CRIMSON);

    host.ui.pointer(PointerKind::Down, r.center_x(), r.center_y(), 0.0);
    host.frame();
    // 400 of 500 ms: the circle (radius 300 pt x 0.8^3) is larger than the button.
    host.frame_after(400.0);
    assert!(host.ui.needs_frame());
    for (px, py) in [(x, y), (r.left as i32 + 10, y), (r.right as i32 - 10, y)] {
        let pixel = host.pixel(px, py);
        // White over the fill: every channel goes up, the most on the darkest one.
        assert!(pixel.g() > CRIMSON.g() + 10 && pixel.r() > CRIMSON.r(), "{pixel:?}");
    }
    // Clipped to the rounded frame: nothing in the cut corner, nothing outside the button.
    assert_eq!(host.pixel(r.left as i32 + 1, r.top as i32 + 1), Color::BLACK);
    assert_eq!(host.pixel(x, r.top as i32 - 2), Color::BLACK);
    assert_eq!(host.pixel(r.right as i32 + 2, y), Color::BLACK);

    // Past its duration the effect is gone, pressed or not.
    host.frame_after(100.0);
    assert_eq!(host.pixel(x, y), CRIMSON);
    assert_eq!(host.pixel(r.left as i32 + 10, y), CRIMSON);
    host.ui.pointer(PointerKind::Up, r.center_x(), r.center_y(), 0.0);
    host.settle();
    assert_eq!(host.pixel(x, y), CRIMSON);
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
}

#[test]
fn the_ripple_starts_at_the_press_point_at_any_scale() {
    let mut host = host(2.0);
    let r = host.rect(host.ui.state.button);
    let (x, y) = (r.left + 50.0, r.top + 30.0);
    host.ui.pointer(PointerKind::Down, x, y, 0.0);
    host.frame();
    // 100 of 500 ms: radius 300 pt x 0.2^3 = 2.4 pt, 4.8 pixels.
    host.frame_after(100.0);
    assert_ne!(host.pixel(x as i32, y as i32), CRIMSON);
    assert_ne!(host.pixel(x as i32 + 3, y as i32), CRIMSON);
    assert_eq!(host.pixel(x as i32 + 8, y as i32), CRIMSON);
    assert_eq!(host.pixel(x as i32, y as i32 - 8), CRIMSON);
}

#[test]
fn apply_effect_and_touch_effect_color_are_followed() {
    let mut host = host(1.0);
    let plain = host.rect(host.ui.state.plain);
    host.ui.pointer(PointerKind::Down, plain.center_x(), plain.center_y(), 0.0);
    host.frame();
    host.frame_after(16.0);
    // No effect: the press asks for no more frames and the pixels stay.
    assert!(!host.ui.needs_frame());
    assert_eq!(host.pixel(plain.center_x() as i32, plain.center_y() as i32), CRIMSON);
    host.ui.pointer(PointerKind::Up, plain.center_x(), plain.center_y(), 0.0);
    host.settle();

    let dark = host.rect(host.ui.state.dark);
    host.ui.pointer(PointerKind::Down, dark.center_x(), dark.center_y(), 0.0);
    host.frame();
    host.frame_after(250.0);
    let pixel = host.pixel(dark.center_x() as i32, dark.center_y() as i32);
    assert!(pixel.r() < CRIMSON.r() && pixel.g() < CRIMSON.g(), "{pixel:?}");
}
