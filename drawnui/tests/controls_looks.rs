//! SkiaButton looks, and the React "Common Controls" page (LooksPage.tsx) as a scene: one card per
//! style with every control, the live card that changes its style, the handlers, what a frame
//! costs and allocates.
//!
//! The frame costs: `cargo test --release -p drawnui --test controls_looks -- --ignored --nocapture`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{ControlId, Tree};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const PAGE: Color = Color::from_rgb(0xF5, 0xF5, 0xF5);
const STYLES: [PrebuiltControlStyle; 5] = [
    PrebuiltControlStyle::Unset,
    PrebuiltControlStyle::Windows,
    PrebuiltControlStyle::Cupertino,
    PrebuiltControlStyle::Material,
    PrebuiltControlStyle::Material3,
];

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

fn allocations() -> usize {
    ALLOCATIONS.with(|a| a.get())
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b())
}

fn tagged(tree: &Tree, id: ControlId, tag: &str) -> Option<ControlId> {
    if tree.base(id)?.p.tag == tag {
        return Some(id);
    }
    tree.children(id).iter().find_map(|c| tagged(tree, *c, tag))
}

#[derive(Default)]
struct Buttons {
    ids: Vec<ControlId>,
    style: PrebuiltControlStyle,
}

#[test]
fn button_looks_follow_the_style() {
    let ui = Ui::new(Buttons::default(), |app: &mut Buttons| {
        let buttons: Vec<_> = STYLES.iter().map(|s| SkiaButton::new("Button").control_style(*s)).collect();
        app.ids = buttons.iter().map(|b| b.id()).collect();
        SkiaLayout::column().padding(20).spacing(20).children(buttons)
    })
    .font_bytes("Default", FONT)
    .background(PAGE);
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    for (i, style) in STYLES.iter().enumerate() {
        let look = ButtonLook::of(*style);
        let id = host.ui.state.ids[i];
        let r = host.rect(id);
        let label = host.ui.tree.children(id)[0];
        let caption = host.ui.tree.find::<SkiaLabel>(label).unwrap();
        assert_eq!((caption.p.font_size, caption.p.font_weight), (look.font_size, look.font_weight), "{style:?}");
        // The floor of the style, or the caption and the padding (16 / 10) when taller.
        let caption_height = host.rect(label).height();
        assert_eq!(r.width(), 100.0, "{style:?}");
        assert_eq!(r.height(), look.minimum_height.max(caption_height + 20.0), "{style:?}");
        assert!(near(host.pixel(r.left as i32 + 4, r.center_y() as i32), look.background, 1), "{style:?}");
        // Inside a 4 point corner at (2, 2), outside an 8 or 20 point one.
        let corner = host.pixel(r.left as i32 + 1, r.top as i32 + 1);
        if look.corner_radius <= 4.0 {
            assert!(near(corner, look.background, 40), "{style:?}: {corner:?}");
        } else {
            assert!(!near(corner, look.background, 40), "{style:?}: {corner:?}");
        }
        // A shadow under the frame for the looks that have one.
        let below = host.pixel(r.center_x() as i32, r.bottom as i32 + 1);
        assert_eq!(below != PAGE, look.shadow.is_some(), "{style:?}: {below:?}");
    }
    // Material3: 20 points round, Material 4, Windows 4 (DrawnUI.React; C# draws 8 and 2).
    assert_eq!(ButtonLook::of(PrebuiltControlStyle::Material3).corner_radius, 20.0);
    assert_eq!(ButtonLook::of(PrebuiltControlStyle::Windows).corner_radius, 4.0);
}

#[test]
fn a_style_change_moves_the_minimum_size_the_look_set_and_keeps_the_app_values() {
    let ui = Ui::new(Buttons::default(), |app: &mut Buttons| {
        // A thin padding: the style's floor decides the height, not the caption.
        let styled = SkiaButton::new("B").padding((16, 2)).observe(|me, app: &Buttons| me.set_control_style(app.style));
        let own = SkiaButton::new("B").minimum_height_request(60).corner_radius(3).observe(|me, app: &Buttons| me.set_control_style(app.style));
        app.ids = vec![styled.id(), own.id()];
        SkiaLayout::column().padding(20).spacing(20).children((styled, own))
    })
    .font_bytes("Default", FONT)
    .background(PAGE);
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    assert_eq!(host.rect(host.ui.state.ids[0]).height(), 41.0);
    host.ui.state.style = PrebuiltControlStyle::Windows;
    host.ui.state_changed();
    host.settle();
    let (styled, own) = (host.rect(host.ui.state.ids[0]), host.rect(host.ui.state.ids[1]));
    assert!(host.rect(host.ui.tree.children(host.ui.state.ids[0])[0]).height() + 4.0 < 32.0);
    assert_eq!(styled.height(), 32.0);
    assert_eq!(own.height(), 60.0);
    assert!(near(host.pixel(styled.left as i32 + 4, styled.center_y() as i32), Color::from_rgb(0x00, 0x78, 0xD7), 1));
    // The app's radius stays: 3 points, (1, 1) is inside.
    assert!(near(host.pixel(own.left as i32 + 1, own.top as i32 + 1), Color::from_rgb(0x00, 0x78, 0xD7), 60));
}

#[test]
fn the_caption_takes_the_font_fallback_of_the_button() {
    let ui = Ui::new(Buttons::default(), |app: &mut Buttons| {
        let button = SkiaButton::new("->").font_family_fallback("Symbols, Emoji");
        app.ids = vec![button.id()];
        SkiaLayout::column().padding(20).children(button)
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 300, 200, 1.0);
    host.settle();
    let id = host.ui.state.ids[0];
    let caption = host.ui.tree.find::<SkiaLabel>(host.ui.tree.children(id)[0]).unwrap();
    assert_eq!(caption.p.font_family_fallback, "Symbols, Emoji");
    host.ui.tree.find_mut::<SkiaButton>(id).unwrap().set_font_family_fallback("");
    host.settle();
    assert_eq!(host.ui.tree.find::<SkiaLabel>(host.ui.tree.children(id)[0]).unwrap().p.font_family_fallback, "");
}

// ---------------------------------------------------------------- the Common Controls page

#[derive(Default)]
struct Looks {
    last: String,
    live: usize,
    scroll: Handle<SkiaScroll>,
    live_button: Handle<SkiaButton>,
    /// The first card of each title: its switch, radio Two, slider.
    cards: Vec<(String, ControlId, ControlId, ControlId, ControlId)>,
}

/// LooksPage.tsx Card: the same controls, only the style changes. The live card reads its style
/// from the state.
fn card(app: &mut Looks, title: &'static str, style: Option<PrebuiltControlStyle>) -> Build<SkiaShape> {
    let live = move |i: usize| style.unwrap_or(STYLES[i % STYLES.len()]);
    let caption = move |app: &Looks| match style {
        Some(_) => title.to_owned(),
        None => format!("Live - {:?}", STYLES[app.live]),
    };
    let switch = SkiaSwitch::new()
        .control_style(live(0))
        .is_toggled(true)
        .vertical_options(LayoutOptions::Center)
        .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
        .on_toggled(move |_me, app: &mut Looks, _cx, v| app.last = format!("{title} switch: {v}"));
    let checkbox = SkiaCheckbox::new()
        .control_style(live(0))
        .is_toggled(true)
        .vertical_options(LayoutOptions::Center)
        .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
        .on_toggled(move |_me, app: &mut Looks, _cx, v| app.last = format!("{title} checkbox: {v}"));
    let radio = |text: &'static str, on: bool| {
        SkiaRadioButton::new(text)
            .control_style(live(0))
            .is_toggled(on)
            .group_name(if style.is_some() { title } else { "Live" })
            .vertical_options(LayoutOptions::Center)
            .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
            .on_toggled(move |_me, app: &mut Looks, _cx, v| {
                if v {
                    app.last = format!("{title} radio: {text}");
                }
            })
    };
    let (one, two) = (radio("One", true), radio("Two", false));
    let slider = SkiaSlider::new()
        .control_style(live(0))
        .end(65)
        .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
        .on_end_changed(move |_me, app: &mut Looks, _cx, v| app.last = format!("{title} slider: {v:.0}"));
    app.cards.push((title.to_owned(), switch.id(), two.id(), slider.id(), one.id()));
    SkiaShape::new()
        .corner_radius(16)
        .background_color(PAGE)
        .padding((18, 14))
        .fill_x()
        .use_cache(CacheType::Image)
        .children(SkiaLayout::column().spacing(14).fill_x().children((
            SkiaLabel::new(title)
                .font_size(16)
                .font_attributes(FontAttributes::Bold)
                .text_color(Color::from_rgb(0x11, 0x18, 0x27))
                .observe(move |me, app: &Looks| me.set_text(caption(app))),
            SkiaLayout::row().spacing(16).fill_x().children((switch, checkbox, one, two)),
            SkiaButton::new("Button")
                .control_style(live(0))
                .horizontal_options(LayoutOptions::Start)
                .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
                .on_tapped(move |_me, app: &mut Looks, _cx| app.last = format!("{title} button tapped")),
            SkiaProgress::new().control_style(live(0)).value(65).observe(move |me, app: &Looks| me.set_control_style(live(app.live))),
            slider,
            SkiaSlider::new()
                .control_style(live(0))
                .enable_range(true)
                .start(20)
                .end(80)
                .observe(move |me, app: &Looks| me.set_control_style(live(app.live)))
                .on_start_changed(move |_me, app: &mut Looks, _cx, v| app.last = format!("{title} range start: {v:.0}"))
                .on_end_changed(move |_me, app: &mut Looks, _cx, v| app.last = format!("{title} range end: {v:.0}")),
        )))
}

fn looks_page(app: &mut Looks) -> Build<SkiaScroll> {
    let cards = vec![
        card(app, "Live", None),
        card(app, "Default", Some(PrebuiltControlStyle::Unset)),
        card(app, "Windows - Fluent", Some(PrebuiltControlStyle::Windows)),
        card(app, "Cupertino - iOS", Some(PrebuiltControlStyle::Cupertino)),
        card(app, "Material - Android", Some(PrebuiltControlStyle::Material)),
        card(app, "Material3 - Android", Some(PrebuiltControlStyle::Material3)),
    ];
    let stack = SkiaLayout::column()
        .spacing(16)
        .padding(16)
        .horizontal_options(LayoutOptions::Center)
        .maximum_width_request(720)
        .children((
            SkiaLabel::new("Common Controls").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
            SkiaLabel::new("Last:")
                .font_size(13)
                .text_color(Color::from_rgb(0x6E, 0xA8, 0xFE))
                .horizontal_options(LayoutOptions::Center)
                .observe(|me, app: &Looks| me.set_text(format!("Last: {}", app.last))),
            SkiaButton::new("")
                .horizontal_options(LayoutOptions::Center)
                .assign(&mut app.live_button)
                .observe(|me, app: &Looks| me.set_text(format!("Live card: {:?} - tap to switch style", STYLES[app.live])))
                .on_tapped(|_me, app: &mut Looks, _cx| app.live = (app.live + 1) % STYLES.len()),
            cards,
        ));
    SkiaScroll::new().fill().assign(&mut app.scroll).content(stack)
}

fn looks_host(width: f32, height: f32, scale: f32) -> Headless<Looks> {
    let ui = Ui::new(Looks::default(), looks_page).font_bytes("Default", FONT).background(Color::from_rgb(0x21, 0x25, 0x29));
    let mut host = Headless::new(ui, (width * scale) as i32, (height * scale) as i32, scale);
    host.settle();
    host
}

#[test]
fn the_live_card_follows_the_style_and_every_control_reports() {
    let mut host = looks_host(420.0, 2400.0, 1.0);
    // Switch sizes per style (DrawnUI.React SetContentSize).
    let sizes = [(46.0, 28.0), (48.0, 22.0), (51.0, 31.0), (46.0, 28.0), (52.0, 32.0)];
    let live_switch = host.ui.state.cards[0].1;
    for step in 0..=STYLES.len() {
        let style = step % STYLES.len();
        let r = host.rect(live_switch);
        assert_eq!((r.width(), r.height()), sizes[style], "{:?}", STYLES[style]);
        // The same size as the card of that style.
        assert_eq!(r.size(), host.rect(host.ui.state.cards[1 + style].1).size());
        let b = host.rect(host.ui.state.live_button);
        host.tap(b.center_x(), b.center_y());
    }

    // The handlers of the Default card.
    let (_, switch, two, slider, one) = host.ui.state.cards[1].clone();
    let r = host.rect(switch);
    host.tap(r.center_x(), r.center_y());
    assert_eq!(host.ui.state.last, "Default switch: false");
    let r = host.rect(two);
    host.tap(r.left + 5.0, r.center_y());
    assert_eq!(host.ui.state.last, "Default radio: Two");
    assert!(!host.ui.tree.find::<SkiaToggle>(one).unwrap().p.is_toggled);
    // Other cards keep their own group.
    assert!(host.ui.tree.find::<SkiaToggle>(host.ui.state.cards[2].4).unwrap().p.is_toggled);
    let r = host.rect(slider);
    host.tap(r.left + 30.0, r.center_y());
    // 30 - 17.5 = 12.5 of (width - 35).
    let travel = r.width() - 35.0;
    assert_eq!(host.ui.state.last, format!("Default slider: {:.0}", (12.5 / travel * 100.0 + 0.5).floor()));
    let label = host.ui.tree.find::<SkiaSlider>(slider).unwrap();
    assert_eq!(label.p.end, (12.5 / travel * 100.0 + 0.5).floor());
}

#[test]
fn the_page_renders_at_scale_two() {
    let mut host = looks_host(420.0, 2400.0, 2.0);
    let (_, switch, ..) = host.ui.state.cards[1].clone();
    let r = host.rect(switch);
    assert_eq!(r.size(), Size::new(92.0, 56.0));
    // The Default card's switch is on: crimson track.
    assert!(near(host.pixel(r.left as i32 + 12, r.center_y() as i32), Color::from_rgb(0xDC, 0x14, 0x3C), 1));
}

#[test]
fn scrolling_the_page_allocates_nothing() {
    let mut host = looks_host(420.0, 700.0, 1.0);
    let vp = host.rect(host.ui.state.scroll);
    let fling = |host: &mut Headless<Looks>, up: bool| {
        let (from, to) = if up { (vp.bottom - 50.0, vp.top + 50.0) } else { (vp.top + 50.0, vp.bottom - 50.0) };
        host.pan((vp.center_x(), from), (vp.center_x(), to), 80.0, 5);
    };
    // Every card recorded once.
    for _ in 0..6 {
        fling(&mut host, true);
        host.settle();
    }
    for _ in 0..6 {
        fling(&mut host, false);
        host.settle();
    }
    fling(&mut host, true);
    host.frame_after(16.0);
    let before = allocations();
    for _ in 0..20 {
        host.frame_after(16.0);
    }
    let allocated = allocations() - before;
    assert!(host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().is_animating());
    assert_eq!(allocated, 0);
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_cost_of_the_page() {
    const FRAMES: u32 = 100;
    for scale in [1.0f32, 2.0] {
        let mut host = looks_host(420.0, 800.0, scale);
        let start = Instant::now();
        for _ in 0..FRAMES {
            host.frame_after(16.0);
        }
        let idle = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;

        // A slider drag: the slider's image cache and its card's are recorded again every frame.
        let slider = host.ui.state.cards[1].3;
        let r = host.rect(slider);
        let (x, y) = (r.left + 30.0 * scale, r.center_y());
        host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
        host.frame_after(16.0);
        let (start, before) = (Instant::now(), allocations());
        for i in 0..FRAMES {
            let dx = (i % 40) as f32 * 3.0 * scale;
            host.ui.pointer(PointerKind::Move, x + dx, y, host.time_ms());
            host.frame_after(16.0);
        }
        let drag = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
        let per_frame = (allocations() - before) as f64 / FRAMES as f64;
        host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
        host.settle();
        println!("scale {scale}: idle {idle:.0} us per frame; slider drag {drag:.0} us, {per_frame:.1} allocations per frame");

        // The engine alone: a slider without handlers in a cached card.
        let ui = Ui::new(Handle::<SkiaSlider>::default(), |slider| {
            SkiaShape::new().padding(20).fill_x().use_cache(CacheType::Image).children(SkiaSlider::new().end(50).assign(slider))
        });
        let mut host = Headless::new(ui, (420.0 * scale) as i32, (200.0 * scale) as i32, scale);
        host.settle();
        let r = host.rect(host.ui.state);
        let (x, y) = (r.left + 10.0 * scale, r.center_y());
        host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
        host.frame_after(16.0);
        let (start, before) = (Instant::now(), allocations());
        for i in 0..FRAMES {
            let dx = (i % 40) as f32 * 3.0 * scale;
            host.ui.pointer(PointerKind::Move, x + dx, y, host.time_ms());
            host.frame_after(16.0);
        }
        let drag = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
        let per_frame = (allocations() - before) as f64 / FRAMES as f64;
        println!("scale {scale}: lone slider drag {drag:.0} us, {per_frame:.1} allocations per frame");
    }
}

#[test]
fn a_tagged_part_exists_for_every_toggle() {
    let host = looks_host(420.0, 2400.0, 1.0);
    let (_, switch, two, ..) = host.ui.state.cards[1].clone();
    let tree = &host.ui.tree;
    assert!(tagged(tree, switch, "Frame").is_some() && tagged(tree, switch, "Thumb").is_some());
    assert!(tagged(tree, two, "On").is_some() && tagged(tree, two, "Text").is_some());
}
