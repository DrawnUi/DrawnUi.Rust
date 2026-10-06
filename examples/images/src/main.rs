//! A feed of photos in a recycled list: the image pipeline under scroll. Every tile is its own
//! source (four files, told apart by a `#fragment`), so every row that scrolls in loads three
//! pictures. The host decodes them off the frame thread, at the size of a tile.
//!
//! Desktop: `cargo run --release -- auto` scrolls through the feed by itself; add `full` to
//! decode every picture at its full size. Frame times are printed every 300 frames. In the
//! browser the two buttons do the same and the times are in `window.duiStats`.

use drawnui::prelude::*;

const ROWS: usize = 400;
const TILES: usize = 3;
const TILE: (f32, f32) = (320.0, 400.0);
const GAP: f32 = 8.0;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    list: Handle<SkiaLayout>,
    /// Tiles show the file pixel for pixel, cropped: the whole file is decoded.
    full: bool,
    /// Scroll to the end as soon as the first picture is there.
    auto: bool,
}

fn scroll_through(app: &App, cx: &mut Cx) {
    cx.scroll_to(app.scroll, 0.0, -(ROWS as f32 * (TILE.1 + GAP)), 40_000);
}

fn row() -> (Build<SkiaLayout>, [Handle<SkiaImage>; TILES]) {
    let mut tiles = [Handle::default(); TILES];
    let images: Vec<Build<SkiaImage>> = tiles
        .iter_mut()
        .map(|tile| {
            SkiaImage::new("")
                .width_request(TILE.0)
                .height_request(TILE.1)
                .background_color(Color::from_rgb(30, 32, 44))
                .on_success(|_me, app: &mut App, cx, _source| {
                    if std::mem::take(&mut app.auto) {
                        scroll_through(app, cx);
                    }
                })
                .assign(tile)
        })
        .collect();
    (SkiaLayout::row().spacing(GAP).children(images), tiles)
}

fn bind(tiles: &[Handle<SkiaImage>; TILES], app: &App, index: usize, cx: &mut Cx) {
    for (column, tile) in tiles.iter().enumerate() {
        let item = index * TILES + column;
        if let Some(mut tile) = cx.get_mut(*tile) {
            tile.set_aspect(if app.full { TransformAspect::AspectFill } else { TransformAspect::AspectCover });
            tile.set_source(format!("assets/photo{}.jpg#{item}", item % 4 + 1));
        }
    }
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    let feed = SkiaLayout::column()
        .spacing(GAP)
        .padding(GAP)
        .measure_items_strategy(MeasuringStrategy::MeasureFirst)
        .assign(&mut app.list)
        .items(|_: &App| ROWS, row, bind);
    SkiaLayout::new().fill().children((
        SkiaScroll::new().fill().assign(&mut app.scroll).content(feed),
        // At the bottom: the FPS counter has the top right corner.
        SkiaLayout::row().spacing(GAP).margin(GAP).horizontal_options(LayoutOptions::End).vertical_options(LayoutOptions::End).children((
            SkiaButton::new("Auto scroll").on_tapped(|_me, app: &mut App, cx| scroll_through(app, cx)),
            SkiaButton::new("Full size").on_tapped(|_me, app: &mut App, cx| {
                app.full = !app.full;
                cx.items_reset(app.list);
            }),
        )),
    ))
}

fn main() {
    drawnui::run("DrawnUI images", || {
        let has = |flag: &str| std::env::args().any(|arg| arg == flag);
        let app = App { auto: has("auto"), full: has("full"), ..App::default() };
        let ui = Ui::new(app, build).font("Default", "assets/OpenSans-Regular.ttf");
        Box::new(ui.background(Color::from_rgb(18, 18, 24)).show_fps(true))
    });
}
