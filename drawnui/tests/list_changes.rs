//! Item changes of a list (insert, remove, change, reset), the two cell modes and MeasureAll.
//! Rows that stay on screen keep their cell and their place: nothing is bound that was not changed.

mod list_common;

use drawnui::controls::layout::{MeasureBudget, MeasuringStrategy, RecyclingTemplate};
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::*;

const GAP: f32 = 2.0;
const VIEW: f32 = 400.0;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Item {
    id: usize,
    height: f32,
}

/// An item with a height of 30 to 79 pixels, by its id.
fn item(id: usize) -> Item {
    Item { id, height: 30.0 + (id * 37 % 50) as f32 }
}

fn extent(items: &[Item]) -> f32 {
    items.iter().map(|item| item.height + GAP).sum()
}

#[derive(Default)]
struct App {
    items: Vec<Item>,
    tapped: Vec<usize>,
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
    counters: Counters,
}

fn build(app: &mut App, strategy: MeasuringStrategy, recycling: RecyclingTemplate) -> Build<SkiaLayout> {
    let counters = app.counters.clone();
    let (created, binds) = (counters.created.clone(), counters.binds.clone());
    SkiaLayout::new().fill().children((Viewport::new(false)
        .width_request(200)
        .height_request(VIEW)
        .is_clipped_to_bounds(true)
        .assign(&mut app.viewport)
        .children((SkiaLayout::column()
            .spacing(GAP)
            .measure_items_strategy(strategy)
            .recycling_template(recycling)
            .measure_budget(MeasureBudget::Items(0))
            .assign(&mut app.list)
            .items(
                |app: &App| app.items.len(),
                move || {
                    count(&created);
                    let mut row = Handle::default();
                    let cell = SkiaShape::new()
                        .fill_x()
                        .use_cache(CacheType::Image)
                        .assign(&mut row)
                        .on_tapped(|me, app: &mut App, _cx| {
                            let index = me.base().context_index.unwrap();
                            app.tapped.push(app.items[index].id)
                        })
                        .children((counters.probe(),));
                    (cell, row)
                },
                move |row: &Handle<SkiaShape>, app: &App, index, cx| {
                    count(&binds);
                    if let Some(mut row) = cx.get_mut(*row) {
                        row.set_height_request(app.items[index].height);
                        row.set_background_color(color(app.items[index].id));
                    }
                },
            ),)),))
}

fn host(items: Vec<Item>, strategy: MeasuringStrategy, recycling: RecyclingTemplate) -> Headless<App> {
    let ui = Ui::new(App { items, ..App::default() }, move |app| build(app, strategy, recycling));
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    host
}

fn uneven(count: usize, strategy: MeasuringStrategy) -> Headless<App> {
    host((0..count).map(item).collect(), strategy, RecyclingTemplate::Enabled)
}

/// The rows on screen: (item id, top, height).
fn shown(host: &Headless<App>) -> Vec<(usize, f32, f32)> {
    let app = &host.ui.state;
    let rows = rows(host, app.viewport, app.list);
    rows.into_iter().map(|(index, top, height)| (app.items[index].id, top, height)).collect()
}

/// Every row on screen shows the pixels of its own item.
fn assert_pixels(host: &mut Headless<App>) {
    for (id, top, height) in shown(host) {
        let middle = top + height / 2.0;
        if (0.0..VIEW).contains(&middle) {
            assert_eq!(host.pixel(10, middle as i32), color(id), "pixels of item {id} at {middle}");
        }
    }
}

#[test]
fn insert_at_start_keeps_the_rows_on_screen_in_place() {
    let mut host = uneven(100, MeasuringStrategy::MeasureAll);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 1_000.0);
    host.settle();
    let (before, binds, height) = (shown(&host), host.ui.state.counters.binds.get(), content_height(&host, list));

    let head = [item(1_000), item(1_001), item(1_002)];
    host.ui.state.items.splice(0..0, head);
    host.ui.tree.cx().items_inserted(list, 0, 3);
    host.ui.state_changed();
    // The very next frame: the same items at the same places, the scroll moved by what was
    // inserted above them.
    host.frame();
    assert_eq!(shown(&host), before);
    assert_eq!(scrolled(&host, viewport), 1_000.0 + extent(&head));
    assert_eq!(content_height(&host, list), height + extent(&head));
    // MeasureAll measured the three new rows with a spare cell; no cell on screen was bound.
    assert_eq!(host.ui.state.counters.binds.get() - binds, 3);
    assert_pixels(&mut host);

    // At the very top the list stays on its first row too: the new rows are above it.
    scroll_to(&mut host, viewport, 0.0);
    host.settle();
    assert_eq!(shown(&host)[0], (1_000, 0.0, head[0].height));
    host.ui.state.items.insert(0, item(2_000));
    host.ui.tree.cx().items_inserted(list, 0, 1);
    host.ui.state_changed();
    host.frame();
    assert_eq!(scrolled(&host, viewport), item(2_000).height + GAP);
    assert_eq!(shown(&host)[0], (1_000, 0.0, head[0].height));
}

#[test]
fn insert_at_start_of_uniform_rows_is_arithmetic() {
    let items = (0..10_000).map(|id| Item { id, height: 40.0 }).collect();
    let mut host = host(items, MeasuringStrategy::MeasureFirst, RecyclingTemplate::Enabled);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 2_100.0);
    host.settle();
    let (before, counters) = (shown(&host), host.ui.state.counters.read());

    host.ui.state.items.splice(0..0, (20_000..20_005).map(|id| Item { id, height: 40.0 }));
    host.ui.tree.cx().items_inserted(list, 0, 5);
    host.ui.state_changed();
    host.frame();
    assert_eq!(shown(&host), before);
    assert_eq!(scrolled(&host, viewport), 2_100.0 + 5.0 * 42.0);
    // Nothing created, bound or measured: the cells only moved down with their rows.
    let now = host.ui.state.counters.read();
    assert_eq!((now.0, now.1, now.2), (counters.0, counters.1, counters.2));
    assert_eq!(layout(&host, list).visible_items(), Some((55, 64)));
}

#[test]
fn insert_at_start_with_estimated_rows_is_corrected_when_they_are_seen() {
    let mut host = uneven(200, MeasuringStrategy::MeasureVisible);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 500.0);
    host.settle();
    let (before, binds, at) = (shown(&host), host.ui.state.counters.binds.get(), scrolled(&host, viewport));

    let head = [item(1_000), item(1_001), item(1_002)];
    host.ui.state.items.splice(0..0, head);
    host.ui.tree.cx().items_inserted(list, 0, 3);
    host.ui.state_changed();
    host.frame();
    // A list that measures nothing ahead (a budget of 0 rows) measures none of them in the frame
    // they come in: they count as three average rows.
    assert_eq!(shown(&host), before);
    let average = layout(&host, list).item_height_pixels(0);
    assert_eq!(scrolled(&host, viewport), at + 3.0 * (average + GAP));
    assert_eq!(host.ui.state.counters.binds.get(), binds);
    assert!(!layout(&host, list).is_item_measured(0));

    // Back to the top: the new rows are measured as they show, exact and in order.
    for _ in 0..3 {
        scroll_to(&mut host, viewport, 0.0);
        host.settle();
    }
    let top = shown(&host);
    for (at, row) in top[..3].iter().enumerate() {
        assert_eq!(*row, (1_000 + at, extent(&head[..at]), head[at].height));
    }
    assert_pixels(&mut host);
}

#[test]
fn append_keeps_every_measured_size_and_the_scroll_position() {
    let mut host = uneven(200, MeasuringStrategy::MeasureVisible);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 500.0);
    host.settle();
    let at = scrolled(&host, viewport);
    let before = (shown(&host), host.ui.state.counters.read(), layout(&host, list).measured_items());
    let (height, average) = (content_height(&host, list), layout(&host, list).item_height_pixels(199));

    host.ui.state.items.extend((200..210).map(item));
    host.ui.state_changed();
    host.settle();
    assert_eq!((shown(&host), host.ui.state.counters.read(), layout(&host, list).measured_items()), before);
    assert_eq!(scrolled(&host, viewport), at);
    assert_eq!(content_height(&host, list), height + 10.0 * (average + GAP));
}

#[test]
fn remove_above_and_inside_the_viewport() {
    let mut host = uneven(100, MeasuringStrategy::MeasureAll);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 1_000.0);
    host.settle();
    let (before, counters) = (shown(&host), host.ui.state.counters.read());

    // Above the viewport: the rows on screen stay, the scroll gives back what was removed.
    let removed: Vec<Item> = host.ui.state.items.drain(3..5).collect();
    host.ui.tree.cx().items_removed(list, 3, 2);
    host.ui.state_changed();
    host.frame();
    assert_eq!(shown(&host), before);
    assert_eq!(scrolled(&host, viewport), 1_000.0 - extent(&removed));
    let now = host.ui.state.counters.read();
    assert_eq!((now.0, now.1, now.2), (counters.0, counters.1, counters.2));

    // The third row on screen: the rows above it stay, the rows below move up by its extent,
    // and the row that enters at the bottom is the only one bound.
    let third = before[2];
    let index = host.ui.state.items.iter().position(|item| item.id == third.0).unwrap();
    host.ui.state.items.remove(index);
    host.ui.tree.cx().items_removed(list, index, 1);
    host.ui.state_changed();
    host.frame();
    let after = shown(&host);
    assert_eq!(&after[..2], &before[..2]);
    assert!(after.iter().all(|row| row.0 != third.0));
    for (row, was) in after[2..].iter().zip(&before[3..]) {
        assert_eq!(*row, (was.0, was.1 - third.2 - GAP, was.2));
    }
    assert_eq!(host.ui.state.counters.binds.get() - counters.1, (after.len() + 1 - before.len()) as u32);
    assert_pixels(&mut host);
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);
}

#[test]
fn a_changed_item_is_bound_and_measured_alone() {
    let mut host = uneven(100, MeasuringStrategy::MeasureAll);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    scroll_to(&mut host, viewport, 1_000.0);
    host.settle();
    let (before, counters) = (shown(&host), host.ui.state.counters.read());

    // The second row on screen grows to 120 pixels.
    let second = before[1];
    let index = host.ui.state.items.iter().position(|item| item.id == second.0).unwrap();
    host.ui.state.items[index].height = 120.0;
    host.ui.tree.cx().items_changed(list, index);
    host.ui.state_changed();
    host.frame();
    let after = shown(&host);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], (second.0, second.1, 120.0));
    for (row, was) in after[2..].iter().zip(&before[2..]) {
        assert_eq!(*row, (was.0, was.1 + 120.0 - second.2, was.2));
    }
    // One bind, one cell measured.
    let now = host.ui.state.counters.read();
    assert_eq!((now.0, now.1 - counters.1, now.2 - counters.2), (counters.0, 1, 1));
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);

    // An item above the viewport shrinks by 20: measured with a spare cell, the rows on screen stay.
    host.ui.state.items[2].height -= 20.0;
    host.ui.tree.cx().items_changed(list, 2);
    host.ui.state_changed();
    host.frame();
    assert_eq!(shown(&host), after);
    assert_eq!(scrolled(&host, viewport), 980.0);
    assert_eq!(host.ui.state.counters.binds.get() - now.1, 1);
}

#[test]
fn reset_rebuilds_everything_and_a_smaller_count_is_a_reset() {
    let mut host = uneven(100, MeasuringStrategy::MeasureAll);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    let (created, binds) = (host.ui.state.counters.created.get(), host.ui.state.counters.binds.get());

    host.ui.state.items = (5_000..5_020).map(item).collect();
    host.ui.tree.cx().items_reset(list);
    host.ui.state_changed();
    host.settle();
    let after = shown(&host);
    assert_eq!(after[0], (5_000, 0.0, item(5_000).height));
    assert_eq!(after[1], (5_001, item(5_000).height + GAP, item(5_001).height));
    assert_eq!(layout(&host, list).items_count(), 20);
    assert_eq!(layout(&host, list).measured_items(), 20);
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);
    // Every item was bound again (20 measures, and the rows on screen), in the cells that exist.
    assert_eq!(host.ui.state.counters.created.get(), created);
    assert!(host.ui.state.counters.binds.get() - binds >= 20);
    assert_pixels(&mut host);

    // No call at all and fewer items: nothing can be kept.
    host.ui.state.items.drain(..12);
    host.ui.state_changed();
    host.settle();
    assert_eq!(layout(&host, list).items_count(), 8);
    assert_eq!(shown(&host)[0], (5_012, 0.0, item(5_012).height));
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);
    assert_eq!(scrolled(&host, viewport), 0.0);
    assert_pixels(&mut host);
}

#[test]
fn recycling_disabled_keeps_one_cell_per_item() {
    let mut host = host((0..40).map(item).collect(), MeasuringStrategy::MeasureAll, RecyclingTemplate::Disabled);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    // MeasureAll measured every item with its own cell.
    assert_eq!(host.ui.state.counters.read().0, 40);
    assert_eq!(host.ui.state.counters.read().1, 40);
    assert_eq!(cells(&host, list), 40);

    // To the end and back: no cell is made or bound again.
    let end = content_height(&host, list) - VIEW;
    for y in [end, end / 2.0, 0.0] {
        scroll_to(&mut host, viewport, y);
        host.settle();
        assert_pixels(&mut host);
    }
    assert_eq!((host.ui.state.counters.created.get(), host.ui.state.counters.binds.get()), (40, 40));

    // Only the cells on screen are drawn and can be tapped.
    let on_screen = shown(&host);
    assert!(on_screen.len() < 12 && on_screen[0].0 == 0);
    host.tap(10.0, on_screen[1].1 + 5.0);
    assert_eq!(host.ui.state.tapped, [1]);

    // A changed item: its own cell is bound again. A removed one: its cell waits for a new item.
    host.ui.state.items[30].height = 100.0;
    host.ui.tree.cx().items_changed(list, 30);
    host.ui.state.items.remove(0);
    host.ui.tree.cx().items_removed(list, 0, 1);
    host.ui.state.items.push(item(77));
    host.ui.state_changed();
    host.settle();
    assert_eq!(shown(&host)[0].0, 1);
    // Two binds: the changed item, and the appended one in the cell of the removed one.
    assert_eq!((host.ui.state.counters.created.get(), host.ui.state.counters.binds.get()), (40, 42));
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);
}

#[test]
fn measure_all_measures_every_item_once_up_front() {
    let host = uneven(60, MeasuringStrategy::MeasureAll);
    let list = host.ui.state.list;
    let on_screen = shown(&host);
    assert_eq!(layout(&host, list).measured_items(), 60);
    assert_eq!(content_height(&host, list), extent(&host.ui.state.items) - GAP);
    assert_eq!(layout(&host, list).item_offset_pixels(59), extent(&host.ui.state.items[..59]));
    // Cells for the rows on screen only; one of them measured the 60 items first.
    assert_eq!(cells(&host, list), on_screen.len());
    assert_eq!(host.ui.state.counters.binds.get(), 60 + on_screen.len() as u32);
}

#[test]
fn a_recycled_cell_never_shows_the_pixels_of_its_previous_item() {
    // Cells are image caches. Every frame is checked, not the settled state: a cell that enters
    // must have been recorded again before its first paint.
    let items = (0..500).map(|id| Item { id, height: 40.0 }).collect();
    let mut host = host(items, MeasuringStrategy::MeasureFirst, RecyclingTemplate::Enabled);
    let viewport = host.ui.state.viewport;
    for frame in 1..=60 {
        scroll_to(&mut host, viewport, frame as f32 * 42.0);
        host.frame();
        assert_pixels(&mut host);
    }
    let records = host.cache_records(host.ui.tree.children(host.ui.state.list)[0]);
    // A cell is recorded when it gets an item, not when it only moves.
    scroll_to(&mut host, viewport, 60.0 * 42.0 + 10.0);
    host.frame();
    assert_pixels(&mut host);
    assert_eq!(host.cache_records(host.ui.tree.children(host.ui.state.list)[0]), records);

    // Uneven rows, 25 pixels a frame, rows entering while their sizes are still unknown.
    let mut host = uneven(300, MeasuringStrategy::MeasureVisible);
    let viewport = host.ui.state.viewport;
    for frame in 1..=120 {
        scroll_to(&mut host, viewport, frame as f32 * 25.0);
        host.frame();
        assert_pixels(&mut host);
    }
    // And back up, a whole viewport at a time.
    while scrolled(&host, viewport) > 0.0 {
        let up = (scrolled(&host, viewport) - VIEW).max(0.0);
        scroll_to(&mut host, viewport, up);
        host.frame();
        assert_pixels(&mut host);
    }
}

#[derive(Default)]
struct Buttons {
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
}

#[test]
fn a_ripple_does_not_travel_with_a_recycled_cell() {
    const CRIMSON: Color = Color::from_argb(255, 0xDC, 0x14, 0x3C);
    // 1,000 rows of 50: each cell is a button that plays a ripple when pressed.
    let build = |app: &mut Buttons| {
        let list = SkiaLayout::column()
            .spacing(0)
            .measure_items_strategy(MeasuringStrategy::MeasureFirst)
            .assign(&mut app.list)
            .items(
                |_: &Buttons| 1_000,
                || (SkiaButton::new("").fill_x().height_request(50), ()),
                |_: &(), _: &Buttons, _, _| {},
            );
        let viewport = Viewport::new(false).width_request(200).height_request(VIEW).assign(&mut app.viewport);
        SkiaLayout::new().fill().children((viewport.children((list,)),))
    };
    let mut host = Headless::new(Ui::new(Buttons::default(), build).background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    assert_eq!(host.pixel(100, 25), CRIMSON);

    // A press on row 0: 200 of the ripple's 500 ms later it covers the press point.
    host.ui.pointer(drawnui::PointerKind::Down, 100.0, 25.0, 0.0);
    host.frame();
    host.frame_after(200.0);
    assert_ne!(host.pixel(100, 25), CRIMSON);

    // Rows 0 and 1 leave, rows 8 and 9 enter in their cells: no ripple came along.
    scroll_to(&mut host, viewport, 100.0);
    host.frame_after(16.0);
    assert_eq!(rows(&host, viewport, list).iter().map(|row| row.0).collect::<Vec<_>>(), [2, 3, 4, 5, 6, 7, 8, 9]);
    for y in [25, 75, 325, 375] {
        assert_eq!(host.pixel(100, y), CRIMSON, "row at {y}");
    }
}

#[derive(Default)]
struct Themed {
    tint: Option<Color>,
    viewport: Handle<Viewport>,
}

#[test]
fn an_observer_inside_a_cell_runs_when_the_cell_is_made() {
    // Cells take their color from the app state with `observe`, not from the bind.
    let build = |app: &mut Themed| {
        let cell = || SkiaShape::new().fill_x().height_request(50).observe(|me, app: &Themed| me.set_background_color(app.tint));
        let list = SkiaLayout::column()
            .spacing(0)
            .measure_items_strategy(MeasuringStrategy::MeasureFirst)
            .items(|_: &Themed| 1_000, move || (cell(), ()), |_: &(), _: &Themed, _, _| {});
        let viewport = Viewport::new(false).width_request(200).height_request(VIEW).assign(&mut app.viewport);
        SkiaLayout::new().fill().children((viewport.children((list,)),))
    };
    let state = Themed { tint: Some(Color::GREEN), ..Themed::default() };
    let mut host = Headless::new(Ui::new(state, build).background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    let viewport = host.ui.state.viewport;
    assert_eq!(host.pixel(100, 375), Color::GREEN);

    // Half a row down: a ninth cell is made in a frame in which no observer runs by itself.
    scroll_to(&mut host, viewport, 25.0);
    host.frame();
    assert_eq!(host.pixel(100, 390), Color::GREEN);

    // And the cells follow the state like any control.
    host.ui.state.tint = Some(Color::BLUE);
    host.ui.state_changed();
    host.settle();
    assert_eq!((host.pixel(100, 10), host.pixel(100, 390)), (Color::BLUE, Color::BLUE));
}
