//! A measurement, not a test: what a frame of a playing SkiaLottie costs on the CPU canvas, the
//! React AnimationsPage files at their sizes there (the frame changes every frame: the Operations
//! cache records it again and replays it).
//!
//! `cargo test --release -p drawnui --test lottie_frame_cost -- --ignored --nocapture`

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;

const OK: &[u8] = include_bytes!("lottie/ok.json");
const SHIELD: &[u8] = include_bytes!("lottie/shield.json");

fn file(url: &str) -> Option<Vec<u8>> {
    Some(if url.ends_with("ok.json") { OK } else { SHIELD }.to_vec())
}

fn measure(label: &str, scale: f32, lotties: Vec<(&'static str, f32)>) {
    const FRAMES: u32 = 300;
    let ui = Ui::new((), move |_| {
        SkiaLayout::row().spacing(8).children(
            lotties.into_iter().map(|(source, side)| SkiaLottie::new(source).width_request(side).height_request(side).repeat(-1)).collect::<Vec<_>>(),
        )
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, (560.0 * scale) as i32, (180.0 * scale) as i32, scale);
    host.frame();
    host.deliver_assets(file);
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let start = Instant::now();
    for _ in 0..FRAMES {
        host.frame_after(16.0);
    }
    let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
    println!("scale {scale}, {label}: {micros:.0} us per frame");
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn parse_time() {
    const RUNS: u32 = 50;
    for (name, json) in [("ok.json", OK), ("shield.json", SHIELD)] {
        let json = std::str::from_utf8(json).unwrap();
        let mut total = [0.0f64; 2];
        for run in 0..RUNS * 2 {
            let with = run % 2 == 0;
            let start = Instant::now();
            let ui = Ui::new((), move |_| {
                let lottie = SkiaLottie::new("").width_request(90).height_request(90).auto_play(false);
                if with { lottie.json(json) } else { lottie }
            });
            let mut host = Headless::new(ui, 100, 100, 1.0);
            host.frame();
            total[with as usize] += start.elapsed().as_secs_f64();
        }
        let micros = (total[1] - total[0]) * 1e6 / RUNS as f64;
        println!("{name} ({} bytes): parse + model build {micros:.0} us", json.len());
    }
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_playing_lotties() {
    for scale in [1.0f32, 2.0] {
        measure("nothing", scale, Vec::new());
        measure("shield.json 160 pt", scale, vec![("lottie/shield.json", 160.0)]);
        measure("ok.json 90 pt", scale, vec![("lottie/ok.json", 90.0)]);
        measure("ColorTint card: 3 x ok.json + shield.json at 90 pt", scale, vec![
            ("lottie/ok.json", 90.0),
            ("lottie/ok.json", 90.0),
            ("lottie/ok.json", 90.0),
            ("lottie/shield.json", 90.0),
        ]);
    }
}
