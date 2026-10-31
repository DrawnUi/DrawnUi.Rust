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
