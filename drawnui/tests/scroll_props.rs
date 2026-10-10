//! SkiaScroll's physics as DrawnUI properties: ChangeDistancePanned, MaxVelocity,
//! RubberEffect, ScrollingSpeedMs, and ViewportOffsetX / ViewportOffsetY set from code.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

fn scene(setup: impl FnOnce(Build<SkiaScroll>) -> Build<SkiaScroll>) -> (Headless<()>, Handle<SkiaScroll>) {
    let mut scroll = Handle::default();
    let content = SkiaLayout::new().fill_x().height_request(3000).background_color(Color::BLUE);
    let built = setup(SkiaScroll::new().fill().content(content)).assign(&mut scroll);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(built));
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    (host, scroll)
}

fn offset_y(host: &Headless<()>, scroll: Handle<SkiaScroll>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y()
}

/// Down at `from`, moves of `step` points, still held.
fn drag(host: &mut Headless<()>, from: f32, step: f32, steps: u32) {
    host.ui.pointer(PointerKind::Down, 100.0, from, host.time_ms());
    host.frame_after(16.0);
    for i in 1..=steps {
        host.ui.pointer(PointerKind::Move, 100.0, from + step * i as f32, host.time_ms());
        host.frame_after(16.0);
    }
}

#[test]
fn change_distance_panned_multiplies_the_pan() {
    let moved = |factor: f32| {
        let (mut host, scroll) = scene(|s| s.change_distance_panned(factor));
        drag(&mut host, 250.0, -10.0, 10);
        offset_y(&host, scroll)
    };
    let (one, two) = (moved(1.0), moved(2.0));
    assert!(one < -50.0 && (two / one - 2.0).abs() < 0.1, "1x {one}, 2x {two}");
}

#[test]
fn rubber_effect_sets_how_far_the_content_pulls_past_its_start() {
    let pulled = |effect: f32| {
        let (mut host, scroll) = scene(|s| s.rubber_effect(effect));
        drag(&mut host, 50.0, 10.0, 15);
        offset_y(&host, scroll)
    };
    let (soft, stiff) = (pulled(2.0), pulled(0.2));
    assert!(soft > stiff && stiff > 0.0, "2.0: {soft}, 0.2: {stiff}");
}

#[test]
fn max_velocity_limits_a_fling() {
    let flung = |max: f32| {
        let (mut host, scroll) = scene(|s| s.max_velocity(max));
        host.fling((100.0, 280.0), (100.0, 80.0), 80.0, 5);
        host.settle();
        offset_y(&host, scroll)
    };
    let (fast, slow) = (flung(3000.0), flung(300.0));
    assert!(fast < slow && slow < 0.0, "3000: {fast}, 300: {slow}");
}

#[test]
fn scrolling_speed_ms_is_the_duration_of_a_wheel_step() {
    let after = |ms: f32| {
        let (mut host, scroll) = scene(|s| s.scrolling_speed_ms(ms));
        host.wheel(100.0, 150.0, -1.0);
        host.frame_after(16.0);
        host.frame_after(120.0);
        host.frame_after(16.0);
        offset_y(&host, scroll)
    };
    let (quick, slow) = (after(100.0), after(1000.0));
    assert!(quick < slow, "a 100 ms step is further along at 150 ms: {quick} vs {slow}");
}

#[test]
fn viewport_offset_set_from_code_jumps_inside_the_content() {
    let (mut host, scroll) = scene(|s| s);
    host.ui.tree.cx().set_viewport_offset_y(scroll, -500.0);
    host.settle();
    assert_eq!(offset_y(&host, scroll), -500.0);
    host.ui.tree.cx().set_viewport_offset_y(scroll, -99999.0);
    host.settle();
    assert_eq!(offset_y(&host, scroll), -2700.0, "kept inside: 3000 - 300");
    host.ui.tree.cx().set_viewport_offset_x(scroll, -50.0);
    host.settle();
    assert_eq!(offset_y(&host, scroll), -2700.0, "the other axis stays");
}
