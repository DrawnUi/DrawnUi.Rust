//! The image manager through the headless host: one load per source, the in-flight limit and the
//! order of the queue, preloads, failures, source changes, handlers, and what an arriving bitmap
//! invalidates. The test plays the host: it takes the requests and answers with `App::asset`.

mod image_common;

use std::cell::Cell;
use std::rc::Rc;

use drawnui::App as _;
use drawnui::{Detached, ImageRequest};
use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::*;

#[derive(Default)]
struct App {
    images: Vec<Handle<SkiaImage>>,
    shape: Handle<SkiaShape>,
    log: Vec<String>,
}

/// A row of 20 x 20 images, one per source, image `i` at x = 20 * i.
fn gallery(sources: &[&str], each: fn(Build<SkiaImage>) -> Build<SkiaImage>) -> Headless<App> {
    let sources: Vec<String> = sources.iter().map(|s| s.to_string()).collect();
    let ui = Ui::new(App::default(), move |app: &mut App| {
        app.images = vec![Handle::default(); sources.len()];
        let tiles: Vec<Build<SkiaImage>> = sources
            .iter()
            .zip(&mut app.images)
            .map(|(source, handle)| (SkiaImage::new(source.as_str()).width_request(20).height_request(20), handle))
            .map(|(image, handle)| each(image).assign(handle))
            .collect();
        SkiaLayout::row().spacing(0).children(tiles)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 100, 1.0);
    host.settle();
    host
}

fn plain(image: Build<SkiaImage>) -> Build<SkiaImage> {
    image
}

/// The pixel in the middle of image `i`.
fn shows(host: &mut Headless<App>, i: usize) -> Color {
    host.pixel(20 * i as i32 + 10, 10)
}

fn image(host: &Headless<App>, i: usize) -> &SkiaImage {
    host.ui.tree.find::<SkiaImage>(host.ui.state.images[i]).unwrap()
}

fn set_source(host: &mut Headless<App>, i: usize, source: &str) {
    host.ui.tree.get_mut(host.ui.state.images[i]).unwrap().set_source(source);
}

#[test]
fn the_same_source_in_many_controls_is_one_request_and_one_bitmap() {
    let mut host = gallery(&["7.png", "7.png", "7.png"], plain);
    assert_eq!(shows(&mut host, 0), Color::BLACK);
    assert!(image(&host, 0).is_loading());
    // A control added while the load is with the host waits for the same one.
    let root = host.ui.tree.root().unwrap();
    let late = host.ui.tree.add_child(root, SkiaImage::new("7.png").width_request(20).height_request(20));
    host.settle();

    assert_eq!(deliver(&mut host, numbered), ["7.png"]);
    host.settle();
    for i in 0..4 {
        assert_eq!(shows(&mut host, i), color(7), "image {i}");
    }
    assert!(!image(&host, 0).is_loading());
    // One decoded 8 x 8 bitmap, shared.
    assert_eq!(host.ui.tree.images.memory_bytes(), 8 * 8 * 4);
    let shared = |id: ControlId| host.ui.tree.find::<SkiaImage>(id).unwrap().image().unwrap().unique_id();
    assert_eq!(shared(late), shared(host.ui.state.images[0].into()));
    assert_eq!(Some(shared(late)), host.ui.tree.images.get("7.png").map(|image| image.unique_id()));
    assert!(host.ui.tree.images.take_requests().is_empty());

    // Cleared: the controls keep their bitmap, the next one to ask loads again.
    host.ui.tree.images.clear();
    assert_eq!(host.ui.tree.images.memory_bytes(), 0);
    host.ui.tree.add_child(root, SkiaImage::new("7.png").width_request(20).height_request(20));
    host.settle();
    assert_eq!(shows(&mut host, 0), color(7));
    assert_eq!(shows(&mut host, 4), Color::BLACK);
    assert_eq!(deliver(&mut host, numbered), ["7.png"]);
}

#[test]
fn five_loads_at_once_controls_before_preloads() {
    let mut host = gallery(&["0.png", "1.png", "2.png", "3.png", "4.png", "5.png", "6.png", "7.png"], plain);
    let mut with_host = host.ui.tree.images.take_requests();
    let sources = |requests: &[ImageRequest]| requests.iter().map(|r| r.source.clone()).collect::<Vec<_>>();
    assert_eq!(drawnui::Images::MAX_IN_FLIGHT, 5);
    assert_eq!(sources(&with_host), ["0.png", "1.png", "2.png", "3.png", "4.png"]);

    // Preloads wait behind what controls want; one a control asks for goes before the others.
    host.ui.tree.images.preload(["20.png", "21.png", "0.png"]);
    set_source(&mut host, 0, "21.png");
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());

    // Every answer frees one slot for the next in line.
    let mut order = Vec::new();
    while !with_host.is_empty() {
        let request = with_host.remove(0);
        host.ui.image(request.id, decode(&request, numbered));
        host.settle();
        let next = host.ui.tree.images.take_requests();
        assert!(next.len() <= 1);
        order.extend(sources(&next));
        with_host.extend(next);
    }
    assert_eq!(order, ["5.png", "6.png", "7.png", "21.png", "20.png"]);
    assert_eq!(shows(&mut host, 0), color(21));
    assert_eq!(shows(&mut host, 7), color(7));
    // Preloaded and loaded for a control that left it: both are in the cache.
    assert!(host.ui.tree.images.get("20.png").is_some() && host.ui.tree.images.get("0.png").is_some());
}

/// React's ImagesPage preload card: `PreloadImages(8 urls, Low)` awaited, the queue counts read
/// from the manager meanwhile.
#[test]
fn a_preload_handler_runs_once_every_source_arrived_or_failed() {
    let mut host = gallery(&["0.png"], plain);
    deliver_all(&mut host, numbered);
    let sources = ["10.png", "11.png", "12.png", "13.png", "14.png", "15.png", "bad.png", "10.png"];
    host.ui.tree.cx().preload_images_then(sources, |app: &mut App, cx| {
        let images = cx.images();
        app.log.push(format!("done, {} in flight, {} queued", images.in_flight(), images.queued()));
    });
    host.frame();
    let mut first = host.ui.tree.images.take_requests();
    assert_eq!((first.len(), host.ui.tree.images.in_flight(), host.ui.tree.images.queued()), (5, 5, 2));
    // Four of the five answered: the rest still loads, nothing runs.
    let last = first.pop().unwrap();
    for request in first {
        host.ui.image(request.id, decode(&request, numbered));
    }
    host.settle();
    assert!(host.ui.state.log.is_empty());
    host.ui.image(last.id, decode(&last, numbered));
    host.settle();
    assert!(host.ui.state.log.is_empty());
    // The last two, one of them a failure: over, once.
    deliver_all(&mut host, numbered);
    host.settle();
    assert_eq!(host.ui.state.log, ["done, 0 in flight, 0 queued"]);

    // Everything loaded already: the handler runs in the next frame.
    host.ui.tree.cx().preload_images_then(["10.png", "11.png"], |app: &mut App, _cx| app.log.push("cached".to_owned()));
    host.frame();
    assert!(host.ui.tree.images.take_requests().is_empty());
    assert_eq!(host.ui.state.log, ["done, 0 in flight, 0 queued", "cached"]);
}

#[test]
fn a_queued_load_nobody_waits_for_is_dropped() {
    let mut host = gallery(&["0.png", "1.png", "2.png", "3.png", "4.png", "5.png", "6.png"], plain);
    // "5.png" and "6.png" wait for a slot. Image 6 moves on before its load started.
    set_source(&mut host, 6, "9.png");
    host.settle();
    let answered = deliver_all(&mut host, numbered);
    assert_eq!(answered, ["0.png", "1.png", "2.png", "3.png", "4.png", "5.png", "9.png"]);
    assert_eq!(shows(&mut host, 6), color(9));
    assert!(host.ui.tree.images.get("6.png").is_none());
}

#[test]
fn a_new_source_clears_the_picture_at_once() {
    let mut host = gallery(&["1.png"], plain);
    deliver(&mut host, numbered);
    host.settle();
    assert_eq!(shows(&mut host, 0), color(1));

    set_source(&mut host, 0, "2.png");
    // The very next frame: nothing of the old picture. The host gets its requests after a frame.
    host.frame();
    let mut requests = host.ui.tree.images.take_requests();
    assert_eq!(shows(&mut host, 0), Color::BLACK);
    assert!(image(&host, 0).image().is_none() && image(&host, 0).is_loading());

    // It moves on again before "2.png" arrived: the late bitmap is cached, not shown.
    set_source(&mut host, 0, "3.png");
    host.frame();
    requests.extend(host.ui.tree.images.take_requests());
    assert_eq!(requests.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), ["2.png", "3.png"]);
    host.ui.image(requests[0].id, decode(&requests[0], numbered));
    host.settle();
    assert_eq!(shows(&mut host, 0), Color::BLACK);
    assert!(host.ui.tree.images.get("2.png").is_some());
    host.ui.image(requests[1].id, decode(&requests[1], numbered));
    host.settle();
    assert_eq!(shows(&mut host, 0), color(3));

    // Back to a loaded one: there in the next frame, nothing asked from the host.
    set_source(&mut host, 0, "1.png");
    host.frame();
    assert_eq!(shows(&mut host, 0), color(1));
    // No source: no picture.
    set_source(&mut host, 0, "");
    host.frame();
    assert_eq!(shows(&mut host, 0), Color::BLACK);
    assert!(!image(&host, 0).is_loading());
    assert!(host.ui.tree.images.take_requests().is_empty());
}

#[test]
fn a_failed_load_is_reported_once_and_tried_again_only_when_asked() {
    fn logged(image: Build<SkiaImage>) -> Build<SkiaImage> {
        image
            .on_success(|_me, app: &mut App, _cx, source| app.log.push(format!("ok {source}")))
            .on_error(|_me, app: &mut App, _cx, source| app.log.push(format!("error {source}")))
    }
    let mut host = gallery(&["missing.png", "missing.png", "not-an-image.png"], logged);
    // The host could not read one; the other is not a picture.
    let answer = |source: &str| (source == "not-an-image.png").then(|| b"plain text".to_vec());
    assert_eq!(deliver(&mut host, answer), ["missing.png", "not-an-image.png"]);
    host.settle();
    assert_eq!(host.ui.state.log, ["error missing.png", "error missing.png", "error not-an-image.png"]);
    assert!(image(&host, 0).has_error() && !image(&host, 0).is_loading());
    assert_eq!(shows(&mut host, 0), Color::BLACK);

    // Frames go by: nobody asks again.
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert!(host.ui.tree.images.take_requests().is_empty());
    assert_eq!(host.ui.state.log.len(), 3);

    // Setting the source again is a new try (upstream: a failure is not cached).
    set_source(&mut host, 0, "1.png");
    host.settle();
    // The host has the request when the control moves on.
    let mut requests = host.ui.tree.images.take_requests();
    set_source(&mut host, 0, "missing.png");
    host.settle();
    requests.extend(host.ui.tree.images.take_requests());
    assert_eq!(requests.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), ["1.png", "missing.png"]);
    for request in &requests {
        host.ui.image(request.id, decode(request, numbered));
    }
    host.settle();
    assert_eq!(host.ui.state.log[3..], ["error missing.png"]);
    // "1.png" arrived for nobody: no handler ran, but it is cached and reported when shown.
    set_source(&mut host, 0, "1.png");
    host.settle();
    assert_eq!(host.ui.state.log[4..], ["ok 1.png"]);
    assert!(!image(&host, 0).has_error());
    assert_eq!(shows(&mut host, 0), color(1));
}

#[test]
fn the_success_handler_runs_before_the_first_paint() {
    // The upstream fade-in recipe starts here: the handler hides the image before it ever shows.
    fn hidden(image: Build<SkiaImage>) -> Build<SkiaImage> {
        image.on_success(|me, app: &mut App, cx, source| {
            app.log.push(source.to_owned());
            cx.any_mut(me).unwrap().set_opacity(0.0);
        })
    }
    let mut host = gallery(&["1.png"], hidden);
    deliver(&mut host, numbered);
    host.frame();
    assert_eq!(host.ui.state.log, ["1.png"]);
    assert_eq!(shows(&mut host, 0), Color::BLACK);

    // The same for a bitmap that is cached when the source is set.
    host.ui.tree.images.preload(["2.png"]);
    deliver(&mut host, numbered);
    host.ui.tree.any_mut(host.ui.state.images[0]).unwrap().set_opacity(1.0);
    host.settle();
    assert_eq!(shows(&mut host, 0), color(1));
    set_source(&mut host, 0, "2.png");
    host.frame();
    assert_eq!(host.ui.state.log, ["1.png", "2.png"]);
    assert_eq!(shows(&mut host, 0), Color::BLACK);
}

// ---------------------------------------------------------------- invalidation

/// A layout that counts its measures.
struct Counting {
    layout: SkiaLayout,
    measures: Rc<Cell<u32>>,
}
impl Container for Counting {}
impl Control for Counting {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.measures.set(self.measures.get() + 1);
        self.layout.measure(cx, width, height)
    }
}

/// A counting layout with one image (cached) and a cached shape beside it.
fn counted(sized: bool) -> (Headless<App>, Rc<Cell<u32>>) {
    let measures = Rc::new(Cell::new(0));
    let counter = measures.clone();
    let ui = Ui::new(App::default(), move |app: &mut App| {
        app.images = vec![Handle::default()];
        let image = SkiaImage::new("1.png").aspect(TransformAspect::None).use_cache(CacheType::Image);
        let image = if sized { image.width_request(20).height_request(20) } else { image };
        Build::new(Counting { layout: SkiaLayout::default(), measures: counter }).fill().children((
            image.assign(&mut app.images[0]),
            SkiaShape::new()
                .margin((100, 0, 0, 0))
                .width_request(20)
                .height_request(20)
                .background_color(Color::WHITE)
                .use_cache(CacheType::Image)
                .assign(&mut app.shape),
        ))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 100, 1.0);
    host.settle();
    (host, measures)
}

#[test]
fn nothing_happens_while_waiting_and_an_arrival_draws_once() {
    let (mut host, measures) = counted(true);
    let (image, shape) = (host.ui.state.images[0], host.ui.state.shape);
    let requests = host.ui.tree.images.take_requests();
    assert_eq!(requests.len(), 1);

    // Waiting for the host asks for no frame, and a frame somebody else asked for does nothing here.
    assert!(!host.ui.needs_frame());
    let before = (measures.get(), host.cache_records(image), host.cache_records(shape));
    for _ in 0..3 {
        host.frame_after(16.0);
        assert!(!host.ui.needs_frame());
    }
    assert_eq!((measures.get(), host.cache_records(image), host.cache_records(shape)), before);

    // The bitmap arrives: a frame is due, the image's cache is recorded once, no layout, nothing else redrawn.
    host.ui.image(requests[0].id, decode(&requests[0], numbered));
    assert!(host.ui.needs_frame());
    host.frame_after(16.0);
    let after = (before.0, before.1 + 1, before.2);
    assert_eq!((measures.get(), host.cache_records(image), host.cache_records(shape)), after);
    assert_eq!(host.pixel(10, 10), color(1));
    assert_eq!(host.pixel(110, 10), Color::WHITE);
    host.settle();
    let after = (before.0, before.1 + 1, before.2);
    assert_eq!((measures.get(), host.cache_records(image), host.cache_records(shape)), after);
}

#[test]
fn an_auto_sized_image_is_measured_again_when_its_bitmap_arrives() {
    let (mut host, measures) = counted(false);
    let image = host.ui.state.images[0];
    // Nothing loaded: the offered box.
    assert_eq!(host.rect(image).size(), Size::new(400.0, 100.0));
    let before = measures.get();
    deliver(&mut host, numbered);
    assert!(host.ui.needs_frame());
    host.settle();
    assert_eq!(measures.get(), before + 1);
    // Aspect None: the 8 x 8 bitmap as it is.
    assert_eq!(host.rect(image).size(), Size::new(8.0, 8.0));
    assert_eq!(host.pixel(4, 4), color(1));
    assert_eq!(host.pixel(12, 4), Color::BLACK);
}

// ---------------------------------------------------------------- first draw

/// Measures its child and never places it: what a list does with a row it only measures.
struct MeasureOnly;
impl Container for MeasureOnly {}
impl Control for MeasureOnly {
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let child = cx.child(0);
        cx.measure_child(child, width, height)
    }
    fn arrange(&mut self, _cx: &mut LayoutCx) {}
    fn paint(&self, _cx: &mut PaintCx) {}
}

#[test]
fn load_source_on_first_draw_waits_until_the_control_is_placed() {
    let requests = |on_first_draw: bool, placed: bool| {
        let ui = Ui::new((), move |_: &mut ()| {
            let image = SkiaImage::new("1.png").width_request(20).height_request(20);
            let image = image.load_source_on_first_draw(on_first_draw);
            let parent: Detached = if placed {
                SkiaLayout::new().children((image,)).into()
            } else {
                Build::new(MeasureOnly).children((image,)).into()
            };
            parent
        });
        let mut host = Headless::new(ui.background(Color::BLACK), 100, 100, 1.0);
        host.settle();
        let asked = deliver(&mut host, numbered).len();
        host.settle();
        (asked, host.pixel(10, 10))
    };
    // Measured only: the default loads, `load_source_on_first_draw` does not.
    assert_eq!(requests(false, false), (1, Color::BLACK));
    assert_eq!(requests(true, false), (0, Color::BLACK));
    // Placed: both load and show.
    assert_eq!(requests(false, true), (1, color(1)));
    assert_eq!(requests(true, true), (1, color(1)));
}

#[test]
fn a_cached_bitmap_found_on_first_draw_sizes_an_auto_image_on_the_next_frame() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.images = vec![Handle::default()];
        SkiaLayout::new().fill().children((SkiaImage::new("")
            .aspect(TransformAspect::None)
            .load_source_on_first_draw(true)
            .assign(&mut app.images[0]),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 100, 100, 1.0);
    host.ui.tree.images.preload(["1.png"]);
    host.settle();
    deliver(&mut host, numbered);
    set_source(&mut host, 0, "1.png");
    host.settle();
    assert_eq!(host.rect(host.ui.state.images[0]).size(), Size::new(8.0, 8.0));
    assert_eq!(host.pixel(4, 4), color(1));
    assert!(host.ui.tree.images.take_requests().is_empty());
}
