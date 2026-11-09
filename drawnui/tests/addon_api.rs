//! The add-on API, used from outside the crate as an add-on (drawnui-add3d) does: a control of its
//! own with a frame loop (`Cx::start_frame_loop`) that calls the app's handler with the app state
//! (`Cx::with_control`, `downcast_state`), and ends by itself or when told.

use std::any::Any;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{FrameTick, downcast_state};

type Done = Box<dyn FnMut(&mut Mut<'_, Countdown>, &mut dyn Any, &mut Cx<'_>, u32)>;

/// Counts frames down from `left`, then tells the app; starts its loop when it is added.
struct Countdown {
    id: ControlId,
    left: u32,
    started: bool,
    done: Option<Done>,
}

impl Control for Countdown {
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if !std::mem::replace(&mut self.started, true) {
            cx.start_frame_loop(self.id, tick);
        }
    }
}

fn countdown(left: u32, done: impl FnMut(&mut App, u32) + 'static) -> Build<Countdown> {
    let mut done = done;
    let control = Countdown { id: ControlId::from_bits(0), left, started: false, done: Some(Box::new(move |_me, state, _cx, v| done(downcast_state::<App>(state), v))) };
    let mut build = Build::new(control);
    let id = build.id();
    build.control_mut().id = id;
    build
}

fn tick(id: ControlId, _time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    cx.with_control::<Countdown, _>(id, state, |me, state, cx| {
        let c = me.control_mut();
        c.left = c.left.saturating_sub(1);
        if c.left > 0 {
            return FrameTick { keep: true, state_touched: false };
        }
        let Some(mut done) = me.control_mut().done.take() else { return FrameTick::default() };
        done(me, state, cx, 7);
        me.control_mut().done = Some(done);
        FrameTick { keep: false, state_touched: true }
    })
    .unwrap_or_default()
}

#[derive(Default)]
struct App {
    counter: Handle<Countdown>,
    told: Vec<u32>,
    wrong_type_ran: bool,
}

#[test]
fn an_add_on_control_runs_its_own_frame_loop_and_calls_the_app() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((
            countdown(5, |app, v| app.told.push(v)).assign(&mut app.counter),
            // Restarts the loop and stops it at once; asks for the control as another type.
            SkiaShape::new().width_request(50).height_request(50).margin((40.0, 40.0, 0.0, 0.0)).on_tapped(|_me, app: &mut App, cx| {
                let id: ControlId = app.counter.into();
                cx.start_frame_loop(id, tick);
                cx.stop_frame_loop(id);
                let mut other = App::default();
                app.wrong_type_ran = cx.with_control::<SkiaShape, _>(id, &mut other, |_, _, _| ()).is_some();
            }),
        ))
    });
    let mut host = Headless::new(ui, 100, 100, 1.0);
    // Frames while the loop runs: five ticks after the one that added the control.
    let mut frames = 0;
    while host.ui.needs_frame() && frames < 30 {
        assert!(host.ui.state.told.is_empty() || !host.ui.needs_frame());
        host.frame_after(16.0);
        frames += 1;
    }
    assert_eq!(host.ui.state.told, [7], "the app's handler ran with its state");
    assert!((6..=8).contains(&frames), "{frames} frames for a countdown of 5");
    assert!(!host.ui.needs_frame(), "the loop ended itself: no frames asked");

    host.tap(60.0, 60.0);
    assert!(!host.ui.state.wrong_type_ran, "another control type: nothing runs");
    assert!(!host.ui.needs_frame(), "a stopped loop asks for nothing");
    assert_eq!(host.ui.state.told, [7]);
}
