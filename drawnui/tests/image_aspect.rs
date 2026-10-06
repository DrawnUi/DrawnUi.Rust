//! SkiaImage drawing and measuring against pixels: every aspect mode, the alignments, zoom and
//! offsets, the auto-size measure. The expected rects are the upstream formulas
//! (SkiaControl.RescaleAspect, SkiaImage.CalculateDisplayRect, SkiaImage.OnMeasuring) worked out
//! by hand for a 40 x 20 and a 400 x 100 bitmap.

mod image_common;

use drawnui::Detached;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::*;

/// The image's box in points: 100 x 60 at (20, 30).
const BOX: Rect = Rect { left: 20.0, top: 30.0, right: 120.0, bottom: 90.0 };

#[derive(Default)]
struct App {
    image: Handle<SkiaImage>,
}

fn bytes(source: &str) -> Option<Vec<u8>> {
    match source {
        "small.png" => Some(quadrants(40, 20)),
        "big.png" => Some(quadrants(400, 100)),
        _ => None,
    }
}

/// A 160 x 120 point canvas with the image in `BOX`, both bitmaps loaded, "small.png" showing.
fn host(scale: f32, alignment: DrawImageAlignment) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaImage::new("small.png")
            .margin((20, 30, 0, 0))
            .width_request(100)
            .height_request(60)
            .horizontal_alignment(alignment)
            .vertical_alignment(alignment)
            .assign(&mut app.image),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), (160.0 * scale) as i32, (120.0 * scale) as i32, scale);
    host.ui.tree.images.preload(["big.png"]);
    host.settle();
    assert_eq!(deliver(&mut host, bytes).len(), 2);
    host.settle();
    host
}

fn image<'a>(host: &'a mut Headless<App>) -> Mut<'a, SkiaImage> {
    host.ui.tree.get_mut(host.ui.state.image).unwrap()
}

fn scaled(r: Rect, scale: f32) -> Rect {
    Rect::new(r.left * scale, r.top * scale, r.right * scale, r.bottom * scale)
}

/// The control reports `display` (pixels) and every probed pixel of the canvas shows what a
/// quadrants picture drawn there and cropped to the box must show.
fn assert_drawn(host: &mut Headless<App>, scale: f32, display: Rect, what: &str) {
    let id = host.ui.state.image;
    let bounds = host.rect(id);
    assert_eq!(bounds, scaled(BOX, scale), "{what}: box");
    assert_eq!(host.ui.tree.find::<SkiaImage>(id).unwrap().display_rect(bounds, scale), Some(display), "{what}");
    let (width, height) = ((160.0 * scale) as i32, (120.0 * scale) as i32);
    let mut checked = 0;
    for y in (2..height).step_by(7) {
        for x in (2..width).step_by(7) {
            let Some(color) = expected(display, bounds, Color::BLACK, x as f32 + 0.5, y as f32 + 0.5) else { continue };
            assert_eq!(host.pixel(x, y), color, "{what}: pixel ({x}, {y})");
            checked += 1;
        }
    }
    assert!(checked > 200, "{what}: only {checked} pixels probed");
}

#[test]
fn every_aspect_scales_as_upstream() {
    use TransformAspect::*;
    // (aspect, size of the 40 x 20 bitmap in the 100 x 60 box, size of the 400 x 100 one).
    // s1 = 2.5, s2 = 3 for the small one; s1 = 0.25, s2 = 0.6 for the big one.
    let cases = [
        (None, (40.0, 20.0), (400.0, 100.0)),
        (Tile, (40.0, 20.0), (400.0, 100.0)),
        (Fill, (100.0, 60.0), (400.0, 100.0)),
        (Fit, (40.0, 20.0), (100.0, 60.0)),
        (FitFill, (100.0, 60.0), (100.0, 60.0)),
        (Cover, (100.0, 60.0), (100.0, 60.0)),
        (AspectFit, (100.0, 50.0), (100.0, 25.0)),
        (AspectFill, (120.0, 60.0), (400.0, 100.0)),
        (AspectFitFill, (100.0, 50.0), (100.0, 25.0)),
        (AspectCover, (120.0, 60.0), (240.0, 60.0)),
    ];
    let mut host = host(1.0, DrawImageAlignment::Start);
    for (aspect, small, big) in cases {
        for (source, (w, h)) in [("small.png", small), ("big.png", big)] {
            image(&mut host).set_aspect(aspect);
            image(&mut host).set_source(source);
            host.settle();
            // Start alignment: a picture larger than the box shows its top left part.
            assert_drawn(&mut host, 1.0, Rect::from_xywh(BOX.left, BOX.top, w, h), &format!("{aspect:?} {source}"));
        }
    }
    // Both bitmaps were loaded once; switching between them asked the host for nothing.
    assert!(host.ui.tree.images.take_requests().is_empty());
}

#[test]
fn default_aspect_is_aspect_cover_centered() {
    let mut host = host(1.0, DrawImageAlignment::Center);
    assert_eq!(host.ui.tree.find::<SkiaImage>(host.ui.state.image).unwrap().p.aspect, TransformAspect::AspectCover);
    // 120 x 60 in a 100 x 60 box: 10 pixels cropped on each side.
    assert_drawn(&mut host, 1.0, Rect::from_xywh(10.0, 30.0, 120.0, 60.0), "default");
}

#[test]
fn alignment_places_the_scaled_bitmap() {
    use DrawImageAlignment::*;
    let mut host = host(1.0, Start);
    image(&mut host).set_aspect(TransformAspect::None);
    // 40 x 20 in 100 x 60: 60 and 40 pixels of room.
    for (horizontal, x) in [(Start, 20.0), (Center, 50.0), (End, 80.0)] {
        for (vertical, y) in [(Start, 30.0), (Center, 50.0), (End, 70.0)] {
            image(&mut host).set_horizontal_alignment(horizontal);
            image(&mut host).set_vertical_alignment(vertical);
            host.settle();
            assert_drawn(&mut host, 1.0, Rect::from_xywh(x, y, 40.0, 20.0), &format!("{horizontal:?} {vertical:?}"));
        }
    }
    // A picture wider than the box: End shows its right part.
    image(&mut host).set_aspect(TransformAspect::AspectCover);
    image(&mut host).set_horizontal_alignment(End);
    host.settle();
    assert_drawn(&mut host, 1.0, Rect::from_xywh(0.0, 30.0, 120.0, 60.0), "cover End");
}

#[test]
fn zoom_scales_and_offsets_move_in_points() {
    // Scale 2: the box is 200 x 120 pixels at (40, 60); bitmap pixels are device pixels.
    let mut host = host(2.0, DrawImageAlignment::Center);
    image(&mut host).set_aspect(TransformAspect::None);
    host.settle();
    assert_drawn(&mut host, 2.0, Rect::from_xywh(120.0, 110.0, 40.0, 20.0), "as is");

    image(&mut host).set_zoom_x(2);
    image(&mut host).set_zoom_y(3);
    host.settle();
    assert_drawn(&mut host, 2.0, Rect::from_xywh(100.0, 90.0, 80.0, 60.0), "zoom");

    // 5 points right = 10 pixels; -2.5 points = 5 pixels up.
    image(&mut host).set_horizontal_offset(5);
    image(&mut host).set_vertical_offset(-2.5);
    host.settle();
    assert_drawn(&mut host, 2.0, Rect::from_xywh(110.0, 85.0, 80.0, 60.0), "offset");
}

#[test]
fn zoom_grows_around_the_center_of_the_aligned_picture_as_react() {
    // Start: the 40 x 20 pixels sit at the box's corner (40, 60); zoomed 2 x 3 around their
    // center (60, 70). Upstream zooms first and puts the 80 x 60 at the corner.
    let mut host = host(2.0, DrawImageAlignment::Start);
    image(&mut host).set_aspect(TransformAspect::None);
    image(&mut host).set_zoom_x(2);
    image(&mut host).set_zoom_y(3);
    host.settle();
    assert_drawn(&mut host, 2.0, Rect::from_xywh(20.0, 40.0, 80.0, 60.0), "zoom at Start");
    // End: the corner (240 - 40, 180 - 20) = (200, 160), center (220, 170).
    image(&mut host).set_horizontal_alignment(DrawImageAlignment::End);
    image(&mut host).set_vertical_alignment(DrawImageAlignment::End);
    host.settle();
    assert_drawn(&mut host, 2.0, Rect::from_xywh(180.0, 140.0, 80.0, 60.0), "zoom at End");
}

// ---------------------------------------------------------------- measure

/// A 300 x 200 point canvas with one auto-sized image, inside `parent`.
fn auto_host(scale: f32, wrap: fn(Build<SkiaImage>) -> Detached) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| wrap(SkiaImage::new("small.png").assign(&mut app.image)));
    let mut host = Headless::new(ui.background(Color::BLACK), (300.0 * scale) as i32, (200.0 * scale) as i32, scale);
    host.ui.tree.images.preload(["big.png"]);
    host.settle();
    host
}

fn size(host: &Headless<App>) -> (f32, f32) {
    let rect = host.rect(host.ui.state.image);
    (rect.width(), rect.height())
}

#[test]
fn auto_size_is_the_bitmap_scaled_into_the_offered_box() {
    let mut host = auto_host(1.0, |image| SkiaLayout::new().fill().children((image,)).into());
    // Nothing loaded: the box it is offered (C# OnMeasuring measures "no children, simulated").
    assert_eq!(size(&host), (300.0, 200.0));
    deliver(&mut host, bytes);
    host.settle();
    // AspectCover: 40 x 20 times max(7.5, 10) = 400 x 200, limited to the box.
    assert_eq!(size(&host), (300.0, 200.0));

    for (aspect, source, expected) in [
        (TransformAspect::AspectFit, "small.png", (300.0, 150.0)),
        (TransformAspect::None, "small.png", (40.0, 20.0)),
        (TransformAspect::Fit, "small.png", (40.0, 20.0)),
        (TransformAspect::Fill, "small.png", (300.0, 200.0)),
        // 400 x 100 in 300 x 200: s1 = 0.75, s2 = 2.
        (TransformAspect::Fit, "big.png", (300.0, 100.0)),
        (TransformAspect::AspectFit, "big.png", (300.0, 75.0)),
        (TransformAspect::AspectFill, "big.png", (300.0, 100.0)),
        (TransformAspect::AspectCover, "big.png", (300.0, 200.0)),
    ] {
        image(&mut host).set_aspect(aspect);
        image(&mut host).set_source(source);
        host.settle();
        assert_eq!(size(&host), expected, "{aspect:?} {source}");
    }
}

#[test]
fn bitmap_pixels_are_device_pixels() {
    let mut host =
        auto_host(2.0, |image| SkiaLayout::new().fill().children((image.aspect(TransformAspect::None),)).into());
    deliver(&mut host, bytes);
    host.settle();
    // 40 x 20 bitmap pixels at 2 pixels per point: 20 x 10 points.
    assert_eq!(size(&host), (40.0, 20.0));
    assert_eq!(host.pixel(10, 5), Color::RED);
    assert_eq!(host.pixel(30, 15), Color::YELLOW);
    assert_eq!(host.pixel(45, 5), Color::BLACK);
}

#[test]
fn an_unbounded_side_follows_the_bitmap_aspect() {
    // A vertical scroll offers its content any height.
    let mut host = auto_host(1.0, |image| SkiaScroll::new().fill().content(image).into());
    assert_eq!(size(&host), (300.0, 0.0));
    deliver(&mut host, bytes);
    host.settle();
    // 40 x 20 at the offered width: 300 x 150.
    assert_eq!(size(&host), (300.0, 150.0));
    image(&mut host).set_source("big.png");
    host.settle();
    assert_eq!(size(&host), (300.0, 75.0));
    // A requested width: the height follows it.
    image(&mut host).set_width_request(100);
    host.settle();
    assert_eq!(size(&host), (100.0, 25.0));

    // A horizontal scroll offers any width: the width follows the height.
    let mut host = auto_host(1.0, |image| {
        SkiaScroll::new().fill().orientation(ScrollOrientation::Horizontal).content(image.height_request(60)).into()
    });
    assert_eq!(size(&host), (0.0, 60.0));
    deliver(&mut host, bytes);
    host.settle();
    assert_eq!(size(&host), (120.0, 60.0));
}

#[test]
fn a_fill_side_keeps_the_box_and_the_other_side_follows() {
    let mut host = auto_host(1.0, |image| {
        SkiaScroll::new().fill().content(image.fill_x().aspect(TransformAspect::AspectFit)).into()
    });
    deliver(&mut host, bytes);
    host.settle();
    assert_eq!(size(&host), (300.0, 150.0));
    // The picture fills the control: the quadrants meet at its center.
    assert_eq!(host.pixel(75, 40), Color::RED);
    assert_eq!(host.pixel(225, 40), Color::GREEN);
    assert_eq!(host.pixel(75, 110), Color::BLUE);
    assert_eq!(host.pixel(225, 110), Color::YELLOW);
    assert_eq!(host.pixel(150, 175), Color::BLACK);
}

// ---------------------------------------------------------------- effects

#[test]
fn color_effects_use_the_upstream_matrices() {
    let mut host = host(1.0, DrawImageAlignment::Start);
    image(&mut host).set_aspect(TransformAspect::Cover);
    image(&mut host).set_add_effect(SkiaImageEffect::BlackAndWhite);
    host.settle();
    // Red through 0.2989 / 0.587 / 0.114: 0.2989 * 255 = 76.
    let gray = host.pixel(40, 40);
    assert!((75..=77).contains(&gray.r()) && gray.r() == gray.g() && gray.g() == gray.b(), "{gray:?}");

    // Tint: the color over the picture's alpha (SrcIn).
    image(&mut host).set_add_effect(SkiaImageEffect::Tint);
    image(&mut host).set_color_tint(Color::MAGENTA);
    host.settle();
    assert_eq!(host.pixel(40, 40), Color::MAGENTA);
    assert_eq!(host.pixel(100, 80), Color::MAGENTA);
    // A transparent tint is no effect.
    image(&mut host).set_color_tint(Color::TRANSPARENT);
    host.settle();
    assert_eq!(host.pixel(40, 40), Color::RED);

    // Blur stays inside the box and mixes the quadrants around the center.
    image(&mut host).set_blur(6);
    host.settle();
    let center = host.pixel(70, 60);
    assert!(!QUADRANTS.contains(&center), "{center:?}");
    assert_eq!(host.pixel(125, 60), Color::BLACK);
    assert_eq!(host.pixel(70, 95), Color::BLACK);
}
