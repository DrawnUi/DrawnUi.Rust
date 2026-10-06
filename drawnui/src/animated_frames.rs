//! The frame player of the controls that play frames (DrawnUI AnimatedFramesRenderer, React
//! `AnimatedFramesRenderer.ts`): a position that runs over a range on the frame clock, picks the
//! frame to show, repeats, and reports Started / Finished. A GIF's range is its length in
//! milliseconds; a Lottie's would be its frames. The control embeds a `FramePlayer` as its inner
//! part, gives it the frame ends, and draws `current()`.

use std::any::Any;

use crate::animators::{self, FrameTick};
use crate::control::{Control, Has, part_mut};
use crate::props;
use crate::tree::{Build, ControlId, Cx, Handle, Tree, wrong_state};
use crate::types::Dirty;

props!(FramesProps, FramesBuild, FramesSet {
    /// Plays as soon as the frames are there and the control is placed. Set to false, it stops.
    auto_play / set_auto_play: bool = true, APPLY;
    /// Runs after the first: > 0 that many more, < 0 forever, 0 none. Read when playing starts.
    repeat / set_repeat: i32 = 0, NONE;
    /// Playback speed. Below 1 the run takes `1 + speed_ratio` times its length, as upstream.
    speed_ratio / set_speed_ratio: f32 = 1.0, APPLY;
    /// Position shown while not playing, in the player's range (milliseconds for a GIF); -1 = the end.
    default_frame / set_default_frame: i32 = 0, DRAW_APPLY;
});

/// `handler(app state, cx)`.
type Handler = Box<dyn FnMut(&mut dyn Any, &mut Cx<'_>)>;

#[derive(Default)]
pub struct FramePlayer {
    pub p: FramesProps,
    /// Where each frame ends, in the range's unit, cumulative; the range is the last one.
    ends: Vec<f32>,
    /// Index of the frame on screen.
    current: usize,
    position: f32,
    playing: bool,
    /// Frame time the current run began at.
    started_ms: Option<f64>,
    repeats_left: i32,
    /// Start was asked before the player could play; it plays as soon as it can.
    play_when_available: bool,
    /// The control was arranged once (React `wasLayout`).
    placed: bool,
    /// A frame animator is running for the control.
    ticking: bool,
    /// The app stopped or restarted a run while it played: Finished is due at the next tick.
    stopped: bool,
    /// `auto_play` false was applied: a change to false stops once.
    auto_play_off: bool,
    /// `default_frame` as last applied: only its change moves a stopped player.
    default_applied: i32,
    /// The control, once placed: a stop wakes its sleeping animator.
    id: Option<ControlId>,
    on_started: Option<Handler>,
    on_finished: Option<Handler>,
}

impl FramePlayer {
    /// The frame to draw.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The range: the end of the last frame (a GIF's length in milliseconds).
    pub fn range(&self) -> f32 {
        self.ends.last().copied().unwrap_or(0.0)
    }

    /// Where the player is in its range.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// The frames to play: how long each one lasts, in the range's unit. Shows `default_frame`.
    pub(crate) fn set_frames(&mut self, durations: impl IntoIterator<Item = f32>) {
        let mut end = 0.0;
        self.ends = durations
            .into_iter()
            .map(|duration| {
                end += duration;
                end
            })
            .collect();
        self.seek(self.p.default_frame as f32);
    }

    /// Milliseconds one run takes (React `ApplySpeed`).
    fn run_ms(&self) -> f32 {
        let (length, ratio) = (self.range(), self.p.speed_ratio);
        if ratio < 1.0 { length * (1.0 + ratio) } else { length / ratio }
    }

    /// The frame shown at a position (React `GifAnimation.GetFrameNumber`): negative counts from
    /// the end, past the end wraps.
    fn frame_at(&self, position: f32) -> usize {
        let range = self.range();
        if self.ends.is_empty() || range <= 0.0 {
            return 0;
        }
        let mut position = if position < 0.0 { range + position } else { position };
        position %= range;
        self.ends.iter().position(|end| position < *end).unwrap_or(0)
    }

    /// Frame time the frame on screen gives way to the next one while playing, or the run ends.
    fn next_change_ms(&self) -> Option<f64> {
        let (began, range) = (self.started_ms?, self.range());
        let end = *self.ends.get(self.current)?;
        (self.playing && range > 0.0).then(|| began + (end / range * self.run_ms()) as f64)
    }

    /// Moves to a position in the range. True when the frame on screen changed.
    fn seek(&mut self, position: f32) -> bool {
        self.position = position;
        let frame = self.frame_at(position);
        std::mem::replace(&mut self.current, frame) != frame
    }

    /// One frame of the clock. Returns (frame changed, started, finished).
    fn advance(&mut self, time_ms: f64) -> (bool, bool, bool) {
        if !self.playing {
            return (false, false, false);
        }
        let started = self.started_ms.is_none();
        let began = *self.started_ms.get_or_insert(time_ms);
        let run_ms = self.run_ms();
        let progress = if run_ms > 0.0 { ((time_ms - began) / run_ms as f64) as f32 } else { 1.0 };
        if progress < 1.0 {
            return (self.seek(progress * self.range()), started, false);
        }
        // A run is over: the next one starts at this frame, or the player stops on its range's end.
        let changed = self.seek(self.range());
        if self.repeats_left != 0 {
            self.repeats_left -= (self.repeats_left > 0) as i32;
            self.started_ms = Some(time_ms);
            return (changed, started, false);
        }
        self.playing = false;
        (changed, started, true)
    }
}

impl Has<FramesProps> for FramePlayer {
    fn part(&self) -> &FramesProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut FramesProps {
        &mut self.p
    }
}

impl Control for FramePlayer {
    /// `auto_play`, `speed_ratio` or `default_frame` changed (React's setters): a change of
    /// `auto_play` to false stops; a new default frame shows while not playing.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if !self.p.auto_play && !std::mem::replace(&mut self.auto_play_off, true) {
            self.stop();
            if let Some(id) = self.id {
                animators::sleep(cx.tree, id, 0.0);
            }
        }
        self.auto_play_off &= !self.p.auto_play;
        let default = self.p.default_frame;
        if std::mem::replace(&mut self.default_applied, default) != default && !self.playing {
            self.seek(default as f32);
        }
    }
}

impl FramePlayer {
    /// Stops where it is; a playing run reports Finished at the next tick (React `Stop`, whose
    /// animator's OnStop calls OnFinished when it was started).
    fn stop(&mut self) {
        self.stopped |= self.playing;
        (self.playing, self.play_when_available) = (false, false);
    }
}

fn player<T: Control>(control: &mut T) -> &mut FramePlayer {
    part_mut(control).expect("the control embeds a FramePlayer")
}

impl<T: Has<FramesProps>> Build<T> {
    /// Runs when playing starts (DrawnUI Started).
    pub fn on_started<S: Any>(mut self, mut f: impl FnMut(Handle<T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        let me = self.handle();
        player(self.control_mut()).on_started = Some(Box::new(move |state, cx| {
            f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx)
        }));
        self
    }

    /// Runs when playing ends: after its last run, and when it is stopped or started again while
    /// it plays (DrawnUI Finished). The tick after the stop runs it.
    pub fn on_finished<S: Any>(mut self, mut f: impl FnMut(Handle<T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        let me = self.handle();
        player(self.control_mut()).on_finished = Some(Box::new(move |state, cx| {
            f(me, state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx)
        }));
        self
    }
}

/// Starts the frame animator of a control that can play: it has frames and was placed.
fn start(tree: &mut Tree, id: ControlId) {
    let Some(mut me) = tree.find_mut::<FramePlayer>(id) else { return };
    let Some((changed, tick_now)) = me.control_mut().begin() else { return };
    if changed {
        me.mark(Dirty::DRAW);
    }
    if tick_now {
        animators::start_frame(tree, id, tick);
    }
}

impl FramePlayer {
    /// A run begins when the player can play (it has frames and was placed), else it plays as
    /// soon as it can. Returns (the frame changed, a frame animator must start).
    fn begin(&mut self) -> Option<(bool, bool)> {
        if self.ends.is_empty() || !self.placed {
            self.play_when_available = true;
            return None;
        }
        (self.play_when_available, self.playing, self.started_ms) = (false, true, None);
        self.repeats_left = self.p.repeat;
        let changed = self.seek(0.0);
        Some((changed, !std::mem::replace(&mut self.ticking, true)))
    }
}

/// The control was placed: plays when set to (React `OnLayoutChanged`). Called from the
/// control's `arrange`, where it is out of the tree: it hands its player.
pub(crate) fn placed(player: &mut FramePlayer, tree: &mut Tree, id: ControlId) {
    player.id = Some(id);
    let first = !std::mem::replace(&mut player.placed, true);
    if first && player.p.auto_play {
        player.play_when_available = true;
    }
    if !player.play_when_available {
        return;
    }
    let Some((changed, tick_now)) = player.begin() else { return };
    if changed {
        tree.invalidate(id, Dirty::DRAW);
    }
    if tick_now {
        animators::start_frame(tree, id, tick);
    }
}

/// One frame of a playing control: the position moves, the frame may change, Started and
/// Finished run (Finished of a stopped run first). The animator ends with the run, and sleeps
/// until the frame on screen changes: a GIF of 100 ms frames asks for 10 frames a second.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<FramePlayer>(id) else { return tick };
    let player = me.control_mut();
    let stopped = std::mem::take(&mut player.stopped);
    let (changed, started, finished) = player.advance(time_ms);
    // The handlers leave the control while they run: they reach it through the tree.
    let mut on_started = if started { player.on_started.take() } else { None };
    let mut on_finished = if finished || stopped { player.on_finished.take() } else { None };
    tick.keep = player.playing;
    if !tick.keep {
        player.ticking = false;
    }
    if changed {
        me.mark(Dirty::DRAW);
    }
    if let Some(handler) = on_finished.as_mut().filter(|_| stopped) {
        handler(state, cx);
    }
    if let Some(handler) = &mut on_started {
        handler(state, cx);
    }
    if let Some(handler) = on_finished.as_mut().filter(|_| finished) {
        handler(state, cx);
    }
    tick.state_touched = on_started.is_some() || on_finished.is_some();
    let Some(mut me) = cx.tree.find_mut::<FramePlayer>(id) else { return tick };
    let player = me.control_mut();
    if on_started.is_some() {
        player.on_started = on_started;
    }
    if on_finished.is_some() {
        player.on_finished = on_finished;
    }
    if let Some(wake) = player.next_change_ms().filter(|wake| *wake > time_ms) {
        animators::sleep(cx.tree, id, wake);
    }
    tick
}

impl Cx<'_> {
    /// Plays a control's frames from the start, `repeat` times more (DrawnUI Start). Before the
    /// frames are there or the control is placed, it plays as soon as it can.
    pub fn start_frames(&mut self, control: impl Into<ControlId>) {
        let id = control.into();
        if let Some(mut me) = self.tree.find_mut::<FramePlayer>(id) {
            me.control_mut().stop();
        }
        start(self.tree, id);
        // A sleeping run wakes to end now.
        animators::sleep(self.tree, id, 0.0);
    }

    /// Stops playing where it is; the frame on screen stays, Finished runs when it played
    /// (DrawnUI Stop).
    pub fn stop_frames(&mut self, control: impl Into<ControlId>) {
        let id = control.into();
        if let Some(mut me) = self.tree.find_mut::<FramePlayer>(id) {
            me.control_mut().stop();
        }
        animators::sleep(self.tree, id, 0.0);
    }

    /// Shows the frame at a position of the range (milliseconds for a GIF); negative counts
    /// from the end (DrawnUI Seek).
    pub fn seek_frames(&mut self, control: impl Into<ControlId>, position: f32) {
        if let Some(mut me) = self.tree.find_mut::<FramePlayer>(control)
            && me.control_mut().seek(position)
        {
            me.mark(Dirty::DRAW);
        }
    }
}
