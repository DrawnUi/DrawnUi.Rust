//! LayoutType::Wrap. The first four tests are ports of the C# UnitTests `StackWrapSweepTests`
//! (Wrap_*) and `LayoutSweepTests` (Arrange_FillAxisWiderThanMeasured); every other rect, size and
//! constraint was read from the C# engine with the same tree. C# lays a wrap out inside its box
//! shrunk by 1 px on every side; DrawnUi.React has no such inset and neither has this port, so the
//! numbers were read from the C# engine with that pixel given back (the probe's Wrap grows the box
//! it passes to MeasureWrap by 1 px on every side).

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::controls::layout::SkiaWrap;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const INF: f32 = f32::INFINITY;
const START: LayoutOptions = LayoutOptions::Start;
const CENTER: LayoutOptions = LayoutOptions::Center;
const END: LayoutOptions = LayoutOptions::End;
const FILL: LayoutOptions = LayoutOptions::Fill;

/// Test root: measures its child inside the given constraints and arranges it into the given box,
/// like the C# tests calling `Measure` and `Arrange` on a control directly.
struct Probe {
    constraints: (f32, f32),
    /// `None`: the constraints; an unbounded side is the measured size.
    destination: Option<(f32, f32)>,
}

impl Container for Probe {}

impl Control for Probe {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        let child = cx.child(0);
        cx.measure_child(child, self.constraints.0, self.constraints.1)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let child = cx.child(0);
        let size = cx.child_base(child).measured;
        let (w, h) = self.destination.unwrap_or(self.constraints);
        let side = |given: f32, measured: f32| if given.is_finite() { given } else { measured };
        cx.arrange_child(child, Rect::from_wh(side(w, size.width), side(h, size.height)));
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

fn spy(seen: &Seen) -> Build<Spy> {
    Build::new(Spy { layout: SkiaLayout::default(), seen: seen.clone() })
}

/// C# `Box`: a bare control; -1 leaves that size unset.
fn boxed(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

fn boxes(count: usize, width: i32, height: i32) -> Vec<Build<SkiaLayout>> {
    (0..count).map(|_| boxed(width, height)).collect()
}

/// C# `Panel`: an Absolute layout holding one fixed box, so its size depends on its constraint.
fn panel(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().children(boxed(width, height))
}

fn wrap(horizontal: LayoutOptions, vertical: LayoutOptions, spacing: i32) -> Build<SkiaLayout> {
    SkiaWrap::new().spacing(spacing).horizontal_options(horizontal).vertical_options(vertical)
}

/// One laid out tree: the host, the wrap and its children in order.
struct Laid {
    host: Headless<()>,
    wrap: ControlId,
    items: Vec<ControlId>,
}

impl Laid {
    fn rect(&self, index: usize) -> Rect {
        self.host.rect(self.items[index])
    }
    fn rects(&self) -> Vec<Rect> {
        self.items.iter().map(|id| self.host.rect(*id)).collect()
    }
    fn measured(&self, index: usize) -> Size {
        self.host.ui.tree.base(self.items[index]).unwrap().measured
    }
    fn size(&self) -> Size {
        self.host.ui.tree.base(self.wrap).unwrap().measured
    }
}

/// C# `wrap.Measure(width, height, 1)`, then an arrange at the origin.
fn lay_out(width: f32, height: f32, wrap: Build<SkiaLayout>, items: impl IntoChildren) -> Laid {
    let wrap_id = wrap.id();
    let root = Build::new(Probe { constraints: (width, height), destination: None }).children(wrap.children(items));
    let mut host = Headless::new(Ui::new((), |_| root), 1000, 1000, 1.0);
    host.settle();
    let items = host.ui.tree.children(wrap_id).to_vec();
    Laid { host, wrap: wrap_id, items }
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

fn size(width: i32, height: i32) -> Size {
    Size::new(width as f32, height as f32)
}

// ---------------------------------------------------------------- ported upstream tests

/// Changed from upstream (React rule, Nick 2026-10-02): the Fill child is measured with the whole
/// line, so after a child it starts a line of its own (C# flex-fill: it took the rest of the row).
#[test]
fn wrap_fill_x_child_fills_rest_of_row() {
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), (boxed(50, 20), panel(10, 20).fill_x()));
    assert_eq!(laid.size().height, 40.0);
    assert_eq!(laid.rect(1).top, 20.0);
    assert_eq!(laid.rect(1).left, 0.0);
    assert_eq!(laid.rect(1).width(), 400.0);
}

#[test]
fn wrap_center_child_keeps_flow_slot() {
    let items = (boxed(50, 20), boxed(50, 20).horizontal_options(CENTER), boxed(50, 20));
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), items);
    assert_eq!(laid.rect(1).left, 50.0);
    assert_eq!(laid.rect(2).left, 100.0);
}

#[test]
fn wrap_split2_center_cells_still_centered_in_chunk() {
    let items = (boxed(50, 20).horizontal_options(CENTER), boxed(50, 20).horizontal_options(CENTER));
    // Upstream measures at 412, which its 1 px inset makes 410; without the inset that is 410.
    let laid = lay_out(410.0, INF, wrap(FILL, START, 10).split(2), items);
    assert_eq!(laid.rect(0).left, 75.0);
    assert_eq!(laid.rect(1).left, 285.0);
}

#[test]
fn arrange_fill_axis_wider_than_measured_remeasures_with_final_box() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let wrap = wrap(FILL, START, 0).children(boxes(10, 50, 20));
    let wrap_id = wrap.id();
    let stack = SkiaStack::new().spacing(0).children(wrap);
    let stack_id = stack.id();
    // The sizes right after the measure at 300, before the arrange at 600.
    let root = Build::new(Watch(Probe { constraints: (300.0, 700.0), destination: Some((600.0, 700.0)) }, move |cx| {
        log.borrow_mut().push((cx.child_base(stack_id).measured.width, cx.child_base(wrap_id).measured.height));
    }))
    .children(stack);
    let mut host = Headless::new(Ui::new((), |_| root), 1000, 1000, 1.0);
    host.settle();
    assert_eq!(seen.borrow()[0], (300.0, 40.0));
    let measured = |id: ControlId| host.ui.tree.base(id).unwrap().measured;
    assert_eq!(measured(stack_id).width, 600.0);
    assert_eq!(measured(wrap_id), size(600, 20)); // 10 x 50 = 500 fits one line at 600
}

/// A `Probe` that looks at the tree between its measure and its arrange.
struct Watch<F: FnMut(&LayoutCx) + 'static>(Probe, F);

impl<F: FnMut(&LayoutCx) + 'static> Container for Watch<F> {}

impl<F: FnMut(&LayoutCx) + 'static> Control for Watch<F> {
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let size = self.0.measure(cx, width, height);
        (self.1)(cx);
        size
    }
    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.0.arrange(cx)
    }
}

// ---------------------------------------------------------------- the flow

#[test]
fn a_line_holds_the_whole_width_of_the_wrap() {
    // 6 x 50 in 300: they fit.
    let laid = lay_out(300.0, INF, wrap(FILL, START, 0), boxes(6, 50, 20));
    assert_eq!(laid.rect(4), ltrb(200, 0, 250, 20));
    assert_eq!(laid.rect(5), ltrb(250, 0, 300, 20));
    assert_eq!(laid.size(), size(300, 20));
    // One pixel less and the sixth goes to the next line.
    let laid = lay_out(299.0, INF, wrap(FILL, START, 0), boxes(6, 50, 20));
    assert_eq!(laid.rect(5), ltrb(0, 20, 50, 40));
    assert_eq!(laid.size(), size(299, 40));

    // The last child 1 px wider: stays at 301.
    let laid = lay_out(301.0, INF, wrap(FILL, START, 0), (boxes(5, 50, 20), boxed(51, 20)));
    assert_eq!(laid.rect(5), ltrb(250, 0, 301, 20));
    assert_eq!(laid.size(), size(301, 20));

    // A child is offered the whole width.
    let laid = lay_out(300.0, INF, wrap(FILL, START, 0), (boxed(300, 20), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 300, 20), ltrb(0, 20, 50, 40)]);
}

#[test]
fn spacing_is_between_the_children_and_between_the_lines() {
    // 4 x 50 + 3 x 10 = 230.
    let laid = lay_out(230.0, INF, wrap(FILL, START, 10), boxes(4, 50, 20));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(60, 0, 110, 20), ltrb(120, 0, 170, 20), ltrb(180, 0, 230, 20)]);
    assert_eq!(laid.size(), size(230, 20));

    let laid = lay_out(200.0, INF, wrap(FILL, START, 10), boxes(7, 50, 20));
    assert_eq!(laid.rect(3), ltrb(0, 30, 50, 50));
    assert_eq!(laid.rect(5), ltrb(120, 30, 170, 50));
    assert_eq!(laid.rect(6), ltrb(0, 60, 50, 80));
    assert_eq!(laid.size(), size(200, 80));
}

#[test]
fn a_line_is_as_tall_as_its_tallest_child() {
    let items = (boxed(50, 20), boxed(50, 40), boxed(50, 30), boxed(50, 10), boxed(50, 25));
    let laid = lay_out(170.0, INF, wrap(FILL, START, 5), items);
    let expected =
        [ltrb(0, 0, 50, 20), ltrb(55, 0, 105, 40), ltrb(110, 0, 160, 30), ltrb(0, 45, 50, 55), ltrb(55, 45, 105, 70)];
    assert_eq!(laid.rects(), expected);
    assert_eq!(laid.size(), size(170, 70));
}

#[test]
fn a_child_is_aligned_in_its_line_by_its_vertical_options() {
    let items = (
        boxed(50, 40),
        boxed(50, 20).vertical_options(CENTER),
        boxed(50, 20).vertical_options(END),
        boxed(50, -1).fill_y(),
    );
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 40), ltrb(50, 10, 100, 30), ltrb(100, 20, 150, 40), ltrb(150, 0, 200, 40)]);
    assert_eq!(laid.measured(3), size(50, 40));
    assert_eq!(laid.size(), size(400, 40));

    // Each line on its own.
    let items = (
        boxed(100, 40),
        boxed(100, 20).vertical_options(END),
        boxed(100, 50),
        boxed(100, 20).vertical_options(CENTER),
    );
    let laid = lay_out(250.0, INF, wrap(FILL, START, 10), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 40), ltrb(110, 20, 210, 40), ltrb(0, 50, 100, 100), ltrb(110, 65, 210, 85)]);
    assert_eq!(laid.size(), size(250, 100));
}

/// A centered child takes the odd free pixel of its line (C# `RoundCenterAlignment`).
#[test]
fn center_in_a_line_with_an_odd_free_pixel_takes_it() {
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), (boxed(50, 40), boxed(50, 21).vertical_options(CENTER)));
    assert_eq!(laid.rect(1), ltrb(50, 9, 100, 31));
}

/// React rule (Nick 2026-10-02): a Fill child is measured with the whole line, so a line of its
/// own; the children after it start the next line (C# flex-fill gave it the rest of its line).
#[test]
fn a_fill_x_child_after_others_starts_a_line_of_its_own() {
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), (boxed(50, 20), panel(10, 20).fill_x()));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 20, 400, 40)]);
    assert_eq!((laid.measured(1), laid.size()), (size(400, 20), size(400, 40)));

    // Nothing is left after it: the next child starts a line.
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), (boxed(50, 20), panel(10, 20).fill_x(), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 20, 400, 40), ltrb(0, 40, 50, 60)]);
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), (panel(10, 20).fill_x(), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 400, 20), ltrb(0, 20, 50, 40)]);
    assert_eq!(laid.size(), size(400, 40));

    // Two of them: each has a line of its own.
    let items = (boxed(50, 20), panel(10, 20).fill_x(), panel(10, 30).fill_x());
    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 30, 400, 50), ltrb(0, 60, 400, 90)]);
    assert_eq!(laid.size(), size(400, 90));

    // No room at all on the line: it goes down and takes that line.
    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), (boxed(300, 20), boxed(80, 20), panel(10, 20).fill_x()));
    assert_eq!(laid.rects(), [ltrb(0, 0, 300, 20), ltrb(310, 0, 390, 20), ltrb(0, 30, 400, 50)]);

    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), (boxed(50, 20), boxed(-1, 20).fill_x()));
    assert_eq!(laid.rect(1), ltrb(0, 30, 400, 50));
}

#[test]
fn center_and_end_children_stay_in_the_flow() {
    let items = (boxed(50, 20), boxed(50, 20).horizontal_options(CENTER), boxed(50, 20));
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(50, 0, 100, 20), ltrb(100, 0, 150, 20)]);

    let items = (boxed(50, 20), boxed(50, 20).horizontal_options(END), boxed(50, 20));
    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(60, 0, 110, 20), ltrb(120, 0, 170, 20)]);

    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), boxed(50, 20).horizontal_options(END));
    assert_eq!(laid.rect(0), ltrb(0, 0, 50, 20));
}

#[test]
fn an_auto_width_wrap_is_as_wide_as_its_widest_line() {
    let laid = lay_out(400.0, INF, wrap(START, START, 10), boxes(3, 50, 20));
    assert_eq!(laid.size(), size(170, 20));

    let laid = lay_out(350.0, INF, wrap(START, START, 10), boxes(5, 100, 20));
    assert_eq!(laid.rect(2), ltrb(220, 0, 320, 20));
    assert_eq!(laid.rect(3), ltrb(0, 30, 100, 50));
    assert_eq!(laid.size(), size(320, 50));

    // Unbounded: one line.
    let laid = lay_out(INF, INF, wrap(START, START, 10), boxes(5, 100, 20));
    assert_eq!(laid.rect(4), ltrb(440, 0, 540, 20));
    assert_eq!(laid.size(), size(540, 20));

    // No children.
    assert_eq!(lay_out(400.0, INF, wrap(START, START, 10), boxes(0, 0, 0)).size(), size(0, 0));
    assert_eq!(lay_out(400.0, INF, wrap(FILL, START, 10), boxes(0, 0, 0)).size(), size(400, 0));
}

#[test]
fn an_invisible_child_takes_no_place() {
    let items = (boxed(50, 20), boxed(50, 20).is_visible(false), boxed(50, 20));
    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), items);
    assert_eq!(laid.rect(2), ltrb(60, 0, 110, 20));
    assert_eq!(laid.size(), size(400, 20));
}

#[test]
fn padding_moves_the_flow_and_narrows_the_lines() {
    let laid = lay_out(200.0, INF, wrap(FILL, START, 10).padding((10, 5, 10, 5)), boxes(5, 50, 20));
    let expected =
        [ltrb(10, 5, 60, 25), ltrb(70, 5, 120, 25), ltrb(130, 5, 180, 25), ltrb(10, 35, 60, 55), ltrb(70, 35, 120, 55)];
    assert_eq!(laid.rects(), expected);
    assert_eq!(laid.size(), size(200, 60));
}

#[test]
fn margins_are_part_of_the_slot() {
    let items = (boxed(50, 20).margin(5), boxed(50, 20).margin((0, 10, 0, 0)), boxed(50, 20));
    let laid = lay_out(400.0, INF, wrap(FILL, START, 10), items);
    assert_eq!((laid.measured(0), laid.measured(1)), (size(60, 30), size(50, 30)));
    // Upstream cuts the first one to 40 px (5..45): a size request with a margin is squeezed in its
    // stack slot there. Not ported.
    assert_eq!(laid.rect(0), ltrb(5, 5, 55, 25));
    assert_eq!(laid.rect(1), ltrb(70, 10, 120, 30));
    assert_eq!(laid.rect(2), ltrb(130, 0, 180, 20));
    assert_eq!(laid.size(), size(400, 30));
}

#[test]
fn a_child_wider_than_the_wrap_is_cut_and_has_its_own_line() {
    let laid = lay_out(300.0, INF, wrap(FILL, START, 10), (boxed(50, 20), boxed(400, 20), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 30, 300, 50), ltrb(0, 60, 50, 80)]);
    assert_eq!(laid.size(), size(300, 80));

    let laid = lay_out(300.0, INF, wrap(FILL, START, 10), (panel(400, 20), boxed(50, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 300, 20), ltrb(0, 30, 50, 50)]);
    assert_eq!(laid.size(), size(300, 50));
}

// ---------------------------------------------------------------- height

#[test]
fn a_wrap_with_a_finite_height() {
    // Auto height: its lines. Fill: the height it is given; the lines are the same.
    let laid = lay_out(250.0, 200.0, wrap(FILL, START, 0), boxes(3, 100, 20));
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 20), ltrb(100, 0, 200, 20), ltrb(0, 20, 100, 40)]);
    assert_eq!(laid.size(), size(250, 40));
    let laid = lay_out(250.0, 200.0, wrap(FILL, FILL, 0), boxes(3, 100, 20));
    assert_eq!(laid.rect(2), ltrb(0, 20, 100, 40));
    assert_eq!(laid.size(), size(250, 200));

    // A line past the end is cut.
    let laid = lay_out(250.0, 30.0, wrap(FILL, START, 0), boxes(3, 100, 20));
    assert_eq!(laid.measured(2), size(100, 10));
    assert_eq!(laid.rect(2), ltrb(0, 20, 100, 30));
    assert_eq!(laid.size(), size(250, 30));
}

#[test]
fn a_fill_y_child_takes_the_line_or_all_the_height_that_is_left() {
    let items = || (boxed(100, 20), boxed(100, -1).fill_y(), boxed(100, 30));
    // Unbounded: the line is as tall as the others make it.
    let laid = lay_out(400.0, INF, wrap(FILL, START, 0), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 20), ltrb(100, 0, 200, 30), ltrb(200, 0, 300, 30)]);
    assert_eq!((laid.measured(1), laid.size()), (size(100, 30), size(400, 30)));
    // Bounded: it takes what the wrap has.
    let laid = lay_out(400.0, 200.0, wrap(FILL, START, 0), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 20), ltrb(100, 0, 200, 200), ltrb(200, 0, 300, 30)]);
    assert_eq!(laid.size(), size(400, 200));
}

#[test]
fn a_child_is_offered_the_line_width_and_the_height_under_its_line() {
    let seen = Seen::default();
    lay_out(300.0, 200.0, wrap(FILL, START, 0), (boxed(100, 20), spy(&seen).children(boxed(60, 30))));
    assert_eq!(*seen.borrow(), [(300.0, 200.0)]);
    let seen = Seen::default();
    lay_out(300.0, INF, wrap(FILL, START, 0), (boxed(100, 20), spy(&seen).children(boxed(60, 30))));
    assert_eq!(*seen.borrow(), [(300.0, INF)]);

    // The second line starts under the first and its gap.
    let (first, second) = (Seen::default(), Seen::default());
    let items =
        (boxed(200, 30), spy(&first).children(boxed(60, 30)), boxed(200, 40), spy(&second).children(boxed(60, 30)));
    let laid = lay_out(300.0, 400.0, wrap(FILL, START, 10), items);
    assert_eq!((first.borrow().clone(), second.borrow().clone()), (vec![(300.0, 400.0)], vec![(300.0, 360.0)]));
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 30), ltrb(210, 0, 270, 30), ltrb(0, 40, 200, 80), ltrb(210, 40, 270, 70)]);
    assert_eq!(laid.size(), size(300, 80));
}

#[test]
/// The column measures the wrap unbounded in height (React rule; C# gave it what the column had
/// left and cut its lines). Numbers of the React engine, except the C# rule kept: no measure is
/// larger than its constraint (the second column: 150, React 170).
fn a_wrap_in_a_column_is_measured_unbounded_in_height() {
    let seen = Seen::default();
    let items = (boxed(90, 30), spy(&seen).children(boxed(60, 30)), boxes(4, 90, 30));
    let (flow, last) = (wrap(FILL, START, 0).children(items), boxed(50, 10));
    let (wrap_id, last_id) = (flow.id(), last.id());
    let column = SkiaStack::new().spacing(0).children((boxed(-1, 100).fill_x(), flow, last));
    let column_id = column.id();
    let mut host = Headless::new(Ui::new((), |_| column), 300, 500, 1.0);
    host.settle();
    let measured = |id: ControlId| host.ui.tree.base(id).unwrap().measured;
    assert_eq!(*seen.borrow(), [(300.0, INF)]);
    assert_eq!(host.rect(wrap_id), ltrb(0, 100, 300, 160));
    assert_eq!(host.rect(last_id), ltrb(0, 160, 50, 170));
    assert_eq!(measured(column_id), size(300, 170));

    // Past the end of the column nothing is cut: the second line and the child after the wrap keep
    // their size.
    let items = boxes(6, 90, 30);
    let ids: Vec<_> = items.iter().map(|item| item.id()).collect();
    let (flow, last) = (wrap(FILL, START, 0).children(items), boxed(50, 10));
    let (wrap_id, last_id) = (flow.id(), last.id());
    let mut host = Headless::new(
        Ui::new((), |_| SkiaStack::new().spacing(0).children((boxed(-1, 100).fill_x(), flow, last))),
        300,
        150,
        1.0,
    );
    host.settle();
    let measured = |id: ControlId| host.ui.tree.base(id).unwrap().measured;
    let sizes: Vec<_> = ids.iter().map(|id| measured(*id)).collect();
    assert_eq!(sizes, [size(90, 30); 6]);
    assert_eq!(host.rect(ids[3]), ltrb(0, 130, 90, 160));
    assert_eq!(measured(wrap_id), size(300, 60));
    assert_eq!(host.rect(last_id), ltrb(0, 160, 50, 170));
}

// ---------------------------------------------------------------- split

#[test]
fn split_makes_equal_columns() {
    // The catalog case: two columns of cards that fill their column.
    let cards = || (panel(60, 40).fill_x(), panel(60, 70).fill_x(), panel(60, 30).fill_x());
    let laid = lay_out(410.0, INF, wrap(FILL, START, 10).split(2), cards());
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 40), ltrb(210, 0, 410, 70), ltrb(0, 80, 200, 110)]);
    assert_eq!(laid.size(), size(410, 110));
    let laid = lay_out(360.0, INF, wrap(FILL, START, 10).split(2).padding(16), cards());
    assert_eq!(laid.rects(), [ltrb(16, 16, 175, 56), ltrb(185, 16, 344, 86), ltrb(16, 96, 175, 126)]);
    assert_eq!(laid.size(), size(360, 142));

    let items = (panel(60, 40).fill_x(), panel(60, 70).fill_x(), panel(60, 30).fill_x(), panel(60, 30).fill_x());
    let laid = lay_out(320.0, INF, wrap(FILL, START, 10).split(3), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 40), ltrb(110, 0, 210, 70), ltrb(220, 0, 320, 30), ltrb(0, 80, 100, 110)]);

    // A child that does not fill sits in its column by its own options.
    let items = || (boxed(50, 20), boxed(70, 30), boxed(50, 20));
    let laid = lay_out(410.0, INF, wrap(FILL, START, 10).split(2), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(210, 0, 280, 30), ltrb(0, 40, 50, 60)]);
    assert_eq!(laid.size(), size(410, 60));
    // An auto-width wrap is as wide as its columns.
    assert_eq!(lay_out(410.0, INF, wrap(START, START, 10).split(2), items()).size(), size(410, 60));
    // One column: a line per child.
    let laid = lay_out(410.0, INF, wrap(FILL, START, 10).split(1), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 30, 70, 60), ltrb(0, 70, 50, 90)]);
    // Without SplitAlign the next child follows right after the width of this one.
    let laid = lay_out(410.0, INF, wrap(FILL, START, 10).split(2).split_align(false), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(60, 0, 130, 30), ltrb(0, 40, 50, 60)]);

    // A child is offered its column.
    let seen = Seen::default();
    lay_out(410.0, 300.0, wrap(FILL, START, 10).split(2), (boxed(50, 20), spy(&seen).children(boxed(60, 30))));
    assert_eq!(*seen.borrow(), [(200.0, 300.0)]);
}

// ---------------------------------------------------------------- defaults and the measure memo

#[test]
fn defaults_of_the_alias() {
    let mut tree = drawnui::Tree::default();
    let id = tree.mount(None, SkiaWrap::new());
    let (layout, base) = (tree.find::<SkiaLayout>(id).unwrap(), tree.base(id).unwrap());
    assert_eq!((layout.p.layout_type, layout.p.spacing), (LayoutType::Wrap, 8.0));
    assert_eq!((layout.p.split, layout.p.split_align), (0, true));
    assert_eq!((base.p.horizontal_options, base.p.vertical_options), (FILL, START));
}

/// A wrap of six spies, 50 x 20 each, measured in that box. Returns it with one measure log per spy.
fn spies(width: f32, height: f32) -> (Laid, Vec<Seen>) {
    let logs: Vec<Seen> = (0..6).map(|_| Seen::default()).collect();
    let items: Vec<_> = logs.iter().map(|log| spy(log).width_request(50).height_request(20)).collect();
    (lay_out(width, height, wrap(FILL, START, 0), items), logs)
}

fn counts(logs: &[Seen]) -> Vec<usize> {
    logs.iter().map(|log| log.borrow().len()).collect()
}

fn change(laid: &mut Laid, index: usize, set: impl FnOnce(&mut Mut<'_, Spy>)) {
    set(&mut laid.host.ui.tree.find_mut::<Spy>(laid.items[index]).unwrap());
    laid.host.settle();
}

#[test]
fn every_child_is_measured_once_and_nothing_is_measured_while_nothing_changes() {
    // Two lines of three; the fourth child is measured for the first line and moved down unmeasured.
    let (mut laid, logs) = spies(160.0, INF);
    assert_eq!(laid.rect(3), ltrb(0, 20, 50, 40));
    assert_eq!(counts(&logs), [1; 6]);
    laid.host.frame();
    laid.host.frame();
    assert_eq!(counts(&logs), [1; 6]);
    change(&mut laid, 1, |spy| spy.set_opacity(0.5));
    assert_eq!(counts(&logs), [1; 6]);

    // Unbounded height: a width change moves the children after it and measures none of them.
    change(&mut laid, 0, |spy| spy.set_width_request(70));
    assert_eq!(laid.rect(1), ltrb(70, 0, 120, 20));
    assert_eq!(laid.rect(2), ltrb(0, 20, 50, 40));
    assert_eq!(counts(&logs), [2, 1, 1, 1, 1, 1]);
    // Nor does a height change: they are offered an unbounded height as before.
    change(&mut laid, 0, |spy| spy.set_height_request(30));
    assert_eq!(laid.rect(2), ltrb(0, 30, 50, 50));
    assert_eq!(counts(&logs), [3, 1, 1, 1, 1, 1]);
}

#[test]
fn in_a_bounded_wrap_a_taller_line_measures_the_lines_below_again() {
    let (mut laid, logs) = spies(160.0, 200.0);
    assert_eq!(counts(&logs), [1; 6]);
    // The last child changes: nothing before it is measured.
    change(&mut laid, 5, |spy| spy.set_width_request(40));
    assert_eq!(counts(&logs), [1, 1, 1, 1, 1, 2]);
    // The first line gets taller: its own children keep their constraint, the next line starts lower.
    change(&mut laid, 0, |spy| spy.set_height_request(30));
    assert_eq!(laid.rect(3), ltrb(0, 30, 50, 50));
    assert_eq!(counts(&logs), [2, 1, 1, 1, 2, 3]);
}

// ---------------------------------------------------------------- C# 2026-10-02 fit rules

/// Port of C# `WrapOverflowChildTests`: a fixed-size child whose own child overflows it (an
/// unclipped circle pushed out by a negative margin) takes its fixed size in the line; the
/// overflow neither widens its slot nor breaks the line.
#[test]
fn fixed_size_children_with_overflowing_content_share_one_line() {
    let demo = |clip: bool| {
        SkiaLayer::new().width_request(140).height_request(70).background_color(Color::DARK_GRAY).is_clipped_to_bounds(clip).children(
            SkiaShape::new()
                .shape_type(ShapeType::Circle)
                .background_color(Color::MAGENTA)
                .width_request(110)
                .lock_ratio(1.0)
                .horizontal_options(END)
                .vertical_options(END)
                .margin(Thickness::new(0.0, 0.0, -30.0, -30.0)),
        )
    };
    let mut wrap = Handle::<SkiaLayout>::default();
    let built = SkiaWrap::new().spacing(24).assign(&mut wrap).children((demo(false), demo(true), demo(true)));
    let mut host = Headless::new(Ui::new((), |_| built).background(Color::BLACK), 840, 300, 1.0);
    host.settle();
    let rects: Vec<Rect> = host.ui.tree.children(wrap).to_vec().into_iter().map(|id| host.rect(id)).collect();
    assert!(rects.iter().all(|r| r.top == rects[0].top), "{rects:?}");
    assert!(rects[0].top.abs() <= 1.5, "{rects:?}");
    assert_eq!(rects[0].height(), 70.0, "{rects:?}");
}

/// Port of C# `WrapExactFitTests`: two fixed-width children of (line - spacing) / 2 share their
/// line at any scale, the third starts the next.
#[test]
fn two_halves_share_a_line_the_third_starts_the_next() {
    for (line, scale) in [(772.0, 1.0), (772.0, 1.5), (772.0, 1.75), (772.0, 2.0), (772.0, 3.0), (743.0, 1.0), (743.0, 1.25), (743.0, 2.25), (745.0, 1.5), (745.0, 3.0), (772.0, 2.625), (743.0, 2.625)] {
        let half: f32 = (line - 16.0) / 2.0;
        let mut ids = [Handle::<SkiaShape>::default(); 3];
        let [a, b, c] = &mut ids;
        let built = SkiaWrap::new().spacing(16).width_request(line).horizontal_options(START).children((
            SkiaShape::new().background_color(Color::RED).width_request(half).height_request(40).assign(a),
            SkiaShape::new().background_color(Color::GREEN).width_request(half).height_request(40).assign(b),
            SkiaShape::new().background_color(Color::BLUE).width_request(half).height_request(40).assign(c),
        ));
        let mut host = Headless::new(Ui::new((), |_| built).background(Color::BLACK), (900.0 * scale) as i32, (300.0 * scale) as i32, scale);
        host.settle();
        let (ra, rb, rc) = (host.rect(ids[0]), host.rect(ids[1]), host.rect(ids[2]));
        let at = format!("line {line} scale {scale}: a {ra:?} b {rb:?} c {rc:?}");
        assert!((ra.top - rb.top).abs() <= 0.5, "{at}");
        assert!(rb.left > ra.right, "the second half did not stay beside the first, {at}");
        assert!(rc.top >= ra.bottom, "the third child stayed on the full line, {at}");
    }
}
