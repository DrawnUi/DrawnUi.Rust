//! SkiaLottie playback as React SkiaLottie + AnimatedFramesRenderer: the file through the asset
//! channel, the range animator over 0..TotalFrames on the frame clock, repeat, speed, stop and
//! seek, the IsOn toggle, handlers, one load per source, caches and frames.
//! `ok.json`: 164 frames at 80 fps, 2050 ms a run.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::prelude::*;
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

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

const OK: &[u8] = include_bytes!("lottie/ok.json");
const SHIELD: &[u8] = include_bytes!("lottie/shield.json");
const RUN_MS: f64 = 2050.0;

#[derive(Default)]
struct App {
    log: Vec<String>,
    lottie: Handle<SkiaLottie>,
}

fn file(url: &str) -> Option<Vec<u8>> {
    match url {
        "lottie/ok.json" => Some(OK.to_vec()),
        "lottie/shield.json" => Some(SHIELD.to_vec()),
        _ => None,
    }
}

/// A 100 x 100 lottie of `source`, logging its handlers; `setup` sets its props.
fn host(source: &str, setup: impl FnOnce(Build<SkiaLottie>) -> Build<SkiaLottie>) -> Headless<App> {
    let lottie = SkiaLottie::new(source)
        .width_request(100)
        .height_request(100)
        .on_success(|me, app: &mut App, cx, source| {
            let frames = cx.find::<SkiaLottie>(me).unwrap().total_frames();
            app.log.push(format!("success {source} {frames}"));
        })
        .on_error(|_, app: &mut App, _, source| app.log.push(format!("error {source}")))
        .on_started(|_, app: &mut App, _| app.log.push("started".into()))
        .on_finished(|_, app: &mut App, _| app.log.push("finished".into()));
    let lottie = setup(lottie);
    let ui = Ui::new(App::default(), move |app| SkiaLayout::new().children(lottie.assign(&mut app.lottie))).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.frame();
    host
}

fn lottie(host: &Headless<App>) -> &SkiaLottie {
    host.ui.tree.find(host.ui.state.lottie).unwrap()
}

fn log(host: &mut Headless<App>) -> Vec<String> {
    std::mem::take(&mut host.ui.state.log)
}

/// Delivers the files and runs the frame the handlers run in (the run starts there).
fn load(host: &mut Headless<App>) -> Vec<String> {
    let answered = host.deliver_assets(file);
    host.frame_after(16.0);
    answered
}

fn assert_idle(host: &mut Headless<App>) {
    host.frame_after(16.0);
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None);
}

#[test]
fn a_file_plays_once_on_the_frame_clock_then_shows_its_default_frame() {
    let mut host = host("lottie/ok.json", |l| l);
    assert!(lottie(&host).is_loading());
    assert_eq!(load(&mut host), ["lottie/ok.json"]);
    assert_eq!(log(&mut host), ["success lottie/ok.json 164", "started"]);
    assert!(lottie(&host).is_playing());
    assert_eq!(lottie(&host).frame(), 0.0);

    host.frame_after(RUN_MS / 2.0);
    assert_eq!(lottie(&host).frame(), 82.0);
    host.frame_after(RUN_MS / 4.0);
    assert_eq!(lottie(&host).frame(), 123.0);
    host.frame_after(RUN_MS / 4.0 - 1.0);
    assert!(lottie(&host).frame() < 164.0 && lottie(&host).is_playing());
    // The end: Finished, and the default frame shows (React OnFinished + SeekToDefaultFrame).
    host.frame_after(1.0);
    assert_eq!(log(&mut host), ["finished"]);
    assert!(!lottie(&host).is_playing());
    assert_eq!(lottie(&host).frame(), 0.0);
    assert_idle(&mut host);
}

#[test]
fn speed_ratio_divides_the_run_above_one_and_stretches_it_below() {
    for (ratio, run_ms) in [(2.0f32, RUN_MS / 2.0), (0.5, RUN_MS * 1.5)] {
        let mut host = host("lottie/ok.json", |l| l.speed_ratio(ratio).stop_at_current_frame(true));
        load(&mut host);
        host.frame_after(run_ms / 2.0);
        assert_eq!(lottie(&host).frame(), 82.0, "speed {ratio}");
        host.frame_after(run_ms / 2.0);
        assert!(!lottie(&host).is_playing());
        // StopAtCurrentFrame: the end stays.
        assert_eq!(lottie(&host).frame(), 164.0);
    }
}

#[test]
fn a_repeat_starts_again_on_the_next_frame() {
    let mut host = host("lottie/ok.json", |l| l.repeat(1));
    load(&mut host);
    log(&mut host);
    host.frame_after(RUN_MS);
    assert_eq!(lottie(&host).frame(), 164.0);
    assert!(lottie(&host).is_playing());
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 0.0);
    host.frame_after(RUN_MS / 2.0);
    assert_eq!(lottie(&host).frame(), 82.0);
    host.frame_after(RUN_MS / 2.0);
    assert_eq!(log(&mut host), ["finished"]);
    assert_idle(&mut host);
}

#[test]
fn stop_finishes_a_playing_run_and_seek_shows_any_frame() {
    let mut host = host("lottie/ok.json", |l| l.repeat(-1).default_frame(10));
    load(&mut host);
    log(&mut host);
    host.frame_after(RUN_MS * 3.3);
    assert!(lottie(&host).is_playing());

    let id = host.ui.state.lottie;
    host.ui.tree.get_mut(id).unwrap().stop();
    host.frame_after(16.0);
    // As upstream the animator's OnStop raises Finished; the default frame shows.
    assert_eq!(log(&mut host), ["finished"]);
    assert_eq!(lottie(&host).frame(), 10.0);
    assert_idle(&mut host);

    host.ui.tree.get_mut(id).unwrap().seek(30);
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 30.0);
    host.ui.tree.get_mut(id).unwrap().go_to_end();
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 164.0);
    host.ui.tree.get_mut(id).unwrap().go_to_start();
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 0.0);
    // Stopping what is not playing raises nothing.
    host.ui.tree.get_mut(id).unwrap().stop();
    host.frame_after(16.0);
    assert!(log(&mut host).is_empty());
    assert_idle(&mut host);

    // Start plays from the first frame again.
    host.ui.tree.get_mut(id).unwrap().start();
    host.frame_after(16.0);
    host.frame_after(16.0);
    assert_eq!(log(&mut host), ["started"]);
    assert!(lottie(&host).is_playing());
}

#[test]
fn is_on_shows_the_matching_default_frame_while_stopped() {
    let mut host = host("lottie/ok.json", |l| l.auto_play(false).default_frame(0).default_frame_when_on(-1));
    load(&mut host);
    assert_eq!(log(&mut host), ["success lottie/ok.json 164"]);
    assert_eq!(lottie(&host).frame(), 0.0);
    let id = host.ui.state.lottie;
    host.ui.tree.get_mut(id).unwrap().set_is_on(true);
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 164.0);
    host.ui.tree.get_mut(id).unwrap().set_is_on(false);
    host.frame_after(16.0);
    assert_eq!(lottie(&host).frame(), 0.0);
    assert!(log(&mut host).is_empty());
    assert_idle(&mut host);
}

#[test]
fn start_before_the_file_is_there_plays_when_it_arrives() {
    let mut host = host("lottie/ok.json", |l| l.auto_play(false));
    let id = host.ui.state.lottie;
    host.ui.tree.get_mut(id).unwrap().start();
    host.frame_after(16.0);
    assert!(!lottie(&host).is_playing());
    load(&mut host);
    assert_eq!(log(&mut host), ["success lottie/ok.json 164", "started"]);
    assert!(lottie(&host).is_playing());
}

#[test]
fn a_file_that_does_not_load_raises_error_and_draws_nothing() {
    let mut host = host("lottie/missing.json", |l| l);
    load(&mut host);
    assert_eq!(log(&mut host), ["error lottie/missing.json"]);
    assert!(lottie(&host).has_error() && !lottie(&host).is_loading());
    assert_eq!(host.pixel(50, 50), Color::WHITE);
    assert_idle(&mut host);
}

#[test]
fn one_load_per_source_whatever_the_colors() {
    let ui = Ui::new(App::default(), |_| {
        SkiaLayout::row().spacing(0).children((
            SkiaLottie::new("lottie/ok.json").width_request(100).height_request(100).auto_play(false).default_frame(100),
            SkiaLottie::new("lottie/ok.json").width_request(100).height_request(100).auto_play(false).default_frame(100),
            SkiaLottie::new("lottie/ok.json")
                .width_request(100)
                .height_request(100)
                .auto_play(false)
                .default_frame(100)
                .color_tint(Color::from_rgb(0x20, 0xC9, 0x97)),
        ))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 300, 100, 1.0);
    host.frame();
    assert_eq!(host.deliver_assets(file), ["lottie/ok.json"]);
    host.settle();
    // The check mark's stem, in each: its own green, and the tint.
    assert_eq!(host.pixel(64, 39), Color::from_rgb(0x12, 0xE2, 0x43));
    assert_eq!(host.pixel(164, 39), Color::from_rgb(0x12, 0xE2, 0x43));
    assert_eq!(host.pixel(264, 39), Color::from_rgb(0x20, 0xC9, 0x97));
}

#[test]
fn a_stopped_frame_is_recorded_once_and_a_playing_one_per_frame() {
    let mut host = host("lottie/ok.json", |l| l.auto_play(false));
    load(&mut host);
    let id = host.ui.state.lottie;
    let recorded = host.cache_records(id);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert_eq!(host.cache_records(id), recorded);
    assert_idle(&mut host);

    host.ui.tree.get_mut(id).unwrap().start();
    // The run's first tick shows frame 0, which was on screen already.
    host.frame_after(16.0);
    host.frame_after(16.0);
    let before = host.cache_records(id);
    for _ in 0..10 {
        host.frame_after(16.0);
        assert!(host.ui.needs_frame());
    }
    assert_eq!(host.cache_records(id), before + 10);
}

#[test]
fn a_playing_frame_allocates_nothing_in_rust() {
    let mut host = host("lottie/shield.json", |l| l.repeat(-1));
    load(&mut host);
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    let (before, records) = (allocations(), host.cache_records(host.ui.state.lottie));
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    assert_eq!(allocations() - before, 0);
    assert!(lottie(&host).is_playing());
    assert_eq!(host.cache_records(host.ui.state.lottie), records + 60);
}
