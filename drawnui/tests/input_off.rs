//! DrawnUI 6w: `responds_to_gestures` is the one switch that turns a control's input off, and
//! accessibility follows it (DrawnUi.Net RespondsToGesturesTests).
//! A tap on a switched-off button, toggle or radio stops there and does nothing; moves pass on; a
//! scroll that is off still lets its content take taps.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    log: Vec<&'static str>,
    button: Handle<SkiaButton>,
    switch: Handle<SkiaSwitch>,
    radio: Handle<SkiaRadioButton>,
}

/// A tappable page with a button, a switch and a radio on it, all switched off.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().on_tapped(|_me, app: &mut App, _cx| app.log.push("page")).children(
            SkiaLayout::column().padding(20).spacing(20).children((
                SkiaButton::new("Off")
                    .responds_to_gestures(false)
                    .assign(&mut app.button)
                    .on_tapped(|_me, app: &mut App, _cx| app.log.push("button"))
                    .on_down(|_me, app: &mut App, _cx, _at| app.log.push("down")),
                SkiaSwitch::new().responds_to_gestures(false).assign(&mut app.switch),
                SkiaRadioButton::new("R").responds_to_gestures(false).assign(&mut app.radio),
            )),
        )
    })
    // Roles as an app gives them (HelloRust): the button's is the app's.
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    host
}

fn tap(host: &mut Headless<App>, id: ControlId) {
    let r = host.rect(id);
    host.tap(r.center_x(), r.center_y());
    host.settle();
}

#[test]
fn a_tap_on_a_switched_off_control_stops_there_and_does_nothing() {
    let mut host = host();
    let (button, switch, radio) = (host.ui.state.button.id(), host.ui.state.switch.id(), host.ui.state.radio.id());
    tap(&mut host, button);
    tap(&mut host, switch);
    tap(&mut host, radio);
    assert_eq!(host.ui.state.log, Vec::<&str>::new(), "no handler, nothing under it");
    assert!(!host.ui.tree.find::<SkiaButton>(button).unwrap().is_pressed);
    assert!(!host.ui.tree.find::<SkiaToggle>(switch).unwrap().p.is_toggled);
    assert!(!host.ui.tree.find::<SkiaToggle>(radio).unwrap().p.is_toggled);

    // Switched on at run time: the same taps act.
    host.ui.tree.get_mut(host.ui.state.button).unwrap().set_responds_to_gestures(true);
    host.ui.tree.get_mut(host.ui.state.switch).unwrap().set_responds_to_gestures(true);
    host.settle();
    tap(&mut host, button);
    tap(&mut host, switch);
    assert_eq!(host.ui.state.log, ["down", "button"]);
    assert!(host.ui.tree.find::<SkiaToggle>(switch).unwrap().p.is_toggled);
}

#[test]
fn keyboard_and_screen_readers_see_a_switched_off_control_disabled() {
    let mut host = host();
    host.ui.set_accessibility_enabled(true);
    host.frame_after(1100.0);
    let can_interact = |host: &Headless<App>, id: ControlId| host.ui.accessibility_nodes().iter().find(|n| n.control == id).map(|n| n.can_interact);
    let (button, switch, radio) = (host.ui.state.button.id(), host.ui.state.switch.id(), host.ui.state.radio.id());
    assert_eq!([can_interact(&host, button), can_interact(&host, switch), can_interact(&host, radio)], [Some(false); 3]);
    host.ui.tree.get_mut(host.ui.state.button).unwrap().set_responds_to_gestures(true);
    host.frame_after(1100.0);
    assert_eq!(can_interact(&host, button), Some(true), "a change at run time reaches the snapshot");
}

#[test]
fn a_scroll_that_is_off_lets_its_content_take_taps() {
    let ui = Ui::new(Vec::<&'static str>::new(), |_| {
        SkiaScroll::new().fill().responds_to_gestures(false).content(
            SkiaLayout::column().children(SkiaButton::new("In").on_tapped(|_me, log: &mut Vec<&'static str>, _cx| log.push("in"))),
        )
    });
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    host.tap(40.0, 20.0);
    host.settle();
    assert_eq!(host.ui.state, ["in"]);
}

#[test]
#[allow(deprecated)]
fn is_disabled_is_the_opposite_of_responds_to_gestures() {
    let mut button = Handle::<SkiaButton>::default();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(SkiaButton::new("B").is_disabled(true).assign(&mut button)));
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert!(!host.ui.tree.find::<SkiaButton>(button).unwrap().p.responds_to_gestures);
    host.ui.tree.get_mut(button).unwrap().set_is_disabled(false);
    assert!(host.ui.tree.find::<SkiaButton>(button).unwrap().p.responds_to_gestures);
}
