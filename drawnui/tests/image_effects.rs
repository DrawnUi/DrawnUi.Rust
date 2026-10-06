//! The color effects of SkiaImage on pixels. Where the C# engine and the TypeScript engine
//! (DrawnUi.React) agree, the expected colors were read from the built upstream engine
//! (DrawnUi.Net, HeadlessCanvasHost, a SkiaImage of the same two-color source with the same
//! properties). Five effects follow the TypeScript engine instead, because upstream puts their
//! offset on the wrong scale (1 is full white for Skia, upstream passes steps of 255): Darken and
//! Lighten take their amount in steps of 255, InvertColors inverts, Contrast only scales.
//! Those colors are computed from the matrices of ImageEffects.ts; what upstream draws is kept
//! next to each case.

mod image_common;

use drawnui::prelude::*;
use drawnui::skia::BlendMode;
use drawnui::testing::Headless;
use image_common::*;

const LEFT: Color = Color::from_rgb(200, 100, 50);
const RIGHT: Color = Color::from_rgb(40, 160, 220);

#[derive(Default)]
struct App {
    image: Handle<SkiaImage>,
}

/// A 160 x 100 picture, its left half `LEFT` and its right half `RIGHT`, shown one to one.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaImage::new("two.png")
            .width_request(160)
            .height_request(100)
            .aspect(TransformAspect::Cover)
            .background_color(Color::BLACK)
            .assign(&mut app.image),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 200, 140, 1.0);
    host.settle();
    host.deliver_images(|_| Some(two_colors()));
    host.settle();
    host
}

/// The 160 x 100 source: `LEFT` and `RIGHT` halves.
fn two_colors() -> Vec<u8> {
    png(160, 100, |canvas| {
        canvas.clear(LEFT);
        fill(canvas, Rect::from_xywh(80.0, 0.0, 80.0, 100.0), RIGHT);
    })
}

/// Blur is in points, as React (sigma x scale; upstream takes pixels): blurred at scale 2 the
/// picture is the one of scale 1, twice as large.
#[test]
fn blur_is_in_points() {
    let row = |scale: f32, blur: f32| -> Vec<[f32; 3]> {
        let ui = Ui::new(App::default(), |app: &mut App| {
            SkiaLayout::new().fill().children((SkiaImage::new("two.png")
                .width_request(160)
                .height_request(100)
                .aspect(TransformAspect::Cover)
                .blur(blur)
                .assign(&mut app.image),))
        });
        let mut host = Headless::new(ui.background(Color::BLACK), (200.0 * scale) as i32, (140.0 * scale) as i32, scale);
        host.settle();
        host.deliver_images(|_| Some(two_colors()));
        host.settle();
        // Across the edge between the halves, at x = 80 points: at scale 2 the two pixels of a
        // point, averaged (their centers sit a quarter point either side of the point's).
        (68..=92)
            .map(|x| {
                let pixels = (0..scale as i32).map(|i| host.pixel(x * scale as i32 + i, (50.0 * scale) as i32)).collect::<Vec<_>>();
                let mean = |channel: fn(Color) -> u8| pixels.iter().map(|p| channel(*p) as u32).sum::<u32>() as f32 / pixels.len() as f32;
                [mean(Color::r), mean(Color::g), mean(Color::b)]
            })
            .collect::<Vec<_>>()
    };
    let farthest = |a: &[[f32; 3]], b: &[[f32; 3]]| {
        a.iter().zip(b).flat_map(|(p, q)| p.iter().zip(q).map(|(x, y)| (x - y).abs())).fold(0.0, f32::max)
    };
    let one = row(1.0, 3.0);
    assert!(one[12][0] < 150.0, "blurred at the edge: {:?}", one[12]);
    // Skia's blur is a box approximation whose steps depend on the sigma: a few levels apart.
    let points = farthest(&one, &row(2.0, 3.0));
    // What blur in pixels would draw at scale 2: 1.5 points.
    let pixels = farthest(&one, &row(2.0, 1.5));
    assert!(points <= 6.0 && pixels > 20.0, "scale 2 in points {points}, in pixels {pixels}");
}

/// Both halves show the colors the C# engine shows, within one step of rounding.
fn assert_shows(host: &mut Headless<App>, what: &str, left: (u8, u8, u8), right: (u8, u8, u8)) {
    for (x, (r, g, b)) in [(40, left), (120, right)] {
        let pixel = host.pixel(x, 50);
        let close = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 1;
        assert!(close(pixel.r(), r) && close(pixel.g(), g) && close(pixel.b(), b), "{what} at x {x}: {pixel:?}");
    }
}

type Setup = fn(&mut Mut<'_, SkiaImage>);

#[test]
fn every_effect_gives_its_colors() {
    use SkiaImageEffect::*;
    const WHITE: (u8, u8, u8) = (255, 255, 255);
    let cases: [(&str, SkiaImageEffect, Setup, (u8, u8, u8), (u8, u8, u8)); 14] = [
        ("None", None, |_| {}, (200, 100, 50), (40, 160, 220)),
        ("BlackAndWhite", BlackAndWhite, |_| {}, (124, 124, 124), (131, 131, 131)),
        ("Sepia", Sepia, |_| {}, (165, 147, 114), (180, 161, 125)),
        ("Pastel", Pastel, |_| {}, (188, 138, 113), (125, 185, 215)),
        // As ImageEffects.ts: x 2.5, offset -0.25 / 255. Upstream: (255, 186, 61) and (36, 255, 255).
        ("Contrast 1.5", Contrast, |i| i.set_contrast(1.5), (255, 250, 125), (100, 255, 255)),
        ("Saturation 2", Saturation, |i| i.set_saturation(2), (255, 82, 0), (0, 181, 255)),
        ("Gamma 0.6", Gamma, |i| i.set_gamma(0.6), (172, 57, 18), (13, 120, 201)),
        // As ImageEffects.ts: every channel minus or plus amount / 255 of full white. Upstream
        // draws black for Darken and white for Lighten at both amounts, (149, 49, 0) and
        // (0, 109, 169) for Darken 0.2, (251, 151, 101) and (91, 211, 255) for Lighten 0.2.
        ("Darken 80", Darken, |i| i.set_darken(80), (120, 20, 0), (0, 80, 140)),
        ("Darken default", Darken, |i| i.set_darken(5), (195, 95, 45), (35, 155, 215)),
        ("Lighten 80", Lighten, |i| i.set_lighten(80), (255, 180, 130), (120, 240, 255)),
        ("Lighten default", Lighten, |i| i.set_lighten(5), (205, 105, 55), (45, 165, 225)),
        // As ImageEffects.ts: 255 minus every channel. Upstream draws white (offset 255).
        ("InvertColors", InvertColors, |_| {}, (55, 155, 205), (215, 95, 35)),
        // The amount is the part of full white added; it must be 1 or more, so both engines draw white.
        ("Brightness 1", Brightness, |i| i.set_brightness(1), WHITE, WHITE),
        ("Tint", Tint, |i| {
            i.set_color_tint(Color::from_rgb(0x0D, 0x6E, 0xFD));
            i.set_effect_blend_mode(BlendMode::Multiply);
        }, (11, 43, 50), (3, 69, 218)),
    ];
    let mut host = host();
    for (what, effect, setup, left, right) in cases {
        let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
        image.set_add_effect(effect);
        setup(&mut image);
        host.settle();
        assert_shows(&mut host, what, left, right);
    }
}

/// The app's own filters (React `AddEffect="Custom"` + PaintColorFilter, PaintImageFilter).
#[test]
fn custom_filters_of_the_app_replace_the_effect_and_the_blur() {
    use drawnui::skia::{color_filters, image_filters};
    let mut host = host();
    // Red and blue swapped.
    let swap = color_filters::matrix_row_major(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0], None);
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_add_effect(SkiaImageEffect::Custom);
    image.set_paint_color_filter(swap.clone());
    host.settle();
    assert_shows(&mut host, "swap", (50, 100, 200), (220, 160, 40));
    // The app's filter wins over the effect.
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_add_effect(SkiaImageEffect::Sepia);
    host.settle();
    assert_shows(&mut host, "swap over sepia", (50, 100, 200), (220, 160, 40));
    // The same filter set again is a new setting: nothing breaks, the picture stays.
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_paint_color_filter(swap);
    host.settle();
    assert_shows(&mut host, "swap again", (50, 100, 200), (220, 160, 40));

    // Dilate by 4 pixels: near the edge between the halves every channel is the larger one.
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_paint_color_filter(None);
    image.set_add_effect(SkiaImageEffect::None);
    image.set_paint_image_filter(image_filters::dilate((4.0, 4.0), None, None));
    host.settle();
    assert_shows(&mut host, "dilate", (200, 100, 50), (40, 160, 220));
    let edge = host.pixel(78, 50);
    assert_eq!((edge.r(), edge.g(), edge.b()), (200, 160, 220), "{edge:?}");
}

/// HSL and TSL as ImageEffects.ts: the tint blended in with `effect_blend_mode` (SrcIn by default
/// replaces the picture with it), the color for HSL at hue `gamma`, full saturation, lightness
/// `brightness`; nothing without a background color.
#[test]
fn hsl_and_tsl_tint_from_the_hue_or_the_background_as_react() {
    let mut host = host();
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_add_effect(SkiaImageEffect::HSL);
    image.set_gamma(0.6);
    image.set_saturation(1);
    image.set_brightness(0.5);
    image.set_background_color(Color::WHITE);
    host.settle();
    // hsl(0.6, 1, 0.5) = (0, 102, 255); React draws (1, 103, 255) (a node script over its dist).
    assert_shows(&mut host, "HSL", (0, 102, 255), (0, 102, 255));

    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_add_effect(SkiaImageEffect::TSL);
    host.settle();
    assert_shows(&mut host, "TSL white", (255, 255, 255), (255, 255, 255));
    // The background alone changed: the tint follows it (React reads it at paint).
    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_background_color(Color::from_rgb(255, 0, 0));
    host.settle();
    assert_shows(&mut host, "TSL", (255, 0, 0), (255, 0, 0));

    let mut image = host.ui.tree.get_mut(host.ui.state.image).unwrap();
    image.set_background_color(None);
    host.settle();
    assert_shows(&mut host, "TSL without a background", (200, 100, 50), (40, 160, 220));
}
