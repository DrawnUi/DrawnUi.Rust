//! The decoded path: a bitmap is asked for at the size it is shown at and decoded by the host
//! (here `Images::decode`, what the desktop host runs on its worker), never enlarged, replaced by
//! a bigger one when a bigger box asks; the cache keeps a byte budget.

mod image_common;

use drawnui::App as _;
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
fn a_bitmap_is_asked_for_in_pixels_of_its_box_and_decoded_no_larger() {
    // 100 x 60 points at 2 pixels per point.
    let mut host = host(2.0, "wide.png", &[(100.0, 60.0)]);
    let answered = host.deliver_images(bytes);
    assert_eq!(boxes(&answered), [("wide.png", 200, 120)]);
    host.settle();

    // 800 x 200 covers 200 x 120 at 0.6: 480 x 120 pixels, not 800 x 200.
    assert_eq!(decoded(&host, "wide.png"), (480, 120));
    assert_eq!(host.ui.tree.images.memory_bytes(), 480 * 120 * 4);
    // The control still measures and scales by the file's size.
    assert_eq!(image(&host, 0).source_size(), Some(ISize::new(800, 200)));
    let display = image(&host, 0).display_rect(host.rect(host.ui.state.images[0]), 2.0).unwrap();
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(close(display.left, -140.0) && close(display.top, 0.0) && close(display.right, 340.0), "{display:?}");
    // AspectCover, centered: the quadrants meet at the center of the box.
    assert_eq!(host.pixel(50, 30), Color::RED);
    assert_eq!(host.pixel(150, 30), Color::GREEN);
    assert_eq!(host.pixel(50, 90), Color::BLUE);
    assert_eq!(host.pixel(150, 90), Color::YELLOW);
}

#[test]
fn a_bitmap_is_never_enlarged_and_pixel_exact_aspects_get_every_pixel() {
    let mut host = host(1.0, "small.png", &[(100.0, 60.0)]);
    assert_eq!(boxes(&host.deliver_images(bytes)), [("small.png", 100, 60)]);
    host.settle();
    assert_eq!(decoded(&host, "small.png"), (40, 20));

    // Aspect None shows the file pixel for pixel: the whole file is asked for. The small one
    // already has every pixel, so it is good for it.
    let id = host.ui.state.images[0];
    host.ui.tree.get_mut(id).unwrap().set_aspect(TransformAspect::None);
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
    host.ui.tree.get_mut(id).unwrap().set_source("wide.png");
    host.settle();
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();
    assert_eq!(decoded(&host, "wide.png"), (800, 200));
}

#[test]
fn a_bigger_box_loads_again_and_replaces_the_smaller_bitmap() {
    let mut host = host(1.0, "", &[(40.0, 40.0), (100.0, 100.0)]);
    let (small, big) = (host.ui.state.images[0], host.ui.state.images[1]);
    // The host has the request of the first control when the second one asks.
    host.ui.tree.get_mut(small).unwrap().set_source("wide.png");
    host.settle();
    let first = host.ui.tree.images.take_requests();
    assert_eq!(boxes(&first), [("wide.png", 40, 40)]);
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
    host.ui.image(first[0].id, decode(&first[0], bytes));
    host.settle();
    // Both show what arrived; the second one waits for a bitmap good for its box.
    assert_eq!(decoded(&host, "wide.png"), (160, 40));
    assert!(image(&host, 1).image().is_some());
    assert_eq!(host.pixel(10, 50), Color::RED);
    assert_eq!(host.ui.state.log, ["wide.png", "wide.png"]);

    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 100, 100)]);
    host.settle();
    assert_eq!(decoded(&host, "wide.png"), (400, 100));
    assert_eq!(host.ui.tree.images.memory_bytes(), 400 * 100 * 4);
    let width = |host: &Headless<App>, i: usize| image(host, i).image().unwrap().width();
    assert_eq!((width(&host, 0), width(&host, 1)), (160, 400));
    // The bigger bitmap is the same picture: no second success, no other size.
    assert_eq!(host.ui.state.log.len(), 2);
    assert_eq!(host.rect(big).size(), Size::new(100.0, 100.0));
    assert_eq!(host.pixel(10, 50), Color::RED);

    // The small control grows past everything loaded: it keeps its picture and asks once more.
    host.ui.tree.get_mut(small).unwrap().set_width_request(200);
    host.ui.tree.get_mut(small).unwrap().set_height_request(200);
    host.settle();
    assert_eq!(host.pixel(10, 10), Color::RED);
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 200, 200)]);
    host.settle();
    assert_eq!(decoded(&host, "wide.png"), (800, 200));
    assert!(host.ui.tree.images.take_requests().is_empty());
    // Shrinking asks for nothing.
    host.ui.tree.get_mut(small).unwrap().set_width_request(20);
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
    assert_eq!(host.ui.state.log.len(), 2);
}

#[test]
fn controls_asking_in_one_frame_share_one_request_for_the_largest_box() {
    let mut host = host(1.0, "wide.png", &[(40.0, 40.0), (100.0, 30.0)]);
    // Wider than the first box, taller than the second.
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 100, 40)]);
    host.settle();
    assert_eq!(decoded(&host, "wide.png"), (160, 40));
    assert_eq!(host.ui.state.log, ["wide.png", "wide.png"]);
    assert!(host.ui.tree.images.take_requests().is_empty());
}

/// The host delivers in rounds: a control that joined a load already with the host and needs
/// more gets it from the same call.
#[test]
fn one_delivery_call_answers_what_its_answers_make_the_controls_ask() {
    let mut host = host(1.0, "", &[(40.0, 40.0), (100.0, 100.0)]);
    host.ui.tree.get_mut(host.ui.state.images[0]).unwrap().set_source("wide.png");
    host.settle();
    // In flight for the small box; the host code under test is `deliver_images`, so the request
    // goes back as it would arrive from a real host.
    let first = host.ui.tree.images.take_requests();
    host.ui.tree.get_mut(host.ui.state.images[1]).unwrap().set_source("wide.png");
    host.settle();
    host.ui.image(first[0].id, decode(&first[0], bytes));
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 100, 100)]);
    host.settle();
    assert_eq!(image(&host, 1).image().map(|image| image.width()), Some(400));
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
