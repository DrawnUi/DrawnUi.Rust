//! Without a screen reader, the keyboard focus on a control that goes away (hidden, removed: a pin
//! turned to the back of a globe) moves on to the next Tab stop that is still there, never to
//! nothing (DrawnUi.Net KeyboardTabScrollTests); a whole screen replaced leaves the keys to the app.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    buttons: [Handle<SkiaButton>; 3],
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let [a, b, c] = &mut app.buttons;
        SkiaLayout::column().fill().padding(10).spacing(10).children((
            SkiaButton::new("One").width_request(120).height_request(40).assign(a),
            SkiaButton::new("Two").width_request(120).height_request(40).assign(b),
            SkiaButton::new("Three").width_request(120).height_request(40).assign(c),
        ))
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn frames(host: &mut Headless<App>) {
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    host.frame_after(1100.0);
    host.frame_after(16.0);
}

fn on(host: &Headless<App>) -> Option<String> {
    let focused = host.ui.accessibility_focused()?;
    host.ui.accessibility_nodes().iter().find(|n| n.control == focused).map(|n| n.label.clone())
}

#[test]
fn a_focused_control_that_hides_hands_the_focus_to_the_next_stop() {
    let mut host = host();
    host.ui.set_screen_reader_running(false);
    host.press_key("Tab");
    host.press_key("Tab");
    frames(&mut host);
    assert_eq!(on(&host).as_deref(), Some("Two"));
    let two = host.ui.state.buttons[1];
    host.ui.tree.get_mut(two).unwrap().set_is_visible(false);
    frames(&mut host);
    assert_eq!(on(&host).as_deref(), Some("Three"), "the next stop has the keyboard");
}

#[test]
fn the_last_stop_hands_it_back_to_the_one_before() {
    let mut host = host();
    host.ui.set_screen_reader_running(false);
    for _ in 0..3 {
        host.press_key("Tab");
    }
    frames(&mut host);
    assert_eq!(on(&host).as_deref(), Some("Three"));
    let three = host.ui.state.buttons[2];
    host.ui.tree.get_mut(three).unwrap().set_is_visible(false);
    frames(&mut host);
    assert_eq!(on(&host).as_deref(), Some("Two"), "no stop after it: the one before");
}
