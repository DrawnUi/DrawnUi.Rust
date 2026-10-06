//! What the frame gives the app besides the tree: the FPS overlay.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

#[test]
fn the_fps_pill_is_in_the_bottom_left_corner() {
    for scale in [1.0f32, 2.0] {
        let ui = Ui::new((), |_| SkiaLayout::new().fill()).font_bytes("Default", FONT).background(Color::WHITE).show_fps(true);
        let (width, height) = ((300.0 * scale) as i32, (200.0 * scale) as i32);
        let mut host = Headless::new(ui, width, height, scale);
        host.settle();
        // 26 points tall, 8 from the left and the bottom; black at 220 over white.
        let pill = host.pixel((8.0 * scale + 3.0) as i32, height - (21.0 * scale) as i32);
        assert_eq!(pill, Color::from_rgb(35, 35, 35), "scale {scale}");
        assert_eq!(host.pixel((6.0 * scale) as i32, height - (21.0 * scale) as i32), Color::WHITE);
        assert_eq!(host.pixel((8.0 * scale + 3.0) as i32, height - (6.0 * scale) as i32), Color::WHITE);
        // The top right corner is free for the shell's buttons.
        assert_eq!(host.pixel(width - (20.0 * scale) as i32, (20.0 * scale) as i32), Color::WHITE);
    }
}

#[derive(Default)]
struct Taps {
    taps: u32,
}

/// DrawnUI SendTapped with AnimationTapped = Ripple: the ripple starts where the tap landed, in
/// TouchEffectColor, and the handler runs.
#[test]
fn animation_tapped_plays_a_ripple_where_the_tap_landed() {
    let ui = Ui::new(Taps::default(), |_| {
        SkiaLayout::new().fill().children(
            SkiaShape::new()
                .width_request(200)
                .height_request(200)
                .background_color(Color::BLACK)
                .touch_effect_color(Color::WHITE)
                .animation_tapped(SkiaTouchAnimation::Ripple)
                .animation_tapped_speed(300)
                .on_tapped(|_me, app: &mut Taps, _cx| app.taps += 1),
        )
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    host.ui.pointer(drawnui::PointerKind::Down, 50.0, 50.0, host.time_ms());
    host.frame();
    host.ui.pointer(drawnui::PointerKind::Up, 50.0, 50.0, host.time_ms());
    host.frame_after(16.0);
    host.frame_after(100.0);
    assert_eq!(host.ui.state.taps, 1);
    // Lighter at the tap point, still black far from it.
    let at = host.pixel(50, 50);
    assert!(at.r() > 0 && at.r() == at.g(), "{at:?}");
    assert_eq!(host.pixel(195, 195), Color::BLACK);
    // It ends by itself in its speed.
    host.frame_after(300.0);
    host.settle();
    assert_eq!(host.pixel(50, 50), Color::BLACK);
}

#[derive(Default)]
struct Cards {
    columns: u32,
    sizes: Vec<Size>,
    grid: Handle<SkiaLayout>,
}

/// React `useCardWidth`: one column below 640 points. The resize handler runs before the first
/// layout and on every change; observers follow it.
#[test]
fn the_canvas_size_reaches_the_app_and_observers_follow_it() {
    let build = |app: &mut Cards| {
        SkiaLayout::new().fill().assign(&mut app.grid).observe(|me, app: &Cards| me.set_width_request(if app.columns == 1 { 300.0 } else { 600.0 }))
    };
    let ui = Ui::new(Cards::default(), build).on_canvas_resized(|app: &mut Cards, size, _cx| {
        app.columns = if size.width < 640.0 { 1 } else { 2 };
        app.sizes.push(size);
    });
    let mut host = Headless::new(ui, 1400, 800, 2.0);
    host.settle();
    host.frame_after(16.0);
    // 1400 x 800 pixels at scale 2: 700 x 400 points, told once.
    assert_eq!(host.ui.state.sizes, [Size::new(700.0, 400.0)]);
    assert_eq!(host.ui.canvas_size(), Size::new(700.0, 400.0));
    assert_eq!(host.rect(host.ui.state.grid).width(), 1200.0);

    // The same scene in a narrower host: one column.
    let ui = Ui::new(Cards::default(), build).on_canvas_resized(|app: &mut Cards, size, _cx| {
        app.columns = if size.width < 640.0 { 1 } else { 2 };
        app.sizes.push(size);
    });
    let mut host = Headless::new(ui, 1200, 800, 2.0);
    host.settle();
    assert_eq!(host.ui.state.columns, 1);
    assert_eq!(host.rect(host.ui.state.grid).width(), 600.0);
}

/// React Canvas.FrameTime (the last frame's CPU time) and FPS (frames in the last second).
#[test]
fn frame_stats_count_the_frames_of_the_last_second() {
    let ui = Ui::new((), |_| SkiaLayout::new().fill());
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    for _ in 0..70 {
        host.frame_after(16.0);
    }
    // 1000 / 16 = 62.5: the frames at t - 992 .. t.
    let stats = host.ui.frame_stats();
    assert_eq!(stats.fps, 63);
    assert!(stats.frame_time_ms > 0.0 && stats.frame_time_ms < 1000.0);
    // Idle: the last value stays; a frame after a pause counts only itself.
    host.frame_after(5000.0);
    assert_eq!(host.ui.frame_stats().fps, 1);
}

type Moves = std::rc::Rc<std::cell::RefCell<Vec<(u32, String)>>>;

/// A shell stand-in: pushes a history entry per page, hears the browser's back / forward.
struct Pages {
    moves: Moves,
}
impl Control for Pages {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(100.0, 100.0)
    }
    fn on_history(&mut self, _cx: &mut GestureCx, depth: u32, hash: &str) {
        self.moves.borrow_mut().push((depth, hash.to_owned()));
    }
}

#[derive(Default)]
struct Browser {
    moves: Moves,
    insets: Vec<Thickness>,
    pushed: u32,
}

/// Host events for React SkiaShell (UseBrowserHistory) and Super.Insets: the URL hash at start,
/// the browser's back / forward to the listening controls, the safe area to the resize handler.
#[test]
fn history_moves_reach_listening_controls_and_insets_rerun_the_resize_handler() {
    use drawnui::App as _;
    let ui = Ui::new(Browser::default(), |app| {
        SkiaLayout::new().fill().children(Build::new(Pages { moves: app.moves.clone() }).listen_history().on_tapped(|_me, app: &mut Browser, cx| {
            if cx.has_history() {
                app.pushed += 1;
                cx.history(drawnui::HistoryOp::Push { depth: app.pushed, hash: Some(format!("#/page{}", app.pushed)) });
            }
        }))
    })
    .on_canvas_resized(|app: &mut Browser, _size, cx| app.insets.push(cx.safe_insets()));
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.ui.location("#/start", None);
    host.settle();
    assert!(host.ui.state.moves.borrow().is_empty(), "the start is no move");
    // No browser history (the desktop, a plain test): nothing is pushed.
    host.tap(50.0, 50.0);
    assert_eq!(host.ui.state.pushed, 0);
    host.ui.set_history_enabled(true);
    host.tap(50.0, 50.0);
    assert_eq!(host.ui.state.pushed, 1);
    assert_eq!(host.take_history(), [drawnui::HistoryOp::Push { depth: 1, hash: Some("#/page1".into()) }]);
    assert!(host.take_history().is_empty());
    // The browser went back to the first entry, then forward.
    host.ui.location("", Some(0));
    host.ui.location("#/page1", Some(1));
    assert_eq!(*host.ui.state.moves.borrow(), [(0, String::new()), (1, "#/page1".to_owned())]);

    // A notch appeared: the resize handler runs again with it.
    let notch = Thickness { left: 0.0, top: 44.0, right: 0.0, bottom: 34.0 };
    host.ui.safe_insets(notch);
    host.settle();
    host.ui.safe_insets(notch);
    host.settle();
    assert_eq!(host.ui.state.insets, [Thickness::ZERO, notch]);
}

#[derive(Default)]
struct Game {
    paused: bool,
    seen: Vec<bool>,
}

/// React PongPage: `visibilitychange` pauses the game; the host reports the page or window
/// hidden or shown and the app's handler runs, observers after it.
#[test]
fn the_app_hears_when_the_page_is_hidden_and_shown() {
    use drawnui::App as _;
    let ui = Ui::new(Game::default(), |_| SkiaLayout::new().fill()).on_visibility_changed(|app: &mut Game, visible, _cx| {
        app.paused = !visible;
        app.seen.push(visible);
    });
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host.ui.visibility(false);
    assert!(host.ui.state.paused);
    assert!(host.ui.needs_frame());
    host.settle();
    host.ui.visibility(true);
    assert_eq!(host.ui.state.seen, [false, true]);
    assert!(!host.ui.state.paused);
}
