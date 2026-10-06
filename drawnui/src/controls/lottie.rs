//! SkiaLottie: plays a Lottie (bodymovin JSON) animation (DrawnUI SkiaLottie, React
//! `SkiaLottie.ts` + `AnimatedFramesRenderer.ts`). The file comes through the host's asset
//! channel, once per source however many controls show it; each source and set of replaced colors
//! is parsed once and shared. The React range animator (RangeAnimator over the in point to the out
//! point) runs on a frame animator; the frame is drawn by the renderer in `crate::lottie` and kept
//! in the control's cache while it does not change (`DEFAULT_CACHE`).
// ponytail: no ProcessJson hook, no LoadSource returning an animation for the app; the caches by
// source live as long as the thread (C# CachedAnimations is static too).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use skia_safe::Color;

use crate::animators::{self, FrameTick};
use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::image::Loaded;
use crate::lottie::Animation;
use crate::props;
use crate::tree::{Build, ControlId, Cx, Handle, Mut, Tree, wrong_state};
use crate::types::{CacheType, Dirty, IntoProp};

props!(LottieProps, LottieBuild, LottieSet {
    /// Path or URL of the Lottie JSON, loaded by the host. Empty keeps what is shown.
    source / set_source: String = String::new(), APPLY;
    /// Replaces every color of the animation, alpha kept; transparent = its own colors.
    color_tint / set_color_tint: Color = Color::TRANSPARENT, APPLY;
    /// Replaces the distinct colors in the order they appear; the last one covers the rest.
    /// Wins over `color_tint`.
    colors / set_colors: Vec<Color> = Vec::new(), APPLY;
    /// Plays as soon as the animation is there and the control is placed; false stops it.
    auto_play / set_auto_play: bool = true, APPLY;
    /// Runs after the first: > 0 that many more, < 0 forever.
    repeat / set_repeat: i32 = 0, APPLY;
    /// Playback speed. Below 1 a run takes `1 + speed_ratio` times its length, as upstream.
    speed_ratio / set_speed_ratio: f32 = 1.0, NONE;
    /// Frame shown while stopped; -1 = the last one.
    default_frame / set_default_frame: i32 = 0, APPLY;
    /// A toggle: while stopped and on, `default_frame_when_on` shows instead (animated checkboxes).
    is_on / set_is_on: bool = false, APPLY;
    /// Frame shown while stopped and `is_on`; -1 = the last one.
    default_frame_when_on / set_default_frame_when_on: i32 = 0, APPLY;
    /// Stop and the end of the last run keep the frame on screen instead of the default frame.
    stop_at_current_frame / set_stop_at_current_frame: bool = false, NONE;
    /// Changing `is_on` while stopped shows the matching default frame.
    apply_is_on_when_not_playing / set_apply_is_on_when_not_playing: bool = true, NONE;
});

/// `handler(app state, cx)`.
type Handler = Box<dyn FnMut(&mut dyn Any, &mut Cx<'_>)>;

/// What the handlers hear on the next tick of the control's frame animator: they need the app
/// state, which the code that started or stopped playing may be holding.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Event {
    Started,
    Finished,
    Success,
    Error,
}

/// Where an animation comes from, with the colors replaced in it.
#[derive(Clone, PartialEq)]
struct Request {
    source: String,
    /// Inline JSON (`json`): wins over `source`. Compared by identity.
    json: Option<RcStr>,
    tints: Vec<Color>,
}

/// An `Rc<str>` equal only to itself: the same text set again is not parsed again.
#[derive(Clone)]
struct RcStr(Rc<str>);

impl PartialEq for RcStr {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Plays a Lottie animation from a file or JSON text (DrawnUI SkiaLottie).
pub struct SkiaLottie {
    pub p: LottieProps,
    /// The props the control last acted on (`None` before its first `on_props_changed`).
    applied: Option<LottieProps>,
    id: ControlId,
    json: Option<RcStr>,
    resolved: Option<Request>,
    animation: Option<Rc<Animation>>,
    loading: bool,
    error: bool,
    /// The frame shown, from the in point; fractional while playing.
    frame: f32,
    playing: bool,
    /// React `Animator` exists: the control was placed or an animation was set.
    ready: bool,
    /// Arranged once (React `wasLayout`).
    placed: bool,
    /// Start was asked before the animator existed.
    delayed_play: bool,
    /// Start was asked before the control could play: it plays as soon as it can.
    play_when_available: bool,
    /// The default frame was asked for before the animator existed.
    need_seek: bool,
    /// Frame time the current run began at; set by the first tick of the run.
    started_ms: Option<f64>,
    repeats_left: i32,
    /// A frame animator is registered for the control.
    ticking: bool,
    /// The frame changed: the cache records again.
    redraw: bool,
    events: Vec<Event>,
    on_started: Option<Handler>,
    on_finished: Option<Handler>,
    on_success: Option<Loaded>,
    on_error: Option<Loaded>,
}

/// DrawnUI's ImageDoubleBuffered where workers make the bitmap (the desktop). In the browser there
/// are none: a cache made in the frame would be one more offscreen pass per frame for an animation
/// that changes every frame, so it replays its recorded picture (PARITY.md, SkiaLottie).
const DEFAULT_CACHE: CacheType = if cfg!(target_os = "emscripten") { CacheType::Operations } else { CacheType::ImageDoubleBuffered };

impl SkiaLottie {
    /// A Lottie from a path or URL (`source`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaLottie> {
        let mut build = Build::new(SkiaLottie {
            p: LottieProps::default(),
            applied: None,
            id: ControlId { index: 0, generation: 0 },
            json: None,
            resolved: None,
            animation: None,
            loading: false,
            error: false,
            frame: 0.0,
            playing: false,
            ready: false,
            placed: false,
            delayed_play: false,
            play_when_available: false,
            need_seek: false,
            started_ms: None,
            repeats_left: 0,
            ticking: false,
            redraw: false,
            events: Vec::new(),
            on_started: None,
            on_finished: None,
            on_success: None,
            on_error: None,
        });
        let id = build.id();
        build.control_mut().id = id;
        build.source(source).use_cache(DEFAULT_CACHE)
    }

    /// Frames from the in point to the out point, rounded (React `TotalFrames`); 0 until loaded.
    pub fn total_frames(&self) -> f32 {
        self.animation.as_ref().map_or(0.0, |a| a.total_frames())
    }

    /// The frame on screen, from the in point.
    pub fn frame(&self) -> f32 {
        self.frame
    }

    /// Frames per second of the loaded animation; 0 until loaded.
    pub fn fps(&self) -> f32 {
        self.animation.as_ref().map_or(0.0, |a| a.fps)
    }

    /// Size of the composition in its own units; zero until loaded.
    pub fn composition_size(&self) -> (f32, f32) {
        self.animation.as_ref().map_or((0.0, 0.0), |a| (a.width, a.height))
    }

    /// The range animator runs (React `IsPlaying`).
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The file of the current source is on its way.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// The current source could not be loaded or parsed.
    pub fn has_error(&self) -> bool {
        self.error
    }

    fn tints(&self) -> Vec<Color> {
        if !self.p.colors.is_empty() {
            self.p.colors.clone()
        } else if self.p.color_tint.a() > 0 {
            vec![self.p.color_tint]
        } else {
            Vec::new()
        }
    }

    // ---- playback, as React AnimatedFramesRenderer + SkiaLottie

    fn can_play(&self) -> bool {
        self.placed && self.animation.is_some()
    }

    /// React `Start`: from the first frame, `repeat` times more.
    fn start(&mut self) {
        if !self.ready {
            self.delayed_play = true;
            return;
        }
        self.stop_animator();
        if self.can_play() {
            self.repeats_left = self.p.repeat;
            (self.playing, self.started_ms) = (true, None);
            self.events.push(Event::Started);
        }
        if !self.playing {
            self.play_when_available = true;
        }
    }

    /// React `Animator.Stop`: Finished runs when it was playing, and the default frame shows.
    fn stop_animator(&mut self) {
        if !self.playing {
            return;
        }
        (self.playing, self.started_ms) = (false, None);
        self.events.push(Event::Finished);
        if !self.p.stop_at_current_frame {
            self.seek_default();
        }
    }

    /// React `Stop`.
    fn stop(&mut self) {
        (self.play_when_available, self.delayed_play) = (false, false);
        if self.ready {
            self.stop_animator();
        }
        if !self.p.stop_at_current_frame {
            self.seek_default();
        }
    }

    fn seek_default(&mut self) {
        if !self.ready {
            self.need_seek = true;
            return;
        }
        let frame = if self.p.is_on { self.p.default_frame_when_on } else { self.p.default_frame };
        self.seek(frame as f32);
    }

    /// React `OnAnimatorSeeking`: -1 (any negative) is the out point.
    fn seek(&mut self, frame: f32) {
        let Some(animation) = &self.animation else { return };
        let frame = if frame < 0.0 { animation.total_frames() } else { frame };
        if frame != self.frame {
            (self.frame, self.redraw) = (frame, true);
        }
    }

    /// React `InitializeAnimator` (the animator exists from now on).
    fn initialize(&mut self) {
        self.ready = true;
        self.repeats_left = self.p.repeat;
        if self.delayed_play || (self.p.auto_play && self.animation.is_some()) {
            self.delayed_play = false;
            self.start();
        }
    }

    /// React `PlayIfNeeded`.
    fn play_if_needed(&mut self) {
        if self.play_when_available && self.ready {
            self.play_when_available = false;
            if !self.playing {
                self.start();
            }
        }
    }

    /// React `OnLayoutChanged`.
    fn placed(&mut self) {
        if !self.placed {
            self.placed = true;
            self.initialize();
        }
        self.play_if_needed();
        if std::mem::take(&mut self.need_seek) && !self.playing {
            self.seek_default();
        }
    }

    /// React `SetAnimation`. Success runs after it, when `total_frames` is known (React runs it
    /// just before, when it still reads 0).
    fn set_animation(&mut self, animation: Rc<Animation>) {
        (self.loading, self.error) = (false, false);
        self.events.push(Event::Success);
        if self.animation.as_ref().is_some_and(|a| Rc::ptr_eq(a, &animation)) {
            return;
        }
        let was_playing = self.playing;
        if was_playing {
            self.stop();
        }
        self.animation = Some(animation);
        self.initialize();
        let frame = if self.p.is_on { self.p.default_frame_when_on } else { self.p.default_frame };
        self.seek(frame as f32);
        if was_playing && !self.playing {
            self.start();
        }
        self.play_if_needed();
        self.redraw = true;
    }

    fn failed(&mut self, why: &str) {
        eprintln!("drawnui: lottie {} did not load: {why}", self.p.source);
        (self.loading, self.error) = (false, true);
        self.events.push(Event::Error);
    }

    /// One frame of the range animator (React SkiaValueAnimator over 0..TotalFrames, linear).
    fn advance(&mut self, time_ms: f64) {
        let Some(animation) = self.animation.as_ref().filter(|_| self.playing) else { return };
        let total = animation.total_frames();
        let length_ms = (animation.out_point - animation.in_point) / animation.fps * 1000.0;
        let ratio = self.p.speed_ratio;
        let run_ms = if ratio < 1.0 { length_ms * (1.0 + ratio) } else { length_ms / ratio } as f64;
        let start = *self.started_ms.get_or_insert(time_ms);
        let progress = if run_ms > 0.0 { (time_ms - start) / run_ms } else { 1.0 };
        let value = total * progress.min(1.0) as f32;
        let done = value >= total || progress >= 1.0;
        let frame = if done { total } else { value };
        if frame != self.frame {
            (self.frame, self.redraw) = (frame, true);
        }
        if !done {
            return;
        }
        if self.repeats_left != 0 {
            self.repeats_left -= (self.repeats_left > 0) as i32;
            // The next run starts on the next frame, from the first frame.
            self.started_ms = None;
        } else {
            self.stop_animator();
        }
    }

    // ---- loading

    /// Loads what the props ask for, unless it is what is shown or loading.
    fn resolve(&mut self, tree: &mut Tree) {
        let request = Request { source: self.p.source.clone(), json: self.json.clone(), tints: self.tints() };
        if self.resolved.as_ref() == Some(&request) || (request.json.is_none() && request.source.is_empty()) {
            return;
        }
        if let Some(json) = &request.json {
            match Animation::parse(json.0.as_bytes(), &request.tints) {
                Ok(animation) => self.set_animation(Rc::new(animation)),
                Err(why) => self.failed(&why),
            }
            self.resolved = Some(request);
            return;
        }
        match LOADS.with_borrow_mut(|loads| loads.lookup(&request.source, &request.tints, Some(self.id))) {
            Lookup::Ready(animation) => self.set_animation(animation),
            Lookup::Invalid(why) => self.failed(&why),
            Lookup::Waiting { fetch } => {
                self.loading = true;
                if fetch {
                    let url = request.source.clone();
                    tree.assets.fetch(&request.source, move |tree, bytes| delivered(tree, &url, bytes));
                }
            }
            Lookup::Failed => self.failed("no file"),
        }
        self.resolved = Some(request);
    }

    /// The file of a request arrived (or failed).
    fn arrived(&mut self, url: &str) {
        let Some(request) = self.resolved.as_ref().filter(|r| r.json.is_none() && r.source == url) else { return };
        let tints = request.tints.clone();
        match LOADS.with_borrow_mut(|loads| loads.lookup(url, &tints, None)) {
            Lookup::Ready(animation) => self.set_animation(animation),
            Lookup::Invalid(why) => self.failed(&why),
            Lookup::Waiting { .. } | Lookup::Failed => self.failed("no file"),
        }
    }

    /// What the tree has to do after a change: record the cache again, register the ticker
    /// that plays and delivers the events. Returns (redraw, register).
    fn take_work(&mut self) -> (bool, bool) {
        let register = (self.playing || !self.events.is_empty()) && !self.ticking;
        self.ticking |= register;
        (std::mem::take(&mut self.redraw), register)
    }
}

// ---------------------------------------------------------------- loads

/// A file by url: its bytes once loaded (`None` while in flight), the controls waiting for it.
struct File {
    bytes: Option<Rc<[u8]>>,
    waiting: Vec<ControlId>,
}

#[derive(Default)]
struct Loads {
    files: HashMap<String, File>,
    /// Parsed animations by url and replaced colors.
    parsed: HashMap<(String, Vec<u32>), Rc<Animation>>,
}

enum Lookup {
    Ready(Rc<Animation>),
    Invalid(String),
    /// In flight; `fetch`: nobody asked the host yet.
    Waiting { fetch: bool },
    /// Not loaded and nobody is loading it (a failure is not kept, as upstream).
    Failed,
}

impl Loads {
    /// The animation of `url` with `tints`. `waiter` waits for the file when it is not there
    /// (and makes it load); `None` only looks.
    fn lookup(&mut self, url: &str, tints: &[Color], waiter: Option<ControlId>) -> Lookup {
        let key = (url.to_owned(), tints.iter().map(|c| u32::from_be_bytes([c.a(), c.r(), c.g(), c.b()])).collect());
        if let Some(animation) = self.parsed.get(&key) {
            return Lookup::Ready(animation.clone());
        }
        match self.files.get_mut(url) {
            Some(File { bytes: Some(bytes), .. }) => match Animation::parse(bytes, tints) {
                Ok(animation) => {
                    let animation = Rc::new(animation);
                    self.parsed.insert(key, animation.clone());
                    Lookup::Ready(animation)
                }
                Err(why) => Lookup::Invalid(why),
            },
            Some(File { waiting, .. }) => {
                waiting.extend(waiter.filter(|w| !waiting.contains(w)));
                Lookup::Waiting { fetch: false }
            }
            None => match waiter {
                Some(w) => {
                    self.files.insert(url.to_owned(), File { bytes: None, waiting: vec![w] });
                    Lookup::Waiting { fetch: true }
                }
                None => Lookup::Failed,
            },
        }
    }
}

thread_local! {
    // ponytail: one cache per thread, like svg.rs; two trees on one thread share it.
    static LOADS: RefCell<Loads> = RefCell::new(Loads::default());
}

/// The host answered for `url` (empty bytes = failed): every control waiting for it gets it.
fn delivered(tree: &mut Tree, url: &str, bytes: Vec<u8>) {
    let waiting = LOADS.with_borrow_mut(|loads| {
        let file = loads.files.get_mut(url)?;
        let waiting = std::mem::take(&mut file.waiting);
        if bytes.is_empty() {
            loads.files.remove(url);
        } else {
            file.bytes = Some(Rc::from(bytes));
        }
        Some(waiting)
    });
    for id in waiting.unwrap_or_default() {
        let Some(mut me) = tree.find_mut::<SkiaLottie>(id) else { continue };
        me.control_mut().arrived(url);
        let (redraw, register) = me.control_mut().take_work();
        if redraw {
            me.mark(Dirty::DRAW);
        }
        if register {
            animators::start_frame(tree, id, tick);
        }
    }
}

/// One frame of the control's frame animator: the frame moves, then the handlers of what
/// happened run. It ends when nothing plays and no event waits.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut result = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<SkiaLottie>(id) else { return result };
    let lottie = me.control_mut();
    lottie.advance(time_ms);
    let mut events = std::mem::take(&mut lottie.events);
    let source = if events.iter().any(|e| matches!(e, Event::Success | Event::Error)) { lottie.p.source.clone() } else { String::new() };
    if std::mem::take(&mut lottie.redraw) {
        me.mark(Dirty::DRAW);
    }
    // Handlers leave the control while they run: they reach it through the tree.
    for event in events.drain(..) {
        let Some(mut me) = cx.tree.find_mut::<SkiaLottie>(id) else { return result };
        let lottie = me.control_mut();
        match event {
            Event::Started | Event::Finished => {
                let slot = if event == Event::Started { &mut lottie.on_started } else { &mut lottie.on_finished };
                let Some(mut handler) = slot.take() else { continue };
                handler(state, cx);
                if let Some(mut me) = cx.tree.find_mut::<SkiaLottie>(id) {
                    let lottie = me.control_mut();
                    let slot = if event == Event::Started { &mut lottie.on_started } else { &mut lottie.on_finished };
                    slot.get_or_insert(handler);
                }
            }
            Event::Success | Event::Error => {
                let slot = if event == Event::Success { &mut lottie.on_success } else { &mut lottie.on_error };
                let Some(mut handler) = slot.take() else { continue };
                handler(state, cx, &source);
                if let Some(mut me) = cx.tree.find_mut::<SkiaLottie>(id) {
                    let lottie = me.control_mut();
                    let slot = if event == Event::Success { &mut lottie.on_success } else { &mut lottie.on_error };
                    slot.get_or_insert(handler);
                }
            }
        }
        result.state_touched = true;
    }
    let Some(mut me) = cx.tree.find_mut::<SkiaLottie>(id) else { return result };
    let lottie = me.control_mut();
    // Events a handler caused wait for the next tick; the list keeps its capacity otherwise.
    if lottie.events.is_empty() {
        lottie.events = events;
    }
    result.keep = lottie.playing || !lottie.events.is_empty();
    lottie.ticking = result.keep;
    // A handler may have moved the frame (a seek through `Mut`).
    if std::mem::take(&mut lottie.redraw) {
        me.mark(Dirty::DRAW);
    }
    result
}

impl Has<LottieProps> for SkiaLottie {
    fn part(&self) -> &LottieProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut LottieProps {
        &mut self.p
    }
}

impl Control for SkiaLottie {
    /// What changed since the last time, as the React setters react to it; then the source loads.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if let Some(old) = self.applied.take() {
            let p = self.p.clone();
            if !p.auto_play && old.auto_play {
                self.stop();
            }
            if p.repeat != old.repeat && self.ready {
                self.repeats_left = p.repeat;
            }
            if p.default_frame != old.default_frame && !self.playing && self.ready && !p.is_on {
                self.seek(p.default_frame as f32);
            }
            if p.is_on != old.is_on && self.ready {
                if self.playing || !p.is_on {
                    self.stop();
                } else if p.apply_is_on_when_not_playing {
                    self.seek_default();
                }
            }
            if p.default_frame_when_on != old.default_frame_when_on && !self.playing && self.ready && p.is_on {
                self.seek(p.default_frame_when_on as f32);
            }
        }
        self.applied = Some(self.p.clone());
        self.resolve(cx.tree);
        let (redraw, register) = self.take_work();
        if redraw {
            cx.tree.invalidate(self.id, Dirty::DRAW);
        }
        if register {
            animators::start_frame(cx.tree, self.id, tick);
        }
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.placed();
        let (redraw, register) = self.take_work();
        if redraw {
            cx.tree.invalidate(cx.id, Dirty::DRAW);
        }
        if register {
            animators::start_frame(cx.tree, cx.id, tick);
        }
    }

    /// The frame, scaled to fit the drawing rect and centered (Skottie `render(canvas, dst)`).
    fn paint(&self, cx: &mut PaintCx) {
        if let Some(animation) = &self.animation {
            animation.render(cx.canvas, cx.rect, self.frame);
        }
    }
}

impl Mut<'_, SkiaLottie> {
    /// Plays from the first frame, `repeat` times more (DrawnUI Start). Before the animation is
    /// there or the control is placed, it plays as soon as it can.
    pub fn start(&mut self) {
        self.control_mut().start();
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Stops; Finished runs when it was playing, then the default frame shows unless
    /// `stop_at_current_frame` (DrawnUI Stop).
    pub fn stop(&mut self) {
        self.control_mut().stop();
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Shows a frame, from the in point; negative = the last one (DrawnUI Seek). A playing
    /// animation goes on from its own clock.
    pub fn seek(&mut self, frame: impl IntoProp<f32>) {
        self.control_mut().seek(frame.into_prop());
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Shows the first frame (DrawnUI GoToStart).
    pub fn go_to_start(&mut self) {
        self.seek(0.0);
    }

    /// Shows the last frame (DrawnUI GoToEnd).
    pub fn go_to_end(&mut self) {
        self.seek(-1.0);
    }

    /// Shows an animation from JSON text instead of `source` (parsed on the next frame).
    pub fn set_json(&mut self, json: &str) {
        self.control_mut().json = Some(RcStr(Rc::from(json)));
        self.mark(Dirty::APPLY);
    }
}

fn handler<S: Any>(me: Handle<SkiaLottie>, mut f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>) + 'static) -> Handler {
    Box::new(move |state, cx| f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx))
}

fn loaded<S: Any>(me: Handle<SkiaLottie>, mut f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>, &str) + 'static) -> Loaded {
    Box::new(move |state, cx, source| f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx, source))
}

impl Build<SkiaLottie> {
    /// The animation from JSON text instead of `source` (an `include_str!` file).
    pub fn json(mut self, json: &str) -> Self {
        self.control_mut().json = Some(RcStr(Rc::from(json)));
        self
    }

    /// Runs when playing starts (DrawnUI Started), on the next frame.
    pub fn on_started<S: Any>(mut self, f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().on_started = Some(handler(me, f));
        self
    }

    /// Runs when playing ends: after the last run, and when it is stopped while playing (DrawnUI
    /// Finished, raised by the animator's OnStop).
    pub fn on_finished<S: Any>(mut self, f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().on_finished = Some(handler(me, f));
        self
    }

    /// Runs when the animation of the current source is set: `total_frames` is known then
    /// (React Success). The last argument is the source.
    pub fn on_success<S: Any>(mut self, f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().on_success = Some(loaded(me, f));
        self
    }

    /// Runs when the source could not be loaded or parsed (React Error).
    pub fn on_error<S: Any>(mut self, f: impl FnMut(Handle<SkiaLottie>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().on_error = Some(loaded(me, f));
        self
    }
}
