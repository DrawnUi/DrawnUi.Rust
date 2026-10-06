//! A measurement, not a test: what a frame of the React Pong page costs on the CPU canvas. The
//! scene of `PongPage.tsx` (hint label, a RescalingLayout fitting the 360 x 640 field, a DrawnGame
//! with the border, two paddles, the ball and two labels), the loop moving the ball and both
//! paddles every frame. Without the paddles' bevel and GPU cache of the React sprites.
//!
//! `cargo test --release -p drawnui --test game_frame_cost -- --ignored --nocapture`

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    ball: Handle<SkiaShape>,
    player: Handle<SkiaShape>,
    ai: Handle<SkiaShape>,
    t: f32,
    /// The loop runs but moves nothing: the cost of a frame of the same page that changes nothing.
    still: bool,
}

fn paddle(color: Color, top: f32) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(80)
        .height_request(16)
        .corner_radius(8)
        .background_color(color)
        .stroke_color(Color::from_rgb(0xCC, 0xCC, 0xFF))
        .stroke_width(2)
        .left(140)
        .top(top)
}

fn page(app: &mut App) -> Build<SkiaLayout> {
    let game = DrawnGame::new()
        .width_request(360)
        .height_request(640)
        .horizontal_options(LayoutOptions::Center)
        .vertical_options(LayoutOptions::Center)
        .background_color(Color::from_rgb(0, 100, 0))
        .start_loop(0.0)
        .on_game_loop(|_me, app: &mut App, cx, delta| {
            if app.still {
                return;
            }
            app.t += delta;
            let t = app.t;
            if let Some(mut ball) = cx.get_mut(app.ball) {
                ball.set_left(173.0 + (t * 1.3).sin() * 160.0);
                ball.set_top(313.0 + (t * 0.9).sin() * 280.0);
            }
            if let Some(mut player) = cx.get_mut(app.player) {
                player.set_left(140.0 + (t * 1.7).sin() * 130.0);
            }
            if let Some(mut ai) = cx.get_mut(app.ai) {
                ai.set_left(140.0 + (t * 1.1).cos() * 130.0);
            }
        })
        .children((
            SkiaShape::new().fill().background_color(Color::TRANSPARENT).stroke_color(Color::from_rgb(0xFE, 0xFE, 0xFE)).stroke_width(2),
            paddle(Color::from_rgb(0xFF, 0x22, 0x22), 40.0).assign(&mut app.ai),
            paddle(Color::from_rgb(0x4C, 0xC9, 0xF0), 584.0).assign(&mut app.player),
            SkiaShape::new()
                .shape_type(ShapeType::Circle)
                .width_request(14)
                .height_request(14)
                .background_color(Color::YELLOW)
                .stroke_color(Color::WHITE)
                .stroke_width(2)
                .assign(&mut app.ball),
            SkiaLabel::new("3 : 5").font_size(28).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center).margin((0, 296, 0, 0)),
            SkiaLabel::new("TAP TO SERVE").font_size(14).horizontal_options(LayoutOptions::Center).margin((0, 332, 0, 0)),
        ));
    SkiaLayout::layer().vertical_options(LayoutOptions::Fill).background_color(Color::from_rgb(0x0A, 0x0F, 0x1E)).children((
        SkiaLabel::new("left right or drag to move, tap / Space to serve").font_size(13).horizontal_options(LayoutOptions::Center).margin((12, 8, 12, 0)),
        SkiaLayout::layer().vertical_options(LayoutOptions::Fill).margin((0, 36, 0, 0)).children(RescalingLayout::new(360.0, 640.0).children(game)),
    ))
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_the_pong_page() {
    const FRAMES: u32 = 600;
    for (scale, still) in [(1.0f32, true), (1.0, false), (2.0, true), (2.0, false), (3.0, true), (3.0, false)] {
        let ui = Ui::new(App { still, ..App::default() }, page).font_bytes("Default", FONT).background(Color::BLACK);
        let mut host = Headless::new(ui, (400.0 * scale) as i32, (720.0 * scale) as i32, scale);
        for _ in 0..30 {
            host.frame_after(16.0);
        }
        let start = Instant::now();
        for _ in 0..FRAMES {
            host.frame_after(16.0);
        }
        let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
        let what = if still { "nothing moves" } else { "the loop moves the ball and both paddles" };
        println!("scale {scale}, Pong page 400 x 720 pt, {what}: {micros:.0} us per frame");
    }
}
