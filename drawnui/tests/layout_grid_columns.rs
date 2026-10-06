//! A Column with `split` > 1: rows of equal columns, the grid upstream builds from a stack.
//! `column_split2_non_templated_second_column_starts_at_slot` is the port of the C#
//! `StackWrapSweepTests` case; every other rect, size and constraint was read from the C# engine
//! with the same tree.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;

const INF: f32 = f32::INFINITY;
const START: LayoutOptions = LayoutOptions::Start;
const CENTER: LayoutOptions = LayoutOptions::Center;
const END: LayoutOptions = LayoutOptions::End;
const FILL: LayoutOptions = LayoutOptions::Fill;

/// Test root: measures its child inside the given constraints and arranges it at the origin.
struct Probe(f32, f32);

impl Container for Probe {}

impl Control for Probe {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        let child = cx.child(0);
        cx.measure_child(child, self.0, self.1)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let child = cx.child(0);
        let size = cx.child_base(child).measured;
        let side = |given: f32, measured: f32| if given.is_finite() { given } else { measured };
        cx.arrange_child(child, Rect::from_wh(side(self.0, size.width), side(self.1, size.height)));
    }
}

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

/// C# `Box`: a bare control; -1 leaves that size unset.
fn boxed(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// A card that fills its column and is as tall as what it holds.
fn card(height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().fill_x().children(boxed(60, height))
}

fn column(horizontal: LayoutOptions, split: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().layout_type(LayoutType::Column).spacing(10).split(split).horizontal_options(horizontal)
}

struct Laid {
    host: Headless<()>,
    stack: ControlId,
    items: Vec<ControlId>,
}

impl Laid {
    fn rects(&self) -> Vec<Rect> {
        self.items.iter().map(|id| self.host.rect(*id)).collect()
    }
    fn measured(&self, index: usize) -> Size {
        self.host.ui.tree.base(self.items[index]).unwrap().measured
    }
    fn size(&self) -> Size {
        self.host.ui.tree.base(self.stack).unwrap().measured
    }
}

/// C# `stack.Measure(width, height, 1)`, then an arrange at the origin.
fn lay_out(width: f32, height: f32, stack: Build<SkiaLayout>, items: impl IntoChildren) -> Laid {
    let stack_id = stack.id();
    let root = Build::new(Probe(width, height)).children(stack.children(items));
    let mut host = Headless::new(Ui::new((), |_| root), 1000, 1000, 1.0);
    host.settle();
    let items = host.ui.tree.children(stack_id).to_vec();
    Laid { host, stack: stack_id, items }
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

fn size(width: i32, height: i32) -> Size {
    Size::new(width as f32, height as f32)
}

#[test]
fn column_split2_non_templated_second_column_starts_at_slot() {
    let laid = lay_out(410.0, INF, column(FILL, 2), (boxed(50, 20), boxed(50, 20), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(210, 0, 260, 20), ltrb(0, 30, 50, 50)]);
    assert_eq!(laid.size(), size(410, 50));
}

#[test]
fn cards_fill_their_column_and_a_row_is_as_tall_as_its_tallest_card() {
    let laid = lay_out(410.0, INF, column(FILL, 2), (card(40), card(70), card(30)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 40), ltrb(210, 0, 410, 70), ltrb(0, 80, 200, 110)]);
    assert_eq!(laid.size(), size(410, 110));

    let laid = lay_out(320.0, INF, column(FILL, 3), (card(40), card(70), card(30), card(30)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 40), ltrb(110, 0, 210, 70), ltrb(220, 0, 320, 30), ltrb(0, 80, 100, 110)]);

    // Padding narrows the columns.
    let laid = lay_out(360.0, INF, column(FILL, 2).padding(16), (card(40), card(70), card(30)));
    assert_eq!(laid.rects(), [ltrb(16, 16, 175, 56), ltrb(185, 16, 344, 86), ltrb(16, 96, 175, 126)]);
    assert_eq!(laid.size(), size(360, 142));

    // A half pixel goes to the even side: 301 / 2 -> 150.
    let laid = lay_out(301.0, INF, column(FILL, 2).spacing(0), (card(40), card(70)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 150, 40), ltrb(150, 0, 300, 70)]);

    // Margins are inside the column.
    let items = (boxed(-1, 20).fill_x().margin(5), boxed(-1, 20).fill_x().margin(5));
    let laid = lay_out(410.0, INF, column(FILL, 2), items);
    assert_eq!((laid.measured(0), laid.measured(1)), (size(200, 30), size(200, 30)));
    assert_eq!(laid.rects(), [ltrb(5, 5, 195, 25), ltrb(215, 5, 405, 25)]);
}

#[test]
fn a_child_sits_in_its_column_by_its_horizontal_options_and_at_the_top_of_its_row() {
    let items =
        (boxed(50, 20).horizontal_options(CENTER), boxed(50, 20).horizontal_options(END), boxed(50, 20).horizontal_options(CENTER));
    let laid = lay_out(410.0, INF, column(FILL, 2), items);
    assert_eq!(laid.rects(), [ltrb(75, 0, 125, 20), ltrb(360, 0, 410, 20), ltrb(75, 30, 125, 50)]);

    let items = (boxed(50, 60), boxed(50, 20).vertical_options(CENTER), boxed(50, 60), boxed(50, 20).vertical_options(END));
    let laid = lay_out(410.0, INF, column(FILL, 2), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 60), ltrb(210, 0, 260, 20), ltrb(0, 70, 50, 130), ltrb(210, 70, 260, 90)]);
    assert_eq!(laid.size(), size(410, 130));
}

#[test]
fn dynamic_columns_share_the_width_among_the_children_of_a_short_last_row() {
    let laid = lay_out(410.0, INF, column(FILL, 2).dynamic_columns(true), (card(40), card(70), card(30)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 40), ltrb(210, 0, 410, 70), ltrb(0, 80, 410, 110)]);
    assert_eq!(laid.measured(2), size(410, 30));

    // Five children in three columns: the last two get (320 - 10) / 2 each.
    let items = (
        boxed(50, 20),
        boxed(50, 20),
        boxed(50, 20),
        boxed(50, 20).horizontal_options(CENTER),
        boxed(50, 20).horizontal_options(END),
    );
    let laid = lay_out(320.0, INF, column(FILL, 3).dynamic_columns(true), items);
    let expected =
        [ltrb(0, 0, 50, 20), ltrb(110, 0, 160, 20), ltrb(220, 0, 270, 20), ltrb(53, 30, 103, 50), ltrb(270, 30, 320, 50)];
    assert_eq!(laid.rects(), expected);
}

#[test]
fn a_column_that_takes_the_size_of_its_content() {
    // As wide as its widest row of children; the columns still come from the width it is offered.
    let laid = lay_out(410.0, INF, column(START, 2), (boxed(50, 20), boxed(70, 30), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(210, 0, 280, 30), ltrb(0, 40, 50, 60)]);
    assert_eq!(laid.size(), size(130, 60));
    // Only children that fill: the width it is offered.
    assert_eq!(lay_out(410.0, INF, column(START, 2), (card(40), card(70), card(30))).size(), size(410, 110));
}

#[test]
fn a_row_is_measured_with_the_height_under_it() {
    let seen = Seen::default();
    let spy = Build::new(Spy { layout: SkiaLayout::default(), seen: seen.clone() }).children(boxed(60, 30));
    let laid = lay_out(410.0, 300.0, column(FILL, 2), (boxed(50, 40), boxed(50, 20), boxed(50, 20), spy));
    assert_eq!(*seen.borrow(), [(200.0, 250.0)]);
    assert_eq!(laid.rects()[3], ltrb(210, 50, 270, 80));

    // A row past the end is cut. (The fifth child has no room at all: empty here, upstream keeps
    // its size and draws it below the column.)
    let items = (boxed(50, 60), boxed(50, 40), boxed(50, 60), boxed(50, 40), boxed(50, 20));
    let laid = lay_out(410.0, 100.0, column(FILL, 2), items);
    assert_eq!(laid.rects()[..4], [ltrb(0, 0, 50, 60), ltrb(210, 0, 260, 40), ltrb(0, 70, 50, 100), ltrb(210, 70, 260, 100)]);
    assert_eq!(laid.size(), size(410, 100));
}

#[test]
fn nothing_is_measured_while_nothing_changes() {
    let logs: Vec<Seen> = (0..4).map(|_| Seen::default()).collect();
    let spies: Vec<_> = logs
        .iter()
        .map(|log| Build::new(Spy { layout: SkiaLayout::default(), seen: log.clone() }).children(boxed(60, 30)))
        .collect();
    let mut laid = lay_out(410.0, INF, column(FILL, 2), spies);
    let counts = || logs.iter().map(|log| log.borrow().len()).collect::<Vec<_>>();
    assert_eq!(counts(), [1, 1, 1, 1]);
    laid.host.frame();
    laid.host.frame();
    assert_eq!(counts(), [1, 1, 1, 1]);
    // The last child changes: the others keep their column and the height they were offered.
    laid.host.ui.tree.find_mut::<Spy>(laid.items[3]).unwrap().set_minimum_height_request(50);
    laid.host.settle();
    assert_eq!(counts(), [1, 1, 1, 2]);
}
