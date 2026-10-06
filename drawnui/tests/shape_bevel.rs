//! SkiaShape BevelType / Bevel against the C# engine: the six shapes of the React demo's "Bevel /
//! Emboss" card at scales 1 and 2. `shape_bevel/cs_dump.txt` holds the rect and a column, a row
//! (and for curves a diagonal) of pixels of each, read from DrawnUi.Net (headless, CPU raster) with
//! the same scene: the shape at (20, 20) points on #212529 in a 240 x 200 point canvas. A pixel
//! may differ by `TOLERANCE` per channel, the two engines run different Skia builds.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const TOLERANCE: i32 = 3;
const DUMP: &str = include_str!("shape_bevel/cs_dump.txt");
const PAGE: Color = Color::from_rgb(0x21, 0x25, 0x29);
const STAR: [f32; 20] = [0.5, 0.0, 0.62, 0.38, 1.0, 0.38, 0.69, 0.61, 0.81, 1.0, 0.5, 0.76, 0.19, 1.0, 0.31, 0.61, 0.0, 0.38, 0.38, 0.38];
const HEART: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";

fn rgb(hex: u32) -> Color {
    Color::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The demo's shapes, by the names the dump uses.
fn demo(name: &str) -> Build<SkiaShape> {
    let shape = SkiaShape::new().margin((20, 20, 0, 0));
    match name {
        "bevel" | "emboss" => {
            let kind = if name == "bevel" { BevelType::Bevel } else { BevelType::Emboss };
            shape.corner_radius(12).background_color(rgb(0x495057)).width_request(110).height_request(70).bevel_type(kind).bevel(SkiaBevel::new(4))
        }
        "circle" => shape
            .shape_type(ShapeType::Circle)
            .background_color(rgb(0x0D6EFD))
            .width_request(80)
            .lock_ratio(1)
            .bevel_type(BevelType::Bevel)
            .bevel(SkiaBevel::new(6).light_color(rgb(0x9EC5FE)).shadow_color(rgb(0x052C65)).opacity(0.8)),
        "star" => shape
            .shape_type(ShapeType::Polygon)
            .points(STAR.chunks(2).map(|p| Point::new(p[0], p[1])).collect::<Vec<_>>())
            .background_color(rgb(0xFFC107))
            .width_request(90)
            .height_request(90)
            .bevel_type(BevelType::Emboss)
            .bevel(SkiaBevel::new(3).opacity(0.7)),
        "heart" => shape
            .shape_type(ShapeType::Path)
            .path_data(HEART)
            .background_color(rgb(0xD63384))
            .width_request(90)
            .height_request(90)
            .bevel_type(BevelType::Bevel)
            .bevel(SkiaBevel::new(3).opacity(0.7)),
        "sharp" => shape.background_color(rgb(0x6C757D)).width_request(110).height_request(70).bevel_type(BevelType::Bevel).bevel(SkiaBevel::new(5).opacity(1)),
        _ => unreachable!("{name}"),
    }
}

fn shot(scale: f32, shape: Build<SkiaShape>) -> Headless<Handle<SkiaShape>> {
    let ui = Ui::new(Handle::<SkiaShape>::default(), |me| SkiaLayout::new().fill().children(shape.assign(me))).background(PAGE);
    let mut host = Headless::new(ui, (240.0 * scale) as i32, (200.0 * scale) as i32, scale);
    host.settle();
    host
}

fn parse(hex: &str) -> Color {
    Color::new(u32::from_str_radix(hex, 16).expect("AARRGGBB"))
}

fn hex(color: Color) -> String {
    format!("{:02X}{:02X}{:02X}{:02X}", color.a(), color.r(), color.g(), color.b())
}

fn diff(a: Color, b: Color) -> i32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs();
    d(a.a(), b.a()).max(d(a.r(), b.r())).max(d(a.g(), b.g())).max(d(a.b(), b.b()))
}

/// The pixels of a dump line: "col x=X y=A..B", "row y=Y x=A..B" or "diag from X,Y n=N".
fn positions(spec: &str) -> Vec<(i32, i32)> {
    let numbers: Vec<i32> = spec.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect();
    match spec.split(' ').next() {
        Some("col") => (numbers[1]..=numbers[2]).map(|y| (numbers[0], y)).collect(),
        Some("row") => (numbers[1]..=numbers[2]).map(|x| (x, numbers[0])).collect(),
        Some("diag") => (0..numbers[2]).map(|i| (numbers[0] + i, numbers[1] + i)).collect(),
        _ => panic!("{spec}"),
    }
}

/// Every dumped pixel of the scene `name` at `scale`, compared. Returns the pixels compared.
fn compare(name: &str, scale: f32) -> usize {
    let header = format!("{name} scale {scale} rect ");
    let at = DUMP.find(&header).unwrap_or_else(|| panic!("{header} in the dump"));
    let mut lines = DUMP[at..].lines();
    let rect = lines.next().unwrap()[header.len()..].trim_matches(['[', ']']).split(',').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    let mut host = shot(scale, demo(name));
    let me = host.ui.state;
    assert_eq!(host.rect(me), Rect::new(rect[0], rect[1], rect[2], rect[3]), "{name} at {scale}");
    let mut compared = 0;
    for line in lines.take_while(|line| line.starts_with("  ")) {
        let (spec, values) = line.trim().split_once(": ").unwrap();
        let expected: Vec<Color> = values.split(' ').map(parse).collect();
        let at = positions(spec);
        assert_eq!(at.len(), expected.len(), "{name} {spec}");
        for ((x, y), want) in at.into_iter().zip(expected) {
            let got = host.pixel(x, y);
            assert!(diff(got, want) <= TOLERANCE, "{name} at {scale}, {spec}: ({x}, {y}) is {}, C# {}", hex(got), hex(want));
            compared += 1;
        }
    }
    compared
}

#[test]
fn rounded_rectangles_bevel_and_emboss() {
    for scale in [1.0, 2.0] {
        assert!(compare("bevel", scale) > 150);
        assert!(compare("emboss", scale) > 150);
    }
}

#[test]
fn a_sharp_rectangle_at_full_opacity() {
    for scale in [1.0, 2.0] {
        assert!(compare("sharp", scale) > 150);
    }
}

/// Colored edges; the circle's edges follow the largest circle that fits, as in React (C#
/// strokes the rect's ellipse, the same for this square box).
#[test]
fn a_circle_with_colored_edges() {
    for scale in [1.0, 2.0] {
        assert!(compare("circle", scale) > 200);
    }
}

/// The polygon's edges run on its outline, half the depth outside it, as upstream.
#[test]
fn a_polygon_emboss() {
    for scale in [1.0, 2.0] {
        assert!(compare("star", scale) > 200);
    }
}

/// The heart is fitted to the box by its own bounds here and not in C# (see PARITY, SkiaShape),
/// so its pixels are not compared with the dump: the first half of its outline (from the bottom
/// tip up the left side) is light, the rest dark.
#[test]
fn a_path_bevel_splits_its_outline_in_halves() {
    let fill = rgb(0xD63384);
    let mut plain = shot(2.0, demo("heart").bevel_type(BevelType::None));
    let mut host = shot(2.0, demo("heart"));
    let (mut lighter, mut darker) = (0, 0);
    for y in (40..220).step_by(2) {
        for x in (40..220).step_by(2) {
            let (before, after) = (plain.pixel(x, y), host.pixel(x, y));
            if before != fill || diff(before, after) <= TOLERANCE {
                continue;
            }
            let light = after.r() as i32 + after.g() as i32 + after.b() as i32 > fill.r() as i32 + fill.g() as i32 + fill.b() as i32;
            // The outline starts at the tip (130, 222.7 at scale 2) and goes up the left lobe first.
            if light {
                lighter += 1;
                assert!(x <= 134, "light edge on the left half: ({x}, {y})");
            } else {
                darker += 1;
                assert!(x >= 126, "dark edge on the right half: ({x}, {y})");
            }
        }
    }
    assert!(lighter > 20 && darker > 20, "{lighter} light, {darker} dark samples");
}

/// Without a `bevel`, or with BevelType None, nothing is drawn (upstream needs both).
#[test]
fn both_properties_are_needed() {
    let plain = |shape: Build<SkiaShape>| {
        let mut host = shot(1.0, shape);
        (host.pixel(75, 21), host.pixel(75, 88))
    };
    let fill = rgb(0x495057);
    let base = || SkiaShape::new().margin((20, 20, 0, 0)).background_color(fill).width_request(110).height_request(70);
    assert_eq!(plain(base().bevel_type(BevelType::Bevel)), (fill, fill));
    assert_eq!(plain(base().bevel(SkiaBevel::new(4))), (fill, fill));
    assert_ne!(plain(base().bevel_type(BevelType::Bevel).bevel(SkiaBevel::new(4))), (fill, fill));
}

/// Another type or depth draws again, also through the shape's Operations cache.
#[test]
fn a_change_draws_again() {
    let mut host = shot(1.0, demo("bevel"));
    let me = host.ui.state;
    let light = host.pixel(75, 21);
    host.ui.tree.get_mut(me).unwrap().set_bevel_type(BevelType::Emboss);
    host.settle();
    assert_eq!(host.pixel(75, 88), light, "the light edge moved to the bottom");
    host.ui.tree.get_mut(me).unwrap().set_bevel(SkiaBevel::new(8));
    host.settle();
    assert_eq!(host.pixel(75, 83), light, "8 points deep");
    host.ui.tree.get_mut(me).unwrap().set_bevel_type(BevelType::None);
    host.settle();
    assert_eq!(host.pixel(75, 88), rgb(0x495057));
}
