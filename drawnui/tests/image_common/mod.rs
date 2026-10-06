//! Shared by the image tests: generated PNGs and the asset round trip of the headless host.
#![allow(dead_code)]

use drawnui::prelude::*;
use drawnui::{Decoded, ImageRequest, Images};
use drawnui::skia::{Canvas, EncodedImageFormat, ImageInfo, Paint, surfaces};
use drawnui::testing::Headless;

/// The four quadrants of `quadrants`: top left, top right, bottom left, bottom right.
pub const QUADRANTS: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

fn encoded(size: (i32, i32), format: EncodedImageFormat, draw: impl FnOnce(&Canvas)) -> Vec<u8> {
    let mut surface = surfaces::raster(&ImageInfo::new_n32_premul(size, None), None, None).expect("surface");
    draw(surface.canvas());
    surface.image_snapshot().encode(None, format, 90).expect("encoded").as_bytes().to_vec()
}

/// PNG bytes of a `width` x `height` picture painted by `draw`.
pub fn png(width: i32, height: i32, draw: impl FnOnce(&Canvas)) -> Vec<u8> {
    encoded((width, height), EncodedImageFormat::PNG, draw)
}

/// JPEG bytes of a `width` x `height` picture painted by `draw`.
pub fn jpeg(width: i32, height: i32, draw: impl FnOnce(&Canvas)) -> Vec<u8> {
    encoded((width, height), EncodedImageFormat::JPEG, draw)
}

/// The JPEG with an EXIF orientation (1 to 8) written into its header.
pub fn with_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
    // APP1: "Exif", a little-endian TIFF header, one IFD with the entry 0x0112 (SHORT, 1 value).
    let mut exif = vec![0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 0x2A, 0, 8, 0, 0, 0, 1, 0];
    exif.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, orientation, 0, 0, 0, 0, 0, 0, 0]);
    // Right after the start-of-image marker.
    [&jpeg[..2], &exif, &jpeg[2..]].concat()
}

pub fn fill(canvas: &Canvas, rect: Rect, color: Color) {
    let mut paint = Paint::default();
    paint.set_color(color);
    canvas.draw_rect(rect, &paint);
}

fn draw_quadrants(canvas: &Canvas, width: i32, height: i32) {
    let (w, h) = (width as f32 / 2.0, height as f32 / 2.0);
    for (i, color) in QUADRANTS.into_iter().enumerate() {
        fill(canvas, Rect::from_xywh((i % 2) as f32 * w, (i / 2) as f32 * h, w, h), color);
    }
}

/// A picture of four quadrants in distinct colors, so crops and fits are visible.
pub fn quadrants(width: i32, height: i32) -> Vec<u8> {
    png(width, height, |canvas| draw_quadrants(canvas, width, height))
}

pub fn jpeg_quadrants(width: i32, height: i32) -> Vec<u8> {
    jpeg(width, height, |canvas| draw_quadrants(canvas, width, height))
}

/// A picture of one color.
pub fn solid(color: Color) -> Vec<u8> {
    png(8, 8, |canvas| {
        canvas.clear(color);
    })
}

/// Answers every request the manager has for the host: `bytes_of(source)`, `None` = the load
/// fails. Returns the sources answered, in request order.
pub fn deliver<S>(host: &mut Headless<S>, bytes_of: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<String> {
    host.deliver_images(bytes_of).into_iter().map(|request| request.source).collect()
}

/// What a host answers a request with: the bytes of its source, decoded at the size asked for.
pub fn decode(request: &ImageRequest, bytes_of: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Decoded> {
    Images::decode(&bytes_of(&request.source)?, request.width, request.height)
}

/// What a pixel must show when a quadrants picture is drawn into `display`, cropped to `bounds`,
/// over `background`. `None` for a pixel too close to an edge or to a line between quadrants to
/// be one clean color. `x` and `y` are the pixel's center.
pub fn expected(display: Rect, bounds: Rect, background: Color, x: f32, y: f32) -> Option<Color> {
    const CLEAR: f32 = 3.0;
    let (cx, cy) = (display.center_x(), display.center_y());
    let clean = |v: f32, lines: [f32; 5]| lines.iter().all(|line| (v - line).abs() >= CLEAR);
    if !clean(x, [display.left, cx, display.right, bounds.left, bounds.right])
        || !clean(y, [display.top, cy, display.bottom, bounds.top, bounds.bottom])
    {
        return None;
    }
    let inside = |r: Rect| x > r.left && x < r.right && y > r.top && y < r.bottom;
    let quadrant = QUADRANTS[(x > cx) as usize + 2 * (y > cy) as usize];
    Some(if inside(display) && inside(bounds) { quadrant } else { background })
}

/// A color per number, never the black of the test canvas.
pub fn color(n: usize) -> Color {
    Color::from_argb(255, (40 + n * 53 % 200) as u8, (n * 97 % 256) as u8, (200 - n * 31 % 150) as u8)
}

/// Bytes of the source "<n>.png" (any folder): a picture of `color(n)`. Every other source fails.
pub fn numbered(source: &str) -> Option<Vec<u8>> {
    let n = source.strip_suffix(".png")?.rsplit('/').next()?.parse().ok()?;
    Some(solid(color(n)))
}

/// Answers requests until the manager has none left, a frame after every round.
pub fn deliver_all<S>(host: &mut Headless<S>, bytes_of: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<String> {
    let mut answered = Vec::new();
    loop {
        let round = deliver(host, &bytes_of);
        if round.is_empty() {
            return answered;
        }
        answered.extend(round);
        host.settle();
    }
}
