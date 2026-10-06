//! Wrap with `split`, `split_align` and `dynamic_columns`, and Row / Wrap / Grid with `items`
//! (every item gets a cell, as DrawnUi.React lays them out; `invert` fills a Grid down its
//! columns). Every number was read from the React engine (`dist` of DrawnUi.React run in node with
//! the same trees, scales 1 and 2, 400 x 300 points): boxes of 100 x 30, spacing 8, so that the
//! React numbers are whole pixels and equal here.

use drawnui::prelude::*;
use drawnui::testing::Headless;

type Layout = Build<SkiaLayout>;

fn boxed(width: i32, height: i32) -> Layout {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// Rects of the visible children of `layout` in order, and the layout's measured size.
struct Laid {
    rects: Vec<Rect>,
    measured: Size,
}

fn lay_out(root: Layout, scale: f32) -> Laid {
    let id = root.id();
    let mut host = Headless::new(Ui::new((), |_| root), (400.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    let tree = &host.ui.tree;
    let rects = tree
        .children(id)
        .iter()
        .filter(|child| tree.base(**child).is_some_and(|base| base.p.is_visible))
        .map(|child| host.rect(*child))
        .collect();
    Laid { rects, measured: tree.base(id).unwrap().measured }
}

fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left as f32, top as f32, right as f32, bottom as f32)
}

/// The same rects at scale 2, doubled: every number of these trees is whole at both scales.
fn at(scale: f32, rects: &[Rect]) -> Vec<Rect> {
    rects.iter().map(|r| Rect::new(r.left * scale, r.top * scale, r.right * scale, r.bottom * scale)).collect()
}

const SCALES: [f32; 2] = [1.0, 2.0];

/// Seven boxes in three columns: the slot is (400 - 2 x 8) / 3 = 128 wide, a box starts at the
/// start of its slot; the last box, alone on its line, is centered in its slot.
const SPLIT3: [Rect; 7] = [
    Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 30.0 },
    Rect { left: 136.0, top: 0.0, right: 236.0, bottom: 30.0 },
    Rect { left: 272.0, top: 0.0, right: 372.0, bottom: 30.0 },
    Rect { left: 0.0, top: 38.0, right: 100.0, bottom: 68.0 },
    Rect { left: 136.0, top: 38.0, right: 236.0, bottom: 68.0 },
    Rect { left: 272.0, top: 38.0, right: 372.0, bottom: 68.0 },
    Rect { left: 14.0, top: 76.0, right: 114.0, bottom: 106.0 },
];

fn seven(last: LayoutOptions) -> Vec<Layout> {
    (0..7).map(|i| boxed(100, 30).horizontal_options(if i == 6 { last } else { LayoutOptions::Start })).collect()
}

#[test]
fn split_gives_every_child_a_slot_of_the_same_width() {
    for scale in SCALES {
        let laid = lay_out(SkiaWrap::new().spacing(8).split(3).children(seven(LayoutOptions::Center)), scale);
        assert_eq!(laid.rects, at(scale, &SPLIT3), "@{scale}");
        assert_eq!(laid.measured, Size::new(400.0 * scale, 106.0 * scale), "@{scale}");
    }
}

#[test]
fn dynamic_columns_give_a_short_last_line_the_whole_width() {
    for scale in SCALES {
        let wrap = SkiaWrap::new().spacing(8).split(3).dynamic_columns(true);
        let laid = lay_out(wrap.children(seven(LayoutOptions::Center)), scale);
        // Alone on its line: one slot 400 wide, the box centered in it.
        let mut expected = SPLIT3;
        expected[6] = ltrb(150, 76, 250, 106);
        assert_eq!(laid.rects, at(scale, &expected), "@{scale}");

        // Fill boxes: 128 each on a full line, (400 - 8) / 2 = 196 each on the last one.
        let fills: Vec<Layout> = (0..5).map(|_| boxed(-1, 30).fill_x()).collect();
        let laid = lay_out(SkiaWrap::new().spacing(8).split(3).dynamic_columns(true).children(fills), scale);
        let expected = [ltrb(0, 0, 128, 30), ltrb(136, 0, 264, 30), ltrb(272, 0, 400, 30), ltrb(0, 38, 196, 68), ltrb(204, 38, 400, 68)];
        assert_eq!(laid.rects, at(scale, &expected), "@{scale}");
        assert_eq!(laid.measured, Size::new(400.0 * scale, 68.0 * scale), "@{scale}");
    }
}

#[test]
fn without_split_align_the_next_child_follows_the_width_of_this_one() {
    for scale in SCALES {
        let wrap = SkiaWrap::new().spacing(8).split(3).split_align(false);
        let laid = lay_out(wrap.children(seven(LayoutOptions::Start)), scale);
        let row = |top: i32| [ltrb(0, top, 100, top + 30), ltrb(108, top, 208, top + 30), ltrb(216, top, 316, top + 30)];
        let expected = [row(0), row(38), [ltrb(0, 76, 100, 106); 3]].concat();
        assert_eq!(laid.rects, at(scale, &expected[..7]), "@{scale}");
    }
}

// ---------------------------------------------------------------- items

/// A templated layout of `count` boxes of 100 x 30.
fn templated(layout: Layout, count: usize) -> Layout {
    layout.items(move |_: &()| count, || (boxed(100, 30), ()), |_: &(), _: &(), _, _| {})
}

#[test]
fn a_wrap_with_items_lays_a_cell_per_item_out() {
    for scale in SCALES {
        let wrap = SkiaWrap::new().spacing(8).split(3).dynamic_columns(true);
        let laid = lay_out(templated(wrap, 7), scale);
        let mut expected = SPLIT3;
        expected[6] = ltrb(0, 76, 100, 106);
        assert_eq!(laid.rects, at(scale, &expected), "@{scale}");
        assert_eq!(laid.measured, Size::new(400.0 * scale, 106.0 * scale), "@{scale}");
    }
}

/// The fourth cell ends at 424, past the 400 of the row: a templated Row measures its cells
/// unbounded along it, in React and in C# (probe: 324..424 at scale 1; C# draws only the cells in
/// the viewport). A Row of the same static boxes cuts the fourth to 324..400 in both C# and here.
#[test]
fn a_row_with_items_lays_a_cell_per_item_out() {
    for scale in SCALES {
        let laid = lay_out(templated(SkiaRow::new().spacing(8), 4), scale);
        let expected: Vec<Rect> = (0..4).map(|i| ltrb(i * 108, 0, i * 108 + 100, 30)).collect();
        assert_eq!(laid.rects, at(scale, &expected), "@{scale}");
    }
}

#[test]
fn a_grid_with_items_puts_item_i_in_column_i_mod_split_and_with_invert_down_the_columns() {
    for scale in SCALES {
        let grid = || SkiaGrid::new().column_definitions("Auto, Auto, Auto").column_spacing(8).row_spacing(8).split(3);
        let laid = lay_out(templated(grid(), 7), scale);
        let cell = |column: i32, row: i32| ltrb(column * 108, row * 38, column * 108 + 100, row * 38 + 30);
        let along: Vec<Rect> = (0..7).map(|i| cell(i % 3, i / 3)).collect();
        assert_eq!(laid.rects, at(scale, &along), "@{scale}");
        assert_eq!(laid.measured, Size::new(400.0 * scale, 106.0 * scale), "@{scale}");

        // Seven items in three columns: ceil(7 / 3) = 3 rows, filled column by column.
        let laid = lay_out(templated(grid().invert(true), 7), scale);
        let down: Vec<Rect> = (0..7).map(|i| cell(i / 3, i % 3)).collect();
        assert_eq!(laid.rects, at(scale, &down), "@{scale} invert");
    }
}

// ---------------------------------------------------------------- decorated grid

const GRID_BACK: Color = Color::from_rgb(0x21, 0x25, 0x29);

/// The React page's decorated grid over 403 points: 12 items in 4 star columns with spacing 1, so
/// the columns are 100 wide and start every 101; rows of 30 start every 31 (92 in all).
fn decorated() -> Build<SkiaDecoratedGrid> {
    SkiaDecoratedGrid::new()
        .split(4)
        .column_definitions("*,*,*,*")
        .column_spacing(1)
        .row_spacing(1)
        .background_color(GRID_BACK)
        .items(|_: &()| 12, || (boxed(100, 30), ()), |_: &(), _: &(), _, _| {})
}

fn painted(grid: Build<SkiaDecoratedGrid>, scale: f32) -> (Headless<()>, ControlId) {
    let id = grid.id();
    let mut host = Headless::new(Ui::new((), |_| grid), (403.0 * scale) as i32, (120.0 * scale) as i32, scale);
    host.settle();
    (host, id)
}

/// `src` at `alpha` over `dst`, per channel.
fn over(src: Color, alpha: u8, dst: Color) -> [f32; 3] {
    let a = alpha as f32 / 255.0;
    let mix = |s: u8, d: u8| s as f32 * a + d as f32 * (1.0 - a);
    [mix(src.r(), dst.r()), mix(src.g(), dst.g()), mix(src.b(), dst.b())]
}

fn near(color: Color, expected: [f32; 3]) -> bool {
    [color.r(), color.g(), color.b()].iter().zip(expected).all(|(c, e)| (*c as f32 - e).abs() <= 2.0)
}

#[test]
fn a_decorated_grid_lays_its_items_out_and_paints_lines_in_its_spacing() {
    let band = Color::from_rgb(0xE8, 0xE3, 0xD7);
    for scale in SCALES {
        let (mut host, id) = painted(decorated(), scale);
        let tree = &host.ui.tree;
        let cells: Vec<Rect> = tree.children(id).iter().map(|cell| host.rect(*cell)).collect();
        let expected: Vec<Rect> = (0..12).map(|i| ltrb(i % 4 * 101, i / 4 * 31, i % 4 * 101 + 100, i / 4 * 31 + 30)).collect();
        assert_eq!(cells, at(scale, &expected), "@{scale}");
        assert_eq!(tree.base(id).unwrap().measured, Size::new(403.0 * scale, 92.0 * scale), "@{scale}");

        let pixel = |host: &mut Headless<()>, x: f32, y: f32| host.pixel((x * scale) as i32, (y * scale) as i32);
        // Between columns (x 100, 201, 302): the band at its 0x78 alpha over the background, away
        // from the faded ends.
        for x in [100.0, 201.0, 302.0] {
            let color = pixel(&mut host, x, 15.0);
            assert!(near(color, over(band, 0x78, GRID_BACK)), "@{scale} x {x}: {color:?}");
        }
        // Between rows (y 30, 61): the band over black.
        for y in [30.0, 61.0] {
            let color = pixel(&mut host, 50.0, y);
            assert!(near(color, over(band, 0x78, Color::BLACK)), "@{scale} y {y}: {color:?}");
        }
        // Inside a cell: the background.
        assert_eq!(pixel(&mut host, 50.0, 15.0), GRID_BACK, "@{scale}");
    }
}

#[test]
fn a_decorated_grid_without_lines_paints_its_spacing_as_its_background() {
    let grid = decorated().vertical_line(None).horizontal_line(None);
    let (mut host, _) = painted(grid, 1.0);
    assert_eq!((host.pixel(100, 15), host.pixel(50, 30)), (GRID_BACK, GRID_BACK));
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_a_decorated_grid() {
    const FRAMES: u32 = 500;
    for scale in [1.0f32, 2.0] {
        for lines in [false, true] {
            let grid = if lines { decorated() } else { decorated().vertical_line(None).horizontal_line(None) };
            let (mut host, _) = painted(grid, scale);
            let start = std::time::Instant::now();
            for _ in 0..FRAMES {
                host.frame_after(16.0);
            }
            let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
            println!("scale {scale}, 12 cells, lines {lines}: {micros:.1} us per frame");
        }
    }
}

// ---------------------------------------------------------------- ported upstream (StackWrapSweepTests)

/// A templated layout of three cells made by `cell` (C# `Templated`: ItemsSource of 3).
fn three(layout: Layout, cell: fn() -> Layout) -> Layout {
    layout.items(|_: &()| 3, move || (cell(), ()), |_: &(), _: &(), _, _| {})
}

#[test]
fn templated_row_fill_x_cells_finite_width_are_content_sized() {
    let row = three(SkiaRow::new().spacing(0).vertical_options(LayoutOptions::Fill), || {
        SkiaLayout::new().fill().children(boxed(30, 80))
    });
    let id = row.id();
    let mut host = Headless::new(Ui::new((), |_| row), 1000, 300, 1.0);
    host.settle();
    let tree = &host.ui.tree;
    assert_eq!(tree.base(id).unwrap().measured.width, 90.0);
    let cells: Vec<Rect> = tree.children(id).iter().map(|cell| host.rect(*cell)).collect();
    assert_eq!(cells, [ltrb(0, 0, 30, 300), ltrb(30, 0, 60, 300), ltrb(60, 0, 90, 300)]);
}

#[test]
fn column_split2_templated_slot_is_column_width_not_measured_width() {
    let column = three(SkiaStack::new().spacing(0).split(2), || boxed(50, 20).horizontal_options(LayoutOptions::End));
    let laid = lay_out(column, 1.0);
    assert_eq!(laid.rects, [ltrb(150, 0, 200, 20), ltrb(350, 0, 400, 20), ltrb(150, 20, 200, 40)]);
}

#[test]
fn column_split2_templated_center_cells_not_centered_twice_at_draw() {
    let column = three(SkiaStack::new().spacing(10).split(2), || boxed(50, 20).horizontal_options(LayoutOptions::Center));
    let id = column.id();
    let mut host = Headless::new(Ui::new((), |_| column), 410, 300, 1.0);
    host.settle();
    let cells: Vec<Rect> = host.ui.tree.children(id).iter().map(|cell| host.rect(*cell)).collect();
    assert_eq!(cells[..2], [ltrb(75, 0, 125, 20), ltrb(285, 0, 335, 20)]);
}
