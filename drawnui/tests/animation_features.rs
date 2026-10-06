//! Animator features of React `Animators.ts` / C# AnimatorBase that the React demo pages use:
//! the MAUI easing set, start delay, ping-pong repeats, Pause / Resume, ActionOnTickAnimator.
//! The test moves the frame clock.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    badge: Handle<SkiaShape>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        SkiaLayout::new().children(SkiaShape::new().width_request(40).height_request(40).assign(&mut app.badge))
    });
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host
}

fn x(host: &Headless<App>) -> f32 {
    host.ui.tree.base(host.ui.state.badge).unwrap().p.translation_x
}

/// Starts `animator` on the badge, writing its value to `translation_x`.
fn run(host: &mut Headless<App>, animator: ValueAnimator) -> AnimationId {
    let badge = host.ui.state.badge;
    host.ui.tree.cx().start_animator(badge, animator, move |v, cx| {
        if let Some(mut badge) = cx.any_mut(badge) {
            badge.set_translation_x(v);
        }
    })
}

fn assert_idle(host: &mut Headless<App>) {
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None);
}

#[test]
fn the_maui_easings_have_their_numbers() {
    // At 0.25, 0.5 and 0.75, from the MAUI Easing formulas (React Easing.ts has the first seven).
    #[rustfmt::skip]
    let cases = [
        (easing::linear as Easing, [0.250000, 0.500000, 0.750000]),
        (easing::cubic_in as Easing, [0.015625, 0.125000, 0.421875]),
        (easing::cubic_out as Easing, [0.578125, 0.875000, 0.984375]),
        (easing::cubic_in_out as Easing, [0.062500, 0.500000, 0.937500]),
        (easing::sin_in as Easing, [0.076120, 0.292893, 0.617317]),
        (easing::sin_out as Easing, [0.382683, 0.707107, 0.923880]),
        (easing::sin_in_out as Easing, [0.146447, 0.500000, 0.853553]),
        (easing::bounce_out as Easing, [0.472656, 0.765625, 0.972656]),
        (easing::bounce_in as Easing, [0.027344, 0.234375, 0.527344]),
        (easing::spring_in as Easing, [-0.064137, -0.087698, 0.182590]),
        (easing::spring_out as Easing, [0.817410, 1.087697, 1.064137]),
        (easing::default as Easing, [0.062500, 0.500000, 0.937500]),
    ];
    for (i, (ease, values)) in cases.into_iter().enumerate() {
        assert!(ease(0.0).abs() < 1e-6 && (ease(1.0) - 1.0).abs() < 1e-6, "easing {i} ends");
        for (t, v) in [0.25, 0.5, 0.75].into_iter().zip(values) {
            assert!((ease(t) - v).abs() < 1e-5, "easing {i} at {t}: {} != {v}", ease(t));
        }
    }
}

#[test]
fn easing_shapes_the_value_and_an_overshoot_shows() {
    // React applies the easing to the value; a spring goes past the end and comes back.
    let mut host = host();
    run(&mut host, ValueAnimator::new(0.0, 100.0, 100.0, easing::spring_out));
    host.frame();
    host.frame_after(50.0);
    assert!((x(&host) - 108.7697).abs() < 1e-3);
    host.frame_after(50.0);
    assert_eq!(x(&host), 100.0);
    assert_idle(&mut host);
}

#[test]
fn a_delay_waits_without_frames() {
    let mut host = host();
    let start = host.time_ms();
    run(&mut host, ValueAnimator::new(0.0, 100.0, 100.0, easing::linear).delay(200.0));
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(start + 200.0));
    host.frame_after(100.0);
    assert_eq!(x(&host), 0.0);
    host.frame_after(100.0);
    // The first tick starts the run's clock.
    assert_eq!(x(&host), 0.0);
    host.frame_after(50.0);
    assert_eq!(x(&host), 50.0);
    host.frame_after(50.0);
    assert_eq!(x(&host), 100.0);
    assert_idle(&mut host);
}

#[test]
fn ping_pong_runs_every_repeat_the_other_way() {
    let mut host = host();
    run(&mut host, ValueAnimator::new(0.0, 100.0, 100.0, easing::linear).repeat(1).ping_pong());
    host.frame();
    host.frame_after(50.0);
    assert_eq!(x(&host), 50.0);
    host.frame_after(50.0);
    assert_eq!(x(&host), 100.0);
    host.frame_after(16.0);
    assert_eq!(x(&host), 100.0);
    host.frame_after(25.0);
    assert_eq!(x(&host), 75.0);
    host.frame_after(75.0);
    assert_eq!(x(&host), 0.0);
    assert_idle(&mut host);
}

#[test]
fn pause_freezes_the_run_and_resume_goes_on_from_there() {
    let mut host = host();
    let id = run(&mut host, ValueAnimator::new(0.0, 100.0, 100.0, easing::linear));
    host.frame();
    host.frame_after(40.0);
    assert_eq!(x(&host), 40.0);
    host.ui.tree.cx().pause_animation(id);
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None);
    host.frame_after(500.0);
    assert_eq!(x(&host), 40.0);

    host.ui.tree.cx().resume_animation(id);
    assert!(host.ui.needs_frame());
    host.frame_after(30.0);
    assert_eq!(x(&host), 70.0);
    host.frame_after(30.0);
    assert_eq!(x(&host), 100.0);
    assert_idle(&mut host);
}

#[test]
fn a_pause_during_the_delay_keeps_what_is_left_of_it() {
    let mut host = host();
    let id = run(&mut host, ValueAnimator::new(0.0, 100.0, 100.0, easing::linear).delay(100.0));
    host.frame_after(60.0);
    host.ui.tree.cx().pause_animation(id);
    host.frame_after(1000.0);
    assert_eq!(host.ui.wake_at(), None);
    let resumed = host.time_ms();
    host.ui.tree.cx().resume_animation(id);
    // 40 ms of the delay were left.
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(resumed + 40.0));
    host.frame_after(40.0);
    host.frame_after(50.0);
    assert_eq!(x(&host), 50.0);
}

#[test]
fn action_on_tick_runs_every_frame_until_stopped() {
    let mut host = host();
    let badge = host.ui.state.badge;
    let seen = Rc::new(Cell::new((0u32, 0.0f64)));
    let log = seen.clone();
    let id = host.ui.tree.cx().action_on_tick(badge, move |time, _cx| log.set((log.get().0 + 1, time)));
    for _ in 0..5 {
        host.frame_after(16.0);
        assert!(host.ui.needs_frame());
    }
    assert_eq!(seen.get(), (5, host.time_ms()));
    host.ui.tree.cx().stop_animation(id);
    assert_idle(&mut host);
    assert_eq!(seen.get().0, 5);
}
