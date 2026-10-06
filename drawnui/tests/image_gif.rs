//! SkiaGif and the frame player: frames decoded by the host, the frame picked by time on the
//! frame clock, AutoPlay, Repeat, SpeedRatio, DefaultFrame, Start / Stop / Seek, Started /
//! Finished. The rules are React's `AnimatedFramesRenderer.ts` and `SkiaGif.ts`; the numbers are
//! banana.gif of the React demo: 365 x 360, 8 frames of 100 ms.

mod image_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use drawnui::prelude::*;
use drawnui::skia::ISize;
use drawnui::testing::Headless;
use drawnui::{Detached, Images};
use image_common::*;

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

const BANANA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/images/banana.gif");

#[derive(Default)]
struct App {
    gif: Handle<SkiaGif>,
    log: Vec<String>,
}

fn banana(_: &str) -> Option<Vec<u8>> {
    std::fs::read(BANANA).ok()
}

/// A 140 x 140 gif at the top left, with its handlers logging, `configure`d before it mounts.
fn host(configure: impl FnOnce(Build<SkiaGif>) -> Build<SkiaGif>) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let gif = SkiaGif::new("banana.gif")
            .width_request(140)
            .height_request(140)
            .background_color(Color::BLACK)
            .on_success(|_me, app: &mut App, _cx, source| app.log.push(format!("ok {source}")))
            .on_error(|_me, app: &mut App, _cx, source| app.log.push(format!("error {source}")))
            .on_started(|_me, app: &mut App, _cx| app.log.push("started".to_owned()))
            .on_finished(|_me, app: &mut App, _cx| app.log.push("finished".to_owned()));
        let gif = configure(gif).assign(&mut app.gif);
        SkiaLayout::new().fill().children((gif,))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 200, 200, 1.0);
    host.settle();
    host
}

fn gif(host: &Headless<App>) -> &SkiaGif {
    host.ui.tree.find::<SkiaGif>(host.ui.state.gif).unwrap()
}

/// Two frames at the same clock time: the one the frames arrive in lays the gif out and starts
/// it, the next is its first tick (Started, after Success as in React).
fn started(host: &mut Headless<App>) {
    host.frame();
    host.frame();
}

/// Frames a `play_after` long clock shows, one tick each, from a fresh start.
fn frames_over(host: &mut Headless<App>, steps_ms: &[f64]) -> Vec<usize> {
    steps_ms
        .iter()
        .map(|ms| {
            host.frame_after(*ms);
            gif(host).player().current()
        })
        .collect()
}

#[test]
fn the_host_decodes_every_frame_once_and_a_still_file_has_none() {
    let frames = Images::decode_frames(&banana("").unwrap()).unwrap();
    assert_eq!(frames.source_size, ISize::new(365, 360));
    let frames = frames.frames.expect("an animated file");
    assert_eq!((frames.images.len(), frames.durations.len(), frames.duration_ms()), (8, 8, 800));
    assert!(frames.durations.iter().all(|ms| *ms == 100));
    // The frames differ: the banana moves.
    let (a, b) = (&frames.images[0], &frames.images[4]);
    let differ = (0..360).step_by(5).flat_map(|y| (0..365).step_by(5).map(move |x| (x, y))).any(|(x, y)| {
        let pixel = |image: &drawnui::skia::Image| image.peek_pixels().unwrap().get_color((x, y));
        pixel(a) != pixel(b)
    });
    assert!(differ);
    let still = Images::decode_frames(&quadrants(40, 20)).unwrap();
    assert!(still.frames.is_none() && still.image.width() == 40);
}

#[test]
fn a_gif_asks_for_frames_plays_them_by_time_and_repeats_as_told() {
    let mut host = host(|gif| gif.repeat(-1));
    let asked = host.deliver_images(banana);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].frames && (asked[0].width, asked[0].height) == (0, 0), "{asked:?}");
    started(&mut host);
    let animation = gif(&host).animation().expect("frames");
    assert_eq!((animation.images.len(), animation.duration_ms(), animation.size), (8, 800, ISize::new(365, 360)));
    assert!(gif(&host).player().is_playing());
    assert_eq!(host.ui.state.log, ["ok banana.gif", "started"]);

    // 100 ms a frame on the frame clock; forever: the ninth 100 ms shows frame 0 again.
    let shown = frames_over(&mut host, &[50.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0]);
    assert_eq!(shown, [0, 1, 2, 3, 4, 5, 6, 7, 0, 1]);
    assert!(gif(&host).player().is_playing());
    // The picture changes on screen: frame 4 differs from frame 1 in the tile.
    let rect = host.rect(host.ui.state.gif);
    let sample = |host: &mut Headless<App>| (0..140).step_by(7).map(|i| host.pixel(rect.left as i32 + i, rect.top as i32 + i)).collect::<Vec<_>>();
    let at_1 = sample(&mut host);
    frames_over(&mut host, &[300.0]);
    assert_ne!(sample(&mut host), at_1);
}

#[test]
fn without_repeat_a_run_ends_on_the_first_frame_and_reports_finished() {
    let mut host = host(|gif| gif);
    host.deliver_images(banana);
    started(&mut host);
    let shown = frames_over(&mut host, &[50.0, 700.0, 50.0, 100.0]);
    // At 750 ms frame 7, at 800 the run is over: the range wraps to frame 0, playing stops.
    assert_eq!(shown, [0, 7, 0, 0]);
    assert!(!gif(&host).player().is_playing());
    assert_eq!(host.ui.state.log, ["ok banana.gif", "started", "finished"]);
    // Nothing keeps asking for frames.
    assert!(!host.ui.needs_frame());

    // Repeat 2: three runs, then the end.
    let mut host = self::host(|gif| gif.repeat(2));
    host.deliver_images(banana);
    started(&mut host);
    frames_over(&mut host, &[50.0, 800.0, 800.0]);
    assert!(gif(&host).player().is_playing());
    frames_over(&mut host, &[800.0]);
    assert!(!gif(&host).player().is_playing());
    assert_eq!(host.ui.state.log.last().map(String::as_str), Some("finished"));
}

#[test]
fn speed_ratio_stretches_or_shortens_a_run_as_upstream() {
    // Twice as fast: a run of 400 ms; at 150 ms the position is 300, frame 3.
    let mut host = host(|gif| gif.speed_ratio(2).repeat(-1));
    host.deliver_images(banana);
    started(&mut host);
    assert_eq!(frames_over(&mut host, &[150.0]), [3]);
    // Half: upstream makes the run 1 + 0.5 times the length, 1200 ms; at 150 ms the position is 100, frame 1.
    let mut host = self::host(|gif| gif.speed_ratio(0.5).repeat(-1));
    host.deliver_images(banana);
    started(&mut host);
    assert_eq!(frames_over(&mut host, &[150.0, 150.0]), [1, 2]);
}

#[test]
fn start_stop_seek_and_default_frame() {
    // Not playing by itself; the default frame -1 is the last one.
    let mut host = host(|gif| gif.auto_play(false).default_frame(-1).repeat(-1));
    host.deliver_images(banana);
    host.settle();
    assert!(!gif(&host).player().is_playing());
    assert_eq!(gif(&host).player().current(), 7);
    assert_eq!(host.ui.state.log, ["ok banana.gif"]);
    assert!(!host.ui.needs_frame());

    let id = host.ui.state.gif;
    host.ui.tree.cx().start_frames(id);
    host.frame();
    assert!(gif(&host).player().is_playing());
    assert_eq!(frames_over(&mut host, &[250.0]), [2]);
    host.ui.tree.cx().stop_frames(id);
    host.frame();
    assert!(!gif(&host).player().is_playing());
    // A stop after a start reports Finished (C# and React: the animator's OnStop).
    assert_eq!(host.ui.state.log, ["ok banana.gif", "started", "finished"]);
    // Stopped where it was; frames go by, nothing moves, no frame is asked for.
    assert_eq!(frames_over(&mut host, &[500.0, 500.0]), [2, 2]);
    assert!(!host.ui.needs_frame());
    host.ui.tree.cx().seek_frames(id, 450.0);
    host.frame();
    assert_eq!(gif(&host).player().current(), 4);
    host.ui.tree.cx().seek_frames(id, -1.0);
    host.frame();
    assert_eq!(gif(&host).player().current(), 7);
    // Start again: from the first frame.
    host.ui.tree.cx().start_frames(id);
    host.frame();
    assert_eq!(gif(&host).player().current(), 0);
    assert_eq!(host.ui.state.log.iter().filter(|line| *line == "started").count(), 2);

    // Started again while it plays: the run ends first.
    host.ui.state.log.clear();
    host.ui.tree.cx().start_frames(id);
    host.frame();
    assert_eq!(host.ui.state.log, ["finished", "started"]);
    // AutoPlay turned off stops it, once (turned on it does not start: React's setter).
    host.ui.tree.get_mut(id).unwrap().set_auto_play(true);
    host.frame();
    assert!(gif(&host).player().is_playing());
    host.ui.tree.get_mut(id).unwrap().set_auto_play(false);
    host.frame();
    host.frame();
    assert!(!gif(&host).player().is_playing());
    assert_eq!(host.ui.state.log, ["finished", "started", "finished"]);
}

/// Plays the clock for `ms` as a real host would: a frame 16 ms after one that asked for more,
/// else one when something wakes. Returns how many frames were drawn.
fn run_clock(host: &mut Headless<App>, ms: f64) -> usize {
    let end = host.time_ms() + ms;
    let mut frames = 0;
    while host.time_ms() < end {
        let next = match (host.ui.needs_frame(), host.ui.wake_at()) {
            (true, _) => host.time_ms() + 16.0,
            (false, Some(wake)) => wake,
            (false, None) => end,
        };
        if next > end {
            break;
        }
        host.frame_after(next - host.time_ms());
        frames += 1;
    }
    frames
}

#[test]
fn a_playing_gif_asks_for_frames_only_when_its_picture_changes_and_allocates_nothing() {
    let mut host = host(|gif| gif.repeat(-1));
    host.deliver_images(banana);
    started(&mut host);
    // One run to warm up: every frame drawn once.
    run_clock(&mut host, 800.0);
    let before = ALLOCATIONS.with(|a| a.get());
    let started = Instant::now();
    let frames = run_clock(&mut host, 1600.0);
    let spent = started.elapsed().as_secs_f64() * 1000.0;
    let allocations = ALLOCATIONS.with(|a| a.get()) - before;
    eprintln!("1600 ms of a looping 8 x 100 ms gif: {frames} frames, {:.3} ms each, {allocations} allocations", spent / frames as f64);
    // Two frames per picture change (every 100 ms), not 100 frames at 60 per second: the
    // ImageDoubleBuffered cache (DrawnUI's default) sends the new picture to be made, the bitmap
    // shows in the frame after.
    assert!((30..=32).contains(&frames), "{frames} frames");
    assert_eq!(allocations, 0);
    // The frames shown keep time.
    assert!(gif(&host).player().is_playing());
}

#[test]
fn start_before_the_frames_are_there_plays_when_they_arrive() {
    let mut host = host(|gif| gif.auto_play(false));
    assert!(!gif(&host).player().is_playing());
    host.ui.tree.cx().start_frames(host.ui.state.gif);
    host.settle();
    assert!(!gif(&host).player().is_playing());
    host.deliver_images(banana);
    started(&mut host);
    assert!(gif(&host).player().is_playing());
}

#[test]
fn a_still_image_of_the_same_file_shares_the_load_and_shows_the_first_frame() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((
            SkiaGif::new("banana.gif").width_request(140).height_request(140).assign(&mut app.gif),
            SkiaImage::new("banana.gif").width_request(40).height_request(40).margin((150, 0, 0, 0)),
        ))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 200, 200, 1.0);
    host.settle();
    let asked = host.deliver_images(banana);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].frames);
    host.settle();
    assert!(gif(&host).animation().is_some());
    assert_eq!(host.ui.tree.images.get("banana.gif").map(|image| image.width()), Some(365));
    // Frames and the first frame are counted once each in the cache.
    assert_eq!(host.ui.tree.images.memory_bytes(), 9 * 365 * 360 * 4);
}

#[test]
fn a_failed_gif_reports_an_error_and_a_gif_measures_like_an_image() {
    let mut host = host(|gif| gif);
    host.deliver_images(|_| None);
    host.settle();
    assert!(gif(&host).has_error() && !gif(&host).is_loading());
    assert_eq!(host.ui.state.log, ["error banana.gif"]);

    // Auto-sized in a vertical scroll: the width, and the height by the frames' aspect.
    let ui = Ui::new(App::default(), |app: &mut App| {
        let gif = SkiaGif::new("banana.gif").aspect(TransformAspect::AspectFit).assign(&mut app.gif);
        let scroll: Detached = SkiaScroll::new().fill().content(gif).into();
        scroll
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 365, 500, 1.0);
    host.settle();
    host.deliver_images(banana);
    host.settle();
    assert_eq!(host.rect(host.ui.state.gif).size(), Size::new(365.0, 360.0));
}
