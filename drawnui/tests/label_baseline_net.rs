//! Label lines as in every DrawnUI engine (drawnui-cross 6bm): each line is as tall as its own
//! spans, round((largest ascent + largest descent) * LineHeight); the baseline is the line's descent
//! above its bottom, the room a LineHeight above 1 adds split above and below the text; a font with
//! a leading puts leading * LineSpacing between lines; `line_height_uniform` makes every line as
//! tall as the tallest. Orbitron-Black 38: ascent 38.418, descent 9.234, lines of 48 px.

use drawnui::prelude::*;
use drawnui::skia::{self, Data, Font, FontMgr, Paint};
use drawnui::testing::Headless;

const ORBITRON_BLACK: &[u8] = include_bytes!("../../examples/dungeon/assets/Orbitron-Black.ttf");
const BOX: Color = Color::from_rgb(0, 0, 255);

/// The rows with ink (red above 128, as DrawnUi.React's check), first and last.
fn ink_rows(mut pixel: impl FnMut(i32, i32) -> Color, width: i32, height: i32) -> Option<(i32, i32)> {
    let rows: Vec<i32> = (0..height).filter(|&y| (0..width).any(|x| pixel(x, y).r() > 128)).collect();
    Some((*rows.first()?, *rows.last()?))
}

/// Orbitron-Black with a line gap of 0.2 em (hhea and OS/2 typo line gap): a font with a leading.
fn gapped_font() -> Vec<u8> {
    let mut f = ORBITRON_BLACK.to_vec();
    let be16 = |f: &[u8], at: usize| u16::from_be_bytes([f[at], f[at + 1]]) as usize;
    let be32 = |f: &[u8], at: usize| u32::from_be_bytes([f[at], f[at + 1], f[at + 2], f[at + 3]]) as usize;
    for i in 0..be16(&f, 4) {
        let rec = 12 + i * 16;
        let (tag, offset) = (f[rec..rec + 4].to_vec(), be32(&f, rec + 8));
        let at = match &tag[..] {
            b"hhea" => offset + 8,
            b"OS/2" => offset + 72,
            _ => continue,
        };
        f[at..at + 2].copy_from_slice(&200i16.to_be_bytes());
    }
    f
}

#[derive(Default)]
struct App;

/// A label (white text, blue box 300 px wide) at the canvas' top-left + `top`, Orbitron-Black 38
/// unless `setup` changes it: the rows of its ink and its height in pixels.
fn draw(top: f32, text: &'static str, setup: impl Fn(Build<SkiaLabel>) -> Build<SkiaLabel> + 'static) -> ((i32, i32), i32) {
    let ui = Ui::new(App, move |_app: &mut App| {
        let label = SkiaLabel::new(text).font_family("Orbitron").font_size(38).text_color(Color::WHITE).background_color(BOX).left(10).top(top).width_request(300);
        SkiaLayout::new().fill().children((setup(label),))
    })
    .font_bytes("Orbitron", ORBITRON_BLACK)
    .font_bytes("OrbitronGap", &gapped_font())
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    let rows = ink_rows(|x, y| host.pixel(x, y), 400, 200).expect("the label draws");
    let height = (0..200).filter(|&y| host.pixel(305, y).b() > 127).count() as i32;
    (rows, height)
}

fn metrics(font: &[u8], size: f32) -> (f32, f32, f32) {
    let typeface = FontMgr::new().new_from_data(Data::new_copy(font), None).expect("the font");
    let (_, m) = Font::from_typeface(typeface, size).metrics();
    (-m.ascent, m.descent, m.leading)
}

/// "GAME" drawn by Skia alone at the baseline round(ascent + descent) - descent below `top`, on a
/// whole pixel as DrawnUi.Net draws it.
fn reference_rows(top: f32) -> ((i32, i32), f32) {
    let (a, d, _) = metrics(ORBITRON_BLACK, 38.0);
    let baseline = (top + (a + d).round() - d).round();
    (reference_at(baseline), baseline)
}

/// "GAME" drawn by Skia alone at `baseline`: its ink rows.
fn reference_at(baseline: f32) -> (i32, i32) {
    let typeface = FontMgr::new().new_from_data(Data::new_copy(ORBITRON_BLACK), None).expect("the font");
    let mut font = Font::from_typeface(typeface, 38.0);
    font.set_subpixel(true).set_linear_metrics(true).set_edging(skia::font::Edging::SubpixelAntiAlias);
    let mut surface = skia::surfaces::raster_n32_premul((400, 120)).expect("a surface");
    surface.canvas().clear(Color::BLACK);
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_color(Color::WHITE);
    surface.canvas().draw_str("GAME", (10.0, baseline), &font, &paint);
    let image = surface.image_snapshot();
    let info = skia::ImageInfo::new((400, 120), skia::ColorType::RGBA8888, skia::AlphaType::Premul, None);
    let mut pixels = vec![0u8; 400 * 120 * 4];
    assert!(image.read_pixels(&info, &mut pixels, 400 * 4, (0, 0), skia::image::CachingHint::Disallow));
    let at = |x: i32, y: i32| {
        let i = ((y * 400 + x) * 4) as usize;
        Color::from_argb(pixels[i + 3], pixels[i], pixels[i + 1], pixels[i + 2])
    };
    ink_rows(at, 400, 120).expect("skia draws")
}

#[test]
fn game_sits_on_the_rounded_line() {
    for top in [0.0, 13.0, 22.5, 31.25] {
        let (expected, baseline) = reference_rows(top);
        assert_eq!(draw(top, "GAME", |l| l).0, expected, "top {top}: baseline {baseline}");
    }
    // One line at the top: 48 px, baseline 48 - 9.234 = 38.766 (DrawnUi.React's check: rows 12-38
    // there, 11-38 here, a faint antialiased top row).
    assert_eq!(draw(0.0, "GAME", |l| l), (reference_at(39.0), 48));
}

#[test]
fn game_over_rows_in_the_dungeon_probe() {
    // The Dungeon GLOW probe: .NET's baseline 70.766, plain glyphs on rows 43-70 (its glow mask 44-70).
    let (a, d, _) = metrics(ORBITRON_BLACK, 38.0);
    let top = 70.766 - (a + d).round() + d;
    let (reference, baseline) = reference_rows(top);
    assert_eq!((reference, baseline), ((43, 70), 71.0), "Skia at the .NET baseline");
    assert_eq!(draw(top, "GAME", |l| l).0, reference);
}

#[test]
fn line_height_splits_its_room_above_and_below() {
    // round(47.652 * 1.5) = 71 px; the text's own 47.33 px centered in it: baseline 71 - 11.83 -
    // 9.234 = 49.93 (DrawnUi.React's check: rows 23-49).
    assert_eq!(draw(0.0, "GAME", |l| l.line_height(1.5)), (reference_at(50.0), 71));
}

#[test]
fn each_line_takes_its_own_height() {
    let (a20, d20, _) = metrics(ORBITRON_BLACK, 20.0);
    let own20 = (a20 + d20).round() as i32;
    let mixed = |uniform: bool| {
        draw(0.0, "", move |l| l.font_size(20).line_height_uniform(uniform).spans(vec![TextSpan::new("OVER\n"), TextSpan::new("GAME").font_size(38)]))
    };
    let (own, uniform) = (mixed(false), mixed(true));
    assert_eq!(own.1, own20 + 48, "a 20 px line and a 38 px line, each its own height");
    assert_eq!(uniform.1, 96, "line_height_uniform: both 48 px");
    // The first line's baseline is its bottom minus its descent: in its own line it sits higher.
    assert!(own.0 .0 < uniform.0 .0, "{own:?} {uniform:?}");
    // A line of two fonts: round(largest ascent + largest descent), one rounding.
    let (a44, d44, _) = metrics(ORBITRON_BLACK, 44.0);
    let (a40, d40, _) = metrics(ORBITRON_BLACK, 40.0);
    let two = draw(0.0, "", |l| l.font_size(10).spans(vec![TextSpan::new("GA").font_size(44), TextSpan::new("ME").font_size(40)]));
    assert_eq!(two.1, (a44.max(a40) + d44.max(d40)).round() as i32);
}

#[test]
fn a_font_with_leading_puts_it_between_lines() {
    let gapped = gapped_font();
    let (a, d, lead) = metrics(&gapped, 38.0);
    let line = (a + d).round();
    assert!(lead > 0.0, "the patched font has a leading");
    assert_eq!(draw(0.0, "GAME\nOVER", |l| l.font_family("OrbitronGap")).1, (2.0 * line + lead).ceil() as i32);
    assert_eq!(draw(0.0, "GAME\nOVER", |l| l.font_family("OrbitronGap").line_spacing(1.5)).1, (2.0 * line + lead * 1.5).ceil() as i32);
    // No leading: LineSpacing 1.5 adds half a line.
    assert_eq!(draw(0.0, "GAME\nOVER", |l| l.line_spacing(1.5)).1, 48 * 2 + 24);
}
