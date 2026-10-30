//! An editor focused from code (a sheet appears and focuses its comment field) shows its caret at
//! once, at the end of its text, as after a tap; typing appends. A caret position the app set
//! before focusing is kept. Ports EditorFocusFromCodeTests (DrawnUi.Net.Tests, C# 2285b422).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

/// A color nothing else on screen has: the caret.
const CARET: Color = Color::from_rgb(255, 0, 255);

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
}

type Host = Headless<App>;

fn scene(text: &'static str) -> Host {
    let build = move |app: &mut App| {
        SkiaLayout::new().fill().children(
            SkiaEditor::new()
                .width_request(300)
                .max_lines(3)
                .font_size(18)
                .text(text)
                .placeholder_text("Write a comment")
                .return_type(ReturnType::Send)
                .cursor_color(CARET)
                .assign(&mut app.editor),
        )
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 800, 600, 2.0);
    host.settle();
    host
}

fn editor(host: &Host) -> &SkiaEditor {
    host.ui.tree.find(host.ui.state.editor).unwrap()
}

fn focus_from_code(host: &mut Host) {
    let id = host.ui.state.editor.id();
    host.ui.focus(Some(id));
    for _ in 0..4 {
        host.frame_after(16.0);
    }
}

/// Pixels of the caret color inside the editor.
fn caret_pixels(host: &mut Host) -> usize {
    let r = host.rect(host.ui.state.editor);
    let mut count = 0;
    for y in (r.top as i32..r.bottom as i32).step_by(1) {
        for x in (r.left as i32..r.right as i32).step_by(1) {
            if host.pixel(x, y) == CARET {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn focused_from_code_caret_at_the_end_and_drawn() {
    for text in ["", "Hello", "first line\nsecond"] {
        let mut host = scene(text);
        assert!(!editor(&host).is_focused());
        assert_eq!(caret_pixels(&mut host), 0, "'{text}': a caret before the focus");
        focus_from_code(&mut host);
        assert!(editor(&host).is_focused(), "'{text}': not focused");
        assert_eq!(editor(&host).cursor_position(), text.chars().count(), "'{text}': the caret is not at the end");
        assert!(caret_pixels(&mut host) > 0, "'{text}': no caret drawn");
        assert_eq!(host.ui.focused(), Some(host.ui.state.editor.id()));
    }
}

#[test]
fn focused_from_code_keeps_a_position_the_app_set() {
    let mut host = scene("Hello");
    host.ui.tree.get_mut(host.ui.state.editor).unwrap().set_cursor_position(2);
    host.frame_after(16.0);
    focus_from_code(&mut host);
    assert_eq!(editor(&host).cursor_position(), 2);
    assert!(caret_pixels(&mut host) > 0, "no caret drawn");
}

#[test]
fn focused_from_code_then_typed_appends_at_the_end() {
    let mut host = scene("Hello");
    focus_from_code(&mut host);
    host.type_text("!");
    host.frame_after(16.0);
    assert_eq!(editor(&host).p.text, "Hello!");
    assert_eq!(editor(&host).cursor_position(), 6);
}

/// A tap still places the caret where it lands, not at the end.
#[test]
fn a_tap_places_the_caret_where_it_lands() {
    let mut host = scene("Hello");
    let r = host.rect(host.ui.state.editor);
    // Left padding 12 points: a tap at the very start of the text.
    let (x, y) = (r.left + 26.0, r.top + 30.0);
    host.ui.pointer(drawnui::PointerKind::Down, x, y, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(drawnui::PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
    assert!(editor(&host).is_focused());
    assert_eq!(editor(&host).cursor_position(), 0);
}
