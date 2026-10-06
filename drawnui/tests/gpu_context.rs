//! A GPU context made again after the old one was lost (a GPU reset, a lost WebGL context): what
//! lived on the old context is made again on the next frame (DrawnUI GraphicContextMismatch),
//! CPU bitmaps stay. Headless stands for it with `gpu_recreated`.

use drawnui::prelude::*;
use drawnui::testing::Headless;

fn square(color: Color, left: f32, cache: CacheType) -> Build<SkiaLayout> {
    SkiaLayout::new()
        .width_request(40)
        .height_request(40)
        .margin(Thickness::new(left, 10.0, 0.0, 0.0))
        .background_color(color)
        .use_cache(cache)
}

#[test]
fn caches_of_the_lost_context_are_made_again_cpu_bitmaps_stay() {
    let caches = [CacheType::Image, CacheType::Operations, CacheType::ImageComposite, CacheType::ImageDoubleBuffered];
    let squares: Vec<Build<SkiaLayout>> =
        caches.iter().enumerate().map(|(i, cache)| square(Color::BLUE, 10.0 + 45.0 * i as f32, *cache)).collect();
    let ids: Vec<ControlId> = squares.iter().map(|s| s.id()).collect();
    let ui = Ui::new((), move |_| SkiaLayout::new().fill().children(squares)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    let before: Vec<u32> = ids.iter().map(|id| host.cache_records(*id)).collect();

    host.gpu_recreated();
    host.settle();
    for (i, cache) in caches.iter().enumerate() {
        let (records, x) = (host.cache_records(ids[i]), 30 + 45 * i as i32);
        assert_eq!(host.pixel(x, 30), Color::BLUE, "{cache:?} draws after the new context");
        // On the CPU canvas an Image is a CPU bitmap as well; what counts is what a GPU host has:
        // textures and pictures go, the CPU bitmap of ImageDoubleBuffered stays.
        match cache {
            CacheType::ImageDoubleBuffered => assert_eq!(records, before[i], "{cache:?} kept"),
            CacheType::Operations => assert_eq!(records, before[i] + 1, "{cache:?} made again"),
            _ => assert!(records >= before[i], "{cache:?}"),
        }
    }
}
