//! What the toggles, the slider, the progress bar and the button give the accessibility snapshot
//! (roles, labels, states, tab stops), activation from the overlay and the slider's keys.
//! Upstream AccessibilitySnapshotTests and KeyboardAccessibilityTests, same numbers.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{AccessibilityNode, ControlId};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
/// Frames this far apart rebuild the snapshot every time.
const INTERVAL: f64 = drawnui::ACCESSIBILITY_INTERVAL_MS;

#[derive(Default)]
struct App {
    ids: Vec<ControlId>,
}

fn host<B: Into<drawnui::Detached>>(width: i32, height: i32, build: impl FnOnce(&mut App) -> B) -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT);
    let mut host = Headless::new(ui, width, height, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn by_role<'a>(host: &'a Headless<App>, role: &str) -> &'a AccessibilityNode {
    let mut nodes = host.ui.accessibility_nodes().iter().filter(|n| n.role == role);
    let node = nodes.next().unwrap_or_else(|| panic!("no {role} node"));
    assert!(nodes.next().is_none(), "more than one {role} node");
    node
}

fn key(host: &mut Headless<App>, name: &'static str) -> bool {
    host.ui.key(KeyKind::Down, name, "", Modifiers::default(), false)
}

fn end(host: &Headless<App>, id: ControlId) -> f32 {
    host.ui.tree.find::<SkiaSlider>(id).unwrap().p.end
}

/// A range control's value: now, min, max, step, text.
fn value(node: &AccessibilityNode) -> (f64, f64, f64, f64, &str) {
    let v = node.value.as_ref().expect("a value");
    (v.now, v.min, v.max, v.step, v.text.as_str())
}

/// A screen reader's Increment / Decrement move a slider one step, as the arrow keys, and SetValue
/// sets it; a progress bar has a value but no step.
#[test]
fn increment_and_decrement_move_a_slider_by_its_step() {
    let mut host = host(400, 200, |app: &mut App| {
        let slider = SkiaSlider::new().width_request(300).height_request(40).min(0).max(10).step(0.5).end(2);
        app.ids = vec![slider.id()];
        SkiaLayout::column().children((slider, SkiaProgress::new().width_request(300).value(40)))
    });
    let (id, node) = (host.ui.state.ids[0], by_role(&host, Aria::SLIDER).id);
    host.ui.accessibility_adjust(node, true);
    host.ui.accessibility_adjust(node, true);
    host.ui.accessibility_adjust(node, false);
    assert_eq!(end(&host, id), 2.5);
    // The next frame has it, inside the snapshot interval: a screen reader reads it back at once.
    host.frame_after(16.0);
    assert_eq!(value(by_role(&host, Aria::SLIDER)), (2.5, 0.0, 10.0, 0.5, ""));
    assert_eq!(value(by_role(&host, Aria::PROGRESS_BAR)).3, 0.0);
    // A value set by the screen reader lands on a step, inside the range.
    host.ui.accessibility_set_value(node, 7.3);
    assert_eq!(end(&host, id), 7.5);
    host.ui.accessibility_set_value(node, 40.0);
    assert_eq!(end(&host, id), 10.0);
}

/// A scroll is a node of its own that holds its content's nodes and pages it for a screen reader
/// (VoiceOver's three-finger swipe, TalkBack's scroll forward / back) by the viewport less a tenth.
#[test]
fn a_scroll_node_pages_its_content() {
    let mut host = host(300, 200, |app: &mut App| {
        let button = SkiaButton::new("Top").height_request(40).accessibility_role(Aria::BUTTON);
        let button_id = button.id();
        let scroll = SkiaScroll::new().fill().content(SkiaLayout::column().fill_x().children((button, SkiaLayout::new().height_request(1000).fill_x())));
        app.ids = vec![button_id, scroll.id()];
        scroll
    });
    let (button, scroll) = (host.ui.state.ids[0], host.ui.state.ids[1]);
    let node = by_role(&host, Aria::SCROLL_VIEW);
    assert_eq!((node.control, node.scrolls, node.can_interact), (scroll, (false, true), false));
    let id = node.id;
    let inside = host.ui.accessibility_nodes().iter().find(|n| n.control == button).unwrap();
    assert_eq!(inside.parent, Some(id));

    let offset = |host: &Headless<App>| host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
    host.ui.accessibility_scroll(id, 0.0, 1.0);
    host.settle();
    assert!((offset(&host) + 180.0).abs() <= 0.5, "{}", offset(&host));
    host.ui.accessibility_scroll(id, 0.0, -1.0);
    host.ui.accessibility_scroll(id, 0.0, -1.0);
    host.settle();
    assert_eq!(offset(&host), 0.0);

    // Content that fits: the node stays, with nothing to page.
    let fits = self::host(300, 200, |_| SkiaScroll::new().fill().content(SkiaLayout::new().height_request(50).fill_x()));
    assert_eq!(by_role(&fits, Aria::SCROLL_VIEW).scrolls, (false, false));
}

/// A screen reader's ScrollIntoView scrolls the scroll above a node until it shows.
#[test]
fn scroll_into_view_brings_a_node_on_screen() {
    let mut host = host(300, 200, |app: &mut App| {
        let far = SkiaButton::new("Far").height_request(40).accessibility_role(Aria::BUTTON);
        let far_id = far.id();
        let scroll = SkiaScroll::new().fill().content(SkiaLayout::column().fill_x().children((SkiaLayout::new().height_request(300).fill_x(), far)));
        app.ids = vec![far_id, scroll.id()];
        scroll
    });
    let (far, scroll) = (host.ui.state.ids[0], host.ui.state.ids[1]);
    // Below the viewport, within the snapshot (a screen beyond it).
    let node = host.ui.accessibility_nodes().iter().find(|n| n.control == far).expect("in the snapshot").id;
    host.ui.accessibility_scroll_into_view(node);
    host.settle();
    // The button's bottom (340) at the viewport's bottom (200), 8 points of padding.
    let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
    assert!((offset + 148.0).abs() <= 1.0, "{offset}");
}

/// AccessibilitySnapshotTests.Defaults_FeedTheNode_And_PresentationHidesInnerLabel.
#[test]
fn defaults_feed_the_node_and_the_inner_label_stays_out() {
    let mut host = host(400, 400, |app: &mut App| {
        let button = SkiaButton::new("Save").width_request(120).height_request(40).accessibility_role(Aria::BUTTON);
        let switch = SkiaSwitch::new().width_request(60).height_request(30);
        let slider = SkiaSlider::new().width_request(200).height_request(30).min(0).max(100).end(25);
        app.ids = vec![button.id(), switch.id(), slider.id()];
        SkiaLayout::column().fill().children((button, switch, slider, SkiaLabel::new("not exposed").width_request(120).height_request(20)))
    });
    let nodes = host.ui.accessibility_nodes();
    // The button, the switch, the slider; the plain label and the button's caption stay out.
    assert_eq!(nodes.len(), 3, "{nodes:?}");
    assert!(nodes.iter().all(|n| n.label != "not exposed"));
    let b = by_role(&host, Aria::BUTTON);
    assert_eq!((b.label.as_str(), b.can_interact, b.control), ("Save", true, host.ui.state.ids[0]));

    assert_eq!(by_role(&host, Aria::SWITCH).is_pressed, Some(false));
    host.ui.tree.find_mut::<SkiaSwitch>(host.ui.state.ids[1]).unwrap().set_is_toggled(true);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    assert_eq!(by_role(&host, Aria::SWITCH).is_pressed, Some(true));
    // The value is the value, not the name (C# DefaultAccessibilityLabel made it the name).
    let slider = by_role(&host, Aria::SLIDER);
    assert_eq!((slider.label.as_str(), value(slider)), ("", (25.0, 0.0, 100.0, 1.0, "")));

    let mut button = host.ui.tree.find_mut::<SkiaButton>(host.ui.state.ids[0]).unwrap();
    button.set_is_disabled(true);
    button.set_accessibility_label("Custom");
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    let b = by_role(&host, Aria::BUTTON);
    assert!(!b.can_interact);
    assert_eq!(b.label, "Custom");
}

#[test]
fn every_control_of_the_looks_page_has_its_role_label_and_state() {
    let mut host = host(400, 600, |app: &mut App| {
        let children = (
            SkiaCheckbox::new().is_toggled(true),
            SkiaRadioButton::new("One").is_toggled(true),
            SkiaProgress::new().value(65),
            SkiaSlider::new().enable_range(true).start(20).end(80),
            SkiaSwitch::new().responds_to_gestures(false),
        );
        app.ids = vec![children.3.id()];
        SkiaLayout::column().fill().children(children)
    });
    let checkbox = by_role(&host, Aria::CHECKBOX);
    assert_eq!((checkbox.is_pressed, checkbox.can_interact), (Some(true), true));
    let radio = by_role(&host, Aria::RADIO);
    assert_eq!((radio.label.as_str(), radio.is_pressed), ("One", Some(true)));
    // The caption is not a node of its own.
    assert!(host.ui.accessibility_nodes().iter().all(|n| n.role != Aria::TEXT));
    let progress = by_role(&host, Aria::PROGRESS_BAR);
    assert_eq!((progress.label.as_str(), value(progress)), ("", (65.0, 0.0, 100.0, 0.0, "65%")));
    assert_eq!(value(by_role(&host, Aria::SLIDER)), (80.0, 0.0, 100.0, 1.0, "20 \u{2013} 80"));
    // A switch that takes no taps is no tab stop.
    assert!(!by_role(&host, Aria::SWITCH).can_interact);

    // The value follows.
    host.ui.tree.find_mut::<SkiaSlider>(host.ui.state.ids[0]).unwrap().set_start(35);
    host.frame_after(INTERVAL);
    assert_eq!(value(by_role(&host, Aria::SLIDER)).4, "35 \u{2013} 80");
}

/// KeyboardAccessibilityTests.Slider_KeysStep_ActivationKeepsValue.
#[test]
fn slider_keys_step_and_activation_keeps_the_value() {
    let mut host = host(400, 200, |app: &mut App| {
        let slider = SkiaSlider::new().width_request(300).height_request(40).min(0).max(10).step(0.5).end(2);
        app.ids = vec![slider.id()];
        SkiaLayer::new().children(slider)
    });
    let id = host.ui.state.ids[0];
    // A tap at the center would move the value to 5.
    let node = by_role(&host, Aria::SLIDER).id;
    host.ui.accessibility_activate(node);
    host.frame_after(16.0);
    assert_eq!(end(&host, id), 2.0);

    host.ui.focus(Some(id));
    for (name, value) in [
        ("ArrowRight", 2.5),
        ("ArrowUp", 3.0),
        ("ArrowLeft", 2.5),
        ("PageUp", 3.5),
        ("End", 10.0),
        // At the end: used, stays.
        ("ArrowRight", 10.0),
        ("Home", 0.0),
    ] {
        assert!(key(&mut host, name), "{name}");
        assert_eq!(end(&host, id), value, "{name}");
    }
    assert!(!key(&mut host, "KeyA"));

    host.ui.tree.find_mut::<SkiaSlider>(id).unwrap().set_responds_to_gestures(false);
    host.frame_after(16.0);
    assert!(!key(&mut host, "ArrowRight"));
    assert_eq!(end(&host, id), 0.0);
}

/// KeyboardAccessibilityTests.Slider_WithoutStep_MovesHundredthOfRange.
#[test]
fn slider_without_step_moves_a_hundredth_of_the_range() {
    let mut host = host(400, 200, |app: &mut App| {
        let slider = SkiaSlider::new().width_request(300).height_request(40).min(0).max(2).step(0).end(1);
        app.ids = vec![slider.id()];
        SkiaLayer::new().children(slider)
    });
    let id = host.ui.state.ids[0];
    let r = host.rect(id);
    // The white dot in the middle of the thumb: at 0.5 of 265 points, plus half the 35 point box.
    let (x, y) = ((r.left + 150.0) as i32, r.center_y() as i32);
    assert_eq!(host.pixel(x, y), Color::WHITE);
    host.ui.focus(Some(id));
    key(&mut host, "ArrowRight");
    assert!((end(&host, id) - 1.02).abs() < 1e-6);
    key(&mut host, "PageDown");
    assert!((end(&host, id) - 0.82).abs() < 1e-6);
    // The thumb follows at once: 0.82 of 2 over 300 - 35 points.
    host.frame_after(16.0);
    let thumb = host.ui.tree.find::<SkiaSlider>(id).unwrap().end_thumb_x;
    assert!((thumb - 0.41 * 265.0).abs() < 1e-3, "{thumb}");
    // And is drawn there once its ImageDoubleBuffered bitmap is back (DrawnUI's default cache):
    // the track where the thumb was.
    host.frame();
    assert_eq!(host.pixel(x, y), Color::from_rgb(0xD7, 0xDB, 0xE0));
}

/// KeyboardAccessibilityTests.Toggles_FlipOnActivation_AndReportState.
#[test]
fn toggles_flip_on_activation_and_report_the_state() {
    let mut host = host(400, 200, |app: &mut App| {
        let (switch, check) = (SkiaSwitch::new().width_request(60).height_request(30), SkiaCheckbox::new().width_request(30).height_request(30));
        app.ids = vec![switch.id(), check.id()];
        SkiaStack::new().children((switch, check))
    });
    let (switch, check) = (by_role(&host, Aria::SWITCH).id, by_role(&host, Aria::CHECKBOX).id);
    host.ui.accessibility_activate(switch);
    host.ui.accessibility_activate(check);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    let on = |host: &Headless<App>, i: usize| host.ui.tree.find::<SkiaToggle>(host.ui.state.ids[i]).unwrap().p.is_toggled;
    assert!(on(&host, 0) && on(&host, 1));
    assert_eq!(by_role(&host, Aria::SWITCH).is_pressed, Some(true));
    assert_eq!(by_role(&host, Aria::CHECKBOX).is_pressed, Some(true));

    host.ui.accessibility_activate(switch);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    assert!(!on(&host, 0));
    assert_eq!(by_role(&host, Aria::SWITCH).is_pressed, Some(false));
}

/// C# SkiaAccessibilityManager: the node the overlay's keyboard focus is on gets the keys, so a
/// slider moves from the keyboard without being `Ui::focused`.
#[test]
fn the_overlay_focused_slider_takes_the_arrow_keys() {
    let mut host = host(400, 200, |app: &mut App| {
        let slider = SkiaSlider::new().width_request(300).height_request(40).min(0).max(10).step(1).end(4);
        app.ids = vec![slider.id()];
        SkiaLayer::new().children(slider)
    });
    let id = host.ui.state.ids[0];
    assert!(!key(&mut host, "ArrowRight"), "nobody has the focus yet");
    let node = by_role(&host, Aria::SLIDER).id;
    host.ui.accessibility_focus(node, true);
    assert!(key(&mut host, "ArrowRight"));
    assert!(key(&mut host, "ArrowRight"));
    assert_eq!(end(&host, id), 6.0);
    host.frame_after(INTERVAL);
    assert_eq!(value(by_role(&host, Aria::SLIDER)).0, 6.0);
    host.ui.accessibility_focus(node, false);
    assert!(!key(&mut host, "ArrowLeft"));
    assert_eq!(end(&host, id), 6.0);
}
