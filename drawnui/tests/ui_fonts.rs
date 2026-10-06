//! Fonts the host loads: faces of one family at several weights (React `AddFont(source, alias,
//! weight)`), which the first layout waits for, and fallback fonts it does not wait for.

use drawnui::App as _;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/");

fn bytes(file: &str) -> Vec<u8> {
    std::fs::read(format!("{ASSETS}{file}")).expect("font file")
}

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().children(SkiaLabel::new("★★★★★★").font_family_fallback("Symbols").assign(&mut app.label))
}

#[test]
fn weighted_faces_gate_the_first_layout_and_bold_picks_the_nearest() {
    let ui = Ui::new(App::default(), build)
        .font("Text", "OpenSans-Regular.ttf")
        .font_weight("Text", "OpenSans-Semibold.ttf", 600);
    let mut host = Headless::new(ui, 300, 100, 1.0);
    host.frame();
    host.ui.asset(0, bytes("OpenSans-Regular.ttf"));
    host.settle();
    // One of the two faces is still on its way: nothing is laid out.
    assert!(host.rect(host.ui.state.label).is_empty());
    host.ui.asset(1, bytes("OpenSans-Semibold.ttf"));
    host.settle();
    assert!(!host.rect(host.ui.state.label).is_empty());
    assert_eq!(host.ui.fonts.resolve("Text", 700).map(|(_, weight)| weight), Some(600));
    assert_eq!(host.ui.fonts.resolve("Text", 0).map(|(_, weight)| weight), Some(400));
}

#[test]
fn a_fallback_font_does_not_gate_the_layout_and_texts_are_measured_again_when_it_arrives() {
    let ui = Ui::new(App::default(), build).font("Text", "OpenSans-Regular.ttf").font_fallback("Symbols", "NotoSansSymbols2-Subset.ttf");
    let mut host = Headless::new(ui, 300, 100, 1.0);
    host.frame();
    host.ui.asset(0, bytes("OpenSans-Regular.ttf"));
    host.settle();
    let before = host.rect(host.ui.state.label);
    assert!(!before.is_empty(), "laid out without the fallback");
    host.ui.asset(1, bytes("NotoSansSymbols2-Subset.ttf"));
    host.settle();
    let family = |host: &Headless<App>| host.ui.fonts.typeface("Symbols").map(|t| t.family_name());
    assert_eq!(family(&host).as_deref(), Some("Noto Sans Symbols 2"));
    // The stars now come from the fallback face: the label was measured again.
    assert_ne!(host.rect(host.ui.state.label).width(), before.width());
}
