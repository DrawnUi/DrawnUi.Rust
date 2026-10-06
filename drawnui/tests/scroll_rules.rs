//! SkiaScroll rules with the numbers of a probe on the React engine (DrawnUi.React `dist` in node:
//! its Canvas frame loop and pointer handling copied, the robot of `Headless`, same scenes, 16 ms
//! frames): where the React engine and the C# one differ, the React rules. And the rules of this
//! port: what a scroll frame touches, who gets a gesture, when the handlers run.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const COLORS: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    inner: Handle<SkiaScroll>,
    content: Handle<SkiaLayout>,
    tapped: Vec<usize>,
    scrolled: Vec<f32>,
    loads: u32,
    /// Rows `on_load_more` adds per call.
    grow_by: usize,
    measures: Arc<AtomicU32>,
    arranges: Arc<AtomicU32>,
}

type Host = Headless<App>;

fn host_of<B: Into<drawnui::Detached>>(width: f32, height: f32, scale: f32, build: impl FnOnce(&mut App) -> B) -> Host {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, (width * scale) as i32, (height * scale) as i32, scale);
    host.frame_after(16.0);
    host
}

/// The scene of the probe: a 400 x 600 point vertical scroll over a column `content_height` tall.
fn scene(content_height: f32, scale: f32, bounces: bool) -> Host {
    host_of(400.0, 600.0, scale, |app| {
        let content = SkiaLayout::column().height_request(content_height).background_color(Color::YELLOW);
        SkiaScroll::new().fill().bounces(bounces).assign(&mut app.scroll).content(content)
    })
}

fn scroll(host: &Host) -> &SkiaScroll {
    host.ui.tree.find(host.ui.state.scroll).unwrap()
}

fn y(host: &Host) -> f32 {
    scroll(host).viewport_offset_y()
}

/// Pixels the content is drawn moved by.
fn pixels(host: &Host) -> Point {
    host.ui.tree.base(host.ui.state.scroll).unwrap().content_offset
}

fn scroll_to(host: &mut Host, y: f32, ms: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, y, ms);
}

/// Frames until nothing moves, 16 ms apart: the offset after every frame, up to its last change.
fn run(host: &mut Host) -> Vec<f32> {
    let mut trace = Vec::new();
    while host.ui.needs_frame() && trace.len() < 900 {
        host.frame_after(16.0);
        trace.push(y(host));
    }
    while trace.len() > 1 && trace[trace.len() - 1] == trace[trace.len() - 2] {
        trace.pop();
    }
    trace
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.002
}

fn assert_trace(trace: &[f32], expected: &[(usize, f32)]) {
    for &(frame, value) in expected {
        assert!(close(trace[frame], value), "frame {frame}: {} instead of {value}", trace[frame]);
    }
}

/// The flick of the probe: 200 points up in 60 ms.
fn flick(host: &mut Host, scale: f32) {
    host.pan((200.0 * scale, 500.0 * scale), (200.0 * scale, 300.0 * scale), 60.0, 6);
}

// ---------------------------------------------------------------- physics against the C# engine

#[test]
fn a_fling_travels_and_lasts_as_in_react() {
    let mut host = scene(3000.0, 1.0, true);
    flick(&mut host, 1.0);
    // The pan is smoothed: the content is 5.88 points behind the finger at the release.
    assert!(close(y(&host), -194.118));
    assert!(scroll(&host).is_animating() && !scroll(&host).is_user_panning());
    let trace = run(&mut host);
    assert_trace(&trace, &[(0, -256.448), (1, -315.852), (2, -372.468), (5, -526.866), (10, -739.508), (20, -1038.203)]);
    assert_trace(&trace, &[(40, -1337.1), (60, -1451.38), (80, -1495.074), (100, -1511.78), (120, -1518.168), (160, -1521.544)]);
    // The curve runs out after 163 frames, between pixels; the content is drawn on the nearest one.
    assert!(trace.len() == 163 && close(y(&host), -1521.597), "{} {}", trace.len(), y(&host));
    assert_eq!(pixels(&host), Point::new(0.0, -1522.0));

    // A longer, slower drag: 400 points in 200 ms.
    let mut host = scene(3000.0, 1.0, true);
    host.pan((200.0, 520.0), (200.0, 120.0), 200.0, 12);
    assert!(close(y(&host), -394.118));
    let trace = run(&mut host);
    assert!(trace.len() == 154 && close(y(&host), -1278.914), "{} {}", trace.len(), y(&host));
}

#[test]
fn friction_and_velocity_factor_shape_the_fling() {
    for (friction, change, rest, frames) in [(0.1, 1.33, -4180.373, 483), (0.9, 1.33, -635.319, 56), (0.3, 1.0, -1192.09, 157)] {
        let mut host = scene(30000.0, 1.0, true);
        let mut scroll = host.ui.tree.get_mut(host.ui.state.scroll).unwrap();
        scroll.set_friction_scrolled(friction);
        scroll.set_change_velocity_scrolled(change);
        flick(&mut host, 1.0);
        let length = run(&mut host).len();
        assert!(length == frames && close(y(&host), rest), "friction {friction}, velocity factor {change}: {length} {}", y(&host));
    }
}

#[test]
fn a_fling_is_the_same_curve_at_every_scale() {
    // React has no pixel-aware finish (C# eases the tail onto a pixel below scale 1.5): scale 2
    // runs the same 163 frames to the same place, drawn on the nearest of its pixels.
    let mut host = scene(3000.0, 2.0, true);
    flick(&mut host, 2.0);
    let trace = run(&mut host);
    assert_trace(&trace, &[(0, -256.448), (10, -739.508), (100, -1511.78), (160, -1521.544)]);
    assert!(trace.len() == 163 && close(y(&host), -1521.597), "{} {}", trace.len(), y(&host));
    assert_eq!(pixels(&host), Point::new(0.0, -3043.0));
}

#[test]
fn a_fling_into_the_end_is_cut_at_the_edge_and_bounces() {
    // 1000 points of content in 600: the travel is 400, the flick would go to -1521.
    let mut host = scene(1000.0, 1.0, true);
    flick(&mut host, 1.0);
    let trace = run(&mut host);
    // Frame 3 lands on the edge; the spring starts there with what is left of the speed, one frame
    // later (an animator's first frame is its start), out to -410.105 and back.
    let expected = [
        -256.448, -315.852, -372.468, -400.0, -400.0, -406.023, -409.028, -410.105, -410.009, -409.251, -408.17, -406.981, -405.814,
        -404.742, -403.798, -402.994, -402.326, -401.783, -401.348, -401.006, -400.741, -400.538, -400.0,
    ];
    assert_eq!(trace.len(), expected.len(), "{trace:?}");
    assert_trace(&trace, &expected.iter().copied().enumerate().collect::<Vec<_>>());
    assert!(scroll(&host).overscroll_distance().is_zero());
}

#[test]
fn without_bounces_the_offset_stops_dead_at_both_edges() {
    let mut host = scene(1000.0, 1.0, false);
    flick(&mut host, 1.0);
    let trace = run(&mut host);
    assert_trace(&trace, &[(0, -256.448), (1, -315.852), (2, -372.468), (3, -400.0)]);
    assert_eq!((trace.len(), y(&host)), (4, -400.0));

    // Pulling down at the start moves nothing.
    let mut host = scene(3000.0, 1.0, false);
    host.pan((200.0, 100.0), (200.0, 300.0), 160.0, 10);
    assert_eq!(y(&host), 0.0);
    assert!(run(&mut host).iter().all(|y| *y == 0.0));

    // Code is clamped like the finger.
    scroll_to(&mut host, -5000.0, 0.0);
    host.settle();
    assert_eq!(y(&host), -2400.0);
    scroll_to(&mut host, 300.0, 0.0);
    host.settle();
    assert_eq!(y(&host), 0.0);
}

#[test]
fn the_rubber_band_holds_a_pull_past_the_start_and_the_spring_brings_it_back() {
    let mut host = scene(3000.0, 1.0, true);
    // Ten moves of 20 points down at the top: the content follows less and less, the band
    // stretching over the 600 points of the viewport (React; C# over 100 points).
    let pulled = [9.207, 19.451, 29.536, 39.305, 48.748, 57.879, 66.713, 75.264, 83.544, 91.568];
    host.ui.pointer(PointerKind::Down, 200.0, 100.0, 0.0);
    host.frame_after(16.0);
    for (i, expected) in pulled.iter().enumerate() {
        let step = (i + 1) as f32;
        host.ui.pointer(PointerKind::Move, 200.0, 100.0 + step * 20.0, step as f64 * 16.0);
        host.frame_after(16.0);
        assert!(close(y(&host), *expected), "move {i}: {}", y(&host));
    }
    assert!(close(scroll(&host).overscroll_distance().y, 91.568));
    assert!(scroll(&host).is_user_panning());

    host.ui.pointer(PointerKind::Up, 200.0, 300.0, 176.0);
    host.frame_after(16.0);
    assert!(close(y(&host), 91.568));
    // The release was still moving down: the spring first gives way, then comes back to the edge.
    let trace = run(&mut host);
    assert_trace(&trace, &[(0, 94.074), (1, 88.896), (2, 79.706), (3, 68.878), (5, 47.552), (10, 13.899), (20, 0.345)]);
    assert_eq!((trace.len(), y(&host)), (24, 0.0));
    assert_eq!(pixels(&host), Point::default());
}

#[test]
fn scroll_to_animates_clamps_and_jumps() {
    let mut host = host_of(400.0, 600.0, 1.0, |app| {
        let content = SkiaLayout::column().height_request(3000).background_color(Color::YELLOW);
        SkiaScroll::new()
            .fill()
            .on_scrolled(|_me, app: &mut App, _cx, offset| app.scrolled.push(offset.y))
            .assign(&mut app.scroll)
            .content(content)
    });
    // React ScrollTo: the deceleration curve that lands on the target after the time (DrawnUI
    // DecelerationTimingParameters' second constructor); its first frame is its start.
    scroll_to(&mut host, -500.0, 400.0);
    let expected = [0.0, -33.556, -65.537, -96.018, -125.067, -152.753, -179.14, -204.288, -228.256, -251.099, -272.87];
    for value in expected {
        host.frame_after(16.0);
        assert!(close(y(&host), value), "{} instead of {value}", y(&host));
    }
    host.settle();
    assert_eq!(y(&host), -500.0);
    // One report per frame that moved.
    let moved = host.ui.state.scrolled.len();
    assert!((24..=26).contains(&moved), "{moved}");
    assert_eq!(host.ui.state.scrolled.last(), Some(&-500.0));

    // A jump lands at once, inside the content, and is reported too (React).
    scroll_to(&mut host, -5000.0, 0.0);
    assert_eq!(y(&host), -2400.0);
    host.settle();
    assert_eq!(pixels(&host), Point::new(0.0, -2400.0));
    assert_eq!((host.ui.state.scrolled.len(), host.ui.state.scrolled.last()), (moved + 1, Some(&-2400.0)));

    // A pan is reported move by move.
    host.pan((200.0, 300.0), (200.0, 400.0), 1200.0, 4);
    assert_eq!(host.ui.state.scrolled.len(), moved + 5);
    assert!(close(host.ui.state.scrolled[moved + 4], -2304.41));
}

#[test]
fn a_horizontal_scroll_flings_the_same_and_ignores_a_vertical_pan() {
    let mut host = host_of(400.0, 600.0, 1.0, |app| {
        let content = SkiaLayout::row().width_request(3000).fill_y().background_color(Color::YELLOW);
        SkiaScroll::new().orientation(ScrollOrientation::Horizontal).fill().assign(&mut app.scroll).content(content)
    });
    host.pan((300.0, 300.0), (100.0, 300.0), 60.0, 6);
    assert!(close(scroll(&host).viewport_offset_x(), -194.118));
    host.settle();
    assert!(close(scroll(&host).viewport_offset_x(), -1521.597));
    assert_eq!(pixels(&host), Point::new(-1522.0, 0.0));

    host.pan((300.0, 500.0), (300.0, 300.0), 60.0, 6);
    host.settle();
    assert!(close(scroll(&host).viewport_offset_x(), -1521.597));
    assert_eq!(y(&host), 0.0);
}

// ---------------------------------------------------------------- the wheel

#[test]
fn a_wheel_notch_is_150_points_on_the_csharp_step() {
    // The wheel is the C# one (RangeAnimator, SpringOut, 400 ms; React's counts only the sign of a
    // delta and runs away when steps pile up): numbers of the C# engine, one frame earlier. The
    // glide counts from the event; C# starts at its first frame, which drew no move, and a stream
    // of events (a fast trackpad swipe) kept the content still.
    let mut host = scene(20000.0, 1.0, true);
    assert!(host.ui.wheel(150.0, 200.0, -1.0, 0.0));
    let expected = [-26.699, -50.479, -71.498, -89.91, -105.87, -119.536, -131.061, -140.602, -148.315, -150.0];
    for value in expected {
        host.frame_after(16.0);
        assert!(close(y(&host), value), "{} instead of {value}", y(&host));
    }
    host.settle();
    assert_eq!(y(&host), -150.0);

    // Up by 100 notches: clamped at the start, no bounce. At the start nothing can move up: not
    // used (React), the scroll around it or the page may take it.
    assert!(host.wheel(150.0, 200.0, 100.0));
    host.settle();
    assert_eq!(y(&host), 0.0);
    assert!(!host.wheel(150.0, 200.0, 1.0));
    assert!(run(&mut host).iter().all(|y| *y == 0.0));
}

#[test]
fn touchpad_events_move_the_content_at_once_and_add_up_to_the_fingers() {
    // DrawnUi.React c90cc45: events under half a notch apply at once; their shares add up.
    let mut host = scene(20000.0, 1.0, true);
    for _ in 0..20 {
        // 5 px of a 100 px notch each (Chrome / Edge), as a two-finger move sends them.
        assert!(host.wheel(150.0, 200.0, -0.05));
    }
    host.frame_after(16.0);
    assert!(close(y(&host), -150.0), "{}", y(&host));
    assert!(!host.ui.needs_frame(), "nothing glides");
    // 10 x 2 px: 30 points.
    for _ in 0..10 {
        assert!(host.wheel(150.0, 200.0, -0.02));
    }
    host.frame_after(16.0);
    assert!(close(y(&host), -180.0), "{}", y(&host));
    // A notch still glides, and a touchpad event during its glide goes on from its target.
    assert!(host.wheel(150.0, 200.0, -1.0));
    host.frame_after(16.0);
    assert!(y(&host) > -330.0 && y(&host) < -180.0, "{}", y(&host));
    assert!(host.wheel(150.0, 200.0, -0.1));
    host.frame_after(16.0);
    assert!(close(y(&host), -345.0), "{}", y(&host));
}

#[test]
fn a_glide_restarted_before_every_frame_still_moves_every_frame() {
    // A fast trackpad swipe on a Mac: events of more than half a notch, one before each frame.
    // Each restarted glide counts from its event: the content never stands still.
    let mut host = scene(20000.0, 1.0, true);
    let mut last = y(&host);
    for _ in 0..8 {
        assert!(host.wheel(150.0, 200.0, -1.2));
        assert!(y(&host) < last - 10.0, "stood still at {}", y(&host));
        last = y(&host);
    }
}

#[test]
fn fast_notches_add_up_to_their_targets() {
    // Each notch goes on from where the running step lands, not past it (React cb9cfed).
    let mut host = scene(20000.0, 1.0, true);
    for _ in 0..8 {
        assert!(host.wheel(150.0, 200.0, -1.0));
        host.frame_after(16.0);
    }
    host.settle();
    assert_eq!(y(&host), -1200.0);
}

#[test]
fn the_wheel_is_used_only_by_a_scroll_that_takes_gestures() {
    // A scroll in the left half of the canvas.
    let mut host = host_of(400.0, 600.0, 1.0, |app| {
        let content = SkiaLayout::column().height_request(3000).background_color(Color::YELLOW);
        SkiaLayout::new().fill().children((SkiaScroll::new().width_request(200).fill_y().assign(&mut app.scroll).content(content),))
    });
    // Outside the scroll nobody uses the wheel: a browser page would scroll.
    assert!(!host.wheel(300.0, 200.0, -1.0));
    assert!(host.wheel(100.0, 200.0, -1.0));
    host.settle();
    assert_eq!(y(&host), -150.0);

    let id = host.ui.state.scroll;
    host.ui.tree.get_mut(id).unwrap().set_responds_to_gestures(false);
    assert!(!host.wheel(100.0, 200.0, -1.0));
    host.pan((100.0, 500.0), (100.0, 300.0), 60.0, 6);
    host.settle();
    assert_eq!(y(&host), -150.0);
    // Code still scrolls it.
    scroll_to(&mut host, -300.0, 0.0);
    assert_eq!(y(&host), -300.0);

    let mut scroll = host.ui.tree.get_mut(id).unwrap();
    scroll.set_responds_to_gestures(true);
    scroll.set_orientation(ScrollOrientation::Neither);
    host.settle();
    assert!(!host.wheel(100.0, 200.0, -1.0));
}

// ---------------------------------------------------------------- what a scroll frame touches

/// Counts its measures and arranges.
struct Counter {
    measures: Arc<AtomicU32>,
    arranges: Arc<AtomicU32>,
}
impl Control for Counter {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        self.measures.fetch_add(1, Ordering::Relaxed);
        Size::new(10.0, 10.0)
    }
    fn arrange(&mut self, _cx: &mut LayoutCx) {
        self.arranges.fetch_add(1, Ordering::Relaxed);
    }
}

/// A 200 x 200 scroll over four colored rows of 100, tappable, and a counting control.
fn rows(app: &mut App) -> Build<SkiaLayout> {
    let boxes: Vec<Build<SkiaShape>> = (0..4)
        .map(|i| {
            let row = SkiaShape::new().fill_x().height_request(100).background_color(COLORS[i]);
            row.on_tapped(move |_me, app: &mut App, _cx| app.tapped.push(i))
        })
        .collect();
    let counter = Build::new(Counter { measures: app.measures.clone(), arranges: app.arranges.clone() });
    let content = SkiaLayout::column().spacing(0).assign(&mut app.content).children((boxes, counter));
    SkiaLayout::new().fill().children((SkiaScroll::new().width_request(200).height_request(200).assign(&mut app.scroll).content(content),))
}

#[test]
fn scrolling_runs_no_measure_and_no_arrange() {
    let mut host = host_of(300.0, 300.0, 1.0, rows);
    let counts = |host: &Host| (host.ui.state.measures.load(Ordering::Relaxed), host.ui.state.arranges.load(Ordering::Relaxed));
    let before = counts(&host);

    host.pan((100.0, 150.0), (100.0, 90.0), 64.0, 4);
    assert!(y(&host) < -40.0);
    host.settle();
    host.wheel(100.0, 100.0, -1.0);
    host.settle();
    scroll_to(&mut host, -20.0, 200.0);
    host.settle();
    assert_eq!(y(&host), -20.0);
    assert_eq!(counts(&host), before);
    // The pixels did move: 20 points of the first row are gone.
    assert_eq!(host.pixel(10, 79), COLORS[0]);
    assert_eq!(host.pixel(10, 80), COLORS[1]);
    // And nothing is drawn outside the scroll.
    assert_eq!(host.pixel(10, 201), Color::BLACK);
}

#[test]
fn a_fling_frame_allocates_nothing_in_the_engine() {
    let mut host = scene(30000.0, 1.0, true);
    flick(&mut host, 1.0);
    host.frame_after(16.0);
    host.frame_after(16.0);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let allocations = ALLOCATIONS.with(|a| a.get()) - before;
    assert!(scroll(&host).is_animating());
    assert_eq!(allocations, 0);
}

#[test]
fn a_fling_with_header_bars_tracking_and_indicator_allocates_nothing_and_lays_nothing_out() {
    let mut host = host_of(400.0, 600.0, 1.0, |app| {
        let rows: Vec<Build<SkiaShape>> = (0..300).map(|i| SkiaShape::new().fill_x().height_request(100).background_color(COLORS[i % 4])).collect();
        let counter = Build::new(Counter { measures: app.measures.clone(), arranges: app.arranges.clone() });
        let content = SkiaLayout::column().spacing(0).fill_x().children((rows, counter));
        let scroll = SkiaScroll::new().fill().header_sticky(true).header(SkiaShape::new().fill_x().height_request(50));
        let scroll = scroll.scroll_bars_visibility(ScrollBarVisibility::Vertical).track_index_position(RelativePositionType::Start);
        let scroll = scroll.refresh_enabled(true).refresh_indicator(SkiaShape::new().fill_x().height_request(50));
        scroll.assign(&mut app.scroll).content(content)
    });
    host.settle();
    let counts = |host: &Host| (host.ui.state.measures.load(Ordering::Relaxed), host.ui.state.arranges.load(Ordering::Relaxed));
    let laid_out = counts(&host);
    flick(&mut host, 1.0);
    host.frame_after(16.0);
    host.frame_after(16.0);
    let (before, index) = (ALLOCATIONS.with(|a| a.get()), scroll(&host).current_index());
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let allocations = ALLOCATIONS.with(|a| a.get()) - before;
    assert!(scroll(&host).is_animating() && scroll(&host).current_index() > index);
    assert_eq!((allocations, counts(&host)), (0, laid_out));
}

#[test]
fn a_settled_fling_asks_for_no_more_frames() {
    let mut host = scene(3000.0, 1.0, true);
    host.settle();
    assert!(!host.ui.needs_frame());
    flick(&mut host, 1.0);
    for _ in 0..50 {
        assert!(host.ui.needs_frame());
        host.frame_after(16.0);
    }
    host.settle();
    assert!(!scroll(&host).is_animating());
    let rest = y(&host);
    for _ in 0..3 {
        host.frame_after(16.0);
        assert!(!host.ui.needs_frame());
    }
    assert_eq!(y(&host), rest);
}

#[test]
fn a_press_stops_a_fling_where_it_is() {
    let mut host = scene(3000.0, 1.0, true);
    flick(&mut host, 1.0);
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    host.tap(200.0, 300.0);
    let stopped = y(&host);
    assert!(stopped < -700.0 && stopped > -800.0, "{stopped}");
    assert!(!scroll(&host).is_animating() && !host.ui.needs_frame());
    // At rest the content sits on a whole pixel, the offset keeps its fraction.
    assert_eq!(pixels(&host).y, stopped.round());
}

#[test]
fn content_that_shrinks_pulls_the_offset_back_inside() {
    let mut host = host_of(300.0, 300.0, 1.0, rows);
    scroll_to(&mut host, -200.0, 0.0);
    host.settle();
    assert_eq!(y(&host), -200.0);
    // 410 points of content become 210: 10 points of travel are left.
    let content = host.ui.state.content;
    let rows = host.ui.tree.children(content)[..2].to_vec();
    for row in rows {
        host.ui.tree.remove(row);
    }
    host.settle();
    assert_eq!(y(&host), -10.0);
    assert_eq!(pixels(&host), Point::new(0.0, -10.0));
    assert_eq!(scroll(&host).content_size(), Size::new(200.0, 210.0));
}

// ---------------------------------------------------------------- who gets the gesture

#[test]
fn a_tap_inside_scrolled_content_hits_the_child_under_the_pointer() {
    let mut host = host_of(300.0, 300.0, 1.0, rows);
    host.tap(10.0, 60.0);
    assert_eq!(host.ui.state.tapped, [0]);

    // Wheeled one notch down: rows 1 (50 points left of it), 2 and 3 are in view.
    host.wheel(100.0, 100.0, -1.0);
    host.settle();
    assert_eq!(y(&host), -150.0);
    host.tap(10.0, 40.0);
    host.tap(10.0, 60.0);
    host.tap(10.0, 160.0);
    assert_eq!(host.ui.state.tapped, [0, 1, 2, 3]);
    // Below the scroll nothing is hit, although the content has its rect there.
    host.tap(10.0, 250.0);
    assert_eq!(host.ui.state.tapped.len(), 4);
    assert_eq!(y(&host), -150.0);
}

/// Six buttons of 100 points in a 300 x 300 scroll; each counts its taps.
fn buttons(app: &mut App) -> Build<SkiaScroll> {
    let buttons: Vec<Build<SkiaButton>> = (0..6)
        .map(|i| SkiaButton::new("Go").fill_x().height_request(100).on_tapped(move |_me, app: &mut App, _cx| app.tapped.push(i)))
        .collect();
    SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).children(buttons))
}

#[test]
fn a_pan_on_a_button_scrolls_and_does_not_tap() {
    let mut host = host_of(300.0, 300.0, 1.0, buttons);
    host.tap(150.0, 150.0);
    assert_eq!(host.ui.state.tapped, [1]);

    // The button has the press; when the pointer leaves its 5 points it lets go and the scroll pans.
    host.pan((150.0, 250.0), (150.0, 150.0), 800.0, 8);
    assert!(y(&host) < -80.0, "{}", y(&host));
    host.settle();
    assert_eq!(host.ui.state.tapped, [1]);

    // A pan shorter than the 16 points of a tap still is not one.
    let before = y(&host);
    host.pan((150.0, 250.0), (150.0, 238.0), 400.0, 4);
    assert!(y(&host) < before);
    host.settle();
    assert_eq!(host.ui.state.tapped, [1]);

    // After all that a tap is a tap again, on the button that is under the pointer now.
    assert!((-300.0..-100.0).contains(&y(&host)));
    host.tap(150.0, 150.0);
    assert_eq!(host.ui.state.tapped.len(), 2);
    assert!(host.ui.state.tapped[1] > 1);
}

/// A horizontal strip inside a vertical scroll.
fn nested(ignore_wrong_direction: bool) -> Host {
    host_of(400.0, 600.0, 1.0, |app| {
        let strip = SkiaScroll::new()
            .orientation(ScrollOrientation::Horizontal)
            .fill_x()
            .height_request(200)
            .assign(&mut app.inner)
            .content(SkiaLayout::row().width_request(2000).height_request(200).background_color(Color::GREEN));
        let rest = SkiaShape::new().fill_x().height_request(2800).background_color(Color::YELLOW);
        SkiaScroll::new()
            .fill()
            .ignore_wrong_direction(ignore_wrong_direction)
            .assign(&mut app.scroll)
            .content(SkiaLayout::column().spacing(0).children((strip, rest)))
    })
}

fn inner_x(host: &Host) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.inner).unwrap().viewport_offset_x()
}

#[test]
fn a_vertical_scroll_leaves_horizontal_wheel_events_a_horizontal_one_takes_both() {
    // C# ba03cb20, React 4d2e8e1: the sideways part of a diagonal swipe does not step the page.
    let mut host = nested(false);
    // Over the page, below the strip: a horizontal event is not used, nothing moves.
    assert!(!host.wheel_horizontal(200.0, 400.0, -0.3));
    assert_eq!((y(&host), inner_x(&host)), (0.0, 0.0));
    // A vertical one scrolls the page.
    assert!(host.wheel(200.0, 400.0, -0.3));
    assert!(close(y(&host), -45.0), "{}", y(&host));
    // Over the strip (back at the top): a horizontal event moves the strip, the page stays.
    host.wheel(200.0, 400.0, 0.3);
    assert!(host.wheel_horizontal(200.0, 100.0, -0.3));
    assert!(close(inner_x(&host), -45.0) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));
    // The strip takes vertical events too.
    assert!(host.wheel(200.0, 100.0, -0.2));
    assert!(close(inner_x(&host), -75.0) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));
}

#[test]
fn the_scroll_inside_gets_the_pan_first() {
    let mut host = nested(false);
    // Along the strip: the strip takes it (React asks the children first).
    host.pan((300.0, 100.0), (100.0, 100.0), 60.0, 6);
    host.settle();
    assert!(close(inner_x(&host), -1521.597) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));

    // Straight down the page, starting on the strip: the strip has no speed along its axis and
    // leaves it, the page scrolls.
    host.pan((200.0, 150.0), (200.0, 50.0), 1200.0, 8);
    assert!(close(inner_x(&host), -1521.597) && close(y(&host), -97.794), "{} {}", inner_x(&host), y(&host));
    host.settle();
    assert!(close(y(&host), -97.794));

    // A slanted pan has speed along the strip: the strip takes it, the page stays.
    scroll_to(&mut host, 0.0, 0.0);
    host.settle();
    host.pan((300.0, 150.0), (200.0, 100.0), 1200.0, 8);
    assert!(close(inner_x(&host), -1610.388) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));
}

#[test]
fn ignore_wrong_direction_of_the_page_changes_nothing_for_the_strip_inside() {
    // The strip still goes first; any speed along it is enough, a mostly vertical pan included.
    let mut host = nested(true);
    host.pan((300.0, 150.0), (200.0, 100.0), 1200.0, 8);
    assert!(close(inner_x(&host), -97.794) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));
    host.settle();
    host.pan((200.0, 150.0), (180.0, 50.0), 1200.0, 8);
    assert!(close(inner_x(&host), -117.353) && y(&host) == 0.0, "{} {}", inner_x(&host), y(&host));
}

// ---------------------------------------------------------------- LoadMore

fn row() -> Build<SkiaShape> {
    SkiaShape::new().fill_x().height_request(50).background_color(Color::BLUE)
}

/// A 100 x 200 scroll over `rows` rows of 50 points; `on_load_more` adds `grow_by` rows.
fn loading(rows: usize, grow_by: usize) -> Host {
    let mut host = host_of(100.0, 200.0, 1.0, |app| {
        app.grow_by = grow_by;
        let content = SkiaLayout::column().spacing(0).assign(&mut app.content).children((0..rows).map(|_| row()).collect::<Vec<_>>());
        SkiaScroll::new()
            .fill()
            .on_load_more(|_me, app: &mut App, cx| {
                app.loads += 1;
                for _ in 0..app.grow_by {
                    cx.add_child(app.content, row());
                }
            })
            .assign(&mut app.scroll)
            .content(content)
    });
    host.settle();
    host
}

#[test]
fn load_more_fills_a_viewport_the_content_does_not_fill() {
    // One row of 50 in 200 points, one more per call. React calls while there is nothing to
    // scroll, a content exactly as tall as the viewport included: four calls, 250 points.
    let host = loading(1, 1);
    assert_eq!(host.ui.state.loads, 4);
    assert_eq!(scroll(&host).content_size(), Size::new(100.0, 250.0));

    // A handler that adds nothing is called once, not every frame.
    let mut host = loading(1, 0);
    for _ in 0..200 {
        host.frame_after(16.0);
    }
    assert_eq!(host.ui.state.loads, 1);
}

#[test]
fn load_more_fires_again_for_new_content_or_after_going_away() {
    // 6 rows = 300 points, 100 of travel; the end is the zone (offset 0 points).
    let mut host = loading(6, 4);
    assert_eq!(host.ui.state.loads, 0);
    scroll_to(&mut host, -100.0, 0.0);
    host.settle();
    // The call added 200 points: the viewport is no longer at the end.
    assert_eq!(host.ui.state.loads, 1);
    assert_eq!(scroll(&host).content_size().height, 500.0);
    scroll_to(&mut host, -300.0, 0.0);
    host.settle();
    assert_eq!(host.ui.state.loads, 2);

    // Nothing new to add: the end of the same content calls once more only after the viewport
    // was more than 100 points away and 2 seconds passed.
    host.ui.state.grow_by = 0;
    scroll_to(&mut host, -500.0, 0.0);
    host.settle();
    assert_eq!(host.ui.state.loads, 3);
    scroll_to(&mut host, -300.0, 0.0);
    host.settle();
    scroll_to(&mut host, -500.0, 0.0);
    host.settle();
    assert_eq!(host.ui.state.loads, 3);
    scroll_to(&mut host, -300.0, 0.0);
    host.frame_after(2100.0);
    scroll_to(&mut host, -500.0, 0.0);
    host.settle();
    assert_eq!(host.ui.state.loads, 4);
}

// ---------------------------------------------------------------- a press that is taken away

/// `Headless::pan`, but the press ends in a cancel: the browser took the touch, the window lost
/// the pointer.
fn pan_cancelled(host: &mut Host, from: (f32, f32), to: (f32, f32), duration_ms: f64, steps: u32) {
    let step_ms = duration_ms / steps as f64;
    let send = |host: &mut Host, kind, (x, y): (f32, f32)| {
        let time_ms = host.time_ms();
        host.ui.pointer(kind, x, y, time_ms);
        host.frame_after(step_ms);
    };
    send(host, PointerKind::Down, from);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        send(host, PointerKind::Move, (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
    }
    send(host, PointerKind::Cancel, to);
}

#[test]
fn a_cancelled_press_does_not_fling() {
    // The flick that flies 1,300 points when it is released.
    let mut host = scene(3000.0, 1.0, true);
    pan_cancelled(&mut host, (200.0, 500.0), (200.0, 300.0), 60.0, 6);
    assert!(close(y(&host), -194.118));
    assert!(!scroll(&host).is_animating() && !scroll(&host).is_user_panning());
    host.settle();
    assert!(close(y(&host), -194.118));
    assert_eq!(pixels(&host), Point::new(0.0, -194.0));
    assert!(!host.ui.needs_frame());

    // Pulled past the start and taken away there: it still comes back, without the push of the pull.
    let mut host = scene(3000.0, 1.0, true);
    pan_cancelled(&mut host, (200.0, 100.0), (200.0, 300.0), 160.0, 10);
    assert!(close(y(&host), 91.568) && scroll(&host).is_animating());
    let trace = run(&mut host);
    assert!(trace.iter().all(|y| *y <= 91.568), "no way further out: {:?}", &trace[..3]);
    assert_trace(&trace, &[(0, 88.052), (1, 79.868), (2, 69.601), (3, 58.869), (4, 48.626)]);
    assert_eq!(trace.len(), 23);
    assert_eq!(y(&host), 0.0);

    // A cancel without a press is nothing.
    host.ui.pointer(PointerKind::Cancel, 200.0, 300.0, host.time_ms());
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
}

#[test]
fn a_cancelled_press_does_not_tap() {
    let mut host = host_of(300.0, 300.0, 1.0, buttons);
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Down, 150.0, 150.0, now);
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Cancel, 150.0, 150.0, now + 16.0);
    host.settle();
    assert!(host.ui.state.tapped.is_empty());
    assert_eq!(y(&host), 0.0);
    // The button was let go: the next tap is a tap.
    host.tap(150.0, 150.0);
    assert_eq!(host.ui.state.tapped, [1]);
}

// ---------------------------------------------------------------- handlers

#[test]
fn the_handlers_get_the_scroll_as_me() {
    let mut host = host_of(100.0, 200.0, 1.0, |app| {
        let content = SkiaLayout::column().height_request(1000).background_color(Color::YELLOW);
        SkiaScroll::new()
            .fill()
            .bounces(false)
            .on_scrolled(|me, app: &mut App, _cx, offset| {
                // `me` reads the scroll and sets its properties, like in `on_tapped`.
                assert_eq!(me.viewport_offset_y(), offset.y);
                me.set_load_more_offset(-offset.y);
                app.scrolled.push(offset.y);
            })
            .on_load_more(|me, app: &mut App, _cx| {
                app.loads += 1;
                me.set_friction_scrolled(0.9);
            })
            .assign(&mut app.scroll)
            .content(content)
    });
    host.settle();
    assert_eq!((host.ui.state.loads, scroll(&host).p.friction_scrolled), (0, 0.3));
    // Wheeled one notch: 150 of the 800 points of travel. The zone `on_scrolled` sets is as wide
    // as the way gone, so the end is 650 away and its zone 150: no call yet.
    host.wheel(50.0, 100.0, -1.0);
    host.settle();
    assert_eq!((scroll(&host).p.load_more_offset, host.ui.state.loads), (150.0, 0));
    // Three more notches, each from rest: 600 gone, 200 left, a zone of 600.
    for _ in 0..3 {
        host.wheel(50.0, 100.0, -1.0);
        host.settle();
    }
    assert_eq!(y(&host), -600.0);
    assert_eq!((host.ui.state.loads, scroll(&host).p.friction_scrolled), (1, 0.9));
}

// ---------------------------------------------------------------- a list inside a page, as the C# engine has it

/// The scene of the probe on the C# engine (HelloMaui ScrollPage, card 1): a 400 x 600 page
/// scroll over a 100 point block, a 240 point vertical scroll with 14 rows (654 points, 414 of
/// travel) and `filler` points below.
fn page_with_a_list(filler: f32) -> Host {
    host_of(400.0, 600.0, 1.0, move |app| {
        let rows: Vec<Build<SkiaShape>> = (0..14).map(|_| SkiaShape::new().fill_x().height_request(40).background_color(Color::BLUE)).collect();
        let list = SkiaScroll::new()
            .fill_x()
            .height_request(240)
            .ignore_wrong_direction(true)
            .assign(&mut app.inner)
            .content(SkiaLayout::column().spacing(6).padding(8).children(rows));
        let top = SkiaShape::new().fill_x().height_request(100).background_color(Color::WHITE);
        let below = SkiaShape::new().fill_x().height_request(filler).background_color(Color::YELLOW);
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).children((top, list, below)))
    })
}

fn inner_y(host: &Host) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.inner).unwrap().viewport_offset_y()
}

fn inner_to(host: &mut Host, y: f32) {
    let inner = host.ui.state.inner;
    host.ui.tree.cx().scroll_to(inner, 0.0, y, 0);
    host.settle();
}

#[test]
fn a_pan_over_a_list_inside_a_page_scrolls_the_list() {
    // React: the list inside gets the pan first and keeps it for the press, at its ends too (it
    // pulls there on its rubber band); the page never moves under it.
    for (list_at, up, up_rest, down, down_rest) in [
        (-200.0, -299.118, -414.0, -100.882, 0.0),
        (0.0, -99.118, -236.928, 44.424, 0.0),
        (-414.0, -458.424, -414.0, -314.882, -177.072),
    ] {
        // Finger up 100 points over the list, in 320 ms.
        let mut host = page_with_a_list(1500.0);
        inner_to(&mut host, list_at);
        host.pan((200.0, 280.0), (200.0, 180.0), 320.0, 20);
        assert!(close(inner_y(&host), up) && y(&host) == 0.0, "list at {list_at}: {} {}", inner_y(&host), y(&host));
        host.settle();
        assert!(close(inner_y(&host), up_rest) && y(&host) == 0.0, "list at {list_at}: {}", inner_y(&host));

        // Finger down.
        let mut host = page_with_a_list(1500.0);
        inner_to(&mut host, list_at);
        host.pan((200.0, 180.0), (200.0, 280.0), 320.0, 20);
        assert!(close(inner_y(&host), down) && y(&host) == 0.0, "list at {list_at}: {} {}", inner_y(&host), y(&host));
        host.settle();
        assert!(close(inner_y(&host), down_rest) && y(&host) == 0.0, "list at {list_at}: {}", inner_y(&host));
    }

    // A flick over the list flings the list.
    let mut host = page_with_a_list(1500.0);
    inner_to(&mut host, -200.0);
    host.pan((200.0, 280.0), (200.0, 180.0), 60.0, 6);
    assert!(close(inner_y(&host), -297.059), "{}", inner_y(&host));
    host.settle();
    assert_eq!((inner_y(&host), y(&host)), (-414.0, 0.0));

    // A page with nothing to scroll: the same.
    let mut host = page_with_a_list(100.0);
    inner_to(&mut host, -200.0);
    host.pan((200.0, 280.0), (200.0, 180.0), 320.0, 20);
    assert!(close(inner_y(&host), -299.118), "{}", inner_y(&host));
    host.settle();
    assert_eq!((inner_y(&host), y(&host)), (-414.0, 0.0));
}

#[test]
fn the_wheel_over_a_list_inside_a_page_scrolls_the_list_while_it_can() {
    // React: the list under the pointer takes the wheel while it can move that way.
    let mut host = page_with_a_list(1500.0);
    inner_to(&mut host, -200.0);
    assert!(host.wheel(200.0, 220.0, -1.0));
    host.settle();
    assert_eq!((inner_y(&host), y(&host)), (-350.0, 0.0));
    assert!(host.wheel(200.0, 220.0, 1.0));
    host.settle();
    assert!(host.wheel(200.0, 220.0, 1.0));
    host.settle();
    assert_eq!((inner_y(&host), y(&host)), (-50.0, 0.0));

    // At the end of the list the page takes it.
    inner_to(&mut host, -414.0);
    assert!(host.wheel(200.0, 220.0, -1.0));
    host.settle();
    assert_eq!((inner_y(&host), y(&host)), (-414.0, -150.0));

    // Nothing can move either way: not used, the browser page may scroll.
    let mut host = page_with_a_list(100.0);
    inner_to(&mut host, 0.0);
    assert!(!host.wheel(200.0, 220.0, 1.0));
}

// ---------------------------------------------------------------- the real ScrollPage of HelloMaui / HelloWpf

/// The page as the C# sample lays it out on a 760 x 800 canvas (read from the running page):
/// 2129 points of content; a vertical scroll of 240 at y 102 (card 1: header 70, 14 rows, footer
/// 50, `IgnoreWrongDirection`), a horizontal strip of 70 at y 1246 (card 4), a vertical scroll of
/// 180 at y 1921 (card 7). `inner` names the one the test looks at: 0, 1 or 2.
fn scroll_page(inner: usize) -> Host {
    host_of(760.0, 800.0, 1.0, move |app| {
        let rows = |count: usize| -> Build<SkiaLayout> {
            let rows: Vec<Build<SkiaShape>> = (0..count).map(|_| SkiaShape::new().fill_x().height_request(40).background_color(Color::BLUE)).collect();
            SkiaLayout::column().spacing(6).padding(8).fill_x().children(rows)
        };
        let block = |height: f32| SkiaShape::new().fill_x().height_request(height);
        let mut card1 = SkiaScroll::new().fill_x().height_request(240).ignore_wrong_direction(true);
        card1 = card1.header(block(70.0)).footer(block(50.0));
        let tiles: Vec<Build<SkiaShape>> = (0..14).map(|_| SkiaShape::new().width_request(120).height_request(50)).collect();
        let mut card4 = SkiaScroll::new().orientation(ScrollOrientation::Horizontal).fill_x().height_request(70);
        let mut card7 = SkiaScroll::new().fill_x().height_request(180).ignore_wrong_direction(true);
        match inner {
            0 => card1 = card1.assign(&mut app.inner),
            1 => card4 = card4.assign(&mut app.inner),
            _ => card7 = card7.assign(&mut app.inner),
        }
        let (card1, card7) = (card1.content(rows(14)), card7.content(rows(16)));
        let card4 = card4.content(SkiaLayout::row().spacing(8).padding(8).children(tiles));
        let page = (block(102.0), card1, block(904.0), card4, block(605.0), card7, block(28.0));
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).fill_x().children(page))
    })
}

#[test]
fn the_scroll_page_of_the_sample_scrolls_the_scrolls_in_it() {
    // React numbers for the ScrollPage layout (the C# sample's, laid out by the C# engine): the
    // scroll under the pointer takes the pan and the wheel; the page never moves under a pan
    // that starts on one of them, and takes the wheel only where the inner one cannot move.
    // Card 1, the pointer in its middle (380, 222): the finger goes 60 up in 320 ms.
    for (inner_at, after, rest) in [(0.0, -58.676, -141.143), (-100.0, -158.676, -241.143), (-534.0, -562.447, -534.0)] {
        let mut host = scroll_page(0);
        host.settle();
        assert_eq!(scroll(&host).content_size().height, 2129.0);
        inner_to(&mut host, inner_at);
        host.pan((380.0, 252.0), (380.0, 192.0), 320.0, 8);
        assert!(close(inner_y(&host), after) && y(&host) == 0.0, "inner at {inner_at}: {} {}", inner_y(&host), y(&host));
        host.settle();
        assert!(close(inner_y(&host), rest) && y(&host) == 0.0, "inner at {inner_at}: {}", inner_y(&host));
    }
    // 60 down, the inner scroll in its middle: it goes back to its start.
    let mut host = scroll_page(0);
    inner_to(&mut host, -100.0);
    host.pan((380.0, 192.0), (380.0, 252.0), 320.0, 8);
    assert!(close(inner_y(&host), -41.324) && y(&host) == 0.0, "{}", inner_y(&host));
    host.settle();
    assert_eq!((y(&host), inner_y(&host)), (0.0, 0.0));
    // The wheel over it: the inner scroll's, the page's at its end.
    for (inner_at, inner_rest, page) in [(0.0, -150.0, 0.0), (-100.0, -250.0, 0.0), (-534.0, -534.0, -150.0)] {
        let mut host = scroll_page(0);
        inner_to(&mut host, inner_at);
        assert!(host.wheel(380.0, 222.0, -1.0));
        host.settle();
        assert_eq!((y(&host), inner_y(&host)), (page, inner_rest), "inner at {inner_at}");
    }

    // The page at its end (-1329), card 7 at (380, 682).
    let mut host = scroll_page(2);
    scroll_to(&mut host, -100_000.0, 0.0);
    host.settle();
    assert_eq!(y(&host), -1329.0);
    host.pan((380.0, 712.0), (380.0, 652.0), 320.0, 8);
    assert!(close(inner_y(&host), -58.676) && y(&host) == -1329.0, "{}", inner_y(&host));
    host.settle();
    assert!(close(inner_y(&host), -141.143));
    assert!(host.wheel(380.0, 682.0, -1.0));
    host.settle();
    assert!(close(inner_y(&host), -291.143) && y(&host) == -1329.0, "{}", inner_y(&host));
    inner_to(&mut host, -100.0);
    host.pan((380.0, 652.0), (380.0, 712.0), 320.0, 8);
    assert!(close(inner_y(&host), -41.324) && y(&host) == -1329.0, "{}", inner_y(&host));
    host.settle();
    assert_eq!((y(&host), inner_y(&host)), (-1329.0, 0.0));

    // The horizontal strip (card 4) at (380, 381) with the page at -900: a pan along it is its
    // own, a pan across it the page's (the strip has no speed along it), the wheel the strip's.
    let mut host = scroll_page(1);
    scroll_to(&mut host, -900.0, 0.0);
    host.settle();
    host.pan((410.0, 381.0), (350.0, 381.0), 320.0, 8);
    assert!(close(inner_x(&host), -58.676) && y(&host) == -900.0, "{}", inner_x(&host));
    host.settle();
    assert!(close(inner_x(&host), -141.143));
    let mut host = scroll_page(1);
    scroll_to(&mut host, -900.0, 0.0);
    host.settle();
    host.pan((380.0, 411.0), (380.0, 351.0), 320.0, 8);
    assert!(close(y(&host), -958.676) && inner_x(&host) == 0.0, "{}", y(&host));
    host.settle();
    assert!(close(y(&host), -1041.143), "{}", y(&host));
    assert!(host.wheel(380.0, 240.0, -1.0));
    host.settle();
    assert!(close(y(&host), -1041.143) && inner_x(&host) == -150.0, "{} {}", y(&host), inner_x(&host));
}

