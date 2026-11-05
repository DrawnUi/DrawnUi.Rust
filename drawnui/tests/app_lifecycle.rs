//! The app's closing step on the desktop (`Ui::on_app_closing`, `Cx::close_app`): an app keeps its
//! window open to ask "save first?", then ends itself.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct Editor {
    unsaved: bool,
    asked: u32,
}

fn host(handler: bool) -> Headless<Editor> {
    let mut ui = Ui::new(Editor { unsaved: true, ..Editor::default() }, |_| SkiaLayout::new().fill());
    if handler {
        ui = ui.on_app_closing(|app: &mut Editor, _cx| {
            app.asked += 1;
            !app.unsaved
        });
    }
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

#[test]
fn without_a_handler_the_window_closes_at_once() {
    let mut host = host(false);
    assert!(host.request_close());
    assert!(!host.take_close_app());
}

#[test]
fn an_app_keeps_its_window_open_then_ends_itself() {
    let mut host = host(true);
    // Unsaved work: the window stays, the app asks "save first?".
    assert!(!host.request_close());
    assert_eq!(host.ui.state.asked, 1);
    assert!(!host.take_close_app());
    // Saved: the app ends itself, without being asked again.
    host.ui.state.unsaved = false;
    host.ui.tree.cx().close_app();
    host.frame_after(16.0);
    assert!(host.take_close_app());
    assert_eq!(host.ui.state.asked, 1);
    // Saved work closes at the next request.
    assert!(host.request_close());
}
