//! SkiaShaderEffect and friends on the CPU canvas: uniforms, the input texture from an Image
//! cache, a snapshot or another control, file textures, UseBackground, `iTime` frames, caches,
//! allocations, compile errors, the gl-transitions template, custom effects and their gestures.
//! The GPU look is not seen here; SkSL runs on the CPU raster the same way.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::prelude::*;
use drawnui::skia::{ClipOp, EncodedImageFormat, Paint as SkPaint, surfaces};
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

const BLIT: &str = include_str!("shaders/blit.sksl");
const RIPPLES: &str = include_str!("shaders/ripples.sksl");

/// The React ShadersPage generative shader.
const PLASMA: &str = "
uniform float2 iResolution;
uniform float2 iOffset;
uniform float  iTime;
uniform float4 iMouse;

half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution.xy;
    float t = iTime * 0.6;
    float v = sin(uv.x * 6.0 + t) + sin((uv.y * 6.0 + t) * 0.8) + sin((uv.x + uv.y) * 4.0 - t) + sin(length(uv - 0.5) * 12.0 - t * 1.5);
    v *= 0.25;
    float3 col = 0.5 + 0.5 * cos(6.2831 * (v + float3(0.0, 0.33, 0.67)) + t);
    return half4(col, 1.0);
}";

/// Red from `iTime`, green from the input texture.
const TIMED: &str = "
uniform shader iImage1;
uniform float2 iOffset;
uniform float iTime;
half4 main(float2 p) {
    half4 c = iImage1.eval(p - iOffset);
    return half4(fract(iTime), c.g, 0, 1);
}";

/// A 100 x 60 block of one color at (40, 20).
fn block(color: Color) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(100).height_request(60).margin(Thickness::new(40.0, 20.0, 0.0, 0.0)).background_color(color)
}

/// Left half red, right half blue, at (40, 20).
fn halves() -> Build<SkiaLayout> {
    let half = |color: Color, left: f32| {
        SkiaLayout::new().width_request(50).height_request(60).margin(Thickness::new(left, 0.0, 0.0, 0.0)).background_color(color)
    };
    SkiaLayout::new()
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .children((half(Color::RED, 0.0), half(Color::BLUE, 50.0)))
}

fn host<T: Control>(content: Build<T>) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b()) && d(a.a(), b.a())
}

/// A PNG of one color.
fn png(color: Color, w: i32, h: i32) -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul((w, h)).expect("surface");
    surface.canvas().clear(color);
    let image = surface.image_snapshot();
    image.encode(None, EncodedImageFormat::PNG, None).expect("png").as_bytes().to_vec()
}

#[test]
fn standard_uniforms_are_written_where_declared() {
    let code = "
uniform float2 iResolution;
uniform float2 iOffset;
uniform float  iTime;
uniform float4 iMouse;
uniform float  strength;
half4 main(float2 p) { return half4(iResolution.x / 400.0, iOffset.x / 400.0, strength + (iTime + iMouse.x) * 0.0, 1); }";
    let effect = SkiaShaderEffect::new().shader_code(code).use_background(UseBackground::Never).uniform("strength", &[0.5]);
    let mut id: Handle<SkiaLayout> = Handle::default();
    let mut host = host(block(Color::BLACK).visual_effect(effect).assign(&mut id));
    // iResolution 100 / 400, iOffset 40 / 400, strength 0.5; iImageResolution is not declared.
    assert!(near(host.pixel(60, 50), Color::from_rgb(64, 26, 128), 1), "{:?}", host.pixel(60, 50));
    let control = host.ui.tree.get_mut(id).unwrap();
    let declared = control.effect::<SkiaShaderEffect>().unwrap().declared_uniforms();
    let names: Vec<(&str, usize)> = declared.iter().map(|(n, f)| (n.as_str(), *f)).collect();
    assert_eq!(names, [("iResolution", 2), ("iOffset", 2), ("iTime", 1), ("iMouse", 4), ("strength", 1)]);
}

#[test]
fn array_uniforms_take_every_element() {
    let code = "
uniform float2 origins[10];
uniform float progresses[10];
half4 main(float2 p) { return half4(origins[3].y / 100.0, progresses[9], 0, 1); }";
    let mut origins = [0.0f32; 20];
    origins[7] = 50.0;
    let mut progresses = [0.0f32; 10];
    progresses[9] = 1.0;
    let effect = SkiaShaderEffect::new()
        .shader_code(code)
        .use_background(UseBackground::Never)
        .uniform("origins", &origins)
        .uniform("progresses", &progresses)
        .uniform("undeclared", &[1.0]);
    let mut host = host(block(Color::BLACK).visual_effect(effect));
    assert!(near(host.pixel(60, 50), Color::from_rgb(128, 255, 0), 1), "{:?}", host.pixel(60, 50));
}

#[test]
fn output_only_shader_draws_the_plasma_of_react() {
    let mut effect = SkiaShaderEffect::new().shader_code(PLASMA).use_background(UseBackground::Never).auto_create_input_texture(false);
    effect.time_seconds = 1.5;
    let mut host = host(block(Color::BLACK).visual_effect(effect));
    // The same function, per pixel center.
    let plasma = |x: f32, y: f32| {
        let (u, v) = ((x + 0.5 - 40.0) / 100.0, (y + 0.5 - 20.0) / 60.0);
        let t = 1.5f32 * 0.6;
        let length = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
        let s = ((u * 6.0 + t).sin() + ((v * 6.0 + t) * 0.8).sin() + ((u + v) * 4.0 - t).sin() + (length * 12.0 - t * 1.5).sin()) * 0.25;
        let channel = |shift: f32| ((0.5 + 0.5 * (6.2831 * (s + shift) + t).cos()) * 255.0).round() as u8;
        Color::from_rgb(channel(0.0), channel(0.33), channel(0.67))
    };
    for (x, y) in [(40, 20), (75, 35), (139, 79), (100, 60)] {
        assert!(near(host.pixel(x, y), plasma(x as f32, y as f32), 2), "({x}, {y}): {:?} vs {:?}", host.pixel(x, y), plasma(x as f32, y as f32));
    }
}

#[test]
fn pass_through_equals_the_plain_control_with_every_cache() {
    for cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
        let mut plain = host(halves().use_cache(cache));
        let effect = SkiaShaderEffect::new().shader_code(BLIT);
        let mut shaded = host(halves().use_cache(cache).visual_effect(effect));
        for x in 30..150 {
            for y in [19, 20, 50, 79, 80] {
                assert_eq!(shaded.pixel(x, y), plain.pixel(x, y), "{cache:?} ({x}, {y})");
            }
        }
        assert_eq!(shaded.pixel(60, 50), Color::RED);
        assert_eq!(shaded.pixel(120, 50), Color::BLUE);
    }
}

#[test]
fn the_image_cache_is_not_blitted_under_the_effect() {
    // A shader that ignores its texture: what the control painted must not show.
    let code = "uniform shader iImage1; half4 main(float2 p) { return half4(0, 1, 0, 1); }";
    let mut host = host(block(Color::RED).use_cache(CacheType::Image).visual_effect(SkiaShaderEffect::new().shader_code(code)));
    assert_eq!(host.pixel(60, 50), Color::GREEN);
}

#[test]
fn itime_frames_come_only_while_animated_and_the_cache_stays() {
    let mut id: Handle<SkiaLayout> = Handle::default();
    let effect = SkiaShaderEffect::new().shader_code(TIMED);
    let mut host = host(block(Color::GREEN).use_cache(CacheType::Image).visual_effect(effect).assign(&mut id));
    // Idle: nothing asks for a frame.
    assert!(!host.ui.needs_frame() && host.ui.wake_at().is_none());
    assert_eq!(host.pixel(60, 50), Color::from_rgb(0, 255, 0));

    let animation = host.ui.tree.cx().animate_shaders(id);
    // iTime is the frame time in seconds.
    let red = |host: &Headless<()>| Color::from_rgb(((host.time_ms() / 1000.0).fract() * 255.0).round() as u8, 255, 0);
    host.frame_after(250.0);
    assert!(near(host.pixel(60, 50), red(&host), 1), "{:?} vs {:?}", host.pixel(60, 50), red(&host));
    host.frame_after(250.0);
    assert!(near(host.pixel(60, 50), red(&host), 1), "{:?} vs {:?}", host.pixel(60, 50), red(&host));
    assert!(host.ui.needs_frame(), "an animated shader keeps frames coming");
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    assert_eq!(host.cache_records(id), 1, "the control's own cache is not recorded again for iTime");

    host.ui.tree.cx().stop_animation(animation);
    host.settle();
    assert!(!host.ui.needs_frame() && host.ui.wake_at().is_none());
}

#[test]
fn a_static_effect_allocates_nothing_per_frame() {
    // Over an Image cache nothing changes between frames: the shader of the last frame draws
    // again. Without one the texture is a new snapshot every frame, so the shader is new too:
    // one allocation (skia-safe's list of children in `make_shader`).
    for (cache, per_frame) in [(CacheType::Image, 0), (CacheType::None, 1)] {
        let mut id: Handle<SkiaLayout> = Handle::default();
        let effect = SkiaShaderEffect::new().shader_code(BLIT);
        let mut host = host(halves().use_cache(cache).visual_effect(effect).assign(&mut id));
        let mut x = 0.0;
        let mut frame = |host: &mut Headless<()>| {
            x += 1.0;
            host.ui.tree.any_mut(id).unwrap().set_translation_x(x);
            host.frame_after(16.0);
        };
        frame(&mut host);
        let before = ALLOCATIONS.with(|a| a.get());
        for _ in 0..10 {
            frame(&mut host);
        }
        assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 10 * per_frame, "{cache:?}");
        assert_eq!(host.pixel(60 + 11, 50), Color::RED, "{cache:?}");
    }
}

#[test]
fn once_keeps_the_first_texture_until_released() {
    let mut id: Handle<SkiaLayout> = Handle::default();
    let effect = SkiaShaderEffect::new().shader_code(BLIT).use_background(UseBackground::Once);
    let mut host = host(block(Color::RED).use_cache(CacheType::Image).visual_effect(effect).assign(&mut id));
    assert_eq!(host.pixel(60, 50), Color::RED);
    host.ui.tree.get_mut(id).unwrap().set_background_color(Color::BLUE);
    host.settle();
    assert_eq!(host.cache_records(id), 2, "the cache was recorded again");
    assert_eq!(host.pixel(60, 50), Color::RED, "the frozen texture is drawn");
    let mut control = host.ui.tree.get_mut(id).unwrap();
    let effect = control.effect_mut::<SkiaShaderEffect>().unwrap();
    assert!(effect.aquired_background());
    effect.release_frozen_snapshot();
    host.settle();
    assert_eq!(host.pixel(60, 50), Color::BLUE);
}

#[test]
fn a_shader_that_does_not_compile_leaves_the_control_plain() {
    let mut id: Handle<SkiaLayout> = Handle::default();
    let effect = SkiaShaderEffect::new().shader_code("half4 main(float2 p) { return nope; }");
    let mut host = host(block(Color::RED).use_cache(CacheType::Image).visual_effect(effect).assign(&mut id));
    assert_eq!(host.pixel(60, 50), Color::RED);
    let control = host.ui.tree.get_mut(id).unwrap();
    let effect = control.effect::<SkiaShaderEffect>().unwrap();
    assert!(!effect.is_compiled());
    assert!(effect.error().is_some_and(|e| e.contains("nope")), "{:?}", effect.error());
}

#[test]
fn a_compilation_error_reaches_its_handler_once() {
    let effect = SkiaShaderEffect::new()
        .shader_code("half4 main(float2 p) { return nope; }")
        .on_compilation_error(|me: &mut Mut<'_, dyn Control>, errors: &mut Vec<String>, _cx: &mut Cx<'_>, error: &str| {
            errors.push(error.to_owned());
            // `me` is the control the effect is on.
            me.set_background_color(Color::BLUE);
        });
    let mut id: Handle<SkiaLayout> = Handle::default();
    let ui = Ui::new(Vec::<String>::new(), |_| SkiaLayout::new().fill().children(block(Color::RED).visual_effect(effect).assign(&mut id)));
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    assert_eq!(host.ui.state.len(), 1, "{:?}", host.ui.state);
    assert!(host.ui.state[0].contains("nope"));
    assert_eq!(host.pixel(60, 50), Color::BLUE, "the handler changed its control");
    // Frames later, the same error is not reported again; a new one is.
    host.ui.tree.get_mut(id).unwrap().set_translation_x(1);
    host.settle();
    assert_eq!(host.ui.state.len(), 1);
    host.ui.tree.get_mut(id).unwrap().effect_mut::<SkiaShaderEffect>().unwrap().set_shader_code("half4 main(float2 p) { return oops; }");
    host.settle();
    assert_eq!(host.ui.state.len(), 2);
    assert!(host.ui.state[1].contains("oops"));
}

#[test]
fn a_transition_blends_the_caches_of_two_controls() {
    let (mut from, mut to, mut id): (Handle<SkiaLayout>, Handle<SkiaLayout>, Handle<SkiaLayout>) = Default::default();
    let slide = |color: Color| SkiaLayout::new().fill().background_color(color).use_cache(CacheType::Image);
    let mut effect = SkiaShaderEffect::transition().shader_code(include_str!("shaders/transitions/fade.sksl"));
    effect.progress = 0.25;
    // Controls are ids only once built: the effect is attached after mount.
    let layout = SkiaLayout::new()
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .use_cache(CacheType::Image)
        .children((slide(Color::RED).assign(&mut from), slide(Color::BLUE).assign(&mut to)))
        .assign(&mut id);
    let mut host = host(layout);
    effect.control_from = Some(from.id());
    effect.control_to = Some(to.id());
    host.ui.tree.get_mut(id).unwrap().add_visual_effect(effect);
    host.settle();
    assert!(near(host.pixel(60, 50), Color::from_rgb(191, 0, 64), 1), "{:?}", host.pixel(60, 50));
    host.ui.tree.get_mut(id).unwrap().effect_mut::<SkiaShaderEffect>().unwrap().progress = 1.0;
    host.settle();
    assert_eq!(host.pixel(60, 50), Color::BLUE);
}

#[test]
fn every_gl_transition_compiles_with_the_template() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/transitions");
    let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    names.sort();
    assert_eq!(names.len(), 49);
    for path in names {
        let code = std::fs::read_to_string(&path).unwrap();
        let mut id: Handle<SkiaLayout> = Handle::default();
        let effect = SkiaShaderEffect::transition().shader_code(code);
        let mut host = host(block(Color::RED).use_cache(CacheType::Image).visual_effect(effect).assign(&mut id));
        let control = host.ui.tree.get_mut(id).unwrap();
        let effect = control.effect::<SkiaShaderEffect>().unwrap();
        assert!(effect.is_compiled(), "{}: {:?}", path.display(), effect.error());
    }
}

#[test]
fn a_file_texture_arrives_through_the_image_manager() {
    let code = "
uniform shader iImage1;
uniform shader iImage2;
uniform float2 iOffset;
half4 main(float2 p) { return iImage2.eval(p - iOffset); }";
    let effect = SkiaShaderEffect::new().shader_code(code).secondary_source("nebula.png");
    let mut host = host(block(Color::RED).use_cache(CacheType::Image).visual_effect(effect));
    // Not loaded yet: iImage2 samples transparent over the black background.
    assert_eq!(host.pixel(60, 50), Color::BLACK);
    let asked = host.deliver_images(|source| (source == "nebula.png").then(|| png(Color::YELLOW, 8, 8)));
    assert_eq!(asked.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), ["nebula.png"]);
    host.settle();
    assert_eq!(host.pixel(60, 50), Color::YELLOW);
    // Asked once.
    host.frame_after(16.0);
    assert!(host.deliver_images(|_| None).is_empty());
}

#[test]
fn a_custom_effect_paints_outside_by_its_margin() {
    /// A 10 pixel frame around the control, reported as its margin.
    struct Halo;
    impl SkiaEffect for Halo {
        fn effect_margin(&self, scale: f32) -> Thickness {
            Thickness::uniform(10.0 * scale)
        }
        fn is_post_renderer(&self) -> bool {
            true
        }
        fn render(&self, cx: &mut PaintCx<'_>, _cached: Option<&CachedTexture>) -> bool {
            let mut paint = SkPaint::default();
            paint.set_color(Color::GREEN);
            cx.canvas.save();
            cx.canvas.clip_rect(cx.rect, ClipOp::Difference, false);
            cx.canvas.draw_rect(cx.rect.with_outset((10.0, 10.0)), &paint);
            cx.canvas.restore();
            // Not in the place of the content: an Image cache is blitted still.
            false
        }
    }
    for cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
        // The parent's cache keeps what the effect paints outside its child.
        let parent = SkiaLayout::new().fill().use_cache(cache).children(block(Color::RED).visual_effect(Halo));
        let mut host = host(parent);
        assert_eq!(host.pixel(35, 50), Color::GREEN, "{cache:?}");
        assert_eq!(host.pixel(60, 50), Color::RED, "{cache:?}");
        assert_eq!(host.pixel(25, 50), Color::BLACK, "{cache:?}");
    }
}

#[test]
fn an_effect_sees_gestures_before_the_control() {
    /// Takes every tap.
    struct TapEater(u32);
    impl SkiaEffect for TapEater {
        fn on_gesture(&mut self, _cx: &mut GestureCx<'_>, gesture: &Gesture) -> Handled {
            if gesture.kind == GestureKind::Tapped {
                self.0 += 1;
                return Handled::Yes;
            }
            Handled::No
        }
    }
    let mut id: Handle<SkiaLayout> = Handle::default();
    let ui = Ui::new(0u32, |_| {
        SkiaLayout::new().fill().children(
            block(Color::RED).visual_effect(TapEater(0)).on_tapped(|_, taps: &mut u32, _| *taps += 1).assign(&mut id),
        )
    });
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host.tap(60.0, 50.0);
    assert_eq!(host.ui.state, 0, "the effect consumed the tap");
    assert_eq!(host.ui.tree.get_mut(id).unwrap().effect::<TapEater>().unwrap().0, 1);
}

#[test]
fn a_tap_starts_a_ripple_that_ends_by_itself() {
    register_shader_source("shaders/ripples.sksl", RIPPLES);
    let mut id: Handle<SkiaLayout> = Handle::default();
    let effect = MultiRippleWithTouchEffect::new();
    let mut host = host(halves().use_cache(CacheType::Image).visual_effect(effect).assign(&mut id));
    let plain = host.pixel(70, 40);
    host.ui.pointer(drawnui::PointerKind::Down, 70.0, 40.0, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(drawnui::PointerKind::Up, 70.0, 40.0, host.time_ms());
    host.frame_after(400.0);
    {
        let control = host.ui.tree.get_mut(id).unwrap();
        let ripple = control.effect::<MultiRippleWithTouchEffect>().unwrap();
        assert!(ripple.shader.is_compiled(), "{:?}", ripple.shader.error());
        assert_eq!(ripple.ripples.len(), 1);
        assert_eq!(ripple.ripples[0].origin, Point::new(30.0, 20.0));
        assert!(ripple.ripples[0].progress > 0.0);
    }
    assert!(host.ui.needs_frame(), "the ripple animates");
    host.settle();
    let control = host.ui.tree.get_mut(id).unwrap();
    assert!(control.effect::<MultiRippleWithTouchEffect>().unwrap().ripples.is_empty());
    assert!(!host.ui.needs_frame());
    // Without ripples the shader mixes the content 22.5 % with the reflection, here none.
    assert!(near(plain, Color::from_rgb(198, 0, 0), 1), "{plain:?}");
    assert_eq!(host.pixel(70, 40), plain);
}
