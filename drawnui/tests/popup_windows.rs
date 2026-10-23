//! Popup windows (`Cx::open_popup_window`): content that opens over everything from a point or a
//! rect, the way native menus do. On a desktop host each one is a window of its own; elsewhere it
//! is drawn on the canvas. Both are played here: the canvas, and the host's windows.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    log: Vec<String>,
    menu: Option<PopupId>,
    options: PopupWindowOptions,
}

const RED: Color = Color::from_rgb(220, 30, 30);
const BLUE: Color = Color::from_rgb(30, 30, 220);

/// A menu of two 120 x 30 pt items, red then blue; a tap logs the item and closes the menu.
fn menu() -> Build<SkiaLayout> {
    let item = |name: &'static str, color: Color| {
        SkiaShape::new().width_request(120).height_request(30).background_color(color).accessibility_role(Aria::MENU_ITEM).on_tapped(move |_me, app: &mut App, cx| {
            app.log.push(name.into());
            if let Some(menu) = app.menu.take() {
                cx.close_popup_window(menu);
            }
        })
    };
    SkiaLayout::column().spacing(0.0).children((item("red", RED), item("blue", BLUE)))
}

/// A 300 x 200 pt page at scale 2 with a button at (0, 0) 100 x 100 pt; a right click opens the menu
/// where it was.
fn page() -> Headless<App> {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children(
            SkiaShape::new()
                .width_request(100)
                .height_request(100)
                .background_color(Color::from_rgb(200, 200, 200))
                .accessibility_role(Aria::BUTTON)
                .on_tapped(|_me, app: &mut App, _cx| app.log.push("page".into())),
        )
    })
    .background(Color::WHITE)
    .on_context_menu(|app: &mut App, at, cx| {
        app.menu = Some(cx.open_popup_window(menu(), at.location, app.options));
        true
    });
    let mut host = Headless::new(ui, 600, 400, 2.0);
    host.settle();
    host
}

#[test]
fn on_the_canvas_it_opens_at_the_point_over_everything_and_its_items_take_taps() {
    let mut host = page();
    // Beside the page's button (a right click is a tap too: on the button it would log).
    assert!(host.right_click(210.0, 60.0));
    // From the point: 240 x 120 px.
    assert_eq!(host.pixel(220, 70), RED);
    assert_eq!(host.pixel(220, 130), BLUE);
    assert_eq!(host.pixel(205, 70), Color::WHITE);
    host.tap(300.0, 130.0);
    assert_eq!(host.ui.state.log, ["blue"]);
    // Closed by the item: the page shows again and takes taps.
    assert_eq!(host.pixel(220, 70), Color::WHITE);
    host.tap(110.0, 70.0);
    assert_eq!(host.ui.state.log, ["blue", "page"]);
}

#[test]
fn where_the_canvas_ends_it_opens_to_the_left_and_above() {
    let mut host = page();
    host.right_click(590.0, 390.0);
    // 240 x 120 px ending at the point.
    assert_eq!(host.pixel(351, 271), RED);
    assert_eq!(host.pixel(589, 389), BLUE);
    assert_eq!(host.pixel(349, 271), Color::WHITE);
}

#[test]
fn a_press_outside_closes_it_and_goes_nowhere_and_escape_closes_it() {
    let mut host = page();
    host.right_click(300.0, 200.0);
    assert!(host.ui.state.menu.is_some_and(|m| host.ui.tree.cx().is_popup_window_open(m)));
    // On the page's button: the menu closes, the button gets no tap.
    host.tap(50.0, 50.0);
    assert!(!host.ui.tree.cx().is_popup_window_open(host.ui.state.menu.unwrap()));
    assert!(host.ui.state.log.is_empty());
    host.tap(50.0, 50.0);
    assert_eq!(host.ui.state.log, ["page"]);

    host.right_click(300.0, 200.0);
    assert!(host.press_key("Escape"));
    assert!(!host.ui.tree.cx().is_popup_window_open(host.ui.state.menu.unwrap()));
    assert_eq!(host.pixel(310, 210), Color::WHITE);
}

#[test]
fn a_right_click_beside_it_opens_the_next_one_there() {
    let mut host = page();
    host.right_click(300.0, 200.0);
    let first = host.ui.state.menu.unwrap();
    host.right_click(450.0, 50.0);
    let second = host.ui.state.menu.unwrap();
    let cx = host.ui.tree.cx();
    assert!(!cx.is_popup_window_open(first) && cx.is_popup_window_open(second));
    // 240 px wide: to the left of the point.
    assert_eq!(host.pixel(440, 60), RED);
    assert_eq!(host.pixel(310, 210), Color::WHITE);
}

#[test]
fn in_a_window_of_its_own_it_asks_for_its_size_and_is_drawn_and_tapped_there() {
    let mut host = page();
    host.popup_windows(true);
    host.right_click(300.0, 60.0);
    let requests = host.popup_window_requests();
    assert_eq!(requests.len(), 1);
    let request = requests[0];
    assert_eq!((request.anchor, request.point), (Rect::from_xywh(300.0, 60.0, 0.0, 0.0), true));
    assert_eq!(request.size, Size::new(120.0, 60.0));
    // Nothing on the canvas.
    assert_eq!(host.pixel(310, 70), Color::WHITE);

    // The host made it on a screen of scale 1.5, 1000 px right of the canvas.
    host.place_popup_window(request.id, 180.0, 90.0, 1.5, Point::new(1000.0, 60.0));
    let mut window = host.draw_popup_window(request.id, 180, 90);
    let pixels = window.peek_pixels().unwrap();
    assert_eq!(pixels.get_color((5, 5)), RED);
    assert_eq!(pixels.get_color((175, 85)), BLUE);
    host.popup_tap(request.id, 90.0, 20.0);
    assert_eq!(host.ui.state.log, ["red"]);
    assert!(host.popup_window_requests().is_empty());
}

#[test]
fn in_a_window_a_press_on_the_canvas_closes_it_and_leaving_both_closes_it_when_asked() {
    let mut host = page();
    host.popup_windows(true);
    host.right_click(300.0, 60.0);
    let id = host.popup_window_requests()[0].id;
    host.place_popup_window(id, 240.0, 120.0, 2.0, Point::new(300.0, 60.0));
    host.tap(50.0, 50.0);
    assert!(host.popup_window_requests().is_empty());
    assert!(host.ui.state.log.is_empty());

    host.ui.state.options.close_on_pointer_leave = true;
    host.right_click(300.0, 60.0);
    let id = host.popup_window_requests()[0].id;
    host.place_popup_window(id, 240.0, 120.0, 2.0, Point::new(300.0, 60.0));
    // From the canvas into the popup: the canvas' leave comes after, and changes nothing.
    host.popup_hover(id, Some((20.0, 20.0)));
    host.leave();
    host.frame_after(400.0);
    assert_eq!(host.popup_window_requests().len(), 1);
    // Out of the popup to another app: closed after the grace time.
    host.popup_hover(id, None);
    host.settle();
    assert!(host.popup_window_requests().is_empty());
}

#[test]
fn tab_stays_in_the_popup_and_a_screen_reader_reads_it_where_its_window_is() {
    let mut host = page();
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.popup_windows(true);
    host.right_click(300.0, 60.0);
    let id = host.popup_window_requests()[0].id;
    host.place_popup_window(id, 180.0, 90.0, 1.5, Point::new(1000.0, 60.0));
    host.frame_after(1000.0);
    // The items in canvas points: from (500, 30), 120 x 30 each.
    let rects: Vec<Rect> = host.ui.accessibility_nodes().iter().filter(|n| n.rect.left >= 499.0).map(|n| n.rect).collect();
    assert_eq!(rects, [Rect::from_xywh(500.0, 30.0, 120.0, 30.0), Rect::from_xywh(500.0, 60.0, 120.0, 30.0)]);
    // Tab walks the menu only (the page's button comes first in reading order), Enter picks.
    host.press_key("Tab");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["red"]);
}

#[test]
fn a_dropdown_opens_below_its_control_and_its_items_take_hover_in_their_window() {
    #[derive(Default)]
    struct Drop {
        button: Handle<SkiaShape>,
        hovered: Vec<bool>,
    }
    let ui = Ui::new(Drop::default(), |s| {
        SkiaLayout::new().fill().children(
            SkiaShape::new().margin((40.0, 30.0, 0.0, 0.0)).width_request(100).height_request(20).background_color(BLUE).assign(&mut s.button),
        )
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 600, 400, 2.0);
    host.settle();
    let item = SkiaShape::new().width_request(80).height_request(40).background_color(RED).on_hovered(|_me, s: &mut Drop, _cx, on| s.hovered.push(on));
    let button = host.ui.state.button;
    host.ui.tree.cx().open_popup_window(item, button, PopupWindowOptions::default());
    host.settle();
    // On the canvas: below the button, left edges together (pixels at scale 2).
    assert_eq!(host.pixel(81, 101), RED);
    assert_eq!(host.pixel(79, 101), Color::WHITE);

    // In a window: the anchor is the button's drawn rect, in the canvas's pixels.
    let mut host = {
        let ui = Ui::new(Drop::default(), |s| {
            SkiaLayout::new().fill().children(
                SkiaShape::new().margin((40.0, 30.0, 0.0, 0.0)).width_request(100).height_request(20).background_color(BLUE).assign(&mut s.button),
            )
        });
        let mut host = Headless::new(ui, 600, 400, 2.0);
        host.popup_windows(true);
        host.settle();
        host
    };
    let item = SkiaShape::new().width_request(80).height_request(40).background_color(RED).on_hovered(|_me, s: &mut Drop, _cx, on| s.hovered.push(on));
    let button = host.ui.state.button;
    let id = host.ui.tree.cx().open_popup_window(item, button, PopupWindowOptions::default());
    host.settle();
    let request = host.popup_window_requests()[0];
    assert_eq!((request.anchor, request.point, request.size), (Rect::from_xywh(80.0, 60.0, 200.0, 40.0), false, Size::new(80.0, 40.0)));
    host.place_popup_window(request.id, 160.0, 80.0, 2.0, Point::new(80.0, 100.0));
    host.popup_hover(request.id, Some((10.0, 10.0)));
    host.popup_hover(request.id, None);
    assert_eq!(host.ui.state.hovered, [true, false]);
    assert!(host.ui.tree.cx().is_popup_window_open(id));
}
