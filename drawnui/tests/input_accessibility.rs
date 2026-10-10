//! The accessibility snapshot (React SkiaAccessibilityManager, DrawnUi AccessibilityManager) the
//! web host renders as an ARIA overlay: which controls are nodes, their labels, states and drawn
//! rects, how often it is rebuilt, and activation from the overlay.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
/// React MinUpdateIntervalMs; frames this far apart rebuild every time.
const INTERVAL: f64 = drawnui::ACCESSIBILITY_INTERVAL_MS;

fn enabled<S: 'static>(ui: Ui<S>, width: i32, height: i32, scale: f32) -> Headless<S> {
    let mut host = Headless::new(ui, width, height, scale);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn node<'a, S>(host: &'a Headless<S>, label: &str) -> &'a AccessibilityNode {
    host.ui.accessibility_nodes().iter().find(|n| n.label == label).unwrap_or_else(|| panic!("no node {label}"))
}

#[derive(Default)]
struct Panel {
    panel: Handle<SkiaLayout>,
    moved: Handle<SkiaShape>,
    scaled: Handle<SkiaShape>,
}

/// AccessibilityRectTests.Rect_FollowsTranslationAndScale_OfControlAndAncestors, same numbers.
#[test]
fn rects_follow_the_transforms_of_the_control_and_its_ancestors() {
    let switch = |label: &str| SkiaShape::new().width_request(60).height_request(30).accessibility_role(Aria::SWITCH).accessibility_label(label);
    let ui = Ui::new(Panel::default(), |app| {
        SkiaLayer::new().fill().children(SkiaLayout::new().width_request(200).height_request(200).assign(&mut app.panel).children((
            switch("moved").assign(&mut app.moved),
            switch("scaled").margin((0.0, 100.0, 0.0, 0.0)).assign(&mut app.scaled),
        )))
    });
    let mut host = enabled(ui, 400, 400, 1.0);
    assert_eq!(node(&host, "moved").rect, Rect::new(0.0, 0.0, 60.0, 30.0));

    let (panel, moved, scaled) = (host.ui.state.panel, host.ui.state.moved, host.ui.state.scaled);
    host.ui.tree.get_mut(moved).unwrap().set_translation_x(20);
    host.ui.tree.get_mut(panel).unwrap().set_translation_y(50);
    host.ui.tree.get_mut(scaled).unwrap().set_scale_x(2);
    host.ui.tree.get_mut(scaled).unwrap().set_scale_y(2);
    host.frame_after(INTERVAL);
    assert_eq!(node(&host, "moved").rect, Rect::new(20.0, 50.0, 80.0, 80.0));
    // 60 x 30 at (0, 100) scaled 2x around its center (30, 115), then the panel's +50.
    assert_eq!(node(&host, "scaled").rect, Rect::new(-30.0, 135.0, 90.0, 195.0));
}

#[derive(Default)]
struct Group {
    group: Handle<SkiaLayout>,
    first: Handle<SkiaShape>,
}

fn button(label: &str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(120)
        .height_request(40)
        .accessibility_role(Aria::BUTTON)
        .accessibility_label(label)
        .accessibility_can_interact(Some(true))
}

/// AccessibilitySnapshotTests.HiddenAncestor_PrunesNodes_And_ChangedFiresOnlyOnRealChange.
#[test]
fn a_hidden_ancestor_prunes_its_nodes_and_only_a_real_change_is_a_new_snapshot() {
    let ui = Ui::new(Group::default(), |app| {
        SkiaLayout::column().fill().children((
            button("First").assign(&mut app.first),
            SkiaLayout::column().fill_x().assign(&mut app.group).children(button("Inner")),
        ))
    });
    let mut host = enabled(ui, 400, 400, 1.0);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    assert_eq!(host.ui.accessibility_nodes().len(), 2);
    // Two more identical rebuilds stayed silent.
    assert_eq!(host.ui.accessibility_revision(), 1);

    let group = host.ui.state.group;
    host.ui.tree.get_mut(group).unwrap().set_is_visible(false);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    assert_eq!(host.ui.accessibility_nodes().len(), 1);
    assert_eq!(host.ui.accessibility_nodes()[0].label, "First");
    assert_eq!(host.ui.accessibility_revision(), 2);

    host.ui.tree.get_mut(group).unwrap().set_is_visible(true);
    host.frame_after(INTERVAL);
    host.frame_after(INTERVAL);
    assert_eq!(host.ui.accessibility_nodes().len(), 2);
    assert_eq!(host.ui.accessibility_revision(), 3);
}

/// A toggle as a control reports it through the hooks: default role, state, whether it is usable.
struct Toggle {
    on: Rc<Cell<bool>>,
    disabled: bool,
}
impl Control for Toggle {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(60.0, 30.0)
    }
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::SWITCH)
    }
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some("Sound".into())
    }
    fn accessibility_is_pressed(&self) -> Option<bool> {
        Some(self.on.get())
    }
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(!self.disabled)
    }
}

#[derive(Default)]
struct Page {
    on: Rc<Cell<bool>>,
    taps: u32,
    card: Handle<SkiaShape>,
}

/// AccessibilitySnapshotTests.Defaults_FeedTheNode_And_PresentationHidesInnerLabel, with the
/// defaults of controls given by a custom control (SkiaSwitch / SkiaSlider / SkiaButton defaults are
/// the controls' own ports), plus React's default role per type.
#[test]
fn defaults_feed_the_node_and_presentation_hides_a_control() {
    let ui = Ui::new(Page::default(), |app| {
        SkiaLayout::column().fill().children((
            Build::new(Toggle { on: app.on.clone(), disabled: false }),
            Build::new(Toggle { on: app.on.clone(), disabled: true }).accessibility_label("Muted"),
            SkiaShape::new()
                .width_request(200)
                .height_request(60)
                .accessibility_role(Aria::BUTTON)
                .accessibility_label("Open settings")
                .accessibility_hint("A card that acts as a button")
                .assign(&mut app.card)
                .on_tapped(|_me, app: &mut Page, _cx| app.taps += 1)
                .children(SkiaLabel::new("Settings").accessibility_role(Aria::PRESENTATION)),
            SkiaLabel::new("Read by default"),
            SkiaLabel::new("Heading").accessibility_role(Aria::HEADING).accessibility_live(Aria::LIVE_POLITE),
        ))
    })
    .font_bytes("Default", FONT)
    .default_accessibility_role::<SkiaLabel>(Aria::TEXT);
    let mut host = enabled(ui, 400, 600, 2.0);

    let labels: Vec<&str> = host.ui.accessibility_nodes().iter().map(|n| n.label.as_str()).collect();
    // Top to bottom; the card's inner label is presentation.
    assert_eq!(labels, ["Sound", "Muted", "Open settings", "Read by default", "Heading"]);
    let sound = node(&host, "Sound");
    assert_eq!((&*sound.role, sound.is_pressed, sound.can_interact), (Aria::SWITCH, Some(false), true));
    // Points at scale 2.
    assert_eq!(sound.rect, Rect::new(0.0, 0.0, 30.0, 15.0));
    assert!(!node(&host, "Muted").can_interact);
    let card = node(&host, "Open settings");
    assert_eq!((&*card.role, card.hint.as_str(), card.can_interact), (Aria::BUTTON, "A card that acts as a button", true));
    assert_eq!(node(&host, "Read by default").role, Aria::TEXT);
    let heading = node(&host, "Heading");
    assert_eq!((&*heading.role, &*heading.live, heading.can_interact), (Aria::HEADING, Aria::LIVE_POLITE, false));

    // The state follows at the next rebuild.
    host.ui.state.on.set(true);
    host.ui.state_changed();
    host.frame_after(INTERVAL);
    assert_eq!(node(&host, "Sound").is_pressed, Some(true));

    // The overlay activates the card: a tap at its center.
    let id = node(&host, "Open settings").id;
    host.ui.accessibility_activate(id);
    assert_eq!(host.ui.state.taps, 1);
    host.ui.accessibility_focus(id, true);
    assert_eq!(host.ui.accessibility_focused(), Some(host.ui.state.card.id()));
    host.ui.accessibility_focus(id, false);
    assert_eq!(host.ui.accessibility_focused(), None);
}

#[test]
fn rebuilt_at_most_once_an_interval_with_one_frame_after_it_and_never_when_off() {
    let ui = Ui::new(Group::default(), |app| SkiaLayout::column().fill().children(button("First").assign(&mut app.first)));
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    // Off: nothing is built, nothing wakes the host.
    assert!(host.ui.accessibility_nodes().is_empty());
    assert_eq!(host.ui.wake_at(), None);

    host.ui.set_accessibility_enabled(true);
    host.settle();
    assert_eq!(host.ui.accessibility_nodes().len(), 1);
    let built = host.time_ms();
    // A change inside the interval waits for it; the host is woken once, when it has passed.
    let first = host.ui.state.first;
    host.ui.tree.get_mut(first).unwrap().set_accessibility_label("Renamed");
    host.frame_after(100.0);
    assert_eq!(host.ui.accessibility_nodes()[0].label, "First");
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(built + INTERVAL));
    host.settle();
    assert_eq!(host.ui.accessibility_nodes()[0].label, "Renamed");
    // Rebuilt and idle: no frame every second.
    assert_eq!(host.ui.wake_at(), None);
}

/// Two drawn lines, as a label reports them.
struct Paragraph;
impl Control for Paragraph {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(200.0, 40.0)
    }
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some("First line Second".into())
    }
    fn accessibility_text_lines(&self, scale: f32) -> Vec<drawnui::AccessibilityTextLine> {
        let line = |text: &str, top: f32| drawnui::AccessibilityTextLine {
            text: text.into(),
            left: 0.0,
            top: top / scale,
            width: 150.0 / scale,
            height: 20.0 / scale,
            font_family: "Default".into(),
            font_weight: 400,
            font_size: 14.0,
        };
        vec![line("First line", 0.0), line("Second", 20.0)]
    }
}

/// React AccessibilityTextSelectable: opt-in per control; the node carries the drawn lines.
#[test]
fn selectable_text_carries_its_lines_only_when_asked() {
    let ui = Ui::new((), |_| {
        SkiaLayout::column().fill().children((
            Build::new(Paragraph).accessibility_role(Aria::TEXT).accessibility_text_selectable(true),
            Build::new(Paragraph).accessibility_role(Aria::TEXT).accessibility_label("Plain"),
        ))
    });
    let host = enabled(ui, 400, 400, 2.0);
    let selectable = &host.ui.accessibility_nodes()[0];
    let lines: Vec<(&str, f32)> = selectable.text_lines.iter().map(|l| (l.text.as_str(), l.top)).collect();
    assert_eq!(lines, [("First line", 0.0), ("Second", 10.0)]);
    assert!(node(&host, "Plain").text_lines.is_empty());
}

#[derive(Default)]
struct List {
    scroll: Handle<SkiaScroll>,
    rows: Vec<Handle<SkiaShape>>,
    keys: Vec<String>,
}

/// React SkiaScroll.EnsureVisible on overlay focus, and C# SkiaAccessibilityManager handing the
/// keys the focused control left to the node the overlay's focus is on.
#[test]
fn overlay_focus_scrolls_the_node_into_view_and_takes_the_keys() {
    let ui = Ui::new(List::default(), |app| {
        let rows: Vec<Build<SkiaShape>> = (0..20)
            .map(|i| {
                let mut handle = Handle::default();
                let row = SkiaShape::new()
                    .fill_x()
                    .height_request(50)
                    .accessibility_role(Aria::SLIDER)
                    .accessibility_label(format!("Row {i}"))
                    .assign(&mut handle)
                    .on_key_down(move |_me, app: &mut List, _cx, event| {
                        app.keys.push(format!("row {i} {}", event.key));
                        event.key == "ArrowRight"
                    });
                app.rows.push(handle);
                row
            })
            .collect();
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().spacing(0).fill_x().children(rows))
    });
    let mut host = enabled(ui, 300, 300, 1.0);
    let id = node(&host, "Row 9").id;
    host.ui.accessibility_focus(id, true);
    host.settle();
    // Row 9 is 450..500 in a 300 tall viewport: scrolled until its bottom is 8 points above the end.
    let scroll = host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap();
    assert_eq!(scroll.viewport_offset_y(), -208.0);
    // Already in view: nothing moves.
    host.ui.accessibility_focus(node(&host, "Row 8").id, true);
    host.settle();
    assert_eq!(host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y(), -208.0);

    // Nothing is focused: the overlay's node gets the key.
    assert!(host.press_key("ArrowRight"));
    assert!(!host.press_key("KeyA"));
    assert_eq!(host.ui.state.keys, ["row 8 ArrowRight", "row 8 KeyA"]);
}

#[test]
fn a_role_and_live_region_can_come_from_data() {
    // The same text an app reads from data or a binding (as DrawnUI's AccessibilityRole string),
    // not only an `Aria` constant; set at build time and at run time.
    let (role, live) = (String::from("button"), String::from("polite"));
    let card = SkiaShape::new().width_request(100).height_request(40).accessibility_label("Card").accessibility_role(role).accessibility_live(live);
    let id = card.id();
    let ui = Ui::new((), move |_| SkiaLayout::new().fill().children(card)).font_bytes("Default", FONT);
    let mut host = enabled(ui, 300, 300, 1.0);
    let card = node(&host, "Card");
    assert_eq!((&*card.role, &*card.live), (Aria::BUTTON, Aria::LIVE_POLITE));

    host.ui.tree.any_mut(id).unwrap().set_accessibility_role(String::from("switch"));
    host.frame_after(INTERVAL);
    host.settle();
    assert_eq!(&*node(&host, "Card").role, Aria::SWITCH);
}
