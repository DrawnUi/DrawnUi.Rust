//! Tests ported from DrawnUi.Net.Tests, same scenes and numbers: ShaderEffectSnapshotTests,
//! AspectCoverHeightDrivenTests (the cases with a shader effect) and ShaderEffectPerfTests (an
//! ignored measurement). Upstream sets the bitmap with SetImageInternal; here it arrives through
//! the manager as a PNG.

mod image_common;

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::skia::Canvas;
use drawnui::testing::Headless;
use image_common::*;

const GREEN: Color = Color::from_argb(255, 20, 60, 20);

/// The pass-through shader of the upstream tests.
const PASSTHROUGH: &str = "
uniform float4 iMouse;
uniform float  iTime;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform shader iImage1;
uniform float2 iOffset;
uniform float2 iOrigin;

half4 main(float2 fragCoord)
{
    float2 renderingScale = iImageResolution.xy / iResolution.xy;
    float2 inputCoord = (fragCoord - iOffset) * renderingScale;
    return iImage1.eval(inputCoord);
}
";

/// A square green source of `size` with a red stripe `2 * half` wide at its horizontal center.
fn striped(size: i32, half: f32) -> Vec<u8> {
    png(size, size, |canvas: &Canvas| {
        canvas.clear(GREEN);
        fill(canvas, Rect::from_xywh(size as f32 / 2.0 - half, 0.0, 2.0 * half, size as f32), Color::RED);
    })
}

/// The upstream scene: a tile of `size` at `at` holding a SkiaLayer holding a SkiaImage
/// (AspectCover, filling it) with a pass-through shader, the source delivered.
fn host(canvas: (i32, i32), tile: (f32, f32), at: (f32, f32), cache: CacheType, source: Vec<u8>) -> Headless<()> {
    let image = SkiaImage::new("source.png").aspect(TransformAspect::AspectCover).fill();
    let image = image.visual_effect(SkiaShaderEffect::new().shader_code(PASSTHROUGH));
    let tile = SkiaShape::new().width_request(tile.0).height_request(tile.1).margin((at.0, at.1, 0.0, 0.0)).use_cache(cache);
    let tile = tile.children((SkiaLayout::layer().fill_y().children((image,)),));
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children((tile,))).background(Color::BLACK);
    let mut host = Headless::new(ui, canvas.0, canvas.1, 1.0);
    host.settle();
    assert_eq!(deliver(&mut host, |_| Some(source.clone())), ["source.png"]);
    host.settle();
    host
}

/// The middle of the red run of a row between `left` and `right` (upstream RedCenterX).
fn red_center(host: &mut Headless<()>, left: i32, right: i32, y: i32) -> Option<f32> {
    let is_red = |c: Color| c.r() > 120 && c.g() < 90 && c.b() < 90;
    let red: Vec<i32> = (left..right).filter(|x| is_red(host.pixel(*x, y))).collect();
    Some((*red.first()? + *red.last()?) as f32 / 2.0)
}

/// ShaderEffectSnapshotTests.ShaderOnUncachedImage_SamplesItsOwnPixels: the shader of an
/// uncached image takes its texture from a snapshot in the space of the surface it is drawn into
/// (the window, or the tile's cache), wherever the tile is.
#[test]
fn a_shader_on_an_uncached_image_samples_its_own_pixels() {
    const TILE: (f32, f32) = (180.0, 120.0);
    for (cache, offset) in [(CacheType::None, 0.0), (CacheType::None, 200.0), (CacheType::Image, 0.0), (CacheType::Image, 200.0)] {
        let mut host = host((400, 300), TILE, (offset, 40.0), cache, striped(400, 8.0));
        let marker = red_center(&mut host, offset as i32, (offset + TILE.0) as i32, 90);
        let expected = offset + TILE.0 / 2.0;
        let marker = marker.unwrap_or_else(|| panic!("nothing rendered ({cache:?}, {offset})"));
        assert!((marker - expected).abs() <= 3.0, "{cache:?}, {offset}: shifted by {}", marker - expected);
    }
}

/// AspectCoverHeightDrivenTests.SquareSource_StaysCentered, with the tile's shader effect: the
/// subject stays at the center of an image-cached tile, also when the height drives the cover.
#[test]
fn a_square_source_stays_centered_under_a_shader_effect() {
    const LEFT: f32 = 32.0;
    for (w, h) in [(270.0, 118.0), (270.0, 310.0), (270.0, 400.0)] {
        let mut host = host((400, 500), (w, h), (LEFT, 20.0), CacheType::Image, striped(1088, 10.0));
        let marker = red_center(&mut host, LEFT as i32, (LEFT + w) as i32, (20.0 + h / 2.0) as i32).expect("marker");
        let expected = LEFT + w / 2.0;
        assert!((marker - expected).abs() <= 2.0, "tile {w} x {h}: subject off center by {}", marker - expected);
    }
}

/// ShaderEffectPerfTests.PerFrameShader_SnapshotPathCost: a frame of an uncached image with a
/// per-frame shader, a box matching the drawn picture and one the picture overflows. Upstream
/// reuses the image's scaled bitmap in the first case; here both take a snapshot per frame.
/// `cargo test --release -p drawnui --test effects_upstream -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn per_frame_shader_snapshot_path_cost() {
    const FRAMES: u32 = 200;
    let source = png(1088, 1088, |canvas: &Canvas| {
        canvas.clear(Color::from_rgb(30, 90, 30));
    });
    let mut measured = Vec::new();
    for (w, h) in [(270.0, 118.0), (270.0, 400.0)] {
        let mut host = host((400, 500), (w, h), (32.0, 20.0), CacheType::None, source.clone());
        for _ in 0..5 {
            host.frame_after(16.0);
        }
        let start = Instant::now();
        for _ in 0..FRAMES {
            host.frame_after(16.0);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64;
        println!("{w}x{h}  snapshot per frame  {ms:.3} ms/frame");
        measured.push(ms);
    }
    println!("delta = {:.3} ms/frame ({:.2}x)", measured[1] - measured[0], measured[1] / measured[0]);
}
