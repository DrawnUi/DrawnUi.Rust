//! The list inside the real SkiaScroll (the other list tests use a minimal scroll of their own):
//! jumps, flings, taps, load-more at both ends, ScrollToIndex, and the rows on screen staying
//! where they are while sizes above them change. LoadMoreDistanceTests of `DrawnUi.Net.Tests` is
//! ported here, its scene is a templated list.

use std::cell::Cell;
use std::rc::Rc;

use drawnui::PointerKind;
use drawnui::controls::layout::{MeasureBudget, MeasuringStrategy};
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    /// Row heights in points.
    rows: Vec<f32>,
    tapped: Vec<usize>,
    loads: u32,
    grow_by: usize,
    top_loads: u32,
    /// Rows of 50 points `on_load_more_top` inserts at the start per call.
    grow_top_by: usize,
    scroll: Handle<SkiaScroll>,
    list: Handle<SkiaLayout>,
    binds: Rc<Cell<u32>>,
}

fn color(item: usize) -> Color {
    Color::from_argb(255, (item * 53 % 256) as u8, (item * 97 % 256) as u8, 128)
}

fn list(app: &mut App, strategy: MeasuringStrategy, gap: f32) -> Build<SkiaLayout> {
    let binds = app.binds.clone();
    SkiaLayout::column()
        .spacing(gap)
        .measure_items_strategy(strategy)
        .measure_budget(MeasureBudget::Items(0))
        .assign(&mut app.list)
        .items(
            |app: &App| app.rows.len(),
            || {
                let mut row = Handle::default();
                let cell = SkiaShape::new()
                    .fill_x()
                    .use_cache(CacheType::Image)
                    .assign(&mut row)
                    .on_tapped(|me, app: &mut App, _cx| app.tapped.push(me.base().context_index.unwrap()));
                (cell, row)
            },
            move |row: &Handle<SkiaShape>, app: &App, index, cx| {
                binds.set(binds.get() + 1);
                if let Some(mut row) = cx.get_mut(*row) {
                    row.set_height_request(app.rows[index]);
                    row.set_background_color(color(index));
                }
            },
        )
}

fn headless<B: Into<drawnui::Detached>>(
    rows: Vec<f32>,
    (width, height, scale): (f32, f32, f32),
    build: impl FnOnce(&mut App) -> B,
) -> Headless<App> {
    let ui = Ui::new(App { rows, ..App::default() }, build).background(Color::BLACK);
    let mut host = Headless::new(ui, (width * scale) as i32, (height * scale) as i32, scale);
    host.settle();
    host
}

/// A scroll filling a `width` x `height` point canvas over a list with `gap` points between rows.
fn host(
    rows: Vec<f32>,
    strategy: MeasuringStrategy,
    size: (f32, f32, f32),
    gap: f32,
    load_more_offset: f32,
) -> Headless<App> {
    headless(rows, size, move |app| {
        let list = list(app, strategy, gap);
        SkiaScroll::new()
            .fill()
            .bounces(false)
            .load_more_offset(load_more_offset)
            .assign(&mut app.scroll)
            .on_load_more(|_me, app: &mut App, _cx| {
                app.loads += 1;
                let next = app.rows.len();
                app.rows.extend((next..next + app.grow_by).map(|_| 50.0));
            })
            .content(list)
    })
}

/// The same with a handler at each end; the one at the start inserts rows above.
fn host_loading_both_ways(
    rows: Vec<f32>,
    strategy: MeasuringStrategy,
    size: (f32, f32, f32),
    (load_more_offset, load_more_top_offset): (f32, f32),
) -> Headless<App> {
    headless(rows, size, move |app| {
        let list = list(app, strategy, 0.0);
        SkiaScroll::new()
            .fill()
            .bounces(false)
            .load_more_offset(load_more_offset)
            .load_more_top_offset(load_more_top_offset)
            .assign(&mut app.scroll)
            .on_load_more(|_me, app: &mut App, _cx| app.loads += 1)
            .on_load_more_top(|_me, app: &mut App, cx| {
                app.top_loads += 1;
                let count = app.grow_top_by;
                if count > 0 {
                    app.rows.splice(0..0, std::iter::repeat_n(50.0, count));
                    cx.items_inserted(app.list, 0, count);
                }
            })
            .content(list)
    })
}

/// Points the content is scrolled by.
fn scrolled(host: &Headless<App>) -> f32 {
    -host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// Rows with a visible cell: (item index, top on screen, height), pixels.
fn rows(host: &Headless<App>) -> Vec<(usize, f32, f32)> {
    let tree = &host.ui.tree;
    let offset = tree.base(host.ui.state.scroll).unwrap().content_offset.y;
    let mut rows: Vec<(usize, f32, f32)> = tree
        .children(host.ui.state.list)
        .iter()
        .filter_map(|cell| tree.base(*cell))
        .filter(|base| base.p.is_visible)
        .filter_map(|base| Some((base.context_index?, base.rect.top + offset, base.rect.height())))
        .collect();
    rows.sort_by_key(|row| row.0);
    rows
}

fn scroll_to(host: &mut Headless<App>, points: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, -points, 0);
    host.settle();
}

/// Rows follow each other, cover the viewport, and show the pixels of their own item.
fn assert_rows(host: &mut Headless<App>, height: f32, gap: f32) {
    let rows = rows(host);
    assert!(rows[0].1 <= 0.0 && rows.last().unwrap().1 + rows.last().unwrap().2 + gap >= height, "{rows:?}");
    for pair in rows.windows(2) {
        assert_eq!((pair[1].0, pair[1].1), (pair[0].0 + 1, pair[0].1 + pair[0].2 + gap));
    }
    for (index, top, row_height) in rows {
        assert_eq!(row_height, host.ui.state.rows[index]);
        let middle = top + row_height / 2.0;
        if (0.0..height).contains(&middle) {
            assert_eq!(host.pixel(10, middle as i32), color(index), "pixels of row {index}");
        }
    }
}

#[test]
fn ten_thousand_uniform_rows_in_a_scroll() {
    let mut host = host(vec![40.0; 10_000], MeasuringStrategy::MeasureFirst, (300.0, 400.0, 1.0), 2.0, 0.0);
    let (scroll, list) = (host.ui.state.scroll, host.ui.state.list);
    assert_eq!(host.ui.tree.find::<SkiaScroll>(scroll).unwrap().content_size().height, 419_998.0);
    assert_eq!(host.ui.tree.children(list).len(), 10);

    // ScrollToIndex: the list says where the item is, the scroll goes there.
    let offset = host.ui.tree.find::<SkiaLayout>(list).unwrap().item_offset_pixels(100);
    scroll_to(&mut host, offset);
    assert_eq!(rows(&host)[0], (100, 0.0, 40.0));
    assert_rows(&mut host, 400.0, 2.0);
    host.tap(10.0, 3.0 * 42.0 + 5.0);
    assert_eq!(host.ui.state.tapped, [103]);

    // A fling: every frame on the way shows the right rows, with the cells of one viewport.
    let binds = host.ui.state.binds.get();
    host.pan((150.0, 350.0), (150.0, 50.0), 60.0, 6);
    let mut frames = 0;
    while host.ui.needs_frame() {
        host.frame_after(16.0);
        frames += 1;
        assert_rows(&mut host, 400.0, 2.0);
        assert!(host.ui.tree.children(list).len() <= 11);
        assert!(frames < 1_000, "the fling never ends");
    }
    let travelled = scrolled(&host) - offset;
    assert!(travelled > 1_000.0, "the fling went {travelled} points in {frames} frames");
    // One bind per row that entered, none for a row that stayed.
    let entered = rows(&host).last().unwrap().0 - 109;
    assert_eq!(host.ui.state.binds.get() - binds, entered as u32);

    // The end of the content is the end of the last row.
    scroll_to(&mut host, 1_000_000.0);
    assert_eq!(scrolled(&host), 419_998.0 - 400.0);
    assert_eq!(rows(&host).last(), Some(&(9_999, 360.0, 40.0)));
    assert_rows(&mut host, 400.0, 2.0);
}

#[test]
fn uneven_rows_in_a_scroll_stay_exact_in_both_directions() {
    let heights: Vec<f32> = (0..2_000).map(|i| 30.0 + (i * 37 % 50) as f32).collect();
    let mut host = host(heights, MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 0.0);
    let list = host.ui.state.list;
    let middle = host.ui.tree.find::<SkiaLayout>(list).unwrap().item_offset_pixels(1_000);
    scroll_to(&mut host, middle);
    assert_eq!(rows(&host)[0].0, 1_000);
    assert_rows(&mut host, 400.0, 2.0);
    // Up over rows that were only estimated, then down again: a viewport at a time, and by a few pixels.
    for step in [-400.0, -400.0, -33.0, -33.0, -400.0, 250.0, 17.0, 400.0] {
        let to = scrolled(&host) + step;
        scroll_to(&mut host, to);
        assert_rows(&mut host, 400.0, 2.0);
    }
}

/// LoadMoreDistanceTests.LoadMore_FiresOnlyWithinTheDistanceInPoints: 12 x 50 = 600 points of
/// content in a 200 point viewport, 150 points of distance, at scales 1, 2 and 3.
#[test]
fn load_more_fires_only_within_the_distance_in_points() {
    for scale in [1.0, 2.0, 3.0] {
        let mut host = host(vec![50.0; 12], MeasuringStrategy::MeasureFirst, (100.0, 200.0, scale), 0.0, 150.0);
        let frames = |host: &mut Headless<App>| (0..10).for_each(|_| host.frame_after(16.0));
        frames(&mut host);
        assert_eq!(host.ui.state.loads, 0, "scale {scale}: at the top");

        // 100 points down: still 300 from the end, outside the zone.
        scroll_to(&mut host, 100.0);
        frames(&mut host);
        assert_eq!(host.ui.state.loads, 0, "scale {scale}: at -100");

        // 300 points down: 100 from the end, inside the zone.
        scroll_to(&mut host, 300.0);
        frames(&mut host);
        assert_eq!(host.ui.state.loads, 1, "scale {scale}: at -300");
    }
}

#[test]
fn load_more_appends_rows_without_a_bind_or_a_jump() {
    let mut host = host(vec![50.0; 12], MeasuringStrategy::MeasureFirst, (100.0, 200.0, 1.0), 0.0, 150.0);
    let list = host.ui.state.list;
    host.ui.state.grow_by = 20;
    scroll_to(&mut host, 300.0);
    (0..10).for_each(|_| host.frame_after(16.0));
    // The handler added 20 rows; the list took the larger count as "appended".
    assert_eq!(host.ui.state.loads, 1);
    assert_eq!(host.ui.tree.find::<SkiaLayout>(list).unwrap().items_count(), 32);
    assert_eq!(host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().content_size().height, 32.0 * 50.0);
    assert_eq!(scrolled(&host), 300.0);
    assert_eq!(rows(&host)[0], (6, 0.0, 50.0));
}

// ---------------------------------------------------------------- the rows on screen stay where they are

/// 30 to 79 points, in no order: the average a list estimates with is wrong for every row.
fn uneven(count: usize) -> Vec<f32> {
    (0..count).map(|i| 30.0 + (i * 37 % 50) as f32).collect()
}

fn pixels(host: &Headless<App>) -> f32 {
    host.ui.tree.base(host.ui.state.scroll).unwrap().content_offset.y
}

fn scroll(host: &Headless<App>) -> &SkiaScroll {
    host.ui.tree.find(host.ui.state.scroll).unwrap()
}

fn insert_at_start(host: &mut Headless<App>, heights: &[f32]) {
    let list = host.ui.state.list;
    host.ui.state.rows.splice(0..0, heights.iter().copied());
    host.ui.tree.cx().items_inserted(list, 0, heights.len());
    host.ui.state_changed();
}

/// A plain scroll of the same size over plain content, standing at the same offset: how the
/// content moves under a gesture when nothing above the viewport changes its size.
struct Plain {
    scroll: Handle<SkiaScroll>,
}

fn plain(scrolled: f32) -> Headless<Plain> {
    let ui = Ui::new(Plain { scroll: Handle::default() }, |app: &mut Plain| {
        let content = SkiaLayout::column().height_request(100_000);
        SkiaScroll::new().fill().bounces(false).assign(&mut app.scroll).content(content)
    });
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, -scrolled, 0);
    host.settle();
    host
}

fn feed<S: 'static>(host: &mut Headless<S>, kind: PointerKind, (x, y): (f32, f32)) {
    let time_ms = host.time_ms();
    host.ui.pointer(kind, x, y, time_ms);
    host.frame_after(16.0);
}

/// What every frame of a gesture showed.
#[derive(Default)]
struct Frames {
    /// (item, top on screen, height) of the rows with a cell.
    rows: Vec<Vec<(usize, f32, f32)>>,
    /// Number of items: they only ever come in at the start here.
    items: Vec<usize>,
    /// Pixels the scroll of the list has its content moved by.
    offsets: Vec<f32>,
    /// The same for the plain scroll.
    plain: Vec<f32>,
    /// The frame that handled the release.
    release: usize,
}

/// A press at `from`, `steps` moves to `to` 16 ms apart, the release, then frames until both
/// scrolls are at rest: the same input for the scroll of the list and for the plain one.
/// `before_frame` may change the list ahead of a frame.
fn gesture(
    host: &mut Headless<App>,
    plain: &mut Headless<Plain>,
    (from, to, steps): ((f32, f32), (f32, f32), u32),
    mut before_frame: impl FnMut(usize, &mut Headless<App>),
) -> Frames {
    let mut frames = Frames::default();
    let mut events = vec![(PointerKind::Down, from)];
    events.extend((1..=steps).map(|i| {
        let t = i as f32 / steps as f32;
        (PointerKind::Move, (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t))
    }));
    events.push((PointerKind::Up, to));
    frames.release = events.len() - 1;
    let mut frame = 0;
    loop {
        before_frame(frame, host);
        match events.get(frame) {
            Some(&(kind, at)) => {
                feed(host, kind, at);
                feed(plain, kind, at);
            }
            None if host.ui.needs_frame() || plain.ui.needs_frame() => {
                host.frame_after(16.0);
                plain.frame_after(16.0);
            }
            None => return frames,
        }
        frames.rows.push(rows(host));
        frames.items.push(host.ui.state.rows.len());
        frames.offsets.push(pixels(host));
        frames.plain.push(plain.ui.tree.base(plain.ui.state.scroll).unwrap().content_offset.y);
        frame += 1;
        assert!(frame < 900, "the gesture never ends");
    }
}

impl Frames {
    /// Every row on screen moved from frame to frame by what the gesture moved the plain content
    /// by, and by nothing else, in the first `frames` frames. Returns in how many frames up to
    /// the release, and after it, the scroll of the list had to change its offset by more than
    /// that: content above the viewport changed its size there.
    fn assert_rows_follow_the_gesture(&self, frames: usize) -> (u32, u32) {
        let (mut while_down, mut after) = (0, 0);
        for frame in 1..frames.min(self.rows.len()) {
            let expected = self.plain[frame] - self.plain[frame - 1];
            // Rows that came in at the start gave every item a higher index.
            let inserted = self.items[frame] - self.items[frame - 1];
            let mut compared = 0;
            for row in &self.rows[frame - 1] {
                if let Some(now) = self.rows[frame].iter().find(|now| now.0 == row.0 + inserted) {
                    let moved = now.1 - row.1;
                    let same = (moved - expected).abs() < 0.01;
                    assert!(same, "frame {frame}: row {} moved {moved}, the gesture {expected}", row.0);
                    compared += 1;
                }
            }
            assert!(compared > 2, "frame {frame}: the rows on screen were replaced");
            if (self.offsets[frame] - self.offsets[frame - 1] - expected).abs() > 0.5 {
                *(if frame <= self.release { &mut while_down } else { &mut after }) += 1;
            }
        }
        (while_down, after)
    }
}

#[test]
fn rows_on_screen_never_jump_while_a_pan_and_a_fling_go_back_up_over_estimated_rows() {
    let mut host = host(uneven(300), MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 0.0);
    // A jump to the end, as far as the content is known; again when the rows there got their sizes.
    for _ in 0..5 {
        scroll_to(&mut host, 1_000_000.0);
    }
    let last = *rows(&host).last().unwrap();
    assert_eq!((last.0, last.1 + last.2), (299, 400.0));
    assert_rows(&mut host, 400.0, 2.0);
    let measured = host.ui.tree.find::<SkiaLayout>(host.ui.state.list).unwrap().measured_items();
    assert!(measured < 40, "{measured} rows measured: only what was on screen");

    // The finger drags the content down 320 points, then lets it fly: rows that were only
    // estimated come in at the top and get their real sizes, in almost every frame.
    let mut plain = plain(scrolled(&host));
    let frames = gesture(&mut host, &mut plain, ((150.0, 60.0), (150.0, 380.0), 32), |_, _| {});
    let (while_down, in_the_fling) = frames.assert_rows_follow_the_gesture(usize::MAX);
    assert!(while_down >= 4 && in_the_fling >= 4, "sizes changed above in {while_down} + {in_the_fling} frames");
    assert!(frames.rows.len() > frames.release + 30, "the fling ran {} frames", frames.rows.len() - frames.release);

    // At rest: the fling ran out between pixels (React), the content is drawn on the nearest one;
    // the rows are exact, nothing is left to do.
    assert_eq!(pixels(&host), (-scrolled(&host) + 0.5).floor());
    assert!(!host.ui.needs_frame() && !scroll(&host).is_animating());
    assert_rows(&mut host, 400.0, 2.0);
}

#[test]
fn rows_inserted_at_the_start_leave_the_rows_on_screen_where_they_are() {
    let mut host = host(uneven(300), MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 0.0);
    scroll_to(&mut host, 3_000.0);
    let (before, at) = (rows(&host), scrolled(&host));

    insert_at_start(&mut host, &[50.0; 5]);
    host.frame_after(16.0);
    // The very next frame: the same items at the same places, five indices further down.
    let shifted: Vec<_> = before.iter().map(|row| (row.0 + 5, row.1, row.2)).collect();
    assert_eq!(rows(&host), shifted);
    // The new rows are not measured: they count as five estimated rows, and the scroll went with them.
    let list = host.ui.tree.find::<SkiaLayout>(host.ui.state.list).unwrap();
    assert_eq!(scrolled(&host), at + 5.0 * (list.item_height_pixels(0) + 2.0));
    assert!(!list.is_item_measured(0));
    host.settle();
    assert_eq!(rows(&host), shifted);

    // The same under the finger and in the middle of a fling: the motion goes on as if nothing came in.
    let mut plain = plain(scrolled(&host));
    let frames = gesture(&mut host, &mut plain, ((150.0, 100.0), (150.0, 300.0), 10), |frame, host| {
        if frame == 5 || frame == 20 {
            insert_at_start(host, &[50.0; 8]);
        }
    });
    frames.assert_rows_follow_the_gesture(usize::MAX);
    assert_eq!(host.ui.state.rows.len(), 321);
    assert_eq!(pixels(&host), pixels(&host).round());
}

// ---------------------------------------------------------------- ScrollToIndex

fn scroll_to_index(host: &mut Headless<App>, index: usize, position: RelativePositionType, ms: f32) {
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to_index(scroll, index, position, ms);
}

fn row(host: &Headless<App>, index: usize) -> (usize, f32, f32) {
    *rows(host).iter().find(|row| row.0 == index).unwrap_or_else(|| panic!("row {index} is not on screen"))
}

#[test]
fn scroll_to_index_lands_on_a_row_of_known_size_at_once() {
    let mut host = host(vec![40.0; 10_000], MeasuringStrategy::MeasureFirst, (300.0, 400.0, 1.0), 2.0, 0.0);
    scroll_to_index(&mut host, 5_000, RelativePositionType::Start, 0.0);
    // No frame yet: the offset is there already.
    assert_eq!(scrolled(&host), 5_000.0 * 42.0);
    host.settle();
    assert_eq!(rows(&host)[0], (5_000, 0.0, 40.0));
    assert!(!scroll(&host).has_pending_scroll_order());

    scroll_to_index(&mut host, 50, RelativePositionType::End, 0.0);
    host.settle();
    assert_eq!(row(&host, 50), (50, 360.0, 40.0));
    // A row that cannot go that low: as far as the content goes.
    scroll_to_index(&mut host, 7, RelativePositionType::End, 0.0);
    host.settle();
    assert_eq!(row(&host, 7), (7, 7.0 * 42.0, 40.0));
    scroll_to_index(&mut host, 100, RelativePositionType::Center, 0.0);
    host.settle();
    assert_eq!(row(&host, 100), (100, 180.0, 40.0));
    // Past the content: as far as it goes.
    scroll_to_index(&mut host, 9_999, RelativePositionType::Start, 0.0);
    host.settle();
    assert_eq!(rows(&host).last(), Some(&(9_999, 360.0, 40.0)));
    // No such row: nothing moves, and the order does not stay open.
    scroll_to_index(&mut host, 10_000, RelativePositionType::Start, 0.0);
    host.settle();
    assert_eq!(rows(&host).last(), Some(&(9_999, 360.0, 40.0)));
    assert!(!scroll(&host).has_pending_scroll_order() && !host.ui.needs_frame());
}

#[test]
fn scroll_to_index_reaches_a_far_row_nobody_measured_exactly() {
    let heights = uneven(2_000);
    let mut host = host(heights.clone(), MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 0.0);
    let list = host.ui.state.list;
    let measured = |host: &Headless<App>| host.ui.tree.find::<SkiaLayout>(list).unwrap().measured_items();

    scroll_to_index(&mut host, 1_500, RelativePositionType::Start, 0.0);
    // It went where the row is estimated to be, and waits for the size of the row.
    assert!(scroll(&host).has_pending_scroll_order());
    host.settle();
    assert_eq!(rows(&host)[0], (1_500, 0.0, heights[1_500]));
    assert!(!scroll(&host).has_pending_scroll_order());
    assert_rows(&mut host, 400.0, 2.0);
    // Only what was on screen on the way got measured: the rows in between are still estimates.
    assert!(measured(&host) < 40, "{} rows measured", measured(&host));

    // Center and End need the size of the row itself, which is known once it is on screen.
    scroll_to_index(&mut host, 700, RelativePositionType::Center, 0.0);
    host.settle();
    let centered = row(&host, 700);
    assert_eq!(centered.2, heights[700]);
    assert!((centered.1 - (400.0 - heights[700]) / 2.0).abs() <= 0.5, "{centered:?}");
    scroll_to_index(&mut host, 1_999, RelativePositionType::End, 0.0);
    host.settle();
    let last = row(&host, 1_999);
    assert_eq!((last.1 + last.2, last.2), (400.0, heights[1_999]));
    assert_rows(&mut host, 400.0, 2.0);
    assert!(!scroll(&host).has_pending_scroll_order() && !host.ui.needs_frame());
}

#[test]
fn an_animated_scroll_to_index_follows_the_row_and_a_touch_ends_it() {
    let heights = uneven(2_000);
    let mut host = host(heights.clone(), MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 0.0);
    scroll_to_index(&mut host, 300, RelativePositionType::Start, 400.0);
    let mut frames = 0;
    while host.ui.needs_frame() {
        host.frame_after(16.0);
        frames += 1;
        // Every frame on the way shows real rows.
        assert_rows(&mut host, 400.0, 2.0);
        assert!(frames < 200, "the order never ends");
    }
    // The 400 ms of the animation, then the row stands at the top, whatever the rows on the way
    // turned out to be.
    assert!(frames >= 25, "{frames} frames");
    assert_eq!(rows(&host)[0], (300, 0.0, heights[300]));
    assert!(!scroll(&host).has_pending_scroll_order());

    // A finger on the way takes over: the order is dropped where the content is.
    scroll_to_index(&mut host, 1_500, RelativePositionType::Start, 400.0);
    for _ in 0..4 {
        host.frame_after(16.0);
    }
    assert!(scroll(&host).has_pending_scroll_order() && scroll(&host).is_animating());
    feed(&mut host, PointerKind::Down, (150.0, 200.0));
    assert!(!scroll(&host).has_pending_scroll_order() && !scroll(&host).is_animating());
    let stopped = scrolled(&host);
    feed(&mut host, PointerKind::Up, (150.0, 200.0));
    host.settle();
    assert_eq!(scrolled(&host), stopped);
    assert!(rows(&host)[0].0 < 1_500);
}

#[test]
fn scroll_to_index_finds_a_list_below_other_content() {
    // A header of 120 points above the list, both in one stack inside the scroll.
    let mut host = headless(vec![40.0; 500], (300.0, 400.0, 1.0), |app| {
        let list = list(app, MeasuringStrategy::MeasureFirst, 0.0);
        let header = SkiaShape::new().fill_x().height_request(120).background_color(Color::WHITE);
        let content = SkiaLayout::column().spacing(0).children((header, list));
        SkiaScroll::new().fill().assign(&mut app.scroll).content(content)
    });
    scroll_to_index(&mut host, 50, RelativePositionType::Start, 0.0);
    host.settle();
    assert_eq!(scrolled(&host), 120.0 + 50.0 * 40.0);
    assert_eq!(row(&host, 50), (50, 0.0, 40.0));
    scroll_to_index(&mut host, 0, RelativePositionType::Start, 0.0);
    host.settle();
    // The first row at the top: the header is above the viewport.
    assert_eq!(scrolled(&host), 120.0);
}

// ---------------------------------------------------------------- LoadMore at the start

/// React CheckLoadMore: the start calls when the viewport comes back into its zone after it was
/// more than the zone and 100 points away; never when the scroll opens.
fn leave_the_start(host: &mut Headless<App>) {
    scroll_to(host, 900.0);
}

#[test]
fn load_more_top_inserts_above_and_the_viewport_stays_on_its_rows() {
    // 20 rows of 50 points in 200, a zone of 100 points at the start.
    let size = (100.0, 200.0, 1.0);
    let mut host = host_loading_both_ways(vec![50.0; 20], MeasuringStrategy::MeasureFirst, size, (0.0, 100.0));
    // A fresh scroll at its start calls nothing (React).
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    assert_eq!((host.ui.state.top_loads, host.ui.state.loads), (0, 0));

    // Away for more than 100 points, then back into the zone: it runs, adds ten rows above, and
    // the rows on screen do not move.
    host.ui.state.grow_top_by = 10;
    leave_the_start(&mut host);
    scroll_to(&mut host, 90.0);
    assert_eq!(host.ui.state.top_loads, 1);
    assert_eq!(scrolled(&host), 590.0);
    assert_eq!(rows(&host)[0], (11, -40.0, 50.0));
    assert_eq!(host.ui.state.rows.len(), 30);
    // 590 points from the start now: outside the zone, nothing more is asked for.
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    assert_eq!(host.ui.state.top_loads, 1);
}

#[test]
fn load_more_top_calls_once_per_return_to_the_start() {
    // A zone of 300 points and one row of 50 per call. React: one call on the way back in; the
    // viewport stays in the zone after it (140 points from the start), and nothing more comes until
    // it was 400 points away again. No row on screen moves.
    let size = (100.0, 200.0, 1.0);
    let mut host = host_loading_both_ways(vec![50.0; 20], MeasuringStrategy::MeasureFirst, size, (0.0, 300.0));
    leave_the_start(&mut host);
    host.ui.state.grow_top_by = 1;
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to(scroll, 0.0, -90.0, 0);
    host.frame_after(16.0);
    let first = rows(&host)[0];
    for _ in 0..30 {
        host.frame_after(16.0);
        let now = rows(&host)[0];
        assert_eq!((now.1, now.2), (first.1, first.2));
    }
    assert_eq!((host.ui.state.top_loads, scrolled(&host)), (1, 140.0));
    scroll_to(&mut host, 0.0);
    assert_eq!(host.ui.state.top_loads, 1);
    leave_the_start(&mut host);
    scroll_to(&mut host, 0.0);
    assert_eq!(host.ui.state.top_loads, 2);
}

#[test]
fn a_fling_into_the_start_flies_on_into_the_rows_a_load_more_top_added() {
    let size = (300.0, 400.0, 1.0);
    let mut host = host_loading_both_ways(vec![50.0; 40], MeasuringStrategy::MeasureFirst, size, (0.0, 200.0));
    leave_the_start(&mut host);
    scroll_to(&mut host, 600.0);
    host.ui.state.grow_top_by = 60;

    // A flick down that goes 1,100 points: 600 are there. Planned to stop at the start, it
    // meets 3,000 points of new rows 200 points before it.
    let mut plain = plain(5_000.0);
    let frames = gesture(&mut host, &mut plain, ((150.0, 100.0), (150.0, 300.0), 6), |_, _| {});
    assert_eq!(host.ui.state.top_loads, 1);
    assert_eq!(host.ui.state.rows.len(), 100);
    // The rows on screen moved like the content of a scroll with room to spare, the frame of the
    // insert included. (The last frames are the landing on a whole pixel: it depends on the place.)
    frames.assert_rows_follow_the_gesture(frames.release + 60);
    let travelled = frames.plain.last().unwrap() - frames.plain[0];
    assert!(travelled > 1_000.0, "{travelled}");
    // Where it stands: 3,000 points of new rows above, less what the fling went on top of the 600.
    assert!((scrolled(&host) - (600.0 + 3_000.0 - travelled)).abs() <= 1.0, "{}", scrolled(&host));
    assert_eq!(pixels(&host), pixels(&host).round());
}

#[test]
fn rows_the_start_added_count_as_the_way_away() {
    // Uneven rows, estimated until they were on screen. React does not wait for them to be
    // measured: one call on the way in; the inserted rows push the start away, and it calls again
    // only when the viewport comes back after it was the zone and 100 points away.
    let size = (300.0, 400.0, 1.0);
    let mut host = host_loading_both_ways(uneven(300), MeasuringStrategy::MeasureVisible, size, (0.0, 200.0));
    leave_the_start(&mut host);
    host.ui.state.grow_top_by = 10;
    scroll_to(&mut host, 90.0);
    assert_eq!(host.ui.state.top_loads, 1);
    assert!(scrolled(&host) > 300.0 && !host.ui.needs_frame(), "{}", scrolled(&host));
    // Back to the start from there: the rows above were more than 300 points: a call.
    scroll_to(&mut host, 0.0);
    assert_eq!(host.ui.state.top_loads, 2);
}

#[test]
fn each_end_calls_once_for_the_same_content() {
    // 6 rows = 300 points in 200: 100 of travel, and a zone of 60 points at each end. React has no
    // wait between the two ends (C# IsOppositeLoadMoreBlocked).
    let size = (100.0, 200.0, 1.0);
    let mut host = host_loading_both_ways(vec![50.0; 6], MeasuringStrategy::MeasureFirst, size, (60.0, 60.0));
    let scroll = host.ui.state.scroll;
    assert_eq!((host.ui.state.top_loads, host.ui.state.loads), (0, 0));

    // Straight to the end: it calls at once.
    host.ui.tree.cx().scroll_to(scroll, 0.0, -100.0, 0);
    host.frame_after(16.0);
    assert_eq!((host.ui.state.top_loads, host.ui.state.loads), (0, 1));

    // The same places do not call again: the end needs other content or a way away of more than
    // 160 points, the start a way away of more than 160 points; the travel is 100.
    host.frame_after(5_000.0);
    for y in [-30.0, 0.0, -100.0, 0.0] {
        host.ui.tree.cx().scroll_to(scroll, 0.0, y, 0);
        host.settle();
    }
    assert_eq!((host.ui.state.top_loads, host.ui.state.loads), (0, 1));
}

#[test]
fn load_more_waits_while_a_scroll_to_index_is_on_its_way() {
    // A zone as long as the content: the end handler runs whenever the items change. The app adds
    // one while the viewport is on its way to a far row (where the length of the list also
    // changes in every frame, rows getting their sizes, which is no new content).
    let mut host = host(uneven(2_000), MeasuringStrategy::MeasureVisible, (300.0, 400.0, 1.0), 2.0, 1_000_000.0);
    let loads = host.ui.state.loads;
    assert!(loads > 0);
    scroll_to_index(&mut host, 1_500, RelativePositionType::Start, 400.0);
    host.ui.state.rows.push(50.0);
    host.ui.state_changed();
    let mut frames = 0;
    while scroll(&host).has_pending_scroll_order() {
        assert_eq!(host.ui.state.loads, loads, "frame {frames}");
        host.frame_after(16.0);
        frames += 1;
        assert!(frames < 200, "the order never ends");
    }
    // Arrived, after the 400 ms and more: the edges are looked at again.
    assert!(frames >= 25, "{frames} frames");
    host.settle();
    assert_eq!(host.ui.state.loads, loads + 1);
    assert_eq!(rows(&host)[0].0, 1_500);
}

/// The setup of HelloMaui's UnevenCellsPage: 200 uneven rows, MeasureVisible, a zone of 300
/// points at the end and of 100 at the start. Probed on the C# engine: the start handler runs
/// once when the page opens, without any input; when it inserts 30 rows they come in above the
/// viewport at once and it is not called again; coming back into the zone later calls it again.
#[test]
fn load_more_top_waits_for_the_user_to_come_back_to_the_start() {
    // The setup of HelloMaui's UnevenCellsPage: 200 uneven rows, MeasureVisible, a zone of 300
    // points at the end and of 100 at the start. React: nothing when the page opens (C# calls the
    // start once there); a way back into the zone after being away calls it.
    let size = (400.0, 600.0, 1.0);
    let mut host = host_loading_both_ways(uneven(200), MeasuringStrategy::MeasureVisible, size, (300.0, 100.0));
    for _ in 0..300 {
        host.frame_after(16.0);
    }
    assert_eq!((host.ui.state.top_loads, host.ui.state.loads, scrolled(&host)), (0, 0, 0.0));

    // Away, then to 150 points from the start: outside the zone. A pan down into it: one call.
    scroll_to(&mut host, 2_000.0);
    scroll_to(&mut host, 150.0);
    assert_eq!(host.ui.state.top_loads, 0);
    host.fling((200.0, 200.0), (200.0, 400.0), 320.0, 20);
    assert_eq!((host.ui.state.top_loads, scrolled(&host)), (1, 0.0));

    // A handler that would load 30 rows: not called when the page opens.
    let mut host = headless(uneven(200), size, |app| {
        app.grow_top_by = 30;
        let list = list(app, MeasuringStrategy::MeasureVisible, 8.0);
        SkiaScroll::new()
            .fill()
            .load_more_offset(300)
            .load_more_top_offset(100)
            .assign(&mut app.scroll)
            .on_load_more_top(|_me, app: &mut App, cx| {
                app.top_loads += 1;
                app.rows.splice(0..0, std::iter::repeat_n(50.0, app.grow_top_by));
                cx.items_inserted(app.list, 0, app.grow_top_by);
            })
            .content(list)
    });
    for _ in 0..300 {
        host.frame_after(16.0);
    }
    assert_eq!((host.ui.state.top_loads, host.ui.state.rows.len()), (0, 200));
}

