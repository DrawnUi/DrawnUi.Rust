//! SkiaLottie against CanvasKit Skottie (what DrawnUi.React draws): the demo files rendered at
//! chosen frames. `compare_with_skottie` is an ignored measurement over reference frames a node
//! script renders with CanvasKit (`LOTTIE_REF` = its folder); the other tests carry sampled pixels
//! of those frames.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const OK: &str = include_str!("lottie/ok.json");
const SHIELD: &str = include_str!("lottie/shield.json");
// Synthetic files over the renderer's subset, one feature group each.
const GEOMETRY: &str = include_str!("lottie/f_geometry.json");
const STROKES: &str = include_str!("lottie/f_strokes.json");
const GRADIENTS: &str = include_str!("lottie/f_gradients.json");
const TRANSFORMS: &str = include_str!("lottie/f_transforms.json");
const PRECOMPS: &str = include_str!("lottie/f_precomps.json");
const W: i32 = 200;
const H: i32 = 160;

#[derive(Default)]
struct App;

fn hex(s: &str) -> Color {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap();
    Color::new(0xff00_0000 | v)
}

/// The frame of a file with replaced colors, on white, `W` x `H` at scale 1.
fn render(json: &'static str, tints: &[&str], frame: i32) -> Headless<App> {
    let colors: Vec<Color> = tints.iter().map(|t| hex(t)).collect();
    let ui = Ui::new(App, move |_| SkiaLottie::new("").json(json).colors(colors).auto_play(false).default_frame(frame).fill())
        .background(Color::WHITE);
    let mut host = Headless::new(ui, W, H, 1.0);
    host.settle();
    host
}

fn channel_diff(a: Color, b: [u8; 4]) -> u8 {
    [a.r().abs_diff(b[0]), a.g().abs_diff(b[1]), a.b().abs_diff(b[2])].into_iter().max().unwrap()
}

fn case(name: &str) -> (&'static str, &'static [&'static str]) {
    match name {
        "ok" => (OK, &[]),
        "ok_colors" => (OK, &["#D63384", "#FFC107"]),
        "ok_tint" => (OK, &["#20C997"]),
        "shield" => (SHIELD, &[]),
        "shield_tint" => (SHIELD, &["#0DCAF0"]),
        "f_geometry" => (GEOMETRY, &[]),
        // Three tints over five colors, with the [0, 0, 0] positions that count as black first.
        "f_geometry_colors" => (GEOMETRY, &["#D63384", "#FFC107", "#20C997"]),
        "f_strokes" => (STROKES, &[]),
        "f_gradients" => (GRADIENTS, &[]),
        "f_transforms" => (TRANSFORMS, &[]),
        _ => (PRECOMPS, &[]),
    }
}

/// Pixels of CanvasKit Skottie 0.42 frames (`compare_with_skottie`'s references), where the
/// picture is flat around them: (x, y, RGB).
#[rustfmt::skip]
const SKOTTIE: &[(&str, i32, &[(i32, i32, u32)])] = &[
    ("ok", 0, &[(3, 3, 0xFFFFFF)]),
    ("ok", 40, &[(147, 47, 0x12E243), (152, 62, 0x12E243), (137, 72, 0x12E243), (107, 82, 0x12E243), (67, 92, 0x12E243), (107, 97, 0x12E243), (92, 112, 0x12E243), (3, 3, 0xFFFFFF)]),
    ("ok", 100, &[(122, 62, 0x12E243), (122, 67, 0x12E243), (72, 77, 0x12E243), (112, 77, 0x12E243), (102, 82, 0x12E243), (87, 87, 0x12E243), (87, 92, 0x12E243), (92, 97, 0x12E243), (3, 3, 0xFFFFFF)]),
    ("ok", 164, &[(122, 62, 0x12E243), (122, 67, 0x12E243), (72, 77, 0x12E243), (112, 77, 0x12E243), (102, 82, 0x12E243), (87, 87, 0x12E243), (87, 92, 0x12E243), (92, 97, 0x12E243), (3, 3, 0xFFFFFF)]),
    ("ok_colors", 100, &[(122, 62, 0xD63384), (122, 67, 0xD63384), (72, 77, 0xD63384), (112, 77, 0xD63384), (102, 82, 0xD63384), (87, 87, 0xD63384), (87, 92, 0xD63384), (92, 97, 0xD63384), (3, 3, 0xFFFFFF)]),
    ("ok_tint", 100, &[(122, 62, 0x20C997), (122, 67, 0x20C997), (72, 77, 0x20C997), (112, 77, 0x20C997), (102, 82, 0x20C997), (87, 87, 0x20C997), (87, 92, 0x20C997), (92, 97, 0x20C997), (3, 3, 0xFFFFFF)]),
    ("shield", 15, &[(92, 17, 0xD8EEFF), (87, 47, 0x7CC5FF), (97, 72, 0x32A4FF), (77, 42, 0xD8EEFF), (52, 62, 0xD8EEFF), (122, 77, 0x7CC5FF), (52, 97, 0xD8EEFF), (132, 112, 0xD8EEFF), (3, 3, 0xFFFFFF)]),
    ("shield", 45, &[(87, 22, 0xCEE9FF), (97, 47, 0x70BFFF), (97, 77, 0x2EA1FF), (57, 47, 0xCEE9FF), (117, 62, 0x70BFFF), (147, 77, 0xCEE9FF), (47, 97, 0xCEE9FF), (137, 112, 0xCEE9FF), (3, 3, 0xFFFFFF)]),
    ("shield", 100, &[(82, 7, 0xFAFDFF), (92, 32, 0xAEDBFF), (92, 62, 0x4FB0FF), (92, 37, 0xAEDBFF), (157, 57, 0xFAFDFF), (147, 77, 0xAEDBFF), (142, 97, 0xAEDBFF), (57, 122, 0xFAFDFF), (3, 3, 0xFFFFFF)]),
    ("shield", 137, &[(87, 32, 0xB6DEFF), (92, 57, 0x55B3FF), (137, 47, 0xB6DEFF), (82, 67, 0x55B3FF), (137, 77, 0xB6DEFF), (97, 92, 0x55B3FF), (127, 107, 0xB6DEFF), (3, 3, 0xFFFFFF)]),
    ("shield_tint", 45, &[(87, 22, 0xC9F3FC), (97, 47, 0x60DCF5), (97, 77, 0x15CCF1), (52, 47, 0xC9F3FC), (112, 62, 0x60DCF5), (142, 77, 0xC9F3FC), (152, 92, 0xC9F3FC), (137, 112, 0xC9F3FC), (3, 3, 0xFFFFFF)]),
    ("f_geometry", 10, &[(142, 17, 0x1A4DE6), (27, 27, 0xE61A1A), (37, 97, 0x1AB333), (132, 32, 0x1A4DE6), (37, 42, 0xE61A1A), (142, 47, 0x1A4DE6), (42, 102, 0x1AB333), (32, 117, 0x1AB333), (3, 3, 0xFFFFFF)]),
    ("f_geometry", 30, &[(32, 22, 0xE61A1A), (127, 22, 0x1A4DE6), (37, 97, 0x1AB333), (37, 32, 0xE61A1A), (32, 42, 0xE61A1A), (132, 47, 0x1A4DE6), (62, 112, 0x1AB333), (47, 132, 0x1AB333), (3, 3, 0xFFFFFF)]),
    ("f_geometry", 59, &[(32, 17, 0xE61A1A), (137, 22, 0x1A4DE6), (37, 97, 0x1AB333), (17, 32, 0xE61A1A), (67, 37, 0xE61A1A), (57, 47, 0xE61A1A), (52, 57, 0xE61A1A), (27, 112, 0x1AB333), (3, 3, 0xFFFFFF)]),
    ("f_geometry_colors", 30, &[(32, 22, 0xFFC107), (127, 22, 0x20C997), (37, 32, 0xFFC107), (32, 42, 0xFFC107), (132, 47, 0x20C997), (37, 97, 0x20C997), (62, 112, 0x20C997), (47, 132, 0x20C997), (3, 3, 0xFFFFFF)]),
    ("f_strokes", 10, &[(137, 87, 0x1A4DE6), (142, 97, 0x1A4DE6), (142, 107, 0x1A4DE6), (142, 117, 0x1A4DE6), (142, 127, 0x1A4DE6), (162, 127, 0x1A4DE6), (147, 132, 0x1A4DE6), (167, 132, 0x1A4DE6), (3, 3, 0xFFFFFF)]),
    ("f_strokes", 30, &[(137, 87, 0x1A4DE6), (162, 87, 0x1A4DE6), (142, 92, 0x1A4DE6), (167, 92, 0x1A4DE6), (142, 102, 0x1A4DE6), (137, 117, 0x1A4DE6), (3, 3, 0xFFFFFF)]),
    ("f_strokes", 59, &[(142, 87, 0x1A4DE6), (167, 87, 0x1A4DE6), (152, 92, 0x1A4DE6), (177, 92, 0x1A4DE6), (182, 102, 0x1A4DE6), (177, 117, 0x1A4DE6), (3, 3, 0xFFFFFF)]),
    ("f_gradients", 10, &[(3, 3, 0xFFFFFF)]),
    ("f_gradients", 30, &[(3, 3, 0xFFFFFF)]),
    ("f_gradients", 59, &[(3, 3, 0xFFFFFF)]),
    ("f_transforms", 10, &[(27, 12, 0xE61A1A), (132, 77, 0x1AB333), (127, 107, 0xFFCE85), (27, 112, 0x000000), (27, 122, 0x1A4DE6), (147, 82, 0x1AB333), (47, 117, 0x000000), (122, 122, 0xFFCE85), (3, 3, 0xFFFFFF)]),
    ("f_transforms", 30, &[(92, 52, 0xE61A1A), (97, 77, 0x1A4DE6), (122, 97, 0x1AB333), (47, 107, 0x000000), (147, 107, 0xFFDBA6), (167, 107, 0xFFDBA6), (37, 117, 0x000000), (47, 122, 0x000000), (3, 3, 0xFFFFFF)]),
    ("f_transforms", 59, &[(177, 17, 0x1A4DE6), (172, 52, 0xE61A1A), (47, 107, 0x000000), (97, 107, 0x1AB333), (127, 107, 0xFFEBCC), (132, 112, 0xFFEBCC), (142, 117, 0xFFEBCC), (147, 122, 0xFFEBCC), (3, 3, 0xFFFFFF)]),
    ("f_precomps", 10, &[(12, 12, 0x3366CC), (162, 12, 0xFF9900), (12, 32, 0xE61A1A), (62, 22, 0x3366CC), (42, 37, 0x3366CC), (157, 47, 0xE61A1A), (122, 62, 0x3366CC), (97, 102, 0x3366CC), (3, 3, 0xFFFFFF)]),
    ("f_precomps", 30, &[(12, 12, 0x3366CC), (17, 32, 0xE61A1A), (67, 22, 0x3366CC), (112, 37, 0xE61A1A), (67, 52, 0x3366CC), (22, 67, 0x3366CC), (97, 107, 0x3366CC), (3, 3, 0xFFFFFF)]),
    ("f_precomps", 59, &[(12, 12, 0x3366CC), (62, 12, 0xFF9900), (57, 32, 0xE61A1A), (147, 17, 0x3366CC), (162, 27, 0x3366CC), (162, 37, 0x3366CC), (162, 47, 0x3366CC), (17, 62, 0x3366CC), (3, 3, 0xFFFFFF)]),
    // The trimmed counterclockwise ellipse: which part of the circle is drawn.
    ("f_geometry", 10, &[(170, 115, 0xFFFFFF), (168, 125, 0xFFFFFF), (163, 134, 0xFFFFFF), (155, 141, 0xFFFFFF), (145, 145, 0xFFFFFF), (135, 145, 0xFFFFFF), (125, 141, 0xFFFFFF), (110, 115, 0xFF9900), (112, 105, 0xFF9900), (117, 96, 0xFF9900), (125, 89, 0xFF9900), (135, 85, 0xFFFFFF), (145, 85, 0xFFFFFF), (155, 89, 0xFFFFFF), (163, 96, 0xFFFFFF), (168, 105, 0xFFFFFF)]),
    // The trimmed counterclockwise ellipse: which part of the circle is drawn.
    ("f_geometry", 30, &[(170, 115, 0xFFFFFF), (168, 125, 0xFFFFFF), (163, 134, 0xFFFFFF), (155, 141, 0xFF9900), (145, 145, 0xFF9900), (135, 145, 0xFF9900), (125, 141, 0xFF9900), (117, 134, 0xFF9900), (112, 125, 0xFF9900), (110, 115, 0xFF9900), (112, 105, 0xFF9900), (117, 96, 0xFF9900), (125, 89, 0xFF9900), (135, 85, 0xFFFFFF), (145, 85, 0xFFFFFF), (155, 89, 0xFFFFFF), (163, 96, 0xFFFFFF), (168, 105, 0xFFFFFF)]),
    // The trimmed counterclockwise ellipse: which part of the circle is drawn.
    ("f_geometry", 59, &[(170, 115, 0xFF9900), (168, 125, 0xFF9900), (163, 134, 0xFF9900), (155, 141, 0xFF9900), (145, 145, 0xFF9900), (135, 145, 0xFF9900), (125, 141, 0xFF9900), (117, 134, 0xFF9900), (112, 125, 0xFF9900), (110, 115, 0xFF9900), (112, 105, 0xFF9900), (117, 96, 0xFF9900), (125, 89, 0xFF9900), (135, 85, 0xFFFFFF), (145, 85, 0xFF9900), (155, 89, 0xFF9900), (163, 96, 0xFF9900), (168, 105, 0xFF9900)]),
    // Two circles under a 50% group: one layer, the overlap no darker.
    ("f_transforms", 10, &[(150, 120, 0xFFCE85), (165, 120, 0xFFCE85)]),
    // Two circles under a 50% group: one layer, the overlap no darker.
    ("f_transforms", 30, &[(150, 120, 0xFFDBA6), (165, 120, 0xFFDBA6)]),
    // Two circles under a 50% group: one layer, the overlap no darker.
    ("f_transforms", 59, &[(150, 120, 0xFFEBCC), (165, 120, 0xFFEBCC)]),
];

#[test]
fn frames_match_skottie() {
    for &(name, frame, points) in SKOTTIE {
        let (json, tints) = case(name);
        let mut host = render(json, tints, frame);
        for &(x, y, rgb) in points {
            let expected = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
            let got = host.pixel(x, y);
            assert!(channel_diff(got, expected) <= 3, "{name} frame {frame} at ({x}, {y}): {got:?}, Skottie {rgb:06X}");
        }
    }
}

#[test]
#[ignore = "measurement: needs LOTTIE_REF with the CanvasKit frames"]
fn compare_with_skottie() {
    let dir = std::env::var("LOTTIE_REF").expect("LOTTIE_REF");
    const FEATURE_FRAMES: &[i32] = &[0, 10, 20, 25, 30, 45, 59, 60];
    let cases: [(&str, &'static str, &[&str], &[i32]); 11] = [
        ("ok", OK, &[], &[0, 20, 40, 62, 65, 75, 100, 106, 163, 164]),
        ("ok_colors", OK, &["#D63384", "#FFC107"], &[0, 100]),
        ("ok_tint", OK, &["#20C997"], &[100]),
        ("shield", SHIELD, &[], &[0, 15, 30, 45, 60, 80, 100, 120, 136, 137]),
        ("shield_tint", SHIELD, &["#0DCAF0"], &[45]),
        ("f_geometry", GEOMETRY, &[], FEATURE_FRAMES),
        ("f_geometry_colors", GEOMETRY, &["#D63384", "#FFC107", "#20C997"], &[30]),
        ("f_strokes", STROKES, &[], FEATURE_FRAMES),
        ("f_gradients", GRADIENTS, &[], FEATURE_FRAMES),
        ("f_transforms", TRANSFORMS, &[], FEATURE_FRAMES),
        ("f_precomps", PRECOMPS, &[], FEATURE_FRAMES),
    ];
    for (name, json, tints, frames) in cases {
        for &frame in frames {
            let reference = std::fs::read(format!("{dir}/{name}_{frame}.rgba")).expect("reference frame");
            let mut host = render(json, tints, frame);
            host.save_png(&format!("{dir}/{name}_{frame}.rust.png"));
            let (mut max, mut sum, mut over) = (0u8, 0u64, 0);
            for y in 0..H {
                for x in 0..W {
                    let i = ((y * W + x) * 4) as usize;
                    let d = channel_diff(host.pixel(x, y), reference[i..i + 4].try_into().unwrap());
                    (max, sum) = (max.max(d), sum + d as u64);
                    over += (d > 24) as u32;
                }
            }
            println!("{name} frame {frame}: max {max}, mean {:.3}, pixels over 24: {over}", sum as f64 / (W * H) as f64);
        }
    }
}
