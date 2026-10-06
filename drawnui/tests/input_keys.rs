//! Keyboard input: the focused control (DrawnUI FocusedChild and its tap rule), controls that
//! listen to every key (React KeyboardManager subscribers), window-level handlers, the held keys
//! and the modifiers, typed text, and the text input the host opens over an editor.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::key_name;
use drawnui::prelude::*;
use drawnui::testing::Headless;

type Log = Rc<RefCell<Vec<String>>>;

/// An editor stand-in: claims the focus on its Down (as SkiaEditor), takes typed text, uses the
/// arrows, lets every other key through.
struct Field {
    layout: SkiaLayout,
    log: Log,
}
impl Container for Field {}
impl Control for Field {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match gesture.kind {
            GestureKind::Down => {
                cx.focus();
                Handled::Yes
            }
            GestureKind::Tapped | GestureKind::Up => Handled::Yes,
            _ => Handled::No,
        }
    }
    fn on_key(&mut self, _cx: &mut GestureCx, event: &KeyEvent) -> bool {
        let entry = match event.kind {
            KeyKind::Char => format!("field char {}", event.text),
            kind => format!("field {kind:?} {}", event.key),
        };
        self.log.borrow_mut().push(entry);
        event.kind == KeyKind::Char || event.key.starts_with("Arrow")
    }
    fn on_focus_changed(&mut self, _cx: &mut GestureCx, focused: bool) {
        self.log.borrow_mut().push(format!("field focus {focused}"));
    }
    fn wants_text_input(&self) -> bool {
        true
    }
}

/// A game: gets every key while mounted, focused or not.
struct Game {
    layout: SkiaLayout,
    log: Log,
}
impl Control for Game {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn on_key(&mut self, _cx: &mut GestureCx, event: &KeyEvent) -> bool {
        self.log.borrow_mut().push(format!("game {:?} {}", event.kind, event.key));
        event.key == "Space"
    }
}

#[derive(Default)]
struct App {
    log: Log,
    field: Handle<Field>,
    card: Handle<SkiaShape>,
    button: Handle<SkiaShape>,
    game: Handle<Game>,
}

/// Field at (0, 0), a focusable card at (0, 100), a plain button at (0, 200), 100 x 50 points each;
/// the game is 1 x 1 at (390, 290).
fn host(scale: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        let log = app.log.clone();
        SkiaLayout::new().fill().children((
            Build::new(Field { layout: SkiaLayout::default(), log: log.clone() })
                .width_request(100)
                .height_request(50)
                .can_be_focused(true)
                .assign(&mut app.field)
                .on_focus_changed(|_me, app: &mut App, _cx, focused| app.log.borrow_mut().push(format!("handler focus {focused}")))
                .on_key_down(|_me, app: &mut App, _cx, event| {
                    app.log.borrow_mut().push(format!("handler down {}", event.key));
                    false
                }),
            SkiaShape::new()
                .margin((0.0, 100.0, 0.0, 0.0))
                .width_request(100)
                .height_request(50)
                .can_be_focused(true)
                .assign(&mut app.card)
                .on_tapped(|_me, app: &mut App, _cx| app.log.borrow_mut().push("card tapped".into())),
            SkiaShape::new()
                .margin((0.0, 200.0, 0.0, 0.0))
                .width_request(100)
                .height_request(50)
                .assign(&mut app.button)
                .on_tapped(|_me, app: &mut App, _cx| app.log.borrow_mut().push("button tapped".into())),
            Build::new(Game { layout: SkiaLayout::default(), log })
                .margin((390.0, 290.0, 0.0, 0.0))
                .width_request(1)
                .height_request(1)
                .listen_keys()
                .assign(&mut app.game),
        ))
    })
    .on_key_down(|app: &mut App, event, _cx| {
        app.log.borrow_mut().push(format!("window down {}", event.key));
        event.key == "Escape"
    })
    .on_key_char(|app: &mut App, event, _cx| {
        app.log.borrow_mut().push(format!("window char {}", event.text));
        false
    });
    let mut host = Headless::new(ui, (400.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    host
}

fn take(host: &Headless<App>) -> Vec<String> {
    std::mem::take(&mut *host.ui.state.log.borrow_mut())
}

#[test]
fn keys_go_to_the_focused_control_then_the_listeners_then_the_window() {
    let mut host = host(1.0);
    // Nothing focused: the game and the window.
    assert!(!host.press_key("KeyA"));
    assert_eq!(take(&host), ["game Down KeyA", "window down KeyA", "game Up KeyA"]);

    host.tap(50.0, 25.0);
    assert_eq!(host.ui.focused(), Some(host.ui.state.field.id()));
    assert_eq!(take(&host), ["field focus true", "handler focus true"]);

    // The control's hook, its handler, the listener, the window; any of them may use the key.
    assert!(host.key_down("ArrowLeft", Modifiers::default()));
    assert_eq!(take(&host), ["field Down ArrowLeft", "handler down ArrowLeft", "game Down ArrowLeft", "window down ArrowLeft"]);
    assert!(host.key_down("Space", Modifiers::default()));
    assert!(host.key_down("Escape", Modifiers::default()));
    assert!(!host.key_down("KeyB", Modifiers::default()));
    take(&host);

    // Typed text, one character at a time.
    host.type_text("hé");
    assert_eq!(take(&host), ["field char h", "game Char ", "window char h", "field char é", "game Char ", "window char é"]);
}

#[test]
fn the_focus_moves_on_a_completed_tap_only_and_never_to_what_cannot_take_it() {
    let mut host = host(1.0);
    let (field, card) = (host.ui.state.field.id(), host.ui.state.card.id());
    // The card takes the focus when its tap completes.
    host.tap(50.0, 125.0);
    assert_eq!(host.ui.focused(), Some(card));
    // The field claims the focus on its own Down, tap or not.
    host.pan((50.0, 25.0), (50.0, 80.0), 64.0, 4);
    assert_eq!(host.ui.focused(), Some(field));
    host.tap(50.0, 125.0);
    assert_eq!(host.ui.focused(), Some(card));
    // A plain button leaves the focus where it is (C# ReportFocus keeps it).
    host.tap(50.0, 225.0);
    assert_eq!(host.ui.focused(), Some(card));
    // A tap over nothing clears it (the outside-tap keyboard dismiss).
    host.tap(300.0, 150.0);
    assert_eq!(host.ui.focused(), None);

    // LockFocus: the focused control keeps it whatever is tapped.
    host.tap(50.0, 25.0);
    host.ui.tree.get_mut(host.ui.state.field).unwrap().set_lock_focus(true);
    host.tap(50.0, 125.0);
    host.tap(300.0, 150.0);
    assert_eq!(host.ui.focused(), Some(field));
    assert!(take(&host).contains(&"card tapped".to_string()));
}

#[test]
fn the_host_text_input_follows_a_focused_editor() {
    let mut host = host(2.0);
    assert_eq!(host.ui.text_input(), None);
    host.tap(100.0, 50.0);
    // Points, where the field is.
    assert_eq!(host.ui.text_input(), Some(Rect::new(0.0, 0.0, 100.0, 50.0)));
    // The card takes no text: the keyboard closes.
    host.tap(100.0, 250.0);
    assert_eq!(host.ui.text_input(), None);
}

#[test]
fn held_keys_and_modifiers_are_tracked_and_blur_forgets_them() {
    let mut host = host(1.0);
    let shift = Modifiers { shift: true, ..Modifiers::default() };
    host.key_down("ShiftLeft", shift);
    host.key_down("ArrowRight", shift);
    assert!(host.ui.keyboard.is_pressed("ArrowRight"));
    assert!(host.ui.keyboard.is_shift_pressed());
    host.key_up("ArrowRight", shift);
    assert!(!host.ui.keyboard.is_pressed("ArrowRight"));
    assert!(host.ui.keyboard.is_pressed("ShiftLeft"));
    // The window lost the keyboard: the release of Shift never comes.
    host.ui.blur();
    assert!(!host.ui.keyboard.is_pressed("ShiftLeft"));
    assert!(!host.ui.keyboard.is_shift_pressed());
    // A name the engine does not know.
    assert_eq!(key_name("NoSuchKey"), "Unknown");
    assert_eq!(key_name("MetaLeft"), "MetaLeft");
}

#[test]
fn a_removed_listener_gets_no_more_keys_and_one_can_be_added_at_runtime() {
    let mut host = host(1.0);
    let game = host.ui.state.game;
    host.ui.tree.remove(game);
    host.settle();
    host.press_key("KeyQ");
    assert_eq!(take(&host), ["window down KeyQ"]);

    // A plain control that listens from now on: its key handler runs without focus.
    let ui = Ui::new(App::default(), |app| {
        SkiaLayout::new().fill().children(
            SkiaShape::new()
                .width_request(10)
                .height_request(10)
                .assign(&mut app.button)
                .on_key_down(|_me, app: &mut App, _cx, event| {
                    app.log.borrow_mut().push(format!("shape down {}", event.key));
                    true
                })
                .on_tapped(|me, _app: &mut App, cx| cx.listen_keys(me.id(), true)),
        )
    });
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    assert!(!host.press_key("KeyW"));
    host.tap(5.0, 5.0);
    assert!(host.press_key("KeyW"));
    assert_eq!(take(&host), ["shape down KeyW"]);
}
