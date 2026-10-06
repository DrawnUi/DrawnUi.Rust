//! A measurement, not a test: what the input paths cost over a page of 1000 rows (a column of
//! 50 cards of 20 rows each, every row tappable): the route of one hover move, of one key to the
//! focused control and the window handlers, of one context menu request, and one rebuild of the
//! accessibility snapshot with every row in it. Only the engine work is timed, no frame.
//!
//! `cargo test --release -p drawnui --test input_cost -- --ignored --nocapture`

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{ContextMenuSource, PointerKind};

#[derive(Default)]
struct App {
    taps: u32,
    keys: u32,
}

fn page(accessible: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), |_| {
        let cards: Vec<Build<SkiaLayout>> = (0..50)
            .map(|_| {
                let rows: Vec<Build<SkiaShape>> = (0..20)
                    .map(|_| {
                        let row = SkiaShape::new().fill_x().height_request(2).on_tapped(|_me, app: &mut App, _cx| app.taps += 1);
                        if accessible { row.accessibility_role(Aria::BUTTON).accessibility_label("Row") } else { row }
                    })
                    .collect();
                SkiaLayout::column().spacing(0).fill_x().children(rows)
            })
            .collect();
        SkiaLayout::column().spacing(0).fill().children(cards)
    })
    .on_key_down(|app: &mut App, _event, _cx| {
        app.keys += 1;
        false
    })
    .on_context_menu(|_app: &mut App, _menu, _cx| true);
    let mut host = Headless::new(ui, 400, 2000, 1.0);
    host.settle();
    host
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn cost_of_the_input_paths() {
    const RUNS: u32 = 2000;
    let mut host = page(false);
    let time = host.time_ms();
    let per = |start: Instant| start.elapsed().as_secs_f64() * 1e6 / RUNS as f64;

    // Hover: one route through the hit path, enter / exit, the cursor. Rows alternate, so every
    // move enters one row and leaves another.
    let start = Instant::now();
    for i in 0..RUNS {
        host.ui.pointer(PointerKind::Hover, 200.0, 1000.0 + (i % 2) as f32 * 2.0, time);
        host.ui.key(KeyKind::Up, "Unknown", "", Modifiers::default(), false);
    }
    let hover = per(start);
    let start = Instant::now();
    for _ in 0..RUNS {
        host.ui.key(KeyKind::Up, "Unknown", "", Modifiers::default(), false);
    }
    let key = per(start);
    let start = Instant::now();
    for _ in 0..RUNS {
        host.ui.context_menu(200.0, 1000.0, ContextMenuSource::Mouse, time);
    }
    let menu = per(start);
    println!("1000 tappable rows: hover move {:.2} us (key included), key {key:.2} us, context menu {menu:.2} us", hover);

    let mut host = page(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let start = Instant::now();
    for _ in 0..100 {
        host.frame_after(drawnui::ACCESSIBILITY_INTERVAL_MS);
    }
    let with = start.elapsed().as_secs_f64() * 1e6 / 100.0;
    host.ui.set_accessibility_enabled(false);
    let start = Instant::now();
    for _ in 0..100 {
        host.frame_after(drawnui::ACCESSIBILITY_INTERVAL_MS);
    }
    let without = start.elapsed().as_secs_f64() * 1e6 / 100.0;
    println!(
        "1000 accessible rows ({} nodes): frame with a snapshot rebuild {with:.0} us, without {without:.0} us",
        host.ui.accessibility_nodes().len()
    );
}
