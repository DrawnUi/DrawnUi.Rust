//! Two fingers (AppoMobi.Gestures ManipulationInfo, MultitouchTracker): while a second finger is
//! down, Panning carries `Gesture::manipulation` (pinch `scale`, turn `rotation`, their totals,
//! the center) and the press makes no tap. One finger stays as it was.

use std::sync::{Arc, Mutex};

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{Manipulation, PointerKind};

/// Takes every gesture and keeps it.
struct Zoom {
    log: Arc<Mutex<Vec<Gesture>>>,
}

impl Control for Zoom {
    fn on_gesture(&mut self, _cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        self.log.lock().unwrap().push(*gesture);
        Handled::Yes
    }
}

fn host() -> (Headless<()>, Arc<Mutex<Vec<Gesture>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let ui = Ui::new((), move |_| Build::new(Zoom { log: seen }).fill());
    let mut h = Headless::new(ui, 400, 400, 1.0);
    h.settle();
    (h, log)
}

fn manipulations(log: &Arc<Mutex<Vec<Gesture>>>) -> Vec<Manipulation> {
    log.lock().unwrap().iter().filter_map(|g| g.manipulation).collect()
}

fn kinds(log: &Arc<Mutex<Vec<Gesture>>>) -> Vec<GestureKind> {
    log.lock().unwrap().iter().map(|g| g.kind).collect()
}

#[test]
fn fingers_moving_apart_report_a_pinch_that_multiplies_to_the_spread_ratio() {
    let (mut h, log) = host();
    h.pinch((200.0, 200.0), 100.0, 200.0, 100.0, 10);
    let m = manipulations(&log);
    assert!(!m.is_empty());
    let zoom: f32 = m.iter().map(|m| 1.0 + m.scale).product();
    assert!((zoom - 2.0).abs() < 1e-3, "zoom {zoom}");
    let last = m.last().unwrap();
    assert_eq!(last.touches_count, 2);
    assert_eq!(last.center, Point::new(200.0, 200.0));
    assert!(last.rotation_total.abs() < 1e-3);
    assert!((last.scale_total - m.iter().map(|m| m.scale).sum::<f32>()).abs() < 1e-5);
    let kinds = kinds(&log);
    assert_eq!(kinds.first(), Some(&GestureKind::Down));
    assert_eq!(kinds.last(), Some(&GestureKind::Up));
    assert!(!kinds.contains(&GestureKind::Tapped), "a pinch is no tap");
    assert!(log.lock().unwrap().iter().filter(|g| g.manipulation.is_some()).all(|g| g.kind == GestureKind::Panning && g.touches == 2));
}

#[test]
fn fingers_moving_together_zoom_out() {
    let (mut h, log) = host();
    h.pinch((200.0, 200.0), 200.0, 100.0, 100.0, 10);
    let zoom: f32 = manipulations(&log).iter().map(|m| 1.0 + m.scale).product();
    assert!((zoom - 0.5).abs() < 1e-3, "zoom {zoom}");
}

#[test]
fn fingers_turning_report_the_angle() {
    let (mut h, log) = host();
    let (cx, cy, r) = (200.0f32, 200.0f32, 50.0f32);
    let at = |deg: f32, finger: f32| {
        let a = (deg + finger * 180.0).to_radians();
        (cx + r * a.cos(), cy + r * a.sin())
    };
    let t = h.time_ms();
    let (x0, y0) = at(0.0, 0.0);
    let (x1, y1) = at(0.0, 1.0);
    h.ui.pointer_touch(PointerKind::Down, x0, y0, t);
    h.ui.pointer_finger(7, PointerKind::Down, x1, y1, t);
    h.frame_after(10.0);
    for step in 1..=9 {
        let deg = step as f32 * 10.0;
        let ((x0, y0), (x1, y1), t) = (at(deg, 0.0), at(deg, 1.0), h.time_ms());
        h.ui.pointer_touch(PointerKind::Move, x0, y0, t);
        h.ui.pointer_finger(7, PointerKind::Move, x1, y1, t);
        h.frame_after(10.0);
    }
    let t = h.time_ms();
    h.ui.pointer_finger(7, PointerKind::Up, x1, y1, t);
    h.ui.pointer_touch(PointerKind::Up, x0, y0, t);
    h.frame_after(10.0);
    let last = *manipulations(&log).last().unwrap();
    assert!((last.rotation_total - 90.0).abs() < 0.01, "turned {}", last.rotation_total);
    assert!(last.scale_total.abs() < 1e-3);
}

#[test]
fn one_finger_has_no_manipulation_and_still_taps() {
    let (mut h, log) = host();
    h.use_touch(true);
    h.pan((100.0, 100.0), (200.0, 100.0), 100.0, 10);
    h.tap(50.0, 50.0);
    let log = log.lock().unwrap();
    assert!(log.iter().all(|g| g.manipulation.is_none()));
    assert!(log.iter().filter(|g| g.kind == GestureKind::Panning).all(|g| g.touches == 1));
    assert!(log.iter().any(|g| g.kind == GestureKind::Tapped));
}

#[test]
fn a_finger_without_a_press_does_nothing() {
    let (mut h, log) = host();
    let t = h.time_ms();
    h.ui.pointer_finger(3, PointerKind::Down, 10.0, 10.0, t);
    h.ui.pointer_finger(3, PointerKind::Move, 60.0, 10.0, t);
    h.ui.pointer_finger(3, PointerKind::Up, 60.0, 10.0, t);
    h.settle();
    assert!(log.lock().unwrap().is_empty());
}
