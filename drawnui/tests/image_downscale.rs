//! A photo drawn smaller than its bitmap, against the C# engine and DrawnUi.React on pixels.
//! Upstream resizes the bitmap once with the sampling of `RescalingQuality` into a cached copy
//! (SkiaImage.DrawSource, CacheRescaledSource) and blits that; here the same sampling runs at
//! draw, from the one bitmap the controls of a source share. The pictures are the same: Low
//! (upstream's default) is a linear filter, which leaves single-pixel speckles in fur at a third of
//! its size upstream too; Medium and High sample mip levels and are clean; None is nearest. React
//! has no RescalingQuality: it draws every bitmap linear between mip levels, which is High, the
//! default here.
//!
//! The rows were read from the built upstream engine (DrawnUi.Net, HeadlessCanvasHost) and from
//! React's `SkiaImage.Paint` on a CPU surface of its CanvasKit (a node script over its dist): the
//! baboon of both demos' Images page (1024 x 931) in a 220 x 120 point box, default AspectCover,
//! every eighth pixel of the middle row. A control that alone shows a source gets a bitmap the
//! host reduced for its box: it is compared with React's full bitmap within a wider margin.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const BABOON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/images/baboon.jpg");

/// (quality, pixels per point, the row the C# engine draws).
const ROWS: [(FilterQuality, f32, &[u32]); 5] = [
    // 220 x 120 pixels, scaled copy 220 x 200.
    (FilterQuality::Low, 1.0, &[
        0x6B7F40, 0x374C12, 0x71893D, 0x718B43, 0x788E50, 0x4A391A, 0x50360D, 0xB38B62, 0xA37C50, 0x4C2E12, 0x674B35,
        0x543518, 0x9D8266, 0x72543B, 0x60482E, 0x533A21, 0x846747, 0xBCAB8C, 0x927B5E, 0xDAC0A9, 0xA79E84, 0x6B7E26,
        0x738348, 0x333A1B, 0x42461B, 0x5A5E31, 0x8C9B63,
    ]),
    // 275 x 150 pixels (the page in a browser at 1.25), scaled copy 275 x 250.
    (FilterQuality::Low, 1.25, &[
        0x748944, 0x7A9755, 0x716C4C, 0x859F58, 0x748E45, 0x7B9554, 0x646D36, 0x573913, 0xAB8A67, 0xAD865D, 0x89663C,
        0x331201, 0x523517, 0x644529, 0xA38569, 0x6A4C31, 0x7C6048, 0x7B5D41, 0x4A371D, 0x8A6E59, 0x55350E, 0x533822,
        0xC3AD96, 0xBCA287, 0xB9A48C, 0x85755B, 0x4D5D17, 0x7D8F3E, 0x869575, 0x3C3F22, 0x5F5A32, 0x606235, 0x6D7947,
        0x606933,
    ]),
    (FilterQuality::Medium, 1.25, &[
        0x738843, 0x789553, 0x726C4D, 0x859F58, 0x738D44, 0x7A9453, 0x646E36, 0x593B16, 0x8B6B48, 0xA67F57, 0x98744A,
        0x351401, 0x513416, 0x65472B, 0x86684B, 0x76583C, 0x6F543B, 0x785A3E, 0x503D23, 0x7B604C, 0x876741, 0x81664D,
        0xA38E77, 0xD1B99D, 0xC1AC93, 0xB0A48A, 0x4D5C17, 0x7D8E3D, 0x859473, 0x3C3F22, 0x5F5A33, 0x5F6134, 0x6D7846,
        0x5F6932,
    ]),
    (FilterQuality::High, 1.25, &[
        0x738843, 0x789553, 0x706C4B, 0x859F58, 0x738D44, 0x7A9453, 0x656F36, 0x5A3C17, 0x856541, 0xA27C54, 0x9B774D,
        0x361601, 0x513416, 0x66482D, 0x87694C, 0x7A5D41, 0x71553C, 0x785A3E, 0x534026, 0x7D634F, 0x8F7049, 0x866B51,
        0xA7917A, 0xCFB69B, 0xC1AD94, 0xAEA186, 0x4C5B17, 0x7D8D3C, 0x849372, 0x3D4023, 0x5F5A33, 0x5E6134, 0x6D7846,
        0x5F6832,
    ]),
    (FilterQuality::None, 1.25, &[
        0x758A45, 0x7A9755, 0x726C4C, 0x859F58, 0x738D44, 0x7B9554, 0x646D36, 0x553711, 0xA68562, 0xAC855C, 0x825F35,
        0x331201, 0x523517, 0x57381C, 0xA4866A, 0x694B2F, 0x846850, 0x7C5E42, 0x362207, 0xA88C77, 0x4F2F08, 0x2F1400,
        0xCAB49D, 0xBFA58A, 0xC1AB93, 0x85755B, 0x4E5D18, 0x7E8F3F, 0x869574, 0x3D4023, 0x5E5931, 0x5F6235, 0x727E4C,
        0x616A33,
    ]),
];

/// (pixels per point, the row React draws).
const REACT: [(f32, &[u32]); 2] = [
    (1.0, &[
        0x6B7F40, 0x364B11, 0x70873D, 0x708A43, 0x778C4F, 0x4A3818, 0x5B411B, 0xA8815A, 0xA78154, 0x47290D, 0x664A32,
        0x856649, 0x8C7156, 0x705339, 0x61492F, 0x866E57, 0x9B7C5C, 0xB49E80, 0xAF977A, 0xC4AF96, 0x766C56, 0x6B7D25,
        0x728147, 0x394020, 0x46491F, 0x5A5F31, 0x8A9960,
    ]),
    // The same as upstream's High.
    (1.25, &[
        0x738843, 0x789553, 0x706C4B, 0x859F58, 0x738D44, 0x7A9453, 0x656F36, 0x5A3C17, 0x856541, 0xA27C54, 0x9B774D,
        0x361601, 0x513416, 0x66482D, 0x87694C, 0x7A5D41, 0x71553C, 0x785A3E, 0x534026, 0x7D634F, 0x8F7049, 0x866B51,
        0xA7917A, 0xCFB69B, 0xC1AD94, 0xAEA186, 0x4C5B17, 0x7D8D3C, 0x849372, 0x3D4023, 0x5F5A33, 0x5E6134, 0x6D7846,
        0x5F6832,
    ]),
];

/// The middle row of the photo in the box at `scale`, every eighth pixel from 4. With `alone`
/// the control is the only one showing the source: it gets a bitmap reduced for its box.
fn row(bytes: &[u8], quality: Option<FilterQuality>, scale: f32, alone: bool) -> Vec<Color> {
    let (width, height) = ((220.0 * scale) as i32, (120.0 * scale) as i32);
    let ui = Ui::new((), move |_: &mut ()| {
        let photo = SkiaImage::new("baboon").width_request(220).height_request(120);
        let photo = match quality {
            Some(quality) => photo.rescaling_quality(quality),
            None => photo,
        };
        // Off the canvas, a control that shows the file pixel for pixel, as the "None" tile of
        // the page does: every control of the source draws the full bitmap.
        let full = SkiaImage::new(if alone { "" } else { "baboon" })
            .aspect(TransformAspect::None)
            .width_request(8)
            .height_request(8)
            .margin((0, 900, 0, 0));
        SkiaLayout::new().fill().children((photo, full))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), width, height, scale);
    host.settle();
    host.deliver_images(|_| Some(bytes.to_vec()));
    host.settle();
    let decoded = host.ui.tree.images.get("baboon").map(|image| image.width());
    assert!(if alone { decoded < Some(1024) } else { decoded == Some(1024) }, "{decoded:?}");
    (0..).map(|i| i * 8 + 4).take_while(|x| *x < width).map(|x| host.pixel(x, height / 2)).collect()
}

/// The largest difference of a channel between the row drawn and the expected one.
fn farthest(drawn: &[Color], expected: &[u32]) -> i32 {
    assert_eq!(drawn.len(), expected.len());
    let channels = |c: Color| [c.r() as i32, c.g() as i32, c.b() as i32];
    drawn
        .iter()
        .zip(expected)
        .flat_map(|(a, b)| channels(*a).into_iter().zip(channels(Color::new(0xFF00_0000 | b))).map(|(p, q)| (p - q).abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn the_default_sampling_draws_what_react_draws() {
    let Ok(bytes) = std::fs::read(BABOON) else { return };
    for (scale, expected) in REACT {
        // The shared full bitmap: the same pixels as React, within rounding.
        let shared = farthest(&row(&bytes, None, scale, false), expected);
        assert!(shared <= 8, "shared at {scale}: {shared}");
        // Reduced for the box by the host: a little sharper than React's mip levels.
        let alone = farthest(&row(&bytes, None, scale, true), expected);
        assert!(alone <= ALONE_SLACK, "alone at {scale}: {alone}");
    }
}

/// How far a bitmap the host reduced for the box draws from React's full one, per channel: 11
/// at scale 1 and 24 at 1.25 in the fur (measured), where React blends two mip levels.
const ALONE_SLACK: i32 = 24;

#[test]
fn a_shared_bitmap_drawn_smaller_matches_the_upstream_rescale() {
    // The photo lives with the HelloRust example; without it there is nothing to compare.
    let Ok(bytes) = std::fs::read(BABOON) else { return };
    for (quality, scale, row) in ROWS {
        // None copies pixels and is exact; the filters differ by rounding (5 of 255 at most here).
        let slack = if quality == FilterQuality::None { 0 } else { 8 };
        let far = farthest(&self::row(&bytes, Some(quality), scale, false), row);
        assert!(far <= slack, "{quality:?} at {scale}: {far} from upstream");
    }
}
