//! `accessibility_modal` (aria-modal, DrawnUI `AccessibilityModal`, drawnui-cross 6av): while a
//! flagged control shows, the accessibility tree and Tab keep to it and the popup windows open; the
//! keyboard, when in use, moves into it and comes back when it goes; with several, the one on top.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    dialog: Handle<SkiaLayout>,
    second: Handle<SkiaLayout>,
    open_menu: Handle<SkiaShape>,
}

fn button(name: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(80)
        .height_request(30)
        .accessibility_role(Aria::BUTTON)
        .accessibility_label(name)
        .on_tapped(|_me, _app: &mut App, _cx| {})
}

/// A page with two buttons; over it a hidden dialog "Settings" (its title says the name too) with
/// OK and Cancel, and a second hidden dialog drawn after it.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((
            SkiaLayout::column().spacing(10).padding(10).children((button("Page A"), button("Page B"))),
            SkiaLayout::column()
                .spacing(10)
                .padding((200, 10, 0, 0))
                .is_visible(false)
                .accessibility_label("Settings")
                .accessibility_modal(true)
                .assign(&mut app.dialog)
                .children((
                    SkiaLabel::new("Settings").accessibility_role(Aria::HEADING),
                    button("OK"),
                    button("Cancel"),
                    button("Menu").assign(&mut app.open_menu).on_tapped(|me, _app: &mut App, cx| {
                        let menu = SkiaLayout::column().accessibility_role(Aria::MENU).children((button("Copy"), button("Paste")));
                        cx.open_popup_window(menu, me.id(), PopupWindowOptions::default());
                    }),
                )),
            SkiaLayout::column()
                .padding((300, 10, 0, 0))
                .is_visible(false)
                .accessibility_label("Confirm")
                .accessibility_modal(true)
                .assign(&mut app.second)
                .children((button("Yes"),)),
        ))
    });
    let mut host = Headless::new(ui, 500, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    host
}

fn show(host: &mut Headless<App>, which: fn(&App) -> Handle<SkiaLayout>, on: bool) {
    let id = which(&host.ui.state);
    if let Some(mut c) = host.ui.tree.cx().get_mut(id) {
        c.set_is_visible(on);
    }
    host.frame_after(16.0);
    host.frame_after(16.0);
}

fn labels(host: &Headless<App>) -> Vec<String> {
    host.ui.accessibility_nodes().iter().map(|n| n.label.clone()).filter(|l| !l.is_empty()).collect()
}

fn keyboard_on(host: &Headless<App>) -> Option<String> {
    host.ui.accessibility_focused().and_then(|f| host.ui.tree.base(f)).map(|b| b.p.accessibility_label.to_string())
}

#[test]
fn a_shown_modal_is_all_the_accessibility_tree_has_and_its_node_is_a_modal_dialog() {
    let mut host = host();
    assert!(labels(&host).contains(&"Page A".to_owned()));
    show(&mut host, |s| s.dialog, true);
    let names = labels(&host);
    assert!(names.contains(&"OK".to_owned()) && names.contains(&"Cancel".to_owned()), "{names:?}");
    assert!(!names.contains(&"Page A".to_owned()) && !names.contains(&"Page B".to_owned()), "{names:?}");
    let dialog = host.ui.accessibility_nodes().iter().find(|n| n.control == host.ui.state.dialog.into()).expect("the dialog's node");
    assert_eq!((&*dialog.role, dialog.label.as_str(), dialog.modal), (Aria::DIALOG, "Settings", true));
    // Gone: the page is back at once.
    show(&mut host, |s| s.dialog, false);
    let names = labels(&host);
    assert!(names.contains(&"Page A".to_owned()) && !names.contains(&"OK".to_owned()), "{names:?}");
}

#[test]
fn tab_stays_in_the_modal() {
    let mut host = host();
    show(&mut host, |s| s.dialog, true);
    let mut seen = Vec::new();
    for _ in 0..6 {
        host.press_key("Tab");
        seen.push(keyboard_on(&host).unwrap_or_default());
    }
    assert!(seen.iter().all(|s| ["OK", "Cancel", "Menu"].contains(&s.as_str())), "{seen:?}");
}

#[test]
fn the_keyboard_moves_into_a_modal_and_back_when_it_goes() {
    let mut host = host();
    host.press_key("Tab");
    host.press_key("Tab");
    assert_eq!(keyboard_on(&host).as_deref(), Some("Page B"));
    show(&mut host, |s| s.dialog, true);
    assert_eq!(keyboard_on(&host).as_deref(), Some("OK"), "into its first item");
    show(&mut host, |s| s.dialog, false);
    assert_eq!(keyboard_on(&host).as_deref(), Some("Page B"), "back where it was");
}

#[test]
fn without_the_keyboard_in_use_the_focus_does_not_jump_in() {
    let mut host = host();
    show(&mut host, |s| s.dialog, true);
    assert_eq!(host.ui.accessibility_focused(), None);
}

#[test]
fn the_modal_on_top_wins_and_closing_it_hands_over() {
    let mut host = host();
    show(&mut host, |s| s.dialog, true);
    show(&mut host, |s| s.second, true);
    let names = labels(&host);
    assert!(names.contains(&"Yes".to_owned()) && !names.contains(&"OK".to_owned()), "the second is drawn on top: {names:?}");
    show(&mut host, |s| s.second, false);
    let names = labels(&host);
    assert!(names.contains(&"OK".to_owned()) && !names.contains(&"Yes".to_owned()) && !names.contains(&"Page A".to_owned()), "{names:?}");
}

#[test]
fn a_popup_window_opened_in_a_modal_stays_readable() {
    let mut host = host();
    show(&mut host, |s| s.dialog, true);
    let menu = host.rect(host.ui.state.open_menu);
    host.tap(menu.center_x(), menu.center_y());
    host.settle();
    let names = labels(&host);
    assert!(names.contains(&"Copy".to_owned()) && names.contains(&"OK".to_owned()), "{names:?}");
    assert!(!names.contains(&"Page A".to_owned()), "{names:?}");
}

#[test]
fn nothing_changes_without_a_flag() {
    let ui = Ui::new(App::default(), |_| SkiaLayout::new().fill().children((button("Page A"), SkiaLayout::new().fill().children((button("Over"),)))));
    let mut host = Headless::new(ui, 300, 200, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let names = labels(&host);
    assert!(names.contains(&"Page A".to_owned()) && names.contains(&"Over".to_owned()));
    assert!(host.ui.accessibility_nodes().iter().all(|n| !n.modal));
}

#[test]
fn a_modal_that_closes_on_a_tap_is_no_tab_stop() {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children((
            button("Page A"),
            SkiaLayout::new().fill().accessibility_label("Dialog").accessibility_modal(true).on_tapped(|_me, _app: &mut App, _cx| {}).children((button("OK"),)),
        ))
    });
    let mut host = Headless::new(ui, 300, 200, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    let dialog = host.ui.accessibility_nodes().iter().find(|n| n.label == "Dialog").cloned().expect("the dialog");
    assert!(!dialog.can_interact && dialog.modal);
    for _ in 0..3 {
        host.press_key("Tab");
        assert_eq!(keyboard_on(&host).as_deref(), Some("OK"));
    }
}
