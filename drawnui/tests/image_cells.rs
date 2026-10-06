//! Images in recycled list cells: a cell that gets another item never shows the picture of its
//! last one, not for one frame, and a list that only measures a row does not keep its load.

mod image_common;
mod list_common;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::{color, deliver_all, numbered};
use list_common::{Viewport, rows, scroll_to};

const ROW: f32 = 50.0;
const VIEW: f32 = 400.0;
const ITEMS: usize = 60;

#[derive(Default)]
struct App {
    list: Handle<SkiaLayout>,
    viewport: Handle<Viewport>,
}

/// 60 rows of 50 pixels, 8 on screen; the row of item `i` shows "item/<i>.png" over black.
fn host(strategy: MeasuringStrategy, on_first_draw: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaLayout::new().fill().children((Viewport::new(false)
            .width_request(200)
            .height_request(VIEW)
            .is_clipped_to_bounds(true)
            .assign(&mut app.viewport)
            .children((SkiaLayout::column()
                .spacing(0)
                .measure_items_strategy(strategy)
                .assign(&mut app.list)
                .items(
                    |_: &App| ITEMS,
                    move || {
                        let mut image = Handle::default();
                        // A cached cell, as list cells are: a stale cache would show the old picture too.
                        let cell = SkiaShape::new().fill_x().height_request(ROW).use_cache(CacheType::Image).children((
                            SkiaImage::new("").fill().load_source_on_first_draw(on_first_draw).assign(&mut image),
                        ));
                        (cell, image)
                    },
                    |image: &Handle<SkiaImage>, _: &App, index, cx| {
                        if let Some(mut image) = cx.get_mut(*image) {
                            image.set_source(format!("item/{index}.png"));
                        }
                    },
                ),)),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 500, 1.0);
    host.settle();
    host
}

/// Every row on screen shows its own picture when that is loaded, else nothing: never the
/// picture of another item. Returns the items on screen.
fn assert_rows(host: &mut Headless<App>, what: &str) -> Vec<usize> {
    let (viewport, list) = (host.ui.state.viewport, host.ui.state.list);
    let mut seen = Vec::new();
    for (index, top, height) in rows(host, viewport, list) {
        let middle = top + height / 2.0;
        if !(0.0..VIEW).contains(&middle) {
            continue;
        }
        let loaded = host.ui.tree.images.get(&format!("item/{index}.png")).is_some();
        let expected = if loaded { color(index) } else { Color::BLACK };
        assert_eq!(host.pixel(10, middle as i32), expected, "{what}: row of item {index}, loaded {loaded}");
        seen.push(index);
    }
    seen
}

#[test]
fn a_recycled_cell_never_shows_the_picture_of_its_last_item() {
    use MeasuringStrategy::*;
    let cases = [(MeasureFirst, false), (MeasureAll, false), (MeasureAll, true), (MeasureVisible, true)];
    for (strategy, on_first_draw) in cases {
        let what = format!("{strategy:?}, on first draw {on_first_draw}");
        let mut host = host(strategy, on_first_draw);
        let viewport = host.ui.state.viewport;
        assert_eq!(assert_rows(&mut host, &what), [0, 1, 2, 3, 4, 5, 6, 7]);
        let asked = deliver_all(&mut host, numbered);
        assert_rows(&mut host, &what);
        assert!((0..8).all(|i| host.ui.tree.images.get(&format!("item/{i}.png")).is_some()), "{what}");
        // A list that binds a cell only to measure a row does not load 60 pictures: a load
        // nobody waits for any more leaves the queue.
        assert!(asked.len() <= 8 + drawnui::Images::MAX_IN_FLIGHT, "{what}: {} loads", asked.len());
        if on_first_draw {
            // Only rows that were placed asked at all.
            assert_eq!(asked.len(), 8, "{what}: {asked:?}");
        }

        // Far away: every cell on screen shows another item now. In the very next frame the old
        // pictures are gone and the new ones have not arrived.
        scroll_to(&mut host, viewport, 1_000.0);
        host.frame();
        assert_eq!(assert_rows(&mut host, &what), [20, 21, 22, 23, 24, 25, 26, 27]);
        assert!(host.ui.tree.images.get("item/24.png").is_none(), "{what}");
        assert_eq!(host.pixel(10, 225), Color::BLACK, "{what}");
        host.settle();
        assert_rows(&mut host, &what);
        deliver_all(&mut host, numbered);
        assert_rows(&mut host, &what);
        assert_eq!(host.pixel(10, 225), color(24), "{what}");

        // Back: the pictures are cached, so they are there in the first frame and the host is
        // asked for nothing.
        scroll_to(&mut host, viewport, 0.0);
        host.frame();
        assert_eq!(assert_rows(&mut host, &what), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(host.pixel(10, 225), color(4), "{what}");
        host.settle();
        assert!(host.ui.tree.images.take_requests().is_empty(), "{what}");
    }
}
