//! Tests ported from DrawnUi.Net.Tests, same scenes and numbers: AspectCoverTests,
//! AspectCoverHeightDrivenTests (the cases without a shader effect) and AspectScaleRelayoutTests.
//! Upstream sets the bitmap with SetImageInternal; here it arrives through the manager as a PNG.

mod image_common;

use drawnui::prelude::*;
use drawnui::skia::Canvas;
use drawnui::testing::Headless;
use image_common::*;

const GREEN: Color = Color::from_argb(255, 20, 60, 20);

#[derive(Default)]
struct App {
    image: Handle<SkiaImage>,
    tile: Handle<SkiaShape>,
}

/// A canvas with one tile at the top left holding an image that fills it, the bitmap delivered.
/// `layered`: the image sits in a SkiaLayer inside the tile, as in the upstream picker.
fn host(size: (i32, i32), tile: Build<SkiaShape>, layered: bool, source: Vec<u8>) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let image = SkiaImage::new("source.png").aspect(TransformAspect::AspectCover).fill().assign(&mut app.image);
        let tile = match layered {
            true => tile.children((SkiaLayout::layer().fill_y().children((image,)),)),
            false => tile.children((image,)),
        };
        SkiaLayout::new().fill().children((tile.assign(&mut app.tile),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), size.0, size.1, 1.0);
    host.settle();
    assert_eq!(deliver(&mut host, |_| Some(source.clone())), ["source.png"]);
    host.settle();
    host
}

fn is_red(c: Color) -> bool {
    c.r() > 120 && c.g() < 90 && c.b() < 90
}

/// First and last red pixel of a row between `left` and `right`.
fn red_span(host: &mut Headless<App>, left: i32, right: i32, y: i32) -> Option<(i32, i32)> {
    let red: Vec<i32> = (left..right).filter(|x| is_red(host.pixel(*x, y))).collect();
    Some((*red.first()?, *red.last()?))
}

/// AspectCoverTests.AspectCover_CentersSource: the source center stays on the box center in a
/// tall tile, a wide one and a very wide one.
#[test]
fn aspect_cover_centers_the_source() {
    // 768 x 1024, black, with a red 8 x 8 marker at its exact center.
    let source = png(768, 1024, |canvas: &Canvas| {
        canvas.clear(Color::BLACK);
        fill(canvas, Rect::from_xywh(380.0, 508.0, 8.0, 8.0), Color::RED);
    });
    for (w, h) in [(270.0, 310.0), (210.0, 118.0), (500.0, 100.0)] {
        let tile = SkiaShape::new().width_request(w).height_request(h);
        let mut host = host((600, 800), tile, false, source.clone());
        let dest = host.rect(host.ui.state.image);
        assert_eq!((dest.width(), dest.height()), (w, h));
        let display = host.ui.tree.find::<SkiaImage>(host.ui.state.image).unwrap().display_rect(dest, 1.0).unwrap();

        // Cover: the scaled image is at least as big as the box on both axes.
        assert!(display.width() >= dest.width() - 1.0 && display.height() >= dest.height() - 1.0, "{display:?}");
        // Centered: the same overflow left and right, top and bottom.
        assert!(((display.left - dest.left) - (dest.right - display.right)).abs() <= 1.5, "{display:?}");
        assert!(((display.top - dest.top) - (dest.bottom - display.bottom)).abs() <= 1.5, "{display:?}");
        // And on pixels: the marker is at the center of the tile.
        assert!(is_red(host.pixel((w / 2.0) as i32, (h / 2.0) as i32)), "tile {w} x {h}");
    }
}

/// AspectCoverHeightDrivenTests.SquareSource_StaysCentered: a square source in a tile whose
/// height drives the scale is cropped symmetrically. The tile is image-cached, as upstream.
#[test]
fn a_square_source_stays_centered_when_the_height_drives_the_cover() {
    const TILE_LEFT: f32 = 32.0;
    // 1088 x 1088 with a 20 pixel red stripe at its horizontal center.
    let source = png(1088, 1088, |canvas: &Canvas| {
        canvas.clear(GREEN);
        fill(canvas, Rect::from_xywh(534.0, 0.0, 20.0, 1088.0), Color::RED);
    });
    for (w, h) in [(270.0, 118.0), (270.0, 310.0)] {
        let tile = SkiaShape::new().width_request(w).height_request(h).margin((TILE_LEFT, 20.0, 0.0, 0.0));
        let tile = tile.use_cache(CacheType::Image);
        let mut host = host((400, 500), tile, true, source.clone());
        let (left, right, y) = (TILE_LEFT as i32, (TILE_LEFT + w) as i32, (20.0 + h / 2.0) as i32);
        let (first, last) = red_span(&mut host, left, right, y).expect("marker");
        let (marker, expected) = ((first + last) as f32 / 2.0, TILE_LEFT + w / 2.0);
        assert!((marker - expected).abs() <= 2.0, "tile {w} x {h}: subject off center by {}", marker - expected);
    }
}

/// AspectScaleRelayoutTests.ResizedAfterLoad_RecomputesAspectScale: the scale follows the final
/// box when the tile shrinks after the bitmap was loaded.
#[test]
fn the_aspect_scale_follows_the_box_after_a_resize() {
    // 1088 x 1088 with a red square of 1/8 of the source at its center.
    let source = png(1088, 1088, |canvas: &Canvas| {
        canvas.clear(GREEN);
        fill(canvas, Rect::new(476.0, 476.0, 612.0, 612.0), Color::RED);
    });
    let mut host = host((400, 400), SkiaShape::new().width_request(320).height_request(320), true, source);
    let scale = |host: &Headless<App>| {
        let image = host.ui.state.image;
        host.ui.tree.find::<SkiaImage>(image).unwrap().display_rect(host.rect(image), 1.0).unwrap().width() / 1088.0
    };
    assert_eq!(scale(&host), 320.0 / 1088.0);

    // The tile becomes much smaller, like a star column resolving to its real width.
    let tile = host.ui.state.tile;
    host.ui.tree.get_mut(tile).unwrap().set_width_request(160);
    host.ui.tree.get_mut(tile).unwrap().set_height_request(160);
    host.settle();

    let small = scale(&host);
    assert!((160.0 / 1088.0 * 0.9..=160.0 / 1088.0 * 1.1).contains(&small), "scale {small}");
    // The marker is 1/8 of the source: about 20 pixels in a 160 pixel tile. A stale scale doubles it.
    let (first, last) = red_span(&mut host, 0, 160, 80).expect("marker");
    assert!((14..=28).contains(&(last - first + 1)), "marker width {}", last - first + 1);
}
