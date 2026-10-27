//! Keyboard arrows in a Masonry group move to the tile on screen that way (C# 2ad608d1,
//! KeyboardListNavigationTests MasonryChildren_ArrowsMoveToTheTileOnScreenThatWay and
//! MasonryTemplated_ArrowsAndPageDownFollowTheColumn). Split 2, heights 100,20,20,100,100,20:
//! col0 = #0 (0..100), #4 (100..200); col1 = #1, #2, #3, #5 (0..20, 20..40, 40..140, 140..160).
//! Index + split is rarely the tile underneath (#1 + 2 = #3 skips #2 below it).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const HEIGHTS: [i32; 6] = [100, 20, 20, 100, 100, 20];

#[derive(Default)]
struct App {
    tiles: Vec<Handle<SkiaButton>>,
    count: usize,
    masonry: Handle<SkiaLayout>,
}

fn tile(i: usize) -> Build<SkiaButton> {
    SkiaButton::new(format!("tile {i}")).fill_x().height_request(HEIGHTS[i % HEIGHTS.len()])
}

fn ready(ui: Ui<App>, width: i32, height: i32) -> Headless<App> {
    let ui = ui.default_accessibility_role::<SkiaButton>(Aria::BUTTON).background(Color::BLACK);
    let mut host = Headless::new(ui, width, height, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

/// The text of the button the keyboard is on.
fn focused(host: &Headless<App>) -> String {
    let id = host.ui.accessibility_focused().expect("a focused tile");
    host.ui.tree.find::<SkiaButton>(id).map(|b| b.p.text.clone()).expect("a tile")
}

fn press(host: &mut Headless<App>, key: &'static str) {
    assert!(host.press_key(key), "{key} is used");
    host.settle();
}

/// From #1: Down 2, Down 3, Left 0, Down 4, Right 5, Up 3, Right stays 3.
fn walk(host: &mut Headless<App>, first: ControlId) {
    let node = host.ui.accessibility_nodes().iter().find(|n| n.control == first).expect("a node for tile 1").id;
    host.ui.accessibility_focus(node, true);
    host.settle();
    assert_eq!(focused(host), "tile 1");
    press(host, "ArrowDown");
    assert_eq!(focused(host), "tile 2", "right below, not #3");
    press(host, "ArrowDown");
    assert_eq!(focused(host), "tile 3");
    press(host, "ArrowLeft");
    assert_eq!(focused(host), "tile 0", "middle 90: #0 (50) is nearer than #4 (150)");
    press(host, "ArrowDown");
    assert_eq!(focused(host), "tile 4", "index + split would be #2, in the other column");
    press(host, "ArrowRight");
    assert_eq!(focused(host), "tile 5", "same middle, 150");
    press(host, "ArrowUp");
    assert_eq!(focused(host), "tile 3");
    press(host, "ArrowRight"); // no column on the right: the key is used, nothing moves
    assert_eq!(focused(host), "tile 3");
}

#[test]
fn masonry_children_arrows_move_to_the_tile_on_screen_that_way() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.tiles = vec![Handle::default(); 6];
        let tiles: Vec<_> = (0..6).map(|i| tile(i).assign(&mut app.tiles[i])).collect();
        SkiaMasonry::new().spacing(0).accessibility_role(Aria::GRID).children(tiles)
    });
    let mut host = ready(ui, 200, 400);
    let first = host.ui.state.tiles[1].id();
    walk(&mut host, first);
}

#[test]
fn masonry_templated_arrows_and_page_down_follow_the_column() {
    let ui = Ui::new(App { count: 60, ..App::default() }, |app: &mut App| {
        SkiaScroll::new().fill().content(SkiaMasonry::new().spacing(0).accessibility_role(Aria::GRID).assign(&mut app.masonry).items(
            |app: &App| app.count,
            || {
                let mut button = Handle::default();
                (tile(0).assign(&mut button), button)
            },
            |button: &Handle<SkiaButton>, _: &App, index, cx| {
                if let Some(mut button) = cx.get_mut(*button) {
                    button.set_text(format!("tile {index}"));
                    button.set_height_request(HEIGHTS[index % HEIGHTS.len()]);
                }
            },
        ))
    });
    let mut host = ready(ui, 200, 300);
    let masonry = host.ui.state.masonry;
    let first = host.ui.tree.find::<SkiaLayout>(masonry).unwrap().cell_in_use(1).expect("tile 1 has a view");
    walk(&mut host, first);

    // PageDown from #3 (col1, top 40): the last tile of col1 starting within one 300 px viewport below it.
    let slots = host.ui.tree.children(masonry).to_vec();
    let rects: Vec<Rect> = (0..60).map(|i| host.rect(slots[i])).collect();
    let from = rects[3];
    let expected = (0..60)
        .filter(|i| rects[*i].left == from.left && rects[*i].top > from.top && rects[*i].top <= from.top + 300.0)
        .max_by(|a, b| rects[*a].top.total_cmp(&rects[*b].top))
        .unwrap();
    press(&mut host, "PageDown");
    assert_eq!(focused(&host), format!("tile {expected}"));
    // The scroll brought it on screen.
    let id = host.ui.accessibility_focused().unwrap();
    let scrolled = host.ui.tree.find::<SkiaScroll>(host.ui.tree.parent(masonry).unwrap()).unwrap().viewport_offset_y();
    let top = host.rect(id).top + scrolled;
    assert!(top >= 0.0 && host.rect(id).bottom + scrolled <= 300.0, "tile {expected} at {top} is off screen");
}
