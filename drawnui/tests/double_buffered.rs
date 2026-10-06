//! ImageDoubleBuffered with bake workers, as on the desktop (DrawnUI's contract): the last bitmap
//! shows while the next one is made apart from the frame, a placeholder the very first frame,
//! one bake per control at a time, the same content at a new size made in the frame. Headless
//! makes the bitmaps right after each frame, or holds them until `deliver_bakes`.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::skia::Paint as SkPaint;
use drawnui::testing::Headless;
use drawnui::App as _;

/// A 20 x 20 point control of one color, with an effects margin that can grow.
struct Probe {
    color: Rc<Cell<Color>>,
    margin: Rc<Cell<f32>>,
}
impl Control for Probe {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(20.0 * cx.scale, 20.0 * cx.scale)
    }
    fn paint(&self, cx: &mut PaintCx) {
        let mut paint = SkPaint::default();
        paint.set_color(self.color.get());
        cx.canvas.draw_rect(cx.rect, &paint);
    }
    fn effects_margin(&self, _scale: f32) -> Thickness {
        Thickness::uniform(self.margin.get())
    }
}

struct Scene {
    host: Headless<()>,
    id: ControlId,
    color: Rc<Cell<Color>>,
    margin: Rc<Cell<f32>>,
}

/// The probe at (50, 50), double buffered, in a 200 x 100 white canvas; no frame drawn yet.
fn scene() -> Scene {
    let (color, margin) = (Rc::new(Cell::new(Color::RED)), Rc::new(Cell::new(0.0)));
    let probe = Build::new(Probe { color: color.clone(), margin: margin.clone() })
        .margin(Thickness::new(50.0, 50.0, 0.0, 0.0))
        .use_cache(CacheType::ImageDoubleBuffered);
    let id = probe.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(probe)).background(Color::WHITE);
    Scene { host: Headless::new(ui, 200, 100, 1.0), id, color, margin }
}

impl Scene {
    fn change(&mut self, color: Color) {
        self.color.set(color);
        self.host.ui.tree.invalidate(self.id, Dirty::DRAW);
    }
    fn shown(&mut self) -> Color {
        self.host.pixel(60, 60)
    }
}

#[test]
fn the_first_frame_shows_a_placeholder_then_the_bitmap() {
    let mut s = scene();
    s.host.frame();
    // DrawnUI DrawPlaceholder: no background color, a faint gray over the white canvas.
    let placeholder = s.shown();
    assert!(placeholder != Color::WHITE && placeholder != Color::RED, "placeholder {placeholder:?}");
    s.host.frame();
    assert_eq!(s.shown(), Color::RED);
    assert_eq!(s.host.cache_records(s.id), 1);
}

#[test]
fn the_placeholder_shows_on_every_frame_until_the_first_bitmap_is_back() {
    // C# 23b52ad0: it used to show one frame, then a hole until the bitmap came.
    let mut s = scene();
    s.host.hold_bakes(true);
    let mut shown = Vec::new();
    for _ in 0..4 {
        s.host.frame();
        shown.push(s.shown());
    }
    assert!(shown.iter().all(|c| *c == shown[0] && *c != Color::WHITE && *c != Color::RED), "{shown:?}");
    assert_eq!(s.host.deliver_bakes(), 1, "one bake for the content, not one per frame");
    s.host.frame();
    assert_eq!(s.shown(), Color::RED);
}

#[test]
fn new_content_shows_once_its_bitmap_is_back_the_last_one_meanwhile() {
    let mut s = scene();
    s.host.settle();
    s.host.hold_bakes(true);
    s.change(Color::BLUE);
    s.host.frame();
    s.host.frame();
    assert_eq!(s.shown(), Color::RED, "the last bitmap while the new one is made");
    assert_eq!(s.host.deliver_bakes(), 1);
    s.host.frame();
    assert_eq!(s.shown(), Color::BLUE);
}

#[test]
fn bitmaps_are_made_on_other_threads() {
    let mut s = scene();
    s.host.settle();
    s.host.hold_bakes(true);
    s.change(Color::BLUE);
    s.host.frame();
    let requests = s.host.take_bakes();
    assert_eq!(requests.len(), 1);
    // As the desktop's bake workers: the picture is drawn on another thread, the bitmap comes back.
    let workers: Vec<_> = requests.into_iter().map(|r| std::thread::spawn(move || (r.id, r.bake()))).collect();
    for worker in workers {
        let (id, image) = worker.join().expect("worker");
        assert!(image.is_some());
        s.host.ui.baked(id, image);
    }
    s.host.frame();
    assert_eq!(s.shown(), Color::BLUE);
}

#[test]
fn one_bake_per_control_is_out_and_the_latest_content_follows() {
    let mut s = scene();
    s.host.settle();
    s.host.hold_bakes(true);
    s.change(Color::BLUE);
    s.host.frame();
    s.change(Color::GREEN);
    s.host.frame();
    s.host.frame();
    assert_eq!(s.host.deliver_bakes(), 1, "one out at a time");
    s.host.frame();
    assert_eq!(s.shown(), Color::BLUE, "the bitmap that came back");
    assert_eq!(s.host.deliver_bakes(), 1, "then the latest content");
    s.host.frame();
    assert_eq!(s.shown(), Color::GREEN);
}

#[test]
fn the_same_content_at_a_new_size_is_made_in_the_frame() {
    let mut red: Handle<SkiaLayout> = Handle::default();
    let layout = SkiaLayout::new().fill().background_color(Color::RED).use_cache(CacheType::ImageDoubleBuffered).assign(&mut red);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(layout)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host.hold_bakes(true);
    host.resize(300, 100);
    host.frame();
    assert_eq!(host.pixel(250, 50), Color::RED, "made in the frame at the new size");
    assert_eq!(host.deliver_bakes(), 0);
}

#[test]
fn a_bitmap_that_cannot_be_made_keeps_the_last_one_and_is_not_tried_again() {
    let mut s = scene();
    s.host.settle();
    // A margin no bitmap can hold (two million pixels a side).
    s.margin.set(1_000_000.0);
    s.change(Color::BLUE);
    s.host.settle();
    assert_eq!(s.shown(), Color::RED);
    s.host.hold_bakes(true);
    s.host.frame();
    s.host.frame();
    assert_eq!(s.host.deliver_bakes(), 0, "no new try until the content changes");
    s.margin.set(0.0);
    s.change(Color::BLUE);
    s.host.hold_bakes(false);
    s.host.settle();
    assert_eq!(s.shown(), Color::BLUE);
}

#[test]
fn caches_inside_go_into_the_bitmap_painted_live() {
    let child = SkiaLayout::new().width_request(40).height_request(40).background_color(Color::GREEN).use_cache(CacheType::Image);
    let child_id = child.id();
    let parent = SkiaLayout::new().width_request(100).height_request(60).use_cache(CacheType::ImageDoubleBuffered).children(child);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(parent)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    assert_eq!(host.pixel(20, 20), Color::GREEN);
    // A GPU texture cannot go to a worker: the child's Image cache is never made for it.
    assert_eq!(host.cache_records(child_id), 0);
}

#[test]
fn a_bitmap_that_is_back_is_drawn_again_by_the_caches_above() {
    let mut s = {
        let (color, margin) = (Rc::new(Cell::new(Color::RED)), Rc::new(Cell::new(0.0)));
        let probe = Build::new(Probe { color: color.clone(), margin: margin.clone() })
            .margin(Thickness::new(50.0, 50.0, 0.0, 0.0))
            .use_cache(CacheType::ImageDoubleBuffered);
        let id = probe.id();
        let ui = Ui::new((), |_| SkiaLayout::new().fill().use_cache(CacheType::Image).children(probe)).background(Color::WHITE);
        Scene { host: Headless::new(ui, 200, 100, 1.0), id, color, margin }
    };
    s.host.settle();
    s.change(Color::BLUE);
    s.host.settle();
    assert_eq!(s.shown(), Color::BLUE, "the parent's Image cache was recorded again with it");
}

#[test]
fn without_bake_workers_it_is_made_in_the_frame_as_image() {
    let mut s = scene();
    s.host.without_bake_workers();
    s.host.frame();
    assert_eq!(s.shown(), Color::RED, "no placeholder in the browser");
    s.change(Color::BLUE);
    s.host.frame();
    assert_eq!(s.shown(), Color::BLUE);
    assert_eq!(s.host.deliver_bakes(), 0);
}
