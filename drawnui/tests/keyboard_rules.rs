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

/// `Cx::set_accessibility_focus`: app code moves the keyboard and screen-reader focus (a custom
/// menu tells readers where its highlight is); a control not drawn yet gets it once it is.
#[test]
fn app_code_moves_the_accessibility_focus() {
    let mut host = host(false);
    let two = host.ui.accessibility_nodes().iter().find(|n| n.label == "Two").map(|n| n.control).expect("Two");
    host.ui.tree.cx().set_accessibility_focus(two);
    host.frame_after(16.0);
    assert_eq!(keyboard_on(&host).as_deref(), Some("Two"));
    // The dialog's OK is not drawn yet: it gets the focus once the dialog shows.
    let ok: ControlId = host.ui.state.dialog.into();
    let ok = host.ui.tree.children(ok)[0];
    host.ui.tree.cx().set_accessibility_focus(ok);
    host.frame_after(16.0);
    assert_eq!(keyboard_on(&host).as_deref(), Some("Two"), "OK is hidden");
    if let Some(mut d) = host.ui.tree.cx().get_mut(host.ui.state.dialog) {
        d.set_is_visible(true);
    }
    frames(&mut host);
    assert_eq!(keyboard_on(&host).as_deref(), Some("OK"));
}

#[derive(Default)]
struct Heard {
    /// (key, handled) as the window-level handler heard them, and what the buttons saw by then.
    keys: Vec<(String, bool, Vec<&'static str>)>,
    pressed: Vec<&'static str>,
}

fn heard_button(name: &'static str) -> Build<SkiaShape> {
    SkiaShape::new().width_request(80).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label(name).on_tapped(move |_me, app: &mut Heard, _cx| app.pressed.push(name))
}

/// The canvas's keyboard navigation runs before the window-level key handlers, which learn
/// whether it used the key (`KeyEvent::handled`, DrawnUI KeyboardManager.IsKeyHandled); a host can
/// mark a key it used itself (`mark_key_handled`, the browser overlay).
#[test]
fn window_handlers_hear_whether_navigation_used_the_key() {
    let ui = Ui::new(Heard::default(), |_| {
        SkiaLayout::new().fill().children((SkiaLayout::row().spacing(10).padding(10).accessibility_role(Aria::TOOLBAR).children((heard_button("One"), heard_button("Two"))),))
    })
    .on_key_down(|app: &mut Heard, event, _cx| {
        app.keys.push((event.key.to_owned(), event.handled, app.pressed.clone()));
        false
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    host.press_key("Tab");
    host.press_key("ArrowRight");
    host.press_key("Enter");
    host.press_key("KeyQ");
    host.settle();
    let keys: Vec<(&str, bool)> = host.ui.state.keys.iter().map(|(k, h, _)| (k.as_str(), *h)).collect();
    assert_eq!(keys, [("Tab", true), ("ArrowRight", true), ("Enter", true), ("KeyQ", false)]);
    // Enter reached the app after it pressed Two.
    assert_eq!(host.ui.state.keys[2].2, ["Two"]);
    // A key the host used itself: handled for the app, and navigation leaves it.
    host.ui.mark_key_handled("Enter");
    host.press_key("Enter");
    host.settle();
    assert_eq!(host.ui.state.keys.last().map(|(k, h, _)| (k.as_str(), *h)), Some(("Enter", true)));
    assert_eq!(host.ui.state.pressed, ["Two"], "the marked Enter pressed nothing more");
}

#[derive(Default)]
struct Game {
    /// (key, handled) as a control listening to every key heard them.
    heard: Vec<(String, bool)>,
    pressed: Vec<&'static str>,
}

/// A control that listens to every key (a game) hears the key after the canvas's keyboard
/// navigation and reads `KeyEvent::handled`; its return value claims nothing from navigation.
#[test]
fn a_key_listener_hears_after_navigation_with_handled() {
    let button = |name: &'static str| {
        SkiaShape::new().width_request(80).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label(name).on_tapped(move |_me, app: &mut Game, _cx| app.pressed.push(name))
    };
    let ui = Ui::new(Game::default(), move |_| {
        SkiaLayout::new().fill().listen_keys().on_key_down(|_me, app: &mut Game, _cx, event| {
            app.heard.push((event.key.to_owned(), event.handled));
            true
        }).children((SkiaLayout::row().spacing(10).padding(10).children((button("One"), button("Two"))),))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_keyboard_navigation(true);
    host.settle();
    host.press_key("Tab");
    host.press_key("Space");
    host.press_key("KeyQ");
    host.settle();
    let heard: Vec<(&str, bool)> = host.ui.state.heard.iter().map(|(k, h)| (k.as_str(), *h)).collect();
    assert_eq!(heard, [("Tab", true), ("Space", true), ("KeyQ", false)]);
    assert_eq!(host.ui.state.pressed, ["One"], "the listener's true did not keep Space from pressing One");
}
