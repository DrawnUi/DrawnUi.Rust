//! The Menu key and Shift+F10 (and a screen reader's "show menu") open the context menu of the
//! control in keyboard focus, at its center (drawnui-cross 6bc; DrawnUi.Net KeyboardContextMenuTests).

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::App as _;

#[derive(Default)]
struct App {
    menus: Vec<(&'static str, ContextMenuSource, Point)>,
    fallback: u32,
    app_keys: Vec<bool>,
    card: Handle<SkiaLayout>,
}

/// A card with a menu holding two buttons ("Open" without a menu of its own, "Delete" with one),
/// and a plain button "Plain" outside it.
fn host() -> Headless<App> {
    let button = |name: &'static str| {
        SkiaShape::new().width_request(80).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label(name).on_tapped(|_me, _app: &mut App, _cx| {})
    };
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaLayout::column().spacing(10).padding(10).children((
            SkiaLayout::row()
                .spacing(10)
                .padding(10)
                .assign(&mut app.card)
                .on_context_menu(|_me, app: &mut App, _cx, m: &ContextMenu| {
                    app.menus.push(("card", m.source, m.location));
                    true
                })
                .children((
                    button("Open"),
                    button("Delete").on_context_menu(|_me, app: &mut App, _cx, m: &ContextMenu| {
                        app.menus.push(("delete", m.source, m.location));
                        true
                    }),
                )),
            button("Plain"),
        ))
    })
    .on_context_menu(|app: &mut App, _m, _cx| {
        app.fallback += 1;
        true
    })
    .on_key_down(|app: &mut App, e: &KeyEvent, _cx| {
        app.app_keys.push(e.handled);
        false
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    host
}

fn shift() -> Modifiers {
    Modifiers { shift: true, ..Modifiers::default() }
}

#[test]
fn the_menu_key_opens_the_menu_of_the_focused_control_at_its_center() {
    let mut host = host();
    host.press_key("Tab"); // Open
    host.press_key("Tab"); // Delete
    host.ui.state.app_keys.clear();
    assert!(host.press_key("ContextMenu"));
    let (who, source, at) = host.ui.state.menus[0];
    assert_eq!((who, source), ("delete", ContextMenuSource::Keyboard));
    let delete = host.ui.accessibility_focused().unwrap();
    let r = host.rect(delete);
    assert!((at.x - r.center_x()).abs() < 1.0 && (at.y - r.center_y()).abs() < 1.0, "{at:?} in {r:?}");
    assert_eq!(host.ui.state.app_keys, [true], "the app's key handler sees it handled: no second menu");
}

#[test]
fn shift_f10_walks_up_to_the_first_control_with_a_menu() {
    let mut host = host();
    host.press_key("Tab"); // Open: no menu of its own, its card has one
    assert!(host.key_down("F10", shift()));
    assert_eq!(host.ui.state.menus.iter().map(|m| m.0).collect::<Vec<_>>(), ["card"]);
    // F10 alone is no menu request.
    assert!(!host.key_down("F10", Modifiers::default()));
    assert_eq!(host.ui.state.menus.len(), 1);
}

#[test]
fn a_focused_control_without_any_menu_up_its_chain_gets_the_app_fallback() {
    let mut host = host();
    for _ in 0..3 {
        host.press_key("Tab"); // Open, Delete, Plain
    }
    assert!(host.press_key("ContextMenu"));
    assert_eq!((host.ui.state.menus.len(), host.ui.state.fallback), (0, 1));
}

#[test]
fn with_nothing_in_focus_the_key_is_left_to_the_host() {
    let mut host = host();
    assert!(!host.press_key("ContextMenu"), "the host opens it where the pointer is");
    assert_eq!((host.ui.state.menus.len(), host.ui.state.fallback), (0, 0));
}

#[test]
fn a_screen_readers_show_menu_opens_that_controls_menu() {
    let mut host = host();
    let card: ControlId = host.ui.state.card.into();
    let node = host.ui.accessibility_nodes().iter().find(|n| n.control == card).cloned();
    if let Some(node) = node {
        assert!(node.has_context_menu);
        assert!(host.ui.accessibility_context_menu(node.id));
        assert_eq!(host.ui.state.menus.iter().map(|m| (m.0, m.1)).collect::<Vec<_>>(), [("card", ContextMenuSource::Keyboard)]);
    } else {
        // The card itself has no node: its button's "show menu" walks up to it.
        let open = host.ui.accessibility_nodes().iter().find(|n| n.label == "Open").map(|n| n.id).unwrap();
        assert!(host.ui.accessibility_context_menu(open));
        assert_eq!(host.ui.state.menus.iter().map(|m| m.0).collect::<Vec<_>>(), ["card"]);
    }
}
