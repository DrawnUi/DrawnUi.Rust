//! A control that listens hears the app being hidden and shown (`Control::on_app_visibility`);
//! `Cx::postpone_before_draw` runs at the start of the next frame, whatever is shown.

use std::sync::{Arc, Mutex};

use drawnui::App as _;
use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Writes what it heard into a shared log.
struct Camera {
    name: &'static str,
    log: Arc<Mutex<Vec<String>>>,
}

impl Control for Camera {
    fn on_app_visibility(&mut self, _cx: &mut GestureCx, visible: bool) {
        self.log.lock().unwrap().push(format!("{} {visible}", self.name));
    }
}

#[derive(Default)]
struct App {
    listening: Handle<Camera>,
    other: Handle<Camera>,
    captured: u32,
}

fn host(log: &Arc<Mutex<Vec<String>>>) -> Headless<App> {
    let (a, b) = (log.clone(), log.clone());
    let ui = Ui::new(App::default(), move |app| {
        SkiaLayout::column().children((
            Build::new(Camera { name: "listening", log: a }).height_request(10).listen_app_visibility().assign(&mut app.listening),
            Build::new(Camera { name: "other", log: b }).height_request(10).assign(&mut app.other),
        ))
    });
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host
}

#[test]
fn only_a_listening_control_hears_the_app_hidden_and_shown() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut h = host(&log);
    h.ui.visibility(false);
    h.ui.visibility(true);
    assert_eq!(*log.lock().unwrap(), ["listening false", "listening true"]);
}

#[test]
fn listening_can_be_turned_on_and_off_later() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut h = host(&log);
    let (listening, other) = (h.ui.state.listening, h.ui.state.other);
    let mut cx = h.ui.tree.cx();
    cx.listen_app_visibility(listening, false);
    cx.listen_app_visibility(other, true);
    h.ui.visibility(false);
    assert_eq!(*log.lock().unwrap(), ["other false"]);
}

#[test]
fn a_removed_control_hears_nothing() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut h = host(&log);
    let listening = h.ui.state.listening;
    h.ui.tree.remove(listening);
    h.settle();
    h.ui.visibility(false);
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn postponed_work_runs_at_the_next_frame_even_under_a_hidden_control() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut h = host(&log);
    let listening = h.ui.state.listening;
    let mut cx = h.ui.tree.cx();
    cx.get_mut(listening).unwrap().set_is_visible(false);
    cx.postpone_before_draw(|app: &mut App, _cx| app.captured += 1);
    assert_eq!(h.ui.state.captured, 0, "not before the frame");
    assert!(h.ui.needs_frame());
    h.frame();
    assert_eq!(h.ui.state.captured, 1);
    h.settle();
    assert_eq!(h.ui.state.captured, 1, "once");
}
