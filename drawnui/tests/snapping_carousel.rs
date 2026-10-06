//! SkiaCarousel with the numbers of a probe on the React engine (DrawnUi.React `dist` in node: its
//! Canvas frame loop and pointer handling copied, the robot of `Headless`, 16 ms frames). Scene: a
//! 400 x 300 canvas, a carousel 200 tall filling the width, four slides.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::prelude::*;
use drawnui::PointerKind;
use drawnui::testing::Headless;

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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Heard {
    Index(usize),
    Transition(bool),
    Appearing(usize),
    Disappearing(usize),
}

#[derive(Default)]
struct App {
    carousel: Handle<SkiaCarousel>,
    scroll: Handle<SkiaScroll>,
    heard: Vec<Heard>,
    taps: u32,
    measures: Arc<AtomicU32>,
    arranges: Arc<AtomicU32>,
}

type Host = Headless<App>;

const COLORS: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

fn slides() -> Vec<Build<SkiaShape>> {
    (0..4).map(|i| SkiaShape::new().background_color(COLORS[i])).collect()
}

fn listen(carousel: Build<SkiaCarousel>) -> Build<SkiaCarousel> {
    carousel
        .on_selected_index_changed(|_me, app: &mut App, _cx, i| app.heard.push(Heard::Index(i)))
        .on_transition_changed(|_me, app: &mut App, _cx, t| app.heard.push(Heard::Transition(t)))
        .on_item_appearing(|_me, app: &mut App, _cx, i| app.heard.push(Heard::Appearing(i)))
        .on_item_disappearing(|_me, app: &mut App, _cx, i| app.heard.push(Heard::Disappearing(i)))
}

/// The probe's scene at `scale`, the carousel set up by `setup`.
fn scene(scale: f32, setup: impl FnOnce(Build<SkiaCarousel>) -> Build<SkiaCarousel>) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let carousel = setup(listen(SkiaCarousel::new().height_request(200)).children(slides())).assign(&mut app.carousel);
        SkiaLayout::new().fill().children((carousel,))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), (400.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    host
}

fn carousel(host: &Host) -> &SkiaCarousel {
    host.ui.tree.find(host.ui.state.carousel).unwrap()
}

/// Position along the axis, the selected slide, in transition.
fn state(host: &Host) -> (f32, usize, bool) {
    let c = carousel(host);
    let p = c.current_position();
    (if c.p.is_vertical { p.y } else { p.x }, c.selected_index(), c.in_transition())
}

/// Frames until nothing is pending, 16 ms apart: the state after each.
fn run(host: &mut Host) -> Vec<(f32, usize, bool)> {
    let mut trace = Vec::new();
    while host.ui.needs_frame() && trace.len() < 900 {
        host.frame_after(16.0);
        trace.push(state(host));
    }
    while trace.len() > 1 && trace[trace.len() - 1] == trace[trace.len() - 2] {
        trace.pop();
    }
    trace
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.01
}

/// The positions of the trace, and its end state.
fn assert_trace(trace: &[(f32, usize, bool)], positions: &[f32], end: (f32, usize, bool)) {
    assert_eq!(trace.len(), positions.len(), "{trace:?}");
    for (i, (actual, expected)) in trace.iter().zip(positions).enumerate() {
        assert!(close(actual.0, *expected), "frame {i}: {} instead of {expected}", actual.0);
    }
    assert_eq!(*trace.last().unwrap(), end);
}

/// The probe's flick: 100 points to the left in 60 ms.
fn flick(host: &mut Host, scale: f32) {
    host.pan((300.0 * scale, 100.0 * scale), (200.0 * scale, 100.0 * scale), 60.0, 6);
}

// The React traces, one value per frame after the release.
const K1: [f32; 32] = [
    -115.073, -130.109, -145.068, -159.913, -174.607, -189.112, -203.393, -217.412, -231.135, -244.526, -257.552, -270.181, -282.379,
    -294.117, -305.364, -316.093, -326.275, -335.887, -344.902, -353.298, -361.055, -368.152, -374.572, -380.299, -385.317, -389.614,
    -393.18, -396.006, -398.083, -399.408, -399.976, -400.0,
];

#[test]
fn a_flick_snaps_to_the_next_slide_as_in_react() {
    let mut host = scene(1.0, |c| c);
    let c = carousel(&host);
    assert_eq!(c.snap_points(), [Point::new(0.0, 0.0), Point::new(-400.0, 0.0), Point::new(-800.0, 0.0), Point::new(-1200.0, 0.0)]);
    // The next slide touches the edge: React counts it as on screen.
    assert_eq!(host.ui.state.heard, [Heard::Appearing(0), Heard::Appearing(1)]);
    host.ui.state.heard.clear();

    flick(&mut host, 1.0);
    // The slides followed the finger; the release picked slide 1 by its speed.
    assert_eq!(state(&host), (-100.0, 1, true));
    assert_eq!(host.ui.state.heard, [Heard::Transition(true), Heard::Index(1)]);
    // The snap without `bounces`: SinOut over 0.5 s.
    let trace = run(&mut host);
    assert_trace(&trace, &K1, (-400.0, 1, false));
    assert_eq!(host.ui.state.heard[2..], [Heard::Transition(false), Heard::Appearing(2)]);
    assert_eq!(host.ui.tree.base(host.ui.state.carousel).unwrap().content_offset, Point::new(-400.0, 0.0));
    assert!(!host.ui.needs_frame());
}

#[test]
fn a_drag_whose_first_move_is_tiny_still_pans_on_retina() {
    // A Mac trackpad drag at scale 2: the first move is 1.3 px, under the 2 points that tell the
    // direction. It must not take the drag away from the carousel.
    let mut host = scene(2.0, |c| c);
    let (x, y) = (600.0, 200.0);
    let t = host.time_ms();
    host.ui.pointer(PointerKind::Down, x, y, t);
    host.frame_after(16.0);
    for dx in [1.3, 8.6, 26.4, 50.5, 80.0, 120.0, 160.0] {
        let t = host.time_ms();
        host.ui.pointer(PointerKind::Move, x - dx, y + dx / 20.0, t);
        host.frame_after(16.0);
    }
    assert!(carousel(&host).is_user_panning());
    // The move under 2 points is not applied: 158.7 px of the 160 at scale 2.
    assert!(close(state(&host).0, -79.35), "{:?}", state(&host));
}

#[test]
fn a_slow_drag_goes_back_or_on_by_the_nearest_slide() {
    // 40 points in 640 ms: no speed, the nearest slide is the one it left.
    let mut host = scene(1.0, |c| c);
    host.pan((300.0, 100.0), (260.0, 100.0), 640.0, 16);
    assert_eq!(state(&host), (-40.0, 0, true));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 32);
    for (frame, value) in [(0, -37.99), (5, -28.118), (10, -18.993), (20, -5.193), (30, -0.003)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (0.0, 0, false));

    // 240 points in 1200 ms: the nearest slide is 1, and the finger still went left at 200 points
    // per second (100 and more counts): on to slide 2.
    let mut host = scene(1.0, |c| c);
    host.ui.state.heard.clear();
    host.pan((300.0, 100.0), (60.0, 100.0), 1200.0, 16);
    assert_eq!(state(&host), (-240.0, 2, true));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 32);
    for (frame, value) in [(0, -268.137), (10, -534.098), (20, -727.303), (30, -799.956)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (-800.0, 2, false));
    let heard = &host.ui.state.heard;
    assert_eq!(heard[..2], [Heard::Transition(true), Heard::Index(2)]);
    assert_eq!(heard[2..], [Heard::Disappearing(0), Heard::Appearing(2), Heard::Transition(false), Heard::Appearing(3)]);
}

#[test]
fn bounces_snaps_on_a_spring_and_pulls_past_the_ends() {
    let mut host = scene(1.0, |c| c.bounces(true));
    flick(&mut host, 1.0);
    let trace = run(&mut host);
    assert_eq!(trace.len(), 27);
    for (frame, value) in [(0, -114.75), (1, -143.318), (5, -274.315), (10, -363.329), (20, -398.438), (25, -399.803)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (-400.0, 1, false));

    // Pulled to the right at the first slide: the rubber band, then the spring home.
    let mut host = scene(1.0, |c| c.bounces(true));
    host.pan((100.0, 100.0), (200.0, 100.0), 60.0, 6);
    assert!(close(state(&host).0, 14.458), "{:?}", state(&host));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 19);
    for (frame, value) in [(0, 16.864), (1, 16.992), (5, 10.13), (10, 3.276), (17, 0.44)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (0.0, 0, false));
}

#[test]
fn sides_offset_and_spacing_make_the_step() {
    // 400 + 12 - 2 x 40: slides 332 apart; the neighbours peek in 40 - 12 points.
    let mut host = scene(1.0, |c| c.sides_offset(40).spacing(12));
    assert_eq!(carousel(&host).snap_points()[1], Point::new(-332.0, 0.0));
    assert_eq!(host.ui.state.heard, [Heard::Appearing(0)]);
    flick(&mut host, 1.0);
    let trace = run(&mut host);
    assert_eq!(trace.len(), 32);
    for (frame, value) in [(0, -111.657), (10, -221.841), (20, -301.883), (30, -331.982)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (-332.0, 1, false));
    // The slide is inset by the offset, its box is the carousel's.
    let slide = host.ui.tree.children(host.ui.state.carousel)[1];
    assert_eq!(host.rect(slide), Rect::new(372.0, 0.0, 692.0, 200.0));
}

#[test]
fn a_looped_carousel_wraps_both_ways_through_a_virtual_slide() {
    let mut host = scene(1.0, |c| c.is_looped(true));
    // The last slide is drawn before the first, and counts as on screen.
    assert_eq!(host.ui.state.heard, [Heard::Appearing(0), Heard::Appearing(1), Heard::Appearing(3)]);
    host.ui.state.heard.clear();
    // Right from slide 0: to the virtual anchor one step before it, which is slide 3.
    host.pan((100.0, 100.0), (200.0, 100.0), 60.0, 6);
    assert_eq!(state(&host), (100.0, 3, true));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 32);
    let expected: Vec<f32> = K1.iter().map(|v| -v).collect();
    for frame in [0, 10, 20, 30] {
        assert!(close(trace[frame].0, expected[frame]), "frame {frame}: {:?}", trace[frame]);
    }
    // At rest the virtual anchor becomes the real slide: the same picture at -1200.
    assert_eq!(*trace.last().unwrap(), (-1200.0, 3, false));
    let heard = &host.ui.state.heard;
    assert_eq!(heard[..3], [Heard::Disappearing(1), Heard::Transition(true), Heard::Index(3)]);
    assert_eq!(heard[3..], [Heard::Transition(false), Heard::Appearing(2)]);
    // Slide 3 is at its own place again, slide 0 after it.
    let children = host.ui.tree.children(host.ui.state.carousel).to_vec();
    assert_eq!((host.rect(children[3]).left, host.rect(children[0]).left), (1200.0, 1600.0));

    // Left from slide 3: on to slide 0 through the anchor after the last.
    flick(&mut host, 1.0);
    assert!(close(state(&host).0, -1300.0) && state(&host).1 == 0, "{:?}", state(&host));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 32);
    for frame in [0, 10, 20, 30] {
        assert!(close(trace[frame].0, K1[frame] - 1200.0), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (0.0, 0, false));
    assert_eq!(host.rect(children[3]).left, -400.0);
}

#[test]
fn code_moves_the_carousel() {
    // The index set from code: SinOut over 0.5 s, the auto velocity of 25 points per second.
    let mut host = scene(1.0, |c| c);
    host.ui.state.heard.clear();
    let id = host.ui.state.carousel;
    host.ui.tree.get_mut(id).unwrap().set_selected_index(2);
    let trace = run(&mut host);
    assert_eq!(trace.len(), 33);
    for (frame, value) in [(0, 0.0), (1, -40.195), (10, -385.403), (20, -675.462), (31, -799.937)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (-800.0, 2, false));
    assert_eq!(host.ui.state.heard[..2], [Heard::Index(2), Heard::Transition(true)]);

    // `linear_speed_ms`: 350 ms per slide, two slides.
    let mut host = scene(1.0, |c| c.linear_speed_ms(350));
    let id = host.ui.state.carousel;
    host.ui.tree.get_mut(id).unwrap().set_selected_index(2);
    let trace = run(&mut host);
    assert_eq!(trace.len(), 45);
    for (frame, value) in [(1, -28.717), (19, -504.386), (20, -526.351), (43, -799.71)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }

    // Looped, from the last slide on: through the virtual anchor to slide 0.
    let mut host = scene(1.0, |c| c.is_looped(true).selected_index(3));
    let id = host.ui.state.carousel;
    assert_eq!(state(&host), (-1200.0, 3, false));
    host.ui.tree.get_mut(id).unwrap().go_next();
    let trace = run(&mut host);
    assert_eq!(trace.len(), 33);
    for (frame, value) in [(0, -1200.0), (1, -1220.098), (16, -1488.124), (31, -1599.968)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (0.0, 0, false));

    // At once: no frames of a snap.
    host.ui.tree.get_mut(id).unwrap().scroll_to(2, false);
    host.frame_after(16.0);
    assert_eq!(state(&host), (-800.0, 2, false));
    host.ui.tree.get_mut(id).unwrap().go_prev();
    host.settle();
    assert_eq!(state(&host), (-400.0, 1, false));
}

#[test]
fn a_vertical_carousel_and_a_faster_swipe_speed() {
    let mut host = scene(1.0, |c| c.is_vertical(true));
    assert_eq!(carousel(&host).snap_points()[3], Point::new(0.0, -600.0));
    host.pan((200.0, 180.0), (200.0, 80.0), 60.0, 6);
    assert_eq!(state(&host), (-100.0, 1, true));
    let trace = run(&mut host);
    assert_eq!(trace.len(), 13);
    for (frame, value) in [(0, -112.533), (5, -168.455), (11, -199.803)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), (-200.0, 1, false));

    let mut host = scene(1.0, |c| c.swipe_speed(2));
    flick(&mut host, 1.0);
    let trace = run(&mut host);
    assert_eq!(trace.len(), 16);
    for (frame, value) in [(0, -130.109), (7, -316.093), (14, -399.408)] {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?}", trace[frame]);
    }
}

#[test]
fn the_points_are_the_same_at_scale_2() {
    let mut host = scene(2.0, |c| c);
    flick(&mut host, 2.0);
    assert_eq!(state(&host), (-100.0, 1, true));
    let trace = run(&mut host);
    assert_trace(&trace, &K1, (-400.0, 1, false));
    assert_eq!(host.ui.tree.base(host.ui.state.carousel).unwrap().content_offset, Point::new(-800.0, 0.0));
}

#[test]
fn a_dynamic_size_follows_the_selected_slide() {
    // As the React demo: in a column of a scroll, no height of its own.
    let ui = Ui::new(App::default(), |app: &mut App| {
        let slides: Vec<Build<SkiaShape>> = [80.0, 160.0, 110.0].iter().map(|h| SkiaShape::new().height_request(*h).background_color(Color::RED)).collect();
        let carousel = SkiaCarousel::new().dynamic_size(true).bounces(true).children(slides).assign(&mut app.carousel);
        SkiaScroll::new().fill().content(SkiaLayout::column().children((carousel,)))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    let id = host.ui.state.carousel;
    assert_eq!(host.rect(id).height(), 80.0);
    host.ui.tree.get_mut(id).unwrap().set_selected_index(1);
    host.settle();
    assert_eq!(host.rect(id).height(), 160.0);
    host.ui.tree.get_mut(id).unwrap().set_selected_index(2);
    host.settle();
    assert_eq!(host.rect(id).height(), 110.0);
}

#[test]
fn inside_a_scroll_the_carousel_takes_its_axis_and_leaves_the_other() {
    let scene = || {
        let ui = Ui::new(App::default(), |app: &mut App| {
            let carousel = SkiaCarousel::new().height_request(200).children(slides()).assign(&mut app.carousel);
            let below = SkiaShape::new().fill_x().height_request(1000);
            SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).children((carousel, below)))
        });
        let mut host = Headless::new(ui, 400, 300, 1.0);
        host.frame_after(16.0);
        host
    };
    let scroll_y = |host: &Host| host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y();
    // Down the page on the carousel: it lets go of the press, the page scrolls.
    let mut host = scene();
    host.pan((200.0, 150.0), (200.0, 50.0), 320.0, 8);
    assert!(close(scroll_y(&host), -97.794) && state(&host).0 == 0.0, "{} {:?}", scroll_y(&host), state(&host));
    host.settle();
    assert!(close(scroll_y(&host), -235.604) && state(&host).0 == 0.0, "{}", scroll_y(&host));
    // Along it: the carousel's, the page stays.
    let mut host = scene();
    flick(&mut host, 1.0);
    assert_eq!((scroll_y(&host), state(&host)), (0.0, (-100.0, 1, true)));
    host.settle();
    assert_eq!((scroll_y(&host), state(&host)), (0.0, (-400.0, 1, false)));
}

#[test]
fn a_tap_reaches_the_slide_and_moves_nothing() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let tapped = SkiaShape::new().background_color(Color::RED).on_tapped(|_me, app: &mut App, _cx| app.taps += 1);
        let carousel = SkiaCarousel::new().height_request(200).children((tapped, SkiaShape::new(), SkiaShape::new()));
        SkiaLayout::new().fill().children((carousel.assign(&mut app.carousel),))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    host.tap(200.0, 100.0);
    assert_eq!((host.ui.state.taps, state(&host)), (1, (0.0, 0, false)));
}

#[test]
fn templated_slides_are_bound_by_index_and_loop() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let carousel = SkiaCarousel::new().height_request(130).is_looped(true).sides_offset(30).spacing(10).items(
            |_app: &App| 12,
            || {
                let mut label = Handle::default();
                (SkiaShape::new().children((SkiaLabel::new("").assign(&mut label),)), label)
            },
            |label: &Handle<SkiaLabel>, _app: &App, index, cx| {
                cx.get_mut(*label).unwrap().set_text(format!("Item {}", index + 1));
            },
        );
        SkiaLayout::new().fill().children((carousel.assign(&mut app.carousel),))
    });
    let mut host = Headless::new(ui.font_bytes("Default", FONT), 400, 300, 1.0);
    host.settle();
    let id = host.ui.state.carousel;
    assert_eq!(carousel(&host).children_count(), 12);
    // 400 - 60 + 10: the step.
    assert_eq!(carousel(&host).snap_points()[1], Point::new(-350.0, 0.0));
    host.pan((100.0, 60.0), (200.0, 60.0), 60.0, 6);
    host.settle();
    assert_eq!(state(&host), (-3850.0, 11, false));
    host.ui.tree.get_mut(id).unwrap().go_next();
    host.settle();
    assert_eq!(state(&host), (0.0, 0, false));
}

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

// ---------------------------------------------------------------- what a frame costs

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

#[test]
fn a_snap_frame_lays_nothing_out_and_allocates_nothing() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let counted = || Build::new(Counter { measures: app.measures.clone(), arranges: app.arranges.clone() });
        let slides: Vec<Build<SkiaLayout>> = (0..4).map(|_| SkiaLayout::new().fill().children((counted(),))).collect();
        let carousel = SkiaCarousel::new().height_request(200).children(slides).assign(&mut app.carousel);
        SkiaLayout::new().fill().children((carousel,))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    flick(&mut host, 1.0);
    host.frame_after(16.0);
    let (measures, arranges) = (host.ui.state.measures.load(Ordering::Relaxed), host.ui.state.arranges.load(Ordering::Relaxed));
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..20 {
        host.frame_after(16.0);
    }
    let allocations = ALLOCATIONS.with(|a| a.get()) - before;
    assert!(carousel(&host).in_transition());
    assert_eq!((host.ui.state.measures.load(Ordering::Relaxed), host.ui.state.arranges.load(Ordering::Relaxed)), (measures, arranges));
    assert_eq!(allocations, 0);
}

/// The cost of a frame of a snap on the CPU canvas. `cargo test --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn snap_frame_cost() {
    let mut host = scene(2.0, |c| c);
    flick(&mut host, 2.0);
    let started = std::time::Instant::now();
    let mut frames = 0;
    while host.ui.needs_frame() && frames < 100 {
        host.frame_after(16.0);
        frames += 1;
    }
    println!("{frames} snap frames, {:.3} ms each", started.elapsed().as_secs_f64() * 1000.0 / frames as f64);
}
