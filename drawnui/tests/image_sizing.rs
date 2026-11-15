//! The decoded path: the host decodes the file (here `Images::decode`, what the desktop host runs on
//! its worker), turned as its EXIF says; every control of a source shares the one copy; the cache
//! keeps a byte budget. Controls always ask for the whole file (`image_full_decode`).

mod image_common;

use drawnui::prelude::*;
use drawnui::skia::ISize;
use drawnui::testing::Headless;
use drawnui::{ImageRequest, Images};
use image_common::*;

#[derive(Default)]
struct App {
    images: Vec<Handle<SkiaImage>>,
    log: Vec<String>,
}

/// Images stacked at the left edge, top to bottom with no gap; `boxes` are their sizes in points.
fn host(scale: f32, source: &'static str, boxes: &'static [(f32, f32)]) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        app.images = vec![Handle::default(); boxes.len()];
        let tiles: Vec<Build<SkiaImage>> = boxes
            .iter()
            .zip(&mut app.images)
            .map(|((w, h), handle)| (SkiaImage::new(source).width_request(*w).height_request(*h), handle))
            .map(|(image, handle)| {
                image.on_success(|_me, app: &mut App, _cx, source| app.log.push(source.to_owned())).assign(handle)
            })
            .collect();
        SkiaLayout::column().spacing(0).children(tiles)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 400, scale);
    host.settle();
    host
}

fn bytes(source: &str) -> Option<Vec<u8>> {
    match source {
        "wide.png" => Some(quadrants(800, 200)),
        "small.png" => Some(quadrants(40, 20)),
        "photo.jpg" => Some(jpeg(1600, 1200, |canvas| {
            canvas.clear(Color::from_rgb(40, 90, 160));
        })),
        "turned.jpg" => Some(with_orientation(&jpeg_quadrants(40, 20), 6)),
        _ => None,
    }
}

fn boxes(requests: &[ImageRequest]) -> Vec<(&str, u32, u32)> {
    requests.iter().map(|r| (r.source.as_str(), r.width, r.height)).collect()
}

fn image(host: &Headless<App>, i: usize) -> &SkiaImage {
    host.ui.tree.find::<SkiaImage>(host.ui.state.images[i]).unwrap()
}

fn decoded(host: &Headless<App>, source: &str) -> (i32, i32) {
    let image = host.ui.tree.images.get(source).expect("loaded");
    (image.width(), image.height())
}

#[test]
fn a_jpeg_decodes_at_eighths_of_its_size() {
    let full = Images::decode(&bytes("photo.jpg").unwrap(), 0, 0).unwrap();
    assert_eq!((full.image.width(), full.image.height(), full.source_size), (1600, 1200, ISize::new(1600, 1200)));
    // 200 x 150 is exactly one eighth.
    let eighth = Images::decode(&bytes("photo.jpg").unwrap(), 200, 150).unwrap();
    assert_eq!((eighth.image.width(), eighth.image.height(), eighth.source_size), (200, 150, ISize::new(1600, 1200)));
    // Between two eighths, and far below one: decoded at the next eighth up, then reduced to
    // what was asked, so the cache never holds more.
    let between = Images::decode(&bytes("photo.jpg").unwrap(), 300, 225).unwrap();
    assert_eq!((between.image.width(), between.image.height()), (300, 225));
    let tiny = Images::decode(&bytes("photo.jpg").unwrap(), 40, 0).unwrap();
    assert_eq!((tiny.image.width(), tiny.image.height()), (40, 30));
    // One side only counts when the other is 0; a larger box than the file gives the file.
    let tall = Images::decode(&bytes("photo.jpg").unwrap(), 0, 600).unwrap();
    assert_eq!((tall.image.width(), tall.image.height()), (800, 600));
    let all = Images::decode(&bytes("photo.jpg").unwrap(), 4000, 100).unwrap();
    assert_eq!((all.image.width(), all.image.height()), (1600, 1200));
    assert!(Images::decode(b"plain text", 0, 0).is_none());
}

#[test]
fn a_photo_is_turned_as_its_exif_orientation_says() {
    // Stored 40 x 20 with orientation 6: shown 20 x 40, turned a quarter clockwise.
    let mut host = host(1.0, "turned.jpg", &[(20.0, 40.0)]);
    host.deliver_images(bytes);
    host.settle();
    assert_eq!(image(&host, 0).source_size(), Some(ISize::new(20, 40)));
    assert_eq!(decoded(&host, "turned.jpg"), (20, 40));
    let near = |pixel: Color, color: Color| {
        let far = |a: u8, b: u8| (a as i32 - b as i32).abs() > 60;
        !(far(pixel.r(), color.r()) || far(pixel.g(), color.g()) || far(pixel.b(), color.b()))
    };
    // The stored left column is the top row now: blue, red / yellow, green.
    for (x, y, color) in [(5, 10, Color::BLUE), (15, 10, Color::RED), (5, 30, Color::YELLOW), (15, 30, Color::GREEN)] {
        assert!(near(host.pixel(x, y), color), "({x}, {y}) is {:?}", host.pixel(x, y));
    }
}

#[test]
fn the_cache_keeps_a_byte_budget_by_dropping_the_bitmaps_shown_longest_ago() {
    const BITMAP: usize = 8 * 8 * 4;
    let mut host = host(1.0, "1.png", &[(20.0, 20.0), (20.0, 20.0)]);
    let (first, second) = (host.ui.state.images[0], host.ui.state.images[1]);
    assert_eq!(host.ui.tree.images.budget(), Images::DEFAULT_BUDGET);
    host.ui.tree.images.set_budget(2 * BITMAP);
    let show = |host: &mut Headless<App>, control: Handle<SkiaImage>, source: &str| {
        host.ui.tree.get_mut(control).unwrap().set_source(source);
        host.settle();
        host.deliver_images(numbered);
        host.settle();
    };
    let cached = |host: &Headless<App>| {
        let loaded = |n: &usize| host.ui.tree.images.get(&format!("{n}.png")).is_some();
        (1..=6).filter(loaded).collect::<Vec<usize>>()
    };
    show(&mut host, first, "1.png");
    // The first control goes through three more pictures; the second keeps showing 1.
    show(&mut host, first, "2.png");
    show(&mut host, first, "3.png");
    // Over the budget by one: 1 and 3 are on screen, so 2 goes.
    assert_eq!(cached(&host), [1, 3]);
    assert_eq!(host.ui.tree.images.memory_bytes(), 2 * BITMAP);

    show(&mut host, second, "3.png");
    show(&mut host, first, "4.png");
    // 1 is shown by nobody now and goes; 3 and 4 are on screen.
    assert_eq!(cached(&host), [3, 4]);

    // Everything on screen is kept, whatever the budget.
    host.ui.tree.images.set_budget(0);
    assert_eq!(cached(&host), [3, 4]);
    assert_eq!(host.ui.tree.images.memory_bytes(), 2 * BITMAP);
    assert_eq!((host.pixel(10, 10), host.pixel(10, 30)), (color(4), color(3)));

    // Of the bitmaps nobody shows, the one shown longest ago goes first.
    host.ui.tree.images.set_budget(4 * BITMAP);
    show(&mut host, first, "5.png");
    show(&mut host, first, "6.png");
    assert_eq!(cached(&host), [3, 4, 5, 6]);
    show(&mut host, first, "4.png");
    show(&mut host, first, "1.png");
    // Idle: 5, 6 and 4; 4 was shown last, 5 longest ago.
    assert_eq!(cached(&host), [1, 3, 4, 6]);
    assert_eq!(host.ui.tree.images.memory_bytes(), 4 * BITMAP);
}

/// Many controls of one source that want different sizes, as a gallery page has: one round of
/// answers from the host, and every control has a bitmap at least as large as it asked for.
#[test]
fn controls_of_one_source_each_end_with_a_bitmap_large_enough() {
    use TransformAspect::*;
    const ASPECTS: [TransformAspect; 6] = [AspectCover, AspectFit, AspectFill, Fill, Cover, None];
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.images = vec![Handle::default(); ASPECTS.len()];
        let tiles: Vec<Build<SkiaImage>> = ASPECTS
            .iter()
            .zip(&mut app.images)
            .map(|(aspect, handle)| (SkiaImage::new("wide.png").width_request(60).height_request(30), aspect, handle))
            .map(|(image, aspect, handle)| image.aspect(*aspect).assign(handle))
            .collect();
        SkiaLayout::column().spacing(0).children(tiles)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 400, 1.0);
    host.settle();
    // The first control alone would be served by 120 x 30 pixels; three of the others show the
    // file pixel for pixel. One request, for the most any of them needs.
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();
    for (i, aspect) in ASPECTS.iter().enumerate() {
        assert_eq!(image(&host, i).image().map(|image| image.width()), Some(800), "{aspect:?}");
    }
    // None shows the middle of the file one to one: all red left of the center line, green right.
    assert_eq!((host.pixel(10, 160), host.pixel(50, 160)), (Color::RED, Color::GREEN));
    assert!(host.ui.tree.images.take_requests().is_empty());
}
