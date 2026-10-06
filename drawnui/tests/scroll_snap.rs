//! `track_index_position`, `current_index` and `snap_to_children` of SkiaScroll, with the numbers
//! of a probe on the React engine (DrawnUi.React `dist` in node: its Canvas frame loop and pointer
//! handling copied, the robot of `Headless`, 16 ms frames) over the scenes of HelloMaui's
//! ScrollPage: a horizontal strip of 10 tiles and a column of 16 rows.
//!
//! React rules (GetIndexHit, Snap): the child under a point of the viewport, the point
//! `track_index_position_offset` into it for Start, its middle, or that far before its end; the
//! children as they are drawn, their padding included, a child that starts on the point counts,
//! one that ends there does not. A snap follows a pan, a fling, a bounce, the wheel and an animated
//! scroll from code, once per press, over 600 ms on the ScrollTo curve, and not for 2 points or
//! less. React reads the child under the point one scroll step late (the rects of the frame before);
//! here it is the child under the point where the content is.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    indexes: Vec<Option<usize>>,
}

type Host = Headless<App>;

fn host_of(width: f32, height: f32, scale: f32, scroll: Build<SkiaScroll>) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let scroll = scroll.on_index_changed(|_me, app: &mut App, _cx, index| app.indexes.push(index));
        SkiaLayout::new().fill().children((scroll.assign(&mut app.scroll),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), (width * scale) as i32, (height * scale) as i32, scale);
    host.settle();
    host
}

/// ScrollPage card 6: a 400 x 120 horizontal scroll over 10 tiles of 200 x 100, 12 apart, in a
/// row with a padding of 8. The tiles stand at 8, 220, 432, ...; 1724 points of travel.
fn strip(snap: SnapToChildrenType, track: RelativePositionType, scale: f32) -> Host {
    let tiles: Vec<Build<SkiaShape>> = (0..10).map(|_| SkiaShape::new().width_request(200).height_request(100).background_color(Color::RED)).collect();
    let scroll = SkiaScroll::new().orientation(ScrollOrientation::Horizontal).fill_x().height_request(120);
    host_of(400.0, 200.0, scale, scroll.snap_to_children(snap).track_index_position(track).content(SkiaLayout::row().spacing(12).padding(8).children(tiles)))
}

/// ScrollPage card 7: a 300 x 180 vertical scroll over 16 rows of 40, 6 apart, in a column with
/// a padding of 8. The rows stand at 8, 54, 100, ...; 566 points of travel.
fn column(snap: SnapToChildrenType, track: RelativePositionType, scale: f32) -> Host {
    let rows: Vec<Build<SkiaShape>> = (0..16).map(|_| SkiaShape::new().fill_x().height_request(40).background_color(Color::RED)).collect();
    let scroll = SkiaScroll::new().fill_x().height_request(180);
    let content = SkiaLayout::column().spacing(6).padding(8).fill_x().children(rows);
    host_of(300.0, 300.0, scale, scroll.snap_to_children(snap).track_index_position(track).content(content))
}

fn scroll(host: &Host) -> &SkiaScroll {
    host.ui.tree.find(host.ui.state.scroll).unwrap()
}

/// The offset along the scroll axis.
fn at(host: &Host) -> f32 {
    let scroll = scroll(host);
    if scroll.p.orientation == ScrollOrientation::Horizontal { scroll.viewport_offset_x() } else { scroll.viewport_offset_y() }
}

fn jump(host: &mut Host, to: f32) {
    let (id, horizontal) = (host.ui.state.scroll, scroll(host).p.orientation == ScrollOrientation::Horizontal);
    let (x, y) = if horizontal { (to, 0.0) } else { (0.0, to) };
    host.ui.tree.cx().scroll_to(id, x, y, 0);
    host.settle();
}

/// Frames until nothing moves, 16 ms apart: the offset after every frame, up to its last change.
fn run(host: &mut Host) -> Vec<f32> {
    let mut trace = Vec::new();
    while host.ui.needs_frame() && trace.len() < 900 {
        host.frame_after(16.0);
        trace.push(at(host));
    }
    while trace.len() > 1 && trace[trace.len() - 1] == trace[trace.len() - 2] {
        trace.pop();
    }
    trace
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.01
}

/// The trace has this length and ends with these values, the last of them the rest.
fn assert_ends(trace: &[f32], length: usize, expected: &[f32]) {
    assert_eq!(trace.len(), length, "{trace:?}");
    let tail = &trace[trace.len() - expected.len()..];
    for (i, (actual, expected)) in tail.iter().zip(expected).enumerate() {
        assert!(close(*actual, *expected), "{i} from the start of the tail: {actual} instead of {expected}; tail {tail:?}");
    }
}

fn assert_indexes(host: &mut Host, expected: &[(f32, Option<usize>)]) {
    for &(offset, index) in expected {
        jump(host, offset);
        assert_eq!(scroll(host).current_index(), index, "at {offset}");
    }
}

/// The probe's slow pan: 80 points to the left in 640 ms.
fn slow_pan(host: &mut Host) {
    host.pan((300.0, 60.0), (220.0, 60.0), 640.0, 16);
}

// ---------------------------------------------------------------- the tracked child

#[test]
fn the_center_of_a_strip_tracks_the_tile_under_it() {
    let mut host = strip(SnapToChildrenType::Disabled, RelativePositionType::Center, 1.0);
    // As React: looked up when the content moved, nothing before.
    assert_eq!((at(&host), scroll(&host).current_index()), (0.0, None));
    // The point is 200 into the viewport; the tiles are drawn at 8..208, 220..420, 432..632.
    let expected = [(-10.0, None), (-13.0, None), (-112.0, Some(1)), (-126.0, Some(1)), (-224.0, None), (-225.0, None)];
    assert_indexes(&mut host, &expected);
    assert_indexes(&mut host, &[(-300.0, Some(2)), (-1000.0, Some(5)), (-1716.0, Some(9)), (0.0, Some(0))]);
    // The handler saw every change.
    assert_eq!(host.ui.state.indexes, [Some(1), None, Some(2), Some(5), Some(9), Some(0)]);
}

#[test]
fn the_start_the_end_and_the_center_of_a_column_track_a_row() {
    // The rows as drawn: 8..48, 54..94, 100..140, ... Start is 8 points into the viewport.
    let mut host = column(SnapToChildrenType::Disabled, RelativePositionType::Start, 1.0);
    assert_eq!(scroll(&host).current_index(), None);
    let expected = [(-9.0, Some(0)), (-47.0, Some(1)), (-55.0, Some(1)), (-100.0, Some(2)), (-300.0, Some(6)), (-570.0, Some(12))];
    assert_indexes(&mut host, &expected);
    assert_eq!(at(&host), -566.0);

    // End: 8 points before the end of the 180 of the viewport; a row that ends on it does not count.
    let mut host = column(SnapToChildrenType::Disabled, RelativePositionType::End, 1.0);
    assert_indexes(&mut host, &[(-10.0, Some(3)), (-20.0, Some(4)), (-40.0, Some(4)), (-100.0, Some(5)), (-566.0, None)]);

    // Center: 90 into the viewport.
    let mut host = column(SnapToChildrenType::Disabled, RelativePositionType::Center, 1.0);
    assert_indexes(&mut host, &[(-10.0, Some(2)), (-40.0, Some(2)), (-100.0, None), (-566.0, Some(14))]);

    // The same points at scale 2.
    let mut host = column(SnapToChildrenType::Disabled, RelativePositionType::Start, 2.0);
    assert_indexes(&mut host, &[(-9.0, Some(0)), (-48.0, Some(1)), (-54.0, Some(1)), (-100.0, Some(2))]);
}

#[test]
fn without_a_tracked_point_there_is_no_current_child() {
    let mut host = column(SnapToChildrenType::Side, RelativePositionType::None, 1.0);
    assert_indexes(&mut host, &[(0.0, None), (-30.0, None), (-100.0, None)]);
    assert!(host.ui.state.indexes.is_empty());
    // Turned on later, it is looked up at the next scroll, as React does.
    let id = host.ui.state.scroll;
    host.ui.tree.get_mut(id).unwrap().set_track_index_position(RelativePositionType::Start);
    host.settle();
    assert_eq!(scroll(&host).current_index(), None);
    jump(&mut host, -101.0);
    assert_eq!(scroll(&host).current_index(), Some(2));
}

#[test]
fn a_pan_reports_every_row_that_passes_the_tracked_point() {
    let mut host = column(SnapToChildrenType::Disabled, RelativePositionType::Start, 1.0);
    // The probe: 100 points up in 640 ms, then the fling.
    host.fling((150.0, 150.0), (150.0, 50.0), 640.0, 16);
    assert_eq!(host.ui.state.indexes, [Some(0), None, Some(1), None, Some(2), None, Some(3)]);
    assert!(close(at(&host), -167.525), "{}", at(&host));
    assert_eq!(scroll(&host).current_index(), Some(3));
}

// ---------------------------------------------------------------- snapping

#[test]
fn a_slow_pan_lets_its_fling_end_and_then_centers_the_tile() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    slow_pan(&mut host);
    assert!(close(at(&host), -79.118), "{}", at(&host));
    let trace = run(&mut host);
    // The fling runs out at -133.929 (frame 96), the snap starts the frame after: the center of
    // tile 1 (320) goes to the center of the viewport over 600 ms.
    assert!(close(trace[96], -133.929) && close(trace[97], -133.929), "{:?}", &trace[94..99]);
    assert_ends(&trace, 136, &[-122.512, -122.265, -122.03, -121.805, -121.591, -121.388, -121.194, -121.008, -120.832, -120.664, -120.504, -120.351, -120.206, -120.067, -120.0]);
    assert_eq!((at(&host), scroll(&host).current_index()), (-120.0, Some(1)));
    assert!(!host.ui.needs_frame());
}

#[test]
fn a_pan_released_without_a_fling_snaps_at_once() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    jump(&mut host, -150.0);
    // The probe: 4 points left in 640 ms, too slow to fling.
    host.pan((300.0, 60.0), (296.0, 60.0), 640.0, 4);
    assert!(close(at(&host), -153.824), "{}", at(&host));
    assert!(!scroll(&host).is_user_panning());
    let trace = run(&mut host);
    assert!(close(trace[0], -151.923) && close(trace[1], -150.111), "{:?}", &trace[..3]);
    assert_ends(&trace, 38, &[-126.1, -125.5, -124.928, -124.384, -123.865, -123.37, -122.898, -122.449, -122.021, -121.612, -121.223, -120.853, -120.499, -120.162, -120.0]);
}

#[test]
fn a_fling_snaps_to_the_tile_it_ends_on() {
    // The probe: flicks of 100 ms; frames, where the strip rests, the tile there. No snap where the
    // fling ends between two tiles (120), or within 2 points of the place (180).
    for (distance, after, frames, rest, index) in [
        (60.0, -57.883, 168, -332.0, Some(2)),
        (90.0, -86.824, 177, -544.0, Some(3)),
        (120.0, -115.765, 144, -646.443, None),
        (150.0, -144.706, 187, -756.0, Some(4)),
        (180.0, -173.648, 152, -969.915, Some(5)),
    ] {
        let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
        host.pan((300.0, 60.0), (300.0 - distance, 60.0), 100.0, 5);
        assert!(close(at(&host), after), "flick {distance}: {}", at(&host));
        let trace = run(&mut host);
        assert!(trace.len() == frames && close(at(&host), rest), "flick {distance}: {} frames, at {}", trace.len(), at(&host));
        assert_eq!(scroll(&host).current_index(), index, "flick {distance}");
    }
    // The flick of 200 in 60 ms from the rest at -120: 202 frames to the center of tile 8.
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    host.fling((300.0, 60.0), (220.0, 60.0), 640.0, 16);
    assert_eq!(at(&host), -120.0);
    host.pan((300.0, 60.0), (100.0, 60.0), 60.0, 6);
    assert!(close(at(&host), -314.118), "{}", at(&host));
    let trace = run(&mut host);
    assert_ends(&trace, 202, &[-1610.78, -1610.113, -1609.478, -1608.873, -1608.296, -1607.746, -1607.222, -1606.722, -1606.246, -1605.792, -1605.36, -1604.948, -1604.555, -1604.181, -1604.0]);
    assert_eq!(scroll(&host).current_index(), Some(8));

    // The same flick from the start: every tile and gap that passes the center.
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    host.fling((300.0, 60.0), (100.0, 60.0), 60.0, 6);
    assert_eq!(at(&host), -1604.0);
    let seen = &host.ui.state.indexes;
    assert_eq!(seen.last(), Some(&Some(8)));
    assert!((1..=8).all(|i| seen.contains(&Some(i))), "{seen:?}");
}

#[test]
fn side_snaps_the_start_or_the_end_of_the_tile_to_the_tracked_point() {
    // Start: 8 points into the viewport. Tile 0 is there at the end of the fling; it goes back.
    let mut host = strip(SnapToChildrenType::Side, RelativePositionType::Start, 1.0);
    slow_pan(&mut host);
    let trace = run(&mut host);
    assert_ends(&trace, 136, &[-24.152, -21.778, -19.515, -17.358, -15.302, -13.343, -11.476, -9.697, -8.001, -6.385, -4.844, -3.376, -1.977, -0.643, 0.0]);
    assert_eq!(scroll(&host).current_index(), Some(0));
    // The flick of 200: tile 7 starts 8 points into the viewport.
    host.pan((300.0, 60.0), (100.0, 60.0), 60.0, 6);
    let trace = run(&mut host);
    assert_ends(&trace, 202, &[-1490.78, -1490.113, -1489.478, -1488.873, -1488.296, -1487.746, -1487.222, -1486.722, -1486.246, -1485.792, -1485.36, -1484.948, -1484.555, -1484.181, -1484.0]);
    assert_eq!(scroll(&host).current_index(), Some(7));

    // End: 392 into the viewport; the end of tile 2 goes there.
    let mut host = strip(SnapToChildrenType::Side, RelativePositionType::End, 1.0);
    slow_pan(&mut host);
    let trace = run(&mut host);
    assert_ends(&trace, 136, &[-220.872, -222.752, -224.545, -226.253, -227.881, -229.432, -230.911, -232.32, -233.663, -234.943, -236.163, -237.326, -238.434, -239.491, -240.0]);

    // Center with the point at the start: Center snaps the middle of the tile to the middle.
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Start, 1.0);
    slow_pan(&mut host);
    let trace = run(&mut host);
    assert_ends(&trace, 136, &[-122.512, -122.265, -122.03, -121.805, -121.591, -121.388, -121.194, -121.008, -120.832, -120.664, -120.504, -120.351, -120.206, -120.067, -120.0]);
}

#[test]
fn the_snap_is_the_same_points_at_scale_2() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 2.0);
    host.pan((600.0, 120.0), (440.0, 120.0), 640.0, 16);
    assert!(close(at(&host), -79.118), "{}", at(&host));
    let trace = run(&mut host);
    assert_ends(&trace, 136, &[-122.512, -122.265, -122.03, -121.805, -121.591, -121.388, -121.194, -121.008, -120.832, -120.664, -120.504, -120.351, -120.206, -120.067, -120.0]);
    assert_eq!((at(&host), scroll(&host).current_index()), (-120.0, Some(1)));
}

#[test]
fn a_jump_does_not_snap_the_wheel_and_an_animated_scroll_do() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    for offset in [-30.0, -130.0, -260.0, -1000.0, 0.0] {
        jump(&mut host, offset);
        assert_eq!(at(&host), offset);
    }
    // A notch goes to -150, then the snap centers tile 1.
    assert!(host.wheel(200.0, 60.0, -1.0));
    host.settle();
    assert_eq!((at(&host), scroll(&host).current_index()), (-120.0, Some(1)));
    host.tap(200.0, 60.0);
    assert_eq!(at(&host), -120.0);
    jump(&mut host, 0.0);
    let id = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(id, -150.0, 0.0, 300);
    host.settle();
    assert_eq!((at(&host), scroll(&host).current_index()), (-120.0, Some(1)));
}

#[test]
fn a_pull_past_the_start_comes_back_to_the_start() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    host.pan((100.0, 60.0), (200.0, 60.0), 640.0, 16);
    assert!(close(at(&host), 47.882), "{}", at(&host));
    host.settle();
    assert_eq!((at(&host), scroll(&host).current_index()), (0.0, Some(0)));
}

#[test]
fn a_press_takes_the_content_from_a_snap() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    slow_pan(&mut host);
    // During the fling (it runs to frame 96): a press stops it where it is.
    for _ in 0..42 {
        host.frame_after(16.0);
    }
    let time = host.time_ms();
    host.ui.pointer(drawnui::PointerKind::Down, 200.0, 60.0, time);
    for _ in 0..20 {
        host.frame_after(16.0);
    }
    let held = at(&host);
    assert!(close(held, -127.104), "{held}");
    // Let go without a move: nothing snaps.
    let time = host.time_ms();
    host.ui.pointer(drawnui::PointerKind::Up, 200.0, 60.0, time);
    host.settle();
    assert_eq!(at(&host), held);
}

#[test]
fn a_column_snaps_by_the_same_rules() {
    // The probe's pan: 30 points up in 640 ms.
    let pan = |host: &mut Host| {
        host.pan((150.0, 150.0), (150.0, 120.0), 640.0, 16);
        assert!(close(at(host), -29.669), "{}", at(host));
        run(host)
    };
    // Side + Start: row 1 is 8 points into the viewport at the end of the fling; its start goes there.
    let mut host = column(SnapToChildrenType::Side, RelativePositionType::Start, 1.0);
    let trace = pan(&mut host);
    assert_ends(&trace, 115, &[-46.7, -46.631, -46.566, -46.503, -46.444, -46.387, -46.333, -46.281, -46.232, -46.185, -46.14, -46.098, -46.057, -46.019, -46.0]);
    assert_eq!(scroll(&host).current_index(), Some(1));

    // Side + End: 172 into the viewport; the end of row 4 goes there.
    let mut host = column(SnapToChildrenType::Side, RelativePositionType::End, 1.0);
    let trace = pan(&mut host);
    assert_ends(&trace, 115, &[-58.175, -58.355, -58.526, -58.689, -58.844, -58.992, -59.133, -59.267, -59.396, -59.518, -59.634, -59.745, -59.851, -59.951, -60.0]);

    // Center + Center: the fling ends with the middle of the viewport on the end of row 2, drawn on
    // whole pixels: no row there, no snap.
    let mut host = column(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    let trace = pan(&mut host);
    assert!(trace.len() == 76 && close(at(&host), -49.882), "{} {}", trace.len(), at(&host));
    assert_eq!(scroll(&host).current_index(), None);

    // A pull past the start ends at the start.
    let mut host = column(SnapToChildrenType::Side, RelativePositionType::Start, 1.0);
    host.pan((150.0, 50.0), (150.0, 150.0), 640.0, 16);
    host.settle();
    assert_eq!(at(&host), 0.0);
}

#[test]
fn the_wheel_takes_a_fling_over_and_its_step_snaps() {
    let mut host = strip(SnapToChildrenType::Center, RelativePositionType::Center, 1.0);
    host.pan((300.0, 60.0), (100.0, 60.0), 60.0, 6);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert!(close(at(&host), -477.854), "{}", at(&host));
    // One notch back: the C# step, 150 points from where the content is (React: from where the
    // fling was going, then tile 7 at -1392), then the snap to the tile there.
    assert!(host.wheel(200.0, 60.0, 1.0));
    host.settle();
    assert_eq!((at(&host), scroll(&host).current_index()), (-332.0, Some(2)));
}
