//! A Grid's Auto row holding a label that wraps at the grid's width gets the height of every line
//! (the gallery's Boss fight header on a 390 pt phone).

use drawnui::controls::layout::SkiaGrid;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
    below: Handle<SkiaShape>,
}

#[test]
fn an_auto_row_is_as_tall_as_its_wrapped_label() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaGrid::new().fill().padding(20).row_spacing(10).row_definitions("Auto,*").children((
            SkiaLayout::column().spacing(8).fill_x().use_cache(CacheType::Image).row(0).children((
                SkiaLabel::new("BOSS FIGHT").font_size(30).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("A glTF model drawn in 3D inside DrawnUI, driven by the app. Drag it to turn it, tap it to hit it.")
                    .font_size(15)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center)
                    .assign(&mut app.label),
            )),
            SkiaShape::new().fill().row(1).assign(&mut app.below),
        ))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 390, 844, 1.0);
    host.settle();
    let (label, below) = (host.rect(host.ui.state.label), host.rect(host.ui.state.below));
    assert!(label.width() <= 350.0 + 0.5, "the label keeps to the grid's width: {label:?}");
    assert!(label.height() > 30.0, "wrapped on more than one line: {label:?}");
    assert!(below.top >= label.bottom, "the next row starts under the whole label: label {label:?}, below {below:?}");
}
