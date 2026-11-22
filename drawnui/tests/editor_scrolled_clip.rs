//! A multiline editor scrolled to its caret keeps its text inside its padding: the line cut by
//! the scroll is clipped at the padded box, not drawn over the padding and the border (as DrawnUI
//! .NET).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
}

/// A long link that wraps by character into about 12 lines.
const LINK: &str = "vless://3f1c9a2e-7b44-4d1e-9a6b-0c2f5e8d1a7b@example.net:443?encryption=none&security=reality&sni=www.example.com&fp=chrome&pbk=Zx8Qe3nB5rT1yU7iO9pA2sD4fG6hJ8kL0zX2cV4bN6m&sid=6ba85179e30d4fc2&type=tcp&flow=xtls-rprx-vision#node";

/// An editor 2 lines high, padding 12 x 10, a border and round corners, holding `text`.
fn editor(text: &'static str) -> Headless<App> {
    let build = move |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaEditor::new()
            .width_request(200)
            .max_lines(2)
            .auto_height(true)
            .padding((12, 10))
            .corner_radius(10)
            .stroke_color(Color::from_rgb(255, 0, 0))
            .stroke_width(1)
            .background_color(Color::WHITE)
            .text_color(Color::BLACK)
            .font_size(16)
            .text(text)
            .assign(&mut app.editor),))
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::WHITE);
    let mut host = Headless::new(ui, 600, 400, 2.0);
    host.settle();
    host
}

fn frames(host: &mut Headless<App>) {
    for _ in 0..4 {
        host.frame_after(16.0);
    }
}

fn scroll_y(host: &Headless<App>) -> f32 {
    -host.ui.tree.base(host.ui.state.editor).unwrap().content_offset.y
}

#[test]
fn scrolled_text_stays_inside_the_padding() {
    let mut host = editor("");
    let link = LINK;
    let editor = host.ui.state.editor;
    host.ui.focus(Some(editor.id()));
    // Pasted: the caret ends after the text and the editor scrolls down to it.
    host.ui.tree.cx().get_mut(editor).unwrap().insert_at_cursor(link);
    frames(&mut host);
    assert!(scroll_y(&host) > 0.0, "scrolled to the caret");
    let rect = host.rect(editor);
    // The top padding (10 pt = 20 px), below the 1 pt border and inside the side padding: only
    // the background, no glyph pixel.
    let dark = |c: Color| c.r() < 160 && c.g() < 160 && c.b() < 160;
    for y in (rect.top as i32 + 3)..(rect.top as i32 + 19) {
        for x in (rect.left as i32 + 24)..(rect.right as i32 - 24) {
            let c = host.pixel(x, y);
            assert!(!dark(c), "text drawn over the top padding at ({x}, {y}): {c:?}");
        }
    }
    // The text itself shows inside the padded box.
    let inside = ((rect.top as i32 + 20)..(rect.bottom as i32 - 20))
        .any(|y| ((rect.left as i32 + 24)..(rect.right as i32 - 24)).any(|x| dark(host.pixel(x, y))));
    assert!(inside, "the text shows inside the padding");
}

/// The caret moved from code comes into view, as C# MoveCursorTo: to the end scrolls down, back
/// to the start scrolls up.
#[test]
fn a_caret_moved_from_code_comes_into_view() {
    let mut host = editor(LINK);
    let editor = host.ui.state.editor;
    host.ui.focus(Some(editor.id()));
    frames(&mut host);
    assert_eq!(scroll_y(&host), 0.0);
    host.ui.tree.cx().get_mut(editor).unwrap().set_cursor_position(LINK.chars().count());
    frames(&mut host);
    let bottom = scroll_y(&host);
    assert!(bottom > 300.0, "{bottom}");
    host.ui.tree.cx().get_mut(editor).unwrap().set_cursor_position(0);
    frames(&mut host);
    assert_eq!(scroll_y(&host), 0.0);
}
