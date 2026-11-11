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

/// DrawnUi.Net's reference setup (drawnui-cross 6bm): 256 x 160 at scale 1, the label filling the
/// width, text centered, Orbitron-Black 38 unless `setup` changes it. The ink rows between `y0` and
/// `y1` and the label's height (a blue background).
fn draw_net(text: &'static str, setup: impl Fn(Build<SkiaLabel>) -> Build<SkiaLabel> + 'static, y0: i32, y1: i32) -> ((i32, i32), i32) {
    let ui = Ui::new(App, move |_app: &mut App| {
        let label = SkiaLabel::new(text).font_family("Orbitron").font_size(38).text_color(Color::WHITE).background_color(BOX).fill_x().horizontal_text_alignment(TextAlignment::Center);
        SkiaLayout::new().fill().children((setup(label),))
    })
    .font_bytes("Orbitron", ORBITRON_BLACK)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 256, 160, 1.0);
    host.settle();
    let rows = ink_rows(|x, y| host.pixel(x, y0 + y), 256, y1 - y0).expect("the label draws");
    let height = (0..160).filter(|&y| host.pixel(2, y).b() > 127).count() as i32;
    ((rows.0 + y0, rows.1 + y0), height)
}

/// Rows equal to DrawnUi.Net's, the top row within one (antialiasing differs).
fn assert_rows(name: &str, (rows, height): ((i32, i32), i32), net: ((i32, i32), i32)) {
    assert_eq!((rows.1, height), (net.0 .1, net.1), "{name}: bottom row and height, {rows:?}");
    assert!((rows.0 - net.0 .0).abs() <= 1, "{name}: top row {} vs .NET {}", rows.0, net.0 .0);
}

#[test]
fn rows_as_dotnet_draws_them() {
    // DrawnUi 296a144c: "GAME" of "GAME OVER" centered, rows 43-70, 96 high.
    assert_rows("GAME OVER", draw_net("GAME OVER", |l| l.vertical_options(LayoutOptions::Center), 0, 80), ((43, 70), 96));
    // Lines of 18 and 50: "OVER" rows 5-14, "GAME" rows 29-57, 68 high.
    let mixed = |y0, y1| draw_net("", |l| l.font_size(14).paragraph_spacing(0).spans(vec![TextSpan::new("OVER\n"), TextSpan::new("GAME").font_size(40)]), y0, y1);
    assert_rows("OVER", mixed(0, 18), ((5, 14), 68));
    assert_rows("GAME", mixed(18, 68), ((29, 57), 68));
    assert_rows("LineHeight 1.5", draw_net("GAME", |l| l.line_height(1.5), 0, 160), ((22, 49), 71));
}

#[test]
fn paragraphs_part_by_a_quarter_line() {
    // DrawnUi.Net's default ParagraphSpacing 0.25: a quarter of the label font's line (48 px) above a
    // line after '\n'; a wrapped line adds none.
    assert_eq!(draw(0.0, "GAME\nOVER", |l| l).1, 48 + 12 + 48);
    assert_eq!(draw(0.0, "GAME\nOVER", |l| l.paragraph_spacing(0)).1, 96);
    let wrapped = draw(0.0, "GAME OVER", |l| l.width_request(200)).0;
    assert_eq!(wrapped.1, draw(0.0, "GAME\nOVER", |l| l.paragraph_spacing(0)).0 .1);
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
        draw(0.0, "", move |l| l.font_size(20).paragraph_spacing(0).line_height_uniform(uniform).spans(vec![TextSpan::new("OVER\n"), TextSpan::new("GAME").font_size(38)]))
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
    // The label's own font is a strut: a line of a small span is at least its line (48 px).
    assert_eq!(draw(0.0, "", |l| l.spans(vec![TextSpan::new("go").font_size(12)])).1, 48);
}

#[test]
fn a_font_with_leading_puts_it_between_lines() {
    let gapped = gapped_font();
    let (a, d, lead) = metrics(&gapped, 38.0);
    let line = (a + d).round();
    assert!(lead > 0.0, "the patched font has a leading");
    // Paragraph spacing off: the gap alone.
    let two = |l: Build<SkiaLabel>| l.paragraph_spacing(0);
    assert_eq!(draw(0.0, "GAME\nOVER", move |l| two(l).font_family("OrbitronGap")).1, (2.0 * line + lead).ceil() as i32);
    assert_eq!(draw(0.0, "GAME\nOVER", move |l| two(l).font_family("OrbitronGap").line_spacing(1.5)).1, (2.0 * line + lead * 1.5).ceil() as i32);
    // No leading: LineSpacing 1.5 adds half a line.
    assert_eq!(draw(0.0, "GAME\nOVER", move |l| two(l).line_spacing(1.5)).1, 48 * 2 + 24);
}

/// The lit pixels (red above 128) of "GAME" drawn by Skia alone in Orbitron-Black at `size` px, at
/// (`x`, `baseline`), on a `w` x `h` canvas.
fn skia_ink(size: f32, x: f32, baseline: f32, w: i32, h: i32) -> Vec<(i32, i32)> {
    let typeface = FontMgr::new().new_from_data(Data::new_copy(ORBITRON_BLACK), None).expect("the font");
    let mut font = Font::from_typeface(typeface, size);
    font.set_subpixel(true).set_linear_metrics(true).set_edging(skia::font::Edging::SubpixelAntiAlias);
    let mut surface = skia::surfaces::raster_n32_premul((w, h)).expect("a surface");
    surface.canvas().clear(Color::BLACK);
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_color(Color::WHITE);
    surface.canvas().draw_str("GAME", (x, baseline), &font, &paint);
    let info = skia::ImageInfo::new((w, h), skia::ColorType::RGBA8888, skia::AlphaType::Premul, None);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    assert!(surface.image_snapshot().read_pixels(&info, &mut pixels, (w * 4) as usize, (0, 0), skia::image::CachingHint::Disallow));
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| pixels[((y * w + x) * 4) as usize] > 128).collect()
}

#[test]
fn a_fractional_font_size_draws_at_whole_pixels() {
    // DrawnUi.Net: Math.Round(FontSize * scale): 13.7 at scale 1.25 is a 17 px font, not 17.125.
    let ui = Ui::new(App, |_app: &mut App| {
        SkiaLayout::new().fill().children((SkiaLabel::new("GAME").font_family("Orbitron").font_size(13.7).text_color(Color::WHITE).left(8),))
    })
    .font_bytes("Orbitron", ORBITRON_BLACK)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 100, 1.25);
    host.settle();
    let drawn: Vec<(i32, i32)> = (0..100).flat_map(|y| (0..300).map(move |x| (x, y))).filter(|&(x, y)| host.pixel(x, y).r() > 128).collect();
    let (a, d, _) = metrics(ORBITRON_BLACK, 17.0);
    let baseline = (a + d).round() - d;
    let whole = skia_ink(17.0, 10.0, baseline, 300, 100);
    assert_ne!(whole, skia_ink(17.125, 10.0, baseline, 300, 100), "the two sizes draw apart");
    assert_eq!(drawn, whole);
}
