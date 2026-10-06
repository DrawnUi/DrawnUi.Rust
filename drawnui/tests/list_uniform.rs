//! A list of 10,000 uniform rows (MeasureFirst): cells exist for the visible rows only, scrolling
//! binds what enters and measures nothing, and a frame costs the same for 200 items as for 10,000.

mod list_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::controls::layout::MeasuringStrategy;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::*;

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(|a| a.get())
}

const ROW: f32 = 40.0;
const GAP: f32 = 2.0;
const STRIDE: f32 = ROW + GAP;
const VIEW: f32 = 400.0;

#[derive(Default)]
struct App {
    rows: usize,
    tapped: Vec<usize>,
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
    counters: Counters,
}

#[derive(Default)]
struct RowHandles {
    row: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    let counters = app.counters.clone();
    let (created, binds) = (counters.created.clone(), counters.binds.clone());
    SkiaLayout::new().fill().children((Viewport::new(false)
        .width_request(200)
        .height_request(VIEW)
        .is_clipped_to_bounds(true)
        .assign(&mut app.viewport)
        .children((SkiaLayout::column()
            .spacing(GAP)
            .measure_items_strategy(MeasuringStrategy::MeasureFirst)
            .assign(&mut app.list)
            .items(
                |app: &App| app.rows,
                move || {
                    count(&created);
                    let mut handles = RowHandles::default();
                    let cell = SkiaShape::new()
                        .fill_x()
                        .height_request(ROW)
                        .use_cache(CacheType::Image)
                        .assign(&mut handles.row)
                        .on_tapped(|me, app: &mut App, _cx| app.tapped.push(me.base().context_index.unwrap()))
                        .children((counters.probe(),));
                    (cell, handles)
                },
                move |handles: &RowHandles, _app: &App, index, cx| {
                    count(&binds);
                    if let Some(mut row) = cx.get_mut(handles.row) {
                        row.set_background_color(color(index));
                    }
                },
            ),)),))
}

fn host(rows: usize) -> Headless<App> {
    let ui = Ui::new(App { rows, ..App::default() }, build).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 500, 1.0);
    host.settle();
    host
}

fn total(rows: usize) -> f32 {
    rows as f32 * ROW + (rows - 1) as f32 * GAP
}

#[test]
fn ten_thousand_rows_need_cells_for_the_visible_ones_only() {
    let mut host = host(10_000);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    // Pure arithmetic from the first cell: 10,000 x 40 + 9,999 x 2.
    assert_eq!(content_height(&host, list), 419_998.0);
    assert_eq!(layout(&host, list).visible_items(), Some((0, 9)));
    assert_eq!(cells(&host, list), 10);
    // One bind per row shown; the cell that measured row 0 shows it.
    assert_eq!(host.ui.state.counters.read(), (10, 10, 10, 10));

    // To the end in jumps of 5,000 pixels: never more cells than rows on screen.
    let end = total(10_000) - VIEW;
    let mut y = 0.0;
    while y < end {
        y = (y + 5_000.0).min(end);
        scroll_to(&mut host, viewport, y);
        host.frame();
        assert!(cells(&host, list) <= 11, "{} cells at {y}", cells(&host, list));
    }
    assert_eq!(layout(&host, list).visible_items(), Some((9_990, 9_999)));
    assert_eq!(host.ui.state.counters.created.get(), 11);
    // The last row ends the viewport, and shows the last item.
    assert_eq!(rows(&host, viewport, list).last(), Some(&(9_999, VIEW - ROW, ROW)));
    assert_eq!(host.pixel(10, 380), color(9_999));
    assert_eq!(host.pixel(10, 5), color(9_990));
    assert_eq!(layout(&host, list).item_offset_pixels(9_999), 9_999.0 * STRIDE);
}

#[test]
fn scrolling_binds_what_enters_and_measures_nothing() {
    let mut host = host(10_000);
    let viewport = host.ui.state.viewport;
    // Half a row down: 11 rows show, the most this viewport ever needs.
    scroll_to(&mut host, viewport, 21.0);
    host.settle();
    let counters = host.ui.state.counters.clone();
    assert_eq!(counters.read(), (11, 11, 11, 11));

    // A row per frame: one cell leaves and gets the row that enters. One bind, one cell placed,
    // no cell created, nothing measured.
    for frame in 1..=200 {
        scroll_to(&mut host, viewport, 21.0 + frame as f32 * STRIDE);
        host.frame();
        assert_eq!(counters.read(), (11, 11 + frame, 11, 11 + frame));
    }
    // A frame in which no row enters touches no cell at all.
    let before = counters.read();
    scroll_to(&mut host, viewport, 21.0 + 200.0 * STRIDE + 10.0);
    host.frame();
    assert_eq!(counters.read(), before);
    // The top row leaves, then comes back to the cell it just left: that cell still shows it,
    // so it is neither bound nor placed again.
    scroll_to(&mut host, viewport, 21.0 + 200.0 * STRIDE + 30.0);
    host.frame();
    scroll_to(&mut host, viewport, 21.0 + 200.0 * STRIDE + 10.0);
    host.frame();
    assert_eq!(counters.read(), before);
}

/// Scrolls 300 frames through the list, a third of a row at a time, and returns what it cost:
/// (cells created, binds, measures, arranges, allocations).
fn scroll_cost(rows: usize) -> (u32, u32, u32, u32, usize) {
    let mut host = host(rows);
    let viewport = host.ui.state.viewport;
    scroll_to(&mut host, viewport, 21.0);
    host.settle();
    let counters = host.ui.state.counters.clone();
    let (before, allocations_before, started) = (counters.read(), allocations(), std::time::Instant::now());
    for frame in 1..=300 {
        scroll_to(&mut host, viewport, 21.0 + frame as f32 * STRIDE / 3.0);
        host.frame();
    }
    let micros = started.elapsed().as_secs_f64() * 1e6 / 300.0;
    let (after, allocated) = (counters.read(), allocations() - allocations_before);
    println!("{rows} rows: {micros:.1} us per scrolled frame, paint included");
    (after.0 - before.0, after.1 - before.1, after.2 - before.2, after.3 - before.3, allocated)
}

#[test]
fn a_scrolled_frame_costs_the_same_for_200_items_and_for_10_000() {
    let small = scroll_cost(200);
    let large = scroll_cost(10_000);
    println!("300 scrolled frames, 100 rows entering: (created, binds, measures, arranges, allocations) = {large:?}");
    assert_eq!(small, large);
    // 100 rows entered in 300 frames: 100 binds, 100 cells placed, nothing created or measured.
    assert_eq!((large.0, large.1, large.2, large.3), (0, 100, 0, 100));

    // The same 300 frames with no row entering (back and forth inside one row): what the engine
    // itself allocates per scrolled frame. The difference is what the entering rows cost.
    let mut host = host(10_000);
    let viewport = host.ui.state.viewport;
    scroll_to(&mut host, viewport, 21.0);
    host.settle();
    let before = allocations();
    for frame in 1..=300 {
        scroll_to(&mut host, viewport, 21.0 + (frame % 2) as f32 * 10.0);
        host.frame();
    }
    let idle = allocations() - before;
    println!("300 scrolled frames, no row entering: {idle} allocations");
    println!("allocations per entering row: {}", (large.4 - idle) as f32 / 100.0);
}

#[test]
fn a_tap_hits_the_row_drawn_there_after_recycling() {
    let mut host = host(10_000);
    let viewport = host.ui.state.viewport;
    scroll_to(&mut host, viewport, 100.0 * STRIDE);
    host.settle();
    // Row 103 is drawn 3 rows below the top of the viewport.
    host.tap(10.0, 3.0 * STRIDE + 5.0);
    assert_eq!(host.ui.state.tapped, [103]);
    scroll_to(&mut host, viewport, 5_000.0 * STRIDE);
    host.settle();
    host.tap(10.0, 9.0 * STRIDE + 20.0);
    assert_eq!(host.ui.state.tapped, [103, 5_009]);
    // The gap between two rows belongs to no row.
    host.tap(10.0, ROW + 1.0);
    assert_eq!(host.ui.state.tapped, [103, 5_009]);
}

#[test]
fn append_is_arithmetic_and_keeps_the_scroll_position() {
    let mut host = host(10_000);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 2_100.0);
    host.settle();
    let before = (host.ui.state.counters.read(), rows(&host, viewport, list));

    // No call to the list: a larger count is read as "appended".
    host.ui.state.rows += 50;
    host.ui.state_changed();
    host.settle();
    assert_eq!(content_height(&host, list), total(10_050));
    assert_eq!(layout(&host, list).items_count(), 10_050);
    assert_eq!(scrolled(&host, viewport), 2_100.0);
    // Nothing bound, measured or moved.
    assert_eq!((host.ui.state.counters.read(), rows(&host, viewport, list)), before);
}

#[test]
fn the_pool_follows_the_visible_rows() {
    let mut host = host(10_000);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    assert_eq!(cells(&host, list), 10);

    // The viewport shrinks to 2 rows: 8 cells are spare. DrawnUI keeps four times the visible
    // rows, 8 cells here, all of them.
    host.ui.tree.any_mut(viewport).unwrap().set_height_request(60);
    host.settle();
    assert_eq!(rows(&host, viewport, list).len(), 2);
    assert_eq!(cells(&host, list), 8);
    // One row: 4 cells stay.
    host.ui.tree.any_mut(viewport).unwrap().set_height_request(30);
    host.settle();
    assert_eq!((rows(&host, viewport, list).len(), cells(&host, list)), (1, 4));

    // An explicit pool size caps the cells, but a row on screen always has one.
    host.ui.tree.find_mut::<SkiaLayout>(list).unwrap().set_item_template_pool_size(3);
    host.ui.tree.any_mut(viewport).unwrap().set_height_request(VIEW);
    host.settle();
    assert_eq!((rows(&host, viewport, list).len(), cells(&host, list)), (10, 10));
    host.ui.tree.any_mut(viewport).unwrap().set_height_request(30);
    host.settle();
    assert_eq!((rows(&host, viewport, list).len(), cells(&host, list)), (1, 3));
}
