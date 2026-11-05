//! What tools read from a running app (DrawnUI Designer): a control's type name, where it shows on
//! the canvas (through scroll offsets and transforms), the control seen at a point (z_index,
//! transforms, clipping; input switched off or not), and with feature `designer` the properties by
//! name.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    back: Handle<SkiaShape>,
    front: Handle<SkiaShape>,
    turned: Handle<SkiaShape>,
    label: Handle<SkiaLabel>,
    clipped: Handle<SkiaShape>,
}

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((
            // Two boxes over each other: the one with the higher z_index is on top though it comes first.
            SkiaShape::new().width_request(100).height_request(100).margin((10, 10, 0, 0)).z_index(2).assign(&mut app.front),
            SkiaShape::new().width_request(100).height_request(100).margin((10, 10, 0, 0)).input_transparent(true).assign(&mut app.back),
            // Turned by 90 degrees and twice as large around its center: 40 x 20 shows as 40 x 80.
            SkiaShape::new().width_request(80).height_request(40).margin((200, 50, 0, 0)).rotation(90).scale(2.0).assign(&mut app.turned),
            SkiaLabel::new("Label").margin((10, 150, 0, 0)).assign(&mut app.label),
            // A child past the edge of a clipping parent shows only inside it.
            SkiaLayout::new().width_request(50).height_request(50).margin((300, 150, 0, 0)).is_clipped_to_bounds(true).children((SkiaShape::new()
                .width_request(40)
                .height_request(40)
                .margin((30, 0, 0, 0))
                .assign(&mut app.clipped),)),
        ))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    host
}

#[test]
fn type_names() {
    let host = host();
    let state = &host.ui.state;
    assert_eq!(host.ui.tree.type_name(state.label), Some("SkiaLabel"));
    assert_eq!(host.ui.tree.type_name(state.front), Some("SkiaShape"));
    assert_eq!(host.ui.tree.type_name(host.ui.tree.root().unwrap()), Some("SkiaLayout"));
}

#[test]
fn screen_rect_goes_through_the_transforms() {
    let host = host();
    let state = &host.ui.state;
    assert_eq!(host.ui.tree.screen_rect(state.front), Rect::from_xywh(10.0, 10.0, 100.0, 100.0));
    // 80 x 40 at (200, 50), center (240, 70): turned and doubled, 80 wide and 160 tall around it.
    let r = host.ui.tree.screen_rect(state.turned);
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(near(r.left, 200.0) && near(r.right, 280.0) && near(r.top, -10.0) && near(r.bottom, 150.0), "{r:?}");
}

#[test]
fn hit_test_finds_what_is_seen() {
    let host = host();
    let state = &host.ui.state;
    let at = |x: f32, y: f32| host.ui.hit_test(Point::new(x, y));
    // z_index 2 is on top; the box behind it is input-transparent but still what a tool would see
    // where nothing covers it (here it is fully covered).
    assert_eq!(at(50.0, 50.0), Some(state.front.id()));
    // The turned box shows above its layout rect (y < 50) and not right of it (x > 280).
    assert_eq!(at(240.0, 0.0), Some(state.turned.id()));
    assert_ne!(at(285.0, 70.0), Some(state.turned.id()));
    // The clipped child shows inside its parent (x < 350) only.
    assert_eq!(at(340.0, 170.0), Some(state.clipped.id()));
    assert_ne!(at(360.0, 170.0), Some(state.clipped.id()));
    // Nothing but the root under an empty spot.
    assert_eq!(at(390.0, 290.0), host.ui.tree.root());
}

#[cfg(feature = "designer")]
#[test]
fn label_properties_by_name() {
    let mut host = host();
    let label = host.ui.state.label;
    let props = host.ui.tree.props(label);
    let get = |name: &str| props.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no {name}")).clone();
    assert_eq!(get("text").value, "Label");
    assert!(get("text").is_set && !get("font_size").is_set);
    assert_eq!(get("horizontal_options").value, "Start");
    assert_eq!(get("margin").value, "10,150,0,0");
    assert!(!get("stroke_gradient").settable);

    let tree = &mut host.ui.tree;
    tree.set_prop(label, "text", "Changed").unwrap();
    tree.set_prop(label, "font_size", "30").unwrap();
    tree.set_prop(label, "text_color", "#FF336699").unwrap();
    tree.set_prop(label, "horizontal_options", "center").unwrap();
    tree.set_prop(label, "padding", "4,2").unwrap();
    assert!(tree.set_prop(label, "font_size", "big").is_err());
    assert!(tree.set_prop(label, "no_such", "1").is_err());
    let props = tree.props(label);
    let get = |name: &str| props.iter().find(|p| p.name == name).unwrap().value.clone();
    assert_eq!((get("text"), get("font_size"), get("text_color")), ("Changed".into(), "30".into(), "#FF336699".into()));
    assert_eq!((get("horizontal_options"), get("padding")), ("Center".into(), "4,2,4,2".into()));

    // The change is laid out: the label grew and is centered.
    let before = host.ui.tree.base(label).unwrap().rect;
    host.settle();
    let after = host.ui.tree.base(label).unwrap().rect;
    assert!(after.height() > before.height() && after.center_x() > before.center_x(), "{before:?} -> {after:?}");
}

#[test]
fn an_id_goes_over_the_wire_and_a_stale_one_finds_nothing() {
    let host = host();
    let id: ControlId = host.ui.state.label.into();
    assert_eq!(ControlId::from_bits(id.to_bits()), id);
    assert!(host.ui.tree.base(ControlId::from_bits(id.to_bits())).is_some());
    // The same slot in a later generation, and a slot that was never used.
    assert!(host.ui.tree.base(ControlId::from_bits(id.to_bits() + (1 << 32))).is_none());
    assert!(host.ui.tree.base(ControlId::from_bits(u64::MAX)).is_none());
}

#[cfg(feature = "designer")]
#[test]
fn drawn_rect_is_where_the_last_frame_drew_it() {
    let host = host();
    let state = &host.ui.state;
    assert_eq!(host.ui.tree.drawn_rect(state.front), Some(Rect::from_xywh(10.0, 10.0, 100.0, 100.0)));
    let (drawn, screen) = (host.ui.tree.drawn_rect(state.turned).unwrap(), host.ui.tree.screen_rect(state.turned));
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(near(drawn.left, screen.left) && near(drawn.top, screen.top) && near(drawn.right, screen.right) && near(drawn.bottom, screen.bottom), "{drawn:?}");
    assert_eq!(host.ui.tree.drawn_rect(ControlId::from_bits(u64::MAX)), None);
}

#[cfg(feature = "designer")]
#[test]
fn hit_test_visual_looks_through_an_empty_layer_on_top() {
    #[derive(Default)]
    struct Page {
        shape: Handle<SkiaShape>,
        layer: Handle<SkiaLayout>,
    }
    let ui = Ui::new(Page::default(), |page: &mut Page| {
        SkiaLayout::new().fill().children((
            SkiaShape::new().width_request(100).height_request(100).margin((10, 10, 0, 0)).background_color(Color::BLUE).assign(&mut page.shape),
            // An overlay that fills the canvas and draws nothing (a shell's toast layer).
            SkiaLayer::new().fill().assign(&mut page.layer),
        ))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    let at = Point::new(50.0, 50.0);
    assert_eq!(host.ui.hit_test(at), Some(host.ui.state.layer.into()), "hit_test: what is on top");
    assert_eq!(host.ui.hit_test_visual(at), Some(host.ui.state.shape.into()), "the shape a person sees");
    // Where only empty layouts are, nothing is seen.
    assert_eq!(host.ui.hit_test_visual(Point::new(300.0, 250.0)), None);
}

#[cfg(feature = "designer")]
#[test]
fn after_frame_draws_over_the_frame_reads_it_and_applies_edits() {
    use std::sync::mpsc;
    use drawnui::App as _;
    #[derive(Default)]
    struct Page {
        label: Handle<SkiaLabel>,
    }
    // Edits a tool queued from another thread, and what it saw.
    let (edits, queued) = mpsc::channel::<(u64, &'static str, &'static str)>();
    let (seen_tx, seen) = mpsc::channel::<(Color, Color)>();
    let ui = Ui::new(Page::default(), |page: &mut Page| SkiaLayout::new().fill().children((SkiaLabel::new("Before").assign(&mut page.label),)))
        .font_bytes("Default", FONT)
        .background(Color::WHITE)
        .on_after_frame(move |ui, frame| {
            while let Ok((id, name, value)) = queued.try_recv() {
                ui.tree.set_prop(ControlId::from_bits(id), name, value).unwrap();
            }
            let canvas = frame.surface.canvas();
            canvas.draw_rect(Rect::from_xywh(300.0, 200.0, 20.0, 20.0), &drawnui::skia::Paint::new(drawnui::skia::Color4f::from(Color::RED), None));
            let image = frame.surface.image_snapshot();
            let mut px = [0u8; 4];
            let info = drawnui::skia::ImageInfo::new((1, 1), drawnui::skia::ColorType::RGBA8888, drawnui::skia::AlphaType::Unpremul, None);
            let at = |px: &mut [u8; 4], x, y| {
                image.read_pixels(&info, px, 4, (x, y), drawnui::skia::image::CachingHint::Disallow);
                Color::from_rgb(px[0], px[1], px[2])
            };
            let _ = seen_tx.send((at(&mut px, 310, 210), at(&mut px, 390, 290)));
        });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    let mut surface = drawnui::skia::surfaces::raster_n32_premul((400, 300)).unwrap();
    let (mut gpu, mut app_host) = (drawnui::Gpu::raster(), drawnui::Host::default());
    let mut frame = |ui: &mut Ui<Page>, surface: &mut drawnui::skia::Surface, time_ms: f64| {
        let mut f = drawnui::Frame { surface, gpu: &mut gpu, width: 400.0, height: 300.0, scale: 1.0, time_ms, host: &mut app_host };
        ui.frame(&mut f)
    };
    frame(&mut host.ui, &mut surface, 1000.0);
    assert_eq!(seen.try_recv().unwrap(), (Color::RED, Color::WHITE), "drawn over the app, read back");
    let label = ControlId::from(host.ui.state.label).to_bits();
    edits.send((label, "text", "After")).unwrap();
    let more = frame(&mut host.ui, &mut surface, 1016.0);
    assert!(more, "the edit asks for the next frame");
    frame(&mut host.ui, &mut surface, 1032.0);
    assert_eq!(host.ui.tree.find::<SkiaLabel>(host.ui.state.label).unwrap().p.text, "After");
}

#[cfg(feature = "designer")]
#[test]
fn a_choice_lists_its_cases() {
    let host = host();
    let props = host.ui.tree.props(host.ui.state.label);
    let find = |name: &str| props.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("{name}"));
    assert!(find("horizontal_options").choices.is_some_and(|c| c.contains(&"Center") && c.contains(&"Fill")));
    assert!(find("use_cache").choices.is_some_and(|c| c.contains(&"ImageDoubleBuffered")));
    assert_eq!(find("opacity").choices, None);
    assert_eq!(find("text").choices, None);
}

#[cfg(feature = "designer")]
#[test]
fn the_engines_own_controls_are_internal_the_apps_are_not() {
    #[derive(Default)]
    struct Page {
        button: Handle<SkiaButton>,
        switch: Handle<SkiaSwitch>,
        check: Handle<SkiaCheckbox>,
        content: Handle<SkiaShape>,
        scroll: Handle<SkiaScroll>,
        page_label: Handle<SkiaLabel>,
        shell: Handle<SkiaShell>,
    }
    let ui = Ui::new(Page::default(), |page: &mut Page| {
        SkiaShell::new().assign(&mut page.shell).route("detail", |page: &mut Page, _args: &ShellArguments| {
            SkiaLayout::new().fill().children((SkiaLabel::new("Detail").assign(&mut page.page_label),))
        }).root(
            SkiaLayout::column().fill().children((
                SkiaButton::new("Go").assign(&mut page.button),
                SkiaSwitch::new().assign(&mut page.switch),
                SkiaCheckbox::new().is_toggled(true).assign(&mut page.check),
                SkiaScroll::new().height_request(100).scroll_bars_visibility(ScrollBarVisibility::Vertical).assign(&mut page.scroll).content(SkiaShape::new().height_request(400).assign(&mut page.content)),
            )),
        )
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 400, 600, 1.0);
    host.settle();
    let tree = &host.ui.tree;
    let state = &host.ui.state;
    fn descendants(tree: &drawnui::Tree, id: ControlId) -> Vec<ControlId> {
        let mut out = Vec::new();
        let mut stack = tree.children(id).to_vec();
        while let Some(c) = stack.pop() {
            out.push(c);
            stack.extend_from_slice(tree.children(c));
        }
        out
    }
    // The app's controls.
    for id in [ControlId::from(state.button), state.switch.into(), state.check.into(), state.scroll.into(), state.content.into(), state.shell.into()] {
        assert!(!tree.is_internal(id), "{:?}", tree.type_name(id));
    }
    // A button's caption, a toggle's look all the way down, a scroll's bar.
    assert!(descendants(tree, state.button.into()).iter().all(|c| tree.is_internal(*c)));
    assert!(descendants(tree, state.switch.into()).iter().all(|c| tree.is_internal(*c)));
    assert!(!descendants(tree, state.check.into()).is_empty() && descendants(tree, state.check.into()).iter().all(|c| tree.is_internal(*c)));
    let bars: Vec<ControlId> = tree.children(state.scroll).iter().copied().filter(|c| tree.type_name(*c) == Some("SkiaScrollBar")).collect();
    assert!(!bars.is_empty() && bars.iter().all(|b| tree.is_internal(*b)));
    // A pushed page: the shell's layers and bar are internal, the page's content is not.
    let shell = state.shell;
    host.ui.tree.cx().go_to(shell, "detail", false);
    host.settle();
    let tree = &host.ui.tree;
    let label: ControlId = host.ui.state.page_label.into();
    assert!(!tree.is_internal(label), "the app's page");
    let mut above = Vec::new();
    let mut at = tree.parent(label);
    while let Some(p) = at.filter(|p| *p != shell.into()) {
        above.push(p);
        at = tree.parent(p);
    }
    // The app's page root, then the shell's wrappers up to the shell.
    assert!(!tree.is_internal(above[0]));
    assert!(above[1..].iter().all(|p| tree.is_internal(*p)), "{:?}", above.iter().map(|p| tree.type_name(*p)).collect::<Vec<_>>());
    let title = descendants(tree, shell.into()).into_iter().find(|c| tree.find::<SkiaLabel>(*c).is_some_and(|l| l.p.text == "detail"));
    if let Some(title) = title {
        assert!(tree.is_internal(title), "the nav bar title");
    }
    assert!(!tree.is_internal(ControlId::from_bits(u64::MAX)));
}

#[cfg(feature = "designer")]
#[test]
fn popups_are_listed_and_pictured_without_touching_their_windows() {
    #[derive(Default)]
    struct Page {}
    let ui = Ui::new(Page::default(), |_| SkiaLayout::new().fill()).background(Color::WHITE);
    let mut host = Headless::new(ui, 400, 300, 2.0);
    host.settle();
    host.popup_windows(true);
    let menu = SkiaShape::new().width_request(60).height_request(30).background_color(Color::RED);
    let id = host.ui.tree.cx().open_popup_window(menu, Point::new(10.0, 10.0), PopupWindowOptions::default());
    host.settle();
    let roots = host.ui.tree.popup_roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].0, id);
    // Its window has not been painted yet: the picture does not mark it painted.
    let request = host.popup_window_requests()[0];
    host.place_popup_window(request.id, 120.0, 60.0, 2.0, Point::new(0.0, 0.0));
    host.settle();
    let needs = host.ui.popup_needs_paint(request.id);
    let mut gpu = drawnui::Gpu::raster();
    let image = host.ui.snapshot_popup(id, &mut gpu).expect("a picture");
    assert_eq!((image.width(), image.height()), (120, 60));
    assert_eq!(host.ui.popup_needs_paint(request.id), needs);
    let info = drawnui::skia::ImageInfo::new((1, 1), drawnui::skia::ColorType::RGBA8888, drawnui::skia::AlphaType::Unpremul, None);
    let mut px = [0u8; 4];
    image.read_pixels(&info, &mut px, 4, (60, 30), drawnui::skia::image::CachingHint::Disallow);
    assert_eq!(Color::from_rgb(px[0], px[1], px[2]), Color::RED);
    host.ui.tree.cx().close_popup_window(id);
    host.settle();
    assert!(host.ui.tree.popup_roots().is_empty());
    assert!(host.ui.snapshot_popup(id, &mut gpu).is_none());
}

#[cfg(feature = "designer")]
#[test]
fn a_hidden_frame_is_drawn_off_screen_on_request() {
    use drawnui::App as _;
    let ui = Ui::new(App::default(), |app: &mut App| SkiaLayout::new().fill().children((SkiaShape::new().fill().background_color(Color::BLUE).assign(&mut app.front),)));
    let mut host = Headless::new(ui, 200, 100, 1.0);
    let asked = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let count = asked.clone();
    let mut requests = drawnui::Host::default();
    requests.on_frame_requested(move || {
        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    host.ui.init(&mut requests);
    host.settle();
    assert!(!host.ui.take_hidden_frame_request());
    let waker = host.ui.waker();
    std::thread::spawn(move || waker.request_hidden_frame()).join().unwrap();
    assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 1, "the host is asked for a frame");
    assert!(host.ui.take_hidden_frame_request());
    assert!(!host.ui.take_hidden_frame_request(), "answered once");
    // The frame off screen, at the size the window had.
    let mut gpu = drawnui::Gpu::raster();
    let image = host.ui.render_offscreen(&mut gpu, 300, 150, 1.0, 1000.0).expect("a picture");
    assert_eq!((image.width(), image.height()), (300, 150));
    let info = drawnui::skia::ImageInfo::new((1, 1), drawnui::skia::ColorType::RGBA8888, drawnui::skia::AlphaType::Unpremul, None);
    let mut px = [0u8; 4];
    image.read_pixels(&info, &mut px, 4, (290, 140), drawnui::skia::image::CachingHint::Disallow);
    assert_eq!(Color::from_rgb(px[0], px[1], px[2]), Color::BLUE);
}

#[cfg(feature = "designer")]
#[test]
fn popup_rect_is_where_the_popup_shows_on_the_canvas() {
    #[derive(Default)]
    struct Page {}
    let ui = Ui::new(Page::default(), |_| SkiaLayout::new().fill()).background(Color::WHITE);
    let mut host = Headless::new(ui, 400, 300, 2.0);
    host.settle();
    let menu = || SkiaShape::new().width_request(60).height_request(30).background_color(Color::RED);
    // On the canvas: at the point, 60 x 30 points at scale 2.
    let id = host.ui.tree.cx().open_popup_window(menu(), Point::new(10.0, 10.0), PopupWindowOptions::default());
    host.settle();
    assert_eq!(host.ui.tree.popup_rect(id), Some(Rect::from_xywh(20.0, 20.0, 120.0, 60.0)));
    host.ui.tree.cx().close_popup_window(id);
    host.settle();
    assert_eq!(host.ui.tree.popup_rect(id), None);
    // In a window of its own on a screen of scale 1.5, 1000 px right of the canvas: in the
    // canvas's pixels (scale 2), 90 x 45 window pixels are 120 x 60.
    host.popup_windows(true);
    let id = host.ui.tree.cx().open_popup_window(menu(), Point::new(10.0, 10.0), PopupWindowOptions::default());
    host.settle();
    let request = host.popup_window_requests()[0];
    host.place_popup_window(request.id, 90.0, 45.0, 1.5, Point::new(1000.0, 60.0));
    host.settle();
    assert_eq!(host.ui.tree.popup_rect(id), Some(Rect::from_xywh(1000.0, 60.0, 120.0, 60.0)));
}

#[cfg(feature = "designer")]
#[test]
fn an_optional_choice_takes_its_own_none_and_empty_clears_it() {
    use drawnui::designer::PropText;
    // A choice with a None case of its own.
    assert_eq!(<Option<CacheType> as PropText>::from_text("none"), Some(Some(CacheType::None)));
    assert_eq!(<Option<CacheType> as PropText>::from_text("Image"), Some(Some(CacheType::Image)));
    assert_eq!(<Option<CacheType> as PropText>::from_text(""), Some(None));
    assert_eq!(<Option<CacheType> as PropText>::to_text(&None), "");
    assert_eq!(<Option<CacheType> as PropText>::to_text(&Some(CacheType::None)), "None");
    // A type without one: `none` is nothing, as before.
    assert_eq!(<Option<f32> as PropText>::from_text("none"), Some(None));
    assert_eq!(<Option<f32> as PropText>::from_text(" "), Some(None));
    assert_eq!(<Option<f32> as PropText>::from_text("1.5"), Some(Some(1.5)));
    assert_eq!(<Option<f32> as PropText>::to_text(&None), "none");
    assert_eq!(<Option<f32> as PropText>::from_text("x"), None);
}

#[cfg(feature = "designer")]
#[test]
fn a_control_is_made_by_its_type_name() {
    let types = drawnui::designer::control_types();
    assert!(types.contains(&"SkiaButton") && types.contains(&"SkiaLabel") && types.contains(&"SkiaShell"));
    assert!(!types.contains(&"SkiaToggle"), "a base with no control of its own is not offered");
    for name in types {
        assert!(drawnui::designer::create(name).is_some(), "{name}");
    }
    assert!(drawnui::designer::create("NoSuchControl").is_none());
    // A made button goes into a running tree and reads as a button.
    let mut host = Headless::new(Ui::new((), |_| SkiaLayout::column().fill()), 300, 200, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    let root = host.ui.tree.root().expect("a root");
    let button = host.ui.tree.cx().add_child(root, drawnui::designer::create("SkiaButton").unwrap());
    host.frame_after(drawnui::ACCESSIBILITY_INTERVAL_MS);
    assert_eq!(host.ui.tree.type_name(button), Some("SkiaButton"));
    assert!(host.rect(button).width() > 0.0, "laid out and drawn");
    assert!(host.ui.tree.find::<SkiaButton>(button).is_some_and(|b| b.p.text == "Button"));
}

#[cfg(feature = "designer")]
#[derive(Default)]
struct Moving {
    left: Handle<SkiaLayout>,
    right: Handle<SkiaLayout>,
    card: Handle<SkiaShape>,
    switch: Handle<SkiaSwitch>,
    taps: u32,
}

#[cfg(feature = "designer")]
#[test]
fn move_to_keeps_the_control_its_handlers_and_its_state() {
    let ui = Ui::new(Moving::default(), |app: &mut Moving| {
        SkiaLayout::row().fill().children((
            SkiaLayout::column().width_request(150).assign(&mut app.left).children((
                SkiaShape::new()
                    .width_request(100)
                    .height_request(40)
                    .background_color(Color::RED)
                    .assign(&mut app.card)
                    .on_tapped(|_me, app: &mut Moving, _cx| app.taps += 1)
                    .children((SkiaSwitch::new().assign(&mut app.switch),)),
            )),
            SkiaLayout::column().width_request(150).assign(&mut app.right).children((SkiaShape::new().width_request(100).height_request(40),)),
        ))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.settle();
    let (left, right, card, switch) = (host.ui.state.left, host.ui.state.right, host.ui.state.card, host.ui.state.switch);
    if let Some(mut s) = host.ui.tree.cx().get_mut(switch) {
        s.set_is_toggled(true);
    }
    host.settle();
    // Into the right column, first among its children.
    assert!(host.ui.tree.cx().move_to(card, right, 0));
    host.settle();
    let card_id: ControlId = card.into();
    assert_eq!(host.ui.tree.parent(card_id), Some(right.into()), "same id, new parent");
    assert_eq!(host.ui.tree.children(right)[0], card_id);
    assert!(host.ui.tree.children(left).is_empty());
    let r = host.rect(card);
    assert!(r.left >= 150.0, "drawn in the right column: {r:?}");
    // On the card, beside the switch at its start.
    host.tap(r.right - 5.0, r.center_y());
    assert_eq!(host.ui.state.taps, 1, "its tap handler still runs");
    assert!(host.ui.tree.find::<SkiaToggle>(switch).is_some_and(|t| t.p.is_toggled), "its state stays");
    // Not into itself or below itself, not the root.
    assert!(!host.ui.tree.cx().move_to(right, card, 0));
    assert!(!host.ui.tree.cx().move_to(card, switch, 0));
    let root = host.ui.tree.root().unwrap();
    assert!(!host.ui.tree.cx().move_to(root, left, 0));
}

#[cfg(feature = "designer")]
#[test]
fn a_templated_layout_is_told_apart() {
    #[derive(Default)]
    struct Lists {
        templated: Handle<SkiaLayout>,
        plain: Handle<SkiaLayout>,
    }
    let ui = Ui::new(Lists::default(), |app: &mut Lists| {
        SkiaLayout::column().fill().children((
            SkiaLayout::column().assign(&mut app.templated).items(|_: &Lists| 3, || (SkiaLabel::new("row"), ()), |_: &(), _: &Lists, _, _| {}),
            SkiaLayout::column().assign(&mut app.plain).children((SkiaLabel::new("a"),)),
        ))
    });
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    assert!(host.ui.tree.is_templated(host.ui.state.templated));
    assert!(!host.ui.tree.is_templated(host.ui.state.plain));
}
