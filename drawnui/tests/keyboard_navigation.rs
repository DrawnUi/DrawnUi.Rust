//! Keyboard navigation on a host with none of its own (the desktop): ports of C#
//! `KeyboardNavigationTests` and the focus ring of `KeyboardAccessibilityTests`. Tab walks the
//! tab stops in reading order with a group (list, toolbar...) as one stop; arrows walk the group;
//! Enter and Space activate; Escape leaves; the ring shows after keyboard use only.

use drawnui::prelude::*;
use drawnui::PointerKind;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    taps: Vec<&'static str>,
    a: Handle<SkiaButton>,
    b: Handle<SkiaButton>,
    items: Vec<Handle<SkiaButton>>,
    d: Handle<SkiaButton>,
}

fn button(name: &'static str) -> Build<SkiaButton> {
    SkiaButton::new(name).width_request(120).height_request(40).on_tapped(move |_me, app: &mut App, _cx| app.taps.push(name))
}

/// A, B, a list of three items, D, in a column; navigation on.
fn scene() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let mut items = vec![Handle::default(); 3];
        let list = SkiaLayout::column().spacing(4).accessibility_role(Aria::LIST).children((
            button("item 1").assign(&mut items[0]),
            button("item 2").assign(&mut items[1]),
            button("item 3").assign(&mut items[2]),
        ));
        app.items = items;
        SkiaLayout::column().spacing(8).padding(20).children((
            button("A").assign(&mut app.a),
            button("B").assign(&mut app.b),
            list,
            button("D").assign(&mut app.d),
        ))
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 500, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

/// The snapshot id of a control (what the overlay hands back).
fn id_of<S>(host: &Headless<S>, control: ControlId) -> u32 {
    host.ui.accessibility_nodes().iter().find(|n| n.control == control).expect("a node").id
}

fn focused(host: &Headless<App>) -> Option<ControlId> {
    host.ui.accessibility_focused()
}

fn tab(host: &mut Headless<App>, back: bool) -> Option<ControlId> {
    let modifiers = Modifiers { shift: back, ..Modifiers::default() };
    assert!(host.key_down("Tab", modifiers), "Tab is used");
    host.key_up("Tab", modifiers);
    focused(host)
}

#[test]
fn tab_walks_stops_group_is_one_stop_shift_tab_back_leaves_past_the_end() {
    let mut host = scene();
    let (a, b, d, first) = (host.ui.state.a.id(), host.ui.state.b.id(), host.ui.state.d.id(), host.ui.state.items[0].id());
    assert_eq!(tab(&mut host, false), Some(a));
    assert_eq!(tab(&mut host, false), Some(b));
    assert_eq!(tab(&mut host, false), Some(first), "the list is one stop: its first item");
    assert_eq!(tab(&mut host, false), Some(d), "not item 2: the list was one stop");
    assert_eq!(tab(&mut host, false), None, "past the end the focus leaves");
    assert_eq!(tab(&mut host, true), Some(d), "Shift+Tab from nothing starts at the end");
    assert_eq!(tab(&mut host, true), Some(first));
    assert_eq!(tab(&mut host, true), Some(b));
}

#[test]
fn arrows_walk_the_group_tab_reenters_at_the_last_item() {
    let mut host = scene();
    let (second, third, d) = (host.ui.state.items[1].id(), host.ui.state.items[2].id(), host.ui.state.d.id());
    tab(&mut host, false);
    tab(&mut host, false);
    tab(&mut host, false);
    assert!(host.press_key("ArrowDown"));
    assert_eq!(focused(&host), Some(second));
    assert!(host.press_key("End"));
    assert_eq!(focused(&host), Some(third));
    assert!(host.press_key("ArrowDown"), "the key still belongs to the group at its end");
    assert_eq!(focused(&host), Some(third), "no wrap past the last item");
    assert_eq!(tab(&mut host, false), Some(d));
    assert_eq!(tab(&mut host, true), Some(third), "back into the list at the item it was on");
}

#[test]
fn enter_and_space_activate_escape_leaves_keys_without_focus_are_not_used() {
    let mut host = scene();
    assert!(!host.press_key("Enter"), "nothing focused: Enter is not used");
    tab(&mut host, false);
    assert!(host.press_key("Enter"));
    tab(&mut host, false);
    assert!(host.press_key("Space"));
    assert_eq!(host.ui.state.taps, ["A", "B"]);
    assert!(host.press_key("Escape"));
    assert_eq!(focused(&host), None);
    assert!(!host.press_key("Escape"));
}

/// C# KeyboardAccessibilityTests.Editor_IsATabStop_TakesTheCaret_AndKeepsItsKeys, with keys: Tab
/// in gives the caret, Up / Down stay with the text inside a list, Tab out takes the caret away and
/// the next node keeps the keyboard; after a click into the editor, Tab goes on from it.
#[test]
fn an_editor_takes_the_caret_on_tab_keeps_its_keys_and_lets_go_on_tab() {
    #[derive(Default)]
    struct Form {
        editor: Handle<SkiaEditor>,
        next: Handle<SkiaButton>,
        taps: u32,
    }
    let ui = Ui::new(Form::default(), |app: &mut Form| {
        SkiaLayout::column().padding(20).children((
            SkiaLayout::column().accessibility_role(Aria::LIST).children((
                SkiaEditor::new().width_request(200).height_request(40).assign(&mut app.editor),
                SkiaButton::new("item").width_request(100).height_request(40).accessibility_role(Aria::BUTTON),
            )),
            SkiaButton::new("next").width_request(100).height_request(40).accessibility_role(Aria::BUTTON).assign(&mut app.next).on_tapped(|_me, app: &mut Form, _cx| app.taps += 1),
        ))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let (editor, next) = (host.ui.state.editor.id(), host.ui.state.next.id());

    assert!(host.key_down("Tab", Modifiers::default()));
    assert_eq!((host.ui.accessibility_focused(), host.ui.focused()), (Some(editor), Some(editor)), "Tab in: the caret");
    assert!(host.press_key("ArrowDown"), "the text keeps Down");
    assert_eq!(host.ui.accessibility_focused(), Some(editor), "the list did not move");
    assert!(host.key_down("Tab", Modifiers::default()));
    assert_eq!((host.ui.accessibility_focused(), host.ui.focused()), (Some(next), None), "Tab out: the caret goes");
    assert!(host.press_key("Enter"));
    assert_eq!(host.ui.state.taps, 1, "Enter presses the node the keyboard is on");

    assert!(host.press_key("Escape"));
    // A click without `settle`: a focused editor's caret blinks for ever.
    let r = host.rect(editor);
    host.ui.pointer(PointerKind::Down, r.center_x(), r.center_y(), host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, r.center_x(), r.center_y(), host.time_ms());
    host.frame_after(16.0);
    assert_eq!(host.ui.focused(), Some(editor), "a click gives the caret");
    assert!(host.key_down("Tab", Modifiers::default()));
    assert_eq!((host.ui.accessibility_focused(), host.ui.focused()), (Some(next), None), "Tab goes on from the clicked editor");
}

#[test]
fn the_focus_ring_shows_after_the_keyboard_and_a_press_hides_it() {
    let ring = Color::from_rgb(0x6E, 0xA8, 0xFE);
    let mut host = scene();
    let a = host.ui.state.a.id();
    let r = host.rect(a);
    // The ring's stroke: 2 points, 2 points outside the button, on its left edge.
    let at = |host: &mut Headless<App>| host.pixel((r.left - 3.0) as i32, r.center_y() as i32);
    assert_ne!(at(&mut host), ring, "no ring before the keyboard");
    tab(&mut host, false);
    assert_eq!(at(&mut host), ring, "the ring around the focused button");
    host.tap(390.0, 490.0);
    assert_ne!(at(&mut host), ring, "a pointer press hides it");
}

/// The browser's overlay keeps Tab, Enter and Escape and draws its own ring around its element;
/// the engine moves the arrows in a group (React overlay: SkiaAccessibilityManager.Key) and tells
/// each node its group, for the overlay's roving tabindex.
#[test]
fn an_overlay_host_gets_the_groups_from_the_engine_and_no_canvas_ring() {
    let ring = Color::from_rgb(0x6E, 0xA8, 0xFE);
    let mut host = scene();
    host.ui.set_keyboard_navigation(false);
    let (a, first, second) = (host.ui.state.a.id(), host.ui.state.items[0].id(), host.ui.state.items[1].id());
    assert!(!host.key_down("Tab", Modifiers::default()), "Tab is the browser's");
    // The overlay element of the first item got the page focus.
    host.ui.accessibility_focus(id_of(&host, first), true);
    assert!(host.press_key("ArrowDown"));
    assert_eq!(focused(&host), Some(second));
    let r = host.rect(second);
    assert_ne!(host.pixel((r.left - 3.0) as i32, r.center_y() as i32), ring, "the browser draws the ring");
    let node = |id: ControlId| host.ui.accessibility_nodes().iter().find(|n| n.control == id).cloned().unwrap();
    let list = node(first).group.map(|(g, _)| g).expect("the item is in the list");
    assert_eq!(node(second).group, Some((list, id_of(&host, second))));
    assert_eq!(node(a).group, None);
    host.ui.accessibility_focus(id_of(&host, a), true);
    assert!(!host.press_key("Enter"), "Enter is the overlay element's");
    assert!(!host.press_key("Escape"));
}

/// A text field takes the caret when the overlay's focus comes onto it and gives it up when the
/// focus goes on, also after a click into it (C# SkiaEditor.OnAccessibilityFocused).
#[test]
fn the_overlay_focus_gives_a_text_field_the_caret_and_takes_it_back() {
    #[derive(Default)]
    struct Form {
        editor: Handle<SkiaEditor>,
        next: Handle<SkiaButton>,
    }
    let ui = Ui::new(Form::default(), |app: &mut Form| {
        SkiaLayout::column().padding(20).children((
            SkiaEditor::new().width_request(200).height_request(40).assign(&mut app.editor),
            SkiaButton::new("next").width_request(100).height_request(40).accessibility_role(Aria::BUTTON).assign(&mut app.next).on_tapped(|_me, _app: &mut Form, _cx| {}),
        ))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let (editor, next) = (host.ui.state.editor.id(), host.ui.state.next.id());
    host.ui.accessibility_focus(id_of(&host, editor), true);
    assert_eq!(host.ui.focused(), Some(editor), "onto the field: the caret");
    host.ui.accessibility_focus(id_of(&host, editor), false);
    assert_eq!(host.ui.focused(), None, "off the field: no caret");
    host.ui.focus(Some(editor));
    host.ui.accessibility_focus(id_of(&host, next), true);
    assert_eq!(host.ui.focused(), None, "a clicked field gives the caret up when the focus goes on");
}

/// The page's focus went to a field outside the canvas (C# 845b26e9, GitHub #231: keys belong to
/// the field that has them): the drawn editor gives up the caret, and what is typed afterwards
/// never reaches it.
#[test]
fn a_field_outside_the_canvas_takes_the_keyboard_from_the_drawn_editor() {
    #[derive(Default)]
    struct Form {
        editor: Handle<SkiaEditor>,
    }
    let ui = Ui::new(Form::default(), |app: &mut Form| SkiaLayout::column().padding(20).children(SkiaEditor::new().width_request(200).height_request(40).assign(&mut app.editor)));
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let editor = host.ui.state.editor.id();
    host.ui.focus(Some(editor));
    host.type_text("ab");
    assert_eq!(host.ui.tree.find::<SkiaEditor>(editor).unwrap().p.text, "ab");
    drawnui::App::focus_out(&mut host.ui);
    assert_eq!(host.ui.focused(), None);
    assert_eq!(host.ui.text_input(), None, "the host closes its text input");
    host.type_text("cd");
    host.press_key("Backspace");
    assert_eq!(host.ui.tree.find::<SkiaEditor>(editor).unwrap().p.text, "ab", "the page field's keys never reach the editor");
}
