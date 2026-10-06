//! DrawnGame: a layout that runs a game loop (DrawnUi.Gaming DrawnGame, React `DrawnGame.ts`).
//! `on_game_loop` gets the seconds since the previous frame on every drawn frame between
//! `start_loop` and `stop_loop`; the loop is a frame animator, so a stopped game asks for no frames.
//! The game hears every key while mounted (React KeyboardManager.Subscribe): its `on_key_down` /
//! `on_key_up` handlers. Sprites move by `left` / `top`, which repaint without a layout.
// ponytail: no FrameTimeInterpolator (C# turns it off on the browser, OpenTK and Android too, the
// raw frame delta is used), no OnPaused / OnResumed overrides: the app runs its own code where it
// calls `pause` / `resume`.

use std::any::Any;

use crate::animators::{self, FrameTick};
use crate::control::{Control, Has, part_mut};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::tree::{Build, Container, ControlId, Cx, Mut, Raw, wrong_state};
use crate::types::Dirty;

/// `handler(me, app state, cx, delta seconds)`.
type GameLoop = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, f32)>;

/// A layout that runs a game loop (DrawnUI DrawnGame): Absolute like a bare SkiaLayout.
pub struct DrawnGame {
    layout: SkiaLayout,
    id: ControlId,
    /// Between `start_loop` and `stop_loop`.
    looping: bool,
    /// `start_loop` was asked (with its delay, ms); the tree applies it.
    start: Option<f32>,
    /// A frame animator is registered for the game.
    ticking: bool,
    /// Frame time of the previous tick; `None`: the next tick's delta is 0.
    last_ms: Option<f64>,
    paused: bool,
    on_game_loop: Option<GameLoop>,
    /// The marks `me` makes while the loop handler runs; kept for its allocation.
    queue: Vec<ControlId>,
}

impl DrawnGame {
    /// A game that hears every key; its loop runs once `start_loop` is called.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<DrawnGame> {
        let game = DrawnGame {
            layout: SkiaLayout::default(),
            id: ControlId { index: 0, generation: 0 },
            looping: false,
            start: None,
            ticking: false,
            last_ms: None,
            paused: false,
            on_game_loop: None,
            queue: Vec::new(),
        };
        let mut build = Build::new(game);
        let id = build.id();
        build.control_mut().id = id;
        build.listen_keys()
    }

    /// The loop runs: started and not stopped.
    pub fn is_running(&self) -> bool {
        self.looping || self.start.is_some()
    }

    /// `pause` was called and `resume` not yet (DrawnUI IsPaused). The loop keeps running, as
    /// upstream: the game reads this.
    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

impl Has<LayoutProps> for DrawnGame {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for DrawnGame {}

impl Control for DrawnGame {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// A start asked for registers the loop's frame animator (sleeping through the delay); a stop
    /// removes it, so no frame is asked for any more.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if let Some(delay_ms) = self.start.take() {
            if !self.looping {
                (self.looping, self.last_ms) = (true, None);
                if !std::mem::replace(&mut self.ticking, true) {
                    animators::start_frame(cx.tree, self.id, tick);
                }
                if delay_ms > 0.0 {
                    animators::sleep(cx.tree, self.id, cx.tree.time_ms + delay_ms as f64);
                }
            }
        } else if !self.looping && self.ticking {
            self.ticking = false;
            animators::stop_frames(cx.tree, self.id);
        }
        self.layout.on_props_changed(cx);
    }
}

/// One frame of the loop: the delta since the previous one, then the app's loop handler with the
/// game as `me`, its node out of the tree as for any handler.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut result = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return result };
    let Some(game) = node.kind.as_deref_mut().and_then(part_mut::<DrawnGame>) else {
        cx.tree.put_back(node);
        return result;
    };
    if !game.looping {
        game.ticking = false;
        cx.tree.put_back(node);
        return result;
    }
    let delta = game.last_ms.map_or(0.0, |last| ((time_ms - last) / 1000.0) as f32);
    game.last_ms = Some(time_ms);
    let (handler, mut queue) = (game.on_game_loop.take(), std::mem::take(&mut game.queue));
    if let Some(mut handler) = handler {
        let control = node.kind.as_deref_mut().expect("checked above");
        handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, delta);
        result.state_touched = true;
        let game = node.kind.as_deref_mut().and_then(part_mut::<DrawnGame>).expect("checked above");
        game.on_game_loop.get_or_insert(handler);
    }
    cx.tree.queue.append(&mut queue);
    let game = node.kind.as_deref_mut().and_then(part_mut::<DrawnGame>).expect("checked above");
    game.queue = queue;
    // The handler may have stopped the loop.
    result.keep = game.looping;
    game.ticking = result.keep;
    cx.tree.put_back(node);
    result
}

impl Mut<'_, DrawnGame> {
    /// Starts the game loop, after `delay_ms` (DrawnUI StartLoop). The first frame's delta is 0.
    /// Nothing happens when it runs already.
    pub fn start_loop(&mut self, delay_ms: f32) {
        if !self.control_mut().looping {
            self.control_mut().start = Some(delay_ms);
            self.mark(Dirty::APPLY);
        }
    }

    /// Stops the game loop (DrawnUI StopLoop): the loop handler does not run again and the game
    /// asks for no frames.
    pub fn stop_loop(&mut self) {
        let game = self.control_mut();
        (game.looping, game.start) = (false, None);
        self.mark(Dirty::APPLY);
    }

    /// Marks the game paused (DrawnUI Pause): `is_paused` reads true; the loop keeps running.
    pub fn pause(&mut self) {
        self.control_mut().paused = true;
    }

    /// Clears the pause and restarts the frame clock: the next delta is 0, not the time the
    /// frames stopped for (DrawnUI Resume).
    pub fn resume(&mut self) {
        let game = self.control_mut();
        (game.paused, game.last_ms) = (false, None);
    }
}

impl Build<DrawnGame> {
    /// The game loop (DrawnUI GameLoop override): runs once per drawn frame while the loop runs,
    /// with the seconds since the previous frame (0 on the first one after a start or resume).
    pub fn on_game_loop<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, DrawnGame>, &mut S, &mut Cx<'_>, f32) + 'static) -> Self {
        self.control_mut().on_game_loop = Some(Box::new(move |raw, state, cx, delta| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx, delta)
        }));
        self
    }

    /// Starts the loop once the game is mounted, after `delay_ms` (a StartLoop in the constructor).
    pub fn start_loop(mut self, delay_ms: f32) -> Self {
        self.control_mut().start = Some(delay_ms);
        self
    }
}
