//! Pixel snapping: the drawing rect and the measured size of every control of a set of trees, at
//! scales 1, 1.25, 1.5, 2, 2.625 and 3, equal to what the C# engine gives for the same tree on the
//! same surface. The C# numbers are in `layout_pixels_upstream.rs`, dumped by a probe on
//! DrawnUi.Net's headless host: one line per control, `path left top right bottom width height`,
//! the path being the child indices from the root. The lists have a line for the scroll, one for
//! the list and one per cell in the viewport, by item index.

use drawnui::controls::layout::{MeasuringStrategy, SkiaWrap};
use drawnui::prelude::*;
use drawnui::testing::Headless;

#[path = "layout_pixels_upstream.rs"]
mod upstream;

const S: LayoutOptions = LayoutOptions::Start;
const C: LayoutOptions = LayoutOptions::Center;
const E: LayoutOptions = LayoutOptions::End;
const F: LayoutOptions = LayoutOptions::Fill;

type Layout = Build<SkiaLayout>;

/// A bare control; a negative size is not set.
fn b(width: f32, height: f32) -> Layout {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// A bare control with its options.
fn bo(width: f32, height: f32, horizontal: LayoutOptions, vertical: LayoutOptions) -> Layout {
    b(width, height).horizontal_options(horizontal).vertical_options(vertical)
}

fn layout(kind: LayoutType, horizontal: LayoutOptions, vertical: LayoutOptions, spacing: f32) -> Layout {
    SkiaLayout::new().layout_type(kind).horizontal_options(horizontal).vertical_options(vertical).spacing(spacing)
}

fn grid(columns: &str, rows: &str, horizontal: LayoutOptions, vertical: LayoutOptions, spacing: (f32, f32)) -> Layout {
    let grid = layout(LayoutType::Grid, horizontal, vertical, 8.0).column_spacing(spacing.0).row_spacing(spacing.1);
    let grid = if columns.is_empty() { grid } else { grid.column_definitions(columns) };
    if rows.is_empty() { grid } else { grid.row_definitions(rows) }
}

fn absolute() -> Layout {
    layout(LayoutType::Absolute, F, F, 0.0).padding(7).children((
        bo(33.0, 21.0, S, S).margin((3, 5, 2, 4)),
        bo(41.0, 17.0, C, C).margin((0, 0, 5, 0)),
        bo(25.0, 13.0, E, E).margin(3),
        bo(-1.0, -1.0, F, F).margin(2.5),
        layout(LayoutType::Absolute, S, S, 0.0).padding(2).margin((11, 40, 0, 0)).children((b(30.0, 10.0), bo(-1.0, -1.0, F, F))),
        bo(20.0, 20.0, C, S),
        bo(21.0, 21.0, E, C),
        bo(19.0, 19.0, S, E).margin((1, 0, 0, 1)),
        layout(LayoutType::Absolute, F, S, 0.0)
            .padding((3.0, 1.5, 2.0, 0.5))
            .margin((0, 90, 0, 0))
            .height_request(31.3)
            .children((bo(-1.0, -1.0, F, F), bo(10.0, 10.0, C, C))),
    ))
}

fn column() -> Layout {
    layout(LayoutType::Column, F, F, 3.0).padding((3, 4, 5, 6)).children((
        b(40.0, 9.0),
        b(-1.0, 11.0).fill_x(),
        b(33.0, 7.0).horizontal_options(C),
        b(27.0, 5.0).horizontal_options(E),
        b(-1.0, 7.0).fill_x().margin((2, 1, 4, 2)),
        layout(LayoutType::Absolute, F, S, 0.0).padding(3).children(b(30.0, 9.0).horizontal_options(C)),
        b(50.0, 7.5),
        layout(LayoutType::Row, S, S, 3.0).children((
            b(6.0, 6.0),
            b(8.0, 10.0).vertical_options(C),
            b(5.0, 5.0).vertical_options(E),
        )),
    ))
}

fn row_fill() -> Layout {
    layout(LayoutType::Row, F, F, 3.0).padding(2).children((
        b(40.0, 30.0),
        b(-1.0, 20.0).fill_x(),
        bo(-1.0, -1.0, F, F),
        bo(-1.0, 25.0, F, C),
        b(20.0, -1.0).fill_y(),
    ))
}

fn column_fill() -> Layout {
    layout(LayoutType::Column, F, F, 7.0).children((
        b(-1.0, 30.0).fill_x(),
        b(50.0, -1.0).fill_y(),
        bo(-1.0, -1.0, F, F),
        bo(30.0, -1.0, E, F),
        b(20.0, 20.0),
    ))
}

fn wrap() -> Layout {
    let items: Vec<Layout> = (0..11)
        .map(|i| match i {
            4 => b(47.0, 11.0).vertical_options(C),
            7 => b(-1.0, 19.0).fill_x(),
            _ => b(47.0, 19.0),
        })
        .collect();
    SkiaWrap::new().spacing(3).padding(4).children(items)
}

fn wrap_split() -> Layout {
    let items: Vec<Layout> =
        (0..5).map(|i| layout(LayoutType::Absolute, F, S, 0.0).children(b(20.0, 10.0 + i as f32 * 3.0))).collect();
    SkiaWrap::new().spacing(5).split(3).children(items)
}

fn grid_tree() -> Layout {
    grid("Auto,*,2*,50", "Auto,*,30", F, F, (3.0, 5.0)).padding(5).children((
        b(37.0, 15.0),
        bo(-1.0, -1.0, F, F).column(1),
        bo(20.0, 20.0, C, C).column(2),
        b(-1.0, 10.0).fill_x().column(3),
        bo(-1.0, -1.0, F, F).row(1),
        bo(-1.0, -1.0, F, F).column(1).row(1).column_span(2),
        bo(10.0, 10.0, E, E).column(3).row(1),
        bo(-1.0, -1.0, F, F).row(2).column_span(4),
    ))
}

fn grid_auto() -> Layout {
    grid("Auto,Auto", "Auto,Auto", S, S, (1.0, 1.0)).margin(3.3).children((
        b(33.0, 17.0),
        bo(21.0, 9.0, C, C).column(1),
        b(45.0, 11.0).row(1),
        bo(-1.0, -1.0, F, F).column(1).row(1),
    ))
}

fn nested() -> Layout {
    layout(LayoutType::Column, F, F, 4.0).padding(6).children((
        layout(LayoutType::Row, F, S, 5.0).children((
            b(30.0, 22.0),
            layout(LayoutType::Column, F, S, 2.0).children((b(-1.0, 11.0).fill_x(), b(-1.0, 13.0).fill_x())),
            b(25.0, 17.0).vertical_options(C),
        )),
        grid("*,*", "", F, S, (7.0, 3.0)).children((
            layout(LayoutType::Absolute, F, S, 0.0).padding(3).children(b(40.0, 21.0)),
            layout(LayoutType::Column, F, S, 1.0).column(1).children((b(-1.0, 9.0).fill_x(), b(-1.0, 9.0).fill_x())),
        )),
        SkiaWrap::new().spacing(2).children((0..5).map(|_| b(45.0, 9.0)).collect::<Vec<_>>()),
        layout(LayoutType::Absolute, F, S, 0.0).height_request(24).children((bo(11.0, 11.0, C, C), bo(7.0, 7.0, E, E).margin(1))),
    ))
}

fn center_odd() -> Layout {
    layout(LayoutType::Absolute, F, F, 0.0).children((
        bo(13.0, 10.0, C, C),
        bo(14.0, 11.0, C, C).margin((0, 0, 12, 0)),
        layout(LayoutType::Column, F, S, 0.0)
            .margin((0, 30, 0, 0))
            .children((b(13.0, 10.0).horizontal_options(C), b(50.0, 13.0).horizontal_options(C))),
        layout(LayoutType::Row, S, S, 0.0).margin((0, 60, 0, 0)).children((b(20.0, 31.0), b(20.0, 10.0).vertical_options(C))),
        grid("32,33", "28,27", S, S, (0.0, 0.0))
            .margin((70, 60, 0, 0))
            .children((bo(13.0, 10.0, C, C), bo(13.0, 10.0, C, C).column(1).row(1))),
    ))
}

fn min_max() -> Layout {
    let panel = |width: f32, height: f32| layout(LayoutType::Absolute, S, S, 0.0).children(b(width, height));
    layout(LayoutType::Column, F, F, 0.0).children((
        panel(30.0, 10.0).minimum_width_request(100).minimum_height_request(41),
        b(-1.0, 20.0).fill_x().maximum_width_request(77),
        panel(20.0, 40.0).maximum_height_request(15.5),
        panel(30.0, 10.0).minimum_height_request(41).horizontal_options(C),
        b(10.0, 10.0),
    ))
}

/// Sizes in points that are between pixels at most scales.
const SEAM_SIZES: [f32; 8] = [23.0, 17.5, 9.3, 31.7, 11.0, 20.2, 13.9, 7.5];

/// Children of a stack side by side with no spacing.
fn seam_column() -> Layout {
    let children: Vec<Layout> = SEAM_SIZES.iter().map(|size| b(-1.0, *size).fill_x()).collect();
    layout(LayoutType::Column, F, F, 0.0).padding(2.5).margin(1.3).children(children)
}

fn seam_row() -> Layout {
    let children: Vec<Layout> = SEAM_SIZES.iter().map(|size| b(*size, -1.0).fill_y()).collect();
    layout(LayoutType::Row, F, F, 0.0).padding(2.5).margin(1.3).children(children)
}

/// A shape; a negative size is not set, a stroke of 0 is none.
fn shape(width: f32, height: f32, stroke: f32) -> Build<SkiaShape> {
    let shape = SkiaShape::new().width_request(width).height_request(height);
    if stroke != 0.0 { shape.stroke_width(stroke).stroke_color(Color::WHITE) } else { shape }
}

/// Shapes as parents: plain, stroked, stroked with padding, sized by its content.
fn shape_parent() -> Layout {
    let filling = || bo(-1.0, -1.0, F, F);
    let centered = |shape: Build<SkiaShape>| shape.horizontal_options(C).vertical_options(C);
    layout(LayoutType::Absolute, F, F, 0.0).children((
        shape(70.0, 51.0, 0.0).margin((5, 5, 0, 0)).corner_radius(8).children(centered(shape(33.0, 20.0, 0.0))),
        shape(71.0, 50.0, 2.0)
            .margin((80, 5, 0, 0))
            .children((filling(), centered(shape(40.0, 21.0, 3.0)).children(filling()))),
        shape(90.5, 60.0, 1.0).margin((5, 62, 0, 0)).padding(3.3).children((
            filling(),
            bo(20.5, 10.0, E, E),
            centered(shape(50.0, 30.0, 8.0)).children(filling()),
        )),
        shape(-1.0, -1.0, 2.5).margin((100, 62, 0, 0)).children(b(30.0, 20.0)),
    ))
}

const TREES: [(&str, fn() -> Layout); 14] = [
    ("absolute", absolute),
    ("column", column),
    ("row_fill", row_fill),
    ("column_fill", column_fill),
    ("wrap", wrap),
    ("wrap_split", wrap_split),
    ("grid", grid_tree),
    ("grid_auto", grid_auto),
    ("nested", nested),
    ("center_odd", center_odd),
    ("min_max", min_max),
    ("seam_column", seam_column),
    ("seam_row", seam_row),
    ("shape_parent", shape_parent),
];

fn number(value: f32) -> String {
    if value == value.round() { format!("{}", value as i64) } else { format!("{value:.3}").trim_end_matches('0').to_owned() }
}

fn dump<S>(host: &Headless<S>, id: ControlId, path: String, out: &mut String) {
    let base = host.ui.tree.base(id).unwrap();
    let (r, m) = (base.rect, base.measured);
    let values = [r.left, r.top, r.right, r.bottom, m.width, m.height].map(number).join(" ");
    out.push_str(&format!("{path} {values}\n"));
    for (index, child) in host.ui.tree.children(id).iter().enumerate() {
        dump(host, *child, format!("{path}.{index}"), out);
    }
}

/// The runs upstream dumped for one tree: the scale its host rendered at, the canvas in pixels and
/// the lines. The host takes its canvas in points and derives the scale back from it, so on the
/// odd 487 x 391 surface its 2.625 and 3 are really 2.6357803 and 3.0123205 on 489 x 393; the
/// 630 x 420 surface is whole points at every scale and gives the exact 2.625 and 3.
fn runs(name: &str) -> Vec<(f32, (i32, i32), &'static str)> {
    let header = format!("{name} @");
    let sections = upstream::DUMP.split("## ").filter_map(|section| section.strip_prefix(&header));
    sections
        .map(|section| {
            let (head, lines) = section.split_once('\n').unwrap();
            let values: Vec<&str> = head.split(' ').collect();
            (values[0].parse().unwrap(), (values[1].parse().unwrap(), values[2].parse().unwrap()), lines)
        })
        .collect()
}

/// Lays the tree out in every run and returns the lines that differ from upstream.
fn differences(name: &str, build: fn() -> Layout) -> Vec<String> {
    let mut differences = Vec::new();
    let runs = runs(name);
    assert_eq!(runs.len(), 8, "{name}: runs in the upstream dump");
    for (scale, canvas, expected) in runs {
        let mut host = Headless::new(Ui::new((), |_| build()), canvas.0, canvas.1, scale);
        host.settle();
        let mut actual = String::new();
        dump(&host, host.ui.tree.root().unwrap(), "0".to_owned(), &mut actual);
        for (ours, theirs) in actual.lines().zip(expected.lines()) {
            if ours != theirs {
                differences.push(format!("{name} @{scale}: {ours}   upstream: {theirs}"));
            }
        }
        assert_eq!(actual.lines().count(), expected.lines().count(), "{name} @{scale}: another tree than upstream");
    }
    differences
}

/// Where upstream is not what its own rules give. In its first frame a grid measures its children
/// at the scale a control has before it is attached (1) and sizes the tracks from that; the
/// measure of the second frame is within upstream's 1 px tolerance of the first and is dropped.
/// So this child keeps a size that is 1 px off its cell, at the two scales the host drifted to.
/// Here it is measured for its cell.
const UPSTREAM_STALE: [&str; 2] = [
    "grid @2.6357803: 0.5 119 79 336 288 217 209   upstream: 0.5 119 79 337 288 218 209",
    "grid @3.0123205: 0.5 135 90 314 273 179 183   upstream: 0.5 135 90 314 272 179 182",
];

/// Changed, React rule (Nick, 2026-10-02: React wins): in a wrap a Fill child is measured with the
/// whole line (DrawnUi.React MeasureWrap), so after other children it starts a line of its own.
/// C# 1.9.7.4 flex-fill gave it the rest of its line. In the `wrap` tree child 7 drops to a line
/// of its own, and the wrap and the children after it move down.
const REACT_WRAP_FILL_RULE: [&str; 40] = [
    "wrap @1: 0 0 0 487 71 487 71   upstream: 0 0 0 487 49 487 49",
    "wrap @1: 0.7 4 26 483 45 479 19   upstream: 0.7 354 4 483 23 129 19",
    "wrap @1: 0.8 4 48 51 67 47 19   upstream: 0.8 4 26 51 45 47 19",
    "wrap @1: 0.9 54 48 101 67 47 19   upstream: 0.9 54 26 101 45 47 19",
    "wrap @1: 0.10 104 48 151 67 47 19   upstream: 0.10 104 26 151 45 47 19",
    "wrap @1.25: 0 0 0 487 90 487 90   upstream: 0 0 0 487 62 487 62",
    "wrap @1.25: 0.7 5 33 482 57 477 24   upstream: 0.7 446 5 482 29 36 24",
    "wrap @1.25: 0.8 5 61 64 85 59 24   upstream: 0.8 5 33 64 57 59 24",
    "wrap @1.25: 0.9 68 61 127 85 59 24   upstream: 0.9 68 33 127 57 59 24",
    "wrap @1.25: 0.10 131 61 190 85 59 24   upstream: 0.10 131 33 190 57 59 24",
    "wrap @1.5: 0 0 0 487 136 487 136   upstream: 0 0 0 487 104 487 104",
    "wrap @1.5: 0.7 6 70 481 98 475 28   upstream: 0.7 80 38 481 66 401 28",
    "wrap @1.5: 0.8 6 102 76 130 70 28   upstream: 0.8 6 70 76 98 70 28",
    "wrap @1.5: 0.9 80 102 150 130 70 28   upstream: 0.9 80 70 150 98 70 28",
    "wrap @1.5: 0.10 154 102 224 130 70 28   upstream: 0.10 154 70 224 98 70 28",
    "wrap @2: 0 0 0 487 186 487 186   upstream: 0 0 0 487 142 487 142",
    "wrap @2: 0.7 8 96 479 134 471 38   upstream: 0.7 308 52 479 90 171 38",
    "wrap @2: 0.8 8 140 102 178 94 38   upstream: 0.8 8 96 102 134 94 38",
    "wrap @2: 0.9 108 140 202 178 94 38   upstream: 0.9 108 96 202 134 94 38",
    "wrap @2: 0.10 208 140 302 178 94 38   upstream: 0.10 208 96 302 134 94 38",
    "wrap @2.6357803: 0 0 0 489 304 489 304   upstream: 0 0 0 489 246 489 246",
    "wrap @2.6357803: 0.7 11 185 479 235 468 50   upstream: 0.7 143 127 479 177 336 50",
    "wrap @2.6357803: 0.8 11 243 135 293 124 50   upstream: 0.8 11 185 135 235 124 50",
    "wrap @2.6357803: 0.9 143 243 267 293 124 50   upstream: 0.9 143 185 267 235 124 50",
    "wrap @2.6357803: 0.10 275 243 399 293 124 50   upstream: 0.10 275 185 399 235 124 50",
    "wrap @3.0123205: 0 0 0 489 345 489 345   upstream: 0 0 0 489 279 489 279",
    "wrap @3.0123205: 0.7 12 210 477 267 465 57   upstream: 0.7 163 144 477 201 314 57",
    "wrap @3.0123205: 0.8 12 276 154 333 142 57   upstream: 0.8 12 210 154 267 142 57",
    "wrap @3.0123205: 0.9 163 276 305 333 142 57   upstream: 0.9 163 210 305 267 142 57",
    "wrap @3.0123205: 0.10 314 276 456 333 142 57   upstream: 0.10 314 210 456 267 142 57",
    "wrap @2.625: 0 0 0 630 244 630 244   upstream: 0 0 0 630 186 630 186",
    "wrap @2.625: 0.7 10 126 619 176 609 50   upstream: 0.7 403 68 619 118 216 50",
    "wrap @2.625: 0.8 10 184 133 234 123 50   upstream: 0.8 10 126 133 176 123 50",
    "wrap @2.625: 0.9 141 184 264 234 123 50   upstream: 0.9 141 126 264 176 123 50",
    "wrap @2.625: 0.10 272 184 395 234 123 50   upstream: 0.10 272 126 395 176 123 50",
    "wrap @3: 0 0 0 630 279 630 279   upstream: 0 0 0 630 213 630 213",
    "wrap @3: 0.7 12 144 618 201 606 57   upstream: 0.7 462 78 618 135 156 57",
    "wrap @3: 0.8 12 210 153 267 141 57   upstream: 0.8 12 144 153 201 141 57",
    "wrap @3: 0.9 162 210 303 267 141 57   upstream: 0.9 162 144 303 201 141 57",
    "wrap @3: 0.10 312 210 453 267 141 57   upstream: 0.10 312 144 453 201 141 57",
];

/// Changed, React rule (a stack measures its children unbounded along it, DrawnUi.React
/// SkiaLayout.MeasureAbsolute): the last children of `seam_column` run past the end of the column
/// at the two largest scales. C# measures them with what is left (31 and 17 px, the last one
/// empty); here they keep their own size and the column's box cuts their rects when they are
/// arranged (the C# arrange is kept there; React draws them past the end).
const REACT_STACK_RULE: [&str; 3] = [
    "seam_column @3.0123205: 0.6 12 351 478 382 466 42   upstream: 0.6 12 351 478 382 466 31",
    "seam_column @3.0123205: 0.7 12 393 478 393 466 23   upstream: 0.7 0 0 0 0 0 0",
    "seam_column @3: 0.7 12 392 618 408 607 22   upstream: 0.7 12 392 618 408 607 17",
];

/// A known difference, kept: a shape measures its children once, in the box inside the stroke.
/// Upstream measures them in the whole box and a Fill child again at arrange, for the box it is
/// drawn in, when that differs by more than a pixel; so with the odd Center pixel or a rounded
/// side its measured size is 1 px larger than here (once even than its own rect). Every rect is
/// equal; this many `shape_parent` lines differ in the measured size of a Fill child of a
/// stroked shape. A change of the rule in `shape.rs` moves the number and fails here.
const SHAPE_FILL_CHILD_MEASURED_SIZE_LINES: usize = 18;

#[test]
fn every_rect_and_size_is_the_upstream_one_at_every_scale() {
    let all: Vec<String> = TREES.iter().flat_map(|(name, build)| differences(name, *build)).collect();
    let (shape, differences): (Vec<String>, Vec<String>) = all.into_iter().partition(|line| line.starts_with("shape_parent"));
    let known: Vec<&str> = REACT_WRAP_FILL_RULE.iter().chain(&UPSTREAM_STALE).chain(&REACT_STACK_RULE).copied().collect();
    assert_eq!(differences, known, "{} differences\n{}", differences.len(), differences.join("\n"));
    assert_eq!(shape.len(), SHAPE_FILL_CHILD_MEASURED_SIZE_LINES, "the shape measures its children in another box now\n{}", shape.join("\n"));
    for line in &shape {
        let (ours, theirs) = line.split_once(": ").unwrap().1.split_once("   upstream: ").unwrap();
        let rect = |line: &str| line.split(' ').take(5).collect::<Vec<_>>().join(" ");
        assert_eq!(rect(ours), rect(theirs), "{line}");
    }
}

/// The card of the HelloMaui ShapesPage: a 150 x 110 shape at (20, 20), a 100 x 60 shape centered
/// in it. Returns the rects of both and the measured size of the child.
fn card(scale: f32, canvas: (i32, i32)) -> (Rect, Rect, Size) {
    let child = SkiaShape::new().width_request(100).height_request(60).horizontal_options(C).vertical_options(C);
    let child_id = child.id();
    let card = SkiaShape::new().margin((20, 20, 0, 0)).width_request(150).height_request(110).corner_radius(8);
    let card = card.children(child);
    let card_id = card.id();
    let mut host = Headless::new(Ui::new((), |_| SkiaLayout::new().fill().children(card)), canvas.0, canvas.1, scale);
    host.settle();
    (host.rect(card_id), host.rect(child_id), host.ui.tree.base(child_id).unwrap().measured)
}

/// At scale 1 the child is 100 x 60 in both engines. A C# probe on a 190 x 150 headless host saw
/// 102 x 61: that host renders at 1.0105263, not 1 (its canvas is the surface plus Skia's 1 px clip
/// outset on each side, 192 x 152, and it takes the scale back from that when the surface is under
/// 200 points wide). The same rules give the same numbers here at that scale.
#[test]
fn a_child_centered_in_a_shape_keeps_its_size_at_scale_1() {
    let ltrb = |l: i32, t: i32, r: i32, b: i32| Rect::new(l as f32, t as f32, r as f32, b as f32);
    assert_eq!(card(1.0, (487, 391)), (ltrb(20, 20, 170, 130), ltrb(45, 45, 145, 105), Size::new(100.0, 60.0)));
    assert_eq!(card(1.0105263, (192, 152)), (ltrb(20, 20, 172, 131), ltrb(45, 45, 147, 106), Size::new(101.0, 61.0)));
}

// ---------------------------------------------------------------- templated lists in a scroll

/// 17 to 29.5 points, in no order.
fn row_height(item: usize) -> f32 {
    17.0 + (item * 7 % 13) as f32 + if item % 3 == 0 { 0.5 } else { 0.0 }
}

/// A scroll with margins over a padded list of 60 rows: all 23 points tall (MeasureFirst), or
/// each with its own height (MeasureVisible).
fn list(uneven: bool) -> Build<SkiaScroll> {
    let list = SkiaLayout::column().spacing(3).padding((2, 5, 2, 5));
    let list = if uneven {
        list.measure_items_strategy(MeasuringStrategy::MeasureVisible).items(
            |_: &()| 60,
            || {
                let mut row = Handle::default();
                (SkiaLayout::new().fill_x().assign(&mut row), row)
            },
            |row: &Handle<SkiaLayout>, _: &(), index, cx| {
                if let Some(mut row) = cx.get_mut(*row) {
                    row.set_height_request(row_height(index));
                }
            },
        )
    } else {
        list.measure_items_strategy(MeasuringStrategy::MeasureFirst).items(
            |_: &()| 60,
            || (SkiaShape::new().fill_x().height_request(23), ()),
            |_: &(), _: &(), _, _| {},
        )
    };
    SkiaScroll::new().fill().margin((3, 2, 3, 2)).bounces(false).content(list)
}

fn line(host: &Headless<()>, name: String, id: ControlId) -> String {
    let base = host.ui.tree.base(id).unwrap();
    let (r, m) = (base.rect, base.measured);
    format!("{name} {}\n", [r.left, r.top, r.right, r.bottom, m.width, m.height].map(number).join(" "))
}

#[test]
fn lists_in_a_scroll_are_the_upstream_ones_at_every_scale() {
    let mut differences = Vec::new();
    for (name, uneven) in [("list_uniform", false), ("list_uneven", true)] {
        let runs = runs(name);
        assert_eq!(runs.len(), 8, "{name}: runs in the upstream dump");
        for (scale, canvas, expected) in runs {
            let mut host = Headless::new(Ui::new((), |_| list(uneven)), canvas.0, canvas.1, scale);
            host.settle();
            let tree = &host.ui.tree;
            let scroll = tree.root().unwrap();
            let list = tree.children(scroll)[0];
            let view = tree.base(scroll).unwrap().rect;
            let mut cells: Vec<(usize, ControlId)> = tree
                .children(list)
                .iter()
                .filter_map(|cell| Some((tree.base(*cell)?.context_index?, *cell)))
                .filter(|(_, cell)| tree.base(*cell).is_some_and(|b| b.p.is_visible && b.rect.bottom > view.top && b.rect.top < view.bottom))
                .collect();
            cells.sort_by_key(|cell| cell.0);
            let mut actual = line(&host, "scroll".to_owned(), scroll) + &line(&host, "list".to_owned(), list);
            for (index, cell) in cells {
                actual += &line(&host, format!("cell {index}"), cell);
            }
            // MeasureVisible: upstream's length of the list is an estimate for the rows nobody saw
            // yet (a fractional average), here those rows are measured ahead. It is not compared.
            let known = |line: &str| match line.strip_prefix("list ").filter(|_| uneven) {
                Some(values) => {
                    let values: Vec<&str> = values.split(' ').collect();
                    [values[0], values[1], values[2], values[4]].join(" ")
                }
                None => line.to_owned(),
            };
            for (ours, theirs) in actual.lines().zip(expected.lines()) {
                if known(ours) != known(theirs) {
                    differences.push(format!("{name} @{scale}: {ours}   upstream: {theirs}"));
                }
            }
            if actual.lines().count() != expected.lines().count() {
                differences.push(format!("{name} @{scale}: {} lines, upstream {}", actual.lines().count(), expected.lines().count()));
            }
        }
    }
    assert!(differences.is_empty(), "{} differences\n{}", differences.len(), differences.join("\n"));
}
