//! A control that listens hears the device turn to another quarter (`Control::on_device_rotation`,
//! C# `Super.DeviceRotationSnap` / `Super.OrientationChanged`); `Cx::device_rotation` reads the
//! value. The host reads the motion sensor only while a listener is mounted; after a turn the UI
//! is at rest again unless a listener changed something.

use std::sync::{Arc, Mutex};

use drawnui::App as _;
use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Writes what it heard into a shared log.
struct Camera {
    log: Arc<Mutex<Vec<i32>>>,
}

impl Control for Camera {
    fn on_device_rotation(&mut self, cx: &mut GestureCx, degrees: i32) {
        assert_eq!(cx.cx().device_rotation(), degrees, "the value is stored before listeners hear it");
        self.log.lock().unwrap().push(degrees);
    }
}

#[derive(Default)]
struct App {
    camera: Handle<Camera>,
}

fn host(listen: bool) -> (Headless<App>, Arc<Mutex<Vec<i32>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let ui = Ui::new(App::default(), move |app| {
        let camera = Build::new(Camera { log: seen.clone() }).fill().assign(&mut app.camera);
        if listen { camera.listen_device_rotation() } else { camera }
    });
    let mut h = Headless::new(ui, 100, 100, 1.0);
    h.settle();
    (h, log)
}

#[test]
fn a_listener_hears_each_new_quarter_once() {
    let (mut h, log) = host(true);
    assert_eq!(h.ui.tree.cx().device_rotation(), 0);
    for degrees in [90, 90, 270, 0, 0, 180] {
        h.ui.device_rotation(degrees);
    }
    assert_eq!(*log.lock().unwrap(), [90, 270, 0, 180]);
    assert_eq!(h.ui.tree.cx().device_rotation(), 180);
}

#[test]
fn the_sensor_is_wanted_only_while_a_listener_is_mounted() {
    let (h, _) = host(false);
    assert!(!h.ui.wants_device_rotation());

    let (mut h, log) = host(true);
    assert!(h.ui.wants_device_rotation());
    let camera = h.ui.state.camera;
    h.ui.tree.cx().listen_device_rotation(camera, false);
    assert!(!h.ui.wants_device_rotation());
    h.ui.tree.cx().listen_device_rotation(camera, true);
    assert!(h.ui.wants_device_rotation());
    h.ui.tree.remove(camera);
    h.settle();
    assert!(!h.ui.wants_device_rotation());
    h.ui.device_rotation(90);
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn a_turn_leaves_the_ui_at_rest() {
    let (mut h, log) = host(true);
    assert!(!h.ui.needs_frame());
    h.ui.device_rotation(90);
    assert_eq!(*log.lock().unwrap(), [90]);
    assert!(!h.ui.needs_frame());
}
