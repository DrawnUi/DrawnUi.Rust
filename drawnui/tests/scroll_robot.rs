//! The scroll tests of DrawnUi.Net.Tests, same cases: GestureRobotTests, WheelTouchpadTests and
//! LoadMoreDistanceTests (with a plain stack as the content). The pan and fling numbers are the
//! React engine's (a probe on DrawnUi.React `dist` in node with this robot); the wheel is the C#
//! one, with its numbers.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    loads: u32,
}

/// GestureRobotTests.MakeScrollScene: a vertical scroll filling the canvas over a tall column.
fn scene(width: i32, height: i32, content_height: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let content = SkiaLayout::column().height_request(content_height).background_color(Color::YELLOW);
        SkiaScroll::new().fill().assign(&mut app.scroll).content(content)
    });
    let mut host = Headless::new(ui.background(Color::WHITE), width, height, 1.0);
    host.frame_after(16.0);
    host
}

fn offset_y(host: &Headless<App>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// GestureRobot.SettleFling: frames until the offset stood still for three of them.
fn settle_fling(host: &mut Headless<App>, max_frames: usize) -> usize {
    let (mut stable, mut previous) = (0, offset_y(host));
    for frame in 1..=max_frames {
        host.frame_after(16.0);
        let moved = (offset_y(host) - previous).abs();
        previous = offset_y(host);
        stable = if moved < 0.05 { stable + 1 } else { 0 };
        if stable >= 3 {
            return frame;
        }
    }
    max_frames
}

// ---------------------------------------------------------------- GestureRobotTests

#[test]
fn pan_scrolls_viewport() {
    let mut host = scene(400, 600, 3000.0);
    assert_eq!(offset_y(&host), 0.0);
    host.pan((200.0, 500.0), (200.0, 200.0), 250.0, 16);
    // Dragging the finger up scrolls the content up: the offset goes negative.
    assert!(offset_y(&host) < -100.0, "{}", offset_y(&host));
}

#[test]
fn pan_tracks_finger_one_to_one_without_fling() {
    let mut host = scene(400, 600, 3000.0);
    // A slow drag of exactly 200 px: the release velocity is small.
    host.pan((200.0, 500.0), (200.0, 300.0), 1200.0, 40);
    // Before any fling settles the offset is the drag distance.
    assert!((-210.0..=-190.0).contains(&offset_y(&host)), "{}", offset_y(&host));
    // The C# engine stands at -199.118 here: the pan is smoothed, the last move is not fully in yet.
    assert!((offset_y(&host) + 199.118).abs() < 0.001, "{}", offset_y(&host));
}

#[test]
fn fling_travels_further_than_slow_pan_and_settles() {
    let mut slow = scene(400, 600, 3000.0);
    slow.pan((200.0, 500.0), (200.0, 300.0), 1200.0, 40);
    settle_fling(&mut slow, 600);
    let slow = offset_y(&slow).abs();

    // A fast flick of the same 200 px.
    let mut fast = scene(400, 600, 3000.0);
    fast.pan((200.0, 500.0), (200.0, 300.0), 60.0, 6);
    let frames = settle_fling(&mut fast, 600);
    let travelled = offset_y(&fast).abs();
    assert!(travelled > slow + 100.0, "fling {travelled} should pass the slow pan {slow} by a margin");
    assert!(frames > 1, "a fling needs several frames to settle");
    // Where React is when the robot calls it settled: under 0.05 points a frame for three frames.
    assert!((slow - 272.01).abs() < 0.01 && (travelled - 1521.231).abs() < 0.01, "{slow} {travelled}");
    // React has no pixel-aware finish: the tail still creeps (to 1521.421 five frames later), on
    // the same pixel.
    let pixel = fast.ui.tree.base(fast.ui.state.scroll).unwrap().content_offset;
    for _ in 0..5 {
        fast.frame_after(16.0);
    }
    assert!((offset_y(&fast) + 1521.421).abs() < 0.01, "{}", offset_y(&fast));
    assert_eq!(fast.ui.tree.base(fast.ui.state.scroll).unwrap().content_offset, pixel);
}

#[test]
fn tap_does_not_scroll() {
    let mut host = scene(400, 600, 3000.0);
    host.tap(200.0, 300.0);
    settle_fling(&mut host, 30);
    assert_eq!(offset_y(&host), 0.0);
}

#[test]
fn pan_is_deterministic() {
    let run = || {
        let mut host = scene(400, 600, 3000.0);
        host.pan((200.0, 520.0), (200.0, 120.0), 200.0, 12);
        settle_fling(&mut host, 600);
        offset_y(&host)
    };
    assert_eq!(run(), run());
    // And the same place as React.
    assert!((run() + 1278.538).abs() < 0.01, "{}", run());
}

#[test]
fn wheel_scroll_moves_viewport() {
    let mut host = scene(400, 600, 3000.0);
    // Upstream sends -600 in unknown units, which counts as one notch down. Here the host
    // contract is notches already.
    assert!(host.wheel(200.0, 300.0, -1.0));
    settle_fling(&mut host, 120);
    assert!(offset_y(&host) < -1.0, "the wheel should scroll down, got {}", offset_y(&host));
}

// ---------------------------------------------------------------- WheelTouchpadTests

/// `events` wheel events of `delta` notches, `gap_ms` apart; returns how far the content went.
fn travel(host: &mut Headless<App>, delta: f32, events: u32, gap_ms: f64) -> f32 {
    let start = offset_y(host);
    for _ in 0..events {
        host.ui.wheel(150.0, 200.0, delta, 0.0);
        host.frame_after(gap_ms);
    }
    for _ in 0..120 {
        host.frame_after(16.0);
    }
    start - offset_y(host)
}

#[test]
fn touchpad_burst_scrolls_its_share_of_a_notch() {
    let mut host = scene(300, 400, 20000.0);
    // 20 events of 6 / 120 = one notch in all, the way a precision touchpad reports a short swipe.
    let travelled = travel(&mut host, -6.0 / 120.0, 20, 8.0);
    assert!((travelled - 150.0).abs() < 1.0, "{travelled}");
}

#[test]
fn mouse_notches_one_line_each_fast_spin_adds_up() {
    let mut host = scene(300, 400, 20000.0);
    assert_eq!(travel(&mut host, -1.0, 1, 16.0), 150.0);
    // Eight notches 40 ms apart: each one lands while the last step still runs.
    assert_eq!(travel(&mut host, -1.0, 8, 40.0), 8.0 * 150.0);
}

// ---------------------------------------------------------------- LoadMoreDistanceTests

/// 100 x 200 points of viewport whatever the scale, over 12 rows of 50 points: 400 points of travel.
fn load_more_scene(scale: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let rows: Vec<Build<SkiaShape>> = (0..12).map(|_| SkiaShape::new().fill_x().height_request(50).background_color(Color::BLUE)).collect();
        SkiaScroll::new()
            .fill()
            .bounces(false)
            .load_more_offset(150)
            .on_load_more(|_me, app: &mut App, _cx| app.loads += 1)
            .assign(&mut app.scroll)
            .content(SkiaLayout::column().spacing(0).children(rows))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), (100.0 * scale) as i32, (200.0 * scale) as i32, scale);
    frames(&mut host, 8);
    host
}

fn frames(host: &mut Headless<App>, count: u32) {
    for _ in 0..count {
        host.frame_after(16.0);
    }
}

fn jump_to(host: &mut Headless<App>, y: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, y, 0);
    frames(host, 10);
}

#[test]
fn load_more_fires_only_within_the_distance_in_points() {
    for scale in [1.0, 2.0, 3.0] {
        // A distance of 150 points: the zone is the last 150 points of travel, offsets below -250.
        // Multiplied by a scale of 3 it would be wider than the travel and fire at the top.
        let mut host = load_more_scene(scale);
        frames(&mut host, 10);
        assert_eq!(host.ui.state.loads, 0, "scale {scale}: at the top");

        // 100 points down: still 300 from the end.
        jump_to(&mut host, -100.0);
        assert_eq!(host.ui.state.loads, 0, "scale {scale}: at -100");

        // 300 points down: 100 from the end, inside the zone.
        jump_to(&mut host, -300.0);
        assert_eq!(host.ui.state.loads, 1, "scale {scale}: at -300");

        // As the C# engine: the same content does not fire twice, at the end or on coming back soon.
        jump_to(&mut host, -400.0);
        jump_to(&mut host, -100.0);
        jump_to(&mut host, -300.0);
        assert_eq!(host.ui.state.loads, 1, "scale {scale}: again at -300");
    }
}
