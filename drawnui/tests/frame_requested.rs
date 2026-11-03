//! A host that draws on demand hears when another thread changed something
//! (`Host::on_frame_requested`, DrawnUI .NET `DrawnView.FrameRequested`): once until the next frame
//! takes the changes, on the thread that made the change.

use std::sync::{Arc, Mutex};

use drawnui::App as _;
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
}

#[test]
fn a_burst_from_another_thread_asks_once_until_a_frame_takes_it() {
    let ui = Ui::new(App::default(), |app: &mut App| SkiaLayout::new().fill().children((SkiaLabel::new("").assign(&mut app.label),)));
    let mut host = Headless::new(ui, 200, 100, 1.0);
    // The threads the requests came on.
    let asked: Arc<Mutex<Vec<std::thread::ThreadId>>> = Arc::default();
    let log = asked.clone();
    let mut requests = drawnui::Host::default();
    requests.on_frame_requested(move || log.lock().unwrap().push(std::thread::current().id()));
    host.ui.init(&mut requests);
    host.settle();
    let (waker, label) = (host.ui.waker(), host.ui.state.label);

    let burst = |waker: Waker| {
        std::thread::spawn(move || {
            for _ in 0..5 {
                waker.invalidate(label, Dirty::DRAW);
            }
            std::thread::current().id()
        })
        .join()
        .unwrap()
    };
    let first = burst(waker.clone());
    assert_eq!(*asked.lock().unwrap(), [first], "once, on the thread that made the change");
    // Not again until a frame took the changes.
    burst(waker.clone());
    assert_eq!(asked.lock().unwrap().len(), 1);
    host.frame_after(16.0);
    let second = burst(waker);
    assert_eq!(*asked.lock().unwrap(), [first, second]);
}
