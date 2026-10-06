//! HelloRust's Uneven cells page (DrawnUi.React UnevenCellsPage): 200 posts of 1 to 6 lines,
//! MeasureVisible, LoadMore at both ends with handlers that load a page 400 ms later behind a busy
//! flag. Each end is called once per page, whatever the list measures ahead in the meantime. A
//! page prepended above the viewport is measured in the frame it lands as far as the frame's
//! budget goes, the rest in the frames after it: the posts on screen never move, the offset takes
//! the real size of what was measured. Every case runs with the budgets that made the calls flaky
//! (a few rows measured ahead per frame, as a slow debug build does) and with the default one,
//! which the headless host counts as 4 rows a frame.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

const WORDS: [&str; 28] = [
    "drawn", "ui", "renders", "every", "pixel", "itself", "skia", "canvas", "recycled", "cells", "measure", "visible",
    "estimates", "the", "rest", "and", "refines", "in", "idle", "time", "uneven", "rows", "news", "feed", "social",
    "timeline", "product", "catalog",
];

/// The page's post: a body of 4 to 59 words, the same for the same id (C# / React `makeItem`).
fn body(id: i32) -> String {
    let mut seed = ((id as i64 + 100_000) * 2_654_435_761) as u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed as f64 / 4_294_967_296.0
    };
    let count = 4 + (rnd() * 56.0) as usize;
    let words: Vec<&str> = (0..count).map(|_| WORDS[(rnd() * WORDS.len() as f64) as usize]).collect();
    words.join(" ")
}

#[derive(Default)]
struct Page {
    ids: Vec<i32>,
    /// Posts a load at the start prepends.
    top_page: i32,
    busy: bool,
    top_loads: u32,
    loads: u32,
    scroll: Handle<SkiaScroll>,
    feed: Handle<SkiaLayout>,
}

#[derive(Default)]
struct Handles {
    title: Handle<SkiaLabel>,
    body: Handle<SkiaLabel>,
}

fn load(app: &mut Page, cx: &mut Cx, scroll: ControlId, top: bool) {
    if std::mem::replace(&mut app.busy, true) {
        return;
    }
    if top {
        app.top_loads += 1;
    } else {
        app.loads += 1;
    }
    cx.after(scroll, 400, move |app: &mut Page, cx| {
        if top {
            let (first, count) = (app.ids[0], app.top_page);
            app.ids.splice(0..0, first - count..first);
            cx.items_inserted(app.feed, 0, count as usize);
        } else {
            let last = *app.ids.last().unwrap();
            app.ids.extend(last + 1..=last + 100);
        }
        app.busy = false;
    });
}

fn page(budget: MeasureBudget) -> Headless<Page> {
    page_of(budget, 30)
}

fn page_of(budget: MeasureBudget, top_page: i32) -> Headless<Page> {
    let build = move |app: &mut Page| {
        SkiaLayer::new().fill().children((SkiaScroll::new()
            .fill()
            .margin((0, 36, 0, 0))
            .load_more_offset(300)
            .load_more_top_offset(100)
            .assign(&mut app.scroll)
            .on_load_more(|me, app: &mut Page, cx| load(app, cx, me.id(), false))
            .on_load_more_top(|me, app: &mut Page, cx| load(app, cx, me.id(), true))
            .content(
                SkiaStack::new()
                    .recycling_template(RecyclingTemplate::Enabled)
                    .measure_items_strategy(MeasuringStrategy::MeasureVisible)
                    .measure_budget(budget)
                    .spacing(8)
                    .padding((16, 8))
                    .assign(&mut app.feed)
                    .items(
                        |app: &Page| app.ids.len(),
                        || {
                            let mut handles = Handles::default();
                            let cell = SkiaLayout::new().fill_x().padding((16, 12, 16, 12)).background_color(Color::DARK_GRAY).children((
                                SkiaStack::new().spacing(6).margin((18, 0, 0, 0)).children((
                                    SkiaLabel::new("").font_size(15).assign(&mut handles.title),
                                    SkiaLabel::new("").font_size(13).fill_x().assign(&mut handles.body),
                                )),
                            ));
                            (cell, handles)
                        },
                        |cell: &Handles, app: &Page, index, cx| {
                            let id = app.ids[index];
                            if let Some(mut title) = cx.get_mut(cell.title) {
                                title.set_text(format!("Post {id}"));
                            }
                            if let Some(mut text) = cx.get_mut(cell.body) {
                                text.set_text(body(id));
                            }
                        },
                    ),
            ),),)
    };
    let ui = Ui::new(Page { ids: (1..=200).collect(), top_page, ..Page::default() }, build).font_bytes("Default", FONT).background(Color::BLACK);
    Headless::new(ui, 400, 800, 1.0)
}

fn offset(host: &Headless<Page>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// The budgets that made the calls flaky, and the usual one.
const BUDGETS: [MeasureBudget; 7] = [
    MeasureBudget::Items(0),
    MeasureBudget::Items(1),
    MeasureBudget::Items(2),
    MeasureBudget::Items(3),
    MeasureBudget::Items(5),
    MeasureBudget::Items(30),
    MeasureBudget::Millis(4.0),
];

/// The posts with a visible cell: (id, top on screen in pixels), top to bottom.
fn on_screen(host: &Headless<Page>) -> Vec<(i32, f32)> {
    let (tree, app) = (&host.ui.tree, &host.ui.state);
    let offset = tree.base(app.scroll).unwrap().content_offset.y;
    let mut posts: Vec<(i32, f32)> = tree
        .children(app.feed)
        .iter()
        .filter_map(|cell| tree.base(*cell))
        .filter(|base| base.p.is_visible)
        .filter_map(|base| Some((app.ids[base.context_index?], base.rect.top + offset)))
        .collect();
    posts.sort_by(|a, b| a.1.total_cmp(&b.1));
    posts
}

#[test]
fn the_start_calls_once_and_its_page_comes_in_above_without_moving_the_posts_on_screen() {
    for budget in BUDGETS {
        let mut host = page(budget);
        host.frame_after(16.0);
        // As React: not at open. Away from the start, then back into its zone: one call.
        let scroll = host.ui.state.scroll;
        host.ui.tree.cx().scroll_to(scroll, 0.0, -2_000.0, 0);
        host.frame_after(16.0);
        host.ui.tree.cx().scroll_to(scroll, 0.0, -50.0, 0);
        host.frame_after(16.0);
        assert_eq!(host.ui.state.top_loads, 1, "{budget:?}");
        let (before, offset_before) = (on_screen(&host), offset(&host));

        // The page lands 400 ms later: 30 posts above. No post on screen moves in any frame.
        let (mut landed, mut settled) = (None, None);
        for frame in 0..90 {
            host.frame_after(16.0);
            if landed.is_none() && host.ui.state.ids.len() == 230 {
                landed = Some((frame, offset(&host)));
            }
            if landed.is_some() {
                let now = on_screen(&host);
                for post in &before {
                    if let Some(now) = now.iter().find(|p| p.0 == post.0) {
                        assert_eq!(now.1, post.1, "{budget:?}: post {} moved {frame}", post.0);
                    }
                }
                let list = host.ui.tree.find::<SkiaLayout>(host.ui.state.feed).unwrap();
                if settled.is_none() && (0..30).all(|row| list.is_item_measured(row)) {
                    settled = Some(frame);
                }
            }
        }
        let (landed, at) = landed.expect("the page came");
        assert_eq!((host.ui.state.top_loads, host.ui.state.ids.len()), (1, 230), "{budget:?}");
        // Measured by the budget: in the landing frame when it takes 30 rows, else in as many
        // frames as it takes, and not at all with none.
        let per_frame = match budget {
            MeasureBudget::Items(items) => items as usize,
            MeasureBudget::Millis(ms) => ms as usize,
        };
        match per_frame {
            0 => assert_eq!(settled, None, "{budget:?}"),
            rows => assert_eq!(settled, Some(landed + 30usize.div_ceil(rows) - 1), "{budget:?}"),
        }
        if per_frame >= 30 {
            assert_eq!(offset(&host), at, "{budget:?}: measured in the landing frame, the offset stays");
        }
        if per_frame > 0 {
            let list = host.ui.tree.find::<SkiaLayout>(host.ui.state.feed).unwrap();
            let inserted = list.item_offset_pixels(30) - list.item_offset_pixels(0);
            assert_eq!(offset(&host), offset_before - inserted, "{budget:?}");
        }
    }
}

#[test]
fn the_end_calls_once_while_its_page_is_on_its_way() {
    for budget in BUDGETS {
        let mut host = page(budget);
        host.frame_after(16.0);
        let scroll = host.ui.state.scroll;
        host.ui.tree.cx().scroll_to(scroll, 0.0, -1.0e6, 0);
        for _ in 0..90 {
            host.frame_after(16.0);
        }
        // The rows measured ahead change the length of the list every frame: no new content.
        assert_eq!((host.ui.state.loads, host.ui.state.ids.len()), (1, 300), "{budget:?}");
    }
}

/// A measurement, not a test: the frame in which a page lands above the viewport, next to the
/// frames around it, for 30 and 200 posts, uncapped (a budget of 1000 rows) and with 4 rows (the
/// default 4 ms budget as the headless host counts it; on a real clock the cap is 4 ms of rows).
/// `cargo test --release -p drawnui --test list_load_more_top -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_a_page_landing_above() {
    for (budget, top_page) in [(MeasureBudget::Items(1000), 30), (MeasureBudget::Items(1000), 200), (MeasureBudget::Millis(4.0), 30), (MeasureBudget::Millis(4.0), 200)] {
        let mut host = page_of(budget, top_page);
        host.frame_after(16.0);
        let scroll = host.ui.state.scroll;
        host.ui.tree.cx().scroll_to(scroll, 0.0, -2_000.0, 0);
        host.frame_after(16.0);
        host.ui.tree.cx().scroll_to(scroll, 0.0, -50.0, 0);
        let (mut landing, mut others) = (0.0, Vec::new());
        for _ in 0..60 {
            let (count, started) = (host.ui.state.ids.len(), std::time::Instant::now());
            host.frame_after(16.0);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            if host.ui.state.ids.len() != count { landing = ms } else { others.push(ms) }
        }
        others.sort_by(f64::total_cmp);
        let (median, max) = (others[others.len() / 2], others[others.len() - 1]);
        println!("{top_page} posts landing above, {budget:?}: landing frame {landing:.2} ms; other frames median {median:.2} ms, max {max:.2} ms");
    }
}
