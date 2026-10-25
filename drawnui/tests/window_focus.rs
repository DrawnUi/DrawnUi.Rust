//! The app's window getting or losing the keyboard (`Ui::on_window_focus`, `Cx::window_focused`),
//! as a host reports it (`App::window_focus`): once per change, before the input that follows.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{App as _, PointerKind};

#[derive(Default)]
struct App {
    log: Vec<String>,
}

fn row() -> Headless<App> {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::new().fill().children(
            SkiaShape::new()
                .width_request(200)
                .height_request(60)
                .on_down(|_me, app: &mut App, cx, _at| app.log.push(format!("down, focused {}", cx.window_focused())))
                .on_tapped(|_me, app: &mut App, _cx| app.log.push("tapped".into())),
        )
    })
    .on_window_focus(|app: &mut App, focused, cx| app.log.push(format!("focus {focused}, read back {}", cx.window_focused())));
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    host
}

#[test]
fn the_focus_callback_runs_before_the_click_that_activated_the_window() {
    let mut host = row();
    assert!(host.ui.tree.cx().window_focused(), "true until a host says");
    // A desktop host: Focused(true), then the activating click's Down in the same events.
    host.ui.window_focus(false);
    host.ui.window_focus(true);
    host.ui.pointer(PointerKind::Down, 50.0, 30.0, host.time_ms());
    host.frame();
    host.ui.pointer(PointerKind::Up, 50.0, 30.0, host.time_ms());
    host.settle();
    assert_eq!(host.ui.state.log, ["focus false, read back false", "focus true, read back true", "down, focused true", "tapped"]);
}

#[test]
fn the_same_value_twice_is_reported_once() {
    let mut host = row();
    host.ui.window_focus(true);
    host.ui.window_focus(false);
    host.ui.window_focus(false);
    assert_eq!(host.ui.state.log, ["focus true, read back true", "focus false, read back false"]);
}
