//! DrawnGame (React `DrawnGame.ts`, DrawnUi.Gaming) and RescalingLayout (the React Pong page's):
//! the loop on the frame clock, start with a delay, stop with no frames, pause / resume, keys
//! without focus, sprites moved by `left` / `top` without a layout and without allocating, and
//! children at the fitted scale.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::skia::Size;
use drawnui::testing::Headless;

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

/// A sprite that counts its measures and arranges.
struct Counter {
    layouts: Arc<AtomicU32>,
}

impl Control for Counter {
    fn measure(&mut self, _cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.layouts.fetch_add(1, Ordering::Relaxed);
        Size::new(width, height)
    }
    fn arrange(&mut self, _cx: &mut LayoutCx) {
        self.layouts.fetch_add(1, Ordering::Relaxed);
    }
    fn paint(&self, cx: &mut PaintCx) {
        let mut paint = drawnui::skia::Paint::default();
        paint.set_color(Color::YELLOW);
        cx.canvas.draw_rect(cx.rect, &paint);
    }
}

#[derive(Default)]
struct App {
    ticks: u32,
    deltas: Vec<f32>,
    /// The loop stops itself after this many ticks (0 = never).
    stop_after: u32,
    keys: Vec<String>,
    tapped: u32,
    /// The player's paddle direction the drag sets (React PongGame.ProcessGestures).
    movement: f32,
    moved_right: bool,
    serves: u32,
    game: Handle<DrawnGame>,
    field: Handle<RescalingLayout>,
    ball: Handle<SkiaShape>,
    paddle: Handle<SkiaShape>,
    sprite: Handle<Counter>,
    label: Handle<SkiaLabel>,
    layouts: Arc<AtomicU32>,
}

/// A 360 x 640 point field, rescaled into the canvas: a ball, a paddle and a counting sprite that
/// the loop moves, a label.
fn scene(app: &mut App, start: bool) -> Build<RescalingLayout> {
    let game = DrawnGame::new()
        .width_request(360)
        .height_request(640)
        .horizontal_options(LayoutOptions::Center)
        .vertical_options(LayoutOptions::Center)
        .background_color(Color::from_rgb(0, 100, 0))
        .assign(&mut app.game)
        .on_game_loop(|me, app: &mut App, cx, delta| {
            app.ticks += 1;
            if app.deltas.len() < app.deltas.capacity() {
                app.deltas.push(delta);
            }
            let t = app.ticks as f32;
            if let Some(mut ball) = cx.get_mut(app.ball) {
                ball.set_left(100.0 + t);
                ball.set_top(200.0 + t * 2.0);
            }
            if let Some(mut paddle) = cx.get_mut(app.paddle) {
                let left = paddle.base().p.left;
                paddle.set_left(if app.movement != 0.0 { left + app.movement * 420.0 * delta } else { 140.0 + (t * 0.1).sin() * 50.0 });
            }
            if let Some(mut sprite) = cx.get_mut(app.sprite) {
                sprite.set_left(t);
            }
            if app.ticks == app.stop_after {
                me.stop_loop();
            }
        })
        .on_key_down(|_me, app: &mut App, _cx, key| {
            app.keys.push(format!("down {}", key.key));
            true
        })
        .on_key_up(|_me, app: &mut App, _cx, key| {
            app.keys.push(format!("up {}", key.key));
            true
        })
        // PongGame.ProcessGestures: a drag steers the paddle, a release stops it, a tap serves.
        .consume_gestures(|me, app: &mut App, _cx, gesture| {
            match gesture.kind {
                GestureKind::Panning => {
                    let velocity = gesture.velocity.x / me.base().scale;
                    app.movement = if velocity.abs() > 5.0 { velocity.signum() } else { 0.0 };
                    app.moved_right |= app.movement > 0.0;
                }
                GestureKind::Up => app.movement = 0.0,
                // As React: serve, then the children still get the tap.
                GestureKind::Tapped => {
                    app.serves += 1;
                    return false;
                }
                _ => return false,
            }
            true
        })
        .children((
            SkiaShape::new()
                .shape_type(ShapeType::Circle)
                .width_request(14)
                .height_request(14)
                .background_color(Color::YELLOW)
                .assign(&mut app.ball),
            SkiaShape::new()
                .width_request(80)
                .height_request(16)
                .corner_radius(8)
                .top(584)
                .background_color(Color::from_rgb(0x4C, 0xC9, 0xF0))
                .assign(&mut app.paddle),
            Build::new(Counter { layouts: app.layouts.clone() })
                .width_request(20)
                .height_request(20)
                .top(30)
                .assign(&mut app.sprite)
                .on_tapped(|_me, app: &mut App, _cx| app.tapped += 1),
            SkiaLabel::new("0 : 0")
                .text_color(Color::WHITE)
                .horizontal_options(LayoutOptions::Center)
                .margin((0, 296, 0, 0))
                .assign(&mut app.label),
        ));
    let game = if start { game.start_loop(0.0) } else { game };
    RescalingLayout::new(360.0, 640.0).assign(&mut app.field).children(game)
}

/// A canvas of `width` x `height` pixels at `scale`.
fn host(width: i32, height: i32, scale: f32, start: bool) -> Headless<App> {
    let app = App { deltas: Vec::with_capacity(16), ..App::default() };
    let ui = Ui::new(app, move |app| scene(app, start)).background(Color::BLACK);
    let mut host = Headless::new(ui, width, height, scale);
    host.frame();
    host
}

fn game(host: &Headless<App>) -> &DrawnGame {
    host.ui.tree.find(host.ui.state.game).unwrap()
}

fn assert_idle(host: &mut Headless<App>) {
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None);
}

#[test]
fn the_loop_runs_every_frame_with_the_seconds_since_the_last() {
    let mut host = host(360, 640, 1.0, true);
    assert!(game(&host).is_running());
    assert!(host.ui.needs_frame());
    host.frame_after(16.0);
    host.frame_after(16.0);
    host.frame_after(33.0);
    host.frame_after(8.0);
    // The first tick after a start has no previous frame: 0.
    assert_eq!(host.ui.state.deltas, [0.0, 0.016, 0.033, 0.008]);
    assert!(host.ui.needs_frame());
}

#[test]
fn a_stopped_loop_asks_for_no_frames_and_a_start_waits_its_delay() {
    let mut host = host(360, 640, 1.0, true);
    host.frame_after(16.0);
    host.frame_after(16.0);
    let id = host.ui.state.game;
    host.ui.tree.get_mut(id).unwrap().stop_loop();
    assert_idle(&mut host);
    let ticks = host.ui.state.ticks;
    host.frame_after(500.0);
    assert_eq!(host.ui.state.ticks, ticks);
    assert!(!game(&host).is_running());

    host.ui.tree.get_mut(id).unwrap().start_loop(100.0);
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(host.time_ms() + 100.0));
    host.frame_after(100.0);
    host.frame_after(16.0);
    assert_eq!(host.ui.state.ticks, ticks + 2);
    // The stopped time is not a delta: a start restarts the frame clock.
    assert_eq!(host.ui.state.deltas[ticks as usize..], [0.0, 0.016]);
}

#[test]
fn a_stop_during_the_start_delay_leaves_no_wake() {
    let mut host = host(360, 640, 1.0, false);
    let id = host.ui.state.game;
    host.ui.tree.get_mut(id).unwrap().start_loop(1000.0);
    host.frame_after(16.0);
    assert!(host.ui.wake_at().is_some());
    host.ui.tree.get_mut(id).unwrap().stop_loop();
    assert_idle(&mut host);
    host.frame_after(2000.0);
    assert_eq!(host.ui.state.ticks, 0);
}

#[test]
fn the_loop_can_stop_itself() {
    let mut host = host(360, 640, 1.0, true);
    host.ui.state.stop_after = 3;
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    assert_eq!(host.ui.state.ticks, 3);
    assert_idle(&mut host);
    assert_eq!(host.ui.state.ticks, 3);
}

#[test]
fn a_game_built_without_start_waits_for_start_loop() {
    let mut host = host(360, 640, 1.0, false);
    assert_idle(&mut host);
    assert_eq!(host.ui.state.ticks, 0);
    let id = host.ui.state.game;
    host.ui.tree.get_mut(id).unwrap().start_loop(0.0);
    host.frame_after(16.0);
    host.frame_after(16.0);
    assert_eq!(host.ui.state.ticks, 1);
}

#[test]
fn resume_restarts_the_frame_clock_and_pause_is_a_flag() {
    let mut host = host(360, 640, 1.0, true);
    host.frame_after(16.0);
    let id = host.ui.state.game;
    host.ui.tree.get_mut(id).unwrap().pause();
    assert!(game(&host).is_paused());
    // As upstream the loop keeps running while paused; the game reads the flag.
    host.frame_after(5000.0);
    host.ui.tree.get_mut(id).unwrap().resume();
    assert!(!game(&host).is_paused());
    host.frame_after(3000.0);
    host.frame_after(16.0);
    assert_eq!(host.ui.state.deltas, [0.0, 5.0, 0.0, 0.016]);
}

#[test]
fn a_drag_steers_and_a_tap_serves_through_consume_gestures() {
    let mut host = host(360, 640, 1.0, true);
    host.frame_after(16.0);
    // Away from the sprites, over the game's field.
    host.pan((100.0, 450.0), (220.0, 450.0), 200.0, 10);
    assert!(host.ui.state.moved_right);
    // The release stopped the paddle.
    assert_eq!(host.ui.state.movement, 0.0);
    // A tap (the robot's `tap` settles, which a running game never does).
    host.ui.pointer(PointerKind::Down, 100.0, 450.0, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, 100.0, 450.0, host.time_ms());
    host.frame_after(16.0);
    assert_eq!(host.ui.state.serves, 1);
}

#[test]
fn the_game_hears_every_key_without_focus() {
    let mut host = host(360, 640, 1.0, true);
    assert_eq!(host.ui.focused(), None);
    host.ui.key(KeyKind::Down, "ArrowLeft", "", Modifiers::default(), false);
    host.ui.key(KeyKind::Up, "ArrowLeft", "", Modifiers::default(), false);
    host.ui.key(KeyKind::Down, "Space", "", Modifiers::default(), false);
    assert_eq!(host.ui.state.keys, ["down ArrowLeft", "up ArrowLeft", "down Space"]);
}

#[test]
fn sprites_move_by_left_and_top_without_a_layout_or_an_allocation() {
    let mut host = host(360, 640, 1.0, true);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    let layouts = host.ui.state.layouts.load(Ordering::Relaxed);
    let before = ALLOCATIONS.with(Cell::get);
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    assert_eq!(ALLOCATIONS.with(Cell::get) - before, 0);
    assert_eq!(host.ui.state.layouts.load(Ordering::Relaxed), layouts);
    // Moved: the ball's props, drawn where they say.
    let ball = host.ui.tree.base(host.ui.state.ball).unwrap();
    assert_eq!((ball.p.left, ball.p.top), (165.0, 330.0));
    assert_eq!(host.pixel(172, 337), Color::YELLOW);
}

#[test]
fn the_field_is_fitted_by_the_rendering_scale_of_its_children() {
    // 720 x 640 pixels at scale 2 (360 x 320 points): the height limits, 1 pixel per field point.
    let mut host = host(720, 640, 2.0, false);
    let field = host.ui.tree.find::<RescalingLayout>(host.ui.state.field).unwrap();
    assert_eq!(field.context_scale(), 1.0);
    assert_eq!(host.rect(host.ui.state.game), drawnui::skia::Rect::new(180.0, 0.0, 540.0, 640.0));
    assert_eq!(host.ui.tree.base(host.ui.state.game).unwrap().scale, 1.0);
    // Margins are field points too: arranged at the fitted scale.
    assert_eq!(host.rect(host.ui.state.label).top, 296.0);
    // The 20 x 20 point sprite is 20 x 20 pixels there, not 40 x 40; `top` moves it at paint.
    assert_eq!(host.rect(host.ui.state.sprite), drawnui::skia::Rect::new(180.0, 0.0, 200.0, 20.0));
    let id = host.ui.state.sprite;
    host.ui.tree.get_mut(id).unwrap().set_left(10);
    host.frame_after(16.0);
    assert_eq!(host.pixel(195, 40), Color::YELLOW);
    assert_eq!(host.pixel(185, 40), Color::from_rgb(0, 100, 0));
    // Hit testing follows: a tap on the moved sprite reaches it.
    host.tap(195.0, 40.0);
    assert_eq!(host.ui.state.tapped, 1);

    // 400 x 800 pixels at scale 1: the width limits, 400 / 360 pixels per field point.
    let mut host = host_sized(400, 800);
    let scale = host.ui.tree.find::<RescalingLayout>(host.ui.state.field).unwrap().context_scale();
    assert_eq!(scale, 400.0 / 360.0);
    let r = host.rect(host.ui.state.game);
    assert_eq!((r.left, r.width()), (0.0, 400.0));
    assert!((r.height() - 640.0 * scale).abs() <= 1.0);
    host.frame_after(16.0);
}

fn host_sized(width: i32, height: i32) -> Headless<App> {
    host(width, height, 1.0, false)
}
