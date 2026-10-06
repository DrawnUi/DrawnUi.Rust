//! The engine end to end without a window: build, layout, observers, gestures, caches, pixels.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const CRIMSON: Color = Color::from_argb(255, 0xDC, 0x14, 0x3C);

#[derive(Default)]
struct App {
    count: i32,
    title: Handle<SkiaLabel>,
    button: Handle<SkiaButton>,
    badge: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        SkiaLabel::new("")
            .font_size(24)
            .text_color(Color::WHITE)
            .assign(&mut app.title)
            .observe(|me, app: &App| me.set_text(format!("Count {}", app.count))),
        SkiaButton::new("Tap me").assign(&mut app.button).on_tapped(|_me, app: &mut App, _cx| app.count += 1),
        SkiaShape::new()
            .shape_type(ShapeType::Circle)
            .width_request(40)
            .height_request(40)
            .background_color(Color::GREEN)
            .use_cache(CacheType::Image)
            .assign(&mut app.badge),
    ))
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    host
}

fn title(host: &Headless<App>) -> String {
    host.ui.tree.find::<SkiaLabel>(host.ui.state.title).unwrap().p.text.clone()
}

#[test]
fn layout_follows_the_drawnui_contract() {
    let mut host = host();
    assert_eq!(title(&host), "Count 0");

    let label = host.rect(host.ui.state.title);
    let button = host.rect(host.ui.state.button);
    let badge = host.rect(host.ui.state.badge);
    // Column with padding 24 and spacing 16: children start at the padding, stacked.
    assert_eq!((label.left, label.top), (24.0, 24.0));
    assert_eq!(button.left, 24.0);
    assert_eq!(button.top, label.bottom + 16.0);
    // The button's style floor: 100 x 41 points where the app set no size.
    assert_eq!((button.width(), button.height()), (100.0, 41.0));
    assert_eq!((badge.width(), badge.height()), (40.0, 40.0));
    assert_eq!(badge.top, button.bottom + 16.0);

    // Pixels: the button frame, the circle (center filled, corner of its box empty).
    assert_eq!(host.pixel(button.left as i32 + 50, button.top as i32 + 3), CRIMSON);
    assert_eq!(host.pixel(badge.center_x() as i32, badge.center_y() as i32), Color::GREEN);
    assert_eq!(host.pixel(badge.left as i32 + 1, badge.top as i32 + 1), Color::BLACK);
}

#[test]
fn tap_runs_the_handler_and_observers_follow_the_state() {
    let mut host = host();
    let button = host.rect(host.ui.state.button);
    let label_before = host.rect(host.ui.state.title);

    host.tap(button.center_x(), button.center_y());
    assert_eq!(host.ui.state.count, 1);
    assert_eq!(title(&host), "Count 1");

    // A tap outside every control changes nothing.
    host.tap(390.0, 290.0);
    assert_eq!(host.ui.state.count, 1);

    // A press that turns into a pan is not a tap.
    host.pan((button.center_x(), button.center_y()), (button.center_x() + 80.0, button.center_y()), 64.0, 4);
    assert_eq!(host.ui.state.count, 1);

    for _ in 0..9 {
        host.tap(button.center_x(), button.center_y());
    }
    assert_eq!(title(&host), "Count 10");
    // The longer text was measured again and the cached label re-recorded.
    assert!(host.rect(host.ui.state.title).width() > label_before.width());
}

#[test]
fn runtime_changes_relayout_and_stale_handles_return_none() {
    let mut host = host();
    let (button, badge) = (host.ui.state.button, host.ui.state.badge);
    let before = host.rect(badge);

    // Growing the button pushes the badge down.
    host.ui.tree.get_mut(button).unwrap().set_height_request(80);
    host.settle();
    assert_eq!(host.rect(button).height(), 80.0);
    assert_eq!(host.rect(badge).top, before.top + 39.0);

    // Left/Top on a cached control moves the blit, not the layout.
    host.ui.tree.get_mut(badge).unwrap().set_left(100);
    host.settle();
    let rect = host.rect(badge);
    assert_eq!(rect.left, before.left);
    assert_eq!(host.pixel(rect.center_x() as i32 + 100, rect.center_y() as i32), Color::GREEN);
    assert_eq!(host.pixel(rect.center_x() as i32, rect.center_y() as i32), Color::BLACK);

    // A typed lookup walks the base parts; a removed control leaves a stale handle.
    assert!(host.ui.tree.find::<SkiaLayout>(button).is_some());
    assert!(host.ui.tree.find::<SkiaLabel>(button).is_none());
    host.ui.tree.remove(badge);
    host.settle();
    assert!(host.ui.tree.get_mut(badge).is_none());
    assert_eq!(host.pixel(rect.center_x() as i32 + 100, rect.center_y() as i32), Color::BLACK);
}

#[test]
fn press_and_release_inside_one_frame_still_taps() {
    let mut host = host();
    let button = host.rect(host.ui.state.button);
    for expected in 1..=3 {
        host.ui.pointer(drawnui::PointerKind::Down, button.center_x(), button.center_y(), 0.0);
        host.ui.pointer(drawnui::PointerKind::Up, button.center_x(), button.center_y(), 0.0);
        host.settle();
        assert_eq!(host.ui.state.count, expected);
    }
}

#[test]
fn nothing_is_laid_out_until_registered_fonts_arrive() {
    use drawnui::App as _;
    let ui = Ui::new(App::default(), build).font("Default", "fonts/missing-so-far.ttf").background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.frame();
    // No layout yet: the button has no rect and a click goes nowhere.
    assert!(host.rect(host.ui.state.button).is_empty());
    host.ui.pointer(drawnui::PointerKind::Down, 60.0, 80.0, 0.0);
    host.ui.pointer(drawnui::PointerKind::Up, 60.0, 80.0, 0.0);
    host.frame();
    assert_eq!(host.ui.state.count, 0);

    host.ui.asset(0, FONT.to_vec());
    host.settle();
    let button = host.rect(host.ui.state.button);
    assert_eq!((button.width(), button.height()), (100.0, 41.0));
    host.tap(button.center_x(), button.center_y());
    assert_eq!(host.ui.state.count, 1);
}

#[test]
fn a_transparent_button_paints_no_frame() {
    // Only a button without a background takes the style's crimson; transparent is a background.
    let build = |_: &mut ()| SkiaLayout::column().children(SkiaButton::new("Back").background_color(Color::TRANSPARENT));
    let mut host = Headless::new(Ui::new((), build).font_bytes("Default", FONT).background(Color::BLACK), 200, 100, 1.0);
    host.settle();
    assert_eq!(host.pixel(50, 3), Color::BLACK);
}
