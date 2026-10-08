//! Wrapped and spaced text in a grid (drawnui-cross 6p; C# LabelCharacterSpacingTests and
//! GridAutoRowWrapTests, 7cf1007c): a label wrapping in a star column next to an Auto column gets
//! all its lines and its Auto row grows to them; with CharacterSpacing the label measures and cuts
//! at the width it draws.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const TITLE: &str = "ESTILIZAR UMA FOTO";
const ELLIPSIS: char = '\u{2026}';

type Host = Headless<Handle<SkiaLabel>>;

fn host(width: i32, height: i32, scale: f32, label: Build<SkiaLabel>, root: impl FnOnce(Build<SkiaLabel>) -> Build<SkiaLayout> + 'static) -> Host {
    let ui = Ui::new(Handle::default(), move |handle: &mut Handle<SkiaLabel>| root(label.assign(handle)))
        .font_bytes("Default", FONT)
        .background(Color::BLACK);
    let mut host = Headless::new(ui, width, height, scale);
    host.settle();
    host
}

fn lines(host: &Host) -> Vec<String> {
    host.ui.tree.find::<SkiaLabel>(host.ui.state).unwrap().lines().map(|l| l.0.to_owned()).collect()
}

/// One past the rightmost column with ink, 0 when nothing is drawn.
fn ink_right(host: &mut Host, width: i32, height: i32) -> i32 {
    for x in (0..width).rev() {
        if (0..height).any(|y| host.pixel(x, y).r() > 80) {
            return x + 1;
        }
    }
    0
}

/// The title as one label, and in two spans.
fn titles() -> [(&'static str, Build<SkiaLabel>); 2] {
    [
        ("label", SkiaLabel::new(TITLE)),
        ("spans", SkiaLabel::new("").spans(vec![TextSpan::new("ESTILIZAR "), TextSpan::new("UMA FOTO").is_bold(true)])),
    ]
}

#[test]
fn a_label_wrapping_in_a_star_column_grows_its_auto_row() {
    for vertical in [LayoutOptions::Center, LayoutOptions::Fill] {
        let label = SkiaLabel::new("A long caption that needs two lines in its column").font_size(16).max_lines(3);
        let label = label.text_color(Color::WHITE).vertical_options(vertical).column(0);
        let neighbor = SkiaShape::new().width_request(220).height_request(30).background_color(Color::GRAY).column(1);
        let host = host(400, 300, 1.0, label, |label| SkiaGrid::new().column_spacing(0).column_definitions("*,Auto").children((label, neighbor)));
        let lines = lines(&host);
        assert!(lines.len() >= 2, "{vertical:?}: {lines:?}");
        assert!(lines.iter().all(|l| !l.ends_with(ELLIPSIS)), "{vertical:?}: cut: {lines:?}");
        let label = host.rect(host.ui.state);
        assert!(label.width() <= 181.0, "{vertical:?}: label {} px in a 180 px column", label.width());
        let grid = host.rect(host.ui.tree.parent(host.ui.state).unwrap());
        assert!(grid.height() >= label.height(), "{vertical:?}: grid {} px, label {} px", grid.height(), label.height());
    }
}

#[test]
fn spaced_text_measures_the_width_it_draws() {
    for (name, label) in titles() {
        let label = label.font_size(32).character_spacing(3).text_color(Color::WHITE);
        let mut host = host(700, 200, 1.0, label, |label| SkiaLayout::new().fill().children(label));
        let measured = host.ui.tree.base(host.ui.state).unwrap().measured.width;
        let ink = ink_right(&mut host, 700, 200) as f32;
        assert!(ink > 0.0, "{name}: nothing drawn");
        // The last glyph's side bearing is not ink.
        assert!(ink <= measured + 1.0 && measured - ink < 12.0, "{name}: measured {measured} px, ink ends at {ink}");
    }
}

#[test]
fn a_spaced_title_wraps_inside_its_star_column() {
    let (scale, column) = (1.25, 151.0);
    for (name, label) in titles() {
        let label = label.use_cache(CacheType::Image).font_size(12).character_spacing(3).max_lines(2).text_color(Color::WHITE);
        let label = label.padding(Thickness::new(0.0, 8.0, 12.0, 8.0)).horizontal_options(LayoutOptions::Start).vertical_options(LayoutOptions::Center).column(0);
        let neighbor = SkiaShape::new().width_request(402.0 - 40.0 - column).height_request(38).horizontal_options(LayoutOptions::End).column(1);
        let (width, height) = ((402.0 * scale) as i32, (200.0 * scale) as i32);
        let mut host = host(width, height, scale, label, |label| {
            SkiaGrid::new().margin(Thickness::new(20.0, 0.0, 20.0, 12.0)).column_spacing(0).column_definitions("*,Auto").children((label, neighbor))
        });
        let lines = lines(&host);
        assert_eq!(lines.len(), 2, "{name}: {lines:?}");
        assert!(lines.iter().all(|l| !l.ends_with(ELLIPSIS)), "{name}: cut: {lines:?}");
        let text_right = ((20.0 + column - 12.0) * scale).round() as i32;
        let ink = ink_right(&mut host, width, height);
        assert!(ink > 0 && ink <= text_right + 1, "{name}: ink ends at {ink}, the text area at {text_right}");
    }
}

#[test]
fn spaced_text_is_cut_at_the_width_it_draws() {
    let spans_cut_in_the_second = SkiaLabel::new("").spans(vec![TextSpan::new("ESTI "), TextSpan::new("LIZARUMAFOTOLONGA").is_bold(true)]);
    for (name, label) in titles().into_iter().chain([("cut in the second span", spans_cut_in_the_second)]) {
        let label = label.font_size(20).character_spacing(3).max_lines(1).width_request(150).text_color(Color::WHITE);
        let mut host = host(400, 100, 1.0, label, |label| SkiaLayout::new().fill().children(label));
        let ink = ink_right(&mut host, 400, 100);
        assert!(ink > 0 && ink <= 151, "{name}: ink ends at {ink} in a 150 px label: {:?}", lines(&host));
    }
}
