//! A stack measures its children unbounded along it, as DrawnUi.React (SkiaLayout.MeasureAbsolute):
//! a child keeps its content size there and is never cut by what is left (C# measures it with
//! what is left and cuts it). Kept from C#: no measure is larger than its constraint
//! (AdaptWidthConstraintToRequest), so an auto stack whose children overflow is measured at its
//! constraint (React: at its content), and a child that fills the main axis of a stack whose main
//! axis is set takes a share of what the others leave (React: its content). Sizes, rects and
//! constraints were read from the React engine with the same trees (`dist` in node), except where a
//! comment names the C# rule that was kept.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const START: LayoutOptions = LayoutOptions::Start;
const FILL: LayoutOptions = LayoutOptions::Fill;
const INF: f32 = f32::INFINITY;

type Seen = Rc<RefCell<Vec<(f32, f32)>>>;

/// An Absolute layout that records the constraints of every content measure it runs.
struct Spy {
    layout: SkiaLayout,
    seen: Seen,
}

impl Container for Spy {}

impl Control for Spy {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.seen.borrow_mut().push((width, height));
        self.layout.measure(cx, width, height)
    }
}

fn spy(seen: &Seen) -> Build<Spy> {
    Build::new(Spy { layout: SkiaLayout::default(), seen: seen.clone() })
}

/// C# `Box`: a bare control; -1 leaves that size unset.
fn boxed(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

fn stack(kind: LayoutType, horizontal: LayoutOptions, vertical: LayoutOptions) -> Build<SkiaLayout> {
    SkiaLayout::new().layout_type(kind).spacing(0).horizontal_options(horizontal).vertical_options(vertical)
}

/// Lays `root` out in a canvas of that size: the root is measured with it, like C# `root.Measure(w, h, 1)`.
fn lay_out(width: i32, height: i32, root: impl Into<drawnui::Detached>) -> Headless<()> {
    let mut host = Headless::new(Ui::new((), |_| root).font_bytes("Default", FONT), width, height, 1.0);
    host.settle();
    host
}

fn measured(host: &Headless<()>, id: ControlId) -> Size {
    host.ui.tree.base(id).unwrap().measured
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

fn size(width: i32, height: i32) -> Size {
    Size::new(width as f32, height as f32)
}

// ---------------------------------------------------------------- remaining space

#[test]
fn a_row_child_is_measured_unbounded_along_the_row() {
    let seen = Seen::default();
    lay_out(300, 200, stack(LayoutType::Row, START, START).children((boxed(100, 40), spy(&seen).children(boxed(60, 40)))));
    assert_eq!(*seen.borrow(), [(INF, 200.0)]);

    let seen = Seen::default();
    lay_out(300, 200, stack(LayoutType::Row, START, START).spacing(10).children((boxed(100, 40), spy(&seen).children(boxed(60, 40)))));
    assert_eq!(*seen.borrow(), [(INF, 200.0)]);

    let seen = Seen::default();
    let (watched, last) = (spy(&seen).children(boxed(60, 40)), boxed(30, 40));
    let (watched_id, last_id) = (watched.id(), last.id());
    let row = stack(LayoutType::Row, FILL, START).spacing(10).children((boxed(100, 40), boxed(50, 40), watched, last));
    let row_id = row.id();
    let host = lay_out(300, 200, row);
    assert_eq!(*seen.borrow(), [(INF, 200.0)]);
    assert_eq!(measured(&host, row_id), size(300, 40));
    assert_eq!(host.rect(watched_id), ltrb(170, 0, 230, 40));
    assert_eq!(host.rect(last_id), ltrb(240, 0, 270, 40));
}

/// A Row gives its children an unbounded width, so a label in it does not wrap (the React page
/// says so: use a grid with a star column for text that must wrap).
#[test]
fn a_label_in_a_row_does_not_wrap() {
    let label = SkiaLabel::new("The quick brown fox jumps over the lazy dog again and again").font_size(16);
    let id = label.id();
    let host = lay_out(300, 200, stack(LayoutType::Row, START, START).children((boxed(100, 40), label)));
    let label = host.ui.tree.find::<SkiaLabel>(id).unwrap();
    assert_eq!(label.lines_count(), 1);
    assert!(measured(&host, id).width > 200.0, "{} px", measured(&host, id).width);
    assert_eq!(host.rect(id).left, 100.0);
}

#[test]
fn a_row_child_past_the_end_keeps_its_size() {
    let boxes: Vec<_> = (0..4).map(|_| boxed(100, 40)).collect();
    let ids: Vec<_> = boxes.iter().map(|b| b.id()).collect();
    let row = stack(LayoutType::Row, START, START).children(boxes);
    let row_id = row.id();
    let host = lay_out(250, 300, row);
    let sizes: Vec<_> = ids.iter().map(|id| measured(&host, *id)).collect();
    assert_eq!(sizes, [size(100, 40); 4]);
    // C# rule kept: the row is measured at its constraint (React: 400).
    assert_eq!(measured(&host, row_id), size(250, 40));
    assert_eq!(host.rect(ids[2]), ltrb(200, 0, 300, 40));
    assert_eq!(host.rect(ids[3]), ltrb(300, 0, 400, 40));

    let boxes: Vec<_> = (0..3).map(|_| boxed(100, 40)).collect();
    let ids: Vec<_> = boxes.iter().map(|b| b.id()).collect();
    let row = stack(LayoutType::Row, START, START).spacing(10).children(boxes);
    let row_id = row.id();
    let host = lay_out(250, 300, row);
    assert_eq!(measured(&host, ids[2]), size(100, 40));
    assert_eq!(host.rect(ids[2]), ltrb(220, 0, 320, 40));
    assert_eq!(measured(&host, row_id), size(250, 40));
}

#[test]
fn a_column_child_past_the_end_keeps_its_size() {
    let boxes: Vec<_> = (0..4).map(|_| boxed(100, 100)).collect();
    let ids: Vec<_> = boxes.iter().map(|b| b.id()).collect();
    let column = stack(LayoutType::Column, FILL, START).children(boxes);
    let column_id = column.id();
    let host = lay_out(300, 250, column);
    let sizes: Vec<_> = ids.iter().map(|id| measured(&host, *id)).collect();
    assert_eq!(sizes, [size(100, 100); 4]);
    // C# rule kept: the column is measured at its constraint (React: 400).
    assert_eq!(measured(&host, column_id), size(300, 250));
    assert_eq!(host.rect(ids[2]), ltrb(0, 200, 100, 300));
    assert_eq!(host.rect(ids[3]), ltrb(0, 300, 100, 400));

    let boxes: Vec<_> = (0..3).map(|_| boxed(100, 100)).collect();
    let last = boxes[2].id();
    let host = lay_out(300, 250, stack(LayoutType::Column, FILL, START).spacing(10).children(boxes));
    assert_eq!(measured(&host, last), size(100, 100));
    assert_eq!(host.rect(last), ltrb(0, 220, 100, 320));

    // An auto-sized child keeps what it holds too.
    let (inner, after) = (boxed(80, 300), boxed(100, 50));
    let (inner_id, after_id) = (inner.id(), after.id());
    let panel = SkiaLayout::new().children(inner);
    let panel_id = panel.id();
    let host = lay_out(300, 250, stack(LayoutType::Column, FILL, START).children((boxed(100, 100), panel, after)));
    assert_eq!(measured(&host, panel_id), size(80, 300));
    assert_eq!(measured(&host, inner_id), size(80, 300));
    assert_eq!(host.rect(panel_id), ltrb(0, 100, 80, 400));
    assert_eq!((measured(&host, after_id), host.rect(after_id)), (size(100, 50), ltrb(0, 400, 100, 450)));
}

#[test]
fn nested_stacks_measure_their_children_unbounded_along_them() {
    // Column > Row: the row is offered an unbounded height, its child an unbounded width.
    let seen = Seen::default();
    let row = stack(LayoutType::Row, START, START).children((boxed(100, 40), spy(&seen).children(boxed(60, 40))));
    let row_id = row.id();
    let column = stack(LayoutType::Column, FILL, START).children((boxed(-1, 100).fill_x(), row));
    let column_id = column.id();
    let host = lay_out(300, 500, column);
    assert_eq!(*seen.borrow(), [(INF, INF)]);
    assert_eq!(measured(&host, row_id), size(160, 40));
    assert_eq!(host.rect(row_id), ltrb(0, 100, 160, 140));
    assert_eq!(measured(&host, column_id), size(300, 140));

    // Column > Column.
    let seen = Seen::default();
    let inner = stack(LayoutType::Column, START, START).children((boxed(50, 50), spy(&seen).children(boxed(60, 40))));
    let inner_id = inner.id();
    let host = lay_out(300, 500, stack(LayoutType::Column, FILL, START).children((boxed(-1, 100).fill_x(), inner)));
    assert_eq!(*seen.borrow(), [(300.0, INF)]);
    assert_eq!(measured(&host, inner_id), size(60, 90));

    // Row > Column.
    let seen = Seen::default();
    let inner = stack(LayoutType::Column, START, START).children((boxed(50, 50), spy(&seen).children(boxed(60, 40))));
    let inner_id = inner.id();
    let row = stack(LayoutType::Row, FILL, START).children((boxed(100, 40), inner));
    let row_id = row.id();
    let host = lay_out(300, 500, row);
    assert_eq!(*seen.borrow(), [(INF, INF)]);
    assert_eq!(host.rect(inner_id), ltrb(100, 0, 160, 90));
    assert_eq!(measured(&host, row_id), size(300, 90));

    // Past the end of the outer column nothing is cut; the outer one is measured at its constraint
    // (C# rule kept; React: 430).
    let boxes: Vec<_> = (0..3).map(|_| boxed(50, 100)).collect();
    let ids: Vec<_> = boxes.iter().map(|b| b.id()).collect();
    let inner = stack(LayoutType::Column, START, START).children(boxes);
    let (inner_id, after) = (inner.id(), boxed(50, 30));
    let after_id = after.id();
    let column = stack(LayoutType::Column, FILL, START).children((boxed(-1, 100).fill_x(), inner, after));
    let column_id = column.id();
    let host = lay_out(300, 250, column);
    let sizes: Vec<_> = ids.iter().map(|id| measured(&host, *id)).collect();
    assert_eq!(sizes, [size(50, 100); 3]);
    assert_eq!(measured(&host, inner_id), size(50, 300));
    assert_eq!((measured(&host, after_id), host.rect(after_id)), (size(50, 30), ltrb(0, 400, 50, 430)));
    assert_eq!(measured(&host, column_id), size(300, 250));
}

#[test]
fn a_main_axis_fill_child_shares_only_on_a_set_main_axis() {
    // Auto height: the Fill child has its content height, the last child is not cut. The C# rule
    // kept (the column at its constraint) cuts the last child's rect at the end (React: 130..330).
    let (panel, last) = (SkiaLayout::new().fill().children(boxed(80, 30)), boxed(-1, 200).fill_x());
    let (panel_id, last_id) = (panel.id(), last.id());
    let host = lay_out(300, 250, stack(LayoutType::Column, FILL, START).children((boxed(-1, 100).fill_x(), panel, last)));
    assert_eq!(measured(&host, panel_id), size(300, 30));
    assert_eq!(host.rect(panel_id), ltrb(0, 100, 300, 130));
    assert_eq!(measured(&host, last_id), size(300, 200));
    assert_eq!(host.rect(last_id), ltrb(0, 130, 300, 250));

    // A child between others, no Fill sibling: unbounded.
    let seen = Seen::default();
    let column = stack(LayoutType::Column, FILL, FILL).spacing(10);
    lay_out(300, 500, column.children((boxed(100, 40), spy(&seen).children(boxed(60, 40)), boxed(100, 40))));
    assert_eq!(*seen.borrow(), [(300.0, INF)]);

    // A Row of a set width: the Fill child gets the rest (C# rule kept; React gives it its content,
    // nothing here, and puts the last child at 100..160).
    let seen = Seen::default();
    let (fill, watched) = (boxed(-1, 40).fill_x(), spy(&seen).children(boxed(60, 40)));
    let (fill_id, watched_id) = (fill.id(), watched.id());
    let host = lay_out(400, 200, stack(LayoutType::Row, FILL, START).children((boxed(100, 40), fill, watched)));
    assert_eq!(*seen.borrow(), [(INF, 200.0)]);
    assert_eq!(host.rect(fill_id), ltrb(100, 0, 340, 40));
    assert_eq!(host.rect(watched_id), ltrb(340, 0, 400, 40));
}

// ---------------------------------------------------------------- never larger than the constraint

#[test]
fn a_measure_is_never_larger_than_its_constraint() {
    // Auto size: cut at the constraint, and so is the content.
    let inner = boxed(80, 40);
    let inner_id = inner.id();
    let panel = SkiaLayout::new().children(inner);
    let panel_id = panel.id();
    let host = lay_out(50, 100, panel);
    assert_eq!((measured(&host, panel_id), measured(&host, inner_id)), (size(50, 40), size(50, 40)));

    // The margin is part of the measured size.
    let panel = SkiaLayout::new().margin((10, 0, 10, 0)).children(boxed(95, 40));
    let id = panel.id();
    assert_eq!(measured(&lay_out(100, 100, panel), id), size(100, 40));
    let panel = SkiaLayout::new().margin((10, 0, 10, 0)).children(boxed(70, 40));
    let id = panel.id();
    assert_eq!(measured(&lay_out(100, 100, panel), id), size(90, 40));

    // A size request is cut too.
    let item = boxed(100, 40).margin((10, 0, 10, 0));
    let id = item.id();
    assert_eq!(measured(&lay_out(100, 100, item), id), size(100, 40));
    let item = boxed(50, 300);
    let id = item.id();
    assert_eq!(measured(&lay_out(100, 120, item), id), size(50, 120));
    let item = boxed(300, 50);
    let id = item.id();
    assert_eq!(measured(&lay_out(1000, 1000, boxed(200, 100).children(item)), id), size(200, 50));

    // A minimum wins over the constraint.
    let item = boxed(-1, 40).minimum_width_request(150);
    let id = item.id();
    assert_eq!(measured(&lay_out(100, 100, item), id), size(150, 40));
    let item = boxed(200, 40).minimum_width_request(150);
    let id = item.id();
    assert_eq!(measured(&lay_out(100, 100, item), id), size(150, 40));

    // An auto row inside a fixed box: the second child keeps its size, the row is cut at the box
    // (React: 200).
    let second = boxed(100, 40);
    let second_id = second.id();
    let row = stack(LayoutType::Row, START, START).children((boxed(100, 40), second));
    let row_id = row.id();
    let host = lay_out(1000, 1000, boxed(150, 100).children(row));
    assert_eq!((measured(&host, second_id), measured(&host, row_id)), (size(100, 40), size(150, 40)));
}

// ---------------------------------------------------------------- the measure memo

/// A row of spies, 40 x 20 each. Returns the host, the ids and one measure log per spy.
fn spies(count: usize) -> (Headless<()>, Vec<ControlId>, Vec<Seen>) {
    let logs: Vec<Seen> = (0..count).map(|_| Seen::default()).collect();
    let items: Vec<_> = logs.iter().map(|log| spy(log).width_request(40).height_request(20)).collect();
    let ids = items.iter().map(|item| item.id()).collect();
    (lay_out(1000, 100, stack(LayoutType::Row, START, START).children(items)), ids, logs)
}

fn counts(logs: &[Seen]) -> Vec<usize> {
    logs.iter().map(|log| log.borrow().len()).collect()
}

#[test]
fn nothing_is_measured_again_while_nothing_changes() {
    let (mut host, ids, logs) = spies(3);
    assert_eq!(counts(&logs), [1, 1, 1]);
    host.frame();
    host.frame();
    assert_eq!(counts(&logs), [1, 1, 1]);

    // A repaint of a sibling measures nothing.
    host.ui.tree.find_mut::<Spy>(ids[1]).unwrap().set_opacity(0.5);
    host.settle();
    assert_eq!(counts(&logs), [1, 1, 1]);

    // A sibling changes its size: no other one is measured, their constraint is unbounded.
    host.ui.tree.find_mut::<Spy>(ids[2]).unwrap().set_width_request(60);
    host.settle();
    assert_eq!(counts(&logs), [1, 1, 2]);
    host.ui.tree.find_mut::<Spy>(ids[1]).unwrap().set_width_request(60);
    host.settle();
    assert_eq!(counts(&logs), [1, 2, 2]);

    host.ui.tree.invalidate(ids[0], Dirty::MEASURE);
    host.settle();
    assert_eq!(counts(&logs), [2, 2, 2]);
}

/// The unbounded main axis keeps a size change local: only the changed child is measured again
/// (with C#'s remaining-space rule every child after it was).
#[test]
fn a_size_change_measures_only_the_changed_child_again() {
    let (mut host, ids, logs) = spies(20);
    assert_eq!(counts(&logs), [1; 20]);

    host.ui.tree.find_mut::<Spy>(ids[0]).unwrap().set_width_request(50);
    host.settle();
    let mut expected = [1; 20];
    expected[0] = 2;
    assert_eq!(counts(&logs), expected);

    host.ui.tree.find_mut::<Spy>(ids[19]).unwrap().set_width_request(50);
    host.settle();
    expected[19] = 2;
    assert_eq!(counts(&logs), expected);
}

/// The frame that applies a change lays out and paints it and asks for no other frame (the commit
/// used to ask for one more whenever it had flushed something: two frames per change).
#[test]
fn one_change_takes_one_frame() {
    let item = boxed(40, 20);
    let id = item.id();
    let mut host = lay_out(300, 200, stack(LayoutType::Row, START, START).children(item));
    assert!(!host.ui.needs_frame());
    host.ui.tree.find_mut::<SkiaLayout>(id).unwrap().set_width_request(60);
    assert!(host.ui.needs_frame());
    host.frame();
    assert_eq!(host.rect(id), ltrb(0, 0, 60, 20));
    assert!(!host.ui.needs_frame(), "a second frame for one measure change");
    host.ui.tree.find_mut::<SkiaLayout>(id).unwrap().set_opacity(0.5);
    host.frame();
    assert!(!host.ui.needs_frame(), "a second frame for one draw change");
}
