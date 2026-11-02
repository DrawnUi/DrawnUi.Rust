//! A menu in a popup takes the keyboard as a native one does: as it opens, its checked item (else
//! its first) has the focus, and the arrows move between its items also when the menu role sits on a
//! wrapper (a shape around the column that holds the items).

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    log: Vec<String>,
    checked: usize,
    menu: Option<PopupId>,
}

/// A rounded shape with the menu role around a column of three radio menu items.
fn menu(checked: usize) -> Build<SkiaShape> {
    let item = |index: usize| {
        SkiaShape::new()
            .width_request(120)
            .height_request(30)
            .accessibility_role(Aria::MENU_ITEM_RADIO)
            .accessibility_label(format!("item {index}"))
            .accessibility_is_pressed(Some(index == checked))
            .on_tapped(move |_me, app: &mut App, cx| {
                app.log.push(format!("item {index}"));
                if let Some(menu) = app.menu.take() {
                    cx.close_popup_window(menu);
                }
            })
    };
    SkiaShape::new()
        .corner_radius(8)
        .padding(4)
        .accessibility_role(Aria::MENU)
        .children((SkiaLayout::column().spacing(0.0).children((item(0), item(1), item(2))),))
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |_| SkiaLayout::new().fill()).background(Color::WHITE);
    let mut host = Headless::new(ui, 600, 400, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn open(host: &mut Headless<App>, checked: usize) {
    host.ui.state.checked = checked;
    let id = host.ui.tree.cx().open_popup_window(menu(checked), Point::new(20.0, 20.0), PopupWindowOptions::default());
    host.ui.state.menu = Some(id);
    host.settle();
}

#[test]
fn opening_focuses_the_checked_item_and_the_arrows_move_inside_the_wrapped_column() {
    let mut host = host();
    open(&mut host, 1);
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1"], "the checked item had the focus as the menu opened");

    open(&mut host, 0);
    host.press_key("ArrowDown");
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1", "item 2"], "Down twice from the first item");

    open(&mut host, 2);
    host.press_key("ArrowUp");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1", "item 2", "item 1"], "Up from the checked last item");
}

#[test]
fn with_nothing_checked_the_first_item_has_the_focus() {
    let mut host = host();
    open(&mut host, 9);
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 0"]);
}

/// A palette whose rows learn their state from the app (`observe`), opened from app code.
fn observed_menu() -> Build<SkiaLayout> {
    let item = |index: usize| {
        SkiaShape::new()
            .width_request(40)
            .height_request(30)
            .accessibility_role(Aria::MENU_ITEM_RADIO)
            .observe(move |me, app: &App| {
                me.set_accessibility_label(format!("tint {index}"));
                me.set_accessibility_is_pressed(Some(index == app.checked));
            })
            .on_tapped(move |_me, app: &mut App, _cx| app.log.push(format!("tint {index}")))
    };
    SkiaLayout::row().spacing(0.0).accessibility_role(Aria::MENU).children((item(0), item(1), item(2)))
}

#[test]
fn a_menu_whose_state_comes_from_observers_focuses_its_checked_item() {
    let mut host = host();
    host.ui.state.checked = 2;
    let id = host.ui.tree.cx().open_popup_window(observed_menu(), Point::new(20.0, 20.0), PopupWindowOptions::default());
    host.ui.state.menu = Some(id);
    host.ui.state_changed();
    host.settle();
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["tint 2"]);
}

/// A row with `role` and a tap that logs `name`.
fn row(name: &'static str, role: &'static str, on: bool) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(120)
        .height_request(30)
        .accessibility_role(role)
        .accessibility_label(name)
        .accessibility_is_pressed(Some(on))
        .on_tapped(move |_me, app: &mut App, _cx| app.log.push(name.into()))
}

fn open_content(host: &mut Headless<App>, content: impl Into<drawnui::Detached>) {
    let id = host.ui.tree.cx().open_popup_window(content, Point::new(20.0, 20.0), PopupWindowOptions::default());
    host.ui.state.menu = Some(id);
    host.settle();
}

#[test]
fn a_switched_on_checkbox_item_does_not_take_the_focus() {
    let mut host = host();
    let menu = SkiaLayout::column().spacing(0.0).accessibility_role(Aria::MENU).children((
        row("plain", Aria::MENU_ITEM, false),
        row("box on", Aria::MENU_ITEM_CHECKBOX, true),
    ));
    open_content(&mut host, menu);
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["plain"]);
}

#[test]
fn tab_goes_round_an_open_popup_and_the_arrows_still_work() {
    let mut host = host();
    open(&mut host, 0);
    // The menu is the popup's one Tab stop: Tab stays on it.
    host.press_key("Tab");
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1"]);
}

#[test]
fn a_menu_panel_with_a_title_and_two_columns_moves_in_reading_order() {
    let mut host = host();
    let menu = SkiaShape::new().accessibility_role(Aria::MENU).children((SkiaLayout::column().spacing(0.0).children((
        SkiaLabel::new("Title"),
        SkiaShape::new().fill_x().height_request(1),
        SkiaLayout::column().spacing(0.0).children((row("a", Aria::MENU_ITEM, false), row("b", Aria::MENU_ITEM, false))),
        SkiaLayout::column().spacing(0.0).children((row("c", Aria::MENU_ITEM, false),)),
    )),));
    open_content(&mut host, menu);
    host.press_key("ArrowDown");
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["c"], "a, b, then c in the next column");
    host.press_key("ArrowUp");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["c", "b"]);
}

#[test]
fn a_closed_menu_leaves_the_accessibility_snapshot_in_the_next_frame() {
    let mut host = host();
    open(&mut host, 0);
    let items = |host: &Headless<App>| host.ui.accessibility_nodes().iter().filter(|n| n.label.starts_with("item ")).count();
    assert_eq!(items(&host), 3);
    assert!(host.press_key("Escape"));
    host.frame_after(16.0);
    assert!(!host.ui.tree.cx().is_popup_window_open(host.ui.state.menu.unwrap()));
    assert_eq!(items(&host), 0, "not up to a second later");
    assert_eq!(host.ui.accessibility_focused(), None);
}
