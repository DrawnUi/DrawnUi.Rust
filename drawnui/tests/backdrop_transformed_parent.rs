//! A SkiaBackdrop inside a scaled, moved or turned parent copies the screen area it covers on screen
//! (DrawnUi.Net 6fd3bae3 BackdropTransformedParentTests, drawnui-cross 6af): with no blur, every pixel
//! it covers equals the background at that same screen point. Brightness 1.0001 makes the backdrop
//! copy (Rust skips a backdrop with no blur and brightness 1).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const STRIPES: [Color; 4] = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW];

fn render(scale: f32, translate_x: f32, rotation: f32, rotation_y: f32) -> Headless<()> {
    let ui = Ui::new((), move |_| {
        let stripes = SkiaLayout::row().spacing(0).fill_y().children(
            STRIPES.map(|c| SkiaShape::new().background_color(c).width_request(100).fill_y()).into_iter().collect::<Vec<_>>(),
        );
        SkiaLayer::new().fill().children((
            stripes,
            // The popup frame: scaled / moved / turned around its center.
            SkiaLayer::new()
                .width_request(200)
                .height_request(100)
                .center()
                .scale(scale)
                .translation_x(translate_x)
                .rotation(rotation)
                .rotation_y(rotation_y)
                .children(SkiaBackdrop::new().fill().blur(0.0).brightness(1.0001)),
        ))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    host
}

#[test]
fn a_backdrop_in_a_transformed_parent_copies_the_screen_it_covers() {
    let cases = [
        (1.0, 0.0, 0.0, 0.0),
        (0.75, 0.0, 0.0, 0.0),
        (0.5, 0.0, 0.0, 0.0),
        (1.0, 40.0, 0.0, 0.0),
        (0.75, 30.0, 0.0, 0.0),
        (1.0, 0.0, 30.0, 0.0),
        (0.75, 20.0, 30.0, 0.0),
        (1.0, 150.0, 0.0, 0.0), // partly off screen
        (1.0, 0.0, 0.0, 35.0),  // turned in 3D, with perspective (Rust's own case)
    ];
    let mut wrong_cases = Vec::new();
    for (scale, tx, rotation, rotation_y) in cases {
        let mut host = render(scale, tx, rotation, rotation_y);
        // The frame covers 200 x 100 around the center, scaled, moved, turned: a central disc that
        // stays inside it at any angle, on screen only.
        let (w, h) = (200.0 * scale, 100.0 * scale);
        let (cx, cy) = (200.0 + tx, 100.0);
        let r = w.min(h) * 0.4;
        let (mut wrong, mut total) = (0, 0);
        let mut x = (cx - r) as i32;
        while x < (cx + r) as i32 {
            let mut y = (cy - r) as i32;
            while y < (cy + r) as i32 {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                if dx * dx + dy * dy <= r * r && (0..400).contains(&x) && (0..200).contains(&y) {
                    total += 1;
                    let (a, b) = (host.pixel(x, y), STRIPES[(x / 100) as usize]);
                    let far = |p: u8, q: u8| (p as i32 - q as i32).abs() > 40;
                    if far(a.r(), b.r()) || far(a.g(), b.g()) || far(a.b(), b.b()) {
                        wrong += 1;
                    }
                }
                y += 2;
            }
            x += 2;
        }
        println!("scale={scale} tx={tx} rot={rotation} rot_y={rotation_y}: {wrong}/{total} pixels differ from the background behind");
        if wrong > 0 {
            wrong_cases.push(format!("scale={scale} tx={tx} rot={rotation} rot_y={rotation_y}: {wrong}/{total}"));
        }
    }
    assert!(wrong_cases.is_empty(), "{wrong_cases:#?}");
}
