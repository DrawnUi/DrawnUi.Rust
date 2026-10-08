//! ClipEffects on overlays, ImageDoubleBuffered and ImageComposite caches, on the CPU canvas.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::skia::Paint as SkPaint;
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

fn host<T: Control>(content: Build<T>) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

fn square(color: Color, left: f32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(40).height_request(40).margin(Thickness::new(left, 10.0, 0.0, 0.0)).background_color(color)
}

#[test]
fn overlays_leave_the_shape_only_without_clip_effects() {
    for clip in [true, false] {
        let mut id: Handle<SkiaLayout> = Handle::default();
        let mut host = host(square(Color::BLUE, 80.0).clip_effects(clip).assign(&mut id));
        // A ripple from the center, 300 points at its end (cubic in): at 400 ms of 500, 150.
        host.ui.tree.cx().play_ripple(id, Color::BLACK, 20.0, 20.0, 500.0);
        host.frame_after(16.0);
        host.frame_after(400.0);
        let outside = host.pixel(70, 30);
        assert_eq!(outside == Color::WHITE, clip, "clip_effects {clip}: {outside:?}");
    }
}

/// A 20 x 20 point control of one color that reports an effects margin; both can change.
struct Probe {
    color: Rc<Cell<Color>>,
    margin: Rc<Cell<f32>>,
}
impl Control for Probe {
    fn measure(&mut self, cx: &mut LayoutCx, _width: f32, _height: f32) -> Size {
        Size::new(20.0 * cx.scale, 20.0 * cx.scale)
    }
    fn paint(&self, cx: &mut PaintCx) {
        let mut paint = SkPaint::default();
        paint.set_color(self.color.get());
        cx.canvas.draw_rect(cx.rect, &paint);
    }
    fn effects_margin(&self, _scale: f32) -> Thickness {
        Thickness::uniform(self.margin.get())
    }
}

#[test]
fn a_double_buffered_cache_keeps_the_last_one_while_a_new_one_cannot_be_made() {
    for (cache, kept) in [(CacheType::ImageDoubleBuffered, true), (CacheType::Image, false)] {
        let (color, margin) = (Rc::new(Cell::new(Color::RED)), Rc::new(Cell::new(0.0)));
        let probe = Build::new(Probe { color: color.clone(), margin: margin.clone() }).margin(Thickness::new(50.0, 50.0, 0.0, 0.0));
        let probe = probe.use_cache(cache);
        let id = probe.id();
        let mut host = host(probe);
        assert_eq!(host.pixel(60, 60), Color::RED);
        assert_eq!(host.cache_records(id), 1);
        host.frame_after(16.0);
        assert_eq!(host.cache_records(id), 1, "{cache:?} blits its cache");

        // A margin no surface can hold (two million pixels a side): no new cache can be made.
        color.set(Color::BLUE);
        margin.set(1_000_000.0);
        host.ui.tree.invalidate(id, Dirty::DRAW);
        host.settle();
        let shown = host.pixel(60, 60);
        let expected = if kept { Color::RED } else { Color::BLUE };
        assert_eq!(shown, expected, "{cache:?}: the last cache, or drawn live");

        // It can be made again: the new content shows.
        margin.set(0.0);
        host.ui.tree.invalidate(id, Dirty::DRAW);
        host.settle();
        assert_eq!(host.pixel(60, 60), Color::BLUE, "{cache:?}");
    }
}

/// A 200 x 100 layout with red, green and blue squares, cached as `cache`.
fn composite(cache: CacheType) -> (Headless<()>, ControlId, [ControlId; 3]) {
    let squares = [square(Color::RED, 10.0), square(Color::GREEN, 60.0), square(Color::BLUE, 110.0)];
    let ids = [squares[0].id(), squares[1].id(), squares[2].id()];
    let [a, b, c] = squares;
    let parent = SkiaLayout::new().fill().background_color(Color::from_rgb(240, 240, 240)).use_cache(cache).children((a, b, c));
    let id = parent.id();
    (host(parent), id, ids)
}

fn record(host: &Headless<()>, id: ControlId) -> (bool, Vec<ControlId>) {
    let record = host.ui.tree.last_composite_record(id).expect("recorded");
    (record.partial, record.children)
}

fn same_pixels(a: &mut Headless<()>, b: &mut Headless<()>, what: &str) {
    for y in (0..100).step_by(3) {
        for x in 0..200 {
            assert_eq!(a.pixel(x, y), b.pixel(x, y), "{what}: ({x}, {y})");
        }
    }
}

#[test]
fn a_composite_draws_again_only_the_children_that_changed() {
    let (mut host, parent, [red, green, blue]) = composite(CacheType::ImageComposite);
    let (mut plain, _, [plain_red, plain_green, _]) = composite(CacheType::Image);
    assert_eq!(record(&host, parent), (false, vec![red, green, blue]));
    same_pixels(&mut host, &mut plain, "first record");

    // Green moves right, onto blue: green (where it was and where it is) and blue are drawn again.
    host.ui.tree.any_mut(green).unwrap().set_translation_x(30);
    plain.ui.tree.any_mut(plain_green).unwrap().set_translation_x(30);
    host.settle();
    plain.settle();
    assert_eq!(record(&host, parent), (true, vec![green, blue]));
    same_pixels(&mut host, &mut plain, "green moved onto blue");

    // A new color on red, apart from the others: red alone.
    host.ui.tree.any_mut(red).unwrap().set_background_color(Color::BLACK);
    plain.ui.tree.any_mut(plain_red).unwrap().set_background_color(Color::BLACK);
    host.settle();
    plain.settle();
    assert_eq!(record(&host, parent), (true, vec![red]));
    same_pixels(&mut host, &mut plain, "red changed");

    // The layout itself changed: everything.
    host.ui.tree.any_mut(parent).unwrap().set_background_color(Color::from_rgb(200, 200, 255));
    host.settle();
    assert_eq!(record(&host, parent), (false, vec![red, green, blue]));
    assert_eq!(host.ui.tree.cx().last_composite_record(parent).map(|r| r.children.len()), Some(3));
    assert_eq!(host.pixel(5, 5), Color::from_rgb(200, 200, 255));
}

#[test]
fn a_child_moving_inside_a_composite_allocates_nothing_per_frame() {
    let (mut host, parent, [_, green, _]) = composite(CacheType::ImageComposite);
    let mut x = 0.0;
    let mut frame = |host: &mut Headless<()>| {
        x += 1.0;
        host.ui.tree.any_mut(green).unwrap().set_translation_y(x % 20.0);
        host.frame_after(16.0);
    };
    frame(&mut host);
    frame(&mut host);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..10 {
        frame(&mut host);
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
    assert!(record(&host, parent).0);
}

#[test]
fn a_handler_reads_the_last_composite_record() {
    #[derive(Default)]
    struct App {
        parent: Option<ControlId>,
        read: Option<CompositeRecord>,
    }
    let ui = Ui::new(App::default(), |app: &mut App| {
        let squares = (square(Color::RED, 10.0), square(Color::GREEN, 60.0), square(Color::BLUE, 110.0));
        let parent = SkiaLayout::new().fill().background_color(Color::WHITE).use_cache(CacheType::ImageComposite).children(squares);
        app.parent = Some(parent.id());
        // A tap on this square reads the record of the layout.
        let reader = SkiaLayout::new().width_request(20).height_request(20).margin(Thickness::new(170.0, 70.0, 0.0, 0.0));
        let reader = reader.background_color(Color::BLACK).on_tapped(|_me, app: &mut App, cx| app.read = cx.last_composite_record(app.parent.unwrap()));
        SkiaLayout::new().fill().children((parent, reader))
    });
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    let parent = host.ui.state.parent.unwrap();
    let green = host.ui.tree.children(parent)[1];
    host.ui.tree.any_mut(green).unwrap().set_translation_y(5);
    host.settle();
    host.tap(180.0, 80.0);
    assert_eq!(host.ui.state.read, Some(CompositeRecord { partial: true, children: vec![green], areas: vec![], changed: vec![] }));
}

#[test]
fn a_deep_change_inside_a_composite_allocates_nothing_per_frame() {
    // A card in an uncached stack, the stack a child of the composite: redrawn by its area.
    let card = square(Color::GREEN, 60.0);
    let card_id = card.id();
    let inner = SkiaLayout::new().fill().children((square(Color::RED, 10.0), card));
    let parent = SkiaLayout::new().fill().background_color(Color::WHITE).use_cache(CacheType::ImageComposite).children(inner);
    let id = parent.id();
    let mut host = host(parent);
    let mut i = 0u8;
    let mut frame = |host: &mut Headless<()>| {
        i = i.wrapping_add(1);
        host.ui.tree.any_mut(card_id).unwrap().set_background_color(Color::from_rgb(0, i, 0));
        host.frame_after(16.0);
    };
    frame(&mut host);
    frame(&mut host);
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..10 {
        frame(&mut host);
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
    let record = host.ui.tree.last_composite_record(id).expect("recorded");
    assert_eq!(record.changed, [card_id]);
}
