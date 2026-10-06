//! Scrolling at engine level: a control moves its children at paint time with `content_offset`.
//! Layout does not run, hit testing follows, and only controls that track the viewport are
//! arranged again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::prelude::*;
use drawnui::testing::Headless;

const COLORS: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

/// A container that pans its content with the finger: the smallest possible scroll.
#[derive(Default)]
struct Viewport {
    layout: SkiaLayout,
}
impl Container for Viewport {}
impl Control for Viewport {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    /// Like a vertical scroll: the content is as tall as it needs, whatever the viewport is.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let content = cx.child(0);
        cx.measure_child(content, width, f32::INFINITY);
        Size::new(width, height)
    }
    fn arrange(&mut self, cx: &mut LayoutCx) {
        let content = cx.child(0);
        let (rect, height) = (cx.base().rect, cx.child_base(content).measured.height);
        cx.arrange_child(content, Rect::new(rect.left, rect.top, rect.right, rect.top + height));
    }
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if gesture.kind != GestureKind::Panning {
            return Handled::No;
        }
        let offset = cx.base().content_offset + gesture.delta;
        let id = cx.id;
        cx.cx().set_content_offset(id, offset);
        Handled::Yes
    }
}

/// Counts its arranges; `tracks` makes it follow the viewport like a virtualized list.
struct Counter {
    tracks: bool,
    arranges: Arc<AtomicU32>,
}
impl Control for Counter {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        if self.tracks {
            cx.track_viewport();
        }
        Size::new(10.0, 10.0)
    }
    fn arrange(&mut self, _cx: &mut LayoutCx) {
        self.arranges.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct App {
    tapped: Vec<usize>,
    viewport: Handle<Viewport>,
    boxes: [Handle<SkiaShape>; 4],
    tracker_arranges: Arc<AtomicU32>,
    plain_arranges: Arc<AtomicU32>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    let boxes: Vec<Build<SkiaShape>> = (0..4)
        .map(|i| {
            SkiaShape::new()
                .fill_x()
                .height_request(100)
                .background_color(COLORS[i])
                .assign(&mut app.boxes[i])
                .on_tapped(move |_me, app: &mut App, _cx| app.tapped.push(i))
        })
        .collect();
    SkiaLayout::new().fill().children((Build::new(Viewport::default())
        .width_request(200)
        .height_request(200)
        .is_clipped_to_bounds(true)
        .assign(&mut app.viewport)
        .children((SkiaLayout::column().spacing(0).children((
            boxes,
            Build::new(Counter { tracks: true, arranges: app.tracker_arranges.clone() }),
            Build::new(Counter { tracks: false, arranges: app.plain_arranges.clone() }),
        )),)),))
}

fn host() -> Headless<App> {
    let mut host = Headless::new(Ui::new(App::default(), build).background(Color::BLACK), 300, 300, 1.0);
    host.settle();
    host
}

#[test]
fn content_offset_moves_pixels_and_hit_testing_but_not_the_layout() {
    let mut host = host();
    let (viewport, boxes) = (host.ui.state.viewport, host.ui.state.boxes);
    assert_eq!(host.pixel(10, 10), COLORS[0]);
    assert_eq!(host.rect(boxes[3]).top, 300.0);
    // Below the viewport everything is clipped.
    assert_eq!(host.pixel(10, 250), Color::BLACK);

    host.ui.tree.set_content_offset(viewport, Point::new(0.0, -250.0));
    host.settle();
    // Rects are untouched; the pixels moved up by 250.
    assert_eq!(host.rect(boxes[3]).top, 300.0);
    assert_eq!(host.pixel(10, 10), COLORS[2]);
    assert_eq!(host.pixel(10, 60), COLORS[3]);
    assert_eq!(host.pixel(10, 250), Color::BLACK);

    // A tap goes to what is drawn under the pointer.
    host.tap(10.0, 60.0);
    assert_eq!(host.ui.state.tapped, [3]);
    host.tap(10.0, 10.0);
    assert_eq!(host.ui.state.tapped, [3, 2]);
    // Outside the viewport nothing is hit, although box 2 has its rect there.
    host.tap(10.0, 250.0);
    assert_eq!(host.ui.state.tapped, [3, 2]);
}

#[test]
fn visible_rect_follows_the_offset() {
    let mut host = host();
    let (viewport, boxes) = (host.ui.state.viewport, host.ui.state.boxes);
    assert_eq!(host.ui.tree.visible_rect(boxes[0]), Rect::new(0.0, 0.0, 200.0, 100.0));
    assert!(host.ui.tree.visible_rect(boxes[3]).is_empty());

    host.ui.tree.set_content_offset(viewport, Point::new(0.0, -250.0));
    host.settle();
    assert!(host.ui.tree.visible_rect(boxes[0]).is_empty());
    assert_eq!(host.ui.tree.visible_rect(boxes[2]), Rect::new(0.0, 250.0, 200.0, 300.0));
    assert_eq!(host.ui.tree.visible_rect(boxes[3]), Rect::new(0.0, 300.0, 200.0, 400.0));
}

#[test]
fn panning_scrolls_and_only_viewport_trackers_are_arranged_again() {
    let mut host = host();
    let count = |counter: &Arc<AtomicU32>| counter.load(Ordering::Relaxed);
    let (tracker_before, plain_before) = (count(&host.ui.state.tracker_arranges), count(&host.ui.state.plain_arranges));

    host.pan((100.0, 150.0), (100.0, 50.0), 64.0, 4);
    let viewport = host.ui.state.viewport;
    assert_eq!(host.ui.tree.base(viewport).unwrap().content_offset, Point::new(0.0, -100.0));
    assert_eq!(host.pixel(10, 10), COLORS[1]);
    // The pan was the viewport's, not a tap on a box.
    assert!(host.ui.state.tapped.is_empty());

    // One arrange per scrolled frame for the tracker, none for anything else.
    assert_eq!(count(&host.ui.state.tracker_arranges) - tracker_before, 4);
    assert_eq!(count(&host.ui.state.plain_arranges), plain_before);
}
