//! Column / Row / Absolute measure and arrange rules. Ported from the C# UnitTests
//! `StackWrapSweepTests`, `SkiaLayoutSecondPassTests` and `LayoutSweepTests` (the cases that need no
//! Wrap, Grid, Split or templates), same trees and same numbers. Tests named `csharp_*` have no
//! C# twin: their numbers were read from the C# engine with the same tree.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use drawnui::IntoChildren;
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
    /// Runs between measure and arrange.
    measured: Option<Box<dyn FnMut(&LayoutCx)>>,
}

impl Container for Probe {}

impl Control for Probe {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        let child = cx.child(0);
        let size = cx.measure_child(child, self.constraints.0, self.constraints.1);
        if let Some(measured) = &mut self.measured {
            measured(cx);
        }
        size
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let child = cx.child(0);
        let size = cx.child_base(child).measured;
        let (w, h) = self.destination.unwrap_or(self.constraints);
        let side = |given: f32, measured: f32| if given.is_finite() { given } else { measured };
        cx.arrange_child(child, Rect::from_wh(side(w, size.width), side(h, size.height)));
    }
}

fn probe(width: f32, height: f32, child: impl IntoChildren) -> Build<Probe> {
    Build::new(Probe { constraints: (width, height), destination: None, measured: None }).children(child)
}

#[derive(Default)]
struct Ids {
    stack: Handle<SkiaLayout>,
    a: Handle<SkiaLayout>,
    b: Handle<SkiaLayout>,
    c: Handle<SkiaLayout>,
    spy: Handle<Spy>,
}

fn run(build: impl FnOnce(&mut Ids) -> Build<Probe>) -> Headless<Ids> {
    let mut host = Headless::new(Ui::new(Ids::default(), build), 1000, 1000, 1.0);
    host.settle();
    host
}

fn measured(host: &Headless<Ids>, id: impl Into<ControlId>) -> Size {
    host.ui.tree.base(id).unwrap().measured
}

/// C# `Box`: a bare control; -1 leaves that size unset.
fn boxed(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// C# `Panel`: an Absolute layout holding one fixed box, so its size depends on its constraint.
fn panel(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().children(boxed(width, height))
}

fn stack(kind: LayoutType, horizontal: LayoutOptions, vertical: LayoutOptions) -> Build<SkiaLayout> {
    SkiaLayout::new().layout_type(kind).spacing(0).horizontal_options(horizontal).vertical_options(vertical)
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

/// Records the constraints its content measure gets (C# `MeasureSpy`).
struct Spy {
    layout: SkiaLayout,
    seen: Rc<RefCell<Vec<(f32, f32)>>>,
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

fn spy(kind: LayoutType, seen: &Rc<RefCell<Vec<(f32, f32)>>>) -> Build<Spy> {
    let mut layout = SkiaLayout::default();
    layout.p.layout_type = kind;
    Build::new(Spy { layout, seen: seen.clone() })
}

// ---------------------------------------------------------------- main-axis Center stays in the slot

#[test]
fn column_vertical_center_child_stays_in_slot() {
    let host = run(|ids| {
        probe(
            300.0,
            500.0,
            stack(LayoutType::Column, FILL, START).assign(&mut ids.stack).children((
                boxed(-1, 100).fill_x(),
                boxed(-1, 20).fill_x().vertical_options(CENTER).assign(&mut ids.b),
                boxed(-1, 50).fill_x().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).height, 170.0);
    assert_eq!(host.rect(ids.b), ltrb(0, 100, 300, 120));
    assert_eq!(host.rect(ids.c), ltrb(0, 120, 300, 170));
}

#[test]
fn column_vertical_center_child_first_pass_only_stays_in_slot() {
    let host = run(|ids| {
        probe(
            300.0,
            500.0,
            stack(LayoutType::Column, FILL, START).children((
                boxed(100, 100),
                boxed(100, 20).vertical_options(CENTER).assign(&mut ids.b),
                boxed(100, 50),
            )),
        )
    });
    assert_eq!(host.rect(host.ui.state.b), ltrb(0, 100, 100, 120));
}

#[test]
fn row_horizontal_center_child_stays_in_slot() {
    let host = run(|ids| {
        probe(
            500.0,
            300.0,
            stack(LayoutType::Row, START, FILL).assign(&mut ids.stack).children((
                boxed(100, -1).fill_y(),
                boxed(20, -1).fill_y().horizontal_options(CENTER).assign(&mut ids.b),
                boxed(50, -1).fill_y().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).width, 170.0);
    assert_eq!(host.rect(ids.b), ltrb(100, 0, 120, 300));
    assert_eq!(host.rect(ids.c), ltrb(120, 0, 170, 300));
}

// ---------------------------------------------------------------- Fill child on an unbounded main axis = auto

#[test]
fn column_infinite_height_fill_y_child_is_content_sized() {
    let host = run(|ids| {
        probe(
            300.0,
            INF,
            stack(LayoutType::Column, FILL, START).assign(&mut ids.stack).children((
                boxed(-1, 100).fill_x(),
                panel(80, 30).fill().assign(&mut ids.b),
                boxed(-1, 100).fill_x().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).height, 230.0);
    assert_eq!(measured(&host, ids.b).height, 30.0);
    assert_eq!(host.rect(ids.b), ltrb(0, 100, 300, 130));
    assert_eq!(host.rect(ids.c), ltrb(0, 130, 300, 230));
}

#[test]
fn row_infinite_width_fill_x_child_is_content_sized() {
    let host = run(|ids| {
        probe(
            INF,
            300.0,
            stack(LayoutType::Row, START, FILL).assign(&mut ids.stack).children((
                boxed(100, -1).fill_y(),
                panel(80, 30).fill().assign(&mut ids.b),
                boxed(100, -1).fill_y().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).width, 280.0);
    assert_eq!(host.rect(ids.b), ltrb(100, 0, 180, 300));
    assert_eq!(host.rect(ids.c), ltrb(180, 0, 280, 300));
}

#[test]
fn column_finite_height_fill_y_still_distributes_space() {
    let host = run(|ids| {
        probe(
            300.0,
            500.0,
            stack(LayoutType::Column, FILL, FILL).spacing(10).children((
                boxed(-1, 100).fill_x(),
                panel(80, 30).fill().assign(&mut ids.b),
                boxed(-1, 100).fill_x().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(host.rect(ids.b), ltrb(0, 110, 300, 390));
    assert_eq!(host.rect(ids.c), ltrb(0, 400, 300, 500));
}

// ---------------------------------------------------------------- auto stack whose only cross-axis children are Fill

#[test]
fn column_auto_width_only_fill_x_children_does_not_collapse() {
    let host = run(|ids| {
        probe(
            300.0,
            INF,
            stack(LayoutType::Column, START, START)
                .assign(&mut ids.stack)
                .children((panel(80, 40).fill_x().assign(&mut ids.a), panel(60, 40).fill_x())),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).width, 300.0);
    assert_eq!(measured(&host, ids.a).width, 300.0);
}

#[test]
fn row_auto_height_only_fill_y_children_does_not_collapse() {
    let host = run(|ids| {
        probe(
            INF,
            300.0,
            stack(LayoutType::Row, START, START)
                .assign(&mut ids.stack)
                .children((panel(40, 80).fill_y(), panel(40, 60).fill_y())),
        )
    });
    assert_eq!(measured(&host, host.ui.state.stack).height, 300.0);
}

/// Changed, React rule: the Fill child reports the width the column is measured in, and an auto
/// width is the largest child, Fill included (C# keeps the content width, 100, and squeezes the
/// Fill child to it; React SkiaLayout.MeasureAbsolute + SkiaControl.Measure give 400 and 400).
#[test]
fn column_auto_width_fixed_plus_fill_x_takes_the_fill_child() {
    let host = run(|ids| {
        probe(
            400.0,
            INF,
            stack(LayoutType::Column, START, START)
                .assign(&mut ids.stack)
                .children((boxed(100, 20), panel(10, 20).fill_x().assign(&mut ids.b))),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).width, 400.0);
    assert_eq!(measured(&host, ids.b).width, 400.0);
}

// ---------------------------------------------------------------- Absolute: measure never peeks at the previous arrange

#[test]
fn absolute_fill_layer_infinite_width_measure_does_not_depend_on_previous_arrange() {
    // The width of `b` right after the unbounded measure, before any arrange.
    let first = Rc::new(Cell::new(0.0));
    let seen = first.clone();
    let mut host = run(|ids| {
        let layer = SkiaLayout::layer()
            .assign(&mut ids.stack)
            .children((boxed(100, 20), panel(80, 20).fill_x().assign(&mut ids.b)));
        let b = ids.b.id();
        let mut root = probe(INF, 300.0, layer);
        root.control_mut().destination = Some((200.0, 300.0));
        root.control_mut().measured = Some(Box::new(move |cx| seen.set(cx.child_base(b).measured.width)));
        root
    });
    let (layer, b) = (host.ui.state.stack, host.ui.state.b);
    // Unbounded: content width, no previous frame to borrow from.
    assert_eq!(first.get(), 80.0);
    // Arranged into a finite box: the Fill child follows the real box.
    assert_eq!(measured(&host, b).width, 200.0);

    // The same unbounded measure again gives the numbers of the first time, not of the last arrange.
    first.set(0.0);
    host.ui.tree.invalidate(layer, Dirty::MEASURE);
    host.settle();
    assert_eq!(first.get(), 80.0);
    assert_eq!(measured(&host, b).width, 200.0);
}

// ---------------------------------------------------------------- second pass keeps the main-axis constraint

#[test]
fn column_second_pass_keeps_infinite_height_for_fill_x_child() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let host = run(|ids| {
        probe(
            300.0,
            INF,
            stack(LayoutType::Column, FILL, START).assign(&mut ids.stack).children((
                boxed(-1, 100).fill_x(),
                spy(LayoutType::Column, &seen).fill_x().assign(&mut ids.spy).children(boxed(-1, 40).fill_x()),
            )),
        )
    });
    let ids = &host.ui.state;
    assert!(!seen.borrow().is_empty());
    assert!(seen.borrow().iter().all(|(_, height)| *height == INF), "a finite height leaked: {:?}", seen.borrow());
    assert_eq!(measured(&host, ids.spy).height, 40.0);
    assert_eq!(measured(&host, ids.stack).height, 140.0);
}

#[test]
fn row_second_pass_keeps_infinite_width_for_fill_y_child() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let host = run(|ids| {
        probe(
            INF,
            300.0,
            stack(LayoutType::Row, START, FILL).assign(&mut ids.stack).children((
                boxed(100, -1).fill_y(),
                spy(LayoutType::Row, &seen).fill_y().assign(&mut ids.spy).children(boxed(40, -1).fill_y()),
            )),
        )
    });
    let ids = &host.ui.state;
    assert!(!seen.borrow().is_empty());
    assert!(seen.borrow().iter().all(|(width, _)| *width == INF), "a finite width leaked: {:?}", seen.borrow());
    assert_eq!(measured(&host, ids.spy).width, 40.0);
    assert_eq!(measured(&host, ids.stack).width, 140.0);
}

// ---------------------------------------------------------------- measure-vs-arrange consistency

#[test]
fn maximum_width_request_caps_arrange_on_fill_axis() {
    let host = run(|ids| {
        probe(400.0, 100.0, SkiaLayout::new().fill_x().height_request(50).maximum_width_request(300).assign(&mut ids.stack))
    });
    let layout = host.ui.state.stack;
    assert_eq!(measured(&host, layout).width, 300.0);
    assert_eq!(host.rect(layout).width(), 300.0);
}

/// C# uses a Wrap that gets one row instead of two at the final width. No Wrap here: a Row whose
/// Fill child takes what is left shows the same thing.
#[test]
fn arrange_fill_axis_wider_than_measured_remeasures_with_final_box() {
    let host = run(|ids| {
        let row = stack(LayoutType::Row, FILL, START).children((
            boxed(50, 20),
            boxed(-1, 20).fill_x().assign(&mut ids.b),
            boxed(50, 20).assign(&mut ids.c),
        ));
        let mut root = probe(300.0, 700.0, stack(LayoutType::Column, FILL, START).assign(&mut ids.stack).children(row));
        root.control_mut().destination = Some((600.0, 700.0));
        root
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack).width, 600.0);
    assert_eq!(measured(&host, ids.b).width, 500.0);
    assert_eq!(host.rect(ids.b), ltrb(50, 0, 550, 20));
    assert_eq!(host.rect(ids.c), ltrb(550, 0, 600, 20));
}

// ---------------------------------------------------------------- numbers read from the C# engine

#[test]
fn csharp_main_axis_end_child_moves_to_the_stack_end() {
    let host = run(|ids| {
        probe(
            300.0,
            500.0,
            stack(LayoutType::Column, FILL, FILL).children((
                boxed(100, 100),
                boxed(100, 20).vertical_options(END).assign(&mut ids.b),
                boxed(100, 50).assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(host.rect(ids.b), ltrb(0, 480, 100, 500));
    // The next child follows the End child's slot, not its place.
    assert_eq!(host.rect(ids.c), ltrb(0, 120, 100, 170));
}

#[test]
fn csharp_row_fill_child_takes_what_is_left() {
    let host = run(|ids| {
        probe(
            400.0,
            300.0,
            stack(LayoutType::Row, FILL, START).spacing(10).assign(&mut ids.stack).children((
                boxed(100, 40),
                boxed(-1, 40).fill_x().assign(&mut ids.b),
                boxed(60, 40).assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack), Size::new(400.0, 40.0));
    assert_eq!(host.rect(ids.b), ltrb(110, 0, 330, 40));
    assert_eq!(host.rect(ids.c), ltrb(340, 0, 400, 40));
}

#[test]
fn csharp_cross_axis_center_rounds_the_odd_pixel_up() {
    let host = run(|ids| {
        probe(
            32.0,
            500.0,
            stack(LayoutType::Column, FILL, START)
                .children((boxed(13, 10).horizontal_options(CENTER).assign(&mut ids.a), boxed(20, 10))),
        )
    });
    assert_eq!(host.rect(host.ui.state.a), ltrb(10, 0, 23, 10));

    let host = run(|ids| {
        probe(
            500.0,
            500.0,
            stack(LayoutType::Row, START, START)
                .assign(&mut ids.stack)
                .children((boxed(40, 13).vertical_options(CENTER).assign(&mut ids.a), boxed(40, 32))),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack), Size::new(80.0, 32.0));
    assert_eq!(host.rect(ids.a), ltrb(0, 10, 40, 23));
}

/// In a stack the margin box is centered, so a one-sided margin moves the content by half of it
/// (C# LayoutCell), not by all of it as in an Absolute layout (C# CalculateLayout).
#[test]
fn csharp_cross_axis_center_in_a_stack_centers_the_margin_box() {
    let host = run(|ids| {
        probe(
            100.0,
            500.0,
            stack(LayoutType::Column, FILL, START)
                .children(panel(20, 10).horizontal_options(CENTER).margin((0, 0, 12, 0)).assign(&mut ids.a)),
        )
    });
    assert_eq!(host.rect(host.ui.state.a), ltrb(34, 0, 54, 10));
}

/// Changed, React rule: a Fill child reports the height the row is measured in (500), and an auto
/// height is the largest child (C# follows the tallest child that does not fill, 32).
#[test]
fn row_cross_axis_fill_child_reports_the_height_the_row_is_measured_in() {
    let host = run(|ids| {
        probe(
            500.0,
            500.0,
            stack(LayoutType::Row, START, START).assign(&mut ids.stack).children((
                boxed(40, 20).vertical_options(END).assign(&mut ids.a),
                boxed(40, 32),
                boxed(40, -1).fill_y().assign(&mut ids.c),
            )),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack), Size::new(120.0, 500.0));
    assert_eq!(host.rect(ids.a), ltrb(0, 480, 40, 500));
    assert_eq!(host.rect(ids.c), ltrb(80, 0, 120, 500));
}

#[test]
fn csharp_absolute_fill_child_does_not_size_an_auto_layout() {
    let host = run(|ids| {
        probe(
            300.0,
            500.0,
            SkiaLayout::new().assign(&mut ids.stack).children((SkiaLayout::new().fill().assign(&mut ids.a), boxed(80, 40))),
        )
    });
    let ids = &host.ui.state;
    assert_eq!(measured(&host, ids.stack), Size::new(80.0, 40.0));
    assert_eq!(host.rect(ids.a), ltrb(0, 0, 80, 40));
}

#[test]
fn csharp_defaults_of_the_layout_and_its_aliases() {
    let mut tree = drawnui::Tree::default();
    let mut check = |build: Build<SkiaLayout>, kind: LayoutType, horizontal: LayoutOptions| {
        let id = tree.mount(None, build);
        let layout = tree.find::<SkiaLayout>(id).unwrap();
        let base = tree.base(id).unwrap();
        assert_eq!((layout.p.layout_type, layout.p.spacing), (kind, 8.0));
        assert_eq!((base.p.horizontal_options, base.p.vertical_options), (horizontal, START));
    };
    check(SkiaLayout::new(), LayoutType::Absolute, START);
    check(SkiaStack::new(), LayoutType::Column, FILL);
    check(SkiaRow::new(), LayoutType::Row, START);
    check(SkiaLayer::new(), LayoutType::Absolute, FILL);
}
