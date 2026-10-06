//! Animators: values driven by the frame clock, the same family as DrawnUI AnimatorBase ->
//! SkiaValueAnimator -> RenderingAnimator (+ RippleAnimator). They live in the tree and hold
//! control ids, never references; each ticks once per frame, after input and before observers, on
//! the frame time the host passes. C# awaits an animation; here starting one returns an
//! `AnimationId`, and `Cx::on_finished` takes the code that would follow the await.

use std::any::Any;

use skia_safe::{Color, Paint};

use crate::control::{Control, PaintCx};
use crate::tree::{ControlId, Cx, Mut, Tree, wrong_state};
use crate::types::{Dirty, IntoProp};

/// Maps linear progress 0..1 to eased progress (MAUI Easing).
pub type Easing = fn(f32) -> f32;

/// The MAUI easing set (React `Easing.ts` has Linear, the cubics, SinIn, SinOut and Default).
pub mod easing {
    use std::f32::consts::FRAC_PI_2;

    /// No easing.
    pub fn linear(x: f32) -> f32 {
        x
    }
    /// Starts slow, speeds up.
    pub fn cubic_in(x: f32) -> f32 {
        x * x * x
    }
    /// Starts fast, slows down.
    pub fn cubic_out(x: f32) -> f32 {
        (x - 1.0).powi(3) + 1.0
    }
    /// Slow at both ends.
    pub fn cubic_in_out(x: f32) -> f32 {
        if x < 0.5 { 4.0 * x * x * x } else { (x - 1.0) * (2.0 * x - 2.0).powi(2) + 1.0 }
    }
    /// A quarter sine: starts fast, slows down.
    pub fn sin_out(x: f32) -> f32 {
        (x * FRAC_PI_2).sin()
    }
    /// A quarter sine: starts slow, speeds up.
    pub fn sin_in(x: f32) -> f32 {
        1.0 - (x * FRAC_PI_2).cos()
    }
    /// Half a cosine: slow at both ends.
    pub fn sin_in_out(x: f32) -> f32 {
        -(std::f32::consts::PI * x).cos() / 2.0 + 0.5
    }
    /// Leaps to the end, bounces three times and settles (MAUI `Easing.BounceOut`).
    pub fn bounce_out(mut p: f32) -> f32 {
        if p < 1.0 / 2.75 {
            return 7.5625 * p * p;
        }
        if p < 2.0 / 2.75 {
            p -= 1.5 / 2.75;
            return 7.5625 * p * p + 0.75;
        }
        if p < 2.5 / 2.75 {
            p -= 2.25 / 2.75;
            return 7.5625 * p * p + 0.9375;
        }
        p -= 2.625 / 2.75;
        7.5625 * p * p + 0.984375
    }
    /// Bounces three times, then leaps to the end (MAUI `Easing.BounceIn`).
    pub fn bounce_in(p: f32) -> f32 {
        1.0 - bounce_out(1.0 - p)
    }
    /// Moves away first, then leaps to the end (MAUI `Easing.SpringIn`).
    pub fn spring_in(x: f32) -> f32 {
        x * x * ((1.70158 + 1.0) * x - 1.70158)
    }
    /// Goes a little past the end and comes back (MAUI `Easing.SpringOut`).
    pub fn spring_out(x: f32) -> f32 {
        (x - 1.0) * (x - 1.0) * ((1.70158 + 1.0) * (x - 1.0) + 1.70158) + 1.0
    }
    /// DrawnUI `Easing.Default`.
    pub fn default(x: f32) -> f32 {
        cubic_in_out(x)
    }
}

/// What a value animator runs (DrawnUI SkiaValueAnimator: mMinValue, mMaxValue, Speed, Easing,
/// Repeat; `Start(delayMs)`; PingPongAnimator).
#[derive(Clone, Copy, Debug)]
pub struct ValueAnimator {
    /// Value at the start of a run.
    pub from: f32,
    /// Value at the end of a run.
    pub to: f32,
    /// Milliseconds for one run; 0 finishes on the first frame.
    pub duration_ms: f32,
    /// Shapes the value between the ends, as React: `from + (to - from) * easing(progress)`.
    pub easing: Easing,
    /// How many more times it runs after the first; -1 = forever.
    pub repeat: i32,
    /// Milliseconds before the first run; no frames are drawn for it while it waits.
    pub delay_ms: f32,
    /// Every repeat runs the other way (DrawnUI PingPongAnimator).
    pub ping_pong: bool,
}

impl ValueAnimator {
    /// One run from `from` to `to` over `duration_ms`.
    pub fn new(from: f32, to: f32, duration_ms: f32, easing: Easing) -> Self {
        Self { from, to, duration_ms, easing, repeat: 0, delay_ms: 0.0, ping_pong: false }
    }
    /// Runs after the first: > 0 that many more, -1 forever.
    pub fn repeat(mut self, repeat: i32) -> Self {
        self.repeat = repeat;
        self
    }
    /// Waits `ms` before the first run, without frames (DrawnUI `Start(delayMs)`).
    pub fn delay(mut self, ms: f32) -> Self {
        self.delay_ms = ms;
        self
    }
    /// Repeats run back and forth: `from` to `to`, then `to` to `from`.
    pub fn ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self
    }
}

/// A started animation. Stale once it finished or was stopped; using it then does nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AnimationId(u32);

/// The built-in property animations: one runs per kind per control.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fade,
    Scale,
    Translate,
    Rotate,
}

type Update = Box<dyn FnMut(f32, &mut Cx<'_>)>;
type Overlay = Box<dyn Fn(&mut PaintCx<'_>, f32)>;
type Finished = Box<dyn FnOnce(&mut dyn Any, &mut Cx<'_>)>;

/// What a frame animator answers: whether it goes on, and whether it ran app handlers.
pub(crate) struct FrameTick {
    pub keep: bool,
    pub state_touched: bool,
}

/// `frame(control, time_ms, app state, cx)`: a plain function, so starting one allocates nothing.
pub(crate) type Frame = fn(ControlId, f64, &mut dyn Any, &mut Cx<'_>) -> FrameTick;

pub(crate) struct Animator {
    /// Grows with every start, so the list in the tree stays sorted by it.
    id: u32,
    pub(crate) control: ControlId,
    kind: Option<Kind>,
    run: ValueAnimator,
    /// Frame time of the first tick of the current run.
    start_ms: Option<f64>,
    /// The value of the last tick; an overlay draws with it.
    pub(crate) value: f32,
    /// `None` for an overlay, and while it is being called.
    update: Option<Update>,
    /// Drawn above the control's content while the animator runs (DrawnUI RenderingAnimator).
    pub(crate) overlay: Option<Overlay>,
    finished: Option<Finished>,
    /// Set for a frame animator: it has no run of its own, the control keeps the motion.
    frame: Option<Frame>,
    /// Frame time before which it does not tick and asks for no frames; the host wakes up for it.
    wake_ms: f64,
    /// Frame time it was paused at and the wake time it had: the run, or the wait before it,
    /// goes on from there when resumed.
    paused: Option<(f64, f64)>,
}

fn start(
    tree: &mut Tree,
    control: ControlId,
    kind: Option<Kind>,
    run: ValueAnimator,
    update: Option<Update>,
    overlay: Option<Overlay>,
) -> AnimationId {
    tree.last_animation += 1;
    let id = tree.last_animation;
    let (start_ms, value, finished, frame, paused) = (None, run.from, None, None, None);
    let wake_ms = if run.delay_ms > 0.0 { tree.time_ms + run.delay_ms as f64 } else { 0.0 };
    tree.animators.push(Animator { id, control, kind, run, start_ms, value, update, overlay, finished, frame, wake_ms, paused });
    AnimationId(id)
}

/// Starts a frame animator: `frame` runs once per frame with the frame time and the app state
/// until it answers `keep: false` or its control is removed. For motion that is not one run from
/// a value to another (DrawnUI ScrollFlingAnimator, SpringWithVelocityAnimator): the control
/// keeps the physics and gets the clock here.
pub(crate) fn start_frame(tree: &mut Tree, control: ControlId, frame: Frame) {
    start(tree, control, None, ValueAnimator::new(0.0, 0.0, 0.0, easing::linear), None, None);
    tree.animators.last_mut().expect("just pushed").frame = Some(frame);
}

/// Puts the frame animators of a control to sleep until the frame time `until_ms`: no ticks and
/// no frames for them meanwhile, the host wakes up at that time. 0 wakes them at once.
pub(crate) fn sleep(tree: &mut Tree, control: ControlId, until_ms: f64) {
    for a in tree.animators.iter_mut().filter(|a| a.control == control && a.frame.is_some()) {
        a.wake_ms = until_ms;
    }
}

/// Removes the frame animators of a control: no more ticks, no frame asked for them.
pub(crate) fn stop_frames(tree: &mut Tree, control: ControlId) {
    tree.animators.retain(|a| a.control != control || a.frame.is_none());
}

/// True when an animator wants the next frame.
pub(crate) fn running(tree: &Tree) -> bool {
    tree.animators.iter().any(|a| a.wake_ms <= tree.time_ms)
}

/// The earliest time a sleeping animator wants a frame at. A paused one wants none.
pub(crate) fn next_wake(tree: &Tree) -> Option<f64> {
    tree.animators.iter().map(|a| a.wake_ms).filter(|t| *t > tree.time_ms && t.is_finite()).min_by(f64::total_cmp)
}

/// One frame of every running animator. True when a completion callback ran (it may have
/// changed the app state).
pub(crate) fn tick(tree: &mut Tree, state: &mut dyn Any, time_ms: f64) -> bool {
    let mut state_touched = false;
    // Animators started by a callback during this tick get their first tick on the next frame.
    let newest = tree.last_animation;
    let mut last = 0;
    loop {
        // Callbacks may start and stop animators, so the position is looked up again every time.
        let i = tree.animators.partition_point(|a| a.id <= last);
        let Some(&Animator { id, control, wake_ms, .. }) = tree.animators.get(i).filter(|a| a.id <= newest) else { break };
        last = id;
        if time_ms < wake_ms {
            continue;
        }
        if tree.node(control).is_none() {
            // Its control is gone: nothing to animate, nobody to tell.
            tree.animators.remove(i);
            continue;
        }
        if let Some(frame) = tree.animators[i].frame {
            let tick = frame(control, time_ms, &mut *state, &mut Cx { tree });
            state_touched |= tick.state_touched;
            if !tick.keep
                && let Ok(i) = tree.animators.binary_search_by_key(&id, |a| a.id)
            {
                tree.animators.remove(i);
            }
            continue;
        }
        let a = &mut tree.animators[i];
        let start = *a.start_ms.get_or_insert(time_ms);
        let progress = if a.run.duration_ms > 0.0 { (time_ms - start) / a.run.duration_ms as f64 } else { 1.0 };
        let done = progress >= 1.0;
        let run = a.run;
        let value = if done { run.to } else { run.from + (run.to - run.from) * (run.easing)(progress as f32) };
        a.value = value;
        let mut update = a.update.take();
        if a.overlay.is_some() {
            // The overlay changed (or, when done, is gone): own cache stays, ancestors composite again.
            tree.invalidate(control, Dirty::REPAINT);
        }
        if let Some(update) = update.as_mut() {
            update(value, &mut Cx { tree });
        }
        // The callback may have stopped its own animator.
        let Ok(i) = tree.animators.binary_search_by_key(&id, |a| a.id) else { continue };
        let a = &mut tree.animators[i];
        a.update = update;
        if !done {
            continue;
        }
        if a.run.repeat != 0 {
            a.run.repeat -= (a.run.repeat > 0) as i32;
            a.start_ms = None;
            if a.run.ping_pong {
                std::mem::swap(&mut a.run.from, &mut a.run.to);
            }
            continue;
        }
        if let Some(finished) = tree.animators.remove(i).finished {
            finished(state, &mut Cx { tree });
            state_touched = true;
        }
    }
    state_touched
}

/// The end value exactly when the animator reports its end.
fn lerp(from: f32, to: f32, v: f32) -> f32 {
    if v == 1.0 { to } else { from + (to - from) * v }
}

/// DrawnUI RippleAnimator defaults: duration, radius in points at the end, opacity at the start.
const RIPPLE_MS: f32 = 500.0;
const RIPPLE_DIAMETER: f32 = 300.0;
const RIPPLE_OPACITY: f32 = 0.2;

impl Cx<'_> {
    /// Starts a value animator on a control: `update(value, cx)` runs every frame while the value
    /// goes from `from` to `to`. It stops by itself when the control is removed.
    pub fn start_animator(
        &mut self,
        id: impl Into<ControlId>,
        animator: ValueAnimator,
        update: impl FnMut(f32, &mut Cx<'_>) + 'static,
    ) -> AnimationId {
        start(self.tree, id.into(), None, animator, Some(Box::new(update)), None)
    }

    /// DrawnUI AnimateAsync: `update` gets the eased progress 0..1 over `ms`.
    pub fn animate(
        &mut self,
        id: impl Into<ControlId>,
        ms: impl IntoProp<f32>,
        easing: Easing,
        update: impl FnMut(f32, &mut Cx<'_>) + 'static,
    ) -> AnimationId {
        self.start_animator(id, ValueAnimator::new(0.0, 1.0, ms.into_prop(), easing), update)
    }

    /// A built-in property animation. Starting one stops the running one of the same kind on the
    /// same control (the per-property cancellation of C#); the property stays where that one left it.
    fn animate_own(
        &mut self,
        id: ControlId,
        kind: Kind,
        ms: f32,
        easing: Easing,
        mut apply: impl FnMut(&mut Mut<'_, dyn Control>, f32) + 'static,
    ) -> AnimationId {
        self.tree.animators.retain(|a| a.control != id || a.kind != Some(kind));
        let update = move |v: f32, cx: &mut Cx<'_>| {
            if let Some(mut control) = cx.any_mut(id) {
                apply(&mut control, v)
            }
        };
        start(self.tree, id, Some(kind), ValueAnimator::new(0.0, 1.0, ms, easing), Some(Box::new(update)), None)
    }

    // The start values below are read at the first tick (the same frame), not at the call: a
    // handler's own control is out of the tree while the handler runs.

    /// Animates `opacity` from its current value (DrawnUI FadeToAsync).
    pub fn fade_to(
        &mut self,
        id: impl Into<ControlId>,
        opacity: impl IntoProp<f32>,
        ms: impl IntoProp<f32>,
        easing: Easing,
    ) -> AnimationId {
        let (to, mut from) = (opacity.into_prop(), None);
        self.animate_own(id.into(), Kind::Fade, ms.into_prop(), easing, move |c, v| {
            let from = *from.get_or_insert(c.base().p.opacity);
            c.set_opacity(lerp(from, to, v));
        })
    }

    /// Animates `scale_x` and `scale_y` from their current values (DrawnUI ScaleToAsync).
    pub fn scale_to(
        &mut self,
        id: impl Into<ControlId>,
        x: impl IntoProp<f32>,
        y: impl IntoProp<f32>,
        ms: impl IntoProp<f32>,
        easing: Easing,
    ) -> AnimationId {
        let (x, y, mut from) = (x.into_prop(), y.into_prop(), None);
        self.animate_own(id.into(), Kind::Scale, ms.into_prop(), easing, move |c, v| {
            let (fx, fy) = *from.get_or_insert((c.base().p.scale_x, c.base().p.scale_y));
            c.set_scale_x(lerp(fx, x, v));
            c.set_scale_y(lerp(fy, y, v));
        })
    }

    /// Animates `translation_x` and `translation_y` from their current values (DrawnUI TranslateToAsync).
    pub fn translate_to(
        &mut self,
        id: impl Into<ControlId>,
        x: impl IntoProp<f32>,
        y: impl IntoProp<f32>,
        ms: impl IntoProp<f32>,
        easing: Easing,
    ) -> AnimationId {
        let (x, y, mut from) = (x.into_prop(), y.into_prop(), None);
        self.animate_own(id.into(), Kind::Translate, ms.into_prop(), easing, move |c, v| {
            let (fx, fy) = *from.get_or_insert((c.base().p.translation_x, c.base().p.translation_y));
            c.set_translation_x(lerp(fx, x, v));
            c.set_translation_y(lerp(fy, y, v));
        })
    }

    /// Animates `rotation` (degrees) from its current value (DrawnUI RotateToAsync).
    pub fn rotate_to(
        &mut self,
        id: impl Into<ControlId>,
        degrees: impl IntoProp<f32>,
        ms: impl IntoProp<f32>,
        easing: Easing,
    ) -> AnimationId {
        let (to, mut from) = (degrees.into_prop(), None);
        self.animate_own(id.into(), Kind::Rotate, ms.into_prop(), easing, move |c, v| {
            let from = *from.get_or_insert(c.base().p.rotation);
            c.set_rotation(lerp(from, to, v));
        })
    }

    /// The frame time of this frame, milliseconds on the host's clock.
    pub fn time_ms(&self) -> f64 {
        self.tree.time_ms
    }

    /// DrawnUI ActionOnTickAnimator: `action(time_ms, cx)` runs once per drawn frame, and frames
    /// keep coming, until it is stopped or its control is removed. A game loop.
    pub fn action_on_tick(
        &mut self,
        id: impl Into<ControlId>,
        mut action: impl FnMut(f64, &mut Cx<'_>) + 'static,
    ) -> AnimationId {
        let every_frame = ValueAnimator::new(0.0, 0.0, 0.0, easing::linear).repeat(-1);
        self.start_animator(id, every_frame, move |_, cx| action(cx.time_ms(), cx))
    }

    /// Freezes an animation where it is: no ticks, no frames for it (DrawnUI Pause).
    pub fn pause_animation(&mut self, animation: AnimationId) {
        let Ok(i) = self.tree.animators.binary_search_by_key(&animation.0, |a| a.id) else { return };
        let a = &mut self.tree.animators[i];
        if a.paused.is_none() {
            a.paused = Some((self.tree.time_ms, a.wake_ms));
            a.wake_ms = f64::INFINITY;
        }
    }

    /// Lets a paused animation go on from where it was (DrawnUI Resume).
    pub fn resume_animation(&mut self, animation: AnimationId) {
        let Ok(i) = self.tree.animators.binary_search_by_key(&animation.0, |a| a.id) else { return };
        let a = &mut self.tree.animators[i];
        let Some((paused, wake)) = a.paused.take() else { return };
        // The pause is time that did not pass for the run, or for the delay it waited out.
        let now = self.tree.time_ms;
        if let Some(start) = a.start_ms.as_mut() {
            *start += now - paused;
        }
        a.wake_ms = if wake > paused { now + (wake - paused) } else { 0.0 };
    }

    /// Stops an animation where it is. Its completion callback does not run.
    pub fn stop_animation(&mut self, animation: AnimationId) {
        let Ok(i) = self.tree.animators.binary_search_by_key(&animation.0, |a| a.id) else { return };
        let stopped = self.tree.animators.remove(i);
        if stopped.overlay.is_some() {
            self.tree.invalidate(stopped.control, Dirty::REPAINT);
        }
    }

    /// Runs `run` once, `ms` from now (a timer). No frames are drawn while it waits. It is dropped
    /// when the control is removed; `stop_animation` cancels it.
    pub fn after<S: Any>(
        &mut self,
        id: impl Into<ControlId>,
        ms: impl IntoProp<f32>,
        run: impl FnOnce(&mut S, &mut Cx<'_>) + 'static,
    ) -> AnimationId {
        let animation = start(self.tree, id.into(), None, ValueAnimator::new(0.0, 0.0, 0.0, easing::linear), None, None);
        self.tree.animators.last_mut().expect("just pushed").wake_ms = self.tree.time_ms + ms.into_prop() as f64;
        self.on_finished(animation, run);
        animation
    }

    /// Runs `finished` once when the animation reaches its end by itself: not when it is stopped,
    /// replaced by one of the same kind, or its control is removed. Observers run after it.
    pub fn on_finished<S: Any>(
        &mut self,
        animation: AnimationId,
        finished: impl FnOnce(&mut S, &mut Cx<'_>) + 'static,
    ) {
        let Ok(i) = self.tree.animators.binary_search_by_key(&animation.0, |a| a.id) else { return };
        self.tree.animators[i].finished = Some(Box::new(move |state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            finished(state, cx)
        }));
    }

    /// Starts an overlay effect (DrawnUI RenderingAnimator as a post animator): while the animator
    /// runs, `draw(cx, value)` paints above the control's content every frame, outside its cache,
    /// clipped to the control's `create_clip` shape. `cx.rect` is the control's drawing rect.
    pub fn play_overlay(
        &mut self,
        id: impl Into<ControlId>,
        animator: ValueAnimator,
        draw: impl Fn(&mut PaintCx<'_>, f32) + 'static,
    ) -> AnimationId {
        start(self.tree, id.into(), None, animator, None, Some(Box::new(draw)))
    }

    /// DrawnUI PlayRippleAnimation: a circle that grows from (x, y) and fades out. The point is in
    /// points, relative to the control's top-left. `speed_ms` 0 = the default 500 ms.
    pub fn play_ripple(
        &mut self,
        id: impl Into<ControlId>,
        color: Color,
        x: f32,
        y: f32,
        speed_ms: f32,
    ) -> AnimationId {
        let ms = if speed_ms > 0.0 { speed_ms } else { RIPPLE_MS };
        self.play_overlay(id, ValueAnimator::new(0.0, 1.0, ms, easing::cubic_in), move |cx, progress| {
            let opacity = (RIPPLE_OPACITY * (1.0 - progress * 1.15)).max(0.0);
            let mut paint = Paint::default();
            paint.set_color(color.with_a((opacity * 255.0) as u8));
            let center = (cx.rect.left + x * cx.scale, cx.rect.top + y * cx.scale);
            // As upstream: the "diameter" is used as the radius.
            cx.canvas.draw_circle(center, RIPPLE_DIAMETER * progress * cx.scale, &paint);
        })
    }
}
