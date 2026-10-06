//! Value and property animators on the headless host: the test moves the frame clock.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    done: Vec<&'static str>,
    title: Handle<SkiaLabel>,
    button: Handle<SkiaButton>,
    badge: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        SkiaLabel::new("")
            .text_color(Color::WHITE)
            .assign(&mut app.title)
            .observe(|me, app: &App| me.set_text(format!("Done {}", app.done.len()))),
        SkiaButton::new("Fade").assign(&mut app.button).on_tapped(|_me, app: &mut App, cx| {
            let fade = cx.fade_to(app.badge, 0.5, 100, easing::linear);
            cx.on_finished(fade, |app: &mut App, _cx| app.done.push("tap"));
        }),
        SkiaShape::new()
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

fn props(host: &Headless<App>) -> drawnui::ControlProps {
    host.ui.tree.base(host.ui.state.badge).unwrap().p.clone()
}

/// Nothing is left to do: another frame changes nothing and asks for no more.
fn assert_idle(host: &mut Headless<App>) {
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
}

#[test]
fn a_fade_is_linear_at_the_midpoint_and_exact_at_the_duration() {
    let mut host = host();
    let badge = host.ui.state.badge;
    host.ui.tree.cx().fade_to(badge, 0.0, 200, easing::linear);
    assert!(host.ui.needs_frame());

    // The first tick only starts the clock of the animator.
    host.frame();
    assert_eq!(props(&host).opacity, 1.0);
    host.frame_after(100.0);
    assert_eq!(props(&host).opacity, 0.5);
    assert!(host.ui.needs_frame());
    host.frame_after(99.0);
    assert!(props(&host).opacity > 0.0);
    host.frame_after(1.0);
    assert_eq!(props(&host).opacity, 0.0);
    assert_idle(&mut host);
}

#[test]
fn easing_shapes_the_value_between_the_ends() {
    let mut host = host();
    let badge = host.ui.state.badge;
    // An end value that `from + (to - from) * 1.0` would miss by a rounding step.
    host.ui.tree.cx().fade_to(badge, 0.3, 200, easing::cubic_in);
    host.ui.tree.cx().rotate_to(badge, 90, 200, easing::cubic_out);
    host.frame();
    host.frame_after(100.0);
    // cubic_in(0.5) = 0.125, cubic_out(0.5) = 0.875.
    assert_eq!(props(&host).opacity, 1.0 - 0.7 * 0.125);
    assert_eq!(props(&host).rotation, 90.0 * 0.875);
    host.frame_after(100.0);
    assert_eq!(props(&host).opacity, 0.3);
    assert_eq!(props(&host).rotation, 90.0);
    assert_idle(&mut host);

    for (ease, middle) in [
        (easing::linear as Easing, 0.5),
        (easing::cubic_in, 0.125),
        (easing::cubic_out, 0.875),
        (easing::cubic_in_out, 0.5),
        (easing::default, 0.5),
        (easing::sin_in, 1.0 - std::f32::consts::FRAC_PI_4.cos()),
        (easing::sin_out, std::f32::consts::FRAC_PI_4.sin()),
    ] {
        assert_eq!(ease(0.0), 0.0);
        assert!((ease(0.5) - middle).abs() < 1e-6);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
    }
}

#[test]
fn scale_translate_and_rotate_run_together_from_the_current_values() {
    let mut host = host();
    let badge = host.ui.state.badge;
    host.ui.tree.get_mut(badge).unwrap().set_translation_x(10);
    let mut cx = host.ui.tree.cx();
    cx.scale_to(badge, 2, 3, 100, easing::linear);
    cx.translate_to(badge, 30, 40, 100, easing::linear);
    cx.rotate_to(badge, -90, 100, easing::linear);
    host.frame();
    host.frame_after(50.0);
    let p = props(&host);
    assert_eq!((p.scale_x, p.scale_y), (1.5, 2.0));
    assert_eq!((p.translation_x, p.translation_y), (20.0, 20.0));
    assert_eq!(p.rotation, -45.0);
    host.frame_after(50.0);
    let p = props(&host);
    assert_eq!((p.scale_x, p.scale_y, p.translation_x, p.translation_y, p.rotation), (2.0, 3.0, 30.0, 40.0, -90.0));
    assert_idle(&mut host);
}

#[test]
fn a_new_animation_of_the_same_kind_cancels_the_old_one() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let mut cx = host.ui.tree.cx();
    let first = cx.fade_to(badge, 0.0, 200, easing::linear);
    cx.on_finished(first, |app: &mut App, _cx| app.done.push("first"));
    host.frame();
    host.frame_after(100.0);
    assert_eq!(props(&host).opacity, 0.5);

    // The new fade starts from where the old one stopped.
    let mut cx = host.ui.tree.cx();
    let second = cx.fade_to(badge, 1.0, 100, easing::linear);
    cx.on_finished(second, |app: &mut App, _cx| app.done.push("second"));
    host.frame();
    assert_eq!(props(&host).opacity, 0.5);
    host.frame_after(50.0);
    assert_eq!(props(&host).opacity, 0.75);
    host.frame_after(50.0);
    assert_eq!(props(&host).opacity, 1.0);
    // Past the end of the first one: it never finished and nothing moved the opacity again.
    host.frame_after(200.0);
    assert_eq!(props(&host).opacity, 1.0);
    assert_eq!(host.ui.state.done, ["second"]);
    assert_idle(&mut host);
}

#[test]
fn a_stopped_animation_stays_where_it_is_and_does_not_report_completion() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let mut cx = host.ui.tree.cx();
    let fade = cx.fade_to(badge, 0.0, 200, easing::linear);
    cx.on_finished(fade, |app: &mut App, _cx| app.done.push("stopped"));
    host.frame();
    host.frame_after(50.0);
    host.ui.tree.cx().stop_animation(fade);
    host.frame_after(500.0);
    assert_eq!(props(&host).opacity, 0.75);
    assert!(host.ui.state.done.is_empty());
    assert_idle(&mut host);
}

#[test]
fn a_forever_animator_keeps_frames_coming_until_its_control_is_removed() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let spin = ValueAnimator::new(0.0, 360.0, 100.0, easing::linear).repeat(-1);
    host.ui.tree.cx().start_animator(badge, spin, move |value, cx| {
        if let Some(mut badge) = cx.any_mut(badge) {
            badge.set_rotation(value);
        }
    });
    host.frame();
    let mut seen = Vec::new();
    for _ in 0..12 {
        host.frame_after(25.0);
        assert!(host.ui.needs_frame());
        seen.push(props(&host).rotation);
    }
    // Each run reports its end, then starts again on the next frame.
    assert_eq!(seen[..6], [90.0, 180.0, 270.0, 360.0, 0.0, 90.0]);

    host.ui.tree.remove(badge);
    // `settle` panics when frames never stop.
    host.settle();
    assert_idle(&mut host);
}

#[test]
fn a_counted_repeat_runs_that_many_more_times_then_finishes() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let mut cx = host.ui.tree.cx();
    let twice = cx.start_animator(badge, ValueAnimator::new(0.0, 1.0, 100.0, easing::linear).repeat(1), |_, _| {});
    cx.on_finished(twice, |app: &mut App, _cx| app.done.push("twice"));
    host.frame();
    host.frame_after(100.0);
    assert!(host.ui.state.done.is_empty());
    host.frame_after(16.0);
    host.frame_after(99.0);
    assert!(host.ui.state.done.is_empty());
    host.frame_after(1.0);
    assert_eq!(host.ui.state.done, ["twice"]);
    assert_idle(&mut host);
}

#[test]
fn the_completion_callback_runs_once_with_the_app_state() {
    let mut host = host();
    let button = host.rect(host.ui.state.button);
    host.ui.pointer(drawnui::PointerKind::Down, button.center_x(), button.center_y(), 0.0);
    host.frame();
    host.ui.pointer(drawnui::PointerKind::Up, button.center_x(), button.center_y(), 0.0);
    // The handler starts the fade in this frame.
    host.frame();
    host.frame_after(99.0);
    assert!(host.ui.state.done.is_empty());
    assert!(props(&host).opacity > 0.5);

    host.frame_after(1.0);
    assert_eq!(props(&host).opacity, 0.5);
    assert_eq!(host.ui.state.done, ["tap"]);
    // Observers follow the state the callback changed, in the same frame.
    assert_eq!(host.ui.tree.find::<SkiaLabel>(host.ui.state.title).unwrap().p.text, "Done 1");

    host.settle();
    assert_eq!(host.ui.state.done, ["tap"]);
    assert_idle(&mut host);
}

#[test]
fn opacity_and_transform_animation_do_not_record_a_cache_again() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let recorded = host.cache_records(badge);
    assert_eq!(recorded, 1);
    let center = host.rect(badge).center();

    let mut cx = host.ui.tree.cx();
    cx.fade_to(badge, 0.5, 100, easing::linear);
    cx.rotate_to(badge, 45, 100, easing::linear);
    cx.scale_to(badge, 2, 2, 100, easing::linear);
    cx.translate_to(badge, 100, 0, 100, easing::linear);
    host.frame();
    for _ in 0..10 {
        host.frame_after(10.0);
    }
    assert_eq!(props(&host).opacity, 0.5);
    // The animated pixels are on screen: half-transparent green over black, 100 points to the right.
    let pixel = host.pixel(center.x as i32 + 100, center.y as i32);
    assert!((126..=129).contains(&pixel.g()) && pixel.r() == 0, "{pixel:?}");
    assert_eq!(host.cache_records(badge), recorded);

    // The counter does count: a change of the control's own pixels records again.
    host.ui.tree.get_mut(badge).unwrap().set_background_color(Color::RED);
    host.settle();
    assert_eq!(host.cache_records(badge), recorded + 1);
}
