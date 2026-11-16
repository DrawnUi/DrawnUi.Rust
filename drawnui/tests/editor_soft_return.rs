//! A phone's soft keyboard return key (done / go / search / send) in a single-line field ends
//! editing, as on MAUI: the text is submitted, the field gives up the focus and the keyboard
//! closes. A physical keyboard's Enter submits and keeps the focus; in a multiline field the soft
//! return does what Enter does there (drawnui-cross 6ci, decided 2026-11-16).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
    submitted: Vec<String>,
}

type Host = Headless<App>;

fn scene(lines: i32, return_type: ReturnType) -> Host {
    let build = move |app: &mut App| {
        SkiaLayout::new().fill().children(
            SkiaEditor::new()
                .width_request(300)
                .max_lines(lines)
                .font_size(18)
                .text("Ann")
                .return_type(return_type)
                .assign(&mut app.editor)
                .on_text_submitted(|_me, app: &mut App, _cx, text| app.submitted.push(text.to_owned())),
        )
    };
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 800, 600, 2.0);
    host.settle();
    let id = host.ui.state.editor.id();
    host.ui.focus(Some(id));
    host.frame_after(16.0);
    host
}

fn focused(host: &Host) -> bool {
    host.ui.tree.find(host.ui.state.editor).is_some_and(|e: &SkiaEditor| e.is_focused())
}

fn text(host: &Host) -> String {
    host.ui.tree.find(host.ui.state.editor).map(|e: &SkiaEditor| e.p.text.clone()).unwrap_or_default()
}

fn enter(host: &mut Host) {
    for kind in [KeyKind::Down, KeyKind::Up] {
        host.ui.key(kind, "Enter", "", Modifiers::default(), false);
    }
    host.frame_after(16.0);
}

#[test]
fn the_soft_return_ends_editing_a_single_line_field() {
    for return_type in [ReturnType::Done, ReturnType::Go, ReturnType::Search, ReturnType::Send] {
        let mut host = scene(1, return_type);
        assert!(host.ui.soft_return());
        host.frame_after(16.0);
        assert!(!focused(&host), "{return_type:?}: the field gives up the focus");
        assert_eq!(host.ui.state.submitted, ["Ann"], "{return_type:?}: the text is submitted once");
        assert!(host.ui.text_input().is_none(), "{return_type:?}: the keyboard closes");
    }
}

#[test]
fn a_physical_enter_submits_and_keeps_the_focus() {
    let mut host = scene(1, ReturnType::Done);
    enter(&mut host);
    assert!(focused(&host));
    assert_eq!(host.ui.state.submitted, ["Ann"]);
}

#[test]
fn in_a_multiline_field_the_soft_return_is_enter() {
    let mut host = scene(4, ReturnType::Default);
    host.ui.soft_return();
    host.frame_after(16.0);
    assert!(focused(&host), "a multiline field keeps editing");
    assert_eq!(text(&host), "Ann\n", "and breaks the line");
    let mut host = scene(4, ReturnType::Send);
    host.ui.soft_return();
    host.frame_after(16.0);
    assert!(focused(&host), "Send in a multiline field submits and keeps the focus, as Enter");
    assert_eq!(host.ui.state.submitted, ["Ann"]);
}
