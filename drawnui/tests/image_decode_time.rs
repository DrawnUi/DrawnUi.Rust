//! A measurement, not a test: what a picture costs, and on which thread. `Images::decode` is
//! what a host runs on a worker (the full size, and the size of a 400 x 500 pixel tile);
//! `App::image` is what is left for the frame thread when the decoded picture arrives.
//!
//! `cargo test --release -p drawnui --test image_decode_time -- --ignored --nocapture`
//! `DRAWNUI_PHOTOS` = paths of real photos, `;` separated, timed next to the generated ones.

use std::time::Instant;

use drawnui::App as _;
use drawnui::Images;
use drawnui::prelude::*;
use drawnui::skia::{EncodedImageFormat, ImageInfo, Paint, shaders, surfaces};
use drawnui::testing::Headless;

/// JPEG bytes of a picture with detail at every scale, as a photo has: fractal noise over a color.
fn photo(width: i32, height: i32) -> Vec<u8> {
    let mut surface = surfaces::raster(&ImageInfo::new_n32_premul((width, height), None), None, None).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(Color::from_rgb(90, 120, 150));
    let mut paint = Paint::default();
    paint.set_shader(shaders::fractal_noise((0.01, 0.01), 5, 7.0, None));
    paint.set_alpha(160);
    canvas.draw_paint(&paint);
    surface.image_snapshot().encode(None, EncodedImageFormat::JPEG, 85).expect("jpeg").as_bytes().to_vec()
}

fn median(mut ms: Vec<f64>) -> f64 {
    ms.sort_by(f64::total_cmp);
    ms[ms.len() / 2]
}

/// Milliseconds `f` takes.
fn timed<T>(ms: &mut Vec<f64>, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = f();
    ms.push(started.elapsed().as_secs_f64() * 1000.0);
    result
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn decode_time_of_typical_photos() {
    const RUNS: usize = 9;
    let sizes = [(1080, 1350), (1920, 1080), (4032, 3024)];
    let mut photos: Vec<(String, Vec<u8>)> =
        sizes.iter().map(|(w, h)| (format!("generated {w} x {h}"), photo(*w, *h))).collect();
    for path in std::env::var("DRAWNUI_PHOTOS").unwrap_or_default().split(';').filter(|p| !p.is_empty()) {
        photos.push((path.to_owned(), std::fs::read(path).expect("photo file")));
    }
    for (name, bytes) in photos {
        let (mut full_ms, mut tile_ms, mut frame_ms) = (Vec::new(), Vec::new(), Vec::new());
        let mut size = (0, 0);
        for run in 0..RUNS {
            let image = SkiaImage::new(format!("{run}.jpg")).fill().aspect(TransformAspect::AspectFill);
            let ui = Ui::new((), |_: &mut ()| SkiaLayout::new().fill().children((image,)));
            let mut host = Headless::new(ui, 400, 500, 1.0);
            host.settle();
            let request = host.ui.tree.images.take_requests().remove(0);
            timed(&mut tile_ms, || Images::decode(&bytes, 400, 500).expect("decoded"));
            let full = timed(&mut full_ms, || Images::decode(&bytes, 0, 0).expect("decoded"));
            size = (full.image.width(), full.image.height());
            timed(&mut frame_ms, || host.ui.image(request.id, Some(full)));
        }
        println!(
            "{name}: {} x {}, {} KB | worker: decode {:.1} ms, for a 400 x 500 tile {:.1} ms | frame thread: {:.3} ms",
            size.0,
            size.1,
            bytes.len() / 1024,
            median(full_ms),
            median(tile_ms),
            median(frame_ms),
        );
    }
}
