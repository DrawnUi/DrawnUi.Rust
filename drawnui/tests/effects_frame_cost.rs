//! A measurement, not a test: what a frame costs for a shadowed shape, uncached and with each
//! cache type, on the CPU canvas. A shadow is a blur: painting it live or replaying it from an
//! Operations cache runs the blur every frame, an Image cache blits the result.
//!
//! `cargo test --release -p drawnui --test effects_frame_cost -- --ignored --nocapture`

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_a_shadowed_shape() {
    const FRAMES: u32 = 200;
    for scale in [1.0f32, 2.0] {
        for cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
            let shadow = SkiaShadow { x: 0.0, y: 4.0, blur: 8.0, color: Color::BLACK, opacity: 0.5, shadow_only: false };
            let card = SkiaShape::new().corner_radius(12).width_request(300).height_request(120).background_color(Color::WHITE);
            let card = card.margin(40).use_cache(cache).shadows(shadow);
            let ui = Ui::new((), |_| SkiaLayout::new().fill().children(card)).background(Color::from_rgb(240, 240, 240));
            let mut host = Headless::new(ui, (400.0 * scale) as i32, (220.0 * scale) as i32, scale);
            host.settle();
            let start = Instant::now();
            for _ in 0..FRAMES {
                host.frame_after(16.0);
            }
            let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
            println!("scale {scale}, 300 x 120 pt card, blur 8, {cache:?}: {micros:.0} us per frame");
        }
    }
}
