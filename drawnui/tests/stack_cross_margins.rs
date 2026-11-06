//! A stack measures each child's cross axis once with its margins: the drawn rect of a child is
//! its size, its margins counted once (C# Row / Column cross-axis margins).

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    row: Vec<Handle<SkiaShape>>,
    inner: Vec<Handle<SkiaShape>>,
    column: Vec<Handle<SkiaShape>>,
    list: Handle<SkiaLayout>,
}

fn chip(outer: &mut Handle<SkiaShape>, inner: &mut Handle<SkiaShape>) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(26)
        .height_request(7.5)
        .margin((1.0, 1.0, 3.0, 3.0))
        .background_color(Color::RED)
        .assign(outer)
        .children((SkiaShape::new().fill().margin((5.0, 1.5)).background_color(Color::BLUE).assign(inner),))
}

fn host(scale: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.row = vec![Handle::default(); 2];
        app.inner = vec![Handle::default(); 2];
        app.column = vec![Handle::default(); 2];
        let (r0, r1) = app.row.split_at_mut(1);
        let (i0, i1) = app.inner.split_at_mut(1);
        let (c0, c1) = app.column.split_at_mut(1);
        SkiaLayout::column().children((
            SkiaLayout::row().spacing(3).children((chip(&mut r0[0], &mut i0[0]), chip(&mut r1[0], &mut i1[0]))),
            SkiaLayout::column().spacing(3).children((
                SkiaShape::new().width_request(26).height_request(7.5).margin((1.0, 1.0, 3.0, 3.0)).assign(&mut c0[0]),
                SkiaShape::new().height_request(7.5).horizontal_options(LayoutOptions::Fill).margin((1.0, 1.0, 3.0, 3.0)).assign(&mut c1[0]),
            )),
            SkiaLayout::column().width_request(100).spacing(3).assign(&mut app.list).items(
                |_: &App| 3,
                || (SkiaShape::new().height_request(7.5).horizontal_options(LayoutOptions::Fill).margin((1.0, 1.0, 3.0, 3.0)), ()),
                |_: &(), _: &App, _, _| {},
            ),
        ))
    });
    let mut host = Headless::new(ui, (300.0 * scale) as i32, (200.0 * scale) as i32, scale);
    host.settle();
    host
}

#[test]
fn a_row_child_counts_its_vertical_margins_once() {
    for scale in [1.0f32, 2.0] {
        let host = host(scale);
        for (i, h) in host.ui.state.row.iter().enumerate() {
            let r = host.rect(*h);
            assert!((r.height() - 7.5 * scale).abs() < 0.6, "scale {scale}, row child {i}: {r:?}");
            assert!((r.width() - 26.0 * scale).abs() < 0.6, "scale {scale}, row child {i}: {r:?}");
        }
        for (i, h) in host.ui.state.inner.iter().enumerate() {
            let r = host.rect(*h);
            // Fill inside 26 x 7.5 less margins 5 + 5 and 1.5 + 1.5.
            assert!((r.height() - 4.5 * scale).abs() < 0.6, "scale {scale}, inner {i}: {r:?}");
            assert!((r.width() - 16.0 * scale).abs() < 0.6, "scale {scale}, inner {i}: {r:?}");
        }
        // The row: 7.5 tall plus margins 1 + 3, once.
        let row = host.rect(host.ui.tree.parent(host.ui.state.row[0]).unwrap());
        assert!((row.height() - 11.5 * scale).abs() < 0.6, "scale {scale}, row: {row:?}");
    }
}

#[test]
fn a_column_child_counts_its_horizontal_margins_once() {
    for scale in [1.0f32, 2.0] {
        let host = host(scale);
        let fixed = host.rect(host.ui.state.column[0]);
        assert!((fixed.width() - 26.0 * scale).abs() < 0.6, "scale {scale}: {fixed:?}");
        let filled = host.rect(host.ui.state.column[1]);
        let column = host.rect(host.ui.tree.parent(host.ui.state.column[1]).unwrap());
        assert!((filled.width() - (column.width() - 4.0 * scale)).abs() < 0.6, "scale {scale}: {filled:?} in {column:?}");
        // Templated cells: 100 wide, margins 1 + 3.
        let list = host.ui.state.list;
        let cells = host.ui.tree.children(list);
        assert_eq!(cells.len(), 3, "scale {scale}");
        for cell in cells {
            let r = host.rect(*cell);
            assert!((r.width() - 96.0 * scale).abs() < 0.6, "scale {scale}, cell: {r:?}");
            assert!((r.height() - 7.5 * scale).abs() < 0.6, "scale {scale}, cell: {r:?}");
        }
    }
}
