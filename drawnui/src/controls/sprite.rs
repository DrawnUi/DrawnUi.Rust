//! SkiaSprite: plays a sprite sheet, `columns` x `rows` frames cut from one bitmap, at
//! `frames_per_second` (DrawnUI SkiaSprite, React `SkiaSprite.ts`); SkiaSpriteSet: one sprite per
//! state, the active one shown and playing. The sheet comes whole through the image manager like
//! any picture (one load per source, cached there): the sprite embeds a `SkiaImage` for that.
//! Frames step on a frame animator that sleeps until the next frame is due: no frames in between,
//! nothing allocated per step.
// ponytail: no SpritePlacementConfig, no Started / Finished handlers, no hit box that follows the
// drawn frame, no trimming of transparent frame borders (upstream draws the trimmed part at its
// place in the fitted frame, so the pixels are the same; only the placement config and the hit
// box use the trimmed rect).

use std::any::Any;
use std::collections::HashMap;
use std::sync::Mutex;

use skia_safe::{FilterMode, IRect, MipmapMode, Paint, Rect, SamplingOptions, Size, canvas::SrcRectConstraint};

use crate::animators::{self, FrameTick};
use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::image::{SkiaImage, TransformAspect};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::props;
use crate::tree::{Build, Container, ControlId, ControlProps, Cx, Handle, Mut, Tree, wrong_state};
use crate::types::{CacheType, Dirty, IntoProp, LayoutOptions};

props!(SpriteProps, SpriteBuild, SpriteSet {
    /// The sheet: a path or URL, loaded whole by the host through the image manager.
    source / set_source: String = String::new(), MEASURE_APPLY;
    /// Frames across the sheet.
    columns / set_columns: i32 = 1, MEASURE_APPLY;
    /// Frames down the sheet.
    rows / set_rows: i32 = 1, MEASURE_APPLY;
    /// Frames to play from the sheet, row by row; 0 = all of them.
    max_frames / set_max_frames: i32 = 0, DRAW_APPLY;
    /// Frames shown per second.
    frames_per_second / set_frames_per_second: f32 = 24.0, DRAW_APPLY;
    /// Playback speed. Below 1 a run takes `1 + speed_ratio` times its length, as upstream.
    speed_ratio / set_speed_ratio: f32 = 1.0, DRAW_APPLY;
    /// Sheet frames to play, in this order, instead of all of them.
    frame_sequence / set_frame_sequence: Vec<u32> = Vec::new(), DRAW_APPLY;
    /// A sequence registered with `SkiaSprite::create_animation_sequence`; wins over `frame_sequence`.
    animation_name / set_animation_name: String = String::new(), DRAW_APPLY;
    /// Frame shown when the sheet arrives; -1 = the last one.
    default_frame / set_default_frame: i32 = 0, NONE;
    /// Plays as soon as the sheet is there.
    auto_play / set_auto_play: bool = true, NONE;
    /// Runs after the first: > 0 that many more, < 0 forever, 0 none.
    repeat / set_repeat: i32 = 0, NONE;
});

/// Named frame sequences (C# RegisteredAnimations).
static REGISTERED: Mutex<Option<HashMap<String, Vec<u32>>>> = Mutex::new(None);

/// What the frame animator reads. A position is milliseconds into a run at speed 1 (upstream's
/// animator value), a played frame indexes the sequence played.
#[derive(Default)]
struct Playback {
    /// Pixel size of one frame of the sheet.
    frame: (i32, i32),
    /// Frames played per run (the sequence's length, else the sheet's frames).
    total: u32,
    /// Sheet frame per played frame; empty = played frame = sheet frame.
    sequence: Vec<u32>,
    /// The played frame on screen.
    current: u32,
    /// Where it is in the sheet.
    src: IRect,
    /// True between the start and the end of the last run or `stop`.
    playing: bool,
    /// Plays (again) from `offset` at the next chance: `start`, `seek` while playing, a sheet
    /// that is not there yet.
    pending: bool,
    /// A frame animator is registered.
    ticking: bool,
    /// Frame time playback started at.
    started_ms: f64,
    /// Position playback started from.
    offset: f64,
}

/// Plays a sprite sheet (DrawnUI SkiaSprite).
pub struct SkiaSprite {
    /// Loads and holds the sheet; the manager delivers into it (`inner`) and runs the app's
    /// success / error handlers from its slots.
    image: SkiaImage,
    /// Its own properties.
    pub p: SpriteProps,
    /// For the frame animator: `on_props_changed` gets no id.
    id: Option<ControlId>,
    play: Playback,
    paint: Paint,
}

impl SkiaSprite {
    /// A sprite of the sheet `source`. No cache, as upstream: a frame is one bitmap blit.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaSprite> {
        // Every pixel of the sheet, whatever the box: frames are cut from it. Measured again when
        // it arrives: the frame size is the control's aspect.
        let mut image = SkiaImage::default();
        (image.remeasure_on_arrival, image.p.aspect) = (true, TransformAspect::None);
        let sprite = SkiaSprite { image, p: SpriteProps::default(), id: None, play: Playback::default(), paint: Paint::default() };
        let mut build = Build::new(sprite).source(source);
        let id = build.id();
        build.control_mut().id = Some(id);
        build
    }

    /// Registers a frame sequence that `animation_name` can name (C# CreateAnimationSequence).
    pub fn create_animation_sequence(name: &str, frames: Vec<u32>) {
        REGISTERED.lock().expect("sprite sequences").get_or_insert_default().insert(name.to_owned(), frames);
    }

    /// Frames played per run; 0 until the sheet is there.
    pub fn total_frames(&self) -> u32 {
        self.play.total
    }

    /// Pixel size of one frame of the sheet.
    pub fn frame_size(&self) -> (i32, i32) {
        self.play.frame
    }

    /// Milliseconds a frame shows at speed 1.
    pub fn frame_duration_ms(&self) -> f32 {
        1000.0 / if self.p.frames_per_second > 0.0 { self.p.frames_per_second } else { 24.0 }
    }

    /// Milliseconds a run takes at speed 1.
    pub fn duration_ms(&self) -> f32 {
        self.play.total as f32 * self.frame_duration_ms()
    }

    /// The played frame on screen.
    pub fn current_frame(&self) -> u32 {
        self.play.current
    }

    /// Between the start and the end of the last run or `stop`.
    pub fn is_playing(&self) -> bool {
        self.play.playing
    }

    /// The sheet is on its way.
    pub fn is_loading(&self) -> bool {
        self.image.is_loading()
    }

    /// The sheet could not be loaded.
    pub fn has_error(&self) -> bool {
        self.image.has_error()
    }

    /// Milliseconds a run takes (upstream ApplySpeed).
    fn run_ms(&self) -> f64 {
        let (duration, ratio) = (self.duration_ms() as f64, self.p.speed_ratio as f64);
        if ratio < 1.0 { duration * (1.0 + ratio) } else { duration / ratio }
    }

    fn position_at(&self, time_ms: f64) -> f64 {
        self.play.offset + (time_ms - self.play.started_ms) / self.run_ms() * self.duration_ms() as f64
    }

    fn time_of(&self, position: f64) -> f64 {
        self.play.started_ms + (position - self.play.offset) / self.duration_ms() as f64 * self.run_ms()
    }

    /// The played frame at a position (C# GetFrameNumberFromTime): negative counts from the end,
    /// past the end wraps.
    fn frame_at(&self, position: f64) -> u32 {
        let duration = self.duration_ms() as f64;
        if self.play.total == 0 || duration <= 0.0 {
            return 0;
        }
        let position = position.rem_euclid(duration);
        // The epsilon keeps a position computed back from a frame's start time on that frame.
        let frame = (position / self.frame_duration_ms() as f64 + 1e-6).floor() as u32;
        frame.min(self.play.total - 1)
    }

    /// Reads the sheet's size and the sequence (C# RecalculateFrames); 0 frames without a sheet.
    fn recalculate(&mut self) {
        let Some(sheet) = self.image.image() else {
            self.play.total = 0;
            return;
        };
        let (columns, rows) = (self.p.columns.max(1), self.p.rows.max(1));
        self.play.frame = (sheet.width() / columns, sheet.height() / rows);
        let in_sheet = (columns * rows) as u32;
        let in_sheet = if self.p.max_frames > 0 { in_sheet.min(self.p.max_frames as u32) } else { in_sheet };
        let named = (!self.p.animation_name.is_empty())
            .then(|| REGISTERED.lock().expect("sprite sequences").as_ref()?.get(&self.p.animation_name).cloned())
            .flatten();
        self.play.sequence = named.unwrap_or_else(|| self.p.frame_sequence.clone());
        self.play.total = if self.play.sequence.is_empty() { in_sheet } else { self.play.sequence.len() as u32 };
        self.show(self.play.current);
    }

    /// Makes a played frame the one drawn (C# SetCurrentFrame). True when it changed.
    fn show(&mut self, played: u32) -> bool {
        if self.play.total == 0 {
            return false;
        }
        let played = played.min(self.play.total - 1);
        let frame = self.play.sequence.get(played as usize).copied().unwrap_or(played);
        let ((fw, fh), columns) = (self.play.frame, self.p.columns.max(1) as u32);
        let src = IRect::from_xywh((frame % columns) as i32 * fw, (frame / columns) as i32 * fh, fw, fh);
        let changed = self.play.current != played || self.play.src != src;
        (self.play.current, self.play.src) = (played, src);
        changed
    }

    /// Plays from `offset`, starting now; the frame animator sleeps until the next frame is due.
    fn play(&mut self, tree: &mut Tree, id: ControlId) {
        (self.play.pending, self.play.playing, self.play.started_ms) = (false, true, tree.time_ms);
        self.show(self.frame_at(self.play.offset));
        if !std::mem::replace(&mut self.play.ticking, true) {
            animators::start_frame(tree, id, tick);
        }
        let next = self.next_frame(self.play.offset);
        animators::sleep(tree, id, next);
    }

    /// Frame time the frame after the one at `position` is due.
    fn next_frame(&self, position: f64) -> f64 {
        let frame_ms = self.frame_duration_ms() as f64;
        self.time_of(((position / frame_ms + 1e-6).floor() + 1.0) * frame_ms)
    }

    /// One frame of the clock: the frame to show, and when the next one is due (`None`: the
    /// last run is over). True when the frame changed.
    fn advance(&mut self, time_ms: f64) -> (bool, Option<f64>) {
        let (position, duration) = (self.position_at(time_ms), self.duration_ms() as f64);
        let run = (position / duration + 1e-9).floor();
        if self.p.repeat >= 0 && run > self.p.repeat as f64 {
            // As upstream the position stops at the end of the range, which is the first frame.
            (self.play.playing, self.play.offset) = (false, 0.0);
            return (self.show(self.frame_at(duration)), None);
        }
        (self.show(self.frame_at(position)), Some(self.next_frame(position)))
    }
}

/// The content size of a picture of `aspect` (width / height; 0 = not there yet) offered `width` x
/// `height` pixels, for SkiaSprite and SkiaSvg (React MeasureAbsolute). A side the layout takes
/// from the content (no size request, not Fill, no LockRatio) follows the other side by the
/// aspect, never beyond what is offered; two such sides fit the aspect into the box. Empty when a
/// side stays unbounded. React's stacks measure a child unbounded along the stack, where its rule
/// (an unbounded side follows the aspect, else the box) gives the same size; here a stack offers
/// what is left, so the rule looks at the requests.
pub(crate) fn picture_size(p: &ControlProps, aspect: f32, width: f32, height: f32) -> Size {
    let (mut w, mut h) = (width, height);
    if aspect > 0.0 {
        let locked = p.lock_ratio != 0.0;
        let auto_w = !w.is_finite() || !(locked || p.width_request >= 0.0 || p.horizontal_options == LayoutOptions::Fill);
        let auto_h = !h.is_finite() || !(locked || p.height_request >= 0.0 || p.vertical_options == LayoutOptions::Fill);
        match (auto_w, auto_h) {
            (true, false) => w = w.min(h * aspect),
            (false, true) => h = h.min(w / aspect),
            (true, true) if !h.is_finite() || (w.is_finite() && w / h <= aspect) => h = h.min(w / aspect),
            (true, true) => w = w.min(h * aspect),
            (false, false) => {}
        }
    }
    if !w.is_finite() || !h.is_finite() {
        return Size::default();
    }
    Size::new(w, h)
}

/// One tick of the frame animator: shows the frame the clock is at, then sleeps until the next
/// one is due. Ends with the last run, or when playback was stopped.
fn tick(id: ControlId, time_ms: f64, _state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut result = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<SkiaSprite>(id) else { return result };
    let sprite = me.control_mut();
    let (changed, next) = match sprite.play.playing && sprite.play.total > 0 {
        true => sprite.advance(time_ms),
        false => (false, None),
    };
    sprite.play.ticking = next.is_some();
    if changed {
        me.mark(Dirty::DRAW);
    }
    if let Some(next) = next {
        animators::sleep(cx.tree, id, next);
        result.keep = true;
    }
    result
}

impl Has<SpriteProps> for SkiaSprite {
    fn part(&self) -> &SpriteProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut SpriteProps {
        &mut self.p
    }
}

impl Mut<'_, SkiaSprite> {
    /// Plays from the first frame (C# Start). Before the sheet is there it plays when it arrives.
    pub fn start(&mut self) {
        let play = &mut self.control_mut().play;
        (play.pending, play.playing, play.offset) = (true, false, 0.0);
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Stops on the frame it shows (C# Stop).
    pub fn stop(&mut self) {
        let play = &mut self.control_mut().play;
        (play.pending, play.playing) = (false, false);
        self.mark(Dirty::DRAW);
    }

    /// Shows the frame at `ms` into a run (negative: from the end); playing goes on from there
    /// (C# Seek; upstream's playing animator overrides it at its next frame).
    pub fn seek(&mut self, ms: impl IntoProp<f32>) {
        let sprite = self.control_mut();
        let duration = sprite.duration_ms() as f64;
        let ms = ms.into_prop() as f64;
        sprite.play.offset = if duration > 0.0 { ms.rem_euclid(duration) } else { 0.0 };
        sprite.show(sprite.frame_at(sprite.play.offset));
        sprite.play.pending |= sprite.play.playing;
        self.mark(Dirty::DRAW_APPLY);
    }
}

impl Control for SkiaSprite {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.image)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.image)
    }

    /// The image loads what `source` names; the frames are cut again; playing starts, restarts
    /// or picks up the new timing.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if self.image.p.source != self.p.source {
            self.image.p.source.clone_from(&self.p.source);
            // A new sheet plays when it arrives if the old one was playing (C# SetSpriteSheet).
            let pending = self.play.pending || self.play.playing;
            self.play = Playback { pending, ticking: self.play.ticking, ..Playback::default() };
        }
        self.recalculate();
        let Some(id) = self.id else { return };
        if self.play.pending && self.play.total > 0 {
            self.play(cx.tree, id);
        } else if self.play.playing {
            // Another speed or length: the next frame is computed now.
            animators::sleep(cx.tree, id, 0.0);
        }
    }

    /// The image resolves the source. When the sheet is there the first time the frames are
    /// cut, the default frame shows and playing starts when asked for. Sized by the frame's
    /// aspect (`picture_size`).
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.image.measure(cx, width, height);
        if self.play.total == 0 && self.image.image().is_some() {
            self.recalculate();
            let last = self.play.total.saturating_sub(1);
            let first = if self.p.default_frame < 0 { last } else { (self.p.default_frame as u32).min(last) };
            self.show(first);
            if self.play.pending || self.p.auto_play {
                self.play(cx.tree, cx.id);
            }
        }
        let (fw, fh) = (self.play.frame.0 as f32, self.play.frame.1 as f32);
        let aspect = if self.play.total > 0 && fw > 0.0 && fh > 0.0 { fw / fh } else { 0.0 };
        picture_size(&cx.base().p, aspect, width, height)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.image.arrange(cx);
    }

    /// The current frame, AspectFit into the box and centered, nearest sampling (C# SpriteFrameImage).
    fn paint(&self, cx: &mut PaintCx) {
        let (Some(sheet), true) = (self.image.image(), self.play.total > 0) else { return };
        let (fw, fh) = (self.play.frame.0 as f32, self.play.frame.1 as f32);
        let dest = cx.rect;
        let fit = (dest.width() / fw).min(dest.height() / fh);
        let display = Rect::from_xywh(dest.center_x() - fw * fit / 2.0, dest.center_y() - fh * fit / 2.0, fw * fit, fh * fit);
        let source = Rect::from(self.play.src);
        let sampling = SamplingOptions::new(FilterMode::Nearest, MipmapMode::None);
        cx.canvas.draw_image_rect_with_sampling_options(
            sheet,
            Some((&source, SrcRectConstraint::Fast)),
            display,
            sampling,
            &self.paint,
        );
    }
}

impl Build<SkiaSprite> {
    /// Runs when the sheet is there (C# Success): `total_frames`, `frame_size` and `duration_ms`
    /// are known then.
    pub fn on_success<S: Any>(mut self, mut f: impl FnMut(Handle<SkiaSprite>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().image.on_success = Some(Box::new(move |state, cx, source| {
            f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx, source)
        }));
        self
    }

    /// Runs when the sheet could not be loaded (C# Error).
    pub fn on_error<S: Any>(mut self, mut f: impl FnMut(Handle<SkiaSprite>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().image.on_error = Some(Box::new(move |state, cx, source| {
            f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx, source)
        }));
        self
    }
}

// ---------------------------------------------------------------- SkiaSpriteSet

props!(SpriteSetProps, SpriteSetBuild, SpriteSetSet {
    /// Which of the defined sprites shows and plays. A state without a sprite keeps the one shown.
    state / set_state: i32 = 0, DRAW_APPLY;
});

/// One sprite per integer state, all loaded up front; `state` picks the one shown and playing,
/// the others are hidden and stopped (C# SkiaSpriteSet).
pub struct SkiaSpriteSet {
    layout: SkiaLayout,
    /// Its own properties.
    pub p: SpriteSetProps,
    sprites: Vec<(i32, Handle<SkiaSprite>)>,
    active: Option<Handle<SkiaSprite>>,
    /// The sheets of every state were handed to the image manager.
    preloaded: bool,
}

impl SkiaSpriteSet {
    /// An empty set; add sprites with `define`. Cached as Operations, as upstream.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaSpriteSet> {
        let set = SkiaSpriteSet { layout: SkiaLayout::default(), p: SpriteSetProps::default(), sprites: Vec::new(), active: None, preloaded: false };
        Build::new(set).use_cache(CacheType::Operations)
    }

    /// The sprite of the current state.
    pub fn current_sprite(&self) -> Option<Handle<SkiaSprite>> {
        self.active
    }
}

impl Build<SkiaSpriteSet> {
    /// Adds the sprite of a state (C# Define): `source` cut into `columns` x `rows` frames at
    /// `fps`, `repeat` runs after the first (-1 = forever), playing by itself when `auto_play`
    /// (upstream defaults: 15 fps, -1, true). It fills the set's box. Not cached: its frame changes
    /// at every step (upstream caches it as an image).
    pub fn define(
        mut self,
        state: i32,
        source: impl IntoProp<String>,
        columns: i32,
        rows: i32,
        fps: impl IntoProp<f32>,
        repeat: i32,
        auto_play: bool,
    ) -> Self {
        let set = self.control_mut();
        let shown = set.active.is_none() && state == set.p.state;
        let sprite = SkiaSprite::new(source)
            .columns(columns)
            .rows(rows)
            .frames_per_second(fps)
            .repeat(repeat)
            .auto_play(auto_play)
            .fill()
            // DrawnUI SkiaSpriteSet.Define: each sprite on a GPU cache.
            .use_cache(CacheType::GPU)
            .is_visible(shown);
        let handle = sprite.handle();
        set.sprites.push((state, handle));
        if shown {
            set.active = Some(handle);
        }
        self.push_child(sprite);
        self
    }
}

impl Has<SpriteSetProps> for SkiaSpriteSet {
    fn part(&self) -> &SpriteSetProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut SpriteSetProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaSpriteSet {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for SkiaSpriteSet {}

impl Control for SkiaSpriteSet {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The first time: every sheet goes to the image manager (a hidden sprite is not measured,
    /// upstream loads them all in `Define`). Another state: its sprite shows and plays from its
    /// first frame, the old one hides and stops (C# SetActive).
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if !std::mem::replace(&mut self.preloaded, true) {
            let sources: Vec<String> =
                self.sprites.iter().filter_map(|(_, sprite)| Some(cx.find::<SkiaSprite>(*sprite)?.p.source.clone())).collect();
            cx.tree.images.preload(&sources);
        }
        let wanted = self.sprites.iter().find(|(state, _)| *state == self.p.state).map(|(_, sprite)| *sprite);
        if let Some(wanted) = wanted
            && Some(wanted.id()) != self.active.map(Handle::id)
        {
            if let Some(mut old) = self.active.and_then(|sprite| cx.get_mut(sprite)) {
                old.stop();
                old.set_is_visible(false);
            }
            if let Some(mut new) = cx.get_mut(wanted) {
                new.set_is_visible(true);
                new.start();
            }
            self.active = Some(wanted);
        }
        self.layout.on_props_changed(cx);
    }
}
