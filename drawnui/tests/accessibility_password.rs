//! A password field never gives its text to screen readers: its name is its placeholder, and no
//! node of the snapshot (the field, its own label, the web overlay's source) holds the password
//! (DrawnUi.Net: SkiaEditor's default label was its Text, also for passwords).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[test]
fn a_password_never_reaches_the_accessibility_tree() {
    let ui = Ui::new((), |_: &mut ()| {
        SkiaLayout::column().fill().children((SkiaEditor::new().width_request(300).font_size(16).placeholder_text("Password").is_password(true).text("hunter2"),))
    })
    .font_bytes("FontText", FONT)
    .default_accessibility_role::<SkiaLabel>(Aria::TEXT);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host.frame_after(1100.0);
    let nodes = host.ui.accessibility_nodes();
    let field = nodes.iter().find(|n| n.role == Aria::TEXT_BOX).expect("the field");
    assert_eq!(field.label, "Password", "named by its placeholder");
    assert!(field.is_password, "marked as a password for the screen readers");
    let leaks: Vec<String> = nodes
        .iter()
        .filter(|n| n.label.contains("hunter") || n.hint.contains("hunter") || n.text_lines.iter().any(|l| l.text.contains("hunter")))
        .map(|n| format!("{} {:?}", n.role, n.label))
        .collect();
    assert!(leaks.is_empty(), "the password leaked: {leaks:?}");
}
