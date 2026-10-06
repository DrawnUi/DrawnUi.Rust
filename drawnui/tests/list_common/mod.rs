//! Shared by the list tests: the test-only scroll of `viewport.rs` plus what a scroll does with
//! the list's viewport shift, a control that counts its measures, and helpers to read the rows.
#![allow(dead_code)]

use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// A container that pans its content with the finger: the smallest possible scroll.
#[derive(Default)]
pub struct Viewport {
    layout: SkiaLayout,
    /// Ignores the shift the list reports, as a scroll that knows nothing about lists would.
    loose: bool,
}
impl Viewport {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(loose: bool) -> Build<Viewport> {
        Build::new(Viewport { layout: SkiaLayout::default(), loose })
    }
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
        // The content above the rows on screen grew by `shift`: moving the offset back by it
        // keeps those rows where they are.
        let shift = cx.take_viewport_shift(content);
        if shift != 0.0 && !self.loose {
            cx.base_mut().content_offset.y -= shift;
        }
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

/// What the tests count. Clones share the counters.
#[derive(Default, Clone)]
pub struct Counters {
    /// Cells made from the template.
    pub created: Rc<Cell<u32>>,
    pub binds: Rc<Cell<u32>>,
    /// Measures and arranges of the probes inside the cells.
    pub measures: Rc<Cell<u32>>,
    pub arranges: Rc<Cell<u32>>,
}

impl Counters {
    pub fn probe(&self) -> Build<Probe> {
        Build::new(Probe { measures: self.measures.clone(), arranges: self.arranges.clone() })
    }
    /// (created, binds, measures, arranges)
    pub fn read(&self) -> (u32, u32, u32, u32) {
        (self.created.get(), self.binds.get(), self.measures.get(), self.arranges.get())
    }
}

pub fn count(counter: &Rc<Cell<u32>>) {
    counter.set(counter.get() + 1);
}

/// Cell content that counts how often it is measured and arranged.
pub struct Probe {
    measures: Rc<Cell<u32>>,
    arranges: Rc<Cell<u32>>,
}
impl Control for Probe {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        count(&self.measures);
        Size::new(10.0, 10.0)
    }
    fn arrange(&mut self, _cx: &mut LayoutCx) {
        count(&self.arranges);
    }
}

/// A color per item, so a pixel tells which item a row shows.
pub fn color(item: usize) -> Color {
    Color::from_argb(255, (item * 53 % 256) as u8, (item * 97 % 256) as u8, 128 + (item % 2) as u8 * 100)
}

/// Scrolls so that `y` pixels of the content are above the viewport.
pub fn scroll_to<S>(host: &mut Headless<S>, viewport: Handle<Viewport>, y: f32) {
    host.ui.tree.set_content_offset(viewport, Point::new(0.0, -y));
}

/// Pixels of the content above the viewport.
pub fn scrolled<S>(host: &Headless<S>, viewport: Handle<Viewport>) -> f32 {
    -host.ui.tree.base(viewport).unwrap().content_offset.y
}

/// The rows that have a visible cell: (item index, top on screen relative to the viewport,
/// height), top to bottom.
pub fn rows<S>(host: &Headless<S>, viewport: Handle<Viewport>, list: Handle<SkiaLayout>) -> Vec<(usize, f32, f32)> {
    let tree = &host.ui.tree;
    let view = tree.base(viewport).unwrap();
    let offset = view.content_offset.y - view.rect.top;
    let mut rows: Vec<(usize, f32, f32)> = tree
        .children(list)
        .iter()
        .filter_map(|cell| tree.base(*cell))
        .filter(|base| base.p.is_visible)
        .filter_map(|base| Some((base.context_index?, base.rect.top + offset, base.rect.height())))
        .collect();
    rows.sort_by_key(|row| row.0);
    rows
}

/// Live cell nodes of a list, hidden spare ones included.
pub fn cells<S>(host: &Headless<S>, list: Handle<SkiaLayout>) -> usize {
    host.ui.tree.children(list).len()
}

pub fn layout<S>(host: &Headless<S>, list: Handle<SkiaLayout>) -> &SkiaLayout {
    host.ui.tree.find::<SkiaLayout>(list).unwrap()
}

/// Height of the list as its parents see it.
pub fn content_height<S>(host: &Headless<S>, list: Handle<SkiaLayout>) -> f32 {
    host.ui.tree.base(list).unwrap().measured.height
}
