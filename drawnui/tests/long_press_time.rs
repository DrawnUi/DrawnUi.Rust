//! The long press time is an app setting (C# TouchEffect.LongPressTimeMsDefault, drawnui-cross
//! 6cf): 750 ms by default, read when a press starts.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{PointerKind, TouchEffect};

#[derive(Default)]
struct App {
    long_presses: u32,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |_| {
        SkiaShape::new().fill().background_color(Color::RED).on_long_pressing(|_me, app: &mut App, _cx| app.long_presses += 1)
    });
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    host
}

/// Presses a finger for `ms` and lifts it: whether a LongPressing came.
fn held(host: &mut Headless<App>, ms: f64) -> bool {
    let before = host.ui.state.long_presses;
    host.ui.pointer_touch(PointerKind::Down, 100.0, 100.0, host.time_ms());
    host.frame();
    host.frame_after(ms);
    host.ui.pointer_touch(PointerKind::Up, 100.0, 100.0, host.time_ms());
    host.frame_after(16.0);
    host.frame_after(500.0);
    host.ui.state.long_presses > before
}

// One test: the setting is one value for the process.
#[test]
fn a_long_press_takes_750_ms_by_default_and_the_app_can_change_it() {
    assert_eq!(TouchEffect::long_press_time_ms_default(), 750.0);
    let mut host = host();
    assert!(held(&mut host, 800.0), "0.8 s is a long press");
    assert!(!held(&mut host, 700.0), "0.7 s is not");
    TouchEffect::set_long_press_time_ms_default(1200.0);
    assert!(!held(&mut host, 800.0), "0.8 s is not with 1200 ms set");
    assert!(held(&mut host, 1300.0), "1.3 s is");
    TouchEffect::set_long_press_time_ms_default(750.0);
}
