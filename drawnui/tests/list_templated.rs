//! Templated layouts that are not a list (Row, Wrap, Grid, a Column with `split`): one cell per
//! item, laid out like children, as DrawnUi.React does. Plus the groundwork of a drag to reorder:
//! `items_moved`, the item under a pixel, the cell of an item and where it is drawn, a lifted row
//! that draws blank (DrawnUi.React ReorderPage), the debug line.
//! Ports SplitGridFirstItemTests (ClearThenAppend_OneCellPerItem, FilledBeforeAttach 3 of 6) and
//! ScrollToIndexSplitTests (`DrawnUi.Net.Tests`) with their numbers.

mod list_common;

use drawnui::controls::layout::{MeasureBudget, MeasuringStrategy};
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::*;

#[derive(Default)]
struct App {
    /// Widths (a Row) or heights (a Column) of the items, points.
    sizes: Vec<f32>,
    list: Handle<SkiaLayout>,
    twin: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
    counters: Counters,
}

/// A cell: a shape whose size the bind sets from the item.
fn shape_cell() -> (Build<SkiaShape>, Handle<SkiaShape>) {
    let mut shape = Handle::default();
    (SkiaShape::new().assign(&mut shape), shape)
}

fn rects(host: &Headless<App>, layout: Handle<SkiaLayout>) -> Vec<Rect> {
    let tree = &host.ui.tree;
    let top = tree.base(layout).unwrap().rect.top;
    tree.children(layout)
        .iter()
        .filter_map(|c| tree.base(*c))
        .filter(|b| b.p.is_visible)
        .map(|b| b.rect.with_offset((0.0, -top)))
        .collect()
}

#[test]
fn a_templated_row_lays_its_items_out_like_children() {
    // The templated row and, under it, the same row with static children of the same sizes.
    let build = |app: &mut App| {
        let created = app.counters.created.clone();
        let statics: Vec<Build<SkiaShape>> = app.sizes.iter().map(|w| SkiaShape::new().width_request(*w).height_request(30)).collect();
        SkiaLayout::column().spacing(0).children((
            SkiaLayout::row().spacing(4).assign(&mut app.list).items(
                |app: &App| app.sizes.len(),
                move || {
                    count(&created);
                    shape_cell()
                },
                |shape: &Handle<SkiaShape>, app: &App, index, cx| {
                    if let Some(mut shape) = cx.get_mut(*shape) {
                        shape.set_width_request(app.sizes[index]);
                        shape.set_height_request(30);
                    }
                },
            ),
            SkiaLayout::row().spacing(4).assign(&mut app.twin).children(statics),
        ))
    };
    let mut host = Headless::new(Ui::new(App { sizes: vec![40.0, 70.0, 25.0, 100.0], ..App::default() }, build), 400, 300, 1.0);
    host.settle();
    let (list, twin) = (host.ui.state.list, host.ui.state.twin);
    let cells = rects(&host, list);
    assert_eq!(cells.len(), 4);
    assert_eq!(cells, rects(&host, twin));
    assert_eq!((cells[1].left, cells[3].right), (44.0, 44.0 + 70.0 + 4.0 + 25.0 + 4.0 + 100.0));
    assert_eq!(host.ui.state.counters.created.get(), 4);

    // An item removed: its cell is spare, the rest close up like children would.
    host.ui.state.sizes.remove(1);
    host.ui.tree.cx().items_removed(list, 1, 1);
    host.ui.state_changed();
    host.settle();
    let cells = rects(&host, list);
    assert_eq!(cells.len(), 3);
    assert_eq!((cells[1].left, cells[1].width()), (44.0, 25.0));
    assert_eq!(host.ui.state.counters.created.get(), 4);
}

/// ScrollToIndexSplitTests: `[InlineData(split, count, index, expectedRow)]`, cells of 100 in a
/// 300 x 600 viewport; `Assert.Equal(-expectedRow * CellHeight, offset)`.
#[test]
fn scroll_to_index_lands_on_the_items_row_in_a_split_column() {
    for (split, count, index, expected_row) in [(3, 90, 26, 8), (3, 90, 1, 0), (3, 90, 30, 10), (1, 40, 8, 8), (2, 60, 15, 7)] {
        let build = move |app: &mut App| {
            let list = SkiaLayout::column()
                .split(split)
                .spacing(0)
                .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                .assign(&mut app.list)
                .items(move |_: &App| count, || (SkiaShape::new().fill_x().height_request(100).background_color(Color::BLUE), ()), |_: &(), _: &App, _, _| {});
            Viewport::new(false).fill().assign(&mut app.viewport).children((list,))
        };
        let mut host = Headless::new(Ui::new(App::default(), build), 300, 600, 1.0);
        host.settle();
        let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
        let offset = layout(&host, list).item_offset_pixels(index);
        assert_eq!(offset, expected_row as f32 * 100.0, "split {split} index {index}");
        scroll_to(&mut host, viewport, offset);
        host.settle();
        let cell = layout(&host, list).cell_in_use(index).expect("every item has a cell");
        // Rects are in content pixels: the scroll moves the row by its offset when it paints.
        assert_eq!(host.rect(cell).top - host.rect(viewport).top, offset);
        assert_eq!(scrolled(&host, viewport), offset);
        // A split column is no list, but it recycles (RecyclingTemplate Enabled): views only for
        // the items that can be seen, the one asked for among them.
        let in_use = layout(&host, list).cells_in_use().count();
        assert!(in_use > 0 && in_use < count, "split {split}: {in_use} cells for {count} items");
    }
}

/// SplitGridFirstItemTests.ClearThenAppend_OneCellPerItem: a 3-column grid of 134 pt tiles fed
/// like a paged gallery (cleared and filled again, then one more), `[InlineData(1 | 2 | 4)]`.
#[test]
fn clear_then_append_one_cell_per_item() {
    for count in [1usize, 2, 4] {
        let build = |app: &mut App| {
            let grid = SkiaLayout::column()
                .split(3)
                .spacing(2)
                .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                .assign(&mut app.list)
                .items(|app: &App| app.sizes.len(), shape_cell, |shape: &Handle<SkiaShape>, _: &App, _, cx| {
                    if let Some(mut shape) = cx.get_mut(*shape) {
                        shape.set_height_request(134);
                        shape.set_horizontal_options(LayoutOptions::Fill);
                        shape.set_background_color(Color::DARK_GRAY);
                    }
                });
            Viewport::new(false).fill().assign(&mut app.viewport).children((grid,))
        };
        let mut host = Headless::new(Ui::new(App::default(), build), 402, 700, 1.0);
        host.settle();
        let list = host.ui.state.list;
        let rows = |count: usize| count.div_ceil(3) as f32;
        let refill = |host: &mut Headless<App>, count: usize| {
            host.ui.state.sizes = vec![0.0; count];
            host.ui.tree.cx().items_reset(list);
            host.ui.state_changed();
            host.settle();
        };

        // first visit: empty grid, then the page
        refill(&mut host, count);
        assert_eq!(layout(&host, list).cells_in_use().count(), count);
        assert_eq!(content_height(&host, list), rows(count) * 134.0 + (rows(count) - 1.0) * 2.0);
        // back to the camera and here again: same page, re-applied
        refill(&mut host, count);
        assert_eq!(layout(&host, list).cells_in_use().count(), count);
        // a second shot taken: the page is one longer
        refill(&mut host, count + 1);
        assert_eq!(layout(&host, list).cells_in_use().count(), count + 1);
        assert_eq!(content_height(&host, list), rows(count + 1) * 134.0 + (rows(count + 1) - 1.0) * 2.0);
        // Every visible cell is a whole tile.
        for (_, cell) in layout(&host, list).cells_in_use() {
            assert_eq!(host.rect(cell).height(), 134.0);
        }
    }
}

#[test]
fn a_templated_grid_puts_item_i_in_column_i_mod_split() {
    let build = |app: &mut App| {
        let grid = SkiaLayout::grid().split(3).column_spacing(0).row_spacing(0).assign(&mut app.list).items(
            |app: &App| app.sizes.len(),
            shape_cell,
            |shape: &Handle<SkiaShape>, _: &App, index, cx| {
                if let Some(mut shape) = cx.get_mut(*shape) {
                    shape.set_height_request(50);
                    shape.set_horizontal_options(LayoutOptions::Fill);
                    shape.set_background_color(color(index));
                }
            },
        );
        SkiaLayout::new().fill().children((grid,))
    };
    let mut host = Headless::new(Ui::new(App { sizes: vec![0.0; 7], ..App::default() }, build), 300, 300, 1.0);
    host.settle();
    let list = host.ui.state.list;
    let cells: Vec<(usize, ControlId)> = layout(&host, list).cells_in_use().collect();
    for (index, cell) in cells {
        // The grid places the item's slot (the cell's parent; a Disabled layout places the cell).
        let slot = host.ui.tree.parent(cell).filter(|parent| *parent != list.id()).unwrap_or(cell);
        let placed = host.ui.tree.base(slot).unwrap();
        assert_eq!((placed.p.column, placed.p.row), ((index % 3) as i32, (index / 3) as i32));
        let base = host.ui.tree.base(cell).unwrap();
        assert_eq!(base.rect, Rect::from_xywh((index % 3) as f32 * 100.0, (index / 3) as f32 * 50.0, 100.0, 50.0));
        assert_eq!(host.pixel((index % 3) as i32 * 100 + 50, (index / 3) as i32 * 50 + 25), color(index));
    }
    assert_eq!(layout(&host, list).cells_in_use().count(), 7);
    assert_eq!(layout(&host, list).item_offset_pixels(6), 100.0);
    assert_eq!(layout(&host, list).item_at_pixels(120.0), Some(6));
}

/// A templated wrap that goes from 200 items to 5 keeps the pool limit of spare cells (four times
/// the cells in use), not 195 hidden children its layout walks through.
#[test]
fn a_templated_layout_that_shrinks_drops_its_spare_cells() {
    let build = |app: &mut App| {
        let wrap = SkiaWrap::new().spacing(2).assign(&mut app.list).items(
            |app: &App| app.sizes.len(),
            shape_cell,
            |shape: &Handle<SkiaShape>, _: &App, _, cx| {
                if let Some(mut shape) = cx.get_mut(*shape) {
                    shape.set_width_request(10);
                    shape.set_height_request(10);
                }
            },
        );
        SkiaLayout::new().fill().children((wrap,))
    };
    let mut host = Headless::new(Ui::new(App { sizes: vec![0.0; 200], ..App::default() }, build), 300, 300, 1.0);
    host.settle();
    let list = host.ui.state.list;
    assert_eq!((layout(&host, list).cells_in_use().count(), cells(&host, list)), (200, 200));
    host.ui.state.sizes.truncate(5);
    host.ui.state_changed();
    host.settle();
    assert_eq!((layout(&host, list).cells_in_use().count(), cells(&host, list)), (5, 20));
    assert_eq!(rects(&host, list).len(), 5);
}

// ---------------------------------------------------------------- reorder groundwork

#[derive(Clone, Copy, PartialEq, Debug)]
struct Item {
    id: usize,
    height: f32,
}

#[derive(Default)]
struct Rows {
    items: Vec<Item>,
    /// The id of the item a drag lifted: its row draws blank (DrawnUi.React ReorderCell).
    dragging: Option<usize>,
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
    counters: Counters,
}

fn item(id: usize) -> Item {
    Item { id, height: 30.0 + (id * 37 % 50) as f32 }
}

fn rows_host(strategy: MeasuringStrategy, uniform: bool) -> Headless<Rows> {
    let build = move |app: &mut Rows| {
        let binds = app.counters.binds.clone();
        let list = SkiaLayout::column()
            .spacing(2)
            .measure_items_strategy(strategy)
            .measure_budget(MeasureBudget::Items(0))
            .assign(&mut app.list)
            .items(
                |app: &Rows| app.items.len(),
                shape_cell,
                move |shape: &Handle<SkiaShape>, app: &Rows, index, cx| {
                    count(&binds);
                    if let Some(mut shape) = cx.get_mut(*shape) {
                        shape.set_horizontal_options(LayoutOptions::Fill);
                        shape.set_height_request(app.items[index].height);
                        shape.set_background_color(color(app.items[index].id));
                        shape.set_opacity(if app.dragging == Some(app.items[index].id) { 0.0 } else { 1.0 });
                    }
                },
            );
        Viewport::new(false).width_request(200).height_request(400).is_clipped_to_bounds(true).assign(&mut app.viewport).children((list,))
    };
    let items = (0..200).map(|id| if uniform { Item { id, height: 40.0 } } else { item(id) }).collect();
    let mut host = Headless::new(Ui::new(Rows { items, ..Rows::default() }, build).background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    host
}

/// The rows with a cell run from above the viewport's top to below its bottom (400).
fn covers_viewport(rows: &[(usize, f32, f32)]) {
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    assert!(first.1 <= 0.0 && first.1 + first.2 > 0.0, "{rows:?}");
    assert!(last.1 < 400.0 && last.1 + last.2 >= 400.0, "{rows:?}");
}

/// (id, top on screen, height) of the rows on screen.
fn shown(host: &Headless<Rows>) -> Vec<(usize, f32, f32)> {
    let app = &host.ui.state;
    rows(host, app.viewport, app.list).into_iter().map(|(index, top, height)| (app.items[index].id, top, height)).collect()
}

fn move_item(host: &mut Headless<Rows>, from: usize, to: usize) {
    let list = host.ui.state.list;
    let moved = host.ui.state.items.remove(from);
    host.ui.state.items.insert(to, moved);
    host.ui.tree.cx().items_moved(list, from, to);
    host.ui.state_changed();
    host.frame();
}

#[test]
fn a_moved_item_takes_its_size_along_and_nothing_is_bound_again() {
    for (strategy, uniform) in [(MeasuringStrategy::MeasureVisible, false), (MeasuringStrategy::MeasureFirst, true), (MeasuringStrategy::MeasureAll, false)] {
        let mut host = rows_host(strategy, uniform);
        let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
        scroll_to(&mut host, viewport, 500.0);
        host.settle();
        let before = shown(&host);
        let binds = host.ui.state.counters.binds.get();
        let measured = layout(&host, list).measured_items();

        // The second row on screen goes two rows down: the rows between move up, its size with it.
        let second = host.ui.state.items.iter().position(|i| i.id == before[1].0).unwrap();
        move_item(&mut host, second, second + 2);
        let after = shown(&host);
        assert_eq!(after[0], before[0]);
        assert_eq!((after[1].0, after[2].0, after[3].0), (before[2].0, before[3].0, before[1].0));
        for row in &after {
            assert_eq!(row.2, host.ui.state.items.iter().find(|i| i.id == row.0).unwrap().height, "height travels with item {}", row.0);
        }
        for pair in after.windows(2) {
            assert_eq!(pair[1].1, pair[0].1 + pair[0].2 + 2.0);
        }
        assert_eq!(scrolled(&host, viewport), 500.0);
        assert_eq!(host.ui.state.counters.binds.get(), binds, "{strategy:?}: the cells keep their items");
        assert_eq!(layout(&host, list).measured_items(), measured);

        // A row from above the viewport goes to the end: the scroll keeps its offset (as upstream
        // and React), so every row on screen moves up by that row and its gap, and only the rows
        // that enter at the bottom are bound.
        let lifted = host.ui.state.items[0].height + 2.0;
        let last = host.ui.state.items.len() - 1;
        move_item(&mut host, 0, last);
        let moved = shown(&host);
        for row in &moved {
            if let Some(was) = after.iter().find(|was| was.0 == row.0) {
                assert_eq!(row.1, was.1 - lifted, "item {} moves up by the row that left", row.0);
            }
        }
        covers_viewport(&moved);
        for row in &moved {
            assert_eq!(row.2, host.ui.state.items.iter().find(|i| i.id == row.0).unwrap().height);
        }
        // MeasureVisible: a row that enters is measured, the average of the estimated rows above
        // follows, and the scroll takes that shift so the rows on screen stay where they are.
        let exact = strategy != MeasuringStrategy::MeasureVisible;
        if exact {
            assert_eq!(scrolled(&host, viewport), 500.0);
            assert_eq!(layout(&host, list).measured_items(), measured);
        }
        assert!(host.ui.state.counters.binds.get() <= binds + 1);

        // Reversed: the same items in the other order, sizes with them, nothing measured again.
        let (binds, scrolled_before, measured) = (host.ui.state.counters.binds.get(), scrolled(&host, viewport), layout(&host, list).measured_items());
        host.ui.state.items.reverse();
        host.ui.tree.cx().items_reordered(list, (0..=last).rev().collect());
        host.ui.state_changed();
        host.frame();
        let reversed = shown(&host);
        covers_viewport(&reversed);
        for row in &reversed {
            assert_eq!(row.2, host.ui.state.items.iter().find(|i| i.id == row.0).unwrap().height);
        }
        for pair in reversed.windows(2) {
            assert_eq!(pair[1].1, pair[0].1 + pair[0].2 + 2.0);
        }
        if exact {
            assert_eq!(scrolled(&host, viewport), scrolled_before);
            assert_eq!(layout(&host, list).measured_items(), measured);
        } else {
            // Rows that were never measured come on screen here, and only they are measured.
            assert!(layout(&host, list).measured_items() <= measured + reversed.len());
        }
        // Every row on screen is new here: bound, but out of the cells that exist.
        assert!(host.ui.state.counters.binds.get() - binds <= reversed.len() as u32);
    }
}

#[test]
fn the_item_under_a_pixel_and_the_cell_of_an_item() {
    let mut host = rows_host(MeasuringStrategy::MeasureFirst, true);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 42.0 * 10.0);
    host.settle();
    let list_ref = layout(&host, list);
    assert_eq!(list_ref.item_at_pixels(0.0), Some(0));
    assert_eq!(list_ref.item_at_pixels(42.0 * 10.0 + 5.0), Some(10));
    // The gap below a row belongs to it; before the first row and after the last one is no row.
    assert_eq!(list_ref.item_at_pixels(42.0 * 10.0 + 41.0), Some(10));
    assert_eq!(list_ref.item_at_pixels(-1.0), None);
    assert_eq!(list_ref.item_at_pixels(200.0 * 42.0), None);
    // The cell of a row on screen is drawn at its row; rows off screen have none.
    let cell = list_ref.cell_in_use(12).expect("row 12 is on screen");
    assert_eq!(host.rect(cell).top - host.rect(viewport).top, 12.0 * 42.0);
    assert_eq!(list_ref.cell_in_use(0), None);
    assert_eq!(list_ref.cell_in_use(199), None);
    let in_use: Vec<usize> = list_ref.cells_in_use().map(|(index, _)| index).collect();
    assert_eq!(in_use, (10..20).collect::<Vec<_>>());
    assert_eq!(list_ref.debug_string(), "items 200 visible 10-19 inuse 10 pool 0 created 10");

    let host = rows_host(MeasuringStrategy::MeasureVisible, false);
    let line = layout(&host, host.ui.state.list).debug_string();
    assert!(line.starts_with("items 200 visible 0-") && line.contains(" measured ") && line.contains(" inuse "), "{line}");
}

/// DrawnUi.React ReorderPage: the lifted row draws blank (its bind reads what is dragged, and
/// `items_changed` binds that one row again), it moves step by step with its cell and its size,
/// the floating copy starts at `item_rect` and glides back into it, and the row draws again on
/// drop, also when its cell waited in the pool meanwhile.
#[test]
fn a_lifted_row_draws_blank_travels_with_its_item_and_comes_back_on_drop() {
    for recycling in [RecyclingTemplate::Enabled, RecyclingTemplate::Disabled] {
        lift_and_drop(recycling);
    }
}

fn lift_and_drop(recycling: RecyclingTemplate) {
    let mut host = rows_host(MeasuringStrategy::MeasureFirst, true);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    host.ui.tree.find_mut::<SkiaLayout>(list).unwrap().set_recycling_template(recycling);
    scroll_to(&mut host, viewport, 100.0);
    host.settle();
    // Rows are 40 + 2: row 3 is at 126 in the list, 26 on screen.
    assert_eq!(host.ui.tree.cx().item_rect(list, 3), Some(Rect::from_xywh(0.0, 26.0, 200.0, 40.0)));
    assert_eq!(host.ui.tree.cx().item_rect(list, 30), None, "no cell off screen");
    let blank = |host: &mut Headless<Rows>, index: usize| host.pixel(100, (index * 42) as i32 - 100 + 20) == Color::BLACK;

    // Lift: one bind, nothing measured.
    let (binds, measured) = (host.ui.state.counters.binds.get(), layout(&host, list).measured_items());
    host.ui.state.dragging = Some(host.ui.state.items[3].id);
    host.ui.tree.cx().items_changed(list, 3);
    host.ui.state_changed();
    host.frame();
    assert_eq!(host.ui.state.counters.binds.get(), binds + 1);
    assert_eq!(layout(&host, list).measured_items(), measured);
    assert!(blank(&mut host, 3) && !blank(&mut host, 2) && !blank(&mut host, 4));

    // Carried down five rows: the gap travels, no cell is bound again, the scroll stays.
    for step in 3..8 {
        move_item(&mut host, step, step + 1);
        assert!(blank(&mut host, step + 1) && !blank(&mut host, step), "step {step}");
    }
    assert_eq!(host.ui.state.counters.binds.get(), binds + 1);
    assert_eq!(scrolled(&host, viewport), 100.0);
    assert_eq!(host.ui.tree.cx().item_rect(list, 8), Some(Rect::from_xywh(0.0, 8.0 * 42.0 - 100.0, 200.0, 40.0)));

    // Dropped while its row is just off screen: its cell waits in the pool, still showing the
    // item, and forgets the bind, so the row does not come back blank without one.
    scroll_to(&mut host, viewport, 42.0 * 9.0);
    host.settle();
    assert_eq!(layout(&host, list).cell_in_use(8), None);
    host.ui.state.dragging = None;
    host.ui.tree.cx().items_changed(list, 8);
    host.ui.state_changed();
    host.frame();
    scroll_to(&mut host, viewport, 100.0);
    host.settle();
    for index in 2..12 {
        assert!(!blank(&mut host, index), "{recycling:?}: row {index} draws again");
    }
    assert_eq!(host.ui.state.items[8].id, 3);
}

/// SplitGridFirstItemTests.FilledBeforeAttach_ThenStructureRebuild_OneCellPerItem, the cases
/// without LayoutIsReady (`[InlineData(1 | 2 | 5, false)]`): the page is in the items before
/// the grid is built with a bootstrap tile height, then the tile height follows the width of the
/// scroll and the items are applied again. One cell per item, one tile painted per item.
#[test]
fn filled_before_attach_then_structure_rebuild_one_cell_per_item() {
    #[derive(Default)]
    struct Gallery {
        shots: usize,
        cell_height: f32,
        grid: Handle<SkiaLayout>,
        scroll: Handle<SkiaScroll>,
    }
    for count in [1usize, 2, 5] {
        let build = |app: &mut Gallery| {
            let grid = SkiaLayout::column()
                .split(3)
                .spacing(2)
                .recycling_template(RecyclingTemplate::Enabled)
                .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                .assign(&mut app.grid)
                .items(|app: &Gallery| app.shots, shape_cell, |shape: &Handle<SkiaShape>, app: &Gallery, _, cx| {
                    if let Some(mut shape) = cx.get_mut(*shape) {
                        shape.set_horizontal_options(LayoutOptions::Fill);
                        // The bootstrap value of the app until the width is known.
                        shape.set_height_request(if app.cell_height > 0.0 { app.cell_height } else { 160.0 });
                        shape.set_background_color(Color::from_rgb(0x48, 0x3D, 0x8B));
                        shape.set_use_cache(CacheType::Image);
                    }
                });
            SkiaScroll::new().fill().load_more_offset(600).assign(&mut app.scroll).content(grid)
        };
        let ui = Ui::new(Gallery { shots: count, ..Gallery::default() }, build).background(Color::BLACK);
        let mut host = Headless::new(ui, 402, 700, 1.0);
        host.settle();

        // The tile height from the scroll's width; the items applied again.
        let (grid, scroll) = (host.ui.state.grid, host.ui.state.scroll);
        let column = (host.rect(scroll).width() - 2.0 * 2.0) / 3.0;
        host.ui.state.cell_height = column * 4.0 / 3.0;
        host.ui.tree.cx().items_reset(grid);
        host.ui.state_changed();
        host.settle();

        let cell_height = host.ui.state.cell_height;
        assert_eq!(layout(&host, grid).cells_in_use().count(), count);
        let mut painted = 0usize;
        for y in 0..700 {
            for x in 0..402 {
                painted += (host.pixel(x, y) != Color::BLACK) as usize;
            }
        }
        let (painted, one_tile) = (painted as f32 / (402.0 * 700.0), column * cell_height / (402.0 * 700.0));
        let tiles = one_tile * count as f32;
        assert!(painted >= tiles * 0.9 && painted <= tiles * 1.1, "count {count}: painted {painted}, a tile {one_tile}");
        let rows = count.div_ceil(3) as f32;
        let height = content_height(&host, grid);
        assert!((height - (rows * cell_height + (rows - 1.0) * 2.0)).abs() <= 1.0, "count {count}: content {height}");
    }
}

/// A measurement, not a test: what one step of a drag costs (a move inside the viewport and the
/// frame after it). `cargo test --release -p drawnui --test list_templated -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn drag_step_cost() {
    const STEPS: usize = 400;
    let cases = [(MeasuringStrategy::MeasureFirst, true, "MeasureFirst, uniform"), (MeasuringStrategy::MeasureVisible, false, "MeasureVisible, uneven")];
    for (strategy, uniform, label) in cases {
        let mut host = rows_host(strategy, uniform);
        let viewport = host.ui.state.viewport;
        scroll_to(&mut host, viewport, 500.0);
        host.settle();
        let first = rows(&host, viewport, host.ui.state.list)[1].0;
        let started = std::time::Instant::now();
        for step in 0..STEPS {
            let (from, to) = if step % 2 == 0 { (first, first + 1) } else { (first + 1, first) };
            move_item(&mut host, from, to);
        }
        let micros = started.elapsed().as_secs_f64() * 1e6 / STEPS as f64;
        println!("{label}, 200 rows: {micros:.0} us per drag step (move + frame)");
    }
}
