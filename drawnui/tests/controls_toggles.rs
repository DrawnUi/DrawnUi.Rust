//! SkiaSwitch, SkiaCheckbox and SkiaRadioButton in the five looks: sizes, thumb travel and colors
//! computed from DrawnUI.React (SkiaSwitch.ts, SkiaCheckbox.ts, SkiaRadioButton.ts), taps,
//! `on_toggled`, radio groups, a style change on a live control.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{ControlId, Tree};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const PAGE: Color = Color::from_rgb(0xF5, 0xF5, 0xF5);
const CRIMSON: Color = Color::from_rgb(0xDC, 0x14, 0x3C);
const STYLES: [PrebuiltControlStyle; 5] = [
    PrebuiltControlStyle::Unset,
    PrebuiltControlStyle::Windows,
    PrebuiltControlStyle::Cupertino,
    PrebuiltControlStyle::Material,
    PrebuiltControlStyle::Material3,
];

#[derive(Default)]
struct App {
    ids: Vec<ControlId>,
    /// (index of the control, value) per `on_toggled`.
    toggled: Vec<(usize, bool)>,
    style: PrebuiltControlStyle,
}

fn host<B: Into<drawnui::Detached>>(scale: f32, build: impl FnOnce(&mut App) -> B) -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(PAGE);
    let mut host = Headless::new(ui, (400.0 * scale) as i32, (600.0 * scale) as i32, scale);
    host.settle();
    host
}

/// The first control under `id` (itself included) with this tag.
fn tagged(tree: &Tree, id: ControlId, tag: &str) -> ControlId {
    fn find(tree: &Tree, id: ControlId, tag: &str) -> Option<ControlId> {
        if tree.base(id)?.p.tag == tag {
            return Some(id);
        }
        tree.children(id).iter().find_map(|c| find(tree, *c, tag))
    }
    find(tree, id, tag).unwrap_or_else(|| panic!("no child tagged {tag}"))
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b())
}

fn rgb(v: u32) -> Color {
    Color::new(0xFF00_0000 | v)
}

/// One switch per style, off and on, stacked at 20 points from the left.
fn switches(app: &mut App) -> Build<SkiaLayout> {
    let mut children = Vec::new();
    for (i, style) in STYLES.iter().enumerate() {
        for on in [false, true] {
            let index = i * 2 + on as usize;
            let switch = SkiaSwitch::new()
                .control_style(*style)
                .is_toggled(on)
                .on_toggled(move |_me, app: &mut App, _cx, value| app.toggled.push((index, value)));
            app.ids.push(switch.id());
            children.push(switch);
        }
    }
    SkiaLayout::column().padding(20).spacing(10).children(children)
}

#[test]
fn switch_sizes_and_thumb_travel_follow_the_style() {
    // (width, height) points, thumb side at scales 1 and 2 (pixels), thumb travel when on (points):
    // track width - thumb width - thumb margins, the thumb width as drawn (DrawnUI.React ThumbPosForOn).
    let expected = [
        ((46.0, 28.0), (24.0, 48.0), (18.0, 18.0)),
        // Margin 5.5: 6 pixels a side at scale 1 (halves to even), 11 at scale 2.
        ((48.0, 22.0), (10.0, 22.0), (27.0, 26.0)),
        ((51.0, 31.0), (27.0, 54.0), (20.0, 20.0)),
        ((46.0, 28.0), (28.0, 56.0), (18.0, 18.0)),
        ((52.0, 32.0), (24.0, 48.0), (20.0, 20.0)),
    ];
    for (s, scale) in [1.0f32, 2.0].into_iter().enumerate() {
        let host = host(scale, switches);
        let tree = &host.ui.tree;
        for (i, ((w, h), thumb, travel)) in expected.iter().enumerate() {
            for on in [false, true] {
                let id = host.ui.state.ids[i * 2 + on as usize];
                let r = host.rect(id);
                assert_eq!((r.width(), r.height()), (w * scale, h * scale), "style {i} scale {scale}");
                let t = tree.base(tagged(tree, id, "Thumb")).unwrap();
                let side = if s == 0 { thumb.0 } else { thumb.1 };
                assert_eq!((t.rect.width(), t.rect.height()), (side, side), "style {i} scale {scale}");
                let travel = if s == 0 { travel.0 } else { travel.1 };
                assert_eq!(t.p.translation_x, if on { travel } else { 0.0 }, "style {i} on {on} scale {scale}");
            }
        }
        // Built with the state: nothing was reported.
        assert!(host.ui.state.toggled.is_empty());
    }
}

#[test]
fn switch_colors_follow_the_style() {
    // Track color off (at the right end) and on (at the left end), DrawnUI.React StyleDefault.
    let expected = [
        (rgb(0xD7DBE0), CRIMSON),
        // Windows off: a transparent track inside its outline.
        (PAGE, rgb(0x0078D7)),
        (rgb(0xE5E5E5), rgb(0x30D158)),
        (rgb(0x9E9E9E), rgb(0x2196F3)),
        (rgb(0xE6E0E9), rgb(0x6750A4)),
    ];
    let mut host = host(1.0, switches);
    for (i, (off, on)) in expected.iter().enumerate() {
        let r = host.rect(host.ui.state.ids[i * 2]);
        let pixel = host.pixel(r.right as i32 - 8, r.center_y() as i32);
        assert!(near(pixel, *off, 1), "style {i} off: {pixel:?}");
        let r = host.rect(host.ui.state.ids[i * 2 + 1]);
        let pixel = host.pixel(r.left as i32 + 8, r.center_y() as i32);
        assert!(near(pixel, *on, 1), "style {i} on: {pixel:?}");
    }
}

#[test]
fn a_tap_flips_the_switch_the_thumb_slides_and_on_toggled_runs() {
    let mut host = host(1.0, switches);
    let id = host.ui.state.ids[0];
    let thumb = tagged(&host.ui.tree, id, "Thumb");
    let r = host.rect(id);
    let (x, y) = (r.center_x(), r.center_y());
    host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
    assert!(host.ui.tree.find::<SkiaToggle>(id).unwrap().p.is_toggled);
    // The handler runs at the start of the next frame; the thumb starts moving on it.
    host.frame_after(16.0);
    assert_eq!(host.ui.state.toggled, vec![(0, true)]);
    // 100 of 200 ms, CubicOut: 18 x 0.875.
    host.frame_after(100.0);
    let tx = host.ui.tree.base(thumb).unwrap().p.translation_x;
    assert!((tx - 15.75).abs() < 0.01, "{tx}");
    host.settle();
    assert_eq!(host.ui.tree.base(thumb).unwrap().p.translation_x, 18.0);
    // Track crimson now.
    assert!(near(host.pixel(r.left as i32 + 8, r.center_y() as i32), CRIMSON, 1));

    // And back.
    host.tap(x, y);
    assert_eq!(host.ui.state.toggled, vec![(0, true), (0, false)]);
    assert_eq!(host.ui.tree.base(thumb).unwrap().p.translation_x, 0.0);
    assert_eq!(host.ui.tree.find::<SkiaSwitch>(id).unwrap().accessibility_is_pressed(), Some(false));
}

#[test]
fn a_change_from_code_runs_on_toggled_and_responds_to_gestures_off_ignores_taps() {
    let mut host = host(1.0, |app: &mut App| {
        let switch = SkiaSwitch::new()
            .is_animated(false)
            .observe(|me, app: &App| me.set_is_toggled(app.style == PrebuiltControlStyle::Windows))
            .on_toggled(|_me, app: &mut App, _cx, value| app.toggled.push((0, value)));
        let locked = SkiaSwitch::new().responds_to_gestures(false);
        app.ids = vec![switch.id(), locked.id()];
        SkiaLayout::column().padding(20).children((switch, locked))
    });
    host.ui.state.style = PrebuiltControlStyle::Windows;
    host.ui.state_changed();
    host.settle();
    assert_eq!(host.ui.state.toggled, vec![(0, true)]);
    let thumb = tagged(&host.ui.tree, host.ui.state.ids[0], "Thumb");
    // Not animated: at its end at once.
    assert_eq!(host.ui.tree.base(thumb).unwrap().p.translation_x, 18.0);

    let r = host.rect(host.ui.state.ids[1]);
    host.tap(r.center_x(), r.center_y());
    assert!(!host.ui.tree.find::<SkiaToggle>(host.ui.state.ids[1]).unwrap().p.is_toggled);
}

#[test]
fn a_style_change_rebuilds_the_content_and_keeps_the_app_sizes() {
    let mut host = host(1.0, |app: &mut App| {
        let styled = SkiaSwitch::new().is_toggled(true).observe(|me, app: &App| me.set_control_style(app.style));
        let sized = SkiaSwitch::new().width_request(60).observe(|me, app: &App| me.set_control_style(app.style));
        app.ids = vec![styled.id(), sized.id()];
        SkiaLayout::column().padding(20).children((styled, sized))
    });
    let (styled, sized) = (host.ui.state.ids[0], host.ui.state.ids[1]);
    assert_eq!(host.rect(styled).size(), Size::new(46.0, 28.0));
    assert_eq!(host.rect(sized).size(), Size::new(60.0, 28.0));
    for (style, size, travel) in [
        (PrebuiltControlStyle::Cupertino, (51.0, 31.0), 20.0),
        (PrebuiltControlStyle::Material3, (52.0, 32.0), 20.0),
        (PrebuiltControlStyle::Unset, (46.0, 28.0), 18.0),
    ] {
        host.ui.state.style = style;
        host.ui.state_changed();
        host.settle();
        assert_eq!(host.rect(styled).size(), Size::new(size.0, size.1), "{style:?}");
        assert_eq!(host.rect(sized).size(), Size::new(60.0, size.1), "{style:?}");
        // One frame and one thumb: the old content is gone.
        assert_eq!(host.ui.tree.children(styled).len(), 2);
        let thumb = tagged(&host.ui.tree, styled, "Thumb");
        assert_eq!(host.ui.tree.base(thumb).unwrap().p.translation_x, travel, "{style:?}");
    }
}

/// One checkbox per style, off and on.
fn checkboxes(app: &mut App) -> Build<SkiaLayout> {
    let mut children = Vec::new();
    for style in STYLES {
        for on in [false, true] {
            let checkbox = SkiaCheckbox::new().control_style(style).is_toggled(on);
            app.ids.push(checkbox.id());
            children.push(checkbox);
        }
    }
    SkiaLayout::column().padding(20).spacing(10).children(children)
}

#[test]
fn checkbox_sizes_frames_and_colors_follow_the_style() {
    // Side in points, frame color on (DrawnUI.React SetContentSize and StyleDefault).
    let expected = [(22.0, CRIMSON), (20.0, rgb(0x0078D7)), (22.0, rgb(0x007AFF)), (24.0, rgb(0x2196F3)), (18.0, rgb(0x6750A4))];
    let mut host = host(2.0, checkboxes);
    for (i, (side, accent)) in expected.iter().enumerate() {
        for on in [false, true] {
            let id = host.ui.state.ids[i * 2 + on as usize];
            let r = host.rect(id);
            assert_eq!(r.size(), Size::new(side * 2.0, side * 2.0), "style {i}");
            let tree = &host.ui.tree;
            let visible = |tag: &str| tree.base(tagged(tree, id, tag)).unwrap().p.is_visible;
            assert_eq!((visible("FrameOff"), visible("FrameOn"), visible("ViewCheckOn")), (!on, on, on), "style {i}");
        }
        let on = host.rect(host.ui.state.ids[i * 2 + 1]);
        if i == 0 {
            // The default look: an outline in the accent and an accent square 3 points inside.
            assert!(near(host.pixel(on.center_x() as i32, on.center_y() as i32), CRIMSON, 1));
            assert!(near(host.pixel(on.left as i32 + 3, on.center_y() as i32), PAGE, 1));
        } else {
            // A filled frame with a white check mark.
            assert!(near(host.pixel(on.left as i32 + 3, on.top as i32 + 12), *accent, 1), "style {i}");
            let white = (0..on.width() as i32)
                .flat_map(|x| (0..on.height() as i32).map(move |y| (x, y)))
                .filter(|(x, y)| near(host.pixel(on.left as i32 + x, on.top as i32 + y), Color::WHITE, 8))
                .count();
            assert!(white > 20, "style {i}: {white} white pixels");
        }
        // Off: the inside shows the page.
        let off = host.rect(host.ui.state.ids[i * 2]);
        assert!(near(host.pixel(off.center_x() as i32, off.center_y() as i32), PAGE, 1), "style {i}");
    }
}

#[test]
fn a_tap_toggles_the_checkbox() {
    let mut host = host(1.0, checkboxes);
    let id = host.ui.state.ids[0];
    let r = host.rect(id);
    host.tap(r.center_x(), r.center_y());
    assert!(host.ui.tree.find::<SkiaToggle>(id).unwrap().p.is_toggled);
    assert!(near(host.pixel(r.center_x() as i32, r.center_y() as i32), CRIMSON, 1));
}

#[test]
fn radio_geometry_follows_the_style() {
    let mut host = host(1.0, |app: &mut App| {
        // A group each: siblings are one group, the first on would turn the others off.
        let radios: Vec<_> =
            STYLES.iter().map(|s| SkiaRadioButton::new("One").control_style(*s).group_name(format!("{s:?}")).is_toggled(true)).collect();
        app.ids = radios.iter().map(|r| r.id()).collect();
        SkiaLayout::column().padding(20).spacing(10).children(radios)
    });
    for (i, id) in host.ui.state.ids.clone().into_iter().enumerate() {
        let tree = &host.ui.tree;
        let r = host.rect(id);
        // Minimum height 24; the box 18 (default look) or 20 points, centered; the caption after 26 or 28.
        let (side, gap) = if i == 0 { (18.0, 26.0) } else { (20.0, 28.0) };
        assert_eq!(r.height(), 24.0);
        let indicator = tree.base(tree.children(id)[0]).unwrap().rect;
        assert_eq!(indicator, Rect::from_xywh(r.left, r.top + (24.0 - side) / 2.0, side, side), "style {i}");
        let text = tree.base(tagged(tree, id, "Text")).unwrap().rect;
        assert_eq!(text.left, r.left + gap);
        assert_eq!(r.right, text.right);
        // The ring is the style's accent.
        let accent = [CRIMSON, rgb(0x0078D7), rgb(0x007AFF), rgb(0x2196F3), rgb(0x6750A4)][i];
        let pixel = host.pixel(indicator.left as i32 + 1, indicator.center_y() as i32);
        assert!(near(pixel, accent, 40), "style {i}: {pixel:?}");
    }
}

#[test]
fn radio_groups_by_name_and_by_parent() {
    let mut host = host(1.0, |app: &mut App| {
        let named = |text: &str, i: usize, on: bool| {
            SkiaRadioButton::new(text)
                .group_name("g")
                .is_toggled(on)
                .on_toggled(move |_me, app: &mut App, _cx, value| app.toggled.push((i, value)))
        };
        let (a, b, c) = (named("A", 0, true), named("B", 1, false), named("C", 2, false));
        let (d, e) = (SkiaRadioButton::new("D").is_toggled(true), SkiaRadioButton::new("E"));
        app.ids = vec![a.id(), b.id(), c.id(), d.id(), e.id()];
        SkiaLayout::column().padding(20).children((
            SkiaLayout::row().children((a, b)),
            // Another parent, the same group.
            SkiaLayout::row().children(c),
            // No name: the siblings are the group.
            SkiaLayout::row().children((d, e)),
        ))
    });
    let on = |host: &Headless<App>| -> Vec<bool> {
        host.ui.state.ids.iter().map(|id| host.ui.tree.find::<SkiaToggle>(*id).unwrap().p.is_toggled).collect()
    };
    assert_eq!(on(&host), [true, false, false, true, false]);
    let tap = |host: &mut Headless<App>, i: usize| {
        let r = host.rect(host.ui.state.ids[i]);
        host.tap(r.left + 5.0, r.center_y());
    };
    tap(&mut host, 2);
    assert_eq!(on(&host), [false, false, true, true, false]);
    assert_eq!(host.ui.state.toggled, vec![(2, true), (0, false)]);
    // A tap on the one that is on changes nothing.
    tap(&mut host, 2);
    assert_eq!(on(&host), [false, false, true, true, false]);
    assert_eq!(host.ui.state.toggled.len(), 2);
    tap(&mut host, 4);
    assert_eq!(on(&host), [false, false, true, false, true]);
    // The indicator shows the state.
    let e = host.ui.state.ids[4];
    assert!(host.ui.tree.base(tagged(&host.ui.tree, e, "On")).unwrap().p.is_visible);
    assert!(!host.ui.tree.base(tagged(&host.ui.tree, host.ui.state.ids[3], "On")).unwrap().p.is_visible);
}


/// DrawnUI DefaultValue: sets the state without Toggled, at build and when it changes.
#[test]
fn default_value_sets_the_state_without_on_toggled() {
    let mut host = host(1.0, |app: &mut App| {
        let switch = SkiaSwitch::new()
            .default_value(true)
            .observe(|me, app: &App| me.set_default_value(app.style != PrebuiltControlStyle::Windows))
            .on_toggled(|_me, app: &mut App, _cx, value| app.toggled.push((0, value)));
        let radio = |on: bool| SkiaRadioButton::new("R").group_name("d").default_value(on);
        let (a, b) = (radio(true), radio(false));
        app.ids = vec![switch.id(), a.id(), b.id()];
        SkiaLayout::column().padding(20).children((switch, a, b))
    });
    let on = |host: &Headless<App>, i: usize| host.ui.tree.find::<SkiaToggle>(host.ui.state.ids[i]).unwrap().p.is_toggled;
    assert!(on(&host, 0) && on(&host, 1) && !on(&host, 2));

    host.ui.state.style = PrebuiltControlStyle::Windows;
    host.ui.state_changed();
    host.settle();
    assert!(!on(&host, 0), "a new default value is the state");
    assert!(host.ui.state.toggled.is_empty(), "{:?}", host.ui.state.toggled);

    // A radio given an "on" default turns the others of its group off.
    let b = host.ui.state.ids[2];
    host.ui.tree.find_mut::<SkiaRadioButton>(b).unwrap().set_default_value(true);
    host.settle();
    assert!(!on(&host, 1) && on(&host, 2));

    // Taps still run on_toggled.
    let r = host.rect(host.ui.state.ids[0]);
    host.tap(r.center_x(), r.center_y());
    host.settle();
    assert_eq!(host.ui.state.toggled, vec![(0, true)]);
}
