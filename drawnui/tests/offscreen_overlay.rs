//! A child laid out at a scale of its own (`LayoutCx::measure_child_scaled`, a video recording's
//! overlay) is drawn only into pictures (`PaintCx::record_child`): never on screen, no gestures,
//! no accessibility node. The picture follows the child's `content_epoch`.

use std::cell::{Cell, RefCell};

use drawnui::prelude::*;
use drawnui::skia::Picture;
use drawnui::testing::Headless;

/// The recording frame: 200 x 100 pixels at scale 2.
const FRAME: (f32, f32, f32) = (200.0, 100.0, 2.0);

/// Lays its only child out at the recording size and shows the child's picture scaled down to
/// half (the preview), recorded again only when the child changed.
#[derive(Default)]
struct Recorder {
    picture: RefCell<Option<(Picture, u32)>>,
    records: Cell<u32>,
}

impl Container for Recorder {}

impl Control for Recorder {
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        if cx.child_count() > 0 {
            let overlay = cx.child(0);
            cx.measure_child_scaled(overlay, FRAME.0, FRAME.1, FRAME.2);
        }
        Size::new(width, height)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        if cx.child_count() > 0 {
            let overlay = cx.child(0);
            cx.arrange_child_scaled(overlay, Rect::from_wh(FRAME.0, FRAME.1), FRAME.2);
        }
    }

    fn paint(&self, cx: &mut PaintCx) {
        if cx.child_count() == 0 {
            return;
        }
        let overlay = cx.child(0);
        let epoch = cx.child_base(overlay).content_epoch();
        if self.picture.borrow().as_ref().is_none_or(|(_, e)| *e != epoch) {
            let picture = cx.record_child(overlay, Rect::from_wh(FRAME.0, FRAME.1)).expect("picture");
            *self.picture.borrow_mut() = Some((picture, epoch));
            self.records.set(self.records.get() + 1);
        }
        let picture = self.picture.borrow();
        let (picture, _) = picture.as_ref().expect("recorded");
        cx.canvas.save();
        cx.canvas.translate((cx.rect.left, cx.rect.top));
        cx.canvas.scale((0.5, 0.5));
        cx.canvas.draw_picture(picture, None, None);
        cx.canvas.restore();
        // Painting the children as a layout would draws nothing: the overlay is off screen.
        cx.paint_children();
    }
}

#[derive(Default)]
struct App {
    recorder: Handle<Recorder>,
    badge: Handle<SkiaShape>,
    taps: u32,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        Build::new(Recorder::default()).fill().assign(&mut app.recorder).children(
            SkiaLayout::new().fill().children(
                // 50 x 25 points = 100 x 50 recording pixels; 50 x 25 screen pixels in the preview.
                SkiaShape::new()
                    .width_request(50)
                    .height_request(25)
                    .background_color(Color::RED)
                    .accessibility_role(Aria::BUTTON)
                    .accessibility_label("REC")
                    .assign(&mut app.badge)
                    .on_tapped(|_me, app: &mut App, _cx| app.taps += 1),
            ),
        )
    })
    .background(Color::BLACK);
    let mut h = Headless::new(ui, 300, 200, 1.0);
    h.ui.set_accessibility_enabled(true);
    h.settle();
    h
}

fn records(h: &Headless<App>) -> u32 {
    h.ui.tree.find::<Recorder>(h.ui.state.recorder).expect("recorder").records.get()
}

#[test]
fn the_overlay_is_laid_out_at_its_own_scale_and_drawn_only_through_its_picture() {
    let mut h = host();
    let badge = h.ui.tree.base(h.ui.state.badge).unwrap();
    assert_eq!((badge.rect.width(), badge.rect.height()), (100.0, 50.0), "recording pixels at scale 2");
    // The preview shows the picture at half size: 50 x 25 screen pixels.
    assert_eq!(h.pixel(10, 10), Color::RED);
    assert_eq!(h.pixel(75, 10), Color::BLACK, "the overlay itself is not painted on screen");
}

#[test]
fn the_overlay_takes_no_taps_and_has_no_accessibility_node() {
    let mut h = host();
    h.tap(10.0, 10.0);
    h.tap(75.0, 20.0);
    assert_eq!(h.ui.state.taps, 0);
    assert!(h.ui.accessibility_nodes().iter().all(|n| n.label != "REC"));
}

#[test]
fn the_picture_is_recorded_again_only_when_the_overlay_changed() {
    let mut h = host();
    let before = records(&h);
    h.ui.state_changed();
    h.frame();
    assert_eq!(records(&h), before, "nothing changed: the picture is reused");

    let badge = h.ui.state.badge;
    h.ui.tree.get_mut(badge).unwrap().set_background_color(Color::GREEN);
    h.settle();
    assert_eq!(records(&h), before + 1);
    assert_eq!(h.pixel(10, 10), Color::GREEN);
}
