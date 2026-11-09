//! An ImageComposite parent draws no stale pixels where a child was (DrawnUi.Net 5e552b2e,
//! CompositeRemovedChildTests): a child removed and a new one added elsewhere in the same frame, a
//! child moved to a new place (a pooled sprite), every child cleared, a child moved to another
//! parent; each, and the next frames, show only what is there now.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    field: Handle<SkiaLayout>,
    other: Handle<SkiaLayout>,
    a: Handle<SkiaShape>,
    b: Handle<SkiaShape>,
    step: u32,
}

const BG: Color = Color::BLACK;

fn brick(x: f32, y: f32, color: Color) -> Build<SkiaShape> {
    SkiaShape::new().width_request(40).height_request(20).left(x).top(y).background_color(color)
}

/// A 300 x 150 field cached as ImageComposite with two bricks, a second plain layout below it, and a
/// button that runs step `app.step`.
fn host(cache: CacheType) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaLayout::new().fill().children((
            SkiaLayout::new()
                .width_request(300)
                .height_request(150)
                .use_cache(cache)
                .assign(&mut app.field)
                .children((brick(10.0, 10.0, Color::RED).assign(&mut app.a), brick(100.0, 10.0, Color::GREEN).assign(&mut app.b))),
            SkiaLayout::new().width_request(300).height_request(100).top(160).assign(&mut app.other),
            SkiaShape::new().width_request(40).height_request(40).left(320).top(10).background_color(Color::BLUE).on_tapped(|_me, app: &mut App, cx| {
                match app.step {
                    // Removed, and a new brick added elsewhere in the same frame.
                    0 => {
                        cx.remove(app.a);
                        cx.add_child(app.field, brick(200.0, 100.0, Color::YELLOW));
                    }
                    // The same brick moved to a new place (a pooled sprite reused).
                    1 => {
                        if let Some(mut b) = cx.get_mut(app.b) {
                            b.set_left(10.0);
                            b.set_top(100.0);
                        }
                    }
                    // Every child cleared.
                    _ => {
                        for child in cx.app_children(app.field) {
                            cx.remove(child);
                        }
                    }
                }
                app.step += 1;
            }),
        ))
    })
    .background(BG);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    host
}

fn at(host: &mut Headless<App>, x: i32, y: i32) -> Color {
    host.pixel(x, y)
}

fn check(cache: CacheType) {
    let mut host = host(cache);
    assert_eq!(at(&mut host, 30, 20), Color::RED);
    host.tap(340.0, 30.0);
    host.frame_after(16.0);
    assert_eq!(at(&mut host, 30, 20), BG, "{cache:?}: the removed brick left no pixels");
    assert_eq!(at(&mut host, 220, 110), Color::YELLOW, "{cache:?}: the new brick shows");
    assert_eq!(at(&mut host, 120, 20), Color::GREEN, "{cache:?}: its sibling stays");

    host.tap(340.0, 30.0);
    host.frame_after(16.0);
    assert_eq!(at(&mut host, 120, 20), BG, "{cache:?}: the moved brick left no pixels where it was");
    assert_eq!(at(&mut host, 30, 110), Color::GREEN, "{cache:?}: it shows where it is");

    host.tap(340.0, 30.0);
    host.frame_after(16.0);
    for (x, y) in [(30, 110), (220, 110), (30, 20), (120, 20)] {
        assert_eq!(at(&mut host, x, y), BG, "{cache:?}: cleared, nothing at ({x}, {y})");
    }
}

#[test]
fn image_composite_shows_no_stale_pixels_after_removals_and_moves() {
    check(CacheType::ImageComposite);
}

#[test]
fn gpu_composite_shows_no_stale_pixels_after_removals_and_moves() {
    check(CacheType::ImageCompositeGPU);
}

/// A child moved only by translation (no relayout, a sprite) inside a composite draws at its new
/// place at once and leaves nothing at the old one, also over several moves in a row.
fn check_translation(cache: CacheType) {
    #[derive(Default)]
    struct Field {
        sprite: Handle<SkiaShape>,
        x: f32,
    }
    let ui = Ui::new(Field::default(), move |app: &mut Field| {
        SkiaLayout::new().fill().children((
            SkiaLayout::new().width_request(300).height_request(150).use_cache(cache).children((
                brick(10.0, 10.0, Color::RED).assign(&mut app.sprite),
                brick(150.0, 100.0, Color::GREEN),
            )),
            SkiaShape::new().width_request(40).height_request(40).left(320).top(10).background_color(Color::BLUE).on_tapped(|_me, app: &mut Field, cx| {
                app.x += 60.0;
                let x = app.x;
                if let Some(mut s) = cx.get_mut(app.sprite) {
                    s.set_translation_x(x);
                    s.set_translation_y(x / 2.0);
                }
            }),
        ))
    })
    .background(BG);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    assert_eq!(host.pixel(30, 20), Color::RED);
    let mut was = (30, 20);
    for step in 1..=3 {
        host.tap(340.0, 30.0);
        host.frame_after(16.0);
        let d = 60 * step;
        let now = (30 + d, 20 + d / 2);
        assert_eq!(host.pixel(was.0, was.1), BG, "{cache:?} step {step}: nothing left where the sprite was");
        assert_eq!(host.pixel(now.0, now.1), Color::RED, "{cache:?} step {step}: the sprite shows at its new place");
        assert_eq!(host.pixel(170, 110), Color::GREEN, "{cache:?} step {step}: its sibling stays");
        was = now;
    }
}

#[test]
fn image_composite_draws_a_translated_child_at_its_new_place() {
    check_translation(CacheType::ImageComposite);
}

#[test]
fn gpu_composite_draws_a_translated_child_at_its_new_place() {
    check_translation(CacheType::ImageCompositeGPU);
}
