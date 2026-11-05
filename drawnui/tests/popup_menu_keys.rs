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
    menu_wrapping(checked, false)
}

/// `menu`, with `keyboard_wrap` on its menu shape when `wrap`.
fn menu_wrapping(checked: usize, wrap: bool) -> Build<SkiaShape> {
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
        .keyboard_wrap(wrap)
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

/// Opens the menu in a window of its own, as a desktop host shows it: the window is placed a
/// moment after the popup opened.
fn open_in_window(host: &mut Headless<App>, checked: usize, wrap: bool) {
    host.ui.state.checked = checked;
    let id = host.ui.tree.cx().open_popup_window(menu_wrapping(checked, wrap), Point::new(20.0, 20.0), PopupWindowOptions::default());
    host.ui.state.menu = Some(id);
    host.settle();
    let request = host.popup_window_requests().into_iter().find(|r| r.id == id.get()).expect("a window request");
    host.place_popup_window(request.id, request.size.width, request.size.height, 1.0, Point::new(20.0, 20.0));
    host.settle();
}

#[test]
fn in_a_window_of_its_own_the_checked_item_takes_the_keyboard_once_the_window_shows() {
    let mut host = host();
    host.popup_windows(true);
    open_in_window(&mut host, 1, false);
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1"], "the checked item had the focus");
    open_in_window(&mut host, 0, false);
    host.press_key("ArrowDown");
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 1", "item 2"], "Down twice from the first item");
}

#[test]
fn keyboard_wrap_makes_the_arrows_go_round() {
    let mut host = host();
    host.popup_windows(true);
    open_in_window(&mut host, 2, true);
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 0"], "Down from the last item: the first");
    open_in_window(&mut host, 0, true);
    host.press_key("ArrowUp");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 0", "item 2"], "Up from the first item: the last");
    // Without it the arrows stop at the ends.
    open_in_window(&mut host, 2, false);
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.log, ["item 0", "item 2", "item 2"]);
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

#[derive(Default)]
struct Page {
    button: Handle<SkiaShape>,
    menu: Option<PopupId>,
}

/// A button that opens a menu below itself, and a second button beside it.
fn page() -> Headless<Page> {
    let item = |name: &'static str| {
        SkiaShape::new().width_request(120).height_request(30).accessibility_role(Aria::MENU_ITEM).accessibility_label(name).on_tapped(|_me, page: &mut Page, cx| {
            if let Some(menu) = page.menu.take() {
                cx.close_popup_window(menu);
            }
        })
    };
    let ui = Ui::new(Page::default(), move |page: &mut Page| {
        SkiaLayout::row().spacing(10.0).padding(10.0).children((
            SkiaShape::new()
                .width_request(100)
                .height_request(30)
                .accessibility_role(Aria::BUTTON)
                .accessibility_label("Open menu")
                .assign(&mut page.button)
                .on_tapped(move |me, page: &mut Page, cx| {
                    let menu = SkiaShape::new().accessibility_role(Aria::MENU).children((SkiaLayout::column().spacing(0.0).children((item("one"), item("two"))),));
                    page.menu = Some(cx.open_popup_window(menu, me.id(), PopupWindowOptions::default()));
                }),
            SkiaShape::new().width_request(100).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label("Other").on_tapped(|_me, _page: &mut Page, _cx| {}),
        ))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 600, 400, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn keyboard_on(host: &Headless<Page>) -> Option<String> {
    host.ui.accessibility_focused().and_then(|f| host.ui.tree.base(f)).map(|b| b.p.accessibility_label.to_string())
}

#[test]
fn a_closed_menu_gives_the_keyboard_back_to_the_control_that_opened_it() {
    let mut host = page();
    host.press_key("Tab");
    assert_eq!(keyboard_on(&host).as_deref(), Some("Open menu"));
    // Escape.
    host.press_key("Enter");
    host.settle();
    assert_eq!(keyboard_on(&host).as_deref(), Some("one"));
    host.press_key("Escape");
    host.settle();
    assert_eq!(keyboard_on(&host).as_deref(), Some("Open menu"));
    // An item picked.
    host.press_key("Enter");
    host.settle();
    host.press_key("ArrowDown");
    host.press_key("Enter");
    host.settle();
    assert!(host.ui.state.menu.is_none());
    assert_eq!(keyboard_on(&host).as_deref(), Some("Open menu"));
    // Closed with the mouse elsewhere: the button gets the keyboard back, the ring stays hidden.
    let button = host.rect(host.ui.state.button);
    host.tap(button.center_x(), button.center_y());
    host.settle();
    host.tap(500.0, 300.0);
    host.settle();
    assert_eq!(keyboard_on(&host).as_deref(), Some("Open menu"));
}

#[test]
fn the_keyboard_stays_where_it_went_when_it_left_the_menu_before_it_closed() {
    let mut host = page();
    host.press_key("Tab");
    host.press_key("Enter");
    host.settle();
    assert_eq!(keyboard_on(&host).as_deref(), Some("one"));
    // A screen reader moves to the other button, then the app closes the menu: it stays there.
    let other = host.ui.accessibility_nodes().iter().find(|n| n.label == "Other").map(|n| n.id).unwrap();
    host.ui.accessibility_focus(other, true);
    let menu = host.ui.state.menu.unwrap();
    host.ui.tree.cx().close_popup_window(menu);
    host.settle();
    assert_eq!(keyboard_on(&host).as_deref(), Some("Other"));
}

#[derive(Default)]
struct Picker {
    menu: Option<PopupId>,
    other: Handle<SkiaShape>,
    /// What an item does with the focus as it closes the menu: 0 nothing, 1 focus Other, 2 keep it.
    after: u8,
}

/// `page`, with items that close the menu and then move or keep the focus as `after` says.
fn picker(after: u8) -> Headless<Picker> {
    let item = |name: &'static str| {
        SkiaShape::new().width_request(120).height_request(30).accessibility_role(Aria::MENU_ITEM).accessibility_label(name).on_tapped(|_me, p: &mut Picker, cx| {
            if let Some(menu) = p.menu.take() {
                cx.close_popup_window(menu);
            }
            match p.after {
                1 => cx.set_accessibility_focus(p.other),
                2 => cx.keep_accessibility_focus(),
                _ => {}
            }
        })
    };
    let ui = Ui::new(Picker { after, ..Picker::default() }, move |p: &mut Picker| {
        SkiaLayout::row().spacing(10.0).padding(10.0).children((
            SkiaShape::new().width_request(100).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label("Other").assign(&mut p.other).on_tapped(|_me, _p: &mut Picker, _cx| {}),
            SkiaShape::new().width_request(100).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label("Open menu").on_tapped(move |me, p: &mut Picker, cx| {
                let menu = SkiaShape::new().accessibility_role(Aria::MENU).children((SkiaLayout::column().spacing(0.0).children((item("one"), item("two"))),));
                p.menu = Some(cx.open_popup_window(menu, me.id(), PopupWindowOptions::default()));
            }),
        ))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 600, 400, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

/// An app's `set_accessibility_focus` as a menu closes wins over the return to the opener;
/// `keep_accessibility_focus` keeps the focus from moving there (DrawnUI drawnui-cross 6az).
#[test]
fn the_apps_focus_wins_over_the_return_to_the_opener() {
    let on = |host: &Headless<Picker>| host.ui.accessibility_focused().and_then(|f| host.ui.tree.base(f)).map(|b| b.p.accessibility_label.to_string());
    // Other comes first: where the focus goes when nothing places it, not the opener.
    for (after, want) in [(0, Some("Open menu")), (1, Some("Other"))] {
        let mut host = picker(after);
        host.press_key("Tab");
        host.press_key("Tab");
        host.press_key("Enter");
        host.settle();
        assert_eq!(on(&host).as_deref(), Some("one"));
        host.press_key("Enter");
        host.settle();
        assert_eq!(on(&host).as_deref(), want, "after {after}");
    }
    let mut host = picker(2);
    host.press_key("Tab");
    host.press_key("Tab");
    host.press_key("Enter");
    host.settle();
    host.press_key("Enter");
    host.settle();
    assert_ne!(on(&host).as_deref(), Some("Open menu"), "kept: no return to the opener");
}
