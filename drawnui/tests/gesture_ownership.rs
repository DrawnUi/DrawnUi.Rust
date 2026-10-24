//! A gesture belongs to whoever started panning with it until Up (drawnui-cross 6n, C# fd7de2d2): a
//! vertical list panned inside a side drawer or a carousel keeps the press when it turns sideways;
//! the drawer or carousel moves only for a press that starts sideways.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    drawer: Handle<SkiaDrawer>,
    carousel: Handle<SkiaCarousel>,
}

fn list(color: Color) -> Build<SkiaScroll> {
    let rows: Vec<_> = (0..40)
        .map(|i| SkiaLayout::new().fill_x().height_request(50).background_color(if i % 2 == 0 { color } else { Color::DARK_GRAY }))
        .collect();
    SkiaScroll::new().fill().ignore_wrong_direction(true).content(SkiaLayout::column().spacing(0).children(rows))
}

/// Down at (250, 300), the moves, up, settled; 16 ms between events.
fn press(host: &mut Headless<App>, moves: &[(f32, f32)]) {
    host.ui.pointer(PointerKind::Down, 250.0, 300.0, host.time_ms());
    host.frame_after(16.0);
    for &(x, y) in moves {
        host.ui.pointer(PointerKind::Move, x, y, host.time_ms());
        host.frame_after(16.0);
    }
    let (x, y) = *moves.last().unwrap();
    host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
    host.settle();
}

/// Up 100 points along the list, then 150 sideways (`dx` per step), no lift.
fn list_then_sideways(dx: f32) -> Vec<(f32, f32)> {
    let up = (1..=10).map(|i| (250.0, 300.0 - 10.0 * i as f32));
    up.chain((1..=10).map(move |i| (250.0 + dx * i as f32, 200.0))).collect()
}

fn sideways(dx: f32) -> Vec<(f32, f32)> {
    (1..=10).map(|i| (250.0 + dx * i as f32, 300.0)).collect()
}

fn drawer_scene() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let drawer = SkiaDrawer::new()
            .direction(DrawerDirection::FromRight)
            .header_size(40)
            .width_request(300)
            .fill_y()
            .horizontal_options(LayoutOptions::End)
            .is_open(true)
            .ignore_wrong_direction(true)
            .assign(&mut app.drawer)
            .children(list(Color::GRAY));
        SkiaLayout::new().fill().children(drawer)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 600, 1.0);
    host.settle();
    host
}

fn carousel_scene() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let pages = (list(Color::RED), list(Color::GREEN), list(Color::BLUE));
        let carousel = SkiaCarousel::new().fill().ignore_wrong_direction(true).children(pages).assign(&mut app.carousel);
        SkiaLayout::new().fill().children(carousel)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 600, 1.0);
    host.settle();
    host
}

fn drawer_open(host: &Headless<App>) -> bool {
    host.ui.tree.find::<SkiaDrawer>(host.ui.state.drawer).unwrap().is_open()
}

fn page(host: &Headless<App>) -> usize {
    host.ui.tree.find::<SkiaCarousel>(host.ui.state.carousel).unwrap().selected_index()
}

#[test]
fn a_list_that_panned_keeps_the_press_inside_a_side_drawer() {
    for touch in [false, true] {
        let mut host = drawer_scene();
        host.use_touch(touch);
        press(&mut host, &list_then_sideways(15.0));
        assert!(drawer_open(&host), "touch {touch}: the drawer stayed");
    }
    let mut host = drawer_scene();
    press(&mut host, &sideways(15.0));
    assert!(!drawer_open(&host), "a press that starts sideways closes it");
}

#[test]
fn a_list_that_panned_keeps_the_press_inside_a_carousel() {
    let mut host = carousel_scene();
    press(&mut host, &list_then_sideways(-15.0));
    assert_eq!(page(&host), 0, "the carousel stayed");
    let mut host = carousel_scene();
    press(&mut host, &sideways(-15.0));
    assert_eq!(page(&host), 1, "a press that starts sideways turns the page");
}

// C# a32a6aac (drawnui-cross 6n, last paragraph): the control that consumed the last event owns the
// gesture and is offered the next one first, wherever the finger is.

#[derive(Default)]
struct Strips {
    strip: Handle<SkiaScroll>,
    page: Handle<SkiaScroll>,
    sibling_pans: u32,
}

/// C# GestureOwnershipTests.Strip: a horizontal scroll 100 high at `top`, 30 tiles of 80.
fn strip(top: f32) -> Build<SkiaScroll> {
    let tiles: Vec<_> = (0..30).map(|_| SkiaShape::new().width_request(80).height_request(80).background_color(Color::from_rgb(0, 128, 128))).collect();
    SkiaScroll::new()
        .orientation(ScrollOrientation::Horizontal)
        .height_request(100)
        .fill_x()
        .vertical_options(LayoutOptions::Start)
        .margin((0.0, top, 0.0, 0.0))
        .content(SkiaLayout::row().spacing(4).children(tiles))
}

/// Down at `from`, the moves 16 ms apart (a frame after each), up, settled. Calls `during` after each move.
fn drag(host: &mut Headless<Strips>, from: (f32, f32), moves: &[(f32, f32)], mut during: impl FnMut(&Headless<Strips>, usize)) {
    host.ui.pointer(PointerKind::Down, from.0, from.1, host.time_ms());
    host.frame_after(16.0);
    for (i, &(x, y)) in moves.iter().enumerate() {
        host.ui.pointer(PointerKind::Move, x, y, host.time_ms());
        host.frame_after(16.0);
        during(host, i);
    }
    let (x, y) = *moves.last().unwrap();
    host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
    host.settle();
}

fn offset_x(host: &Headless<Strips>, id: Handle<SkiaScroll>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(id).unwrap().viewport_offset_x()
}

/// C# PanningStrip_KeepsTheGestureOutsideItsBounds_SiblingBelowGetsNothing: a strip in a
/// BlockGesturesBelow layer root over a full-screen sibling that listens; dragged left, then down out
/// of the strip without lifting: the sibling gets no Panning.
#[test]
fn a_panning_strip_keeps_the_gesture_outside_its_bounds_and_the_sibling_below_gets_nothing() {
    let ui = Ui::new(Strips::default(), |s: &mut Strips| {
        SkiaLayer::new().fill().block_gestures_below(true).children((
            SkiaLayout::new().fill().consume_gestures(|_me, s: &mut Strips, _cx, g| {
                if g.kind == GestureKind::Panning {
                    s.sibling_pans += 1;
                }
                false
            }),
            strip(300.0).assign(&mut s.strip),
        ))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 700, 1.0);
    host.settle();
    let left = (1..=10).map(|i| (300.0 - 15.0 * i as f32, 350.0));
    let down = (1..=8).map(|i| (150.0, 350.0 + 15.0 * i as f32));
    let moves: Vec<_> = left.chain(down).collect();
    let mut scrolled = 0.0;
    let strip = host.ui.state.strip;
    drag(&mut host, (300.0, 350.0), &moves, |host, i| {
        if i == 9 {
            scrolled = offset_x(host, strip);
        }
    });
    assert!(scrolled < -50.0, "the strip did not scroll: {scrolled}");
    assert_eq!(host.ui.state.sibling_pans, 0);
}

/// C# PanningStrip_InsideAVerticalScroll_ThenDown_PageStays: a horizontal strip in a vertical page
/// scroll, dragged left then up without lifting: the page does not move.
#[test]
fn a_panning_strip_inside_a_vertical_scroll_keeps_the_page_still() {
    let ui = Ui::new(Strips::default(), |s: &mut Strips| {
        let rows: Vec<_> = (0..12)
            .map(|i| SkiaShape::new().background_color(Color::from_rgb(72, 61, 139)).height_request(80).fill_x().margin((0.0, 320.0 + 90.0 * i as f32, 0.0, 0.0)))
            .collect();
        SkiaScroll::new()
            .fill()
            .assign(&mut s.page)
            .content(SkiaLayer::new().fill_x().children((strip(200.0).assign(&mut s.strip), rows)))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 700, 1.0);
    host.settle();
    let left = (1..=10).map(|i| (300.0 - 15.0 * i as f32, 250.0));
    let up = (1..=8).map(|i| (150.0, 250.0 - 15.0 * i as f32));
    let moves: Vec<_> = left.chain(up).collect();
    let (strip, page) = (host.ui.state.strip, host.ui.state.page);
    let (mut scrolled, mut page_max) = (0.0, 0.0f32);
    drag(&mut host, (300.0, 250.0), &moves, |host, i| {
        if i == 9 {
            scrolled = offset_x(host, strip);
        }
        if i >= 10 {
            page_max = page_max.max(host.ui.tree.find::<SkiaScroll>(page).unwrap().viewport_offset_y().abs());
        }
    });
    assert!(scrolled < -50.0, "the strip did not scroll: {scrolled}");
    assert!(page_max < 1.0, "the page scrolled {page_max} pt during the strip's gesture");
}
