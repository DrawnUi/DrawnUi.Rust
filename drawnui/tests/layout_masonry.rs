//! LayoutType::Masonry: `split` columns, every item under the column that ends highest, children
//! or `items`. Ports MasonryLayoutTests (`DrawnUi.Net.Tests`, C# 1.10.8.1): the same trees, scale
//! 1, two 100 pt columns, spacing 0. Plus what the Rust slots give: an append or a change binds
//! (and so measures) only the new or changed items.

use std::cell::{Cell, RefCell};

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Wrap rows take the tallest item of each row: (100,20) (20,100) (100,20) = 300. Masonry: col0
/// 100, col1 20 -> item2 col1 (20..40) -> item3 col1 (40..140) -> item4 col0 (100..200) -> item5
/// col1 (140..160): 200 tall.
const HEIGHTS: [i32; 6] = [100, 20, 20, 100, 100, 20];

fn height(i: usize) -> i32 {
    HEIGHTS[i % HEIGHTS.len()]
}

/// Reference placement: two 100 pt columns, each item to the shortest one (the first on a tie).
fn expected(heights: &[i32]) -> (Vec<(f32, f32)>, f32) {
    let mut bottoms = [0f32; 2];
    let placed = heights
        .iter()
        .map(|h| {
            let column = if bottoms[1] < bottoms[0] { 1 } else { 0 };
            let at = (column as f32 * 100.0, bottoms[column]);
            bottoms[column] += *h as f32;
            at
        })
        .collect();
    (placed, bottoms[0].max(bottoms[1]))
}

fn boxes(kind: Build<SkiaLayout>, heights: &[i32]) -> Build<SkiaLayout> {
    kind.split(2).spacing(0).children(
        heights
            .iter()
            .map(|h| SkiaShape::new().fill_x().height_request(*h).background_color(Color::from_rgb(72, 61, 139)))
            .collect::<Vec<_>>(),
    )
}

#[derive(Default)]
struct Two {
    wrap: Handle<SkiaLayout>,
    masonry: Handle<SkiaLayout>,
    second: Handle<SkiaShape>,
}

/// The rects of the children of `layout`, from its own corner.
fn placed<S>(host: &Headless<S>, layout: impl Into<ControlId>) -> Vec<Rect> {
    let id = layout.into();
    let origin = host.rect(id);
    host.ui.tree.children(id).iter().map(|c| host.rect(*c).with_offset((-origin.left, -origin.top))).collect()
}

fn measured_height<S>(host: &Headless<S>, layout: impl Into<ControlId>) -> f32 {
    host.ui.tree.base(layout.into()).unwrap().measured.height
}

#[test]
fn children_fill_the_shortest_column() {
    let build = |app: &mut Two| {
        SkiaLayer::new().fill().children((
            boxes(SkiaWrap::new(), &HEIGHTS).assign(&mut app.wrap),
            boxes(SkiaMasonry::new(), &HEIGHTS).assign(&mut app.masonry),
        ))
    };
    let mut host = Headless::new(Ui::new(Two::default(), build), 200, 400, 1.0);
    host.settle();
    let (wrap, masonry) = (host.ui.state.wrap, host.ui.state.masonry);
    assert_eq!(measured_height(&host, wrap), 300.0);
    assert_eq!(measured_height(&host, masonry), 200.0);
    let (at, _) = expected(&HEIGHTS);
    for (i, r) in placed(&host, masonry).iter().enumerate() {
        assert_eq!((r.left, r.top, r.width(), r.height()), (at[i].0, at[i].1, 100.0, HEIGHTS[i] as f32), "#{i}");
    }
}

#[test]
fn spacing_goes_between_columns_and_between_items() {
    let build = |app: &mut Two| SkiaLayer::new().fill().children((boxes(SkiaMasonry::new(), &HEIGHTS).spacing(10).assign(&mut app.masonry),));
    let mut host = Headless::new(Ui::new(Two::default(), build), 210, 400, 1.0);
    host.settle();
    let masonry = host.ui.state.masonry;
    // Columns of (210 - 10) / 2 = 100; col0 0..100, col1 20, 40..60, 70..170, col0 110..210, col1 180..200.
    let tops = [(0.0, 0.0), (110.0, 0.0), (110.0, 30.0), (110.0, 60.0), (0.0, 110.0), (110.0, 170.0)];
    let rects: Vec<(f32, f32)> = placed(&host, masonry).iter().map(|r| (r.left, r.top)).collect();
    assert_eq!(rects, tops);
    assert_eq!(measured_height(&host, masonry), 210.0);
}

#[test]
fn child_grows_layout_follows() {
    let build = |app: &mut Two| {
        let shapes = HEIGHTS.iter().enumerate().map(|(i, h)| {
            let shape = SkiaShape::new().fill_x().height_request(*h).background_color(Color::BLUE);
            if i == 1 { shape.assign(&mut app.second) } else { shape }
        });
        SkiaLayer::new().fill().children((SkiaMasonry::new().spacing(0).assign(&mut app.masonry).children(shapes.collect::<Vec<_>>()),))
    };
    let mut host = Headless::new(Ui::new(Two::default(), build), 200, 400, 1.0);
    host.settle();
    let (masonry, second) = (host.ui.state.masonry, host.ui.state.second);
    host.ui.tree.get_mut(second).unwrap().set_height_request(80);
    host.settle();
    let mut heights = HEIGHTS;
    heights[1] = 80;
    let (at, total) = expected(&heights);
    assert_eq!(measured_height(&host, masonry), total);
    for (i, r) in placed(&host, masonry).iter().enumerate() {
        assert_eq!((r.left, r.top, r.height()), (at[i].0, at[i].1, heights[i] as f32), "#{i}");
    }
}

// ---------------------------------------------------------------- templated

#[derive(Default)]
struct Feed {
    heights: Vec<i32>,
    /// Items to add at each load more (none: the handler only counts).
    page: Vec<i32>,
    loads: u32,
    taps: Vec<usize>,
    binds: Cell<u32>,
    /// The view each item was last bound to.
    views: RefCell<Vec<(usize, Handle<SkiaShape>)>>,
    scroll: Handle<SkiaScroll>,
    masonry: Handle<SkiaLayout>,
}

fn feed(heights: Vec<i32>, viewport: (i32, i32)) -> Headless<Feed> {
    let build = |app: &mut Feed| {
        SkiaScroll::new()
            .fill()
            .bounces(false)
            .load_more_offset(50)
            .assign(&mut app.scroll)
            .on_load_more(|_me, app: &mut Feed, _cx| {
                app.loads += 1;
                let page = app.page.clone();
                app.heights.extend(page);
            })
            .content(
                SkiaMasonry::new().spacing(0).assign(&mut app.masonry).items(
                    |app: &Feed| app.heights.len(),
                    || {
                        let mut shape = Handle::default();
                        let cell = SkiaShape::new()
                            .fill_x()
                            .background_color(Color::from_rgb(72, 61, 139))
                            .assign(&mut shape)
                            .on_tapped(|me, app: &mut Feed, _cx| app.taps.push(me.base().context_index.expect("a bound cell")));
                        (cell, shape)
                    },
                    |shape: &Handle<SkiaShape>, app: &Feed, index, cx| {
                        app.binds.set(app.binds.get() + 1);
                        app.views.borrow_mut().push((index, *shape));
                        if let Some(mut shape) = cx.get_mut(*shape) {
                            shape.set_height_request(app.heights[index]);
                        }
                    },
                ),
            )
    };
    let mut host = Headless::new(Ui::new(Feed { heights, ..Feed::default() }, build), viewport.0, viewport.1, 1.0);
    host.settle();
    host
}

fn templated(items: usize) -> Headless<Feed> {
    feed((0..items).map(height).collect(), (200, 200))
}

fn masonry(host: &Headless<Feed>) -> &SkiaLayout {
    host.ui.tree.find::<SkiaLayout>(host.ui.state.masonry).unwrap()
}

/// Every item at the place a fresh layout of these heights gives it, the content as tall.
fn assert_placed_as_fresh(host: &Headless<Feed>) {
    let heights = &host.ui.state.heights;
    let (at, total) = expected(heights);
    assert_eq!(measured_height(host, host.ui.state.masonry), total);
    let layout = masonry(host);
    assert_eq!(layout.items_count(), heights.len());
    let slots = host.ui.tree.children(host.ui.state.masonry);
    let origin = host.rect(host.ui.state.masonry);
    for (i, h) in heights.iter().enumerate() {
        let r = host.rect(slots[i]);
        assert_eq!((r.left - origin.left, r.top - origin.top, r.height()), (at[i].0, at[i].1, *h as f32), "#{i}");
        assert_eq!(layout.item_offset_pixels(i), at[i].1, "#{i} offset");
    }
}

fn scroll_to_end(host: &mut Headless<Feed>) {
    let (scroll, masonry) = (host.ui.state.scroll, host.ui.state.masonry);
    let end = measured_height(host, masonry) - 200.0;
    host.ui.tree.cx().set_viewport_offset_y(scroll, -end);
    host.ui.state_changed();
    for _ in 0..12 {
        host.frame_after(16.0);
    }
    host.settle();
}

#[test]
fn templated_same_layout_as_children_draws_cells() {
    let mut host = templated(30);
    assert_placed_as_fresh(&host);
    let in_use = masonry(&host).cells_in_use().count();
    assert!(in_use > 0 && in_use < 30, "{in_use} views for 30 items: not recycled");
    // The viewport is covered: 200 x 200 of cells.
    let lit = (0..20).flat_map(|y| (0..20).map(move |x| (x * 10 + 5, y * 10 + 5))).filter(|(x, y)| host.pixel(*x, *y) != Color::BLACK).count();
    assert!(lit > 360, "cells were not drawn: {lit} of 400 points");
}

#[test]
fn templated_load_more_fires_at_the_end() {
    let mut host = templated(30);
    scroll_to_end(&mut host);
    assert!(host.ui.state.loads > 0, "load more never fired at the end of a templated masonry");
}

#[test]
fn templated_scroll_to_index_arrives() {
    let mut host = templated(30);
    let (at, _) = expected(&host.ui.state.heights);
    let top = at[15].1; // 400, a right column item
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().scroll_to_index(scroll, 15, RelativePositionType::Start, 0.0);
    host.settle();
    let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
    assert_eq!(offset, -top);
}

#[test]
fn templated_tap_hits_the_cell_under_the_finger() {
    let mut host = templated(30);
    host.tap(150.0, 90.0); // right column, y 40..140 = item 3
    host.settle();
    host.tap(50.0, 150.0); // left column, y 100..200 = item 4
    host.settle();
    assert_eq!(host.ui.state.taps, vec![3, 4]);
}

#[test]
fn templated_cell_grows_items_are_placed_again() {
    let mut host = feed((0..12).map(height).collect(), (200, 400));
    let binds = host.ui.state.binds.get();
    host.ui.state.heights[1] = 80; // right column, 20 -> 80
    let masonry = host.ui.state.masonry;
    host.ui.tree.cx().items_changed(masonry, 1);
    host.ui.state_changed();
    host.settle();
    assert_placed_as_fresh(&host);
    assert_eq!(host.ui.state.binds.get() - binds, 1, "only the changed item is bound and measured again");
}

#[test]
fn templated_visible_cell_grows_again_and_again() {
    let mut host = feed((0..12).map(height).collect(), (200, 400));
    let masonry = host.ui.state.masonry;
    for _ in 0..6 {
        host.ui.state.heights[2] += 20; // on screen, right column
        host.ui.tree.cx().items_changed(masonry, 2);
        host.ui.state_changed();
        host.settle();
        assert_placed_as_fresh(&host);
    }
}

/// A view that changes its own size (an image arrived), with nothing reported to the layout: its
/// slot is measured again and every item placed again, every time.
#[test]
fn templated_view_grows_by_itself_again_and_again() {
    let mut host = feed((0..12).map(height).collect(), (200, 400));
    for _ in 0..6 {
        host.ui.state.heights[2] += 20; // on screen, right column
        let view = host.ui.state.views.borrow().iter().rev().find(|(index, _)| *index == 2).unwrap().1;
        let grown = host.ui.state.heights[2];
        host.ui.tree.get_mut(view).unwrap().set_height_request(grown);
        host.settle();
        assert_placed_as_fresh(&host);
    }
}

#[test]
fn templated_new_items_of_the_same_count_lay_out_fresh() {
    let mut host = templated(30);
    host.ui.state.heights = (0..30).map(|i| height(29 - i)).collect();
    let masonry = host.ui.state.masonry;
    host.ui.tree.cx().items_reset(masonry);
    host.ui.state_changed();
    host.settle();
    assert_placed_as_fresh(&host);
}

#[test]
fn templated_append_places_new_items_under_the_columns() {
    let mut host = templated(30);
    let masonry = host.ui.state.masonry;
    let binds = host.ui.state.binds.get();
    for h in [140, 20, 60, 90, 30, 110, 50, 20, 80, 40, 130, 70] {
        // one change per item, as an observable collection raises them
        let at = host.ui.state.heights.len();
        host.ui.state.heights.push(h);
        host.ui.tree.cx().items_inserted(masonry, at, 1);
    }
    host.ui.state_changed();
    host.settle();
    assert_placed_as_fresh(&host);
    // The 12 new items are measured; the 30 before keep their sizes (views bound to the slots
    // that can be seen are not counted: none of them changed).
    assert_eq!(host.ui.state.binds.get() - binds, 12, "an append measured items that were there");
}

#[test]
fn templated_insert_and_remove_lay_out_fresh() {
    let mut host = templated(30);
    let masonry = host.ui.state.masonry;
    host.ui.state.heights.insert(5, 140);
    host.ui.tree.cx().items_inserted(masonry, 5, 1);
    host.ui.state_changed();
    host.settle();
    assert_placed_as_fresh(&host);

    // a remove and an append before one measure
    host.ui.state.heights.remove(0);
    host.ui.tree.cx().items_removed(masonry, 0, 1);
    host.ui.state.heights.push(90);
    let at = host.ui.state.heights.len() - 1;
    host.ui.tree.cx().items_inserted(masonry, at, 1);
    host.ui.state_changed();
    host.settle();
    assert_placed_as_fresh(&host);
}

#[test]
fn templated_load_more_pages_keep_the_layout() {
    let mut host = templated(30);
    for round in 1..=3 {
        host.ui.state.page = (0..10).map(|i| height(i + round) + 10 * round as i32).collect();
        scroll_to_end(&mut host);
    }
    assert!(host.ui.state.loads >= 3, "load more fired {} times", host.ui.state.loads);
    assert_placed_as_fresh(&host);
}

#[test]
fn templated_big_source_lays_out_every_item() {
    let host = templated(1_100);
    assert_placed_as_fresh(&host);
}
