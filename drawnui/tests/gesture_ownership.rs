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
