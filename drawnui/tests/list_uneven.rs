//! A list of uneven rows (MeasureVisible): what is on screen is measured and exact, the rest is
//! estimated and measured inside a budget per frame, and rows on screen stay where they are when
//! a row above them turns out to have another size than estimated.

mod list_common;

use drawnui::controls::layout::{MeasureBudget, MeasuringStrategy};
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::*;

const ROWS: usize = 2_000;
const GAP: f32 = 2.0;
const VIEW: f32 = 400.0;

/// 30 to 79 pixels, in no order.
fn height(item: usize) -> f32 {
    30.0 + (item * 37 % 50) as f32
}

/// Top of a row when every row above it has its real size.
fn top(item: usize) -> f32 {
    (0..item).map(|i| height(i) + GAP).sum()
}

#[derive(Default)]
struct App {
    rows: usize,
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
    counters: Counters,
}

fn build(app: &mut App, budget: MeasureBudget, loose: bool) -> Build<SkiaLayout> {
    let binds = app.counters.binds.clone();
    SkiaLayout::new().fill().children((Viewport::new(loose)
        .width_request(200)
        .height_request(VIEW)
        .is_clipped_to_bounds(true)
        .assign(&mut app.viewport)
        .children((SkiaLayout::column()
            .spacing(GAP)
            .measure_items_strategy(MeasuringStrategy::MeasureVisible)
            .measure_budget(budget)
            .assign(&mut app.list)
            .items(
                |app: &App| app.rows,
                || {
                    let mut row = Handle::default();
                    (SkiaShape::new().fill_x().assign(&mut row), row)
                },
                move |row: &Handle<SkiaShape>, _app: &App, index, cx| {
                    count(&binds);
                    if let Some(mut row) = cx.get_mut(*row) {
                        row.set_height_request(height(index));
                        row.set_background_color(color(index));
                    }
                },
            ),)),))
}

fn host(budget: MeasureBudget, loose: bool) -> Headless<App> {
    let ui = Ui::new(App { rows: ROWS, ..App::default() }, move |app| build(app, budget, loose));
    Headless::new(ui.background(Color::BLACK), 300, 500, 1.0)
}

/// Every row on screen has its real height and follows the one above it.
fn assert_rows_are_exact(rows: &[(usize, f32, f32)]) {
    for pair in rows.windows(2) {
        assert_eq!(pair[1].0, pair[0].0 + 1);
        assert_eq!(pair[1].1, pair[0].1 + pair[0].2 + GAP, "row {} follows row {}", pair[1].0, pair[0].0);
    }
    for row in rows {
        assert_eq!(row.2, height(row.0), "height of row {}", row.0);
    }
}

#[test]
fn visible_rows_are_exact_and_the_rest_is_estimated() {
    let mut host = host(MeasureBudget::Items(0), false);
    host.settle();
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    let rows = rows(&host, viewport, list);
    assert_rows_are_exact(&rows);
    assert_eq!(rows[0], (0, 0.0, height(0)));
    // They fill the viewport, and only they were bound and measured.
    let last = rows.last().unwrap();
    assert!(last.1 <= VIEW && last.1 + last.2 + GAP > VIEW);
    let list = layout(&host, list);
    assert_eq!(list.measured_items(), rows.len());
    assert_eq!(host.ui.state.counters.binds.get(), rows.len() as u32);
    assert!(list.is_item_measured(rows.len() - 1) && !list.is_item_measured(rows.len()));

    // The rest counts as the average of the measured rows, in whole pixels.
    let measured: f32 = rows.iter().map(|row| row.2).sum();
    let average = (measured / rows.len() as f32).round();
    let estimated = measured + (ROWS - rows.len()) as f32 * average + (ROWS - 1) as f32 * GAP;
    assert_eq!(content_height(&host, host.ui.state.list), estimated);
    assert_eq!(list.item_height_pixels(1_000), average);
}

#[test]
fn the_budget_bounds_every_frame_and_the_content_size_converges() {
    let mut host = host(MeasureBudget::Items(25), false);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    host.frame();
    let on_screen = rows(&host, viewport, list);
    assert_rows_are_exact(&on_screen);
    // The first frame measures what it shows, plus the budget.
    let mut measured = layout(&host, list).measured_items();
    assert_eq!(measured, on_screen.len() + 25);

    let mut frames = 0;
    while layout(&host, list).measured_items() < ROWS {
        assert!(host.ui.needs_frame(), "the list asks for frames while rows are left to measure");
        let binds = host.ui.state.counters.binds.get();
        host.frame();
        frames += 1;
        // Never more than the budget: 25 rows bound and measured, none of them on screen.
        let now = layout(&host, list).measured_items();
        assert_eq!(now - measured, 25.min(ROWS - measured));
        assert_eq!(host.ui.state.counters.binds.get() - binds, (now - measured) as u32);
        measured = now;
        // The rows on screen do not move while the rows below them get their sizes.
        assert_eq!(rows(&host, viewport, list), on_screen);
        // One spare cell does the measuring.
        assert_eq!(cells(&host, list), on_screen.len() + 1);
    }
    assert_eq!(frames, (ROWS - on_screen.len() - 25).div_ceil(25));
    host.settle();
    assert!(!host.ui.needs_frame());

    // Every estimate became a real size.
    assert_eq!(content_height(&host, list), top(ROWS) - GAP);
    assert_eq!(host.rect(list).height(), top(ROWS) - GAP);
    assert_eq!(layout(&host, list).item_offset_pixels(1_234), top(1_234));
}

/// Jumps to the middle of the list, where nothing was measured, then scrolls back up 15 pixels
/// per frame over rows that were only estimated. Returns how many frames moved a row on screen
/// by anything else than those 15 pixels, and how many frames the list reported a shift in.
fn scroll_back_up(loose: bool) -> (u32, u32) {
    let mut host = host(MeasureBudget::Items(0), loose);
    host.settle();
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    let middle = layout(&host, list).item_offset_pixels(1_000);
    scroll_to(&mut host, viewport, middle);
    host.settle();
    assert_eq!(rows(&host, viewport, list)[0].0, 1_000);

    let (mut jumps, mut shifts) = (0, 0);
    for _ in 0..400 {
        let before = rows(&host, viewport, list);
        let target = scrolled(&host, viewport) - 15.0;
        scroll_to(&mut host, viewport, target);
        host.frame();
        let after = rows(&host, viewport, list);
        assert_rows_are_exact(&after);
        shifts += (scrolled(&host, viewport) != target) as u32;
        let moved = |row: &(usize, f32, f32)| after.iter().find(|a| a.0 == row.0).map(|a| a.1 - row.1);
        jumps += before.iter().filter_map(moved).any(|by| (by - 15.0).abs() > 0.01) as u32;
    }
    // 6,000 pixels up: far above row 1,000 now.
    assert!(rows(&host, viewport, list)[0].0 < 920);
    (jumps, shifts)
}

#[test]
fn scrolling_back_up_keeps_the_rows_on_screen_in_place() {
    // The scroll takes the shift the list reports: no row on screen ever jumps.
    let (jumps, shifts) = scroll_back_up(false);
    assert_eq!(jumps, 0);
    assert!(shifts > 50, "rows above the viewport had other sizes than estimated in {shifts} frames");

    // A scroll that ignores the report shows why it exists: the rows jump.
    let (jumps, _) = scroll_back_up(true);
    assert!(jumps > 50, "{jumps} frames with a jump");
}

#[test]
fn a_jump_to_the_end_lands_on_the_last_rows() {
    let mut host = host(MeasureBudget::Items(0), false);
    host.settle();
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    // As a scroll does it: to the end of the content as it is known, again when it changed.
    for _ in 0..5 {
        let end = content_height(&host, list) - VIEW;
        scroll_to(&mut host, viewport, end);
        host.settle();
    }
    let rows = rows(&host, viewport, list);
    assert_rows_are_exact(&rows);
    let last = rows.last().unwrap();
    assert_eq!((last.0, last.1 + last.2), (ROWS - 1, VIEW));
    // Only the rows that were on screen on the way were measured.
    assert!(layout(&host, list).measured_items() < 60);
    assert_eq!(host.pixel(10, 395), color(ROWS - 1));
}

#[test]
fn the_time_budget_measures_until_the_time_is_up_and_at_least_one_row() {
    // No time at all: the one row every frame gets.
    let mut slow = host(MeasureBudget::Millis(0.0), false);
    let list = slow.ui.state.list;
    slow.frame();
    let mut measured = layout(&slow, list).measured_items();
    for _ in 0..20 {
        slow.frame();
        assert_eq!(layout(&slow, list).measured_items(), measured + 1);
        measured += 1;
    }

    // The default, 4 ms a frame: frames go on until every row has its size.
    let mut usual = host(MeasureBudget::Millis(4.0), false);
    let list = usual.ui.state.list;
    let mut frames = 0;
    usual.frame();
    while usual.ui.needs_frame() {
        usual.frame();
        frames += 1;
        assert!(frames < ROWS, "more frames than rows");
    }
    assert_eq!(layout(&usual, list).measured_items(), ROWS);
    assert_eq!(content_height(&usual, list), top(ROWS) - GAP);
}
