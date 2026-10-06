//! Controls outside the clip are not painted and their caches are not recorded.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    rows: Vec<Handle<SkiaShape>>,
}

#[test]
fn rows_scrolled_out_record_no_cache_until_they_come_in() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.rows = vec![Handle::default(); 100];
        let rows: Vec<Build<SkiaShape>> = app
            .rows
            .iter_mut()
            .map(|row| SkiaShape::new().horizontal_options(LayoutOptions::Fill).height_request(40).background_color(Color::GREEN).use_cache(CacheType::Image).assign(row))
            .collect();
        SkiaScroll::new().fill().content(SkiaLayout::column().spacing(0).children(rows))
    });
    let mut host = Headless::new(ui, 200, 400, 1.0);
    host.settle();
    let rows = &host.ui.state.rows;
    let (first, tenth, twelfth, last) = (rows[0], rows[9], rows[11], rows[99]);
    // 400 px show rows 0 to 9. (Row 10 touches the clip edge, which is tested a pixel wide.)
    assert_eq!((host.cache_records(first), host.cache_records(tenth)), (1, 1));
    assert_eq!((host.cache_records(twelfth), host.cache_records(last)), (0, 0));

    // One notch is 150 points: rows 3 to 13 are in, row 0 is out and keeps its cache.
    host.wheel(100.0, 200.0, -1.0);
    host.settle();
    assert_eq!((host.cache_records(first), host.cache_records(twelfth), host.cache_records(last)), (1, 1, 0));
    assert_eq!(host.pixel(100, 200), Color::GREEN);
}
