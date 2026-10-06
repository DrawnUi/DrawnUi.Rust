//! Shadows and the effects margin: what a control paints outside its rect survives its own cache,
//! the caches of its ancestors and their bounds clips, and never takes part in layout or hit
//! testing. Expected pixels were read from the C# engine (DrawnUi.Net, headless) with the same
//! scenes; `TOLERANCE` per channel covers the two Skia builds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::prelude::*;
use drawnui::testing::Headless;

const TOLERANCE: i32 = 3;
const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
/// A font whose glyphs reach below its descent. Without it the label test proves nothing.
const INTER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Inter-Regular.ttf");

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
static ALLOCATOR: Counting = Counting;

/// The content in an Absolute root over a white canvas of 200 x 200 points.
fn shot(scale: f32, content: impl IntoChildren) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::WHITE);
    let pixels = (200.0 * scale) as i32;
    let mut host = Headless::new(ui, pixels, pixels, scale);
    host.settle();
    host
}

/// Half-transparent black, 10 points down, blur 4: reaches 12 points left, right and up (hidden
/// under the shape there) and 22 points down.
fn shadow() -> SkiaShadow {
    SkiaShadow { x: 0.0, y: 10.0, blur: 4.0, color: Color::BLACK, opacity: 0.5, shadow_only: false }
}

/// A blue 60 x 60 shape with the shadow.
fn shadowed(cache: CacheType) -> Build<SkiaShape> {
    SkiaShape::new().width_request(60).height_request(60).background_color(Color::BLUE).use_cache(cache).shadows(shadow())
}

fn at(x: i32, y: i32, shape: Build<SkiaShape>) -> Build<SkiaShape> {
    shape.margin((x, y, 0, 0))
}

fn hex(color: Color) -> String {
    format!("{:02X}{:02X}{:02X}{:02X}", color.a(), color.r(), color.g(), color.b())
}

#[track_caller]
fn assert_pixels(what: &str, actual: Vec<Color>, expected: &str) {
    let expected: Vec<Color> = expected.split_whitespace().map(|v| Color::new(u32::from_str_radix(v, 16).unwrap())).collect();
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= TOLERANCE;
    let close = |a: &Color, b: &Color| d(a.a(), b.a()) && d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b());
    let same = actual.len() == expected.len() && actual.iter().zip(&expected).all(|(a, b)| close(a, b));
    let print = |colors: &[Color]| colors.iter().map(|c| hex(*c)).collect::<Vec<_>>().join(" ");
    assert!(same, "{what}\n  got      {}\n  upstream {}", print(&actual), print(&expected));
}

fn row<S>(host: &mut Headless<S>, y: i32, from: i32, to: i32, step: i32) -> Vec<Color> {
    (from..=to).step_by(step as usize).map(|x| host.pixel(x, y)).collect()
}

fn column<S>(host: &mut Headless<S>, x: i32, from: i32, to: i32, step: i32) -> Vec<Color> {
    (from..=to).step_by(step as usize).map(|y| host.pixel(x, y)).collect()
}

/// Every pixel of the two canvases is the same.
#[track_caller]
fn assert_same<A, B>(what: &str, host: &mut Headless<A>, reference: &mut Headless<B>, size: i32) {
    for y in 0..size {
        for x in 0..size {
            let (got, expected) = (host.pixel(x, y), reference.pixel(x, y));
            assert_eq!(got, expected, "{what}: pixel {x}, {y} is {} and not {}", hex(got), hex(expected));
        }
    }
}

// ---------------------------------------------------------------- the shadow itself

/// Below the shape (its rect ends at 130 points) the shadow fades out over 22 points; left of it
/// over 12. Same numbers as upstream at scale 1 and 2.
#[test]
fn a_shadow_is_painted_outside_the_rect() {
    let mut host = shot(1.0, at(70, 70, shadowed(CacheType::None)));
    assert_pixels(
        "scale 1, below",
        column(&mut host, 100, 128, 154, 1),
        "FF0000FF FF0000FF FF808080 FF818181 FF848484 FF878787 FF8C8C8C FF929292 FF9A9A9A FFA3A3A3 FFAEAEAE FFB9B9B9 FFC5C5C5 FFD0D0D0 FFDBDBDB FFE4E4E4 FFECECEC FFF2F2F2 FFF7F7F7 FFFAFAFA FFFDFDFD FFFEFEFE FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
    );
    assert_pixels(
        "scale 1, left",
        row(&mut host, 100, 56, 72, 1),
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFEFEFE FFFDFDFD FFFAFAFA FFF7F7F7 FFF2F2F2 FFECECEC FFE4E4E4 FFDBDBDB FFD0D0D0 FFC5C5C5 FF0000FF FF0000FF FF0000FF",
    );
    let mut host = shot(2.0, at(70, 70, shadowed(CacheType::None)));
    assert_pixels(
        "scale 2, below",
        column(&mut host, 200, 256, 308, 2),
        "FF0000FF FF0000FF FF7F7F7F FF808080 FF818181 FF848484 FF888888 FF8D8D8D FF949494 FF9E9E9E FFA9A9A9 FFB5B5B5 FFC2C2C2 FFCFCFCF FFDBDBDB FFE5E5E5 FFEEEEEE FFF4F4F4 FFF9F9F9 FFFCFCFC FFFDFDFD FFFEFEFE FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF",
    );
    assert_pixels(
        "scale 2, left",
        row(&mut host, 200, 112, 144, 2),
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFEFEFE FFFDFDFD FFFAFAFA FFF6F6F6 FFF1F1F1 FFEAEAEA FFE0E0E0 FFD5D5D5 FFC9C9C9 FF0000FF FF0000FF FF0000FF",
    );
}

/// 3 sigma of the blur around the offset shadow, per side, pixels (C# MergeShadowMargin).
#[test]
fn a_shape_reports_its_shadows_as_effects_margin() {
    let margin = |shape: Build<SkiaShape>, scale: f32| {
        let ui = Ui::new(Handle::<SkiaShape>::default(), |h| SkiaLayout::new().fill().children(shape.assign(h)));
        let host = Headless::new(ui, 100, 100, scale);
        let shape = host.ui.tree.find::<SkiaShape>(host.ui.state).unwrap();
        Control::effects_margin(shape, scale)
    };
    assert_eq!(margin(SkiaShape::new(), 1.0), Thickness::ZERO);
    assert_eq!(margin(shadowed(CacheType::None), 1.0), Thickness::new(12.0, 2.0, 12.0, 22.0));
    assert_eq!(margin(shadowed(CacheType::None), 2.0), Thickness::new(24.0, 4.0, 24.0, 44.0));
    // Several shadows: the largest reach on each side. A shadow moved right leaves nothing on the left.
    let right = SkiaShadow { x: 10.0, y: 0.0, blur: 2.0, ..shadow() };
    let left = SkiaShadow { x: -10.0, ..right };
    assert_eq!(margin(SkiaShape::new().shadows(right), 1.0), Thickness::new(0.0, 6.0, 16.0, 6.0));
    assert_eq!(margin(SkiaShape::new().shadows(vec![right, left]), 1.0), Thickness::new(16.0, 6.0, 16.0, 6.0));
}

/// Upstream rules of the shadow list: a hollow shape keeps only what is outside its outline, a
/// shadow-only shadow leaves the shape out, a shape without a background has no shadow, a color
/// with its own alpha ignores `opacity`, and every shadow draws the shape again.
#[test]
fn shadow_variants() {
    let mut host = shot(1.0, at(70, 70, shadowed(CacheType::None).clip_background_color(true)));
    assert_pixels(
        "hollow",
        column(&mut host, 100, 60, 145, 5),
        "FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF FF808080 FF929292 FFC5C5C5 FFF2F2F2",
    );
    // Without shadows `clip_background_color` does nothing, as upstream.
    let plain = SkiaShape::new().width_request(60).height_request(60).background_color(Color::BLUE).clip_background_color(true);
    let mut host = shot(1.0, at(70, 70, plain));
    assert_eq!(host.pixel(100, 100), Color::BLUE);

    let only = SkiaShadow { shadow_only: true, ..shadow() };
    let mut host = shot(1.0, at(70, 70, shadowed(CacheType::None).shadows(only)));
    assert_pixels(
        "shadow only",
        column(&mut host, 100, 60, 145, 5),
        "FFFFFFFF FFFFFFFF FFFEFEFE FFECECEC FFB9B9B9 FF8C8C8C FF7F7F7F FF7F7F7F FF7F7F7F FF7F7F7F FF7F7F7F FF7F7F7F FF7F7F7F FF7F7F7F FF808080 FF929292 FFC5C5C5 FFF2F2F2",
    );

    let mut host = shot(1.0, at(70, 70, SkiaShape::new().width_request(60).height_request(60).shadows(shadow())));
    assert!(column(&mut host, 100, 60, 160, 1).iter().all(|c| *c == Color::WHITE), "no background, so no shadow");

    let tinted = SkiaShadow { color: Color::from_argb(128, 255, 0, 0), opacity: 0.1, ..shadow() };
    let mut host = shot(1.0, at(70, 70, shadowed(CacheType::None).shadows(tinted)));
    assert_pixels(
        "a color with alpha keeps it",
        column(&mut host, 100, 128, 150, 2),
        "FF0000FF FFFF8080 FFFF8484 FFFF8C8C FFFF9A9A FFFFAEAE FFFFC5C5 FFFFDBDB FFFFECEC FFFFF7F7 FFFFFDFD FFFFFFFF",
    );

    let right = SkiaShadow { x: 10.0, y: 0.0, blur: 2.0, opacity: 1.0, ..shadow() };
    let left = SkiaShadow { x: -10.0, color: Color::RED, ..right };
    let mut host = shot(1.0, at(70, 70, shadowed(CacheType::None).shadows(vec![right, left])));
    assert_pixels(
        "two shadows, left side",
        row(&mut host, 100, 52, 70, 2),
        "FFFFFFFF FFFFFFFF FFFFF2F2 FFFFBFBF FFFF6969 FFFF2020 FFFF0303 FFFF0000 FFFF0000 FF0000FF",
    );
    assert_pixels(
        "two shadows, right side",
        row(&mut host, 100, 130, 148, 2),
        "FF000000 FF000000 FF000000 FF0D0D0D FF404040 FF969696 FFDFDFDF FFFCFCFC FFFFFFFF FFFFFFFF",
    );
}

// ---------------------------------------------------------------- caches

/// A cache is recorded for the rect grown by the margin, so a cached shape looks exactly like an
/// uncached one. Also at a scale that puts its rect between pixels.
#[test]
fn a_cached_shape_shows_the_same_pixels_as_an_uncached_one() {
    for scale in [1.0, 1.25, 2.0] {
        let size = (200.0 * scale) as i32;
        let mut live = shot(scale, at(70, 70, shadowed(CacheType::None)));
        assert_ne!(live.pixel((100.0 * scale) as i32, (140.0 * scale) as i32), Color::WHITE, "the shadow is there");
        for cache in [CacheType::Operations, CacheType::Image] {
            let mut cached = shot(scale, at(70, 70, shadowed(cache)));
            assert_same(&format!("scale {scale}, {cache:?}"), &mut cached, &mut live, size);
        }
    }
}

/// The margin of a descendant grows the caches of its ancestors, and their bounds clips when
/// they let effects out (`clip_effects(false)`, as React; C# always grows that clip).
#[test]
fn a_shadow_survives_the_caches_and_the_bounds_clips_above_it() {
    for scale in [1.0, 2.0] {
        let size = (200.0 * scale) as i32;
        let mut live = shot(scale, at(70, 70, shadowed(CacheType::None)));
        for parent_cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
            for clipped in [false, true] {
                // The parent is exactly as large as the shape: the whole shadow is outside of it.
                let parent = SkiaLayout::new()
                    .margin((70, 70, 0, 0))
                    .use_cache(parent_cache)
                    .is_clipped_to_bounds(clipped)
                    .clip_effects(false)
                    .children(shadowed(CacheType::Operations));
                let mut host = shot(scale, parent);
                assert_same(&format!("scale {scale}, parent {parent_cache:?}, clipped {clipped}"), &mut host, &mut live, size);
            }
        }
        // Two cached levels above an image-cached shape.
        let inner = SkiaLayout::new().use_cache(CacheType::Image).children(shadowed(CacheType::Image));
        let outer = SkiaLayout::new().margin((70, 70, 0, 0)).use_cache(CacheType::Image).children(inner);
        let mut host = shot(scale, outer);
        assert_same(&format!("scale {scale}, three image caches"), &mut host, &mut live, size);
    }
}

/// A bounds clip that lets effects out ends at the box plus the margin, as upstream: a child
/// moved 30 points down out of its clipped parent (box 70..130) shows down to 152 = 130 + 22 and
/// no further. With `clip_effects` (the default, as React) it ends at the box.
#[test]
fn a_bounds_clip_cuts_at_the_box_plus_the_margin() {
    let parent = |clipped: bool| {
        SkiaLayout::new()
            .margin((70, 70, 0, 0))
            .is_clipped_to_bounds(clipped)
            .clip_effects(false)
            .children(shadowed(CacheType::Operations).translation_y(30))
    };
    let mut host = shot(1.0, parent(true));
    let blue = "FF0000FF ".repeat(27);
    let white = "FFFFFFFF ".repeat(24);
    assert_pixels("clipped", column(&mut host, 100, 125, 175, 1), &format!("{blue}{white}"));
    let exact = SkiaLayout::new().margin((70, 70, 0, 0)).is_clipped_to_bounds(true);
    let mut host = shot(1.0, exact.children(shadowed(CacheType::Operations).translation_y(30)));
    assert_eq!(host.pixel(100, 129), Color::BLUE);
    assert_eq!(host.pixel(100, 130), Color::WHITE, "clip_effects cuts at the box");
    // Not clipped: the shape goes on to 160, its shadow below it.
    let mut host = shot(1.0, parent(false));
    assert_eq!(host.pixel(100, 158), Color::BLUE);
    assert_ne!(host.pixel(100, 165), Color::WHITE);
    // A parent without anything that overflows clips at its box exactly.
    let plain = SkiaShape::new().width_request(60).height_request(60).background_color(Color::BLUE).translation_y(30);
    let mut host = shot(1.0, SkiaLayout::new().margin((70, 70, 0, 0)).is_clipped_to_bounds(true).children(plain));
    assert_eq!(host.pixel(100, 129), Color::BLUE);
    assert_eq!(host.pixel(100, 130), Color::WHITE);
}

/// Left / Top move a cached control by blitting its cache at an offset: the margin moves with it.
#[test]
fn a_cache_blitted_at_an_offset_keeps_its_shadow() {
    let mut live = shot(1.0, at(77, 75, shadowed(CacheType::None)));
    for cache in [CacheType::Operations, CacheType::Image] {
        let mut moved = shot(1.0, at(70, 70, shadowed(cache).left(7).top(5)));
        assert_same(&format!("{cache:?}"), &mut moved, &mut live, 200);
    }
}

/// A shape that contains its children: they are cut at its outline, its own shadow is not, and a
/// child's shadow is content like any other.
#[test]
fn a_shape_clips_its_children_but_not_its_own_shadow() {
    let child = SkiaShape::new().width_request(60).height_request(60).background_color(Color::RED).translation_y(30).shadows(shadow());
    for cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
        let mut host = shot(1.0, at(70, 70, shadowed(cache).children(child_clone(&child))));
        // Inside the shape: the child from 100 points down.
        assert_eq!(host.pixel(100, 99), Color::BLUE, "{cache:?}");
        assert_eq!(host.pixel(100, 101), Color::RED, "{cache:?}");
        // Below the shape: its own shadow, exactly as without the child.
        let mut alone = shot(1.0, at(70, 70, shadowed(CacheType::None)));
        for y in 130..160 {
            assert_eq!(host.pixel(100, y), alone.pixel(100, y), "{cache:?}, row {y}");
        }
    }

    fn child_clone(_: &Build<SkiaShape>) -> Build<SkiaShape> {
        SkiaShape::new().width_request(60).height_request(60).background_color(Color::RED).translation_y(30).shadows(shadow())
    }
}

// ---------------------------------------------------------------- layout and input do not see the margin

#[derive(Default)]
struct Taps {
    shape: Handle<SkiaShape>,
    parent: Handle<SkiaLayout>,
    count: u32,
}

fn tappable(cache: CacheType, parent_cache: CacheType) -> Headless<Taps> {
    let ui = Ui::new(Taps::default(), |app| {
        let shape = shadowed(cache).assign(&mut app.shape).on_tapped(|_me, app: &mut Taps, _cx| app.count += 1);
        let parent = SkiaLayout::new().margin((70, 70, 0, 0)).use_cache(parent_cache).assign(&mut app.parent).children(shape);
        SkiaLayout::new().fill().children(parent)
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    host
}

#[test]
fn the_margin_is_not_part_of_the_rect_and_takes_no_taps() {
    for (cache, parent_cache) in [(CacheType::None, CacheType::None), (CacheType::Image, CacheType::Image)] {
        let mut host = tappable(cache, parent_cache);
        let (shape, parent) = (host.ui.state.shape, host.ui.state.parent);
        assert_eq!(host.rect(shape), Rect::from_xywh(70.0, 70.0, 60.0, 60.0));
        assert_eq!(host.rect(parent), Rect::from_xywh(70.0, 70.0, 60.0, 60.0));
        // In the shadow, 10 points below the shape.
        assert_ne!(host.pixel(100, 140), Color::WHITE);
        host.tap(100.0, 140.0);
        assert_eq!(host.ui.state.count, 0, "{cache:?}");
        // The first and the last row and column of the rect itself.
        host.tap(70.0, 70.0);
        host.tap(129.0, 129.0);
        assert_eq!(host.ui.state.count, 2, "{cache:?}");
        host.tap(130.0, 100.0);
        host.tap(100.0, 69.0);
        assert_eq!(host.ui.state.count, 2, "{cache:?}");
    }
}

// ---------------------------------------------------------------- invalidation and cost

/// A changed shadow records the caches above it again, larger; frames without a change record
/// nothing.
#[test]
fn a_changed_shadow_grows_the_caches_above_it() {
    let mut host = tappable(CacheType::Image, CacheType::Image);
    let (shape, parent) = (host.ui.state.shape, host.ui.state.parent);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert_eq!((host.cache_records(shape), host.cache_records(parent)), (1, 1));
    // 130 + 22 = 152: nothing below that.
    assert_eq!(host.pixel(100, 156), Color::WHITE);

    let wide = SkiaShadow { blur: 8.0, ..shadow() };
    host.ui.tree.get_mut(shape).unwrap().set_shadows(wide);
    host.settle();
    assert_eq!((host.cache_records(shape), host.cache_records(parent)), (2, 2));
    // Now it reaches 130 + 10 + 24 = 164.
    assert_ne!(host.pixel(100, 156), Color::WHITE);
    let mut live = shot(1.0, at(70, 70, shadowed(CacheType::None).shadows(wide)));
    assert_same("after the change", &mut host, &mut live, 200);

    // No shadow any more: the caches shrink back to the rect.
    host.ui.tree.get_mut(shape).unwrap().set_shadows(Vec::new());
    host.settle();
    assert_eq!(host.pixel(100, 135), Color::WHITE);
    assert_eq!(host.pixel(100, 129), Color::BLUE);
}

/// An unchanged shape paints without allocating: its shaders, path, shadow filters and dash are
/// kept, cached or not.
#[test]
fn an_unchanged_shape_allocates_nothing_per_frame() {
    for cache in [CacheType::None, CacheType::Operations, CacheType::Image] {
        let gradient = SkiaGradient::new(GradientType::Linear, [Color::RED, Color::BLUE]);
        let shape = shadowed(cache)
            .shape_type(ShapeType::Polygon)
            .points(vec![(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)])
            .fill_gradient(gradient.clone())
            .stroke_width(3)
            .stroke_gradient(gradient)
            .stroke_path(vec![6.0, 3.0]);
        let mut host = shot(1.0, at(70, 70, shape));
        host.frame_after(16.0);
        let before = ALLOCATIONS.with(|a| a.get());
        for _ in 0..10 {
            host.frame_after(16.0);
        }
        assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0, "{cache:?}");
        assert_ne!(host.pixel(100, 145), Color::WHITE, "{cache:?}: the shadow is painted");
    }
}

/// A custom control that paints 5 pixels around its rect, reports that, and counts how often it
/// is asked.
struct Glow {
    asked: Arc<AtomicU32>,
}
impl Control for Glow {
    fn measure(&mut self, _cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(20.0, 20.0)
    }
    fn effects_margin(&self, _scale: f32) -> Thickness {
        self.asked.fetch_add(1, Ordering::Relaxed);
        Thickness::uniform(5.0)
    }
    fn paint(&self, cx: &mut PaintCx) {
        let mut paint = drawnui::skia::Paint::default();
        paint.set_color(Color::RED);
        cx.canvas.draw_rect(cx.rect.with_outset((5.0, 5.0)), &paint);
    }
}

/// The hook is for any control. The margin of a subtree is kept: frames without a change, and
/// changes elsewhere in the tree, do not ask the control again.
#[test]
fn a_margin_is_asked_for_once_until_the_control_changes() {
    #[derive(Default)]
    struct App {
        glow: Handle<Glow>,
        sibling: Handle<SkiaShape>,
    }
    for (cache, clipped) in [(CacheType::None, true), (CacheType::Image, false), (CacheType::Operations, true)] {
        let asked = Arc::new(AtomicU32::new(0));
        let ui = Ui::new(App::default(), |app| {
            let glow = Build::new(Glow { asked: asked.clone() }).assign(&mut app.glow);
            let sibling = SkiaShape::new().width_request(10).height_request(10).background_color(Color::BLUE).assign(&mut app.sibling);
            let parent = SkiaLayout::new().margin((70, 70, 0, 0)).use_cache(cache).is_clipped_to_bounds(clipped).clip_effects(false);
            SkiaLayout::new().fill().children(parent.children((glow, sibling)))
        })
        .background(Color::WHITE);
        let mut host = Headless::new(ui, 200, 200, 1.0);
        host.settle();
        let what = format!("{cache:?}, clipped {clipped}");
        // The parent is the glow's 20 x 20 box: the 5 pixels around it show through its cache or clip...
        assert_eq!(host.pixel(66, 80), Color::RED, "{what}");
        assert_eq!(host.pixel(94, 94), Color::RED, "{what}");
        // ...and nothing further out.
        assert_eq!(host.pixel(64, 80), Color::WHITE, "{what}");
        for _ in 0..10 {
            host.frame_after(16.0);
        }
        assert_eq!(asked.load(Ordering::Relaxed), 1, "{what}: unchanged frames");

        let sibling = host.ui.state.sibling;
        host.ui.tree.get_mut(sibling).unwrap().set_background_color(Color::GREEN);
        host.settle();
        assert_eq!(host.pixel(72, 72), Color::GREEN, "{what}");
        assert_eq!(asked.load(Ordering::Relaxed), 1, "{what}: a sibling changed");

        let glow = host.ui.state.glow;
        host.ui.tree.invalidate(glow, Dirty::DRAW);
        host.settle();
        assert_eq!(asked.load(Ordering::Relaxed), 2, "{what}: the control itself changed");
    }
}

// ---------------------------------------------------------------- labels

/// Glyph ink above the ascent and below the descent is outside the label's rect (letters with
/// stacked accents, a dot below). The label reports it as its effects margin, so an image cache,
/// which holds nothing but its own pixels, keeps that ink.
#[test]
fn a_cached_label_keeps_the_ink_outside_its_line_box() {
    let Ok(inter) = std::fs::read(INTER) else {
        eprintln!("Inter is not on this machine: nothing proved");
        return;
    };
    // (pixels, rect of the label, its margin)
    let render = |scale: f32, cache: CacheType| {
        let ui = Ui::new(Handle::<SkiaLabel>::default(), |label| {
            let text = SkiaLabel::new("\u{1ead}\u{1eac}\u{1fa}\u{1ef5}g\u{1ef1}").font_family("FontText").font_size(14);
            let text = text.text_color(Color::BLACK).use_cache(cache).margin((20, 20, 0, 0));
            SkiaLayout::new().fill().children(text.assign(label))
        });
        let ui = ui.font_bytes("Default", FONT).font_bytes("FontText", &inter).background(Color::WHITE);
        let (width, height) = ((200.0 * scale) as i32, (80.0 * scale) as i32);
        let mut host = Headless::new(ui, width, height, scale);
        host.settle();
        let label = host.ui.state;
        let margin = Control::effects_margin(host.ui.tree.find::<SkiaLabel>(label).unwrap(), scale);
        let pixels: Vec<Vec<Color>> = (0..height).map(|y| (0..width).map(|x| host.pixel(x, y)).collect()).collect();
        (pixels, host.rect(label), margin)
    };
    // Rows with solid ink; text on a cache surface is antialiased another way, its faint fringes differ.
    let inked = |pixels: &[Vec<Color>]| -> Vec<usize> {
        (0..pixels.len()).filter(|y| pixels[*y].iter().any(|c| c.r() < 128)).collect()
    };
    for scale in [1.25f32, 1.5] {
        let (live, rect, margin) = render(scale, CacheType::None);
        assert!(margin.top >= 1.0 && margin.bottom >= 1.0 && margin.left == 0.0 && margin.right == 0.0, "{margin:?}");
        let rows = inked(&live);
        let (top, bottom) = (rect.top.floor() as usize, rect.bottom.ceil() as usize);
        let outside = rows.iter().filter(|y| **y < top || **y >= bottom).count();
        assert!(outside > 0, "scale {scale}: no ink outside the rect {rect:?} (rows {rows:?}), nothing proved");
        // The margin covers all of it.
        assert!(rows[0] + margin.top as usize >= top && rows[rows.len() - 1] < bottom + margin.bottom as usize, "{rows:?}");

        let (operations, ..) = render(scale, CacheType::Operations);
        assert!(operations == live, "scale {scale}: the default Operations cache differs from the uncached label");
        let (image, ..) = render(scale, CacheType::Image);
        assert_eq!(inked(&image), rows, "scale {scale}: the Image cache lost rows of ink");
    }
}

// ---------------------------------------------------------------- the hollow cards of the Shapes page

/// A HelloMaui demo card (150 x 110, rounded 8, #2B3035) at (20, 20) on the page color, with the
/// demo shape at the rect upstream draws it in. That is not the centered 100 x 60: upstream's Center
/// grows a control by a pixel or two, and a shape's outline comes from the rect of an earlier
/// arrange pass. Both are layout matters; the rects here are the ones the probe measured.
fn card(demo: Build<SkiaShape>, rect: [i32; 4]) -> Headless<()> {
    let [left, top, right, bottom] = rect;
    let demo = demo.margin((left - 20, top - 20, 0, 0)).width_request(right - left).height_request(bottom - top);
    let card = SkiaShape::new().margin((20, 20, 0, 0)).width_request(150).height_request(110).corner_radius(8);
    let card = card.background_color(Color::from_rgb(0x2B, 0x30, 0x35)).children(demo);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(card)).background(Color::from_rgb(0x21, 0x25, 0x29));
    let mut host = Headless::new(ui, 190, 150, 1.0);
    host.settle();
    host
}

/// HelloMaui ShapesPage "Hollow + shadow", "Hollow: ClipBackgroundColor" and "StrokeGradient":
/// upstream draws no shadow for a shape without a background (the shadow is a filter on the fill),
/// and without shadows `ClipBackgroundColor` clips nothing, so that card is a filled box.
#[test]
fn the_hollow_cards_of_the_shapes_page_look_as_upstream() {
    const CARD: &str = "FF2B3035 ";
    let shadow = SkiaShadow::new(Color::BLACK).x(0).y(5).blur(5).opacity(0.7);
    let hollow = || SkiaShape::new().corner_radius(12).clip_background_color(true).stroke_color(Color::WHITE).stroke_width(2);

    // No background: the white stroke and nothing else, no shadow below it.
    let mut host = card(hollow().shadows(shadow), [46, 46, 146, 105]);
    assert_pixels("hollow + shadow, column", column(&mut host, 95, 40, 125, 5), &CARD.repeat(18));
    let across = format!("{}FF979A9C {}", CARD.repeat(22), CARD.repeat(2));
    assert_pixels("hollow + shadow, row", row(&mut host, 75, 35, 155, 5), &across);
    let edge = format!("{}FF96989B FFFFFFFF FF96989B {}", CARD.repeat(2), CARD.repeat(8));
    assert_pixels("hollow + shadow, bottom edge", column(&mut host, 95, 100, 112, 1), &edge);

    // With a background the shadow is there and the inside stays empty.
    let mut host = card(hollow().background_color(Color::BLACK).shadows(shadow), [46, 46, 146, 105]);
    assert_pixels(
        "hollow + shadow with a background, column",
        column(&mut host, 95, 40, 125, 5),
        "FF2B3035 FF2A2E33 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF2B3035 FF16181B FF222629 FF2A2E33 FF2B3035 FF2B3035",
    );
    assert_pixels(
        "hollow + shadow with a background, bottom edge",
        column(&mut host, 95, 100, 112, 1),
        "FF2B3035 FF2B3035 FF96989B FFFFFFFF FF8A8B8C FF16181B FF181B1D FF1A1D20 FF1D2023 FF1F2327 FF222629 FF24282C FF262A2E",
    );

    // Background, no shadows: nothing is clipped away.
    let blue = Color::from_rgb(0x0D, 0x6E, 0xFD);
    let filled = SkiaShape::new().corner_radius(12).clip_background_color(true).background_color(blue).stroke_color(blue).stroke_width(3);
    let mut host = card(filled, [40, 40, 152, 111]);
    let fill = "FF0D6EFD ";
    // The edge pixel: upstream's Skia build gives FF1A509D on vertical edges, FF1B4F99 on horizontal ones.
    let across = format!("{CARD}FF1B4F99 {}{CARD}", fill.repeat(22));
    assert_pixels("hollow without shadows, row", row(&mut host, 75, 35, 155, 5), &across);
    let down = format!("{CARD}FF1B4F99 {}FF1B4F99 {CARD}", fill.repeat(13));
    assert_pixels("hollow without shadows, column", column(&mut host, 95, 35, 115, 5), &down);

    // No background, a stroke gradient: the stroke alone, 8 px wide from x = 41 and up to x = 151.
    let gradient = SkiaGradient::new(GradientType::Linear, [Color::from_rgb(0xFF, 0xC1, 0x07), Color::from_rgb(0xD6, 0x33, 0x84)]);
    let stroked = SkiaShape::new().corner_radius(16).clip_background_color(true).stroke_width(8);
    let mut host = card(stroked.stroke_gradient(gradient.end_x_ratio(1).end_y_ratio(0)), [41, 41, 151, 110]);
    assert_pixels("stroke gradient, left", row(&mut host, 75, 42, 47, 1), "FFFFC107 FFFFC107 FFFFC107 FFFFC008 FFFEBF09 FFFEBE0A");
    let right = "FFD73681 FFD73582 FFD63483 FFD63384 FFD63384 FFD63384 FFD63384";
    assert_pixels("stroke gradient, right", row(&mut host, 75, 144, 150, 1), right);
    assert_pixels("stroke gradient, top", column(&mut host, 95, 41, 48, 1), &"FFEB7B45 ".repeat(8));
    assert_pixels("stroke gradient, bottom", column(&mut host, 95, 102, 109, 1), &"FFEB7B45 ".repeat(8));
    assert_pixels("stroke gradient, inside", row(&mut host, 75, 52, 139, 3), &CARD.repeat(30));
    assert_pixels("stroke gradient, inside", column(&mut host, 95, 50, 100, 5), &CARD.repeat(11));
}
