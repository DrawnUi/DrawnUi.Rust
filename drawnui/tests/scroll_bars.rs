//! Scroll bars of SkiaScroll. The thumb positions, the squash past an edge and the drag numbers
//! are read from the C# engine: a probe on DrawnUi.Net's headless host with the same scene (a
//! 300 x 400 scroll over 2000 points, a red thumb), scanning the pixels of the bar. Upstream
//! counts the delay before a bar hides on real time; here it runs on the frame clock.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

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
    bar: Handle<SkiaScrollBar>,
    taps: u32,
    scrolled: u32,
}

type Host = Headless<App>;

/// The scene of the probe: a 300 x 400 point scroll on white over content `length` long, with a
/// red bar that does not hide by itself unless `bar` says so.
fn scene(length: f32, scale: f32, horizontal: bool, bar: impl FnOnce(Build<SkiaScrollBar>) -> Build<SkiaScrollBar>) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let content = match horizontal {
            true => SkiaShape::new().width_request(length).fill_y(),
            false => SkiaShape::new().height_request(length).fill_x(),
        };
        let content = content.on_tapped(|_me, app: &mut App, _cx| app.taps += 1);
        let bar = bar(SkiaScrollBar::new().auto_hide(false)).assign(&mut app.bar);
        let orientation = if horizontal { ScrollOrientation::Horizontal } else { ScrollOrientation::Vertical };
        let scroll = SkiaScroll::new().fill().orientation(orientation).scroll_bar_thumb_color(Color::RED).assign(&mut app.scroll);
        let scroll = scroll.on_scrolled(|_me, app: &mut App, _cx, _offset| app.scrolled += 1).content(content);
        if horizontal { scroll.scroll_bar_horizontal(bar) } else { scroll.scroll_bar(bar) }
    });
    let mut host = Headless::new(ui.background(Color::WHITE), (300.0 * scale) as i32, (400.0 * scale) as i32, scale);
    host.settle();
    host
}

fn scroll(host: &Host) -> &SkiaScroll {
    host.ui.tree.find(host.ui.state.scroll).unwrap()
}

fn y(host: &Host) -> f32 {
    scroll(host).viewport_offset_y()
}

/// How visible the bar is: the opacity of the upstream bar.
fn shown(host: &Host) -> f32 {
    host.ui.tree.find::<SkiaScrollBar>(host.ui.state.bar).unwrap().shown()
}

fn scroll_to(host: &mut Host, x: f32, y: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, x, y, 0);
    host.settle();
}

/// The runs of thumb pixels (red) down the column `at`, or along the row `at`. The format of the probe.
fn thumb(host: &mut Host, at: i32, down: bool, length: i32) -> String {
    let red = |c: Color| c.r() > 150 && c.g() < 100 && c.b() < 100;
    let (mut runs, mut start) = (Vec::new(), None);
    for i in 0..=length {
        let is_red = i < length && red(if down { host.pixel(at, i) } else { host.pixel(i, at) });
        match (is_red, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                runs.push(format!("{from}-{i}"));
                start = None;
            }
            _ => {}
        }
    }
    if runs.is_empty() { "none".into() } else { runs.join(" ") }
}

fn press(host: &mut Host, kind: PointerKind, x: f32, y: f32) {
    let time = host.time_ms();
    host.ui.pointer(kind, x, y, time);
    host.frame_after(16.0);
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.005
}

// ---------------------------------------------------------------- where the thumb is

#[test]
fn the_thumb_shows_where_the_viewport_is_and_its_share_of_the_content() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar);
    // Hidden until the first scroll.
    assert_eq!((shown(&host), thumb(&mut host, 295, true, 400)), (0.0, "none".to_owned()));
    scroll_to(&mut host, 0.0, -1.0);
    assert_eq!(shown(&host), 1.0);
    // 400 of 2000 points: a thumb of 80 on a track of 400, 320 to go for 1600 of scrolling.
    for (offset, run) in [(-400.0, "80-160"), (-800.0, "160-240"), (-1200.0, "240-320"), (-1600.0, "320-400")] {
        scroll_to(&mut host, 0.0, offset);
        assert_eq!((shown(&host), thumb(&mut host, 295, true, 400)), (1.0, run.to_owned()), "at {offset}");
    }
    // 4 points thick, 2 from the right edge.
    scroll_to(&mut host, 0.0, -800.0);
    assert_eq!(thumb(&mut host, 200, false, 300), "294-298");
}

#[test]
fn the_thumb_is_never_shorter_than_its_minimum() {
    let mut host = scene(20000.0, 1.0, false, |bar| bar);
    // 400 of 20000 would be 8 points: it is 32, and travels the 368 that are left.
    scroll_to(&mut host, 0.0, -9800.0);
    assert_eq!(thumb(&mut host, 295, true, 400), "184-216");
    scroll_to(&mut host, 0.0, -19600.0);
    assert_eq!(thumb(&mut host, 295, true, 400), "368-400");
}

#[test]
fn content_that_fits_has_no_bar() {
    let mut host = scene(300.0, 1.0, false, |bar| bar);
    scroll_to(&mut host, 0.0, -100.0);
    host.wheel(150.0, 200.0, -1.0);
    host.settle();
    assert_eq!((y(&host), shown(&host), thumb(&mut host, 295, true, 400)), (0.0, 0.0, "none".to_owned()));
}

#[test]
fn dock_thickness_and_edge_margin_place_the_bar() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar.dock(ScrollBarDock::Start).thickness(10).edge_margin(5));
    scroll_to(&mut host, 0.0, -800.0);
    assert_eq!(thumb(&mut host, 8, true, 400), "160-240");
    assert_eq!(thumb(&mut host, 200, false, 300), "5-15");
}

#[test]
fn a_horizontal_scroll_has_its_bar_at_the_bottom() {
    let mut host = scene(2000.0, 1.0, true, |bar| bar);
    // 300 of 2000: a thumb of 45; at 800 of 1700 it starts at 120.
    scroll_to(&mut host, -800.0, 0.0);
    assert_eq!(thumb(&mut host, 395, false, 300), "120-165");
    assert_eq!(thumb(&mut host, 150, true, 400), "394-398");
}

#[test]
fn the_bar_is_the_same_points_at_scale_2() {
    let mut host = scene(2000.0, 2.0, false, |bar| bar);
    scroll_to(&mut host, 0.0, -800.0);
    assert_eq!(thumb(&mut host, 590, true, 800), "320-480");
    assert_eq!(thumb(&mut host, 400, false, 600), "588-596");
}

#[test]
fn content_pulled_past_an_edge_squashes_the_thumb() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar);
    // Four moves of 20 points down at the start: the rubber band (React: over the 400 points of
    // the viewport) lets the content go 38.058.
    press(&mut host, PointerKind::Down, 150.0, 100.0);
    for step in 1..=4 {
        press(&mut host, PointerKind::Move, 150.0, 100.0 + step as f32 * 20.0);
    }
    assert!(close(y(&host), 38.058), "{}", y(&host));
    // 80 - 38.06 long, still at the start of the track.
    assert_eq!(thumb(&mut host, 295, true, 400), "0-42");
    press(&mut host, PointerKind::Up, 150.0, 180.0);
    host.settle();

    scroll_to(&mut host, 0.0, -1600.0);
    press(&mut host, PointerKind::Down, 150.0, 300.0);
    for step in 1..=4 {
        press(&mut host, PointerKind::Move, 150.0, 300.0 - step as f32 * 20.0);
    }
    assert!(close(y(&host), -1638.058), "{}", y(&host));
    assert_eq!(thumb(&mut host, 295, true, 400), "358-400");
    press(&mut host, PointerKind::Up, 150.0, 100.0);
    host.settle();
    assert_eq!((y(&host), thumb(&mut host, 295, true, 400)), (-1600.0, "320-400".to_owned()));
}

#[test]
fn the_scroll_gives_its_colors_to_a_default_bar() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let content = SkiaShape::new().height_request(2000).fill_x();
        let scroll = SkiaScroll::new().fill().scroll_bars_visibility(ScrollBarVisibility::Vertical);
        scroll.scroll_bar_thumb_color(Color::RED).scroll_bar_track_color(Color::BLUE).assign(&mut app.scroll).content(content)
    });
    let mut host = Headless::new(ui.background(Color::WHITE), 300, 400, 1.0);
    host.settle();
    host.wheel(150.0, 200.0, -1.0);
    // 150 points: the thumb of 80 is 30 down its track, the track shows where the thumb is not.
    for _ in 0..600 {
        if !scroll(&host).is_animating() {
            break;
        }
        host.frame_after(16.0);
    }
    assert_eq!(y(&host), -150.0);
    assert_eq!(thumb(&mut host, 295, true, 400), "30-110");
    assert_eq!((host.pixel(295, 20), host.pixel(295, 200), host.pixel(290, 200)), (Color::BLUE, Color::BLUE, Color::WHITE));

    // Another color later goes into the bar too.
    let scroll = host.ui.state.scroll;
    host.ui.tree.get_mut(scroll).unwrap().set_scroll_bar_thumb_color(Color::BLUE);
    host.frame_after(16.0);
    assert_eq!(thumb(&mut host, 295, true, 400), "none");
    assert_eq!(host.pixel(295, 50), Color::BLUE);
}

// ---------------------------------------------------------------- shown and hidden

#[test]
fn a_bar_shows_while_scrolling_and_fades_out_a_second_after() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar.auto_hide(true));
    assert!(!host.ui.needs_frame());
    host.wheel(150.0, 200.0, -1.0);
    assert_eq!(shown(&host), 1.0);
    for _ in 0..600 {
        if !scroll(&host).is_animating() {
            break;
        }
        host.frame_after(16.0);
        assert_eq!(shown(&host), 1.0);
    }
    assert_eq!(y(&host), -150.0);
    // The motion ended in this frame. `hide_delay_secs` = 1: 62 frames of 16 ms later still there.
    for frame in 1..=62 {
        host.frame_after(16.0);
        assert_eq!(shown(&host), 1.0, "frame {frame}");
    }
    // Then `hide_duration_secs` = 0.25 of fading: 8 ms into it, 136 ms into it, and gone.
    host.frame_after(16.0);
    assert!(close(shown(&host), 0.968), "{}", shown(&host));
    for _ in 0..8 {
        host.frame_after(16.0);
    }
    assert!(close(shown(&host), 0.456), "{}", shown(&host));
    // Half gone: the red over white is a light red.
    let faded = host.pixel(295, 60);
    assert!(faded.r() == 255 && faded.g() > 100 && faded.g() < 180, "{faded:?}");
    for _ in 0..8 {
        host.frame_after(16.0);
    }
    assert_eq!((shown(&host), thumb(&mut host, 295, true, 400)), (0.0, "none".to_owned()));
    // Nothing is left to do after the frame that every change is followed by.
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());

    // Scrolling again within the delay starts the delay over.
    scroll_to(&mut host, 0.0, -400.0);
    assert_eq!(shown(&host), 0.0, "settle runs through the delay");
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, -800.0, 0);
    for _ in 0..40 {
        host.frame_after(16.0);
    }
    host.ui.tree.cx().scroll_to(scroll, 0.0, -900.0, 0);
    for _ in 0..40 {
        host.frame_after(16.0);
    }
    assert_eq!(shown(&host), 1.0);
}

#[test]
fn a_bar_waiting_to_hide_asks_for_no_frames() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar.auto_hide(true));
    host.wheel(150.0, 200.0, -1.0);
    for _ in 0..600 {
        if !scroll(&host).is_animating() {
            break;
        }
        host.frame_after(16.0);
    }
    let rest = host.time_ms();
    // The frame that follows every change; then the scroll sleeps until the bar must hide.
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!((host.ui.wake_at(), shown(&host)), (Some(rest + 1000.0), 1.0));
    // A frame somebody else asks for meanwhile changes nothing.
    host.frame_after(500.0);
    assert!(!host.ui.needs_frame());
    assert_eq!((host.ui.wake_at(), shown(&host)), (Some(rest + 1000.0), 1.0));
    // A scroll wakes it, and the wait starts over where that one comes to rest.
    host.wheel(150.0, 200.0, -1.0);
    assert!(host.ui.needs_frame());
    for _ in 0..600 {
        if !scroll(&host).is_animating() {
            break;
        }
        host.frame_after(16.0);
    }
    let rest = host.time_ms();
    host.frame_after(16.0);
    assert_eq!((host.ui.needs_frame(), host.ui.wake_at()), (false, Some(rest + 1000.0)));
    // At the hide time it fades, a frame each, and is done: 1 s of nothing, 0.25 s of frames.
    let frames_before = host.time_ms();
    host.settle();
    assert_eq!((shown(&host), host.ui.needs_frame(), host.ui.wake_at()), (0.0, false, None));
    let took = host.time_ms() - rest;
    assert!((1250.0..1300.0).contains(&took), "{took} ms after the rest, from {frames_before}");
}

#[test]
fn a_bar_that_does_not_hide_costs_no_frames_after_the_scroll() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar);
    host.wheel(150.0, 200.0, -1.0);
    for _ in 0..600 {
        if !scroll(&host).is_animating() {
            break;
        }
        host.frame_after(16.0);
    }
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(shown(&host), 1.0);
}

/// The scene of upstream ScrollBarHoldTests: a 300 x 300 canvas, a scroll 200 tall with a default
/// vertical bar over a button and 800 points, the bar hiding 0.05 s after a scroll, at once.
fn hold_scene() -> Host {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let button = SkiaButton::new("Go").width_request(100).height_request(40);
        let content = SkiaLayout::column().spacing(0).children((button, SkiaLayout::new().height_request(800)));
        let scroll = SkiaScroll::new().fill_x().height_request(200).scroll_bars_visibility(ScrollBarVisibility::Vertical);
        SkiaLayout::layer().fill().children((scroll.assign(&mut app.scroll).content(content),))
    });
    let mut host = Headless::new(ui.font_bytes("Default", FONT).background(Color::BLACK), 300, 300, 1.0);
    host.settle();
    // The default bar is a child of the scroll.
    let children = host.ui.tree.children(host.ui.state.scroll).to_vec();
    let bar = children.into_iter().find(|child| host.ui.tree.find::<SkiaScrollBar>(*child).is_some()).expect("a default bar");
    let mut bar = host.ui.tree.find_mut::<SkiaScrollBar>(bar).unwrap();
    bar.set_hide_delay_secs(0.05);
    bar.set_hide_duration_secs(0);
    host
}

fn hold_shown(host: &Host) -> f32 {
    let children = host.ui.tree.children(host.ui.state.scroll);
    children.iter().find_map(|child| host.ui.tree.find::<SkiaScrollBar>(*child)).unwrap().shown()
}

/// Upstream KeyboardFocusInside_And_KeepScrollBarsVisible_HoldTheBar, the part this engine has:
/// there is no keyboard focus and no pointer-over here yet.
#[test]
fn keep_scroll_bars_visible_holds_the_bar() {
    let mut host = hold_scene();
    assert_eq!(hold_shown(&host), 0.0);

    let scroll = host.ui.state.scroll;
    host.ui.tree.get_mut(scroll).unwrap().set_keep_scroll_bars_visible(true);
    // It fades in over 150 ms.
    host.frame_after(16.0);
    host.frame_after(16.0);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert!(close(hold_shown(&host), 80.0 / 150.0), "{}", hold_shown(&host));
    host.settle();
    assert_eq!(hold_shown(&host), 1.0);
    // Held: also long after the delay, and a scroll does not start the countdown.
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    host.wheel(150.0, 100.0, -1.0);
    host.settle();
    assert_eq!(hold_shown(&host), 1.0);

    host.ui.tree.get_mut(scroll).unwrap().set_keep_scroll_bars_visible(false);
    host.settle();
    assert_eq!(hold_shown(&host), 0.0);
}

#[test]
fn a_scroll_shows_the_bar_of_the_hold_scene_and_it_hides_again() {
    let mut host = hold_scene();
    host.wheel(150.0, 100.0, -1.0);
    assert_eq!(hold_shown(&host), 1.0);
    host.settle();
    assert_eq!(hold_shown(&host), 0.0);
}

// ---------------------------------------------------------------- dragging

#[test]
fn a_draggable_thumb_moves_the_content_and_a_press_on_the_track_jumps_there() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar.is_draggable(true));
    // The thumb is 0..80. Taken at 40, nothing moves; the bar shows at once.
    press(&mut host, PointerKind::Down, 295.0, 40.0);
    assert_eq!((y(&host), shown(&host)), (0.0, 1.0));
    // 100 down: 100 of the 320 the thumb can go, of 1600.
    press(&mut host, PointerKind::Move, 295.0, 140.0);
    assert_eq!((y(&host), thumb(&mut host, 295, true, 400)), (-500.0, "100-180".to_owned()));
    // Past the end of the track: the end of the content, not further.
    press(&mut host, PointerKind::Move, 295.0, 500.0);
    assert_eq!(y(&host), -1600.0);
    press(&mut host, PointerKind::Up, 295.0, 500.0);
    host.settle();
    assert_eq!((y(&host), scroll(&host).is_animating()), (-1600.0, false));
    assert!(host.ui.state.scrolled >= 2);

    // A press on the track puts the middle of the thumb there: 260 of 320.
    scroll_to(&mut host, 0.0, 0.0);
    press(&mut host, PointerKind::Down, 295.0, 300.0);
    assert_eq!((y(&host), thumb(&mut host, 295, true, 400)), (-1300.0, "260-340".to_owned()));
    press(&mut host, PointerKind::Move, 295.0, 200.0);
    assert_eq!(y(&host), -800.0);
    press(&mut host, PointerKind::Up, 295.0, 200.0);
    host.settle();
    assert_eq!(y(&host), -800.0);
    // Neither the press nor the drag was a tap or a pan of the content.
    assert_eq!(host.ui.state.taps, 0);

    // `grab_padding` = 8: the track is 294..298, a press at 287 is on the bar, one at 284 is not.
    scroll_to(&mut host, 0.0, 0.0);
    host.tap(287.0, 300.0);
    assert_eq!((y(&host), host.ui.state.taps), (-1300.0, 0));
    scroll_to(&mut host, 0.0, 0.0);
    host.tap(284.0, 300.0);
    assert_eq!((y(&host), host.ui.state.taps), (0.0, 1));
}

#[test]
fn a_bar_that_is_not_draggable_lets_every_gesture_through() {
    let mut host = scene(2000.0, 1.0, false, |bar| bar);
    // The pan of the probe, on the bar: the content pans as anywhere else.
    host.pan((295.0, 300.0), (295.0, 200.0), 320.0, 8);
    assert!(close(y(&host), -97.79), "{}", y(&host));
    host.settle();
    host.tap(295.0, 300.0);
    assert_eq!(host.ui.state.taps, 1);
}

#[test]
fn a_horizontal_bar_drags_along_x() {
    let mut host = scene(2000.0, 1.0, true, |bar| bar.is_draggable(true));
    // The thumb is 45 of 300; held by its middle at 150: 127.5 of the 255 it can go, of 1700.
    press(&mut host, PointerKind::Down, 150.0, 396.0);
    assert_eq!(scroll(&host).viewport_offset_x(), -850.0);
    press(&mut host, PointerKind::Move, 300.0, 396.0);
    assert_eq!(scroll(&host).viewport_offset_x(), -1700.0);
    press(&mut host, PointerKind::Up, 300.0, 396.0);
}

// ---------------------------------------------------------------- what a bar costs

#[test]
fn a_bar_changes_no_layout_and_allocates_nothing_while_scrolling() {
    let mut host = scene(30000.0, 1.0, false, |bar| bar.auto_hide(true).track_color(Color::BLUE));
    let measured = |host: &Host| host.ui.tree.base(host.ui.state.scroll).unwrap().measured;
    let before_size = measured(&host);
    host.pan((150.0, 350.0), (150.0, 150.0), 60.0, 6);
    host.frame_after(16.0);
    host.frame_after(16.0);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let allocations = ALLOCATIONS.with(|a| a.get()) - before;
    assert!(scroll(&host).is_animating());
    assert_eq!(allocations, 0);
    // Through the wait and the fade too.
    host.settle();
    let before = ALLOCATIONS.with(|a| a.get());
    host.wheel(150.0, 200.0, -1.0);
    let wheel = ALLOCATIONS.with(|a| a.get()) - before;
    host.settle();
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, wheel);
    assert_eq!((shown(&host), measured(&host)), (0.0, before_size));
}
