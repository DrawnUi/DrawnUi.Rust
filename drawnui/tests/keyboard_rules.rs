//! Keyboard rules shared with DrawnUi.Net (drawnui-cross 6c; C# KeyboardNavigationTests,
//! AccessibilityModalTests): arrows that start navigation when the app turns it on, a focus behind
//! an open dialog that counts as none, and the focus when its control leaves the screen.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    pressed: Vec<&'static str>,
    dialog: Handle<SkiaLayout>,
    first: Handle<SkiaShape>,
}

fn button(name: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(80)
        .height_request(30)
        .accessibility_role(Aria::BUTTON)
        .accessibility_label(name)
        .on_tapped(move |_me, app: &mut App, _cx| app.pressed.push(name))
}

/// Three page buttons; over them a hidden modal dialog with OK.
fn host(arrows: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((
            SkiaLayout::column().spacing(10).padding(10).children((button("One").assign(&mut app.first), button("Two"), button("Three"))),
            SkiaLayout::column()
                .padding((200, 10, 0, 0))
                .is_visible(false)
                .accessibility_label("Dialog")
                .accessibility_modal(true)
                .assign(&mut app.dialog)
                .children((button("OK"),)),
        ))
    })
    .arrows_start_keyboard_navigation(arrows);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    host
}

fn keyboard_on(host: &Headless<App>) -> Option<String> {
    host.ui.accessibility_focused().and_then(|f| host.ui.tree.base(f)).map(|b| b.p.accessibility_label.to_string())
}

fn frames(host: &mut Headless<App>) {
    host.frame_after(16.0);
    host.frame_after(16.0);
}

#[test]
fn arrows_start_navigation_only_when_the_app_turns_it_on() {
    let mut off = host(false);
    assert!(off.ui.has_no_keyboard_focus());
    off.press_key("ArrowDown");
    assert_eq!(keyboard_on(&off), None, "off by default: an arrow does nothing");

    let mut on = host(true);
    on.press_key("ArrowDown");
    assert_eq!(keyboard_on(&on).as_deref(), Some("One"), "Down / Right: as Tab");
    assert!(!on.ui.has_no_keyboard_focus());
    // With the keyboard on a control, the arrows are what they were (no group here: nothing).
    on.press_key("ArrowDown");
    assert_eq!(keyboard_on(&on).as_deref(), Some("One"));

    let mut back = host(true);
    back.press_key("ArrowUp");
    assert_eq!(keyboard_on(&back).as_deref(), Some("Three"), "Up / Left: as Shift+Tab");
}

#[test]
fn a_focus_behind_an_open_dialog_is_no_focus() {
    let mut host = host(true);
    // The keyboard on a page button with its ring hidden (as after a mouse click), then the dialog
    // opens: the keyboard is not in use, so it stays behind the dialog.
    let one = host.ui.accessibility_nodes().iter().find(|n| n.label == "One").map(|n| n.id).unwrap();
    host.ui.accessibility_focus(one, true);
    if let Some(mut d) = host.ui.tree.cx().get_mut(host.ui.state.dialog) {
        d.set_is_visible(true);
    }
    frames(&mut host);
    assert_eq!(keyboard_on(&host).as_deref(), Some("One"));
    assert!(host.ui.has_no_keyboard_focus(), "behind the active modal");
    // Enter does not press it.
    host.press_key("Enter");
    host.settle();
    assert!(host.ui.state.pressed.is_empty(), "{:?}", host.ui.state.pressed);
    // An arrow starts inside the dialog.
    host.press_key("ArrowDown");
    assert_eq!(keyboard_on(&host).as_deref(), Some("OK"));
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.pressed, ["OK"]);
}

#[test]
fn without_a_screen_reader_the_focus_clears_when_its_control_leaves() {
    let mut host = host(false);
    host.ui.set_screen_reader_running(false);
    host.press_key("Tab");
    assert_eq!(keyboard_on(&host).as_deref(), Some("One"));
    if let Some(mut c) = host.ui.tree.cx().get_mut(host.ui.state.first) {
        c.set_is_visible(false);
    }
    host.frame_after(1100.0);
    host.frame_after(16.0);
    assert_eq!(host.ui.accessibility_focused(), None, "the keys are the app's again");
}

#[test]
fn with_a_screen_reader_the_focus_follows_where_the_reader_moves() {
    let mut host = host(false);
    host.ui.set_screen_reader_running(true);
    host.press_key("Tab");
    assert_eq!(keyboard_on(&host).as_deref(), Some("One"));
    if let Some(mut c) = host.ui.tree.cx().get_mut(host.ui.state.first) {
        c.set_is_visible(false);
    }
    host.frame_after(1100.0);
    host.frame_after(16.0);
    assert_eq!(keyboard_on(&host).as_deref(), Some("Two"), "the first node on screen");
}
