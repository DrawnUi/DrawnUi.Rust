//! SkiaBackdrop on the CPU canvas: blur and brightness of what is under it, the tint, its
//! children, inside Image and Operations caches, no allocation per frame.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn half(color: Color, left: f32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(50).height_request(60).margin(Thickness::new(left, 0.0, 0.0, 0.0)).background_color(color)
}

/// A 100 x 60 box at (40, 20): red left half, blue right half, the backdrop over both.
fn scene(backdrop: Build<SkiaBackdrop>, cache: CacheType) -> Headless<()> {
    let content = SkiaLayout::new()
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .use_cache(cache)
        .children((half(Color::RED, 0.0), half(Color::BLUE, 50.0), backdrop));
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b()) && d(a.a(), b.a())
}

#[test]
fn the_blur_mixes_what_is_under_it() {
    let mut host = scene(SkiaBackdrop::new().blur(5), CacheType::None);
    let edge = host.pixel(89, 50);
    assert!(edge.r() < 200 && edge.b() > 55, "the edge between the halves is blurred: {edge:?}");
    // Far from the edge, and at the backdrop's own sides (mirrored), the color stays.
    assert!(near(host.pixel(45, 50), Color::RED, 2), "{:?}", host.pixel(45, 50));
    assert!(near(host.pixel(40, 20), Color::RED, 2), "{:?}", host.pixel(40, 20));
    assert!(near(host.pixel(139, 79), Color::BLUE, 2), "{:?}", host.pixel(139, 79));
    // Outside the box nothing changed.
    assert_eq!(host.pixel(39, 50), Color::BLACK);
    assert_eq!(host.pixel(140, 50), Color::BLACK);
}

#[test]
fn without_blur_and_brightness_only_the_tint_is_drawn() {
    let glass = SkiaBackdrop::new().blur(0).background_color(Color::from_argb(128, 255, 255, 255));
    let mut host = scene(glass, CacheType::None);
    assert!(near(host.pixel(60, 50), Color::from_rgb(255, 127, 127), 1), "{:?}", host.pixel(60, 50));
    assert_eq!(host.pixel(89, 50), host.pixel(60, 50));
}

#[test]
fn brightness_is_the_upstream_gamma() {
    // 1.5 is moved to 0.5: channel 255 * (c / 255)^0.5. Red and blue channels stay 0 or 255.
    let mut host = scene(SkiaBackdrop::new().blur(0).brightness(1.5).background_color(Color::from_rgb(128, 128, 128)), CacheType::None);
    assert!(near(host.pixel(60, 50), Color::from_rgb(180, 180, 180), 1), "{:?}", host.pixel(60, 50));
}

#[test]
fn inside_an_image_cache_it_blurs_the_same() {
    let mut plain = scene(SkiaBackdrop::new().blur(5), CacheType::None);
    let mut cached = scene(SkiaBackdrop::new().blur(5), CacheType::Image);
    for x in 38..142 {
        for y in [20, 50, 79] {
            assert!(near(cached.pixel(x, y), plain.pixel(x, y), 1), "({x}, {y}): {:?} vs {:?}", cached.pixel(x, y), plain.pixel(x, y));
        }
    }
}

#[test]
fn a_picture_copies_the_surface_it_lands_on() {
    // An Operations cache records a picture, which has no pixels: the copy comes from the surface
    // the picture lands on (C# and React `Context.Surface`). What the same picture drew before the
    // backdrop is not there yet: here the halves, so the copy is the black under the box.
    let mut host = scene(SkiaBackdrop::new().blur(5), CacheType::Operations);
    for x in [45, 89, 90, 135] {
        assert!(near(host.pixel(x, 50), Color::BLACK, 2), "({x}, 50): {:?}", host.pixel(x, 50));
    }
}

/// The halves, over them a layer with its own cache holding the backdrop (the HelloRust glass
/// card: a SkiaShape, Operations by default, over a photo), a white square above it. Handles to
/// the red half, the layer and the square.
fn glass_over(cache: CacheType) -> (Headless<()>, [ControlId; 3]) {
    let red = half(Color::RED, 0.0);
    let glass = SkiaLayout::new().fill().use_cache(cache).children(SkiaBackdrop::new().blur(5));
    let above = SkiaLayout::new().width_request(10).height_request(10).margin(Thickness::new(5.0, 5.0, 0.0, 0.0)).background_color(Color::WHITE);
    let ids = [red.id(), glass.id(), above.id()];
    let content = SkiaLayout::new().width_request(100).height_request(60).margin(Thickness::new(40.0, 20.0, 0.0, 0.0));
    let content = content.children((red, half(Color::BLUE, 50.0), glass, above));
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    (host, ids)
}

#[test]
fn in_its_own_picture_it_blurs_what_is_under_it() {
    let (mut plain, _) = glass_over(CacheType::None);
    for cache in [CacheType::Operations, CacheType::OperationsFull] {
        let (mut cached, _) = glass_over(cache);
        for x in 38..142 {
            for y in [20, 50, 79] {
                assert!(near(cached.pixel(x, y), plain.pixel(x, y), 1), "{cache:?} ({x}, {y}): {:?} vs {:?}", cached.pixel(x, y), plain.pixel(x, y));
            }
        }
    }
}

#[test]
fn its_picture_is_recorded_again_when_what_is_under_it_changes() {
    let (mut host, [red, glass, above]) = glass_over(CacheType::Operations);
    let first = host.cache_records(glass);
    // Something above it moves: nothing under it changed, the picture stays.
    for i in 1..=3 {
        host.ui.tree.any_mut(above).unwrap().set_translation_x(i as f32);
        host.frame_after(16.0);
    }
    assert_eq!(host.cache_records(glass), first);
    host.ui.tree.any_mut(red).unwrap().set_background_color(Color::YELLOW);
    host.settle();
    assert_eq!(host.cache_records(glass), first + 1);
    assert!(near(host.pixel(45, 50), Color::YELLOW, 2), "{:?}", host.pixel(45, 50));
}

#[test]
fn its_children_are_blurred_with_the_rest() {
    let dot = SkiaLayout::new().width_request(10).height_request(10).margin(Thickness::new(20.0, 25.0, 0.0, 0.0));
    let glass = SkiaBackdrop::new().blur(3).children(dot.background_color(Color::WHITE));
    let mut host = scene(glass, CacheType::None);
    // The dot at (60..70, 45..55): its inside is lighter than red, its edge softer than white.
    let center = host.pixel(65, 50);
    assert!(center.g() > 100 && center.g() < 255, "{center:?}");
    assert!(host.pixel(58, 50).g() > 0, "the white spreads out: {:?}", host.pixel(58, 50));
}

#[test]
fn a_frame_allocates_nothing() {
    let mut glass = Handle::<SkiaBackdrop>::default();
    let mut host = scene(SkiaBackdrop::new().blur(5).assign(&mut glass), CacheType::None);
    let mut shift = 0.0;
    let mut frame = |host: &mut Headless<()>| {
        shift += 1.0;
        host.ui.tree.get_mut(glass).unwrap().set_translation_x(shift);
        host.frame_after(16.0);
    };
    frame(&mut host);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..10 {
        frame(&mut host);
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
}

/// Red and blue halves, the backdrop, a white square above it; handles to each.
fn layered() -> (Headless<()>, [ControlId; 3], Handle<SkiaBackdrop>, ControlId) {
    let (red, blue) = (half(Color::RED, 0.0), half(Color::BLUE, 50.0));
    let above = SkiaLayout::new().width_request(10).height_request(10).margin(Thickness::new(5.0, 5.0, 0.0, 0.0)).background_color(Color::WHITE);
    let mut glass = Handle::<SkiaBackdrop>::default();
    let ids = [red.id(), blue.id(), above.id()];
    let content = SkiaLayout::new().width_request(100).height_request(60).margin(Thickness::new(40.0, 20.0, 0.0, 0.0));
    let content = content.children((red, blue, SkiaBackdrop::new().blur(5).assign(&mut glass), above));
    let parent = content.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    (host, ids, glass, parent)
}

fn copies(host: &mut Headless<()>, glass: Handle<SkiaBackdrop>) -> u32 {
    host.ui.tree.get_mut(glass).unwrap().copies()
}

#[test]
fn the_blur_is_kept_while_nothing_under_it_changes() {
    let (mut host, [red, blue, above], glass, parent) = layered();
    assert_eq!(copies(&mut host, glass), 1);
    let edge = host.pixel(89, 50);

    // Something above it moves, its parent fades: nothing under it changed.
    for i in 1..=10 {
        host.ui.tree.any_mut(above).unwrap().set_translation_x(i as f32);
        host.ui.tree.any_mut(parent).unwrap().set_opacity(1.0 - i as f32 / 100.0);
        host.frame_after(16.0);
    }
    host.ui.tree.any_mut(parent).unwrap().set_opacity(1.0);
    host.settle();
    assert_eq!(copies(&mut host, glass), 1, "drawn again from the kept copy");
    assert_eq!(host.pixel(89, 50), edge);
    assert_eq!(host.pixel(55, 30), Color::WHITE, "the square above it is drawn where it went");

    // Under it: a new color, a move, its own tint, a move of the backdrop.
    host.ui.tree.any_mut(red).unwrap().set_background_color(Color::YELLOW);
    host.settle();
    assert_eq!(copies(&mut host, glass), 2);
    assert!(host.pixel(60, 50).g() > 200, "{:?}", host.pixel(60, 50));
    host.ui.tree.any_mut(blue).unwrap().set_translation_y(10);
    host.settle();
    assert_eq!(copies(&mut host, glass), 3);
    host.ui.tree.get_mut(glass).unwrap().set_background_color(Color::from_argb(60, 0, 0, 0));
    host.settle();
    assert_eq!(copies(&mut host, glass), 4);
    host.ui.tree.get_mut(glass).unwrap().set_translation_x(3);
    host.settle();
    assert_eq!(copies(&mut host, glass), 5);
}

#[test]
fn a_kept_copy_equals_a_new_one() {
    let (mut host, [_, _, above], glass, _) = layered();
    host.ui.tree.any_mut(above).unwrap().set_translation_x(30);
    host.settle();
    assert_eq!(copies(&mut host, glass), 1);
    // The same scene with a copy made after the move: the same pixels.
    let (mut fresh, [_, _, fresh_above], fresh_glass, _) = layered();
    fresh.ui.tree.any_mut(fresh_above).unwrap().set_translation_x(30);
    fresh.ui.tree.get_mut(fresh_glass).unwrap().set_blur(5.0001);
    fresh.settle();
    fresh.ui.tree.get_mut(fresh_glass).unwrap().set_blur(5);
    fresh.settle();
    assert_eq!(copies(&mut fresh, fresh_glass), 3);
    for x in 38..142 {
        for y in [22, 50, 78] {
            assert_eq!(host.pixel(x, y), fresh.pixel(x, y), "({x}, {y})");
        }
    }
}

#[test]
fn an_effect_under_it_is_copied_every_frame() {
    let code = "uniform float iTime; half4 main(float2 p) { return half4(fract(iTime), 0, 0, 1); }";
    let red = half(Color::RED, 0.0).visual_effect(SkiaShaderEffect::new().shader_code(code).use_background(UseBackground::Never));
    let id = red.id();
    let mut glass = Handle::<SkiaBackdrop>::default();
    let content = SkiaLayout::new().width_request(100).height_request(60).margin(Thickness::new(40.0, 20.0, 0.0, 0.0));
    let content = content.children((red, SkiaBackdrop::new().blur(5).assign(&mut glass)));
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    let animation = host.ui.tree.cx().animate_shaders(id);
    let before = copies(&mut host, glass);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert_eq!(copies(&mut host, glass) - before, 5);
    host.ui.tree.cx().stop_animation(animation);
}
