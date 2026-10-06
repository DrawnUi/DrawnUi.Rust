//! List tests ported from `DrawnUi.Net.Tests`, with their cases and numbers:
//! MeasureFirstAutoWidthTests (both), VariableHeightPlanesTests (both; the planes are gone
//! upstream, what they pin is kept: taps land on rows of varying height, rows are continuous),
//! ScrollToIndexSplitTests (the plain column case: where item 8 of 40 sits).

mod list_common;

use drawnui::controls::layout::{MeasureBudget, MeasuringStrategy};
use drawnui::prelude::*;
use drawnui::testing::Headless;
use list_common::*;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

// ---------------------------------------------------------------- MeasureFirstAutoWidthTests

#[derive(Default)]
struct Widths {
    items: Vec<f32>,
    stack: Handle<SkiaLayout>,
    frame: Handle<SkiaShape>,
}

/// An auto-width Column of cells that take their width from their item, inside an auto-size frame.
fn auto_width(strategy: MeasuringStrategy) -> Headless<Widths> {
    let build = move |app: &mut Widths| {
        let stack = SkiaLayout::new()
            .layout_type(LayoutType::Column)
            .spacing(0)
            .measure_items_strategy(strategy)
            .assign(&mut app.stack)
            .items(
                |app: &Widths| app.items.len(),
                || {
                    let mut cell = Handle::default();
                    (SkiaLayout::new().assign(&mut cell), cell)
                },
                |cell: &Handle<SkiaLayout>, app: &Widths, index, cx| {
                    if let Some(mut cell) = cx.get_mut(*cell) {
                        cell.set_width_request(app.items[index]);
                        cell.set_height_request(20);
                    }
                },
            );
        SkiaLayout::new().fill().children((SkiaShape::new().assign(&mut app.frame).children((stack,)),))
    };
    let mut host = Headless::new(Ui::new(Widths { items: vec![50.0], ..Widths::default() }, build), 400, 400, 1.0);
    host.settle();
    host
}

#[test]
fn auto_width_column_measure_all_grows_when_a_wider_item_is_added() {
    let mut host = auto_width(MeasuringStrategy::MeasureAll);
    let (stack, frame) = (host.ui.state.stack, host.ui.state.frame);
    assert_eq!(host.ui.tree.base(stack).unwrap().measured.width, 50.0);

    host.ui.state.items.push(200.0);
    host.ui.state_changed();
    host.settle();
    let tree = &host.ui.tree;
    let second = tree.children(stack).iter().filter_map(|c| tree.base(*c)).find(|b| b.context_index == Some(1));
    assert_eq!(second.expect("a cell for the second item").measured.width, 200.0);
    assert_eq!(tree.base(stack).unwrap().measured.width, 200.0);
    assert_eq!(host.rect(frame).width(), 200.0);
}

#[test]
fn auto_width_column_measure_first_keeps_uniform_cells_by_design() {
    let mut host = auto_width(MeasuringStrategy::MeasureFirst);
    let stack = host.ui.state.stack;
    host.ui.state.items.push(200.0);
    host.ui.state_changed();
    host.settle();
    // The appended cell is stamped with the first cell's width: the uniform contract.
    assert_eq!(host.ui.tree.base(stack).unwrap().measured.width, 50.0);
    assert_eq!(host.rect(stack).height(), 40.0);
}

// ---------------------------------------------------------------- ScrollToIndexSplitTests

#[derive(Default)]
struct Plain {
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
}

#[test]
fn scroll_to_index_lands_on_the_items_row() {
    // [InlineData(1, 40, 8, 8)]: a plain column of 40 cells of 100, item 8 sits in row 8.
    let build = |app: &mut Plain| {
        Viewport::new(false).fill().assign(&mut app.viewport).children((SkiaLayout::column()
            .spacing(0)
            .measure_items_strategy(MeasuringStrategy::MeasureFirst)
            .assign(&mut app.list)
            .items(
                |_: &Plain| 40,
                || (SkiaShape::new().fill_x().height_request(100).background_color(Color::BLUE), ()),
                |_: &(), _: &Plain, _, _| {},
            ),))
    };
    let mut host = Headless::new(Ui::new(Plain::default(), build), 300, 600, 1.0);
    host.settle();
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    // What SkiaScroll.ScrollToIndex(8, Start) scrolls by.
    let offset = layout(&host, list).item_offset_pixels(8);
    assert_eq!(offset, 800.0);
    scroll_to(&mut host, viewport, offset);
    host.settle();
    assert_eq!(rows(&host, viewport, list)[0], (8, 0.0, 100.0));
}

// ---------------------------------------------------------------- VariableHeightPlanesTests

const WORDS: [&str; 21] = [
    "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod", "tempor",
    "incididunt", "labore", "magna", "aliqua", "enim", "minim", "veniam", "quis", "nostrud",
];

/// One to three lines per item, the same for an index every time. Upstream seeds .NET `Random`
/// with the index; the sequence here is its own.
fn make_text(index: usize) -> String {
    let mut seed = index as u64 * 2_654_435_761 + 1;
    let mut next = |range: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % range
    };
    let mut text = format!("Message {index} - tap this link: <https://drawnui.net> [0]");
    for _ in 0..next(3) {
        text.push('\n');
        let words: Vec<&str> = (0..4 + next(7)).map(|_| WORDS[next(21) as usize]).collect();
        text.push_str(&words.join(" "));
    }
    text
}

#[derive(Default)]
struct Chat {
    rows: Vec<String>,
    tapped: Option<usize>,
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
}

/// The upstream scene: 300 rows of 1 to 3 lines, a rounded bubble with a label per cell.
fn chat(padding: i32, pool: i32) -> Headless<Chat> {
    let build = move |app: &mut Chat| {
        let viewport = Viewport::new(false).fill().assign(&mut app.viewport);
        SkiaLayout::new().fill().children((viewport.children((SkiaLayout::column()
            .spacing(2)
            .padding((0, padding))
            .item_template_pool_size(pool)
            .measure_items_strategy(MeasuringStrategy::MeasureVisible)
            .measure_budget(MeasureBudget::Items(20))
            .assign(&mut app.list)
            .items(
                |app: &Chat| app.rows.len(),
                || {
                    let mut label = Handle::default();
                    let bubble = SkiaShape::new()
                        .corner_radius(8)
                        .padding((18, 10))
                        .margin((8, 0))
                        .background_color(Color::from_rgb(0x2A, 0x2A, 0x2A))
                        .fill_x()
                        .children((SkiaLabel::new("").font_size(15).text_color(Color::WHITE).assign(&mut label),));
                    let cell = SkiaLayout::new()
                        .fill_x()
                        .on_tapped(|me, app: &mut Chat, _cx| app.tapped = me.base().context_index)
                        .children((bubble,));
                    (cell, label)
                },
                |label: &Handle<SkiaLabel>, app: &Chat, index, cx| {
                    if let Some(mut label) = cx.get_mut(*label) {
                        label.set_text(app.rows[index].clone());
                    }
                },
            ),)),))
    };
    let state = Chat { rows: (0..300).map(make_text).collect(), ..Chat::default() };
    let ui = Ui::new(state, build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 430, 640, 1.0);
    host.settle();
    host
}

#[test]
fn tap_hits_correct_variable_height_row() {
    let mut host = chat(0, -1);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    for _ in 0..4 {
        host.pan((215.0, 520.0), (215.0, 220.0), 90.0, 8);
    }
    assert_eq!(scrolled(&host, viewport), 1_200.0);
    let average = (0..300).map(|i| layout(&host, list).item_height_pixels(i)).sum::<f32>() / 300.0;

    // For every fully visible row, a tap on its upper and on its lower part must hit that row.
    let (mut tested, mut tall) = (0, 0);
    for (index, top, height) in rows(&host, viewport, list) {
        if top < 20.0 || top + height > 640.0 - 20.0 {
            continue;
        }
        for part in [0.2, 0.8] {
            host.ui.state.tapped = None;
            host.tap(215.0, top + height * part);
            assert_eq!(host.ui.state.tapped, Some(index), "row {index} of height {height}, tapped at {part}");
        }
        tested += 1;
        tall += (height > average * 1.3) as i32;
    }
    assert!(tested >= 3, "expected to test several live rows, tested {tested}");
    assert!(tall >= 1, "expected at least one tall row (the lower half is the regression)");
}

#[test]
fn rows_have_continuous_variable_heights() {
    let mut host = chat(8, 64);
    let (list, viewport) = (host.ui.state.list, host.ui.state.viewport);
    // Drive the list down by gesture until every row was measured.
    for _ in 0..40 {
        if layout(&host, list).measured_items() == 300 {
            break;
        }
        host.pan((215.0, 520.0), (215.0, 160.0), 90.0, 8);
    }
    let list_top = host.rect(list).top;
    let layout = layout(&host, list);
    assert_eq!(layout.measured_items(), 300);

    // No gaps, no overlaps: every row starts where the one before it ends, plus the spacing.
    let mut heights = std::collections::BTreeSet::new();
    assert_eq!(layout.item_offset_pixels(0), 8.0);
    for index in 0..299 {
        let (top, height) = (layout.item_offset_pixels(index), layout.item_height_pixels(index));
        heights.insert(height.round() as i32);
        assert!((layout.item_offset_pixels(index + 1) - (top + height + 2.0)).abs() <= 1.5, "gap after row {index}");
    }
    assert!(heights.len() >= 2, "expected variable heights but cells came out uniform");
    // And the cells on screen are where the list says their rows are.
    let content_top = list_top - scrolled(&host, viewport);
    for (index, top, height) in rows(&host, viewport, list) {
        assert_eq!((top, height), (content_top + layout.item_offset_pixels(index), layout.item_height_pixels(index)));
    }
    assert_eq!(content_height(&host, host.ui.state.list), layout.item_offset_pixels(300) - 2.0 + 8.0);
}
