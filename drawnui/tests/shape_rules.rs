//! SkiaShape against the C# engine: shape types, per-corner radii, the stroke rules, children inside
//! the stroke, gradients. The expected pixels and rects were read from DrawnUi.Net (headless, CPU
//! raster) with the same scenes; a pixel may differ by `TOLERANCE` per channel, the two engines
//! run different Skia builds.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const TOLERANCE: i32 = 3;
/// C# Colors.Green.
const GREEN: Color = Color::from_rgb(0, 128, 0);

/// The content in an Absolute root over a white canvas of `size` points.
fn shot(scale: f32, size: i32, content: impl IntoChildren) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::WHITE);
    let pixels = (size as f32 * scale) as i32;
    let mut host = Headless::new(ui, pixels, pixels, scale);
    host.settle();
    host
}

/// A blue shape at (x, y), points.
fn boxed(x: i32, y: i32, width: i32, height: i32) -> Build<SkiaShape> {
    SkiaShape::new().margin((x, y, 0, 0)).width_request(width).height_request(height).background_color(Color::BLUE)
}

fn fill_child() -> Build<SkiaLayout> {
    SkiaLayout::new().fill().background_color(GREEN)
}

fn hex(color: Color) -> String {
    format!("{:02X}{:02X}{:02X}{:02X}", color.a(), color.r(), color.g(), color.b())
}

fn close(a: Color, b: Color) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= TOLERANCE;
    d(a.a(), b.a()) && d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b())
}

/// Compares pixels with a row of `AARRGGBB` values printed by the C# probe.
#[track_caller]
fn assert_pixels(what: &str, actual: Vec<Color>, expected: &str) {
    let expected: Vec<Color> = expected.split_whitespace().map(|v| Color::new(u32::from_str_radix(v, 16).unwrap())).collect();
    let same = actual.len() == expected.len() && actual.iter().zip(&expected).all(|(a, b)| close(*a, *b));
    let print = |colors: &[Color]| colors.iter().map(|c| hex(*c)).collect::<Vec<_>>().join(" ");
    assert!(same, "{what}\n  got      {}\n  upstream {}", print(&actual), print(&expected));
}

fn row<S>(host: &mut Headless<S>, y: i32, from: i32, to: i32, step: i32) -> Vec<Color> {
    (from..=to).step_by(step as usize).map(|x| host.pixel(x, y)).collect()
}

fn column<S>(host: &mut Headless<S>, x: i32, from: i32, to: i32, step: i32) -> Vec<Color> {
    (from..=to).step_by(step as usize).map(|y| host.pixel(x, y)).collect()
}

// ---------------------------------------------------------------- corners

/// The pixel 2 px inside each corner of the 100 x 100 shape at (50, 50): top left, top right,
/// bottom left, bottom right. White = the corner is rounded away.
fn corners(radius: impl drawnui::IntoProp<CornerRadius>) -> [bool; 4] {
    let mut host = shot(1.0, 200, boxed(50, 50, 100, 100).corner_radius(radius));
    [(52, 52), (147, 52), (52, 147), (147, 147)].map(|(x, y)| host.pixel(x, y) == Color::WHITE)
}

/// Each radius rounds the corner it names. Upstream draws BottomLeft on the bottom right corner
/// and BottomRight on the bottom left one (CreateScaledRadii hands SetRectRadii its radii in the
/// wrong order): the port follows the names.
#[test]
fn a_corner_radius_rounds_the_corner_it_names() {
    assert_eq!(corners(CornerRadius::new(30.0, 0.0, 0.0, 0.0)), [true, false, false, false]);
    assert_eq!(corners(CornerRadius::new(0.0, 30.0, 0.0, 0.0)), [false, true, false, false]);
    assert_eq!(corners(CornerRadius::new(0.0, 0.0, 30.0, 0.0)), [false, false, true, false]);
    assert_eq!(corners(CornerRadius::new(0.0, 0.0, 0.0, 30.0)), [false, false, false, true]);
    assert_eq!(corners((30, 0, 0, 30)), [true, false, false, true]);
    // A plain number still sets all four.
    assert_eq!(corners(30), [true; 4]);
    assert_eq!(corners(30.0), [true; 4]);
    assert_eq!(corners(0), [false; 4]);
}

// ---------------------------------------------------------------- strokes

/// A red stroke on the blue 100 x 100 shape at (50, 50): the 10 pixels from one outside the left
/// edge, through the middle row. Positive widths are points, negative ones pixels; on a rounded
/// rectangle a stroke of a point or less is drawn at 0.55 of its width; up to 2 points it runs
/// through pixel centers.
#[test]
fn stroke_widths_follow_the_upstream_rules() {
    // (scale, stroke width, corner radius, the row, the column when upstream's differs from the row)
    #[rustfmt::skip]
    let cases: [(f32, f32, i32, &str, &str); 16] = [
        (1.0, 4.0, 0, "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        // A plain rect keeps its width: half a pixel on each side of x = 51. These are upstream's column
        // values; its Skia build antialiases the vertical edges of a 1 px frame heavier (FFFF1818 FF654DE7).
        (1.0, 1.0, 0, "FFFFFFFF FFFF8080 FFAA2A7F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        // 0.55 px: the pixel is mostly the fill below it.
        (1.0, 1.0, 20, "FFFFFFFF FFFFFFFF FFB22774 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", "FFFFFFFF FFFFFFFF FFA81D74 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
        (1.0, 2.0, 20, "FFFFFFFF FFFF7F7F FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (1.0, 3.0, 20, "FFFFFFFF FFFF7F7F FFFF0000 FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (1.0, -3.0, 0, "FFFFFFFF FFFF7F7F FFFF0000 FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (1.0, -3.0, 20, "FFFFFFFF FFFF7F7F FFFF0000 FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        // Just above a point: the real width.
        (1.0, 1.55, 20, "FFFFFFFF FFFFB8B8 FFFF0000 FF4600B9 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", "FFFFFFFF FFFFBFBF FFFF0000 FF4000BF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
        (2.0, 4.0, 0, "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF", ""),
        (2.0, 1.0, 0, "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        // 2 px * 0.55 = 1.1 px around the pixel center 101.5.
        (2.0, 1.0, 20, "FFFFFFFF FFFFF2F2 FFFF0000 FF0C00F3 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", "FFFFFFFF FFFFFFFF FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
        (2.0, 2.0, 20, "FFFFFFFF FFFF7F7F FFFF0000 FFFF0000 FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (2.0, 3.0, 20, "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF", ""),
        // Pixels stay pixels at any scale.
        (2.0, -3.0, 0, "FFFFFFFF FFFF7F7F FFFF0000 FFFF0000 FF80007F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (2.0, -3.0, 20, "FFFFFFFF FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF", ""),
        (2.0, 1.55, 20, "FFFFFFFF FFFFF2F2 FFFF0000 FFFF0000 FFFF0000 FF0C00F3 FF0000FF FF0000FF FF0000FF FF0000FF", "FFFFFFFF FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
    ];
    for (scale, width, radius, across, down) in cases {
        let shape = boxed(50, 50, 100, 100).stroke_width(width).stroke_color(Color::RED).corner_radius(radius);
        let mut host = shot(scale, 200, shape);
        let (left, middle) = ((50.0 * scale) as i32, (100.0 * scale) as i32);
        let what = format!("scale {scale} stroke {width} radius {radius}");
        let down = if down.is_empty() { across } else { down };
        assert_pixels(&format!("{what}, row"), row(&mut host, middle, left - 1, left + 8, 1), across);
        assert_pixels(&format!("{what}, column"), column(&mut host, middle, left - 1, left + 8, 1), down);
    }
}

/// Upstream StrokeColor is gray: a width alone strokes. A transparent color strokes nothing.
#[test]
fn the_default_stroke_color_is_gray_and_transparent_means_no_stroke() {
    let mut host = shot(1.0, 200, boxed(50, 50, 100, 100).stroke_width(4));
    assert_eq!(host.pixel(51, 100), Color::from_rgb(0x80, 0x80, 0x80));
    let mut host = shot(1.0, 200, boxed(50, 50, 100, 100).stroke_width(4).stroke_color(Color::TRANSPARENT));
    assert_eq!(host.pixel(51, 100), Color::BLUE);
}

/// Circle (the largest that fits, centered) and Ellipse in a 100 x 60 box at (50, 50).
#[test]
fn circle_and_ellipse_strokes() {
    #[rustfmt::skip]
    let cases: [(ShapeType, i32, &str, &str); 4] = [
        (ShapeType::Circle, 1,
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFC1C1 FF560AB2 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFC1C1 FF5307B3 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
        (ShapeType::Circle, 4,
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFF0B0B FFFF0000 FFFF0000 FFFF0000 FF0B00F4 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0909 FFFF0000 FFFF0000 FFFF0000 FF0C00F3 FF0000FF FF0000FF FF0000FF FF0000FF"),
        (ShapeType::Ellipse, 1,
            "FFFFFFFF FFFFC5C5 FF9141AE FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFBEBE FF7A31B5 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
        (ShapeType::Ellipse, 4,
            "FFFFFFFF FFFF0909 FFFF0000 FFFF0000 FFFF0000 FF1400EB FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF"),
    ];
    for (shape_type, width, across, down) in cases {
        let shape = boxed(50, 50, 100, 60).shape_type(shape_type).stroke_width(width).stroke_color(Color::RED);
        let mut host = shot(1.0, 200, shape);
        assert_pixels(&format!("{shape_type:?} stroke {width}, row 80"), row(&mut host, 80, 49, 80, 1), across);
        assert_pixels(&format!("{shape_type:?} stroke {width}, column 100"), column(&mut host, 100, 49, 58, 1), down);
    }
}

// ---------------------------------------------------------------- children and the stroke

/// Children are laid out inside the outline (the rect inset by half the stroke on whole pixels,
/// at least 1), and an auto-sized shape is its content plus that inset on every side.
#[test]
fn children_live_inside_the_stroke() {
    // (scale, stroke, child of the fixed 100 x 100 shape at (50, 50), auto shape around a 50 x 40 child, that child)
    #[rustfmt::skip]
    let cases: [(f32, f32, [f32; 4], [f32; 4], [f32; 4]); 15] = [
        (1.0, 0.0, [50., 50., 150., 150.], [50., 170., 100., 210.], [50., 170., 100., 210.]),
        (1.0, 1.0, [51., 51., 149., 149.], [50., 170., 102., 212.], [51., 171., 101., 211.]),
        (1.0, 3.0, [52., 52., 148., 148.], [50., 170., 104., 214.], [52., 172., 102., 212.]),
        (1.0, 4.0, [52., 52., 148., 148.], [50., 170., 104., 214.], [52., 172., 102., 212.]),
        (1.0, -3.0, [52., 52., 148., 148.], [50., 170., 104., 214.], [52., 172., 102., 212.]),
        (1.5, 0.0, [75., 75., 225., 225.], [75., 255., 150., 315.], [75., 255., 150., 315.]),
        (1.5, 1.0, [76., 76., 224., 224.], [75., 255., 152., 317.], [76., 256., 151., 316.]),
        (1.5, 3.0, [78., 78., 222., 222.], [75., 255., 156., 321.], [78., 258., 153., 318.]),
        (1.5, 4.0, [78., 78., 222., 222.], [75., 255., 156., 321.], [78., 258., 153., 318.]),
        (1.5, -3.0, [77., 77., 223., 223.], [75., 255., 154., 319.], [77., 257., 152., 317.]),
        (2.0, 0.0, [100., 100., 300., 300.], [100., 340., 200., 420.], [100., 340., 200., 420.]),
        (2.0, 1.0, [101., 101., 299., 299.], [100., 340., 202., 422.], [101., 341., 201., 421.]),
        (2.0, 3.0, [103., 103., 297., 297.], [100., 340., 206., 426.], [103., 343., 203., 423.]),
        (2.0, 4.0, [104., 104., 296., 296.], [100., 340., 208., 428.], [104., 344., 204., 424.]),
        (2.0, -3.0, [102., 102., 298., 298.], [100., 340., 204., 424.], [102., 342., 202., 422.]),
    ];
    #[derive(Default)]
    struct Handles {
        child: Handle<SkiaLayout>,
        auto: Handle<SkiaShape>,
        inner: Handle<SkiaLayout>,
    }
    for (scale, stroke, child, auto, inner) in cases {
        let ui = Ui::new(Handles::default(), |h| {
            let fixed = boxed(50, 50, 100, 100).stroke_width(stroke).stroke_color(Color::RED).children(fill_child().assign(&mut h.child));
            let content = SkiaLayout::new().width_request(50).height_request(40).background_color(GREEN).assign(&mut h.inner);
            let auto = SkiaShape::new()
                .margin((50, 170, 0, 0))
                .background_color(Color::BLUE)
                .stroke_width(stroke)
                .stroke_color(Color::RED)
                .children(content)
                .assign(&mut h.auto);
            SkiaLayout::new().fill().children((fixed, auto))
        });
        let mut host = Headless::new(ui, (300.0 * scale) as i32, (300.0 * scale) as i32, scale);
        host.settle();
        let rect = |r: [f32; 4]| Rect::new(r[0], r[1], r[2], r[3]);
        let (h_child, h_auto, h_inner) = (host.ui.state.child, host.ui.state.auto, host.ui.state.inner);
        let what = format!("scale {scale} stroke {stroke}");
        assert_eq!(host.rect(h_child), rect(child), "{what}: child of the fixed shape");
        assert_eq!(host.rect(h_auto), rect(auto), "{what}: auto-sized shape");
        assert_eq!(host.rect(h_inner), rect(inner), "{what}: child of the auto-sized shape");
    }
}

/// The stroke is drawn over the children, and the children are clipped to the inside of the shape.
#[test]
fn the_stroke_covers_the_children_and_the_shape_clips_them() {
    // Rounded 20, stroke 6, a child that fills: along the diagonal from the corner and along the middle row.
    let shape = boxed(50, 50, 100, 100).corner_radius(20).stroke_width(6).stroke_color(Color::RED).children(fill_child());
    let mut host = shot(1.0, 200, shape);
    let diagonal: Vec<Color> = (50..=70).map(|i| host.pixel(i, i)).collect();
    assert_pixels(
        "diagonal",
        diagonal,
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFE1E1 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000",
    );
    assert_pixels(
        "middle row",
        row(&mut host, 100, 49, 60, 1),
        "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF008000 FF008000 FF008000 FF008000 FF008000",
    );
}

// ---------------------------------------------------------------- shape types

/// Rows 55, 75, 100, 125, 145 of the 100 x 100 shape at (50, 50), every 10th pixel from 50 to 150.
#[track_caller]
fn assert_rows(what: &str, shape: Build<SkiaShape>, expected: [&str; 5]) {
    let mut host = shot(1.0, 200, shape);
    for (y, expected) in [55, 75, 100, 125, 145].into_iter().zip(expected) {
        assert_pixels(&format!("{what}, row {y}"), row(&mut host, y, 50, 150, 10), expected);
    }
}

fn triangle() -> Vec<(f32, f32)> {
    vec![(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)]
}

#[test]
fn path_is_stretched_over_the_box() {
    let rows = [
        "FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF",
        "FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF",
        "FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF",
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
    ];
    let path = |data: &str| boxed(50, 50, 100, 100).shape_type(ShapeType::Path).path_data(data);
    assert_rows("triangle at the origin", path("M0,0 L40,0 L20,40 Z"), rows);
    // Upstream moves a path that does not start at (0, 0) off the box (a wrong matrix order in
    // CalculateSizeForStroke); the port fits the path's own bounds to the box, whatever they are.
    assert_rows("triangle away from the origin", path("M10,10 L50,10 L30,50 Z"), rows);
}

#[test]
fn polygon_points_are_ratios_of_the_box() {
    let polygon = || boxed(50, 50, 100, 100).shape_type(ShapeType::Polygon).points(triangle());
    assert_rows(
        "polygon",
        polygon(),
        [
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF",
        ],
    );
    // Children are clipped to the polygon.
    assert_rows(
        "polygon with a child that fills",
        polygon().children(fill_child()),
        [
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF008000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF008000 FF008000 FF008000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FF008000 FF008000 FF008000 FF008000 FF008000 FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FFFFFFFF FFFFFFFF",
            "FFFFFFFF FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FFFFFFFF",
        ],
    );
    assert_rows(
        "polygon with a stroke",
        polygon().stroke_width(4).stroke_color(Color::RED),
        [
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF890076 FF0000FF FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFFEBEB FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF FFFFFFFF",
            "FFFFEBEB FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFFFFFF",
        ],
    );
}

/// Open shapes: no fill, no children, the stroke only. The cap decides how a line ends.
#[test]
fn line_and_arc_are_stroked_only() {
    const NONE: &str = "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF";
    let line = || {
        boxed(50, 50, 100, 100)
            .shape_type(ShapeType::Line)
            .points(vec![(0.0, 0.5), (1.0, 0.5)])
            .stroke_width(4)
            .stroke_color(Color::RED)
    };
    // The default cap is Round: it reaches half the stroke past the end of the line.
    let round = "FFFF2727 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF";
    assert_rows("line", line(), [NONE, NONE, round, NONE, NONE]);
    let butt = "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF";
    assert_rows("line, butt cap", line().stroke_cap(PaintCap::Butt), [NONE, NONE, butt, NONE, NONE]);

    let arc = |width: i32| {
        boxed(50, 50, 100, 100)
            .shape_type(ShapeType::Arc)
            .value1(0)
            .value2(180)
            .stroke_width(width)
            .stroke_color(Color::RED)
            .stroke_cap(PaintCap::Butt)
    };
    assert_rows(
        "arc from 0 over 180 degrees",
        arc(6),
        [
            NONE,
            NONE,
            "FFFF0505 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFF0606 FFFF0000 FFFF0000 FFFF0000 FFFF6767 FFFFFFFF FFFFFFFF FFFFFFFF",
        ],
    );
    assert_rows(
        "arc with a child that fills",
        arc(4).children(fill_child()),
        [
            NONE,
            NONE,
            "FFFF0505 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFF2A2A FFFFFFFF FFFF0000 FFFF6565 FFFFFFFF FFFFFFFF FFFFFFFF",
        ],
    );
}

#[test]
fn a_circle_clips_its_children() {
    let shape = boxed(50, 50, 100, 100).shape_type(ShapeType::Circle).stroke_width(4).stroke_color(Color::RED).children(fill_child());
    assert_rows(
        "circle",
        shape,
        [
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FF00711D FF008000 FF1F494D FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF",
            "FFFFFFFF FFEB0013 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FFFF0000 FFFFFFFF",
            "FFFF0505 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FFFFFFFF",
            "FFFFFFFF FFFF0000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FF008000 FFFF0000 FFFFFFFF",
            "FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFD50226 FF008000 FFFF0000 FFFF6565 FFFFFFFF FFFFFFFF FFFFFFFF",
        ],
    );
}

/// Dashes in points along the outline, starting at its top left corner.
#[test]
fn stroke_path_dashes_the_stroke() {
    let shape = boxed(50, 50, 100, 100).stroke_width(4).stroke_color(Color::RED).stroke_path(vec![10.0, 5.0]).stroke_cap(PaintCap::Butt);
    let mut host = shot(1.0, 200, shape);
    assert_pixels(
        "top edge",
        row(&mut host, 52, 50, 110, 2),
        "FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF FFFFFFFF FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF FFFFFFFF",
    );
}

// ---------------------------------------------------------------- gradients

fn red_to_blue(gradient_type: GradientType) -> SkiaGradient {
    SkiaGradient::new(gradient_type, [Color::RED, Color::BLUE])
}

/// A shape of `width` x `height` at (50, 50) with a gradient and no background color.
fn filled(gradient: SkiaGradient, width: i32, height: i32) -> Build<SkiaShape> {
    SkiaShape::new().margin((50, 50, 0, 0)).width_request(width).height_request(height).fill_gradient(gradient)
}

/// Ten pixels down the middle column and ten along the middle row of a `width` x `height` box at (50, 50).
#[track_caller]
fn assert_gradient(what: &str, content: impl IntoChildren, width: i32, height: i32, down: &str, across: &str) {
    let mut host = shot(1.0, 200, content);
    assert_pixels(&format!("{what}, column"), column(&mut host, 100, 50, 50 + height - 1, height / 10), down);
    assert_pixels(&format!("{what}, row"), row(&mut host, 50 + height / 2, 50, 50 + width - 1, width / 10), across);
}

const DOWN: &str = "FFFE0001 FFE4001B FFCB0034 FFB1004E FF980067 FF7E0081 FF65009A FF4B00B4 FF3200CD FF1800E7";
const MIDDLE: &str = "FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081 FF7E0081";

#[test]
fn linear_gradient_runs_between_its_ratios() {
    // Top to bottom by default.
    assert_gradient("default", filled(red_to_blue(GradientType::Linear), 100, 100), 100, 100, DOWN, MIDDLE);
    // The gradient wins over the background color.
    let over = filled(red_to_blue(GradientType::Linear), 100, 100).background_color(Color::GREEN);
    assert_gradient("over a background color", over, 100, 100, DOWN, MIDDLE);
    // A base property: any control has it.
    let layout = SkiaLayout::new().margin((50, 50, 0, 0)).width_request(100).height_request(100).fill_gradient(red_to_blue(GradientType::Linear));
    assert_gradient("a plain layout", layout, 100, 100, DOWN, MIDDLE);
    // Conical is drawn as Linear upstream.
    assert_gradient("conical", filled(red_to_blue(GradientType::Conical), 100, 100), 100, 100, DOWN, MIDDLE);
    assert_gradient("angle 90", filled(red_to_blue(GradientType::Linear).angle(90.0), 100, 100), 100, 100, MIDDLE, DOWN);
    // Type None paints nothing.
    let white = "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF";
    assert_gradient("none", filled(red_to_blue(GradientType::None), 100, 100), 100, 100, white, white);
}

/// Upstream LinearGradientAngleToPoints: 0 = top to bottom, 90 = left to right, 180 = bottom to
/// top, 270 = right to left.
#[test]
fn angle_sets_the_ratios_as_upstream() {
    let ratios = |degrees: f32| {
        let g = SkiaGradient::default().angle(degrees);
        [g.start_x_ratio, g.start_y_ratio, g.end_x_ratio, g.end_y_ratio].map(|v| (v * 1000.0).round() / 1000.0)
    };
    assert_eq!(ratios(0.0), [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(ratios(45.0), [0.0, 0.0, 0.707, 0.707]);
    assert_eq!(ratios(90.0), [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(ratios(180.0), [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(ratios(270.0), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn color_positions_opacity_and_light() {
    let mut stops = SkiaGradient::new(GradientType::Linear, [Color::RED, Color::GREEN, Color::BLUE]);
    stops.color_positions = vec![0.0, 0.2, 1.0];
    assert_gradient(
        "positions 0, 0.2, 1",
        filled(stops, 100, 100),
        100,
        100,
        "FFF90600 FF798600 FF00FD02 FF00DE21 FF00BE41 FF009E61 FF007E81 FF005EA1 FF003EC1 FF001EE1",
        "FF009E61 FF009E61 FF009E61 FF009E61 FF009E61 FF009E61 FF009E61 FF009E61 FF009E61 FF009E61",
    );
    let mut faded = red_to_blue(GradientType::Linear);
    faded.opacity = 0.5;
    assert_gradient(
        "opacity 0.5",
        filled(faded, 100, 100),
        100,
        100,
        "FFFE7F80 FFF17F8D FFE57F99 FFD87FA6 FFCB7FB3 FFBE7FC0 FFB27FCC FFA57FD9 FF987FE6 FF8B7FF3",
        "FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0 FFBE7FC0",
    );
    let mut dark = red_to_blue(GradientType::Linear);
    dark.light = 0.5;
    assert_gradient(
        "light 0.5",
        filled(dark, 100, 100),
        100,
        100,
        "FF7F0001 FF73000D FF66001A FF590027 FF4C0034 FF3F0041 FF33004D FF26005A FF190067 FF0C0074",
        "FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041 FF3F0041",
    );
    let mut light = SkiaGradient::new(GradientType::Linear, [Color::from_rgb(200, 50, 50), Color::from_rgb(50, 50, 200)]);
    light.light = 1.5;
    assert_gradient(
        "light 1.5",
        filled(light, 100, 100),
        100,
        100,
        "FFE49898 FFDC98A0 FFD498A8 FFCD98AF FFC598B7 FFBE98BE FFB698C6 FFAE98CE FFA798D5 FF9F98DD",
        "FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE FFBE98BE",
    );
}

/// Circular: radius = half of the smaller side. Oval: stretched to both sides. 100 x 60 box.
#[test]
fn radial_gradients() {
    let centered = |gradient_type| {
        let mut g = red_to_blue(gradient_type);
        (g.start_x_ratio, g.start_y_ratio) = (0.5, 0.5);
        filled(g, 100, 60)
    };
    assert_gradient(
        "circular",
        centered(GradientType::Circular),
        100,
        60,
        "FF0400FB FF3700C8 FF6A0095 FF9D0062 FFD0002F FFF90006 FFC80037 FF95006A FF62009D FF2F00D0",
        "FF0000FF FF0000FF FF0400FB FF5900A6 FFAE0051 FFF90006 FFA60059 FF5100AE FF0000FF FF0000FF",
    );
    assert_gradient(
        "oval",
        centered(GradientType::Oval),
        100,
        60,
        "FF0400FB FF3700C8 FF6A0095 FF9D0062 FFD0002F FFFA0005 FFC80037 FF95006A FF62009D FF2F00D0",
        "FF0300FC FF3600C9 FF680097 FF9B0064 FFCE0031 FFFA0005 FFC90036 FF960069 FF63009C FF3000CF",
    );
}

/// Sweep: around the center from `value1` over `value2` degrees.
#[test]
fn sweep_gradient_uses_value1_and_value2() {
    let sweep = || filled(red_to_blue(GradientType::Sweep), 100, 100);
    let full = (
        "FF3F00C0 FF3F00C0 FF3F00C0 FF3F00C0 FF3E00C1 FFDF0020 FFC1003E FFC0003F FFC0003F FFC0003F",
        "FF80007F FF80007F FF80007F FF81007E FF82007D FFDF0020 FFFD0002 FFFE0001 FFFE0001 FFFE0001",
    );
    assert_gradient("0 over 360", sweep().value1(0).value2(360), 100, 100, full.0, full.1);
    // Changed: upstream draws a zero sweep (its default) as one solid color; here it is the full circle.
    assert_gradient("0 over 0", sweep(), 100, 100, full.0, full.1);
    assert_gradient(
        "90 over 180",
        sweep().value1(90).value2(180),
        100,
        100,
        "FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000",
        "FF80007F FF81007E FF81007E FF82007D FF84007B FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFF0000",
    );
}

/// The stroke gradient runs over the outline rect; the alpha of `stroke_color` still applies.
#[test]
fn stroke_gradient() {
    let stroked = |color: Color| {
        boxed(50, 50, 100, 100).stroke_width(10).stroke_color(color).stroke_gradient(red_to_blue(GradientType::Linear))
    };
    assert_gradient(
        "stroke gradient",
        stroked(Color::BLACK),
        100,
        100,
        "FFFF0000 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0D00F2",
        "FF7E0081 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF7E0081",
    );
    assert_gradient(
        "stroke gradient, stroke color at half alpha",
        stroked(Color::from_argb(128, 0, 0, 0)),
        100,
        100,
        "FFFF7F7F FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0700F8",
        "FFBE7FC0 FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF0000FF FF3F00C0",
    );
}

/// A gradient or a rect change builds the shader again; nothing else does.
#[test]
fn a_changed_gradient_is_painted() {
    let ui = Ui::new(Handle::<SkiaShape>::default(), |shape| {
        SkiaLayout::new().fill().children(filled(red_to_blue(GradientType::Linear), 100, 100).assign(shape))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_pixels("before", column(&mut host, 100, 50, 149, 10), DOWN);
    let shape = host.ui.state;
    host.ui.tree.get_mut(shape).unwrap().set_fill_gradient(red_to_blue(GradientType::Linear).angle(90.0));
    host.settle();
    assert_pixels("after the gradient changed", row(&mut host, 100, 50, 149, 10), DOWN);
    // Twice as high: the gradient stretches with the rect.
    host.ui.tree.get_mut(shape).unwrap().set_fill_gradient(red_to_blue(GradientType::Linear));
    host.ui.tree.get_mut(shape).unwrap().set_height_request(50);
    host.settle();
    assert_pixels("after the rect changed", column(&mut host, 100, 50, 99, 5), DOWN);
}

// ---------------------------------------------------------------- overlays and the button

/// Overlay effects (the ripple) are clipped to the shape, whatever its type.
#[test]
fn a_ripple_stays_inside_a_polygon() {
    let ui = Ui::new(Handle::<SkiaShape>::default(), |shape| {
        let polygon = boxed(50, 50, 100, 100).shape_type(ShapeType::Polygon).points(triangle());
        SkiaLayout::new().fill().children(polygon.assign(shape))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    let shape = host.ui.state;
    host.ui.tree.cx().play_ripple(shape, Color::BLACK, 50.0, 80.0, 0.0);
    host.frame();
    // 400 of 500 ms: the circle (radius 300 pt x 0.8^3) is larger than the shape.
    host.frame_after(400.0);
    let inside = host.pixel(100, 130);
    assert!(inside.b() < 250 && inside.r() == 0, "black over the blue triangle: {inside:?}");
    // Inside the box, outside the triangle: nothing.
    assert_eq!(host.pixel(55, 55), Color::WHITE);
    assert_eq!(host.pixel(145, 60), Color::WHITE);
    host.settle();
    assert_eq!(host.pixel(100, 130), Color::BLUE);
}

/// The button paints its own frame: it takes the base `fill_gradient` like every control.
#[test]
fn a_button_frame_takes_the_fill_gradient() {
    const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
    let ui = Ui::new((), |_| {
        let button = SkiaButton::new("").margin((50, 50, 0, 0)).width_request(100).height_request(100);
        SkiaLayout::new().fill().children(button.corner_radius(0).fill_gradient(red_to_blue(GradientType::Linear)))
    });
    let mut host = Headless::new(ui.font_bytes("Default", FONT).background(Color::WHITE), 200, 200, 1.0);
    host.settle();
    assert_pixels("column", column(&mut host, 100, 50, 149, 10), DOWN);
    assert_pixels("row", row(&mut host, 100, 50, 149, 10), MIDDLE);
}

// ---------------------------------------------------------------- one-pixel strokes

/// The four edges of thin strokes, 6 pixels across each, against upstream. A 1 px stroke on a
/// plain rectangle sits on the line between two pixels (the outline is inset by a whole pixel) and
/// covers both at half alpha, in C# as here; 2 px cover two whole pixels. A rounded rectangle runs
/// through pixel centers instead: one pixel, at 0.55 of the width.
/// The left edge of a thin plain frame is drawn heavier by upstream's Skia build (FFFF1818 FF654DE7
/// where its other three edges have FFFF8080 FFAA2A7F); the geometry is the same, so that edge is
/// held to the mirror of the right one.
#[test]
fn one_pixel_strokes_cover_the_same_pixels_as_upstream() {
    // (scale, stroke width, corner radius, left, top, right, bottom)
    #[rustfmt::skip]
    let cases: [(f32, f32, i32, &str, &str, &str, &str); 16] = [
        (1.0, 1.0, 0, // left: mirror of right, see above
            "FFFFFFFF FFFF7F7F FFA92A80 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF8080 FFAA2A7F FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF"),
        (1.0, 1.0, 12,
            "FFFFFFFF FFFFFFFF FFB22774 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FFA81D74 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFB22774 FFFFFFFF FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA81D74 FFFFFFFF FFFFFFFF"),
        (1.0, -1.0, 0, // left: mirror of right, see above
            "FFFFFFFF FFFF7F7F FFA92A80 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF8080 FFAA2A7F FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF"),
        (1.0, -1.0, 12,
            "FFFFFFFF FFFFFFFF FFB22774 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FFA81D74 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFB22774 FFFFFFFF FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA81D74 FFFFFFFF FFFFFFFF"),
        (1.0, 0.5, 0, // left: mirror of right, see above
            "FFFFFFFF FFFFC0C0 FF5F20C0 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFC0C0 FF5F20C0 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FF5F20C0 FFFFC0C0 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FF5F20C0 FFFFC0C0 FFFFFFFF"),
        (1.0, 0.5, 12,
            "FFFFFFFF FFFFFFFF FF641FBA FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FF732EBA FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FF6520BA FFFFFFFF FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FF732EBA FFFFFFFF FFFFFFFF"),
        (1.0, -2.0, 0,
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF"),
        (1.0, 2.0, 0,
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF"),
        (2.0, 1.0, 0,
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF"),
        (2.0, 1.0, 12,
            "FFFFFFFF FFFFF2F2 FFFF0000 FF0C00F3 FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0D00F2 FFFF0000 FFFFF3F3 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFFFFFF FFFFFFFF"),
        (2.0, -1.0, 0, // left: mirror of right, see above
            "FFFFFFFF FFFF7F7F FFA92A80 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF8080 FFAA2A7F FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF"),
        (2.0, -1.0, 12,
            "FFFFFFFF FFFFFFFF FFB22774 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FFA81D74 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFB22774 FFFFFFFF FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA81D74 FFFFFFFF FFFFFFFF"),
        (2.0, 0.5, 0, // left: mirror of right, see above
            "FFFFFFFF FFFF7F7F FFA92A80 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF8080 FFAA2A7F FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA92A80 FFFF7F7F FFFFFFFF"),
        (2.0, 0.5, 12,
            "FFFFFFFF FFFFFFFF FFB22774 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFFFFFF FFA81D74 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFB22774 FFFFFFFF FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFA81D74 FFFFFFFF FFFFFFFF"),
        (2.0, -2.0, 0,
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FF0000FF FF0000FF FF0000FF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF",
            "FF0000FF FF0000FF FF0000FF FFFF0000 FFFF0000 FFFFFFFF"),
        (2.0, 2.0, 0,
            "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF",
            "FFFFFFFF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FF0000FF",
            "FF0000FF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF",
            "FF0000FF FFFF0000 FFFF0000 FFFF0000 FFFF0000 FFFFFFFF"),
    ];
    for (scale, width, radius, left, top, right, bottom) in cases {
        let shape = boxed(50, 50, 100, 100).stroke_width(width).stroke_color(Color::RED).corner_radius(radius);
        let mut host = shot(scale, 200, shape);
        let (l, m, r) = ((50.0 * scale) as i32, (100.0 * scale) as i32, (150.0 * scale) as i32);
        let what = format!("scale {scale} stroke {width} radius {radius}");
        assert_pixels(&format!("{what}, left"), row(&mut host, m, l - 1, l + 4, 1), left);
        assert_pixels(&format!("{what}, top"), column(&mut host, m, l - 1, l + 4, 1), top);
        assert_pixels(&format!("{what}, right"), row(&mut host, m, r - 5, r, 1), right);
        assert_pixels(&format!("{what}, bottom"), column(&mut host, m, r - 5, r, 1), bottom);
    }
}
