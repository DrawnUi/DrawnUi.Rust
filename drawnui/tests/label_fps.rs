//! SkiaLabelFps never asks for a frame by itself, as C#: a still screen draws nothing (FPS 00.0
//! until anything drew a rate); while something animates it shows the rate; once the frames stop
//! it keeps that value, with no frame and no wake. `force_refresh` keeps the canvas drawing, as
//! C# ForceRefresh does.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    fps: Handle<SkiaLabelFps>,
    mover: Handle<SkiaShape>,
}

fn host(force: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaLayout::new().fill().children((
            SkiaShape::new().width_request(20).height_request(20).background_color(Color::WHITE).assign(&mut app.mover),
            SkiaLabelFps::new().force_refresh(force).assign(&mut app.fps),
        ))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 200, 100, 1.0);
    if !force {
        host.settle();
    }
    host
}

fn shown(host: &Headless<App>) -> String {
    let label = host.ui.tree.find::<SkiaLabelFps>(host.ui.state.fps).unwrap();
    drawnui::Has::<drawnui::controls::label::LabelProps>::part(label).text.clone()
}

#[test]
fn the_label_asks_for_no_frame_and_keeps_its_last_value() {
    let mut host = host(false);
    assert!(!host.ui.needs_frame(), "the label asks for no frame");
    assert_eq!(host.ui.wake_at(), None);
    assert_eq!(shown(&host), "FPS 00.0");

    // One second of animation at 60 frames per second: the rate shows.
    let mover = host.ui.state.mover;
    host.ui.tree.cx().animate(mover, 1000.0, easing::linear, move |v, cx| {
        if let Some(mut c) = cx.any_mut(mover) {
            c.set_translation_x(v * 100.0);
        }
    });
    let mut frames = 0;
    while host.ui.needs_frame() && frames < 200 {
        host.frame_after(1000.0 / 60.0);
        frames += 1;
    }
    let rate = shown(&host);
    assert!(rate.starts_with("FPS 5") || rate.starts_with("FPS 6"), "{rate} after {frames} frames");

    // The frames stopped: no frame and no wake; the label keeps the rate it showed (C#).
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), None, "no wake for the label");
    assert_eq!(shown(&host), rate);
}

#[test]
fn force_refresh_keeps_the_canvas_drawing_until_it_is_off() {
    let mut host = host(true);
    for _ in 0..90 {
        host.frame_after(1000.0 / 60.0);
        assert!(host.ui.needs_frame(), "force_refresh asks for every frame");
    }
    let rate = shown(&host);
    assert!(rate.starts_with("FPS 5") || rate.starts_with("FPS 6"), "{rate}");
    let fps = host.ui.state.fps;
    host.ui.tree.cx().get_mut(fps).unwrap().set_force_refresh(false);
    let mut frames = 0;
    while host.ui.needs_frame() && frames < 10 {
        host.frame_after(1000.0 / 60.0);
        frames += 1;
    }
    assert!(!host.ui.needs_frame(), "off: the frames stop ({frames} more)");
}
