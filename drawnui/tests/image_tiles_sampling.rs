//! SkiaImageTiles shrinks its picture into a tile the way a SkiaImage shrinks it: a 512 px pattern
//! with a 2 px line every 32 px, drawn as 64 px tiles, keeps every line (one per 4 px period, as
//! bright as in the shrunk image), none lost and none doubled (DrawnUi.Net f0da51c38).

mod image_common;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::*;

#[derive(Default)]
struct App;

/// 512 x 512, black, a white 2 px vertical line every 32 px.
fn lines() -> Vec<u8> {
    png(512, 512, |canvas| {
        canvas.clear(Color::BLACK);
        for i in 0..16 {
            fill(canvas, Rect::from_xywh(i as f32 * 32.0 + 15.0, 0.0, 2.0, 512.0), Color::WHITE);
        }
    })
}

fn shown(content: drawnui::Detached) -> Headless<App> {
    let ui = Ui::new(App, move |_| SkiaLayout::new().fill().children((content,)));
    let mut host = Headless::new(ui.background(Color::BLACK), 200, 100, 1.0);
    host.settle();
    host.deliver_images(|_| Some(lines()));
    host.settle();
    host
}

/// The light of each 4 px period along row 32 of the first 64 px.
fn periods(host: &mut Headless<App>) -> Vec<u32> {
    (0..16).map(|p| (0..4).map(|i| host.pixel(p * 4 + i, 32).r() as u32).sum()).collect()
}

#[test]
fn a_shrunk_tile_keeps_every_line_as_a_shrunk_image_does() {
    let mut image = shown(SkiaImage::new("lines.png").width_request(64).height_request(64).aspect(TransformAspect::Cover).into());
    let mut tiles = shown(SkiaImageTiles::new("lines.png").width_request(128).height_request(64).tile_width(64).tile_height(64).tile_aspect(TransformAspect::Cover).into());
    let (a, b) = (periods(&mut image), periods(&mut tiles));
    for (p, (x, y)) in a.iter().zip(&b).enumerate() {
        assert!(*x > 30, "the shrunk image keeps line {p}: {a:?}");
        assert!((*x as i32 - *y as i32).abs() <= 12, "period {p}: image {x}, tile {y}\nimage {a:?}\ntiles {b:?}");
    }
}
