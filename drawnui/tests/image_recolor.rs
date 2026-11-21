//! SkiaImageEffect::Recolor (C# ImageRecolorTests): the picture's lightness in the tint's colors,
//! each channel tint x ((luma - 0.5) x contrast + 0.5); on SkiaImageTiles the effect covers the
//! whole tiled picture.

mod image_common;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::*;

#[derive(Default)]
struct App;

/// A `size` x `size` picture, its left half gray 64 and its right half gray 191.
fn two_grays(size: i32) -> Vec<u8> {
    png(size, size, |canvas| {
        canvas.clear(Color::from_rgb(64, 64, 64));
        fill(canvas, Rect::from_xywh(size as f32 / 2.0, 0.0, size as f32 / 2.0, size as f32), Color::from_rgb(191, 191, 191));
    })
}

fn expected(tint: Color, gray: u8, contrast: f32) -> [f32; 3] {
    let k = (gray as f32 / 255.0 - 0.5) * contrast + 0.5;
    [tint.r(), tint.g(), tint.b()].map(|t| (t as f32 * k).clamp(0.0, 255.0))
}

fn close(pixel: Color, want: [f32; 3], at: (i32, i32)) {
    let got = [pixel.r(), pixel.g(), pixel.b()];
    for (g, w) in got.iter().zip(want) {
        assert!((*g as f32 - w).abs() <= 3.0, "at {at:?}: {got:?}, expected {want:?}");
    }
}

fn shown(content: drawnui::Detached, picture: Vec<u8>) -> Headless<App> {
    let ui = Ui::new(App, move |_| SkiaLayout::new().fill().children((content,)));
    let mut host = Headless::new(ui.background(Color::BLACK), 140, 140, 1.0);
    host.settle();
    host.deliver_images(|_| Some(picture.clone()));
    host.settle();
    host
}

#[test]
fn recolor_puts_the_lightness_in_the_tint_with_its_contrast() {
    let tint = Color::from_rgb(97, 56, 140);
    for contrast in [1.0, 2.6] {
        let image = SkiaImage::new("two.png")
            .width_request(100)
            .height_request(100)
            .add_effect(SkiaImageEffect::Recolor)
            .color_tint(tint)
            .contrast(contrast);
        let mut host = shown(image.into(), two_grays(100));
        close(host.pixel(25, 50), expected(tint, 64, contrast), (25, 50));
        close(host.pixel(75, 50), expected(tint, 191, contrast), (75, 50));
    }
    // A transparent tint: the picture as it is.
    let image = SkiaImage::new("two.png").width_request(100).height_request(100).add_effect(SkiaImageEffect::Recolor);
    let mut host = shown(image.into(), two_grays(100));
    close(host.pixel(25, 50), [64.0; 3], (25, 50));
    close(host.pixel(75, 50), [191.0; 3], (75, 50));
}

#[test]
fn an_effect_on_image_tiles_colors_every_tile() {
    let tint = Color::from_rgb(190, 140, 45);
    let tiles = SkiaImageTiles::new("two.png")
        .width_request(100)
        .height_request(60)
        .tile_width(20)
        .tile_height(20)
        .tile_aspect(TransformAspect::Cover)
        .add_effect(SkiaImageEffect::Recolor)
        .color_tint(tint)
        .contrast(1.6);
    let mut host = shown(tiles.into(), two_grays(40));
    for tx in 0..5 {
        for ty in 0..3 {
            let (x, y) = (tx * 20, ty * 20 + 10);
            close(host.pixel(x + 4, y), expected(tint, 64, 1.6), (x + 4, y));
            close(host.pixel(x + 15, y), expected(tint, 191, 1.6), (x + 15, y));
        }
    }
}
