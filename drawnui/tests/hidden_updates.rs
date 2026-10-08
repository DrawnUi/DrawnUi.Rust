//! Hidden controls ask for no frames (drawnui-cross 6k, C# 329f6c44 HiddenUpdateTests): a live
//! control under a hidden screen changes without waking the frame loop, and shows its latest state
//! when the screen is shown again; its animators pause and go on from where they were.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// A screen with a 20 x 20 live square at (10, 10), under a cached or plain parent.
fn scene(cache: CacheType) -> (Headless<()>, ControlId, ControlId) {
    let live = SkiaLayout::new().width_request(20).height_request(20).margin(Thickness::uniform(10.0)).background_color(Color::RED);
    let live_id = live.id();
    let screen = SkiaLayout::new().fill().use_cache(cache).children(live);
    let screen_id = screen.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(screen)).background(Color::WHITE);
    let mut host = Headless::new(ui, 60, 60, 1.0);
    host.settle();
    (host, screen_id, live_id)
}

#[test]
fn a_live_control_under_a_hidden_screen_asks_for_no_frames() {
    for cache in [CacheType::None, CacheType::Image] {
        let (mut host, screen, live) = scene(cache);
        let mut asked = 0;
        // Visible: every change asks for a frame.
        for i in 0..10u8 {
            host.ui.tree.any_mut(live).unwrap().set_background_color(Color::from_rgb(i * 20, 0, 0));
            asked += host.ui.needs_frame() as u32;
            host.settle();
        }
        assert_eq!(asked, 10, "{cache:?}: visible changes ask for frames");

        host.ui.tree.any_mut(screen).unwrap().set_is_visible(false);
        host.settle();
        asked = 0;
        for i in 0..10u8 {
            host.ui.tree.any_mut(live).unwrap().set_background_color(Color::from_rgb(0, 0, 100 + i * 10));
            asked += host.ui.needs_frame() as u32;
            host.settle();
        }
        assert_eq!(asked, 0, "{cache:?}: changes under a hidden screen ask for no frames");

        // Shown again: the latest change is drawn.
        host.ui.tree.any_mut(screen).unwrap().set_is_visible(true);
        assert!(host.ui.needs_frame());
        host.settle();
        assert_eq!(host.pixel(20, 20), Color::from_rgb(0, 0, 190), "{cache:?}");
    }
}

#[test]
fn an_animation_under_a_hidden_screen_pauses_and_goes_on_when_shown() {
    let (mut host, screen, live) = scene(CacheType::None);
    let value = Rc::new(Cell::new(0.0f32));
    let seen = value.clone();
    host.ui.tree.cx().animate(live, 1000.0, easing::linear, move |v, _| seen.set(v));
    for _ in 0..5 {
        host.frame_after(40.0);
    }
    let before = value.get();
    assert!(before > 0.1 && before < 0.3, "{before}");

    host.ui.tree.any_mut(screen).unwrap().set_is_visible(false);
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame(), "a hidden animation keeps no frames coming");
    host.frame_after(5000.0);
    let paused = value.get();
    assert!((paused - before).abs() < 0.05, "it did not run while hidden: {before} -> {paused}");

    host.ui.tree.any_mut(screen).unwrap().set_is_visible(true);
    host.frame_after(16.0);
    host.frame_after(40.0);
    let resumed = value.get();
    assert!(resumed > paused && resumed < paused + 0.1, "it goes on from where it was: {paused} -> {resumed}");
}
