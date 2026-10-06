//! `Cx::after`: a timer draws no frames while it waits and runs once when its time comes.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    fired: u32,
    shape: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().children(
        SkiaShape::new().width_request(100).height_request(50).background_color(Color::GREEN).assign(&mut app.shape).on_tapped(
            |me, _app: &mut App, cx| {
                cx.after(me.id(), 1000, |app: &mut App, _cx| app.fired += 1);
            },
        ),
    )
}

#[test]
fn a_timer_waits_without_frames_and_runs_once() {
    let mut host = Headless::new(Ui::new(App::default(), build), 200, 100, 1.0);
    host.settle();
    host.frame_after(500.0);

    // By hand: `tap` settles, and settling jumps to the timer.
    host.ui.pointer(PointerKind::Down, 20.0, 20.0, host.time_ms());
    host.frame();
    host.ui.pointer(PointerKind::Up, 20.0, 20.0, host.time_ms());
    host.frame();
    let started = host.time_ms();
    host.frame_after(16.0);

    // Nothing asks for a frame; the host is told when to wake.
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(started + 1000.0));
    assert_eq!(host.ui.state.fired, 0);

    // A frame before its time (a resize, say) does not run it.
    host.frame_after(500.0);
    assert_eq!(host.ui.state.fired, 0);
    assert!(!host.ui.needs_frame());

    host.settle();
    assert_eq!(host.ui.state.fired, 1);
    assert_eq!(host.time_ms(), started + 1000.0);
    assert_eq!(host.ui.wake_at(), None);
    host.frame_after(5000.0);
    assert_eq!(host.ui.state.fired, 1);
}
