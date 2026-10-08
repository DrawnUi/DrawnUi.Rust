//! Templated Row, Wrap, Grid, decorated grid and split Column follow RecyclingTemplate (drawnui-cross
//! 6b, DrawnUi.React 6a29273): Enabled lays out a slot per item measured through pooled views and
//! binds views only to the slots that can be seen (every slot under a cache); Disabled gives every
//! item a view of its own. Both draw the same pixels: the same trees side by side at scroll offsets
//! and after item, Split and mode changes. Plus the views in use, taps under a cached card, no
//! allocation per scrolled frame, and a measurement of a 1000-item wrap. Ports
//! WrapTemplatedSplitTests (`DrawnUi.Net.Tests`).

mod list_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::controls::layout::decorated_grid::SkiaDecoratedGrid;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::{Viewport, color, scroll_to as viewport_to};

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

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

/// The templated layouts the order names.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    WrapSplit3,
    WrapFlow,
    WrapSplit4Dynamic,
    Row,
    GridSplit3Invert,
    DecoratedSplit4,
    ColumnSplit2,
    /// A Split 3 wrap inside a cached card in the scroll: every slot draws there.
    CachedCard,
}

impl Kind {
    /// Cells that fill their column take a height per item; the others a size per item.
    fn fills(self) -> bool {
        !matches!(self, Kind::WrapFlow | Kind::Row)
    }
}

#[derive(Default)]
struct App {
    /// The ids of the items, in order: an id gives its color, size and text.
    items: Vec<u32>,
    /// What each bind sets: a label (`true`) or only colors and sizes.
    text: bool,
    /// More than 0: the items whose id is a multiple of it fill their line, the others do not
    /// (a decision per item); 0: as the kind says.
    fill_every: u32,
    binds: Cell<u32>,
    layout: Option<ControlId>,
    scroll: Option<ControlId>,
    taps: Vec<usize>,
}

#[derive(Default)]
struct Handles {
    shape: Handle<SkiaShape>,
    label: Handle<SkiaLabel>,
}

fn template() -> (Build<SkiaShape>, Handles) {
    let mut handles = Handles::default();
    let cell = SkiaShape::new()
        .corner_radius(6)
        .stroke_color(Color::WHITE)
        .stroke_width(1)
        .assign(&mut handles.shape)
        .on_tapped(|me, app: &mut App, _cx| app.taps.push(me.base().context_index.expect("a bound cell")))
        .children(SkiaLabel::new("").font_size(12).text_color(Color::WHITE).center().assign(&mut handles.label));
    (cell, handles)
}

fn bind(kind: Kind) -> impl FnMut(&Handles, &App, usize, &mut Cx) + 'static {
    move |cell, app, index, cx| {
        app.binds.set(app.binds.get() + 1);
        let id = app.items[index];
        let fills = if app.fill_every > 0 { id % app.fill_every == 0 } else { kind.fills() };
        if let Some(mut shape) = cx.get_mut(cell.shape) {
            shape.set_background_color(color(id as usize));
            shape.set_height_request(30.0 + (id * 37 % 40) as f32);
            // A decision per item, made by the bind: the slot learns it when it measures.
            shape.set_horizontal_options(if fills { LayoutOptions::Fill } else { LayoutOptions::Start });
            shape.set_width_request(if fills { -1.0 } else { 40.0 + (id * 53 % 70) as f32 });
        }
        if app.text
            && let Some(mut label) = cx.get_mut(cell.label)
        {
            label.set_text(id.to_string());
        }
    }
}

/// The templated layout of `kind` with `recycling`, in a scroll filling a 300 x 400 point canvas.
fn scene(kind: Kind, recycling: RecyclingTemplate, count: u32, scale: f32) -> Headless<App> {
    scene_with(kind, recycling, count, scale, 0)
}

fn scene_with(kind: Kind, recycling: RecyclingTemplate, count: u32, scale: f32, fill_every: u32) -> Headless<App> {
    let build = move |app: &mut App| {
        let items = |layout: Build<SkiaLayout>| {
            layout.recycling_template(recycling).items(|app: &App| app.items.len(), template, bind(kind))
        };
        let layout: Build<SkiaLayout> = match kind {
            Kind::WrapSplit3 | Kind::CachedCard => items(SkiaWrap::new().spacing(6).split(3).padding(8)),
            Kind::WrapFlow => items(SkiaWrap::new().spacing(6).padding(8)),
            Kind::WrapSplit4Dynamic => items(SkiaWrap::new().spacing(4).split(4).dynamic_columns(true)),
            Kind::Row => items(SkiaRow::new().spacing(6).padding(4)),
            Kind::GridSplit3Invert => items(SkiaGrid::new().split(3).invert(true).column_spacing(4).row_spacing(4)),
            Kind::ColumnSplit2 => items(SkiaStack::new().split(2).spacing(5)),
            Kind::DecoratedSplit4 => {
                let grid = SkiaDecoratedGrid::new().split(4).column_spacing(6).row_spacing(6).recycling_template(recycling);
                let grid = grid.items(|app: &App| app.items.len(), template, bind(kind));
                app.layout = Some(grid.id());
                let scroll = SkiaScroll::new().fill().content(grid);
                app.scroll = Some(scroll.id());
                return scroll;
            }
        };
        app.layout = Some(layout.id());
        let scroll = match kind {
            Kind::Row => SkiaScroll::new().fill().orientation(ScrollOrientation::Horizontal).content(layout),
            Kind::CachedCard => SkiaScroll::new().fill().content(SkiaStack::new().spacing(0).children((
                SkiaLayout::new().fill_x().height_request(250).background_color(Color::from_rgb(20, 40, 60)),
                SkiaShape::new().fill_x().padding(6).background_color(Color::from_rgb(60, 60, 60)).use_cache(CacheType::Image).children(layout),
                SkiaLayout::new().fill_x().height_request(400),
            ))),
            _ => SkiaScroll::new().fill().content(layout),
        };
        app.scroll = Some(scroll.id());
        scroll
    };
    let app = App { items: (0..count).collect(), text: true, fill_every, ..App::default() };
    let ui = Ui::new(app, build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, (300.0 * scale) as i32, (400.0 * scale) as i32, scale);
    host.settle();
    host
}

fn layout_of(host: &Headless<App>) -> &SkiaLayout {
    host.ui.tree.find::<SkiaLayout>(host.ui.state.layout.unwrap()).unwrap()
}

fn in_use(host: &Headless<App>) -> usize {
    layout_of(host).cells_in_use().count()
}

/// Every pixel of the last frame.
fn pixels(host: &mut Headless<App>, width: i32, height: i32) -> Vec<Color> {
    let mut all = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            all.push(host.pixel(x, y));
        }
    }
    all
}

fn scroll_to(host: &mut Headless<App>, kind: Kind, points: f32) {
    let scroll = host.ui.state.scroll.unwrap();
    let (x, y) = if kind == Kind::Row { (-points, 0.0) } else { (0.0, -points) };
    host.ui.tree.cx().scroll_to(scroll, x, y, 0);
    host.settle();
}

/// The two hosts draw the same pixels at every offset.
fn same_pixels(enabled: &mut Headless<App>, disabled: &mut Headless<App>, kind: Kind, scale: f32, step: &str) {
    let (width, height) = ((300.0 * scale) as i32, (400.0 * scale) as i32);
    for offset in [0.0, 45.0, 210.0, 777.0, 1.0e6] {
        scroll_to(enabled, kind, offset);
        scroll_to(disabled, kind, offset);
        let (a, b) = (pixels(enabled, width, height), pixels(disabled, width, height));
        let differ = a.iter().zip(&b).filter(|(a, b)| a != b).count();
        if differ > 0 {
            let at = a.iter().zip(&b).position(|(a, b)| a != b).unwrap() as i32;
            panic!("{kind:?} @{scale} {step}, offset {offset}: {differ} pixels differ, first at ({}, {})", at % width, at / width);
        }
    }
}

/// Changes both hosts the same way.
fn change(hosts: [&mut Headless<App>; 2], what: impl Fn(&mut App, &mut Cx, ControlId)) {
    for host in hosts {
        let layout = host.ui.state.layout.unwrap();
        what(&mut host.ui.state, &mut host.ui.tree.cx(), layout);
        host.ui.state_changed();
        host.settle();
    }
}

fn check(kind: Kind, scale: f32) {
    let count = if kind == Kind::Row { 40 } else { 60 };
    let mut enabled = scene(kind, RecyclingTemplate::Enabled, count, scale);
    let mut disabled = scene(kind, RecyclingTemplate::Disabled, count, scale);
    same_pixels(&mut enabled, &mut disabled, kind, scale, "as built");

    // In use: the slots that can be seen, or every item under the cached card.
    scroll_to(&mut enabled, kind, 0.0);
    let (on, all) = (in_use(&enabled), in_use(&disabled));
    println!("{kind:?} @{scale}: {on} views in use, Disabled {all}");
    assert_eq!(all, count as usize, "{kind:?}: Disabled has a view per item");
    match kind {
        Kind::CachedCard => assert_eq!(on, count as usize, "under a cache every slot draws"),
        _ => assert!(on > 0 && on < count as usize / 2, "{kind:?}: {on} views in use of {count}"),
    }

    change([&mut enabled, &mut disabled], |app, cx, layout| {
        app.items[5] = 900;
        cx.items_changed(layout, 5);
    });
    same_pixels(&mut enabled, &mut disabled, kind, scale, "after a change");
    change([&mut enabled, &mut disabled], |app, cx, layout| {
        app.items.splice(1..1, [1000, 1001, 1002]);
        cx.items_inserted(layout, 1, 3);
    });
    same_pixels(&mut enabled, &mut disabled, kind, scale, "after an insert");
    change([&mut enabled, &mut disabled], |app, cx, layout| {
        app.items.drain(4..6);
        cx.items_removed(layout, 4, 2);
    });
    same_pixels(&mut enabled, &mut disabled, kind, scale, "after a remove");
    change([&mut enabled, &mut disabled], |app, cx, layout| {
        let moved = app.items.remove(0);
        app.items.insert(7, moved);
        cx.items_moved(layout, 0, 7);
    });
    same_pixels(&mut enabled, &mut disabled, kind, scale, "after a move");
    change([&mut enabled, &mut disabled], |app, _cx, _layout| app.items.extend(2000..2010));
    same_pixels(&mut enabled, &mut disabled, kind, scale, "after an append");
    if !matches!(kind, Kind::WrapFlow | Kind::Row) {
        for host in [&mut enabled, &mut disabled] {
            let layout = host.ui.state.layout.unwrap();
            let split = host.ui.tree.find::<SkiaLayout>(layout).unwrap().p.split;
            host.ui.tree.find_mut::<SkiaLayout>(layout).unwrap().set_split(split + 1);
            host.settle();
        }
        same_pixels(&mut enabled, &mut disabled, kind, scale, "after another Split");
    }
    // The mode at runtime: the cells are made again for it.
    let layout = enabled.ui.state.layout.unwrap();
    enabled.ui.tree.find_mut::<SkiaLayout>(layout).unwrap().set_recycling_template(RecyclingTemplate::Disabled);
    enabled.settle();
    assert_eq!(in_use(&enabled), enabled.ui.state.items.len());
    same_pixels(&mut enabled, &mut disabled, kind, scale, "switched to Disabled");
    enabled.ui.tree.find_mut::<SkiaLayout>(layout).unwrap().set_recycling_template(RecyclingTemplate::Enabled);
    enabled.settle();
    same_pixels(&mut enabled, &mut disabled, kind, scale, "switched back to Enabled");
}

#[test]
fn wrap_split_3_draws_as_disabled() {
    check(Kind::WrapSplit3, 1.0);
    check(Kind::WrapSplit3, 2.0);
}

#[test]
fn wrap_flow_draws_as_disabled() {
    check(Kind::WrapFlow, 1.0);
}

#[test]
fn wrap_split_4_dynamic_columns_draws_as_disabled() {
    check(Kind::WrapSplit4Dynamic, 1.0);
}

#[test]
fn row_draws_as_disabled() {
    check(Kind::Row, 1.0);
}

#[test]
fn grid_split_3_invert_draws_as_disabled() {
    check(Kind::GridSplit3Invert, 1.0);
}

#[test]
fn decorated_grid_split_4_draws_as_disabled() {
    check(Kind::DecoratedSplit4, 1.0);
}

#[test]
fn column_split_2_draws_as_disabled() {
    check(Kind::ColumnSplit2, 1.0);
}

#[test]
fn a_wrap_in_a_cached_card_draws_every_slot() {
    check(Kind::CachedCard, 1.0);
    check(Kind::CachedCard, 2.0);
}

/// WrapTemplatedSplitTests.EveryCellArrangedAndDrawn (`DrawnUi.Net.Tests`, 6 of 6): ten chips in a
/// templated wrap, alone, in a card and in a card in a scroll, each with a cell arranged and drawn
/// (C# drew none with Enabled before 3d7bd78f: its size key was 0 for a Wrap).
#[test]
fn every_cell_arranged_and_drawn() {
    #[derive(Clone, Copy, Debug)]
    enum Host {
        Alone,
        Card,
        ScrollCard,
    }
    let cases = [
        (Host::Alone, 3, RecyclingTemplate::Enabled),
        (Host::Card, 3, RecyclingTemplate::Enabled),
        (Host::ScrollCard, 3, RecyclingTemplate::Enabled),
        (Host::ScrollCard, 3, RecyclingTemplate::Disabled),
        (Host::ScrollCard, 0, RecyclingTemplate::Enabled),
        (Host::ScrollCard, 2, RecyclingTemplate::Enabled),
    ];
    for (host_kind, split, recycling) in cases {
        let build = move |app: &mut App| {
            let chip = || {
                let mut label = Handle::default();
                let cell = SkiaLayout::new().fill_x().children(
                    SkiaShape::new()
                        .corner_radius(10)
                        .background_color(Color::from_rgb(0x46, 0x82, 0xB4))
                        .fill_x()
                        .children(SkiaLabel::new("").font_size(13).padding((12, 8)).horizontal_options(LayoutOptions::Center).assign(&mut label)),
                );
                (cell, label)
            };
            let wrap = SkiaWrap::new().spacing(8).split(split).recycling_template(recycling).items(
                |app: &App| app.items.len(),
                chip,
                |label: &Handle<SkiaLabel>, app: &App, index, cx| {
                    if let Some(mut label) = cx.get_mut(*label) {
                        label.set_text(format!("Item {}", app.items[index]));
                    }
                },
            );
            app.layout = Some(wrap.id());
            let card = |content: Build<SkiaLayout>| {
                SkiaShape::new()
                    .corner_radius(8)
                    .background_color(Color::from_rgb(0x2F, 0x4F, 0x4F))
                    .fill_x()
                    .children(SkiaStack::new().padding((16, 12)).spacing(10).children((content,)))
            };
            let content: drawnui::Detached = match host_kind {
                Host::Alone => wrap.into(),
                Host::Card => card(wrap).into(),
                Host::ScrollCard => SkiaScroll::new().fill_y().content(SkiaStack::new().children((card(wrap),))).into(),
            };
            SkiaLayer::new().fill_y().children((content,))
        };
        let ui = Ui::new(App { items: (1..=10).collect(), ..App::default() }, build).font_bytes("Default", FONT).background(Color::BLACK);
        let mut host = Headless::new(ui, 800, 600, 1.0);
        for _ in 0..6 {
            host.frame_after(16.0);
        }
        let case = format!("{host_kind:?} split={split} {recycling:?}");
        assert_eq!(in_use(&host), 10, "{case}");
        for index in 0..10 {
            let cell = layout_of(&host).cell_in_use(index).unwrap_or_else(|| panic!("{case}: item {index} has a cell"));
            let rect = host.rect(cell);
            assert!(rect.width() >= 1.0 && rect.height() >= 1.0, "{case}: item {index} at {rect:?}");
        }
    }
}

/// The layout reads what the template decided for an item (here: fill the rest of the line) before
/// it measures the item. A slot learns it when it is measured; the next slot is laid out with it
/// first, and when an item decided otherwise the layout lays its children out once more. Opening
/// binds every item once to measure it, and the items that can be seen once more to draw them.
#[test]
fn decisions_per_item_are_laid_out_as_disabled_lays_them_out() {
    for fill_every in [1, 5] {
        let mut enabled = scene_with(Kind::WrapFlow, RecyclingTemplate::Enabled, 60, 1.0, fill_every);
        let mut disabled = scene_with(Kind::WrapFlow, RecyclingTemplate::Disabled, 60, 1.0, fill_every);
        let (a, b) = (pixels(&mut enabled, 300, 400), pixels(&mut disabled, 300, 400));
        assert!(a == b, "every {fill_every}: {} pixels differ", a.iter().zip(&b).filter(|(a, b)| a != b).count());
        if fill_every == 1 {
            assert_eq!(enabled.ui.state.binds.get() as usize, 60 + in_use(&enabled), "one bind per item, one per view drawn");
        }
    }
}

/// A view on screen that changes its size by itself (a cell that expands) moves the items after
/// it, as a view of its own does.
#[test]
fn a_view_that_changes_its_size_moves_the_slots_after_it() {
    let mut enabled = scene(Kind::WrapSplit3, RecyclingTemplate::Enabled, 60, 1.0);
    let mut disabled = scene(Kind::WrapSplit3, RecyclingTemplate::Disabled, 60, 1.0);
    for host in [&mut enabled, &mut disabled] {
        let cell = layout_of(host).cell_in_use(2).expect("item 2 is on screen");
        host.ui.tree.find_mut::<SkiaShape>(cell).unwrap().set_height_request(140);
        host.settle();
    }
    let (a, b) = (pixels(&mut enabled, 300, 400), pixels(&mut disabled, 300, 400));
    assert!(a == b, "{} pixels differ", a.iter().zip(&b).filter(|(a, b)| a != b).count());
}

/// Taps reach the view of the item drawn where the pointer is, under a cached card whose drawing
/// pass is skipped (its cache is blitted), also after the slots moved.
#[test]
fn taps_hit_the_item_drawn_there_under_a_cached_card() {
    let mut host = scene(Kind::CachedCard, RecyclingTemplate::Enabled, 60, 1.0);
    scroll_to(&mut host, Kind::CachedCard, 180.0);
    let layout = host.ui.state.layout.unwrap();
    let tap_item = |host: &mut Headless<App>, index: usize| {
        let rect = host.ui.tree.cx().item_rect(layout, index).expect("every slot has a view under the cache");
        host.ui.state.taps.clear();
        host.tap(rect.center_x(), rect.center_y());
        assert_eq!(host.ui.state.taps, [index], "tap on item {index} at {rect:?}");
    };
    for index in [0, 4, 8] {
        tap_item(&mut host, index);
    }
    // Slots move (two items come in at the start): the views follow without a drawing pass of
    // the card in between.
    host.ui.state.items.splice(0..0, [3000, 3001]);
    host.ui.tree.cx().items_inserted(layout, 0, 2);
    host.ui.state_changed();
    host.settle();
    for index in [0, 2, 7] {
        tap_item(&mut host, index);
    }
}

/// A 1000-item Split 3 wrap in the test-only viewport (no text: what the engine allocates).
fn wide(recycling: RecyclingTemplate) -> (Headless<App>, Handle<Viewport>) {
    let mut viewport = Handle::default();
    let build = |app: &mut App| {
        let wrap = SkiaWrap::new().spacing(6).split(3).padding(8).recycling_template(recycling);
        let wrap = wrap.items(|app: &App| app.items.len(), template, bind(Kind::WrapSplit3));
        app.layout = Some(wrap.id());
        Viewport::new(false).width_request(300).height_request(400).is_clipped_to_bounds(true).assign(&mut viewport).children((wrap,))
    };
    let ui = Ui::new(App { items: (0..1000).collect(), ..App::default() }, build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    (host, viewport)
}

#[test]
fn a_scrolled_frame_allocates_nothing_while_the_same_slots_show() {
    let (mut host, viewport) = wide(RecyclingTemplate::Enabled);
    viewport_to(&mut host, viewport, 500.0);
    host.settle();
    let before = allocations();
    // Frames 16 ms apart, as a host draws them (the frame statistics keep the last second).
    for frame in 1..=300 {
        viewport_to(&mut host, viewport, 500.0 + (frame % 2) as f32 * 10.0);
        host.frame_after(16.0);
    }
    let idle = allocations() - before;
    println!("300 scrolled frames, no slot entering: {idle} allocations");
    assert_eq!(idle, 0);
    // Frames where slots enter and leave: a view goes from the pool into a slot and back.
    let (first, before) = (layout_of(&host).visible_items(), allocations());
    for frame in 1..=300 {
        viewport_to(&mut host, viewport, 500.0 + frame as f32 * 7.0);
        host.frame_after(16.0);
    }
    let entering = allocations() - before;
    println!("300 scrolled frames, 2100 points: {entering} allocations");
    assert_ne!(layout_of(&host).visible_items(), first, "slots entered");
    assert_eq!(entering, 0);
}

/// A measurement, not a test: a 1000-item Split 3 wrap with text, Enabled against Disabled: views,
/// the first frame, and a scrolled frame. DrawnUi.React for scale: 27 views vs 1000, first frame
/// 18-24 vs 88-91 ms, scroll frame 4-5 vs 9-10 ms.
/// `cargo test --release -p drawnui --test layout_recycled -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn recycled_wrap_cost() {
    const RUNS: usize = 7;
    const FRAMES: u32 = 300;
    for recycling in [RecyclingTemplate::Enabled, RecyclingTemplate::Disabled] {
        let (mut firsts, mut scrolls, mut line, mut nodes) = (Vec::new(), Vec::new(), String::new(), 0);
        for _ in 0..RUNS {
            let mut viewport = Handle::default();
            let build = |app: &mut App| {
                let wrap = SkiaWrap::new().spacing(6).split(3).padding(8).recycling_template(recycling);
                let wrap = wrap.items(|app: &App| app.items.len(), template, bind(Kind::WrapSplit3));
                app.layout = Some(wrap.id());
                Viewport::new(false).width_request(300).height_request(400).is_clipped_to_bounds(true).assign(&mut viewport).children((wrap,))
            };
            let ui = Ui::new(App { items: (0..1000).collect(), text: true, ..App::default() }, build).font_bytes("Default", FONT).background(Color::BLACK);
            let mut host = Headless::new(ui, 300, 400, 1.0);
            let started = std::time::Instant::now();
            host.frame();
            firsts.push(started.elapsed().as_secs_f64() * 1000.0);
            host.settle();
            line = layout_of(&host).debug_string();
            nodes = count_nodes(&host, host.ui.tree.root().unwrap());
            let started = std::time::Instant::now();
            for frame in 1..=FRAMES {
                viewport_to(&mut host, viewport, frame as f32 * 7.0);
                host.frame_after(16.0);
            }
            scrolls.push(started.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64);
        }
        let median = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        };
        let (first, scroll) = (median(&mut firsts), median(&mut scrolls));
        println!("{recycling:?}: {line}; {nodes} nodes; first frame {first:.1} ms, scrolled frame {scroll:.2} ms (medians of {RUNS})");
    }
}

fn count_nodes(host: &Headless<App>, id: ControlId) -> usize {
    1 + host.ui.tree.children(id).iter().map(|child| count_nodes(host, *child)).sum::<usize>()
}

#[test]
fn an_append_binds_only_the_new_items() {
    // 12 items appended to 170 (a partial last row). Wrap and Column bind the new items once. A
    // Grid measures an Auto row's cell twice (at the grid's width, then at its column), so its new
    // items bind twice. An inverted Grid moves every item to another row: before, a new height
    // offer measured every slot again (141 binds); a view that fits and does not fill its height
    // keeps its size now (35).
    for (kind, most) in [(Kind::WrapSplit3, 12), (Kind::ColumnSplit2, 12), (Kind::DecoratedSplit4, 24), (Kind::GridSplit3Invert, 40)] {
        let mut enabled = scene(kind, RecyclingTemplate::Enabled, 170, 1.0);
        let mut disabled = scene(kind, RecyclingTemplate::Disabled, 170, 1.0);
        enabled.ui.state.binds.set(0);
        change([&mut enabled, &mut disabled], |app, cx, layout| {
            app.items.extend(170..182);
            cx.items_inserted(layout, 170, 12);
        });
        let binds = enabled.ui.state.binds.get();
        assert!(binds <= most, "{kind:?}: {binds} binds for 12 new items");
        same_pixels(&mut enabled, &mut disabled, kind, 1.0, "after an append");
    }
}
