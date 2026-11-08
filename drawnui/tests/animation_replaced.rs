//! A control animated again and again in one frame (DrawnUi.Net c5528d6f, drawnui-cross 6bg): each
//! new fade / scale replaces the running one of its kind; the frame's other animators go on, and
//! the last call wins. A replaced animation's completion does not run (it did not reach its end).

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    label: Handle<SkiaShape>,
    other: Handle<SkiaShape>,
    frames: u32,
    other_done: bool,
    replaced_done: u32,
}

#[test]
fn re_animating_in_a_frame_keeps_every_other_animation_going() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((DrawnGame::new()
            .fill()
            .start_loop(0.0)
            .on_game_loop(|_me, app: &mut App, cx, _delta| {
                app.frames += 1;
                if app.frames == 1 {
                    let a = cx.scale_to(app.other, 2.0, 2.0, 200, easing::linear);
                    cx.on_finished(a, |app: &mut App, _cx| app.other_done = true);
                }
                if app.frames <= 10 {
                    // Twenty fades of one label in one frame; each replaces the one before.
                    for k in 0..20 {
                        let id = cx.fade_to(app.label, k as f32 / 20.0, 100, easing::linear);
                        cx.on_finished(id, |app: &mut App, _cx| app.replaced_done += 1);
                    }
                    // A completion handler that replaces an animation while the animator pass runs.
                    let quick = cx.rotate_to(app.label, 10.0, 0, easing::linear);
                    let label = app.label;
                    cx.on_finished(quick, move |_app: &mut App, cx| {
                        cx.rotate_to(label, 30.0, 50, easing::linear);
                    });
                }
            })
            .children((
                SkiaShape::new().width_request(40).height_request(40).assign(&mut app.label),
                SkiaShape::new().width_request(40).height_request(40).margin((60.0, 0.0, 0.0, 0.0)).assign(&mut app.other),
            )),))
    });
    let mut host = Headless::new(ui, 200, 100, 1.0);
    for _ in 0..40 {
        host.frame_after(16.0);
    }
    let (label, other) = (host.ui.state.label, host.ui.state.other);
    let p = |id: Handle<SkiaShape>| host.ui.tree.base(id).unwrap().p.clone();
    assert!(host.ui.state.other_done, "the other control's animation reached its end");
    assert_eq!(p(other).scale_x, 2.0);
    assert_eq!(p(label).opacity, 19.0 / 20.0, "the last fade wins");
    assert_eq!(p(label).rotation, 30.0, "the rotation started inside a completion ran");
    assert_eq!(host.ui.state.replaced_done, 1, "only the last fade, the one not replaced, finished");
}
