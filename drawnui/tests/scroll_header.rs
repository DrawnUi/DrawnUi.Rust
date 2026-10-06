//! Header and Footer of SkiaScroll: in the flow, sticky, behind the content, with parallax. The
//! expected pictures are read from the C# engine: a probe on DrawnUi.Net's headless host with the
//! same scene, scanning one column of pixels after each scroll.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const GREEN: Color = Color::from_rgb(0, 128, 0);

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    /// 0 = the header, 1 = the footer, 100 + i = row i.
    tapped: Vec<usize>,
}

type Host = Headless<App>;

/// The scene of the probe: a 300 x 500 black canvas; at y = 50 a white scroll 400 tall over 20
/// rows of 50 (the first yellow, the rest blue), with a red header of 70 and a green footer of 50.
fn scene(header: bool, footer: bool, setup: impl FnOnce(Build<SkiaScroll>) -> Build<SkiaScroll>) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let rows: Vec<Build<SkiaShape>> = (0..20)
            .map(|i| {
                let row = SkiaShape::new().fill_x().height_request(50).background_color(if i == 0 { Color::YELLOW } else { Color::BLUE });
                row.on_tapped(move |_me, app: &mut App, _cx| app.tapped.push(100 + i))
            })
            .collect();
        let mut scroll = SkiaScroll::new().fill_x().height_request(400).margin((0, 50, 0, 0)).background_color(Color::WHITE);
        if header {
            let red = SkiaShape::new().fill_x().height_request(70).background_color(Color::RED);
            scroll = scroll.header(red.on_tapped(|_me, app: &mut App, _cx| app.tapped.push(0)));
        }
        if footer {
            let green = SkiaShape::new().fill_x().height_request(50).background_color(GREEN);
            scroll = scroll.footer(green.on_tapped(|_me, app: &mut App, _cx| app.tapped.push(1)));
        }
        let scroll = setup(scroll).assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).children(rows));
        SkiaLayout::new().fill().children((scroll,))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    host
}

fn y(host: &Host) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

fn scroll_to(host: &mut Host, y: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, y, 0);
    host.settle();
}

/// The colors down the column x = 150 as runs: R header, Y first row, B rows, G footer, W the
/// scroll's own background, K the canvas, ? a blended edge. The format of the probe.
fn column(host: &mut Host) -> String {
    let name = |c: Color| match (c.r(), c.g(), c.b()) {
        (r, g, b) if r > 200 && g < 80 && b < 80 => 'R',
        (r, g, b) if r > 200 && g > 200 && b < 80 => 'Y',
        (r, g, b) if b > 200 && r < 80 && g < 80 => 'B',
        (r, g, b) if g > 100 && r < 80 && b < 80 => 'G',
        (r, g, b) if r > 200 && g > 200 && b > 200 => 'W',
        (r, g, b) if r < 40 && g < 40 && b < 40 => 'K',
        _ => '?',
    };
    let mut runs = Vec::new();
    let (mut current, mut start) = (' ', 0);
    for y in 0..=500 {
        let color = if y < 500 { name(host.pixel(150, y)) } else { ' ' };
        if color != current {
            if current != ' ' {
                runs.push(format!("{current}{start}-{y}"));
            }
            (current, start) = (color, y);
        }
    }
    runs.join(" ")
}

/// Scrolls to each offset and compares the column; the last offset asked for is the end.
fn assert_columns(host: &mut Host, end: f32, expected: &[(f32, &str)]) {
    scroll_to(host, -100_000.0);
    assert_eq!(y(host), end, "the end of the scrolled length");
    for &(offset, picture) in expected {
        scroll_to(host, offset);
        assert_eq!(column(host), picture, "at {offset}");
    }
}

const IN_FLOW: [(f32, &str); 5] = [
    (0.0, "K0-50 R50-120 Y120-170 B170-450 K450-500"),
    (-30.0, "K0-50 R50-90 Y90-140 B140-450 K450-500"),
    (-100.0, "K0-50 Y50-70 B70-450 K450-500"),
    (-300.0, "K0-50 B50-450 K450-500"),
    (-720.0, "K0-50 B50-400 G400-450 K450-500"),
];

#[test]
fn header_and_footer_in_the_flow_scroll_with_the_content_and_add_to_its_length() {
    // 1000 points of rows in 400: 600 of travel without them.
    let mut host = scene(false, false, |scroll| scroll);
    assert_columns(&mut host, -600.0, &[(0.0, "K0-50 Y50-100 B100-450 K450-500"), (-600.0, "K0-50 B50-450 K450-500")]);

    let mut host = scene(true, true, |scroll| scroll);
    assert_columns(&mut host, -720.0, &IN_FLOW);

    let mut host = scene(true, false, |scroll| scroll);
    assert_columns(&mut host, -670.0, &[(-30.0, "K0-50 R50-90 Y90-140 B140-450 K450-500"), (-670.0, "K0-50 B50-450 K450-500")]);

    let mut host = scene(false, true, |scroll| scroll);
    assert_columns(&mut host, -650.0, &[(0.0, "K0-50 Y50-100 B100-450 K450-500"), (-650.0, "K0-50 B50-400 G400-450 K450-500")]);
}

#[test]
fn a_sticky_header_stays_while_the_content_scrolls_under_it() {
    let mut host = scene(true, true, |scroll| scroll.header_sticky(true));
    assert_columns(
        &mut host,
        -720.0,
        &[
            (0.0, "K0-50 R50-120 Y120-170 B170-450 K450-500"),
            (-30.0, "K0-50 R50-120 Y120-140 B140-450 K450-500"),
            (-100.0, "K0-50 R50-120 B120-450 K450-500"),
            (-720.0, "K0-50 R50-120 B120-400 G400-450 K450-500"),
        ],
    );

    // `content_offset` puts a gap under a sticky header, and counts for the length.
    let mut host = scene(true, true, |scroll| scroll.header_sticky(true).content_offset(10));
    assert_columns(
        &mut host,
        -730.0,
        &[(0.0, "K0-50 R50-120 W120-130 Y130-180 B180-450 K450-500"), (-30.0, "K0-50 R50-120 Y120-150 B150-450 K450-500")],
    );
}

#[test]
fn a_header_behind_is_covered_by_the_content_and_parallax_moves_it_slower() {
    // Behind with the full ratio looks like a header in the flow; so does behind and sticky, where
    // the header stays and the content covers it.
    let mut host = scene(true, true, |scroll| scroll.header_behind(true));
    assert_columns(&mut host, -720.0, &IN_FLOW);
    let mut host = scene(true, true, |scroll| scroll.header_behind(true).header_sticky(true));
    assert_columns(&mut host, -720.0, &IN_FLOW);

    // Half the speed, in the flow: the header is drawn over the content it lags behind.
    let mut host = scene(true, true, |scroll| scroll.header_parallax_ratio(0.5));
    assert_columns(
        &mut host,
        -720.0,
        &[(-30.0, "K0-50 R50-105 Y105-140 B140-450 K450-500"), (-100.0, "K0-50 R50-70 B70-450 K450-500"), (-300.0, "K0-50 B50-450 K450-500")],
    );

    // The cover of the demo: behind, half speed, the content starts 24 points up on the header.
    let mut host = scene(true, true, |scroll| scroll.header_behind(true).header_parallax_ratio(0.5).content_offset(-24));
    assert_columns(
        &mut host,
        -696.0,
        &[
            (0.0, "K0-50 R50-96 Y96-146 B146-450 K450-500"),
            (-30.0, "K0-50 R50-66 Y66-116 B116-450 K450-500"),
            (-100.0, "K0-50 B50-450 K450-500"),
            (-696.0, "K0-50 B50-400 G400-450 K450-500"),
        ],
    );
}

#[test]
fn content_offset_without_a_covering_header_is_a_gap_before_the_footer_as_upstream() {
    let mut host = scene(true, true, |scroll| scroll.content_offset(10));
    assert_columns(
        &mut host,
        -730.0,
        &[(0.0, "K0-50 R50-120 Y120-170 B170-450 K450-500"), (-730.0, "K0-50 B50-390 W390-400 G400-450 K450-500")],
    );
}

/// Ten moves of 10 points down at the start: the content stands 47.6 points past it (React: the
/// rubber band over the 400 points of the viewport).
fn pull_down(host: &mut Host) {
    let send = |host: &mut Host, kind, y: f32| {
        let time_ms = host.time_ms();
        host.ui.pointer(kind, 150.0, y, time_ms);
        host.frame_after(16.0);
    };
    send(host, PointerKind::Down, 200.0);
    for i in 1..=10 {
        send(host, PointerKind::Move, 200.0 + i as f32 * 10.0);
    }
    assert!((y(host) - 47.6).abs() < 0.01, "{}", y(host));
}

#[test]
fn a_pull_past_the_start_moves_the_header_by_its_kind() {
    // The content is drawn on whole pixels, 48 below its place. React draws the header at the same
    // rects: in the flow at 98, sticky at 50, behind with parallax at half the pull (74), and with
    // the content when parallax is off for the pull.
    let mut host = scene(true, true, |scroll| scroll);
    pull_down(&mut host);
    assert_eq!(column(&mut host), "K0-50 W50-98 R98-168 Y168-218 B218-450 K450-500");

    let mut host = scene(true, true, |scroll| scroll.header_sticky(true));
    pull_down(&mut host);
    assert_eq!(column(&mut host), "K0-50 R50-120 W120-168 Y168-218 B218-450 K450-500");

    let mut host = scene(true, true, |scroll| scroll.header_behind(true).header_parallax_ratio(0.5));
    pull_down(&mut host);
    assert_eq!(column(&mut host), "K0-50 W50-74 R74-144 W144-168 Y168-218 B218-450 K450-500");

    let mut host = scene(true, true, |scroll| scroll.header_behind(true).header_parallax_ratio(0.5).parallax_overscroll_enabled(false));
    pull_down(&mut host);
    assert_eq!(column(&mut host), "K0-50 W50-98 R98-168 Y168-218 B218-450 K450-500");
}

#[test]
fn header_and_footer_take_taps_where_they_are_drawn_and_a_pan_on_them_scrolls() {
    let mut host = scene(true, true, |scroll| scroll.header_sticky(true));
    scroll_to(&mut host, -100.0);
    // On the sticky header, rows under it: the header.
    host.tap(150.0, 60.0);
    assert_eq!(host.ui.state.tapped, [0]);
    // Below it: the row drawn there (content starts 70 below the viewport, 100 scrolled: row 3).
    host.tap(150.0, 300.0);
    assert_eq!(host.ui.state.tapped, [0, 105]);

    // A pan that starts on the header scrolls: 40 points, then the fling React ends at -193.929.
    host.fling((150.0, 80.0), (150.0, 40.0), 320.0, 8);
    assert!((y(&host) + 193.929).abs() < 0.01, "{}", y(&host));

    // The footer at the end.
    scroll_to(&mut host, -100_000.0);
    host.tap(150.0, 440.0);
    assert_eq!(host.ui.state.tapped, [0, 105, 1]);

    // A header in the flow is hit where it scrolled to, and not where it was.
    let mut host = scene(true, true, |scroll| scroll);
    scroll_to(&mut host, -30.0);
    host.tap(150.0, 80.0);
    host.tap(150.0, 110.0);
    assert_eq!(host.ui.state.tapped, [0, 100]);

    // A header behind the content: visible part yes, covered part no.
    let mut host = scene(true, true, |scroll| scroll.header_behind(true));
    scroll_to(&mut host, -50.0);
    host.tap(150.0, 60.0);
    host.tap(150.0, 100.0);
    assert_eq!(host.ui.state.tapped, [0, 100]);
}

#[test]
fn a_horizontal_scroll_has_its_header_and_footer_on_the_horizontal_axis() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let cells: Vec<Build<SkiaShape>> = (0..10).map(|_| SkiaShape::new().width_request(100).fill_y().background_color(Color::BLUE)).collect();
        SkiaScroll::new()
            .orientation(ScrollOrientation::Horizontal)
            .fill()
            .header_sticky(true)
            .header(SkiaShape::new().width_request(60).fill_y().background_color(Color::RED))
            .footer(SkiaShape::new().width_request(40).fill_y().background_color(GREEN))
            .assign(&mut app.scroll)
            .content(SkiaLayout::row().spacing(0).fill_y().children(cells))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 100, 1.0);
    host.settle();
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, -100_000.0, 0.0, 0);
    host.settle();
    // 60 + 1000 + 40 in 300.
    assert_eq!(host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_x(), -800.0);
    assert_eq!((host.pixel(30, 50), host.pixel(100, 50), host.pixel(280, 50)), (Color::RED, Color::BLUE, GREEN));
}
