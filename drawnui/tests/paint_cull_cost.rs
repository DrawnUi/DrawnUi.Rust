//! A measurement, not a test: what a frame costs for a scroll over a long plain column (no
//! recycling), where most children are off screen, on the CPU canvas.
//!
//! `cargo test --release -p drawnui --test paint_cull_cost -- --ignored --nocapture`

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_a_long_plain_column_in_a_scroll() {
    const FRAMES: u32 = 2000;
    for rows in [50usize, 500, 5000] {
        let ui = Ui::new((), |_| {
            let cards: Vec<Build<SkiaShape>> = (0..rows)
                .map(|i| {
                    let dot = SkiaShape::new().shape_type(ShapeType::Circle).width_request(24).height_request(24).margin(8);
                    let card = SkiaShape::new().corner_radius(8).height_request(40).background_color(Color::from_rgb(40, 44, (i % 200) as u8 + 40));
                    card.horizontal_options(LayoutOptions::Fill).children(dot.background_color(Color::WHITE))
                })
                .collect();
            SkiaScroll::new().fill().content(SkiaLayout::column().spacing(8).padding(8).children(cards))
        });
        let mut host = Headless::new(ui.background(Color::BLACK), 800, 600, 1.0);
        let start = Instant::now();
        host.frame();
        let first = start.elapsed().as_secs_f64() * 1e3;
        host.settle();
        // Into the middle of the content.
        for _ in 0..3 {
            host.wheel(400.0, 300.0, -1.0);
        }
        host.settle();
        let start = Instant::now();
        for _ in 0..FRAMES {
            host.frame_after(16.0);
        }
        let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
        println!("{rows} rows, about 13 on screen: first frame {first:.1} ms, then {micros:.0} us per frame");
    }
}
