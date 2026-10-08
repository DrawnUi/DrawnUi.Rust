//! LayoutType::Grid. Ports of the C# `LayoutSweepTests` (Grid_*), `GridAutoSizeTests`,
//! `GridInScrollStackTests` and `CenterAlignmentMarginTests`; every other rect, size and constraint
//! was read from the C# engine with the same tree (tracks chosen so that nothing lands on a
//! fraction: upstream rounds rects to pixels, this port does not).

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::controls::layout::{GridLength, SkiaGrid, SkiaWrap};
use drawnui::prelude::*;
use drawnui::testing::Headless;

const INF: f32 = f32::INFINITY;
const START: LayoutOptions = LayoutOptions::Start;
const CENTER: LayoutOptions = LayoutOptions::Center;
const END: LayoutOptions = LayoutOptions::End;
const FILL: LayoutOptions = LayoutOptions::Fill;

/// Test root: measures its child inside the given constraints and arranges it at the origin, like
/// the C# tests calling `Measure` on a control directly.
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

/// A spy around a 40 x 20 box.
fn spy(seen: &Seen) -> Build<Spy> {
    Build::new(Spy { layout: SkiaLayout::default(), seen: seen.clone() }).children(boxed(40, 20))
}

/// C# `Box`: a bare control; -1 leaves that size unset.
fn boxed(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// C# `Panel`: an Absolute layout holding one fixed box, so its size depends on its constraint.
fn panel(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().children(boxed(width, height))
}

/// A wrap of `count` boxes, 50 x 20, no spacing: what stands in for a wrapping label.
fn flow(count: usize, horizontal: LayoutOptions) -> Build<SkiaLayout> {
    let boxes: Vec<_> = (0..count).map(|_| boxed(50, 20)).collect();
    SkiaWrap::new().spacing(0).horizontal_options(horizontal).children(boxes)
}

/// A grid with no spacing unless given; an empty string leaves the definitions out.
fn grid(columns: &str, rows: &str, horizontal: LayoutOptions, vertical: LayoutOptions) -> Build<SkiaLayout> {
    let grid = SkiaGrid::new().column_spacing(0).row_spacing(0).horizontal_options(horizontal).vertical_options(vertical);
    let grid = if columns.is_empty() { grid } else { grid.column_definitions(columns) };
    if rows.is_empty() { grid } else { grid.row_definitions(rows) }
}

/// One laid out tree: the host, the grid and its children in order.
struct Laid {
    host: Headless<()>,
    grid: ControlId,
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
        self.host.ui.tree.base(self.grid).unwrap().measured
    }
}

/// C# `grid.Measure(width, height, 1)`, then an arrange at the origin.
fn lay_out(width: f32, height: f32, grid: Build<SkiaLayout>, items: impl IntoChildren) -> Laid {
    let grid_id = grid.id();
    let root = Build::new(Probe(width, height)).children(grid.children(items));
    let mut host = Headless::new(Ui::new((), |_| root), 1000, 1000, 1.0);
    host.settle();
    let items = host.ui.tree.children(grid_id).to_vec();
    Laid { host, grid: grid_id, items }
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

fn size(width: i32, height: i32) -> Size {
    Size::new(width as f32, height as f32)
}

// ---------------------------------------------------------------- ported upstream tests

/// ArtOfFoto exposure page: a grid without column definitions, a Fill stack in row 1 holding a
/// centered row. The stack is measured for the width it is arranged at, so the row is centered.
#[test]
fn grid_implicit_auto_column_fill_stack_centered_row_is_centered_at_full_width() {
    let row = SkiaRow::new().spacing(8).horizontal_options(CENTER).children((boxed(90, 40), boxed(90, 40), boxed(90, 40)));
    let row_id = row.id();
    let stack = SkiaStack::new().spacing(0).height_request(100).children(row);
    let laid = lay_out(375.0, 800.0, grid("", "*, 100", FILL, FILL), (boxed(10, 10).row(0), stack.row(1)));
    assert_eq!(laid.measured(1).width, 375.0);
    assert_eq!(laid.rect(1), ltrb(0, 700, 375, 800));
    assert_eq!(laid.host.rect(row_id), ltrb(45, 700, 331, 740));
}

#[test]
fn grid_fill_wrap_in_auto_column_does_not_inflate_track_past_grid() {
    let laid = lay_out(300.0, 700.0, grid("", "", FILL, FILL), flow(10, FILL));
    assert_eq!(laid.rect(0).width(), 300.0);
    assert_eq!(laid.measured(0), size(300, 40)); // 10 x 50 px boxes in 300 px = 2 lines of 20
}

#[test]
fn grid_infinite_height_does_not_stretch_last_row_to_infinity() {
    let laid = lay_out(300.0, INF, grid("", "", FILL, FILL), boxed(50, 20));
    assert_eq!(laid.size(), size(300, 20));
    assert_eq!(laid.rect(0), ltrb(0, 0, 50, 20));
}

#[test]
fn grid_implicit_auto_columns_fill_wrap_in_non_last_column_is_not_zero_width() {
    let laid = lay_out(400.0, 700.0, grid("", "", FILL, FILL), (flow(10, FILL).column(0), boxed(30, 20).column(1)));
    // The unbounded wrap (500) is cut to the grid (400); Auto tracks do not shrink for siblings, so
    // nothing is left for the second column.
    assert_eq!(laid.measured(0), size(400, 40));
    assert_eq!(laid.measured(1), size(0, 0));
}

/// C# `GridAutoSizeTests`: columns "24, *, 40", one implied Auto row, a Fill / Fill overlay in the
/// last column.
fn reply_panel(with_overlay: bool) -> Laid {
    let icon = boxed(18, 18).center().column(0);
    let content = boxed(-1, 30).fill_x().vertical_options(CENTER).column(1);
    let overlay = SkiaLayout::new().fill().children(boxed(16, 16).center()).column(2);
    let grid = SkiaGrid::new().column_spacing(10).padding((12, 8)).column_definitions("24, *, 40");
    lay_out(360.0, 700.0, grid, (icon, content, with_overlay.then_some(overlay)))
}

#[test]
fn grid_auto_row_height_adapts_to_children_with_fill_child() {
    let laid = reply_panel(true);
    // Tallest child 30 + vertical padding 16.
    assert_eq!(laid.size().height, 46.0);
    // The Fill child is measured at the resolved cell, not at the whole constraint.
    assert_eq!(laid.measured(2).height, 30.0);
}

#[test]
fn grid_padding_applied_once() {
    let laid = reply_panel(true);
    // The star column: 360 - 24 (padding) - 24 - 40 (absolute) - 20 (2 x spacing 10) = 252.
    assert_eq!(laid.rect(1), ltrb(46, 8, 298, 38));
    assert_eq!(laid.rect(0), Rect::from_xywh(15.0, 14.0, 18.0, 18.0));
}

#[test]
fn grid_auto_row_height_adapts_to_children_no_fill_children() {
    assert_eq!(reply_panel(false).size().height, 46.0);
}

/// C# `GridInScrollStackTests`: star columns resolve against the viewport when the grid sits in a
/// vertical scroll, alone or under a padded stack, with and without Fill set on the grid.
fn star_columns_in_a_scroll(inside_stack: bool, explicit_fill: bool, scale: f32) {
    const HOST: (i32, i32) = (485, 692);
    let cell = || SkiaShape::new().background_color(Color::BLUE).fill().height_request(100);
    let (left, right) = (cell().column(0), cell().column(1));
    let (left_id, right_id) = (left.id(), right.id());
    let grid = SkiaGrid::new().column_spacing(12).row_spacing(12).column_definitions("*,*").row_definitions("100");
    let grid = if explicit_fill { grid.horizontal_options(FILL) } else { grid }.children((left, right));
    let scroll = SkiaScroll::new().fill();
    let scroll = match inside_stack {
        true => scroll.content(SkiaStack::new().padding((20, 24, 20, 16)).spacing(0).children(grid)),
        false => scroll.content(grid),
    };
    let mut host = Headless::new(Ui::new((), |_| scroll), HOST.0, HOST.1, scale);
    host.settle();

    // Pixels: padding and spacing are points.
    let available = if inside_stack { HOST.0 as f32 - 40.0 * scale } else { HOST.0 as f32 };
    let expected = (available - 12.0 * scale) / 2.0;
    let (left, right) = (host.rect(left_id), host.rect(right_id));
    assert!(right.right <= HOST.0 as f32 + 1.0, "the grid overflows the viewport: {right:?}");
    assert!((left.width() - right.width()).abs() <= 1.0, "columns not equal: {left:?} {right:?}");
    assert!((left.width() - expected).abs() <= 2.0, "column {left:?}, expected width {expected}");
}

#[test]
fn star_columns_fit_viewport() {
    star_columns_in_a_scroll(false, true, 1.0);
    star_columns_in_a_scroll(true, true, 1.0);
    star_columns_in_a_scroll(true, true, 1.25);
    star_columns_in_a_scroll(true, true, 2.0);
}

#[test]
fn star_columns_without_explicit_fill_fit_viewport() {
    star_columns_in_a_scroll(true, false, 1.0);
    star_columns_in_a_scroll(true, false, 1.25);
}

/// C# `CenterAlignmentMarginTests`: the icon in the 32 pt column of a "*, 32" grid. The upstream
/// test file still expects a width of 13 and fails there: it is older than `RoundCenterAlignment`,
/// which gives the icon the odd free pixel. 14 is what the C# engine draws.
#[test]
fn centered_with_right_margin_in_narrow_cell_keeps_measured_width() {
    let icon = boxed(13, 10).center().margin((0, 0, 12, 0)).column(1);
    let id = icon.id();
    let grid = SkiaGrid::new().height_request(38).column_spacing(8).column_definitions("*, 32").children(icon);
    let mut host = Headless::new(Ui::new((), |_| grid), 400, 100, 1.0);
    host.settle();
    assert_eq!(host.rect(id).width(), 14.0);
    assert_eq!(host.rect(id), ltrb(362, 14, 376, 24));
}

// ---------------------------------------------------------------- tracks

#[test]
fn absolute_star_weighted_star_and_auto_columns() {
    let items = (
        boxed(-1, 20).fill_x().column(0),
        boxed(-1, 20).fill_x().column(1),
        boxed(-1, 25).fill_x().column(2),
        boxed(50, 20).column(3),
    );
    let laid = lay_out(400.0, 300.0, grid("100, *, 3*, Auto", "", FILL, START).column_spacing(10), items);
    // Auto takes 50; the stars share 400 - 100 - 50 - 3 x 10 = 220 as 1 : 3.
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 20), ltrb(110, 0, 165, 20), ltrb(175, 0, 340, 25), ltrb(350, 0, 400, 20)]);
    assert_eq!(laid.measured(2), size(165, 25));
    assert_eq!(laid.size(), size(400, 25));

    // The typed form is the same thing.
    let typed = [GridLength::Absolute(100.0), GridLength::STAR, GridLength::Star(3.0), GridLength::Auto];
    let items = (boxed(-1, 20).fill_x().column(1), boxed(50, 20).column(3));
    let laid = lay_out(400.0, 300.0, SkiaGrid::new().column_spacing(10).column_definitions(typed), items);
    assert_eq!(laid.rect(0), ltrb(110, 0, 165, 20));
}

#[test]
fn rows_and_a_star_row_takes_what_a_finite_height_leaves() {
    let items = || (boxed(30, 20).row(0), boxed(30, 20).row(1), boxed(30, 40).row(2));
    let laid = lay_out(200.0, 300.0, grid("", "50, *, Auto", FILL, FILL).row_spacing(5), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 30, 20), ltrb(0, 55, 30, 75), ltrb(0, 260, 30, 300)]);
    assert_eq!(laid.size(), size(200, 300));
    // Also when the grid does not fill: a star takes the height there is.
    let laid = lay_out(200.0, 300.0, grid("", "50, *, Auto", FILL, START).row_spacing(5), items());
    assert_eq!(laid.rect(2), ltrb(0, 260, 30, 300));
    assert_eq!(laid.size(), size(200, 300));

    let laid = lay_out(200.0, 400.0, grid("", "*, 3*", FILL, FILL), (boxed(30, 20).row(0), boxed(30, 20).row(1)));
    assert_eq!(laid.rect(1), ltrb(0, 100, 30, 120));

    let laid = lay_out(200.0, 300.0, grid("", "Auto, *", FILL, FILL).row_spacing(5), (boxed(30, 40), boxed(-1, -1).fill().row(1)));
    assert_eq!(laid.rect(1), ltrb(0, 45, 200, 300));
    assert_eq!(laid.measured(1), size(200, 255));
}

#[test]
fn spacing_is_between_the_tracks() {
    let items = (
        boxed(-1, 30).fill_x(),
        boxed(-1, 40).fill_x().column(1),
        boxed(-1, 20).fill_x().row(1),
        boxed(-1, 10).fill_x().column(1).row(1),
    );
    let laid = lay_out(412.0, 300.0, grid("*, *", "Auto, Auto", FILL, START).column_spacing(12).row_spacing(7), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 30), ltrb(212, 0, 412, 40), ltrb(0, 47, 200, 67), ltrb(212, 47, 412, 57)]);
    assert_eq!(laid.size(), size(412, 67));
}

#[test]
fn a_child_over_auto_tracks_makes_them_share_what_it_needs() {
    let auto = |columns: &str, rows: &str| grid(columns, rows, START, START).column_spacing(10).row_spacing(5);
    // 200 over two columns of 40 and 60 and a gap of 10: each gets 45 more.
    let items = (boxed(40, 20), boxed(60, 20).column(1), boxed(30, 20).column(2), boxed(200, 20).row(1).column_span(2));
    let laid = lay_out(400.0, 300.0, auto("Auto, Auto, Auto", "Auto, Auto"), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 40, 20), ltrb(95, 0, 155, 20), ltrb(210, 0, 240, 20), ltrb(0, 25, 200, 45)]);
    assert_eq!(laid.size(), size(240, 45));

    // Rows: 105 over 20 and 30 and a gap of 5: each gets 25 more.
    let items = (boxed(40, 105).row_span(2), boxed(60, 20).column(1), boxed(60, 30).column(1).row(1));
    let laid = lay_out(400.0, 300.0, auto("Auto, Auto", "Auto, Auto"), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 40, 105), ltrb(50, 0, 110, 20), ltrb(50, 50, 110, 80)]);
    assert_eq!(laid.size(), size(110, 105));

    // A span beyond the definitions makes Auto columns, and they share too.
    let laid = lay_out(400.0, 300.0, auto("100", ""), (boxed(250, 20).column_span(3), boxed(40, 20).column(1).row(1)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 250, 20), ltrb(110, 25, 150, 45)]);
    assert_eq!(laid.size(), size(250, 45));
}

#[test]
fn a_child_over_a_star_track_does_not_size_the_auto_one() {
    let items = (boxed(40, 20), boxed(60, 20).column(1), boxed(300, 20).row(1).column_span(2));
    let laid = lay_out(400.0, 300.0, grid("Auto, *", "Auto, Auto", FILL, START).column_spacing(10).row_spacing(5), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 40, 20), ltrb(50, 0, 110, 20), ltrb(0, 25, 300, 45)]);

    let items = (
        boxed(-1, 20).fill_x(),
        boxed(-1, 20).fill_x().column(1).column_span(2),
        boxed(-1, 30).fill_x().row(1).column_span(3),
    );
    let laid = lay_out(320.0, 300.0, grid("*, *, *", "Auto, Auto", FILL, START).column_spacing(10).row_spacing(5), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 20), ltrb(110, 0, 320, 20), ltrb(0, 25, 320, 55)]);
    assert_eq!((laid.measured(1), laid.measured(2)), (size(210, 20), size(320, 30)));
    assert_eq!(laid.size(), size(320, 55));
}

#[test]
fn a_child_beyond_the_definitions_gets_default_tracks() {
    let items = || (boxed(40, 20), boxed(60, 30).column(1), boxed(50, 25).row(1), boxed(70, 10).column(2).row(1));
    // The default is Auto.
    let laid = lay_out(400.0, 300.0, grid("", "", START, START).column_spacing(10).row_spacing(5), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 40, 20), ltrb(60, 0, 120, 30), ltrb(0, 35, 50, 60), ltrb(130, 35, 200, 45)]);
    assert_eq!(laid.size(), size(200, 60));

    let defaults = grid("", "", FILL, START).default_column_definition("*").default_row_definition("40");
    let items = (boxed(40, 20), boxed(60, 30).column(1), boxed(50, 25).row(1), boxed(-1, -1).fill().column(2).row(1));
    let laid = lay_out(410.0, 300.0, defaults.column_spacing(10).row_spacing(5), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 40, 20), ltrb(140, 0, 200, 30), ltrb(0, 45, 50, 70), ltrb(280, 45, 410, 85)]);
    assert_eq!(laid.size(), size(410, 85));

    // `split` without column definitions: that many star columns.
    let items = (boxed(-1, 20).fill_x(), boxed(-1, 30).fill_x().column(1), boxed(-1, 25).fill_x().row(1));
    let laid = lay_out(410.0, 300.0, grid("", "", FILL, START).split(2).column_spacing(10).row_spacing(5), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 200, 20), ltrb(210, 0, 410, 30), ltrb(0, 35, 200, 60)]);
}

#[test]
fn an_invisible_child_sizes_nothing() {
    let items = (boxed(40, 20), boxed(200, 80).column(1).is_visible(false), boxed(60, 30).column(2));
    let laid = lay_out(400.0, 300.0, grid("Auto, Auto, Auto", "", START, START).column_spacing(10), items);
    assert_eq!(laid.rect(2), ltrb(60, 0, 120, 30));
    assert_eq!(laid.size(), size(120, 30));
}

// ---------------------------------------------------------------- the size of the grid

#[test]
fn an_auto_size_grid_is_its_tracks_spacing_and_padding() {
    let items = (boxed(40, 20), boxed(60, 30).column(1), boxed(50, 25).row(1));
    let padded = grid("Auto, Auto", "Auto, Auto", START, START).column_spacing(10).row_spacing(5).padding((10, 6, 12, 8));
    let laid = lay_out(400.0, 300.0, padded, items);
    assert_eq!(laid.rects(), [ltrb(10, 6, 50, 26), ltrb(70, 6, 130, 36), ltrb(10, 41, 60, 66)]);
    assert_eq!(laid.size(), size(142, 74));

    let padded = grid("*, *", "Auto", FILL, START).column_spacing(10).padding((10, 6, 12, 8));
    let laid = lay_out(400.0, 300.0, padded, (boxed(-1, 20).fill_x(), boxed(-1, 30).fill_x().column(1)));
    assert_eq!(laid.rects(), [ltrb(10, 6, 194, 26), ltrb(204, 6, 388, 36)]);
    assert_eq!(laid.size(), size(400, 44));
}

#[test]
fn the_last_track_of_a_grid_that_fills_takes_what_is_left() {
    // Auto, Auto in a Fill grid: 80 and 60 -> 80 and 210.
    let items = || (panel(80, 40).fill_x(), panel(60, 30).fill_x().column(1));
    let laid = lay_out(300.0, 300.0, grid("Auto, Auto", "", FILL, START).column_spacing(10), items());
    assert_eq!(laid.rects(), [ltrb(0, 0, 80, 40), ltrb(90, 0, 300, 30)]);
    assert_eq!((laid.measured(1), laid.size()), (size(210, 30), size(300, 40)));
    // Not in a grid that takes the size of its tracks.
    let laid = lay_out(300.0, 300.0, grid("Auto, Auto", "", START, START).column_spacing(10), items());
    assert_eq!(laid.rect(1), ltrb(90, 0, 150, 30));
    assert_eq!(laid.size(), size(150, 40));
    // A width request fills too.
    let laid = lay_out(400.0, 300.0, grid("Auto, Auto", "", START, START).column_spacing(10).width_request(300), items());
    assert_eq!(laid.rect(1), ltrb(90, 0, 300, 30));

    // Absolute tracks as well.
    let items = (boxed(-1, 20).fill_x(), boxed(-1, 30).fill_x().column(1));
    let laid = lay_out(400.0, 300.0, grid("100, 100", "", FILL, START).column_spacing(10), items);
    assert_eq!(laid.rect(1), ltrb(110, 0, 400, 30));

    // Rows of a grid that fills the height.
    let items = (boxed(40, 20), panel(60, 30).fill_y().row(1));
    let laid = lay_out(400.0, 300.0, grid("", "Auto, Auto", START, FILL).row_spacing(5), items);
    assert_eq!(laid.rect(1), ltrb(0, 25, 60, 300));
    assert_eq!(laid.size(), size(60, 300));
}

#[test]
fn a_grid_wider_than_its_box() {
    // The Auto column takes all there is; the star one and its child get nothing.
    let laid = lay_out(150.0, 300.0, grid("Auto, *", "", FILL, START).column_spacing(10), (boxed(200, 20), boxed(30, 20).column(1)));
    assert_eq!((laid.measured(0), laid.measured(1)), (size(140, 20), size(0, 0)));
    assert_eq!(laid.size(), size(150, 20));

    // Absolute columns stay; the last child sticks out.
    let items = (boxed(-1, 20).fill_x(), boxed(-1, 20).fill_x().column(1), boxed(-1, 20).fill_x().column(2));
    let laid = lay_out(250.0, 300.0, grid("100, 100, 100", "", FILL, START), items);
    assert_eq!(laid.rect(2), ltrb(200, 0, 300, 20));
    assert_eq!(laid.size(), size(250, 20));

    // A child is cut to its cell.
    let laid = lay_out(300.0, 300.0, grid("100, *", "40", FILL, START).column_spacing(10), (boxed(150, 60), boxed(300, 20).column(1)));
    assert_eq!((laid.measured(0), laid.measured(1)), (size(100, 40), size(190, 20)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 100, 40), ltrb(110, 0, 300, 20)]);
}

// ---------------------------------------------------------------- children in cells

#[test]
fn a_child_sits_in_its_cell_by_its_options_and_margins() {
    let items = (
        boxed(30, 20).center(),
        boxed(30, 20).horizontal_options(END).vertical_options(END).column(1),
        boxed(30, 20).margin((5, 6, 7, 8)).column(2),
        boxed(-1, -1).fill().margin((5, 6, 7, 8)).column(3),
    );
    let laid = lay_out(500.0, 300.0, grid("100, 100, 100, 100", "60", START, START), items);
    assert_eq!(laid.rects(), [ltrb(35, 20, 65, 40), ltrb(170, 40, 200, 60), ltrb(205, 6, 235, 26), ltrb(305, 6, 393, 52)]);
    assert_eq!((laid.measured(2), laid.measured(3)), (size(42, 34), size(100, 60)));
    assert_eq!(laid.size(), size(400, 60));
}

#[test]
fn a_fill_child_in_an_auto_track_is_measured_at_the_track() {
    // Alone: the track is its content.
    let laid = lay_out(300.0, 300.0, grid("", "", START, START), panel(80, 40).fill());
    assert_eq!((laid.measured(0), laid.size()), (size(80, 40), size(80, 40)));
    // A wider sibling makes the track: measured width = arranged width.
    let laid = lay_out(300.0, 300.0, grid("", "", START, START), (panel(80, 40).fill(), boxed(120, 60).row(1)));
    assert_eq!(laid.measured(0), size(120, 40));
    assert_eq!(laid.rects(), [ltrb(0, 0, 120, 40), ltrb(0, 40, 120, 100)]);
    // A minimum of the Fill child holds the track open.
    let items = (boxed(-1, 20).fill_x().minimum_width_request(120), boxed(60, 30).column(1));
    let laid = lay_out(400.0, 300.0, grid("Auto, Auto", "", START, START).column_spacing(10), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 120, 20), ltrb(130, 0, 190, 30)]);
}

/// What wraps inside a grid. The wrap is measured at its final cell. Numbers of the C# engine with
/// its 1 px wrap inset given back (see `layout_wrap.rs`).
#[test]
fn a_wrapping_child_is_laid_out_for_its_final_cell() {
    // Not Fill, in an Auto column: offered the grid, it takes 6 of its 10 boxes per line.
    let laid = lay_out(300.0, 700.0, grid("", "", FILL, START), flow(10, START));
    assert_eq!((laid.measured(0), laid.size()), (size(300, 40), size(300, 40)));

    // In a star column with an Auto row: three per line in 199.
    let star = || grid("100, *", "", FILL, START).column_spacing(1);
    let laid = lay_out(300.0, 700.0, star(), (boxed(50, 20), flow(8, FILL).column(1)));
    assert_eq!(laid.rect(1), ltrb(101, 0, 300, 60));
    assert_eq!(laid.size(), size(300, 60));
    let laid = lay_out(300.0, 700.0, star(), (boxed(50, 20), flow(8, START).column(1)));
    assert_eq!(laid.rect(1), ltrb(101, 0, 251, 60));

    // Fill in an Auto column: measured unbounded (one line of 400, 20 tall), the track is cut to
    // the 190 the grid has. At 190 it wraps to three lines, and the Auto row grows to them (C#
    // 7cf1007c, drawnui-cross 6p: a child on a single Auto row gets the height the row can still
    // grow to; before, the row stayed one line tall and the wrap was cut to it).
    let auto = |horizontal| grid("100, Auto", "", horizontal, START).column_spacing(10);
    let laid = lay_out(300.0, 700.0, auto(FILL), (boxed(50, 20), flow(8, FILL).column(1)));
    assert_eq!(laid.rect(1), ltrb(110, 0, 300, 60));
    assert_eq!(laid.size(), size(300, 60));
    let laid = lay_out(300.0, 700.0, auto(START), (boxed(50, 20), flow(8, FILL).column(1)));
    assert_eq!(laid.rect(1), ltrb(110, 0, 300, 60));
    assert_eq!(laid.size(), size(300, 60));
}

// ---------------------------------------------------------------- unbounded, nested

#[test]
fn star_tracks_on_an_unbounded_axis_take_their_largest_child() {
    // Rows "*, 2*": the largest child is 30, so 30 and 60.
    let laid = lay_out(300.0, INF, grid("", "*, 2*", FILL, START).row_spacing(5), (boxed(50, 20), boxed(50, 30).row(1)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(0, 35, 50, 65)]);
    assert_eq!(laid.size(), size(300, 95));

    let items = (boxed(50, 20), boxed(80, 30).column(1), boxed(20, 10).column(2));
    let laid = lay_out(INF, 300.0, grid("*, *, Auto", "", START, START).column_spacing(10), items);
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(90, 0, 170, 30), ltrb(180, 0, 200, 10)]);
    assert_eq!(laid.size(), size(200, 30));

    let items = (panel(50, 20).fill_x(), panel(80, 30).fill_x().column(1));
    let laid = lay_out(INF, 300.0, grid("*, *", "", FILL, START).column_spacing(10), items);
    assert_eq!((laid.measured(0), laid.measured(1), laid.size()), (size(80, 20), size(80, 30), size(170, 30)));
}

#[test]
fn a_grid_in_an_unbounded_column() {
    let (a, b, last) = (boxed(-1, 40).fill_x(), boxed(-1, 60).fill_x().column(1), boxed(50, 10));
    let (a_id, b_id, last_id) = (a.id(), b.id(), last.id());
    let grid = grid("*, *", "Auto", FILL, START).column_spacing(10).children((a, b));
    let grid_id = grid.id();
    let column = SkiaStack::new().spacing(0).children((boxed(-1, 100).fill_x(), grid, last));
    let column_id = column.id();
    let mut host = Headless::new(Ui::new((), |_| Build::new(Probe(300.0, INF)).children(column)), 1000, 1000, 1.0);
    host.settle();
    let measured = |id: ControlId| host.ui.tree.base(id).unwrap().measured;
    assert_eq!((measured(a_id), measured(b_id)), (size(145, 40), size(145, 60)));
    assert_eq!(host.rect(grid_id), ltrb(0, 100, 300, 160));
    assert_eq!(host.rect(last_id), ltrb(0, 160, 50, 170));
    assert_eq!(measured(column_id), size(300, 170));
}

#[test]
fn nested_grids() {
    // A Fill grid in a star column.
    let (a, b) = (boxed(-1, 20).fill_x(), boxed(-1, 30).fill_x().column(1));
    let (a_id, b_id) = (a.id(), b.id());
    let inner = grid("*, *", "Auto", FILL, START).column_spacing(10).children((a, b));
    let outer = grid("100, *", "Auto, Auto", FILL, START).column_spacing(10).row_spacing(5);
    let laid = lay_out(320.0, 300.0, outer, (boxed(50, 20), inner.column(1), boxed(40, 40).row(1)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 50, 20), ltrb(110, 0, 320, 30), ltrb(0, 35, 40, 75)]);
    assert_eq!((laid.host.rect(a_id), laid.host.rect(b_id)), (ltrb(110, 0, 210, 20), ltrb(220, 0, 320, 30)));
    assert_eq!(laid.size(), size(320, 75));

    // A grid that takes the size of its tracks, in an Auto column.
    let (a, b) = (boxed(40, 20), boxed(60, 30).column(1));
    let (a_id, b_id) = (a.id(), b.id());
    let inner = grid("Auto, Auto", "Auto", START, START).column_spacing(10).children((a, b));
    let outer = grid("Auto, *", "Auto", FILL, START).column_spacing(10).row_spacing(5);
    let laid = lay_out(320.0, 300.0, outer, (inner, boxed(-1, 20).fill_x().column(1)));
    assert_eq!(laid.rects(), [ltrb(0, 0, 110, 30), ltrb(120, 0, 320, 20)]);
    assert_eq!((laid.host.rect(a_id), laid.host.rect(b_id)), (ltrb(0, 0, 40, 20), ltrb(50, 0, 110, 30)));
}

// ---------------------------------------------------------------- defaults, measures

#[test]
fn defaults_of_the_alias() {
    let mut tree = drawnui::Tree::default();
    let id = tree.mount(None, SkiaGrid::new());
    let (layout, base) = (tree.find::<SkiaLayout>(id).unwrap(), tree.base(id).unwrap());
    assert_eq!(layout.p.layout_type, LayoutType::Grid);
    assert_eq!((layout.p.column_spacing, layout.p.row_spacing), (1.0, 1.0));
    assert_eq!((layout.p.default_column_definition, layout.p.default_row_definition), (GridLength::Auto, GridLength::Auto));
    assert!(layout.p.column_definitions.is_empty() && layout.p.row_definitions.is_empty());
    assert_eq!((base.p.horizontal_options, base.p.vertical_options), (FILL, START));
    assert_eq!((base.p.column, base.p.row, base.p.column_span, base.p.row_span), (0, 0, 1, 1));
}

/// A child in an Auto track is measured for its content and then at its cell; any other child
/// only at its cell. The constraints are the ones upstream gives; on a single Auto row the height
/// is what the row can still grow to (C# 7cf1007c AvailableHeight: 300 - 75 + 20 = 245).
#[test]
fn the_measures_of_a_first_layout() {
    let logs: Vec<Seen> = (0..5).map(|_| Seen::default()).collect();
    let items = (
        spy(&logs[0]),
        spy(&logs[1]).fill_x().column(1),
        spy(&logs[2]).column(2),
        spy(&logs[3]).fill_x().row(1),
        spy(&logs[4]).column(2).row(1),
    );
    let laid = lay_out(400.0, 300.0, grid("Auto, *, 100", "Auto, 50", FILL, START).column_spacing(10).row_spacing(5), items);
    assert_eq!(*logs[0].borrow(), [(280.0, 245.0), (40.0, 245.0)]); // Auto column, Auto row
    // Star column, Auto row: its content measure is already at its cell (the second is a memo hit).
    assert_eq!(*logs[1].borrow(), [(240.0, 245.0)]);
    assert_eq!(*logs[2].borrow(), [(100.0, 245.0)]); // absolute column, Auto row: the same
    assert_eq!(*logs[3].borrow(), [(INF, 50.0), (40.0, 50.0)]); // Fill in the Auto column, absolute row
    assert_eq!(*logs[4].borrow(), [(100.0, 50.0)]); // absolute column and row
    assert_eq!(laid.rects()[1], ltrb(50, 0, 290, 20));
    assert_eq!(laid.size(), size(400, 75));

    // No Auto track: one measure each.
    let logs: Vec<Seen> = (0..4).map(|_| Seen::default()).collect();
    let items = (spy(&logs[0]).fill(), spy(&logs[1]).column(1), spy(&logs[2]).row(1), spy(&logs[3]).column(1).row(1));
    lay_out(400.0, 300.0, grid("*, 100", "*, 50", FILL, FILL).column_spacing(10).row_spacing(5), items);
    let seen: Vec<_> = logs.iter().map(|log| log.borrow().clone()).collect();
    assert_eq!(seen, [[(290.0, 245.0)], [(100.0, 245.0)], [(290.0, 50.0)], [(100.0, 50.0)]]);
}

#[test]
fn nothing_is_measured_while_nothing_changes_and_a_change_measures_only_what_it_moves() {
    let logs: Vec<Seen> = (0..4).map(|_| Seen::default()).collect();
    let items = (spy(&logs[0]), spy(&logs[1]).column(1), spy(&logs[2]).row(1), spy(&logs[3]).column(1).row(1));
    let mut laid = lay_out(400.0, 300.0, grid("Auto, Auto", "Auto, Auto", START, START).column_spacing(10).row_spacing(5), items);
    let counts = || logs.iter().map(|log| log.borrow().len()).collect::<Vec<_>>();
    assert_eq!(counts(), [2, 2, 2, 2]);
    laid.host.frame();
    laid.host.frame();
    assert_eq!(counts(), [2, 2, 2, 2]);

    let change = |laid: &mut Laid, index: usize, set: &dyn Fn(&mut Mut<'_, Spy>)| {
        set(&mut laid.host.ui.tree.find_mut::<Spy>(laid.items[index]).unwrap());
        laid.host.settle();
    };
    change(&mut laid, 1, &|spy| spy.set_opacity(0.5));
    assert_eq!(counts(), [2, 2, 2, 2]);
    // Measured again, same size: its two measures, nothing else. The others keep the content
    // measure the grid remembers and their cell.
    laid.host.ui.tree.invalidate(laid.items[3], Dirty::MEASURE);
    laid.host.settle();
    assert_eq!(counts(), [2, 2, 2, 4]);
    // The last child gets taller: the row does. The neighbor on that row keeps its offer (the room
    // the row can grow to is the same); the first row's children have less room left, so they
    // are measured once more (C# 7cf1007c offers that room too).
    change(&mut laid, 3, &|spy| spy.set_minimum_height_request(30));
    assert_eq!(counts(), [3, 3, 2, 6]);
}
