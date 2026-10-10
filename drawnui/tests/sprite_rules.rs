//! SkiaSprite and SkiaSpriteSet: frames cut from a sheet, trimmed and fitted; playback on the
//! frame clock; a set switching sprites by state. The sheet is generated here and delivered as a
//! host would deliver a file.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use drawnui::prelude::*;
use drawnui::skia::{EncodedImageFormat, ImageInfo, Paint, surfaces};
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
static ALLOCATOR: Counting = Counting;

const FRAME: i32 = 16;
/// Transparent border of every frame, trimmed by the sprite.
const BORDER: i32 = 2;
const COLORS: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

/// PNG of a sheet: `columns` x `rows` frames of 16 x 16 px, each a solid square of the color for
/// its number (cycling) inside a 2 px transparent border.
fn sheet(columns: i32, rows: i32) -> Vec<u8> {
    let info = ImageInfo::new_n32_premul((columns * FRAME, rows * FRAME), None);
    let mut surface = surfaces::raster(&info, None, None).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    let mut paint = Paint::default();
    for frame in 0..columns * rows {
        let (x, y) = ((frame % columns) * FRAME, (frame / columns) * FRAME);
        paint.set_color(COLORS[frame as usize % COLORS.len()]);
        let square = Rect::from_xywh((x + BORDER) as f32, (y + BORDER) as f32, (FRAME - 2 * BORDER) as f32, (FRAME - 2 * BORDER) as f32);
        canvas.draw_rect(square, &paint);
    }
    surface.image_snapshot().encode(None, EncodedImageFormat::PNG, None).expect("png").as_bytes().to_vec()
}

#[derive(Default)]
struct App {
    sprite: Handle<SkiaSprite>,
    set: Handle<SkiaSpriteSet>,
    loaded: Vec<String>,
    failed: Vec<String>,
    /// `total_frames` as the success handler saw it.
    frames_when_loaded: u32,
    /// Started / Finished, in order.
    played: Vec<&'static str>,
}

/// The sprite in a 64 x 64 box at (20, 20) on white, its sheet delivered before the first frame shows.
fn sprite_host(sprite: Build<SkiaSprite>) -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        let sprite = sprite
            .assign(&mut app.sprite)
            .on_success(|me, app: &mut App, cx, source| {
                app.loaded.push(source.to_owned());
                app.frames_when_loaded = cx.find::<SkiaSprite>(me).map_or(0, |sprite| sprite.total_frames());
            })
            .on_error(|_me, app: &mut App, _cx, source| app.failed.push(source.to_owned()));
        SkiaLayout::new().fill().children(sprite.margin((20, 20, 0, 0)).width_request(64).height_request(64))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 120, 120, 1.0);
    host.frame();
    host.deliver_images(|source| match source {
        "sheet4x1.png" => Some(sheet(4, 1)),
        "sheet2x2.png" => Some(sheet(2, 2)),
        _ => None,
    });
    host.frame();
    host
}

/// Draws the frames a host would draw at once, at the same time (a handler that ran or a change
/// committed in the last frame asks for one more), at most 3. Returns how many.
fn follow_ups(host: &mut Headless<App>) -> u32 {
    let mut frames = 0;
    while host.ui.needs_frame() && frames < 3 {
        host.frame_after(0.0);
        frames += 1;
    }
    frames
}

fn ten_fps(source: &str) -> Build<SkiaSprite> {
    SkiaSprite::new(source).columns(4).rows(1).frames_per_second(10).repeat(-1)
}

/// The color the sprite shows: the frame is 16 px fitted into 64, so its 12 px square lands 8 px
/// in from every side of the box.
fn shown(host: &mut Headless<App>) -> Color {
    host.pixel(20 + 32, 20 + 32)
}

#[test]
fn frames_are_cut_from_the_sheet_trimmed_and_fitted() {
    let mut host = sprite_host(ten_fps("sheet4x1.png"));
    let sprite = host.ui.state.sprite;
    let it = host.ui.tree.find::<SkiaSprite>(sprite).unwrap();
    assert_eq!((it.total_frames(), it.frame_size(), it.duration_ms()), (4, (16, 16), 400.0));
    assert!(it.is_playing(), "auto play");
    assert_eq!(host.ui.state.loaded, ["sheet4x1.png"]);
    assert_eq!(host.ui.state.frames_when_loaded, 4, "the frames are known in the success handler");
    // Frame 0 at t = 0: red, 4 px per sheet pixel, the 2 px border trimmed away.
    assert_eq!(shown(&mut host), Color::RED);
    assert_eq!(host.pixel(20 + 8, 20 + 8), Color::RED);
    assert_eq!(host.pixel(20 + 7, 20 + 32), Color::WHITE);
    assert_eq!(host.pixel(20 + 56, 20 + 32), Color::WHITE);
    assert_eq!(host.pixel(20 + 55, 20 + 55), Color::RED);
    // Nearest sampling: the edge is a hard step.
    assert_eq!(host.pixel(20 + 8, 20 + 7), Color::WHITE);

    let mut host = sprite_host(SkiaSprite::new("sheet2x2.png").columns(2).rows(2).frames_per_second(10).repeat(-1));
    host.frame_after(250.0);
    // 250 ms at 10 fps: frame 2, the first of the second row.
    assert_eq!(host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().current_frame(), 2);
    assert_eq!(shown(&mut host), Color::BLUE);
}

/// Frames step on the frame clock at `frames_per_second`, forever with `repeat` -1; between
/// two frames nothing asks for a frame (the animator sleeps until the next one is due).
#[test]
fn frames_advance_on_the_clock_and_sleep_in_between() {
    let mut host = sprite_host(ten_fps("sheet4x1.png"));
    assert_eq!(shown(&mut host), Color::RED);
    assert!(follow_ups(&mut host) <= 1);
    assert!(!host.ui.needs_frame(), "nothing to draw until the next frame is due");
    assert_eq!(host.ui.wake_at(), Some(100.0));
    host.frame_after(99.0);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(2.0);
    assert_eq!(shown(&mut host), Color::GREEN);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::BLUE);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::YELLOW);
    // Around again.
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(1000.0);
    assert_eq!(shown(&mut host), Color::BLUE);
    // The sleeping animator wakes the host when the next frame is due: 1401 is 201 ms into a
    // run, frame 3 starts at 300.
    assert_eq!(host.time_ms(), 1401.0);
    assert!(follow_ups(&mut host) <= 1);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(1500.0));
}

/// `repeat` counts the runs after the first. At the end the position stops at the end of the
/// range, which shows the first frame, as in C# and DrawnUi.React (`GetFrameNumberFromTime`
/// wraps the range's end to 0).
#[test]
fn repeat_counts_runs() {
    let mut host = sprite_host(ten_fps("sheet4x1.png").repeat(0));
    host.frame_after(350.0);
    assert_eq!(shown(&mut host), Color::YELLOW);
    host.frame_after(200.0);
    assert_eq!(shown(&mut host), Color::RED, "one run, ended on the range's end");
    assert!(!host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().is_playing());
    assert!(follow_ups(&mut host) <= 1);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None);

    let mut host = sprite_host(ten_fps("sheet4x1.png").repeat(1));
    host.frame_after(550.0);
    assert_eq!(shown(&mut host), Color::GREEN, "the second run plays");
    assert!(host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().is_playing());
    host.frame_after(500.0);
    assert_eq!(shown(&mut host), Color::RED);
    assert!(!host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().is_playing());
}

/// SpeedRatio as upstream ApplySpeed: 2 runs in half the time, below 1 a run takes 1 + ratio
/// times its length. A new frame rate while playing applies at once.
#[test]
fn speed_ratio_and_a_new_frame_rate() {
    let mut host = sprite_host(ten_fps("sheet4x1.png").speed_ratio(2));
    host.frame_after(51.0);
    assert_eq!(shown(&mut host), Color::GREEN);
    assert_eq!(host.ui.wake_at(), Some(100.0));

    // 0.5: a 400 ms run takes 600 ms, a frame 150 ms.
    let mut host = sprite_host(ten_fps("sheet4x1.png").speed_ratio(0.5));
    host.frame_after(149.0);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(2.0);
    assert_eq!(shown(&mut host), Color::GREEN);
    assert_eq!(host.ui.wake_at(), Some(300.0));

    // 10 fps, then 20 from the frame at 166 ms: the run is 200 ms, 166 is frame 3, shown at the
    // next frame instead of when the old rate's next frame was due.
    let mut host = sprite_host(ten_fps("sheet4x1.png"));
    host.frame_after(150.0);
    assert_eq!(shown(&mut host), Color::GREEN);
    host.ui.tree.get_mut(host.ui.state.sprite).unwrap().set_frames_per_second(20);
    host.frame_after(16.0);
    host.frame_after(0.0);
    assert_eq!(shown(&mut host), Color::YELLOW, "166 ms at 20 fps is frame 3");
    assert_eq!(host.ui.wake_at(), Some(200.0));
}

/// The demo's sheet: 8 frames of 192 x 192, 15 fps; the size follows the frame's aspect.
#[test]
fn the_demo_sheet() {
    const IDLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/anims/BlueWarrior/Warrior_Idle.png"));
    let ui = Ui::new(App::default(), |app| {
        let sprite = SkiaSprite::new("anims/BlueWarrior/Warrior_Idle.png").columns(8).rows(1).frames_per_second(15).repeat(-1);
        SkiaStack::new().children(sprite.width_request(160).assign(&mut app.sprite))
    });
    let mut host = Headless::new(ui, 400, 400, 2.0);
    host.frame();
    host.deliver_images(|_| Some(IDLE.to_vec()));
    host.frame();
    let sprite = host.ui.state.sprite;
    let it = host.ui.tree.find::<SkiaSprite>(sprite).unwrap();
    assert_eq!((it.total_frames(), it.frame_size()), (8, (192, 192)));
    assert!((it.duration_ms() - 533.333).abs() < 0.01);
    assert_eq!(host.rect(sprite), Rect::from_xywh(0.0, 0.0, 320.0, 320.0));
    host.frame_after(70.0);
    assert_eq!(host.ui.tree.find::<SkiaSprite>(sprite).unwrap().current_frame(), 1);
}

#[test]
fn start_stop_and_seek() {
    let mut host = sprite_host(ten_fps("sheet4x1.png").auto_play(false));
    let sprite = host.ui.state.sprite;
    assert!(!host.ui.tree.find::<SkiaSprite>(sprite).unwrap().is_playing());
    assert_eq!(shown(&mut host), Color::RED, "the default frame shows");
    host.frame_after(500.0);
    assert_eq!(shown(&mut host), Color::RED);

    host.ui.tree.get_mut(sprite).unwrap().start();
    host.frame_after(16.0);
    assert!(host.ui.tree.find::<SkiaSprite>(sprite).unwrap().is_playing());
    host.frame_after(200.0);
    assert_eq!(shown(&mut host), Color::BLUE);

    host.ui.tree.get_mut(sprite).unwrap().stop();
    host.frame_after(16.0);
    assert_eq!(shown(&mut host), Color::BLUE, "stopped on its frame");
    host.frame_after(300.0);
    assert_eq!(shown(&mut host), Color::BLUE);
    assert!(!host.ui.needs_frame());

    host.ui.tree.get_mut(sprite).unwrap().seek(150);
    host.frame_after(16.0);
    assert_eq!(shown(&mut host), Color::GREEN, "seek shows the frame at 150 ms");

    // Start plays from the first frame again.
    host.ui.tree.get_mut(sprite).unwrap().start();
    host.frame_after(16.0);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::GREEN);

    // Seek while playing: playback goes on from there.
    host.ui.tree.get_mut(sprite).unwrap().seek(300);
    host.frame_after(16.0);
    assert_eq!(shown(&mut host), Color::YELLOW);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::RED);
}

#[test]
fn frame_sequence_max_frames_and_named_animations() {
    let mut host = sprite_host(ten_fps("sheet4x1.png").frame_sequence(vec![3, 1]));
    assert_eq!(host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().total_frames(), 2);
    assert_eq!(shown(&mut host), Color::YELLOW);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::GREEN);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::YELLOW);

    let mut host = sprite_host(ten_fps("sheet4x1.png").max_frames(2));
    host.frame_after(200.0);
    assert_eq!(shown(&mut host), Color::RED, "two frames only");

    SkiaSprite::create_animation_sequence("blink", vec![0, 2]);
    let mut host = sprite_host(ten_fps("sheet4x1.png").animation_name("blink"));
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::BLUE);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::RED);
}

#[test]
fn a_missing_sheet_reports_an_error_and_draws_nothing() {
    let mut host = sprite_host(ten_fps("missing.png"));
    assert_eq!(host.ui.state.failed, ["missing.png"]);
    assert!(host.ui.tree.find::<SkiaSprite>(host.ui.state.sprite).unwrap().has_error());
    assert_eq!(shown(&mut host), Color::WHITE);
    host.frame_after(100.0);
    assert!(!host.ui.needs_frame());
}

/// A sprite step marks the sprite dirty and paints a frame from the sheet: no Rust allocation.
#[test]
fn a_frame_step_allocates_nothing() {
    let mut host = sprite_host(ten_fps("sheet4x1.png"));
    host.frame_after(100.0);
    host.frame_after(100.0);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..10 {
        host.frame_after(100.0);
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
    assert_eq!(shown(&mut host), Color::RED);
}

// ---------------------------------------------------------------- SkiaSpriteSet

fn set_host(state: i32) -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        let set = SkiaSpriteSet::new()
            .define(0, "sheet4x1.png", 4, 1, 10, -1, true)
            .define(1, "sheet2x2.png", 2, 2, 10, -1, true)
            .state(state)
            .assign(&mut app.set)
            .margin((20, 20, 0, 0))
            .width_request(64)
            .height_request(64);
        SkiaLayout::new().fill().children(set)
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 120, 120, 1.0);
    host.frame();
    host.deliver_images(|source| match source {
        "sheet4x1.png" => Some(sheet(4, 1)),
        "sheet2x2.png" => Some(sheet(2, 2)),
        _ => None,
    });
    host.frame();
    host
}

#[test]
fn a_sprite_set_shows_and_plays_the_sprite_of_its_state() {
    let mut host = set_host(0);
    let set = host.ui.state.set;
    let idle = host.ui.tree.find::<SkiaSpriteSet>(set).unwrap().current_sprite().expect("a sprite for state 0");
    assert_eq!(host.ui.tree.find::<SkiaSprite>(idle).unwrap().total_frames(), 4);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::GREEN);

    // State 1: the other sprite shows from its first frame, the old one stops.
    host.ui.tree.get_mut(set).unwrap().set_state(1);
    host.frame_after(16.0);
    let run = host.ui.tree.find::<SkiaSpriteSet>(set).unwrap().current_sprite().unwrap();
    assert_ne!(run.id(), idle.id());
    assert_eq!(host.ui.tree.find::<SkiaSprite>(run).unwrap().total_frames(), 4);
    assert!(!host.ui.tree.find::<SkiaSprite>(idle).unwrap().is_playing());
    assert!(!host.ui.tree.base(idle).unwrap().p.is_visible);
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(250.0);
    assert_eq!(shown(&mut host), Color::BLUE, "frame 2 of the 2 x 2 sheet");

    // Mirroring the current sprite, as WarriorSprite does for a left-facing state.
    host.ui.tree.get_mut(run).unwrap().set_scale_x(-1);
    host.frame_after(0.0);
    assert_eq!(shown(&mut host), Color::BLUE);

    // Back to state 0: it plays from its first frame again.
    host.ui.tree.get_mut(set).unwrap().set_state(0);
    host.frame_after(16.0);
    assert_eq!(host.ui.tree.find::<SkiaSpriteSet>(set).unwrap().current_sprite().map(|s| s.id()), Some(idle.id()));
    assert_eq!(shown(&mut host), Color::RED);
    host.frame_after(100.0);
    assert_eq!(shown(&mut host), Color::GREEN);

    // A state that was not defined keeps the sprite shown, as upstream.
    host.ui.tree.get_mut(set).unwrap().set_state(7);
    host.frame_after(100.0);
    assert_eq!(host.ui.tree.find::<SkiaSpriteSet>(set).unwrap().current_sprite().map(|s| s.id()), Some(idle.id()));
    assert_eq!(shown(&mut host), Color::BLUE);
}

/// A set built with another initial state starts on that sprite.
#[test]
fn a_sprite_set_starts_in_its_initial_state() {
    let mut host = set_host(1);
    host.frame_after(250.0);
    assert_eq!(shown(&mut host), Color::BLUE);
}

/// A measurement: a frame in which a sprite steps, uncached (the default) and image-cached (the
/// demo page's `UseCache="Image"`, upstream's `Define`), alone and in a set (cached Operations).
///
/// `cargo test --release -p drawnui --test sprite_rules -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn cost_of_a_frame_step() {
    const IDLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/anims/BlueWarrior/Warrior_Idle.png"));
    const STEPS: u32 = 400;
    for in_set in [false, true] {
        for cache in [CacheType::None, CacheType::Image] {
            let ui = Ui::new(App::default(), move |app| {
                let content: Build<SkiaLayout> = SkiaLayout::new().fill();
                match in_set {
                    false => {
                        let sprite = SkiaSprite::new("idle.png").columns(8).rows(1).frames_per_second(15).repeat(-1);
                        content.children(sprite.width_request(160).height_request(160).use_cache(cache).assign(&mut app.sprite))
                    }
                    true => {
                        let set = SkiaSpriteSet::new().define(0, "idle.png", 8, 1, 15, -1, true);
                        content.children(set.width_request(160).height_request(160).assign(&mut app.set))
                    }
                }
            });
            let mut host = Headless::new(ui, 800, 600, 2.0);
            if in_set {
                let sprite = host.ui.tree.find::<SkiaSpriteSet>(host.ui.state.set).unwrap().current_sprite().unwrap();
                host.ui.tree.get_mut(sprite).unwrap().set_use_cache(cache);
            }
            host.frame();
            host.deliver_images(|_| Some(IDLE.to_vec()));
            host.frame();
            let start = Instant::now();
            for _ in 0..STEPS {
                // One frame per sprite frame: 1000 / 15 ms.
                host.frame_after(1000.0 / 15.0);
            }
            let micros = start.elapsed().as_secs_f64() * 1e6 / STEPS as f64;
            let place = if in_set { "in a set" } else { "alone" };
            println!("160 pt sprite at scale 2, {place}, sprite {cache:?}: {micros:.0} us per stepped frame");
        }
    }
}

/// DrawnUI Started / Finished: playing starts; it ends after the last run, or when it is stopped
/// or started again while it plays (Finished, then Started).
#[test]
fn started_and_finished_report_playing() {
    let sprite = ten_fps("sheet4x1.png")
        .repeat(0)
        .on_started(|_me, app: &mut App, _cx| app.played.push("started"))
        .on_finished(|_me, app: &mut App, _cx| app.played.push("finished"));
    let mut host = sprite_host(sprite);
    host.frame_after(16.0);
    assert_eq!(host.ui.state.played, ["started"]);
    host.frame_after(550.0);
    host.frame_after(16.0);
    assert_eq!(host.ui.state.played, ["started", "finished"], "after its one run");

    host.ui.tree.get_mut(host.ui.state.sprite).unwrap().start();
    host.frame_after(16.0);
    host.frame_after(16.0);
    host.ui.tree.get_mut(host.ui.state.sprite).unwrap().start();
    host.frame_after(16.0);
    host.frame_after(16.0);
    host.ui.tree.get_mut(host.ui.state.sprite).unwrap().stop();
    host.frame_after(16.0);
    host.frame_after(16.0);
    let tail = &host.ui.state.played[2..];
    assert_eq!(tail, ["started", "finished", "started", "finished"], "started, restarted, stopped");
}
