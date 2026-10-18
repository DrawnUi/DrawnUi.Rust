//! `PaintCx::gpu`: no GPU context on the CPU (the headless host) and none inside a picture
//! recorded for a bake worker. Importing textures runs on the desktop GPU only.

use std::sync::{Arc, Mutex};

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Remembers whether `paint` saw a GPU context.
struct Probe {
    seen: Arc<Mutex<Vec<bool>>>,
}

impl Control for Probe {
    fn paint(&self, cx: &mut PaintCx) {
        let has = cx.gpu().is_some();
        self.seen.lock().unwrap().push(has);
    }
}

#[test]
fn the_cpu_host_offers_no_gpu_context() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = seen.clone();
    let ui = Ui::new((), move |_| Build::new(Probe { seen: probe }).fill());
    let mut h = Headless::new(ui, 50, 50, 1.0);
    h.settle();
    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty(), "painted");
    assert!(seen.iter().all(|has| !has));
}
