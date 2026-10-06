//! SkiaDrawer with the numbers of a probe on the React engine (DrawnUi.React `dist` in node: its
//! Canvas frame loop and pointer handling copied, the robot of `Headless`, 16 ms frames). Scene: a
//! 400 x 600 canvas, a layer over it, and in it a drawer from the bottom, 320 tall, 56 of header.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    drawer: Handle<SkiaDrawer>,
    opened: Vec<bool>,
    completed: Vec<bool>,
    inside: u32,
    below: u32,
}

type Host = Headless<App>;

fn scene(setup: impl FnOnce(Build<SkiaDrawer>) -> Build<SkiaDrawer>) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let below = SkiaShape::new().fill().on_tapped(|_me, app: &mut App, _cx| app.below += 1);
        let content = SkiaShape::new().fill().background_color(Color::BLUE).on_tapped(|_me, app: &mut App, _cx| app.inside += 1);
        let drawer = SkiaDrawer::new()
            .header_size(56)
            .height_request(320)
            .vertical_options(LayoutOptions::End)
            .on_is_open_changed(|_me, app: &mut App, _cx, open| app.opened.push(open))
            .on_state_transition_complete(|_me, app: &mut App, _cx, open| app.completed.push(open))
            .children((content,));
        let drawer = setup(drawer).assign(&mut app.drawer);
        SkiaLayout::new().fill().children((below, SkiaLayout::new().fill().children((drawer,))))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 600, 1.0);
    host.settle();
    host
}

fn drawer(host: &Host) -> &SkiaDrawer {
    host.ui.tree.find(host.ui.state.drawer).unwrap()
}

/// The translation along its axis, open, in transition.
fn state(host: &Host) -> (f32, bool, bool) {
    let (d, p) = (drawer(host), &host.ui.tree.base(host.ui.state.drawer).unwrap().p);
    let horizontal = matches!(d.p.direction, DrawerDirection::FromLeft | DrawerDirection::FromRight);
    (if horizontal { p.translation_x } else { p.translation_y }, d.is_open(), d.in_transition())
}

fn run(host: &mut Host) -> Vec<(f32, bool, bool)> {
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

fn assert_frames(trace: &[(f32, bool, bool)], length: usize, expected: &[(usize, f32)], end: (f32, bool, bool)) {
    assert_eq!(trace.len(), length, "{trace:?}");
    for &(frame, value) in expected {
        assert!(close(trace[frame].0, value), "frame {frame}: {:?} instead of {value}", trace[frame]);
    }
    assert_eq!(*trace.last().unwrap(), end);
}

#[test]
fn a_flick_opens_and_closes_it_on_the_spring() {
    let mut host = scene(|d| d.bounces(true));
    // Closed: 320 - 56 below its place.
    assert_eq!(drawer(&host).snap_points(), [Point::new(0.0, 0.0), Point::new(0.0, 264.0)]);
    assert_eq!((state(&host), host.rect(host.ui.state.drawer)), ((264.0, false, false), Rect::new(0.0, 280.0, 400.0, 600.0)));

    // Up on the header, 100 points in 60 ms.
    host.pan((200.0, 570.0), (200.0, 470.0), 60.0, 6);
    assert_eq!(state(&host), (164.0, false, true));
    let trace = run(&mut host);
    let expected = [(0, 137.737), (1, 113.332), (5, 44.929), (10, 11.237), (15, 2.262), (21, 0.22)];
    assert_frames(&trace, 23, &expected, (0.0, true, false));
    // Open is told when it arrived, not on the way (React: no report while in transition).
    assert_eq!((host.ui.state.opened.clone(), host.ui.state.completed.clone()), (vec![true], vec![true]));

    // Down on the open drawer.
    host.pan((200.0, 300.0), (200.0, 400.0), 60.0, 6);
    assert_eq!(state(&host), (100.0, true, true));
    let trace = run(&mut host);
    let expected = [(0, 126.263), (5, 219.071), (10, 252.763), (21, 263.78)];
    assert_frames(&trace, 23, &expected, (264.0, false, false));
    assert_eq!((host.ui.state.opened.clone(), host.ui.state.completed.clone()), (vec![true, false], vec![true, false]));
}

#[test]
fn a_slow_drag_goes_to_the_nearest_state() {
    // 150 points in 1200 ms: at 114 the open state is nearer.
    let mut host = scene(|d| d.bounces(true));
    host.pan((200.0, 570.0), (200.0, 420.0), 1200.0, 16);
    assert_eq!(state(&host), (114.0, false, true));
    let trace = run(&mut host);
    let expected = [(0, 108.038), (1, 97.01), (5, 47.294), (10, 13.762), (22, 0.272)];
    assert_frames(&trace, 24, &expected, (0.0, true, false));
}

#[test]
fn code_opens_and_closes_it() {
    let mut host = scene(|d| d.bounces(true));
    let id = host.ui.state.drawer;
    host.ui.tree.get_mut(id).unwrap().open();
    let trace = run(&mut host);
    // The spring starts with 1500 points per second (React GetAutoVelocity).
    let expected = [(0, 264.0), (1, 235.794), (2, 203.302), (10, 32.863), (25, 0.17)];
    assert_frames(&trace, 27, &expected, (0.0, true, false));
    // Told at once when code sets it.
    assert_eq!((host.ui.state.opened.clone(), host.ui.state.completed.clone()), (vec![true], vec![true]));

    host.ui.tree.get_mut(id).unwrap().close();
    let trace = run(&mut host);
    let expected = [(0, 0.0), (1, 28.206), (10, 231.137), (25, 263.83)];
    assert_frames(&trace, 27, &expected, (264.0, false, false));
    assert_eq!(host.ui.state.completed, [true, false]);
}

#[test]
fn without_bounces_it_eases_and_does_not_pull_past_open() {
    let mut host = scene(|d| d);
    host.pan((200.0, 570.0), (200.0, 470.0), 60.0, 6);
    assert_eq!(state(&host), (164.0, false, true));
    let trace = run(&mut host);
    let expected = [(0, 162.657), (1, 153.254), (2, 127.733), (3, 78.157), (4, 31.92), (5, 8.854), (6, 0.9)];
    assert_frames(&trace, 8, &expected, (0.0, true, false));

    let mut host = scene(|d| d);
    let id = host.ui.state.drawer;
    host.ui.tree.get_mut(id).unwrap().set_is_open(true);
    let trace = run(&mut host);
    let expected = [(0, 264.0), (1, 262.424), (4, 163.117), (8, 0.665)];
    assert_frames(&trace, 10, &expected, (0.0, true, false));
}

#[test]
fn a_drawer_from_the_left() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let drawer = SkiaDrawer::new()
            .direction(DrawerDirection::FromLeft)
            .header_size(40)
            .width_request(300)
            .horizontal_options(LayoutOptions::Start)
            .vertical_options(LayoutOptions::Fill)
            .bounces(true)
            .children((SkiaShape::new().fill(),))
            .assign(&mut app.drawer);
        SkiaLayout::new().fill().children((SkiaLayout::new().fill().children((drawer,)),))
    });
    let mut host = Headless::new(ui, 400, 600, 1.0);
    host.settle();
    assert_eq!(state(&host), (-260.0, false, false));
    host.pan((20.0, 300.0), (120.0, 300.0), 60.0, 6);
    assert_eq!(state(&host), (-160.0, false, true));
    let trace = run(&mut host);
    let expected = [(0, -133.894), (5, -43.201), (10, -10.729), (21, -0.205)];
    assert_frames(&trace, 23, &expected, (0.0, true, false));
}

#[test]
fn taps_reach_its_content_and_pass_by_it() {
    let mut host = scene(|d| d.bounces(true));
    // On the header of the closed drawer: its content.
    host.tap(200.0, 570.0);
    assert_eq!((host.ui.state.inside, host.ui.state.below), (1, 0));
    // Above it: what is under the layer.
    host.tap(200.0, 100.0);
    assert_eq!((host.ui.state.inside, host.ui.state.below), (1, 1));
    assert_eq!(state(&host), (264.0, false, false));
}
