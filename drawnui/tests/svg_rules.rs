//! SkiaSvg: inline markup and files, sizing by the picture's aspect, LockRatio, TintColor, and
//! the rasterization at the displayed size. Needs the `svg` feature (on by default).
#![cfg(feature = "svg")]

use std::time::Instant;

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// A red square with a blue bottom-right quarter, 24 units, a viewBox only (no width / height).
const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#FF0000"/><rect x="12" y="12" width="12" height="12" fill="#0000FF"/></svg>"##;
/// Twice as wide as high, width / height attributes and no viewBox.
const WIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#00FF00"/></svg>"##;
/// The logo of the demo's SvgPage (Inkscape file, 640 x 640).
const LOGO: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/images/drawnui.svg"));

fn shot(content: impl IntoChildren) -> Headless<()> {
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    host
}

fn files(url: &str) -> Option<Vec<u8>> {
    match url {
        "images/square.svg" => Some(SQUARE.as_bytes().to_vec()),
        "images/drawnui.svg" => Some(LOGO.to_vec()),
        _ => None,
    }
}

#[test]
fn inline_markup_is_drawn_at_the_displayed_size() {
    let svg = SkiaSvg::from_string(SQUARE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
    let mut host = shot(svg);
    assert_eq!(host.pixel(21, 21), Color::RED);
    assert_eq!(host.pixel(69, 69), Color::RED);
    assert_eq!(host.pixel(70, 70), Color::BLUE);
    assert_eq!(host.pixel(119, 119), Color::BLUE);
    assert_eq!(host.pixel(120, 120), Color::WHITE);
    assert_eq!(host.pixel(19, 60), Color::WHITE);
}

/// LockRatio 1 with a width makes the square box; an unbounded side follows the picture's aspect.
#[test]
fn sizing_follows_lock_ratio_and_the_pictures_aspect() {
    let ui = Ui::new(Handle::<SkiaSvg>::default(), |svg| {
        SkiaLayout::new().fill().children(SkiaSvg::from_string(SQUARE).width_request(120).lock_ratio(1).assign(svg))
    });
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_eq!(host.rect(host.ui.state), Rect::from_xywh(0.0, 0.0, 120.0, 120.0));

    // A column gives the width and no height: the height comes from the aspect (40 x 20).
    let ui = Ui::new(Handle::<SkiaSvg>::default(), |svg| {
        SkiaStack::new().children(SkiaSvg::from_string(WIDE).width_request(100).assign(svg))
    });
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_eq!(host.rect(host.ui.state), Rect::from_xywh(0.0, 0.0, 100.0, 50.0));
    assert_eq!(host.ui.tree.find::<SkiaSvg>(host.ui.state).unwrap().intrinsic_size(), Size::new(40.0, 20.0));
    assert_eq!(host.pixel(50, 25), Color::GREEN);
}

/// AspectFitFill (the default) keeps the aspect inside the box and centers the picture.
#[test]
fn aspect_and_alignment() {
    let svg = SkiaSvg::from_string(WIDE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
    let mut host = shot(svg);
    // 40 x 20 fitted into 100 x 100: 100 x 50, centered: rows 45..95.
    assert_eq!(host.pixel(70, 44), Color::WHITE);
    assert_eq!(host.pixel(70, 45), Color::GREEN);
    assert_eq!(host.pixel(70, 94), Color::GREEN);
    assert_eq!(host.pixel(70, 95), Color::WHITE);
    assert_eq!(host.pixel(20, 70), Color::GREEN);
    assert_eq!(host.pixel(119, 70), Color::GREEN);

    let svg = SkiaSvg::from_string(WIDE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
    let mut host = shot(svg.vertical_alignment(DrawImageAlignment::Start));
    assert_eq!(host.pixel(70, 20), Color::GREEN);
    assert_eq!(host.pixel(70, 70), Color::WHITE);

    // Fill stretches to the box, as the browser stretches the decoded SVG in DrawnUi.React.
    let svg = SkiaSvg::from_string(WIDE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
    let mut host = shot(svg.aspect(TransformAspect::Fill));
    assert_eq!(host.pixel(70, 21), Color::GREEN);
    assert_eq!(host.pixel(70, 118), Color::GREEN);
    assert_eq!(host.pixel(21, 70), Color::GREEN);
    assert_eq!(host.pixel(118, 70), Color::GREEN);
}

/// A root with its own size (`width="800px"`, as the checkbox marks have) and no xmlns is drawn
/// into the box by its viewBox, not at 800 pixels.
#[test]
fn a_root_size_does_not_win_over_the_box() {
    const MARK: &str = r##"<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none"><rect width="12" height="12" fill="#FF0000"/></svg>"##;
    let mut host = shot(SkiaSvg::from_string(MARK).margin((20, 20, 0, 0)).width_request(48).height_request(48));
    assert_eq!(host.pixel(21, 21), Color::RED);
    assert_eq!(host.pixel(43, 43), Color::RED);
    assert_eq!(host.pixel(44, 44), Color::WHITE);
    assert_eq!(host.pixel(60, 30), Color::WHITE);
}

/// TintColor recolors every opaque pixel (SrcIn); transparent keeps the picture's colors.
#[test]
fn tint_color() {
    let tinted = SkiaSvg::from_string(SQUARE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
    let mut host = shot(tinted.tint_color(Color::from_rgb(0x4E, 0xCD, 0xC4)));
    assert_eq!(host.pixel(40, 40), Color::from_rgb(0x4E, 0xCD, 0xC4));
    assert_eq!(host.pixel(100, 100), Color::from_rgb(0x4E, 0xCD, 0xC4));
    assert_eq!(host.pixel(19, 60), Color::WHITE);

    let ui = Ui::new(Handle::<SkiaSvg>::default(), |svg| {
        let plain = SkiaSvg::from_string(SQUARE).margin((20, 20, 0, 0)).width_request(100).height_request(100);
        SkiaLayout::new().fill().children(plain.assign(svg))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_eq!(host.pixel(40, 40), Color::RED);
    host.ui.tree.get_mut(host.ui.state).unwrap().set_tint_color(Color::BLACK);
    host.settle();
    assert_eq!(host.pixel(40, 40), Color::BLACK);
    host.ui.tree.get_mut(host.ui.state).unwrap().set_tint_color(Color::TRANSPARENT);
    host.settle();
    assert_eq!(host.pixel(100, 100), Color::BLUE);
}

/// A file comes through the tree's asset channel: one request per source however many controls
/// show it, and every one of them draws it when it arrives. An empty answer is an error. A
/// control that shows a file another control has takes it without a request.
#[test]
fn files_load_once_per_source_through_the_asset_channel() {
    let ui = Ui::new(Vec::<Handle<SkiaSvg>>::default(), |svgs| {
        let mut one = |source: &str, x: i32, y: i32| {
            let mut handle = Handle::default();
            let svg = SkiaSvg::new(source).margin((x, y, 0, 0)).width_request(50).height_request(50).assign(&mut handle);
            svgs.push(handle);
            svg
        };
        let children = (one("images/square.svg", 20, 20), one("images/square.svg", 90, 20), one("images/missing.svg", 20, 100));
        SkiaLayout::new().fill().children(children)
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    let svgs = host.ui.state.clone();
    assert!(host.ui.tree.find::<SkiaSvg>(svgs[0]).unwrap().is_loading());
    assert!(host.ui.tree.find::<SkiaSvg>(svgs[1]).unwrap().is_loading());
    assert_eq!(host.pixel(30, 30), Color::WHITE);

    assert_eq!(host.deliver_assets(files), ["images/square.svg", "images/missing.svg"]);
    host.settle();
    assert_eq!(host.pixel(30, 30), Color::RED);
    assert_eq!(host.pixel(100, 30), Color::RED);
    assert_eq!(host.pixel(30, 110), Color::WHITE);
    assert!(host.ui.tree.find::<SkiaSvg>(svgs[2]).unwrap().has_error());
    assert!(!host.ui.tree.find::<SkiaSvg>(svgs[0]).unwrap().is_loading());

    // The failed one shows the file the others have: no request, drawn in the next frame.
    host.ui.tree.get_mut(svgs[2]).unwrap().set_source("images/square.svg");
    host.settle();
    assert!(host.deliver_assets(files).is_empty(), "nothing asked for twice");
    assert_eq!(host.pixel(30, 110), Color::RED);
    assert!(!host.ui.tree.find::<SkiaSvg>(svgs[2]).unwrap().has_error());
}

/// The logo of the demo page: 640 x 640, drawn into 200 points with LockRatio at scale 2.
#[test]
fn the_demo_logo() {
    let ui = Ui::new(Handle::<SkiaSvg>::default(), |svg| {
        SkiaStack::new().children(SkiaSvg::new("images/drawnui.svg").width_request(200).lock_ratio(1).assign(svg))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 500, 500, 2.0);
    host.settle();
    host.deliver_assets(files);
    host.settle();
    let svg = host.ui.state;
    let size = host.ui.tree.find::<SkiaSvg>(svg).unwrap().intrinsic_size();
    assert!((size.width - 640.0).abs() < 0.01 && (size.height - 640.0).abs() < 0.01, "{size:?}");
    assert_eq!(host.rect(svg), Rect::from_xywh(0.0, 0.0, 400.0, 400.0));
    let inked = (0..400).step_by(4).flat_map(|y| (0..400).step_by(4).map(move |x| (x, y))).filter(|&(x, y)| host.pixel(x, y) != Color::WHITE).count();
    assert!(inked > 1000, "the logo draws: {inked} of 10000 sampled pixels");
}

/// Another size rasterizes again.
#[test]
fn another_size_rasterizes_again() {
    let ui = Ui::new(Handle::<SkiaSvg>::default(), |svg| {
        let it = SkiaSvg::from_string(SQUARE).margin((20, 20, 0, 0)).width_request(100).height_request(100).use_cache(CacheType::None);
        SkiaLayout::new().fill().children(it.assign(svg))
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_eq!(host.pixel(119, 119), Color::BLUE);
    host.ui.tree.get_mut(host.ui.state).unwrap().set_width_request(40);
    host.ui.tree.get_mut(host.ui.state).unwrap().set_height_request(40);
    host.settle();
    assert_eq!(host.pixel(59, 59), Color::BLUE);
    assert_eq!(host.pixel(60, 60), Color::WHITE);
}

/// A measurement: parsing and rasterizing the demo logo, and a frame of the demo page's five
/// logos once they are rasters.
///
/// `cargo test --release -p drawnui --test svg_rules -- --ignored --nocapture`
#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn cost_of_the_demo_logo() {
    const RUNS: u32 = 50;
    for side in [72, 200, 600] {
        let start = Instant::now();
        for _ in 0..RUNS {
            let ui = Ui::new((), |_| SkiaStack::new().children(SkiaSvg::from_string(std::str::from_utf8(LOGO).unwrap()).width_request(side).lock_ratio(1)));
            let mut host = Headless::new(ui, 600, 600, 1.0);
            host.settle();
        }
        let micros = start.elapsed().as_secs_f64() * 1e6 / RUNS as f64;
        println!("logo parsed and rasterized at {side} px, first frame: {micros:.0} us");
    }
    let ui = Ui::new((), |_| {
        let logo = |side: i32, tint: Color| SkiaSvg::new("images/drawnui.svg").width_request(side).lock_ratio(1).tint_color(tint);
        let row = SkiaRow::new().children((
            logo(72, Color::WHITE),
            logo(72, Color::from_rgb(0xFF, 0x6B, 0x6B)),
            logo(72, Color::from_rgb(0x4E, 0xCD, 0xC4)),
            logo(72, Color::from_rgb(0xFF, 0xD9, 0x3D)),
        ));
        SkiaStack::new().children((logo(200, Color::TRANSPARENT), row))
    });
    let mut host = Headless::new(ui, 800, 800, 2.0);
    host.settle();
    host.deliver_assets(files);
    host.settle();
    const FRAMES: u32 = 200;
    let start = Instant::now();
    for _ in 0..FRAMES {
        host.frame_after(16.0);
    }
    let micros = start.elapsed().as_secs_f64() * 1e6 / FRAMES as f64;
    println!("five logos at scale 2, a frame: {micros:.0} us");
}
