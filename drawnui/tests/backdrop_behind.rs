//! A backdrop and the caches of its parents (drawnui-cross 6af, Nick 2026-10-25: the library never
//! changes what a parent's cache does because a backdrop sits inside it; C# f989c6c8, cf18cf29).
//! Under an UNCACHED parent the glass follows what is behind it, also through a zoom. Recorded
//! into a parent's PICTURE cache (Operations / OperationsFull, SkiaShape's default) its copy of
//! what is behind freezes in the picture, as in C#: the app sets `use_cache(CacheType::None)` on
//! such parents. Brightness 1.0001 makes the backdrop copy what is behind (Rust skips a backdrop
//! with no blur and brightness 1).

use drawnui::prelude::*;
use drawnui::testing::Headless;

struct Ids {
    behind: Handle<SkiaShape>,
    frame: Handle<SkiaLayout>,
    glass: Handle<SkiaBackdrop>,
}

fn scene(cache: CacheType) -> Headless<Ids> {
    let ui = Ui::new(Ids { behind: Handle::default(), frame: Handle::default(), glass: Handle::default() }, move |s| {
        SkiaLayer::new().fill().children((
            SkiaShape::new().fill().background_color(Color::RED).assign(&mut s.behind),
            SkiaLayer::new().width_request(200).height_request(100).center().assign(&mut s.frame).children(
                SkiaShape::new()
                    .use_cache(cache)
                    .fill()
                    .children(SkiaBackdrop::new().fill().blur(0.0).brightness(1.0001).assign(&mut s.glass)),
            ),
        ))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    host
}

fn copies(host: &Headless<Ids>) -> u32 {
    host.ui.tree.find::<SkiaBackdrop>(host.ui.state.glass).unwrap().copies()
}

fn red(c: Color) -> bool {
    c.r() > 200 && c.b() < 50
}

fn blue(c: Color) -> bool {
    c.b() > 200 && c.r() < 50
}

#[test]
fn under_an_uncached_parent_the_glass_follows_what_is_behind() {
    let mut host = scene(CacheType::None);
    let before = copies(&host);
    assert!(before >= 1, "the glass copied what is behind");
    let behind = host.ui.state.behind;
    host.ui.tree.get_mut(behind).unwrap().set_background_color(Color::BLUE);
    host.settle();
    assert!(copies(&host) > before, "the glass did not copy again");
    assert!(blue(host.pixel(200, 100)), "the glass shows a frozen copy: {:?}", host.pixel(200, 100));
}

/// The popup case: the frame zooms in (0.75 -> 1) while what is behind changes; under an uncached
/// parent the glass shows what is behind it now, at its place.
#[test]
fn under_an_uncached_parent_the_glass_follows_through_a_zoom() {
    let mut host = scene(CacheType::None);
    let (behind, frame) = (host.ui.state.behind, host.ui.state.frame);
    host.ui.tree.get_mut(frame).unwrap().set_scale(0.75);
    host.settle();
    host.ui.tree.get_mut(behind).unwrap().set_background_color(Color::BLUE);
    host.ui.tree.get_mut(frame).unwrap().set_scale(1.0);
    host.settle();
    for (x, y) in [(110, 60), (200, 100), (290, 140)] {
        assert!(blue(host.pixel(x, y)), "({x}, {y}) shows {:?} after the zoom", host.pixel(x, y));
    }
}

/// Recorded into a parent's picture, the copy is frozen there: the parent's cache is not recorded
/// again because of the backdrop (what is behind changed, the glass still shows the old red).
#[test]
fn in_a_picture_cached_parent_the_copy_freezes_as_documented() {
    for cache in [CacheType::Operations, CacheType::OperationsFull] {
        let mut host = scene(cache);
        let before = copies(&host);
        assert!(before >= 1, "{cache:?}: the glass copied what is behind when recorded");
        assert!(red(host.pixel(200, 100)), "{cache:?}: {:?}", host.pixel(200, 100));
        let behind = host.ui.state.behind;
        host.ui.tree.get_mut(behind).unwrap().set_background_color(Color::BLUE);
        host.settle();
        assert_eq!(copies(&host), before, "{cache:?}: the parent's picture was recorded again because of the backdrop");
        assert!(red(host.pixel(200, 100)), "{cache:?}: expected the frozen copy, got {:?}", host.pixel(200, 100));
    }
}
