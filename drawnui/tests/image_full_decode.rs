//! Every image decodes the whole file, and every control showing the same source shares that one
//! copy (DrawnUi.Net: full decode, SkiaImageManager.ReuseBitmaps). A picture shown small first and
//! large later is sharp at once: no load in between, no copy stretched.

mod image_common;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::ImageRequest;
use drawnui::skia::Image;
use image_common::*;

#[derive(Default)]
struct App {
    thumb: Handle<SkiaImage>,
    big: Handle<SkiaImage>,
}

/// A 40 x 40 point thumbnail and a 400 x 100 point view, both empty until a test gives them a source.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::column().spacing(0).children((
            SkiaImage::new("").width_request(40).height_request(40).assign(&mut app.thumb),
            SkiaImage::new("").width_request(400).height_request(100).assign(&mut app.big),
        ))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 400, 400, 1.0);
    host.settle();
    host
}

fn bytes(source: &str) -> Option<Vec<u8>> {
    (source == "wide.png").then(|| quadrants(800, 200))
}

fn boxes(requests: &[ImageRequest]) -> Vec<(&str, u32, u32)> {
    requests.iter().map(|r| (r.source.as_str(), r.width, r.height)).collect()
}

fn shown(host: &Headless<App>, control: Handle<SkiaImage>) -> Option<Image> {
    host.ui.tree.find::<SkiaImage>(control).unwrap().image().cloned()
}

#[test]
fn a_thumbnail_decodes_the_whole_file_and_a_big_view_shares_it() {
    let mut host = host();
    let (thumb, big) = (host.ui.state.thumb, host.ui.state.big);
    host.ui.tree.get_mut(thumb).unwrap().set_source("wide.png");
    host.settle();
    // Shown at 40 x 40, still the whole file is asked for.
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();
    assert_eq!(shown(&host, thumb).map(|image| image.width()), Some(800));

    // The same picture shown big: the copy is there, sharp at once, no second load.
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
    let (a, b) = (shown(&host, thumb).unwrap(), shown(&host, big).unwrap());
    assert_eq!(a.unique_id(), b.unique_id(), "one shared copy");
    assert_eq!(host.ui.tree.images.memory_bytes(), 800 * 200 * 4);
}

#[test]
fn every_aspect_and_size_asks_for_the_whole_file_once() {
    let mut host = host();
    let (thumb, big) = (host.ui.state.thumb, host.ui.state.big);
    host.ui.tree.get_mut(thumb).unwrap().set_source("wide.png");
    host.ui.tree.get_mut(big).unwrap().set_aspect(TransformAspect::AspectFit);
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();
    // A box growing past anything asks for nothing more.
    host.ui.tree.get_mut(thumb).unwrap().set_width_request(390);
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
}

/// A cached picture older than `max_age` (30 min by default, as DrawnUi.Net CacheLongevitySecs) is
/// not served again: the next control asking loads the source anew, so a file changed on the server
/// shows.
#[test]
fn a_picture_older_than_max_age_loads_again() {
    let mut host = host();
    let (thumb, big) = (host.ui.state.thumb, host.ui.state.big);
    assert_eq!(host.ui.tree.images.max_age(), std::time::Duration::from_secs(30 * 60));
    host.ui.tree.get_mut(thumb).unwrap().set_source("wide.png");
    host.settle();
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();

    // Still young: served from the cache.
    let loaded = host.time_ms();
    host.set_time_ms(loaded + 29.0 * 60_000.0);
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert!(host.ui.tree.images.take_requests().is_empty());
    assert!(shown(&host, big).is_some());

    // Past 30 min: not served, loaded again.
    host.ui.tree.get_mut(big).unwrap().set_source("");
    host.settle();
    host.set_time_ms(loaded + 31.0 * 60_000.0);
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert!(shown(&host, big).is_none());
    assert_eq!(boxes(&host.deliver_images(bytes)), [("wide.png", 0, 0)]);
    host.settle();
    assert!(shown(&host, big).is_some());
    assert_eq!(host.ui.tree.images.memory_bytes(), 800 * 200 * 4, "the old copy went");

    // An app's own age.
    host.ui.tree.images.set_max_age(std::time::Duration::from_secs(10));
    host.ui.tree.get_mut(big).unwrap().set_source("");
    host.settle();
    host.set_time_ms(host.time_ms() + 11_000.0);
    host.ui.tree.get_mut(big).unwrap().set_source("wide.png");
    host.settle();
    assert_eq!(boxes(&host.ui.tree.images.take_requests()), [("wide.png", 0, 0)]);
}
