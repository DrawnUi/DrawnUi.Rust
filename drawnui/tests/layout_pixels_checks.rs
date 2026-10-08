//! What whole-pixel layout is for, checked on the rendered frame, and what it must not cost:
//! cached neighbors in a stack touch without a seam, a 1 px stroke is the same pixels wherever its
//! shape sits, a layout is done in one frame and a frame after it measures nothing.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::controls::layout::SkiaWrap;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const SCALES: [f32; 6] = [1.0, 1.25, 1.5, 2.0, 2.625, 3.0];

/// Sizes in points that are between pixels at most scales.
const SIZES: [f32; 8] = [23.0, 17.5, 9.3, 31.7, 11.0, 20.2, 13.9, 7.5];

#[test]
fn image_cached_neighbors_in_a_stack_touch_without_a_seam_or_an_overlap() {
    for column in [true, false] {
        for scale in SCALES {
            let color = |index: usize| if index % 2 == 0 { Color::GREEN } else { Color::RED };
            let children: Vec<Build<SkiaLayout>> = SIZES
                .iter()
                .enumerate()
                .map(|(index, size)| {
                    let child = SkiaLayout::new().background_color(color(index)).use_cache(CacheType::Image);
                    if column { child.fill_x().height_request(*size) } else { child.fill_y().width_request(*size) }
                })
                .collect();
            let kind = if column { LayoutType::Column } else { LayoutType::Row };
            let root = SkiaLayout::new().layout_type(kind).spacing(0).fill().padding(2.5).margin(1.3).children(children);
            let mut host = Headless::new(Ui::new((), |_| root), 660, 480, scale);
            host.settle();

            let root = host.ui.tree.root().unwrap();
            let rects: Vec<Rect> = host.ui.tree.children(root).iter().map(|child| host.rect(*child)).collect();
            let along = |rect: &Rect| if column { (rect.top, rect.bottom) } else { (rect.left, rect.right) };
            for (index, pair) in rects.windows(2).enumerate() {
                let over = along(&pair[0]).1 - along(&pair[1]).0;
                // As upstream: at a scale that is not a power of two a box can be 1 px longer than
                // its measured size (see `layout::place`), and the next child is drawn over that pixel.
                let allowed = if [1.0, 1.25, 1.5, 2.0].contains(&scale) { 0.0 } else { 1.0 };
                assert!((0.0..=allowed).contains(&over), "column {column} @{scale}: child {index} ends {over} px into the next");
            }
            // One line of pixels through all of them: each pixel is the color of the child it is in
            // (of the later one where two share it).
            let (first, last) = (rects[0], rects[SIZES.len() - 1]);
            let line = if column { first.center_x() } else { first.center_y() } as i32;
            let pixel = |host: &mut Headless<()>, at: i32| if column { host.pixel(line, at) } else { host.pixel(at, line) };
            assert_eq!(pixel(&mut host, along(&first).0 as i32 - 1), Color::TRANSPARENT, "column {column} @{scale}: before the first");
            assert_eq!(pixel(&mut host, along(&last).1 as i32), Color::TRANSPARENT, "column {column} @{scale}: after the last");
            for (index, rect) in rects.iter().enumerate() {
                let (start, end) = along(rect);
                assert_eq!(start.fract() + end.fract(), 0.0, "column {column} @{scale}: child {index} is on whole pixels");
                let end = rects.get(index + 1).map_or(end, |next| along(next).0);
                for at in start as i32..end as i32 {
                    assert_eq!(pixel(&mut host, at), color(index), "column {column} @{scale}: pixel {at}, child {index}");
                }
            }
        }
    }
}

#[test]
fn a_one_pixel_stroke_is_the_same_pixels_wherever_the_shape_sits() {
    for scale in SCALES {
        // Every shape starts at another fraction of a point.
        let shapes: Vec<Build<SkiaShape>> = (0..6)
            .map(|index| {
                let shape = SkiaShape::new().stroke_width(-1).stroke_color(Color::WHITE).background_color(Color::BLUE);
                shape.width_request(40.3).height_request(11.7).margin((index as f32 * 0.37, 0.0, 0.0, 0.0))
            })
            .collect();
        let root = SkiaLayout::column().spacing(3.3).padding(1.7).children(shapes);
        let mut host = Headless::new(Ui::new((), |_| root), 487, 391, scale);
        host.settle();

        let root = host.ui.tree.root().unwrap();
        let rects: Vec<Rect> = host.ui.tree.children(root).iter().map(|child| host.rect(*child)).collect();
        // Across each edge through the middle of the side: one pixel outside, three inside.
        let mut edges = |rect: &Rect| {
            let (x, y) = (rect.center_x() as i32, rect.center_y() as i32);
            let (left, top, right, bottom) = (rect.left as i32, rect.top as i32, rect.right as i32, rect.bottom as i32);
            [
                [-1, 0, 1, 2].map(|at| host.pixel(left + at, y)),
                [-1, 0, 1, 2].map(|at| host.pixel(x, top + at)),
                [0, -1, -2, -3].map(|at| host.pixel(right + at, y)),
                [0, -1, -2, -3].map(|at| host.pixel(x, bottom + at)),
            ]
        };
        // The outline runs a pixel inside the rect, on the line between two pixels (upstream's
        // shape geometry, C# GetInflationForStroke): the stroke is half of each, at every edge of
        // every shape, because every rect is whole pixels.
        let stroked = edges(&rects[0]);
        for side in stroked {
            assert_eq!(side[0], Color::TRANSPARENT, "@{scale}: nothing outside the rect");
            assert_eq!(side[3], Color::BLUE, "@{scale}: the fill");
        }
        for (index, rect) in rects.iter().enumerate() {
            assert_eq!([rect.left.fract(), rect.top.fract(), rect.right.fract(), rect.bottom.fract()], [0.0; 4], "@{scale}: shape {index}");
            assert_eq!(edges(rect), stroked, "@{scale}: shape {index} at {rect:?}");
        }
    }
}

// ---------------------------------------------------------------- costs

/// Content measures of every spy, in the order the spies were made.
type Measures = Rc<RefCell<Vec<u32>>>;

/// An Absolute layout that counts its content measures.
struct Spy {
    layout: SkiaLayout,
    index: usize,
    measures: Measures,
}

impl Container for Spy {}

impl Control for Spy {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.measures.borrow_mut()[self.index] += 1;
        self.layout.measure(cx, width, height)
    }
}

/// Every layout type with sizes between pixels, 16 spies as the leaves.
fn costly(measures: &Measures) -> Build<SkiaLayout> {
    let spy = || {
        measures.borrow_mut().push(0);
        Build::new(Spy { layout: SkiaLayout::default(), index: measures.borrow().len() - 1, measures: measures.clone() })
    };
    let sized = |width: f32, height: f32| spy().width_request(width).height_request(height);
    let (center, end) = (LayoutOptions::Center, LayoutOptions::End);
    SkiaLayout::column().spacing(3.3).fill().padding(2.5).children((
        SkiaLayout::row().spacing(2.7).fill_x().children((
            sized(40.3, 20.0),
            spy().fill_x().height_request(17.5),
            spy().fill_x().height_request(9.1),
            sized(13.3, 11.0),
        )),
        SkiaLayout::grid().column_definitions("Auto,*,2*").row_definitions("Auto,*").height_request(61.3).children((
            sized(37.7, 15.5),
            spy().fill().column(1),
            sized(20.3, 10.0).horizontal_options(center).vertical_options(center).column(2),
            spy().fill().row(1).column_span(3),
        )),
        SkiaWrap::new().spacing(3.3).children((0..5).map(|_| sized(47.3, 19.1)).collect::<Vec<_>>()),
        SkiaLayout::new().fill().children((
            sized(21.0, 11.0).horizontal_options(center).vertical_options(center).margin((0, 0, 5, 0)),
            spy().fill().margin(2.5),
            sized(10.5, 10.5).horizontal_options(end).vertical_options(end),
        )),
    ))
}

#[test]
fn a_layout_is_done_in_one_frame_and_the_next_frames_measure_nothing() {
    for scale in SCALES {
        let measures = Measures::default();
        let tree = costly(&measures);
        let mut host = Headless::new(Ui::new((), |_| tree), 900, 700, scale);
        host.frame();
        let first = measures.borrow().clone();
        // Once each. The grid children twice: for the tracks, then in their cells. The vertical
        // Fill child on the Auto row three times: it is measured unbounded for the height its row
        // grows to (C# 7cf1007c), then at its final cell. The last child of the row a second time
        // where the shares, whole pixels each, took a pixel of its room.
        assert!(first[3] == 1 || first[3] == 2, "@{scale}: {first:?}");
        let others: Vec<u32> = first.iter().enumerate().filter(|(index, _)| *index != 3).map(|(_, count)| *count).collect();
        assert_eq!(others, [1, 1, 1, 2, 3, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1], "@{scale}: content measures of the first frame");
        for _ in 0..5 {
            host.frame_after(16.0);
        }
        // Rounding does not send a size back for another measure.
        assert_eq!(*measures.borrow(), first, "@{scale}: a steady frame measured");
        assert!(!host.ui.needs_frame(), "@{scale}");
    }
}
