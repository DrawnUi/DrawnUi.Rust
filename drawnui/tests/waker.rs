//! `Waker`: another thread marks a control dirty (a camera frame arrived); the next frame applies
//! the mark and paints the new data, and no frame is asked for at rest.

use std::sync::{Arc, Mutex};

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Paints the color another thread left in its slot (a camera preview in miniature).
struct Preview {
    slot: Arc<Mutex<Color>>,
}

impl Control for Preview {
    fn paint(&self, cx: &mut PaintCx) {
        let mut paint = drawnui::skia::Paint::default();
        paint.set_color(*self.slot.lock().unwrap());
        cx.canvas.draw_rect(cx.rect, &paint);
    }
}

#[derive(Default)]
struct App {
    preview: Handle<Preview>,
}

fn host(slot: Arc<Mutex<Color>>) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app| Build::new(Preview { slot }).fill().assign(&mut app.preview)).background(Color::BLACK);
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host
}

#[test]
fn a_mark_from_another_thread_asks_for_a_frame_that_paints_the_new_data() {
    let slot = Arc::new(Mutex::new(Color::RED));
    let mut h = host(slot.clone());
    assert_eq!(h.pixel(50, 50), Color::RED);
    assert!(!h.ui.needs_frame(), "at rest");

    let (waker, id) = (h.ui.waker(), h.ui.state.preview);
    std::thread::spawn(move || {
        *slot.lock().unwrap() = Color::GREEN;
        waker.invalidate(id, Dirty::DRAW);
    })
    .join()
    .unwrap();

    assert!(h.ui.needs_frame(), "the mark asks for a frame");
    h.frame();
    assert_eq!(h.pixel(50, 50), Color::GREEN);
    assert!(!h.ui.needs_frame(), "at rest again");
}

#[test]
fn a_mark_for_a_removed_control_is_skipped() {
    let slot = Arc::new(Mutex::new(Color::RED));
    let mut h = host(slot);
    let (waker, id) = (h.ui.waker(), h.ui.state.preview);
    h.ui.tree.remove(id);
    h.settle();
    waker.invalidate(id, Dirty::DRAW);
    h.frame();
    assert!(!h.ui.needs_frame());
}
