//! Snap on release: a resting finger is not a flick (drawnui-cross 6o, C# 662cb81b). A release speed
//! under 100 points per second counts as none for every snapping control, the drawer included: a
//! 70 point drag held still and released goes back; a flick or a drag past half closes.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    drawer: Handle<SkiaDrawer>,
}

/// An open drawer from the right, 300 wide with a 40 header, dragged right by `moves` (points, ms
/// before each), released: whether it is open after.
fn released_open(touch: bool, moves: &[(f32, f64)]) -> bool {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let drawer = SkiaDrawer::new()
            .direction(DrawerDirection::FromRight)
            .header_size(40)
            .width_request(300)
            .fill_y()
            .horizontal_options(LayoutOptions::End)
            .is_open(true)
            .assign(&mut app.drawer)
            .children(SkiaLayout::new().fill().background_color(Color::GRAY));
        SkiaLayout::new().fill().children(drawer)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 600, 1.0);
    host.use_touch(touch);
    host.settle();
    let (mut x, y) = (200.0f32, 300.0f32);
    host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
    for &(dx, ms) in moves {
        host.frame_after(ms);
        x += dx;
        host.ui.pointer(PointerKind::Move, x, y, host.time_ms());
    }
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
    host.settle();
    host.ui.tree.find::<SkiaDrawer>(host.ui.state.drawer).unwrap().is_open()
}

#[test]
fn a_resting_finger_is_not_a_flick() {
    // 70 points in ~0.5 s, then 19 moves of 0.5 point (a resting fingertip), released.
    let mut resting: Vec<(f32, f64)> = (0..31).map(|_| (70.0 / 31.0, 16.0)).collect();
    resting.extend((0..19).map(|_| (0.5, 16.0)));
    // 60 points in ~64 ms.
    let flick: Vec<(f32, f64)> = (0..4).map(|_| (15.0, 16.0)).collect();
    // 160 points (past half of 260) in ~1 s, held still.
    let mut slow: Vec<(f32, f64)> = (0..62).map(|_| (160.0 / 62.0, 16.0)).collect();
    slow.extend((0..19).map(|_| (0.0, 16.0)));
    for touch in [false, true] {
        assert!(released_open(touch, &resting), "touch {touch}: a resting finger goes back");
        assert!(!released_open(touch, &flick), "touch {touch}: a flick closes");
        assert!(!released_open(touch, &slow), "touch {touch}: past half closes");
    }
}
