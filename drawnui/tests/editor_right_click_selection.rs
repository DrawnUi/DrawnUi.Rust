//! A right click on an editor's selection keeps it for the context menu (DrawnUi.Net 70abfbd2):
//! only the main button places the caret or starts a selection.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{ContextMenuSource, MouseButton, PointerKind};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));
const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };

#[test]
fn a_right_click_on_the_selection_keeps_it() {
    let ui = Ui::new(Handle::<SkiaEditor>::default(), |editor: &mut Handle<SkiaEditor>| {
        SkiaLayout::new().fill().children(SkiaEditor::new().width_request(300).font_size(18).text("Hello wonderful world").assign(editor))
    })
    .font_bytes("FontText", FONT)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    // (Not `tap` / `right_click`: they settle, and the caret blinks once focused.)
    let press = |host: &mut Headless<Handle<SkiaEditor>>, button| {
        host.ui.pointer_button(PointerKind::Down, button, 40.0, 12.0, host.time_ms());
        host.frame_after(16.0);
        host.ui.pointer_button(PointerKind::Up, button, 40.0, 12.0, host.time_ms());
        host.frame_after(16.0);
    };
    press(&mut host, MouseButton::Left);
    assert!(host.key_down("KeyA", CTRL), "the editor took Ctrl+A");
    let editor = |host: &Headless<Handle<SkiaEditor>>| host.ui.tree.find::<SkiaEditor>(host.ui.state).unwrap().selected_text().to_owned();
    assert_eq!(editor(&host), "Hello wonderful world");
    press(&mut host, MouseButton::Right);
    host.ui.context_menu(40.0, 12.0, ContextMenuSource::Mouse, host.time_ms());
    host.frame_after(16.0);
    assert_eq!(editor(&host), "Hello wonderful world", "the right button's press kept the selection");
}
