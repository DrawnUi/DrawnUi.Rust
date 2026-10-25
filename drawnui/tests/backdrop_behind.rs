//! A backdrop inside a picture-cached SkiaShape follows what is behind it (DrawnUi.Net bcce0739,
//! BackdropTransformedParentTests.Backdrop_InPictureCachedShape_FollowsWhatIsBehind; drawnui-cross 6af).
//! C# draws such a backdrop live from the frame after; Rust records the picture again when what is
//! under it changes (`RenderSlot::reads_below`). Brightness 1.0001 makes the backdrop copy what is
//! behind (Rust skips a backdrop with no blur and brightness 1).

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
                SkiaShape::new().use_cache(cache).fill().children(SkiaBackdrop::new().fill().blur(0.0).brightness(1.0001).assign(&mut s.glass)),
            ),
        ))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 200, 1.0);
    host.settle();
    host
}

fn blue(c: Color) -> bool {
    c.b() > 200 && c.r() < 50
}

#[test]
fn the_glass_in_a_picture_cached_shape_follows_what_is_behind() {
    for cache in [CacheType::Operations, CacheType::OperationsFull, CacheType::None] {
        let mut host = scene(cache);
        let (behind, glass) = (host.ui.state.behind, host.ui.state.glass);
        let copies = host.ui.tree.find::<SkiaBackdrop>(glass).unwrap().copies();
        assert!(copies >= 1, "{cache:?}: the glass copied what is behind");
        host.ui.tree.get_mut(behind).unwrap().set_background_color(Color::BLUE);
        host.settle();
        assert!(host.ui.tree.find::<SkiaBackdrop>(glass).unwrap().copies() > copies, "{cache:?}: the glass did not copy again");
        assert!(blue(host.pixel(200, 100)), "{cache:?}: the glass shows a frozen copy: {:?}", host.pixel(200, 100));
    }
}

/// The popup case: the frame zooms in (0.75 -> 1) while what is behind changes; the glass shows
/// what is behind it now, at its place.
#[test]
fn the_glass_follows_through_a_zoom() {
    for cache in [CacheType::Operations, CacheType::OperationsFull, CacheType::None] {
        let mut host = scene(cache);
        let (behind, frame) = (host.ui.state.behind, host.ui.state.frame);
        host.ui.tree.get_mut(frame).unwrap().set_scale(0.75);
        host.settle();
        host.ui.tree.get_mut(behind).unwrap().set_background_color(Color::BLUE);
        host.ui.tree.get_mut(frame).unwrap().set_scale(1.0);
        host.settle();
        for (x, y) in [(110, 60), (200, 100), (290, 140)] {
            assert!(blue(host.pixel(x, y)), "{cache:?}: ({x}, {y}) shows {:?} after the zoom", host.pixel(x, y));
        }
    }
}
