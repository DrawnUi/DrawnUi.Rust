//! `Ui::prepare` runs a frame's work that does not draw (input, animators, observers, layout) ahead
//! of `draw` at the same time, for hosts that wait for their drawing target (Metal's drawable,
//! drawnui-cross 6k): the work runs once and the pixels are those of a plain `draw`.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

fn scene() -> (Headless<()>, ControlId, Rc<Cell<u32>>) {
    let square = SkiaLayout::new().width_request(20).height_request(20).background_color(Color::RED);
    let id = square.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(square)).background(Color::WHITE);
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    let ticks = Rc::new(Cell::new(0));
    let counted = ticks.clone();
    host.ui.tree.cx().animate(id, 1000.0, easing::linear, move |v, cx| {
        counted.set(counted.get() + 1);
        if let Some(mut square) = cx.any_mut(id) {
            square.set_translation_x(v * 60.0);
        }
    });
    (host, id, ticks)
}

#[test]
fn prepared_frames_tick_once_and_draw_as_plain_ones() {
    let (mut prepared, _, ticks) = scene();
    let (mut plain, _, plain_ticks) = scene();
    for _ in 0..5 {
        let time = prepared.time_ms() + 100.0;
        let before = ticks.get();
        prepared.ui.prepare(100.0, 100.0, 1.0, time);
        assert_eq!(ticks.get(), before + 1, "prepare ticks the animators");
        prepared.frame_after(100.0);
        assert_eq!(ticks.get(), before + 1, "the draw at the prepared time does not tick again");
        plain.frame_after(100.0);
        assert_eq!(plain_ticks.get(), ticks.get());
        for (x, y) in [(5, 5), (35, 10), (65, 10)] {
            assert_eq!(prepared.pixel(x, y), plain.pixel(x, y), "({x}, {y})");
        }
    }

    // Prepared for another time than the frame draws at: the frame prepares again.
    let before = ticks.get();
    let time = prepared.time_ms() + 50.0;
    prepared.ui.prepare(100.0, 100.0, 1.0, time);
    prepared.frame_after(100.0);
    assert_eq!(ticks.get(), before + 2);
}
