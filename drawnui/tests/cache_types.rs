//! DrawnUI's cache type names: GPU and ImageCompositeGPU are the GPU caches Image and
//! ImageComposite already are; OperationsFull records the whole area the canvas shows.

use drawnui::prelude::*;
use drawnui::testing::Headless;

fn host(content: Build<SkiaLayout>) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

fn square(color: Color, left: f32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(40).height_request(40).margin(Thickness::new(left, 10.0, 0.0, 0.0)).background_color(color)
}

/// Three squares in a 160 x 60 layout cached as `cache`.
fn scene(cache: CacheType) -> (Headless<()>, ControlId) {
    let parent = SkiaLayout::new()
        .width_request(160)
        .height_request(60)
        .background_color(Color::from_rgb(240, 240, 240))
        .use_cache(cache)
        .children((square(Color::RED, 10.0), square(Color::GREEN, 60.0), square(Color::BLUE, 110.0)));
    let id = parent.id();
    (host(parent), id)
}

#[test]
fn gpu_names_draw_and_cache_as_image_and_composite() {
    for (named, same) in [(CacheType::GPU, CacheType::Image), (CacheType::ImageCompositeGPU, CacheType::ImageComposite)] {
        let ((mut a, id), (mut b, _)) = (scene(named), scene(same));
        a.frame_after(16.0);
        assert_eq!(a.cache_records(id), 1, "{named:?} recorded again");
        for y in (0..100).step_by(3) {
            for x in 0..200 {
                assert_eq!(a.pixel(x, y), b.pixel(x, y), "{named:?} vs {same:?} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn operations_full_records_what_the_control_paints_outside_its_rect() {
    for (cache, outside) in [(CacheType::Operations, false), (CacheType::OperationsFull, true)] {
        // A 60 x 60 layout whose child is moved to x 100..140, outside the layout's rect.
        let parent = SkiaLayout::new().width_request(60).height_request(60).use_cache(cache).children(square(Color::BLUE, 0.0).translation_x(100.0));
        let id = parent.id();
        let mut host = host(parent);
        assert_eq!(host.pixel(120, 30) == Color::BLUE, outside, "{cache:?}");
        host.frame_after(16.0);
        assert_eq!(host.cache_records(id), 1, "{cache:?} recorded again");
    }
}

#[test]
fn operations_full_records_again_when_the_area_the_canvas_shows_changes_size() {
    let parent = SkiaLayout::new()
        .width_request(60)
        .height_request(60)
        .use_cache(CacheType::OperationsFull)
        .children(square(Color::BLUE, 0.0).translation_x(100.0));
    let id = parent.id();
    let mut host = host(parent);
    host.resize(240, 100);
    host.settle();
    assert_eq!(host.cache_records(id), 2);
    assert_eq!(host.pixel(120, 30), Color::BLUE);
}
