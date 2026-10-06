//! Pull to refresh of SkiaScroll, with the numbers of a probe on the React engine (DrawnUi.React
//! `dist` in node: its Canvas frame loop and pointer handling copied, the robot of `Headless`, 16 ms
//! frames) over the scene of the ScrollPage card: a 300 x 220 white scroll over 12 blue rows of 40,
//! a green indicator 50 tall, RefreshDistanceLimit 60.
//!
//! React: the pull stretches on the plain rubber band (over the viewport); past the limit the
//! refresh starts, once per pull; released, the content springs back to `refresh_show_distance`
//! and a pan still moves it; the indicator's far edge follows the pull up to that distance, its
//! opacity the pull as a share of it. When the app ends the refresh the content goes back over
//! 600 ms on the ScrollTo curve. (React runs that curve through the rubber band, the content
//! jumping from 50 to 24.4 in the first frame; not ported.)

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const GREEN: Color = Color::from_rgb(0, 128, 0);

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    indicator: Handle<SkiaShape>,
    refreshes: u32,
}

type Host = Headless<App>;

fn scene(limit: f32, handler: bool, scale: f32) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let rows: Vec<Build<SkiaShape>> = (0..12).map(|_| SkiaShape::new().fill_x().height_request(40).background_color(Color::BLUE)).collect();
        let indicator = SkiaShape::new().fill_x().height_request(50).background_color(GREEN).assign(&mut app.indicator);
        let mut scroll = SkiaScroll::new().fill_x().height_request(220).background_color(Color::WHITE);
        scroll = scroll.refresh_enabled(true).refresh_distance_limit(limit).refresh_indicator(indicator);
        if handler {
            scroll = scroll.on_refresh(|_me, app: &mut App, _cx| app.refreshes += 1);
        }
        let scroll = scroll.assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).fill_x().children(rows));
        SkiaLayout::new().fill().children((scroll,))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), (300.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    host
}

fn scroll(host: &Host) -> &SkiaScroll {
    host.ui.tree.find(host.ui.state.scroll).unwrap()
}

fn y(host: &Host) -> f32 {
    scroll(host).viewport_offset_y()
}

/// Where the top of the indicator stands in the viewport, points, and its opacity.
fn indicator(host: &Host) -> (f32, f32) {
    let base = host.ui.tree.base(host.ui.state.indicator).unwrap();
    (base.p.translation_y, base.p.opacity)
}

fn refreshing(host: &Host) -> (bool, u32) {
    (scroll(host).p.is_refreshing, host.ui.state.refreshes)
}

fn set_refreshing(host: &mut Host, on: bool) {
    let id = host.ui.state.scroll;
    host.ui.tree.get_mut(id).unwrap().set_is_refreshing(on);
}

fn pointer(host: &mut Host, kind: PointerKind, x: f32, y: f32) {
    let time = host.time_ms();
    host.ui.pointer(kind, x, y, time);
    host.frame_after(16.0);
}

/// Presses at (150, from) points and pulls down 10 points a move, as the probe. The offset after
/// every move.
fn pull(host: &mut Host, from: f32, moves: usize, scale: f32) -> Vec<f32> {
    pointer(host, PointerKind::Down, 150.0 * scale, from * scale);
    (1..=moves)
        .map(|step| {
            pointer(host, PointerKind::Move, 150.0 * scale, (from + step as f32 * 10.0) * scale);
            y(host)
        })
        .collect()
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.01
}

fn assert_all(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(close(*actual, *expected), "{i}: {actual} instead of {expected}");
    }
}

/// The React pull: 10 points a move on the rubber band of a 220 point viewport.
const PULLED: [f32; 16] = [4.578, 9.612, 14.508, 19.195, 23.675, 27.959, 32.06, 35.99, 39.759, 43.377, 46.852, 50.193, 53.408, 56.503, 59.485, 62.36];

/// The colors down the column x = 150 points as runs: G the indicator, B rows, W the scroll,
/// K the canvas, ? a blend.
fn column(host: &mut Host, scale: f32) -> String {
    let name = |c: Color| match (c.r(), c.g(), c.b()) {
        (r, g, b) if g > 100 && r < 80 && b < 80 => 'G',
        (r, g, b) if b > 200 && r < 80 && g < 80 => 'B',
        (r, g, b) if r > 200 && g > 200 && b > 200 => 'W',
        (r, g, b) if r < 40 && g < 40 && b < 40 => 'K',
        _ => '?',
    };
    let height = (300.0 * scale) as i32;
    let mut runs = Vec::new();
    let (mut current, mut start) = (' ', 0);
    for y in 0..=height {
        let color = if y < height { name(host.pixel((150.0 * scale) as i32, y)) } else { ' ' };
        if color != current {
            if current != ' ' {
                runs.push(format!("{current}{start}-{y}"));
            }
            (current, start) = (color, y);
        }
    }
    runs.join(" ")
}

#[test]
fn a_pull_shows_the_indicator_and_past_the_limit_starts_a_refresh() {
    let mut host = scene(60.0, true, 1.0);
    assert_eq!((indicator(&host).1, column(&mut host, 1.0)), (0.0, "B0-220 K220-300".to_owned()));
    let pulled = pull(&mut host, 50.0, 13, 1.0);
    assert_all(&pulled, &PULLED[..13]);
    assert_eq!(refreshing(&host), (false, 0));

    // The indicator's bottom follows the pull, its opacity is the pull over 50 points.
    let mut host = scene(60.0, true, 1.0);
    pull(&mut host, 50.0, 2, 1.0);
    let (top, opacity) = indicator(&host);
    assert!(close(top, -40.388) && close(opacity, 0.192), "{top} {opacity}");
    for step in 3..=10 {
        pointer(&mut host, PointerKind::Move, 150.0, 50.0 + step as f32 * 10.0);
    }
    let (top, opacity) = indicator(&host);
    assert!(close(top, -6.623) && close(opacity, 0.868), "{top} {opacity}");
    for step in 11..=12 {
        pointer(&mut host, PointerKind::Move, 150.0, 50.0 + step as f32 * 10.0);
    }
    // All there at 50: it stays at the start of the viewport, the content pulls on below it.
    assert!(close(y(&host), 50.193));
    assert_eq!((indicator(&host), column(&mut host, 1.0)), ((0.0, 1.0), "G0-50 B50-220 K220-300".to_owned()));
    for step in 13..=15 {
        pointer(&mut host, PointerKind::Move, 150.0, 50.0 + step as f32 * 10.0);
    }
    assert_eq!(refreshing(&host), (false, 0));

    // Past 60: the refresh starts, once.
    pointer(&mut host, PointerKind::Move, 150.0, 210.0);
    assert!(close(y(&host), 62.36), "{}", y(&host));
    assert_eq!(refreshing(&host), (true, 1));
    // Released: back to 50, where it stays.
    pointer(&mut host, PointerKind::Up, 150.0, 210.0);
    host.settle();
    assert_eq!((y(&host), refreshing(&host), indicator(&host)), (50.0, (true, 1), (0.0, 1.0)));
    assert_eq!(column(&mut host, 1.0), "G0-50 B50-220 K220-300");
    // A pan moves it while it refreshes (React; C# holds it).
    host.fling((150.0, 150.0), (150.0, 50.0), 320.0, 8);
    assert!(close(y(&host), -185.604), "{}", y(&host));

    // Done: the content goes back to its start, the indicator leaves.
    set_refreshing(&mut host, false);
    host.settle();
    assert_eq!((y(&host), indicator(&host).1, column(&mut host, 1.0)), (0.0, 0.0, "B0-220 K220-300".to_owned()));
    // The next pull refreshes again.
    let pulled = pull(&mut host, 50.0, 16, 1.0);
    assert!(close(pulled[15], 62.36));
    assert_eq!(refreshing(&host), (true, 2));
}

#[test]
fn a_pull_let_go_before_the_limit_springs_back() {
    let mut host = scene(60.0, true, 1.0);
    let pulled = pull(&mut host, 50.0, 5, 1.0);
    assert!(close(pulled[4], 23.675));
    assert!(close(indicator(&host).1, 0.473));
    pointer(&mut host, PointerKind::Up, 150.0, 100.0);
    // The spring of the probe, from the frame after the release.
    let mut trace = Vec::new();
    for _ in 0..12 {
        host.frame_after(16.0);
        trace.push(y(&host));
    }
    assert_all(&trace, &[28.788, 29.678, 28.1, 25.229, 21.823, 18.352, 15.088, 12.17, 9.654, 7.544, 5.814, 4.42]);
    host.settle();
    assert_eq!((y(&host), indicator(&host), refreshing(&host)), (0.0, (-50.0, 0.0), (false, 0)));
}

#[test]
fn the_app_starts_and_ends_a_refresh() {
    let mut host = scene(60.0, true, 1.0);
    set_refreshing(&mut host, true);
    host.settle();
    // The content makes room: 50 (`refresh_show_distance`), the indicator in it.
    assert_eq!((y(&host), refreshing(&host), indicator(&host)), (50.0, (true, 1), (0.0, 1.0)));
    assert_eq!(column(&mut host, 1.0), "G0-50 B50-220 K220-300");

    // Ended by the app: 600 ms back to the start on the ScrollTo curve (React's curve and time),
    // from the frame that takes the change; the indicator fades on the way.
    set_refreshing(&mut host, false);
    let mut trace = Vec::new();
    for _ in 0..11 {
        host.frame_after(16.0);
        trace.push(y(&host));
    }
    assert_all(&trace, &[50.0, 47.19, 44.512, 41.96, 39.527, 37.208, 34.999, 32.893, 30.886, 28.973, 27.15]);
    let (top, opacity) = indicator(&host);
    assert!(close(top, 27.15 - 50.0) && close(opacity, 27.15 / 50.0), "{top} {opacity}");
    host.settle();
    assert_eq!((y(&host), indicator(&host).1, refreshing(&host)), (0.0, 0.0, (false, 1)));
    assert!(!host.ui.needs_frame());
}

#[test]
fn without_a_handler_or_below_the_limit_a_pull_refreshes_nothing() {
    // No `on_refresh`: the pull goes on along the rubber band.
    let mut host = scene(60.0, false, 1.0);
    let pulled = pull(&mut host, 50.0, 16, 1.0);
    assert!(close(pulled[15], 62.36), "{}", pulled[15]);
    pointer(&mut host, PointerKind::Up, 150.0, 210.0);
    host.settle();
    assert_eq!((y(&host), refreshing(&host)), (0.0, (false, 0)));

    // The default limit of 150: 270 points of pull give 88.31.
    let mut host = scene(150.0, true, 1.0);
    let pulled = pull(&mut host, 20.0, 27, 1.0);
    let every_third: Vec<f32> = pulled.iter().skip(2).step_by(3).copied().collect();
    assert_all(&every_third, &[14.508, 27.959, 39.759, 50.193, 59.485, 67.813, 75.319, 82.12, 88.31]);
    assert_eq!(refreshing(&host), (false, 0));
}

#[test]
fn a_pull_refreshes_at_scale_2() {
    let mut host = scene(60.0, true, 2.0);
    let pulled = pull(&mut host, 50.0, 16, 2.0);
    assert_all(&pulled, &PULLED);
    assert_eq!(refreshing(&host), (true, 1));
    pointer(&mut host, PointerKind::Up, 300.0, 420.0);
    host.settle();
    assert_eq!(y(&host), 50.0);
    assert_eq!(column(&mut host, 2.0), "G0-100 B100-440 K440-600");
}

#[test]
fn content_that_changes_during_a_refresh_stays_pulled() {
    let mut host = scene(60.0, true, 1.0);
    pull(&mut host, 50.0, 16, 1.0);
    pointer(&mut host, PointerKind::Up, 150.0, 210.0);
    host.settle();
    // The refresh brings other content: one more row.
    let content = host.ui.tree.children(host.ui.state.scroll)[1];
    host.ui.tree.add_child(content, SkiaShape::new().fill_x().height_request(40).background_color(Color::BLUE));
    host.settle();
    assert_eq!(y(&host), 50.0);
    set_refreshing(&mut host, false);
    host.settle();
    assert_eq!(y(&host), 0.0);
}

#[test]
fn a_scroll_that_does_not_refresh_has_the_same_rubber_band() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let content = SkiaShape::new().fill_x().height_request(480).background_color(Color::BLUE);
        let scroll = SkiaScroll::new().fill_x().height_request(220).assign(&mut app.scroll).content(content);
        SkiaLayout::new().fill().children((scroll,))
    });
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    let pulled = pull(&mut host, 50.0, 2, 1.0);
    assert_all(&pulled, &PULLED[..2]);
}

#[test]
fn a_pull_that_is_still_held_when_the_refresh_ends_starts_another_one() {
    let mut host = scene(60.0, true, 1.0);
    pull(&mut host, 10.0, 16, 1.0);
    assert_eq!(refreshing(&host), (true, 1));
    // Done while the finger is still down and pulls on: React starts the next refresh at once.
    set_refreshing(&mut host, false);
    host.frame_after(16.0);
    for step in 17..=20 {
        pointer(&mut host, PointerKind::Move, 150.0, 10.0 + step as f32 * 10.0);
    }
    assert!(scroll(&host).is_user_panning());
    assert_eq!(refreshing(&host), (true, 2));
    pointer(&mut host, PointerKind::Up, 150.0, 210.0);
    host.settle();
    assert_eq!(y(&host), 50.0);
}
