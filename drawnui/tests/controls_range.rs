//! SkiaProgress and SkiaSlider in the five looks, with the geometry and colors of DrawnUI.React
//! (SkiaProgress.ts, SkiaSlider.ts): track heights, thumb positions, drag, a press on the trail,
//! range mode, the value handlers. Then upstream SliderInScrollDragTests.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::ControlId;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const PAGE: Color = Color::from_rgb(0xF5, 0xF5, 0xF5);
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
    /// (index, start or end, value) per handler call.
    changed: Vec<(usize, &'static str, f32)>,
    scroll: Handle<SkiaScroll>,
    sharpness: Handle<SkiaSlider>,
}

fn host<B: Into<drawnui::Detached>>(scale: f32, build: impl FnOnce(&mut App) -> B) -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(PAGE);
    let mut host = Headless::new(ui, (300.0 * scale) as i32, (560.0 * scale) as i32, scale);
    host.settle();
    host
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b())
}

fn rgb(v: u32) -> Color {
    Color::new(0xFF00_0000 | v)
}

fn slider(host: &Headless<App>, i: usize) -> &SkiaSlider {
    host.ui.tree.find::<SkiaSlider>(host.ui.state.ids[i]).unwrap()
}

#[test]
fn progress_tracks_and_colors_follow_the_style() {
    // Track height (points), track and progress colors (C# ResolvedTrackColor / ProgressColor).
    let expected = [
        (8.0, rgb(0xD7DBE0), rgb(0xDC143C)),
        (6.0, rgb(0xF3F2F1), rgb(0x0078D4)),
        (4.0, rgb(0xE5E5EA), rgb(0x007AFF)),
        (4.0, rgb(0xE8EAED), rgb(0x2196F3)),
        (4.0, rgb(0xE6E0E9), rgb(0x6750A4)),
    ];
    for scale in [1.0f32, 2.0] {
        let mut host = host(scale, |app: &mut App| {
            let bars: Vec<_> = STYLES.iter().map(|s| SkiaProgress::new().control_style(*s).value(65).width_request(200)).collect();
            app.ids = bars.iter().map(|b| b.id()).collect();
            SkiaLayout::column().padding(20).spacing(20).children(bars)
        });
        for (i, (height, track, progress)) in expected.iter().enumerate() {
            let r = host.rect(host.ui.state.ids[i]);
            assert_eq!(r.size(), Size::new(200.0 * scale, height * scale), "style {i}");
            let y = r.center_y() as i32;
            // 65 of 100: the trail ends at 130 points.
            assert!(near(host.pixel((r.left + 10.0 * scale) as i32, y), *progress, 1), "style {i}");
            assert!(near(host.pixel((r.left + 120.0 * scale) as i32, y), *progress, 1), "style {i}");
            assert!(near(host.pixel((r.left + 180.0 * scale) as i32, y), *track, 1), "style {i}");
            if i == 4 {
                // Material 3: a 4 point gap after the trail, a stop dot at the end.
                assert!(near(host.pixel((r.left + 132.0 * scale) as i32, y), PAGE, 1));
                assert!(near(host.pixel((r.left + 136.0 * scale) as i32, y), *track, 1));
                assert!(near(host.pixel((r.right - 2.0 * scale) as i32, y), *progress, 60));
            } else {
                assert!(near(host.pixel((r.left + 132.0 * scale) as i32, y), *track, 1), "style {i}");
            }
        }
    }
}

#[test]
fn progress_value_changes_repaint_and_clamp() {
    let mut host = host(1.0, |app: &mut App| {
        let bar = SkiaProgress::new().value(0).observe(|me, app: &App| me.set_value(app.changed.len() as f32 * 150.0));
        app.ids = vec![bar.id()];
        SkiaLayout::column().padding(20).children(bar)
    });
    let r = host.rect(host.ui.state.ids[0]);
    // A column fills the width: 260 points; nothing done yet.
    assert_eq!(r.width(), 260.0);
    assert!(near(host.pixel(r.left as i32 + 5, r.center_y() as i32), rgb(0xD7DBE0), 1));
    host.ui.state.changed.push((0, "", 0.0));
    host.ui.state_changed();
    host.settle();
    // 150 of 100: full.
    assert!(near(host.pixel(r.right as i32 - 5, r.center_y() as i32), rgb(0xDC143C), 1));
}

/// One slider per style at width 200, single then range.
fn sliders(app: &mut App) -> Build<SkiaLayout> {
    let mut children = Vec::new();
    for range in [false, true] {
        for style in STYLES {
            let i = children.len();
            let slider = SkiaSlider::new().control_style(style).width_request(200).horizontal_options(LayoutOptions::Start);
            let slider = if range { slider.enable_range(true).start(20).end(80) } else { slider.end(65) };
            let slider = slider
                .on_start_changed(move |_me, app: &mut App, _cx, v| app.changed.push((i, "start", v)))
                .on_end_changed(move |_me, app: &mut App, _cx, v| app.changed.push((i, "end", v)));
            app.ids.push(slider.id());
            children.push(slider);
        }
    }
    SkiaLayout::column().padding(20).spacing(20).children(children)
}

#[test]
fn slider_heights_and_thumbs_follow_the_style() {
    // Height = thumb box (points); range mode adds 8 except in the default look.
    let heights = [35.0, 20.0, 28.0, 20.0, 20.0];
    for scale in [1.0f32, 2.0] {
        let host = host(scale, sliders);
        for (i, h) in heights.iter().enumerate() {
            for range in [false, true] {
                let index = i + range as usize * 5;
                let h = if range && i != 0 { h + 8.0 } else { *h };
                let r = host.rect(host.ui.state.ids[index]);
                assert_eq!(r.size(), Size::new(200.0 * scale, h * scale), "style {i} range {range}");
                // The thumb travels over the width minus its box: value / 100 of that.
                let s = slider(&host, index);
                let travel = 200.0 - h;
                let (start, end) = if range { (0.2 * travel, 0.8 * travel) } else { (0.0, 0.65 * travel) };
                assert!((s.end_thumb_x - end).abs() < 1e-3, "style {i}: {} vs {end}", s.end_thumb_x);
                assert!((s.start_thumb_x - start).abs() < 1e-3, "style {i}: {} vs {start}", s.start_thumb_x);
            }
        }
        // Built with the values: nothing was reported.
        assert!(host.ui.state.changed.is_empty());
    }
}

#[test]
fn slider_track_trail_and_thumb_colors_follow_the_style() {
    // Track, selected trail, thumb (C# style builders).
    let expected = [
        (rgb(0xD7DBE0), rgb(0xDC143C), rgb(0xDC143C)),
        (rgb(0xC6C6C6), rgb(0x0078D4), rgb(0x0078D4)),
        (rgb(0xCCCCCC), rgb(0x007AFF), Color::WHITE),
        (rgb(0xE8EAED), rgb(0x2196F3), rgb(0x2196F3)),
        (rgb(0xE6E0E9), rgb(0x6750A4), rgb(0x6750A4)),
    ];
    let mut host = host(2.0, sliders);
    for (i, (track, selected, thumb)) in expected.iter().enumerate() {
        let r = host.rect(host.ui.state.ids[i]);
        let s = slider(&host, i);
        let (end, h) = (s.end_thumb_x, s.slider_height());
        let y = r.center_y() as i32;
        assert!(near(host.pixel((r.right - 4.0) as i32, y), *track, 1), "style {i}");
        assert!(near(host.pixel((r.left + 4.0) as i32, y), *selected, 1), "style {i}");
        // Middle of the thumb: the default look has a white dot there, Windows an accent dot.
        let center = (r.left + (end + h / 2.0) * 2.0) as i32;
        let dot = if i == 0 { Color::WHITE } else { *thumb };
        assert!(near(host.pixel(center, y), dot, 2), "style {i}: {:?}", host.pixel(center, y));
        if i == 0 {
            // The accent circle around it: 25 points across.
            assert!(near(host.pixel(center + 16, y), *thumb, 2), "{:?}", host.pixel(center + 16, y));
        }
        if i == 1 {
            // Windows: a white ring around the dot.
            assert!(near(host.pixel(center + 14, y), Color::WHITE, 2), "{:?}", host.pixel(center + 14, y));
        }
    }
}

#[test]
fn dragging_the_thumb_moves_it_in_steps_and_reports_the_value() {
    let mut host = host(1.0, sliders);
    let r = host.rect(host.ui.state.ids[0]);
    let s = slider(&host, 0);
    let x = r.left + s.end_thumb_x + 17.5;
    let y = r.center_y();
    // 50 points right: the thumb goes from 107.25 to 157.25 of 165, the value to 95.3, in steps of 1.
    host.pan((x, y), (x + 50.0, y), 160.0, 10);
    host.settle();
    let s = slider(&host, 0);
    assert_eq!(s.end_thumb_x, 157.25);
    assert_eq!(s.p.end, 95.0);
    assert!(!s.is_pressed && !s.is_user_panning);
    // Reported once per frame at most, the last one is the value.
    let reports: Vec<f32> = host.ui.state.changed.iter().filter(|c| c.0 == 0).map(|c| c.2).collect();
    assert_eq!(reports.last(), Some(&95.0));
    assert!(reports.len() <= 11 && reports.windows(2).all(|w| w[0] < w[1]), "{reports:?}");

    // Past the end: clamped.
    host.pan((x + 50.0, y), (x + 150.0, y), 160.0, 10);
    host.settle();
    assert_eq!(slider(&host, 0).p.end, 100.0);
    assert_eq!(slider(&host, 0).end_thumb_x, 165.0);
}

#[test]
fn a_press_on_the_trail_moves_the_nearest_thumb_there() {
    let mut host = host(1.0, sliders);
    // Single: the press point becomes the thumb's center: 20 - 17.5 = 2.5 of 165 = 1.5, stepped 2.
    let r = host.rect(host.ui.state.ids[0]);
    host.tap(r.left + 20.0, r.center_y());
    assert_eq!(slider(&host, 0).p.end, 2.0);
    assert_eq!(host.ui.state.changed, vec![(0, "end", 2.0)]);

    // Range (Windows, box 28): the left half moves the start thumb, the right half the end one.
    let r = host.rect(host.ui.state.ids[6]);
    host.tap(r.left + 4.0, r.center_y());
    // 4 - 14 < 0: clamped to 0.
    assert_eq!((slider(&host, 6).p.start, slider(&host, 6).p.end), (0.0, 80.0));
    host.tap(r.right - 4.0, r.center_y());
    assert_eq!((slider(&host, 6).p.start, slider(&host, 6).p.end), (0.0, 100.0));
    assert_eq!(&host.ui.state.changed[1..], [(6, "start", 0.0), (6, "end", 100.0)]);

    // Off: nothing moves.
    host.ui.tree.find_mut::<SkiaSlider>(host.ui.state.ids[1]).unwrap().set_click_on_trail_enabled(false);
    let r = host.rect(host.ui.state.ids[1]);
    host.tap(r.left + 20.0, r.center_y());
    assert_eq!(slider(&host, 1).p.end, 65.0);
}

#[test]
fn range_thumbs_do_not_cross() {
    let mut host = host(1.0, sliders);
    let index = 7; // Cupertino range: box 36, travel 164, start at 32.8, end at 131.2.
    let r = host.rect(host.ui.state.ids[index]);
    let s = slider(&host, index);
    let y = r.center_y();
    let x = r.left + s.start_thumb_x + 18.0;
    // Drag the start thumb far right: it stops at the end thumb.
    host.pan((x, y), (x + 200.0, y), 160.0, 10);
    host.settle();
    let s = slider(&host, index);
    assert_eq!(s.start_thumb_x, s.end_thumb_x);
    assert_eq!((s.p.start, s.p.end), (80.0, 80.0));
}

#[test]
fn values_set_from_code_move_the_thumbs_clamped_and_are_reported() {
    let mut host = host(1.0, sliders);
    let id = host.ui.state.ids[0];
    host.ui.tree.find_mut::<SkiaSlider>(id).unwrap().set_end(40);
    host.settle();
    assert_eq!(slider(&host, 0).end_thumb_x, 0.4 * 165.0);
    host.ui.tree.find_mut::<SkiaSlider>(id).unwrap().set_end(140);
    host.settle();
    assert_eq!(slider(&host, 0).p.end, 100.0);
    assert_eq!(host.ui.state.changed, vec![(0, "end", 40.0), (0, "end", 100.0)]);
}

// ---------------------------------------------------------------- SliderInScrollDragTests

/// Upstream AdjustPage.SliderRow: title and value on top, the slider below, the value follows `end`.
fn slider_row(title: &str, value: f32, min: f32, max: f32, step: f32, tapped: bool, handle: Option<&mut Handle<SkiaSlider>>) -> Build<SkiaLayout> {
    let mut label = Handle::<SkiaLabel>::default();
    let mut slider = SkiaSlider::new()
        .control_style(PrebuiltControlStyle::Windows)
        .min(min)
        .max(max)
        .step(step)
        .end(value)
        .fill_x()
        .on_end_changed(move |_me, _app: &mut App, cx, v| {
            if let Some(mut label) = cx.get_mut(label) {
                label.set_text(format!("{v:.2}"));
            }
        });
    if tapped {
        // Upstream attaches a gesture listener that consumes nothing; a tapped handler here.
        slider = slider.on_tapped(|_me, _app: &mut App, _cx| {});
    }
    if let Some(handle) = handle {
        slider = slider.assign(handle);
    }
    SkiaLayout::column().spacing(6).padding((16, 12, 16, 10)).children((
        SkiaLayout::layer().children((
            SkiaLabel::new(title).text_color(Color::WHITE),
            SkiaLabel::new(format!("{value:.2}"))
                .font_size(14)
                .text_color(Color::from_rgb(0x80, 0x80, 0x80))
                .horizontal_options(LayoutOptions::End)
                .vertical_options(LayoutOptions::Center)
                .assign(&mut label),
        )),
        slider,
    ))
}

fn card(rows: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(14)
        .background_color(Color::from_rgb(0x2F, 0x4F, 0x4F))
        .fill_x()
        .children(SkiaLayout::column().spacing(0).children(rows))
}

fn adjust_scene(tapped: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let gray = Color::from_rgb(0x80, 0x80, 0x80);
        let heading = |text: &str| SkiaLabel::new(text).text_color(gray).margin((0, 12, 0, 8));
        let content = SkiaLayout::column().spacing(0).fill_x().children((
            heading("IMAGE"),
            card((
                slider_row("Brightness", 1.0, 0.5, 1.5, 0.01, tapped, None),
                slider_row("Contrast", 1.0, 0.5, 1.5, 0.01, tapped, None),
                slider_row("Saturation", 1.0, 0.0, 2.0, 0.01, tapped, None),
                slider_row("Hue", 0.0, -180.0, 180.0, 1.0, tapped, None),
            )),
            heading("DETAIL"),
            card(slider_row("Sharpness", 0.3, 0.0, 1.0, 0.01, tapped, Some(&mut app.sharpness))),
        ));
        let scroll = SkiaScroll::new().fill().scroll_bar(SkiaScrollBar::new()).content(content).assign(&mut app.scroll);
        SkiaLayout::grid()
            .row_spacing(0)
            .padding((24, 20))
            .fill()
            .row_definitions(vec![GridLength::Auto, GridLength::Star(1.0), GridLength::Auto, GridLength::Auto])
            .children((
                SkiaLayout::column().spacing(6).children((
                    SkiaLabel::new("ADJUST").font_size(32).text_color(Color::RED),
                    SkiaLabel::new("Fine-tune the target of your filter. Double tap a slider to reset it.")
                        .font_size(13)
                        .text_color(gray)
                        .fill_x(),
                )),
                scroll.row(1),
                card(SkiaLayout::row().padding((16, 12)).children((
                    SkiaLabel::new("Background only").text_color(Color::WHITE),
                    SkiaSwitch::new().horizontal_options(LayoutOptions::End),
                )))
                .margin((0, 12, 0, 0))
                .row(2),
                SkiaLayout::row().spacing(12).margin((0, 16, 0, 0)).horizontal_options(LayoutOptions::Center).row(3).children((
                    SkiaButton::new("Reset").width_request(100),
                    SkiaButton::new("Undo").width_request(100),
                    SkiaButton::new("Close").width_request(100),
                )),
            ))
    })
    .font_bytes("Default", FONT)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 480, 520, 1.0);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    host
}

fn offset(host: &Headless<App>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// Upstream DraggingThumbBelowTheFold_KeepsScrollOffset: scroll down, press the thumb of the last
/// slider and drag it: the scroll keeps its offset while the slider takes the pan.
#[test]
fn dragging_a_thumb_below_the_fold_keeps_the_scroll_offset() {
    for tapped in [true, false] {
        let mut host = adjust_scene(tapped);
        let vp = host.rect(host.ui.state.scroll);
        host.pan((vp.center_x(), vp.bottom - 40.0), (vp.center_x(), vp.top + 40.0), 250.0, 16);
        host.settle();
        let before = offset(&host);
        assert!(before < -50.0, "scroll did not move: {before}");
        // Where the slider is seen: its rect moved by the scroll.
        let slider = host.ui.state.sharpness;
        let rect = host.rect(slider).with_offset((0.0, before));
        assert!(rect.top > vp.top && rect.bottom < vp.bottom, "slider not in the viewport: {rect:?} vs {vp:?}");

        let (x, y) = (rect.left + rect.width() * 0.3, rect.center_y());
        let end_before = host.ui.tree.find::<SkiaSlider>(slider).unwrap().p.end;
        host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
        host.frame_after(16.0);
        for i in 1..=8 {
            host.ui.pointer(PointerKind::Move, x + i as f32 * 8.0, y + (i % 2) as f32, host.time_ms());
            host.frame_after(16.0);
        }
        let during = offset(&host);
        host.ui.pointer(PointerKind::Up, x + 64.0, y, host.time_ms());
        for _ in 0..3 {
            host.frame_after(16.0);
        }
        let end = host.ui.tree.find::<SkiaSlider>(slider).unwrap().p.end;
        assert!((during - before).abs() <= 1.0, "moved during the drag: {before} -> {during}");
        assert!((offset(&host) - before).abs() <= 1.0, "moved after the drag: {before} -> {}", offset(&host));
        assert!(end > end_before, "the slider did not take the drag: {end_before} -> {end}");
    }
}

/// Upstream ContentThatShrinksToFit_LandsOnZero: content that shrinks to fit the viewport puts
/// the scroll back at 0.
#[test]
fn content_that_shrinks_to_fit_lands_on_zero() {
    #[derive(Default)]
    struct Scene {
        scroll: Handle<SkiaScroll>,
        spacer: Handle<SkiaShape>,
    }
    let ui = Ui::new(Scene::default(), |app: &mut Scene| {
        let spacer = SkiaShape::new().height_request(900).fill_x().background_color(Color::from_rgb(0x80, 0x80, 0x80));
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().fill_x().children(spacer.assign(&mut app.spacer)))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 400, 300, 1.0);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    host.pan((200.0, 250.0), (200.0, 50.0), 250.0, 16);
    host.settle();
    let offset = |host: &Headless<Scene>| host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y();
    assert!(offset(&host) < -100.0, "scroll did not move: {}", offset(&host));
    host.ui.tree.find_mut::<SkiaShape>(host.ui.state.spacer).unwrap().set_height_request(100);
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert!(offset(&host).abs() <= 0.5, "{}", offset(&host));
}
