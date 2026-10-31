//! A label keeps the lines it laid out with no limit on the width for a later measure that only
//! changes the room, while the room holds them. Every change of the label, and a room too small,
//! lays it out again: after each change the label measures as a fresh label with the same
//! properties.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));
const TEXT: &str = "Lines laid out once are kept";

#[derive(Default)]
struct App {
    stack: Handle<SkiaLayout>,
    label: Handle<SkiaLabel>,
}

type Change = fn(&mut Mut<'_, SkiaLabel>);

/// A label in a card that fills a column sized by its content: measured for its content (no width
/// limit), then at the column's width.
fn host(change: Option<Change>, max_width: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let label = SkiaLabel::new(TEXT).font_size(20).assign(&mut app.label);
        SkiaLayout::new().fill().children((SkiaLayout::column()
            .horizontal_options(LayoutOptions::Center)
            .maximum_width_request(max_width)
            .assign(&mut app.stack)
            .children((SkiaLayout::new().fill_x().padding(8).children((label,)),)),))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 600, 400, 1.0);
    // A fresh label: changed before its first measure.
    if let Some(change) = change {
        let label = host.ui.state.label;
        host.ui.tree.get_mut(label).map(|mut me| change(&mut me));
    }
    host.settle();
    host
}

fn size(host: &Headless<App>) -> (f32, f32) {
    let r = host.ui.tree.base(host.ui.state.label).unwrap().rect;
    (r.width(), r.height())
}

#[test]
fn every_change_lays_the_label_out_again() {
    let changes: [(&str, Change); 9] = [
        ("text", |me| me.set_text("Another, much longer text that is laid out again")),
        ("font size", |me| me.set_font_size(26)),
        ("bold", |me| me.set_font_attributes(FontAttributes::Bold)),
        ("character spacing", |me| me.set_character_spacing(1.5)),
        ("text transform", |me| me.set_text_transform(TextTransform::Uppercase)),
        ("max lines", |me| {
            me.set_text("One line\nTwo lines\nThree lines");
            me.set_max_lines(2);
        }),
        ("span text", |me| me.set_spans((TextSpan::new("A span with its own, longer text"),))),
        ("span size", |me| me.set_spans((TextSpan::new(TEXT).font_size(30.0),))),
        ("span added", |me| me.set_spans((TextSpan::new(TEXT), TextSpan::new(" and one more span")))),
    ];
    for (name, change) in changes {
        let mut changed = host(None, 1000.0);
        let label = changed.ui.state.label;
        changed.ui.tree.get_mut(label).map(|mut me| change(&mut me));
        changed.settle();
        let fresh = host(Some(change), 1000.0);
        assert_eq!(size(&changed), size(&fresh), "{name}");
    }
}

#[test]
fn a_narrower_parent_wraps_the_kept_lines() {
    let mut narrowed = host(None, 1000.0);
    let wide = size(&narrowed);
    let stack = narrowed.ui.state.stack;
    narrowed.ui.tree.get_mut(stack).unwrap().set_maximum_width_request(120);
    narrowed.settle();
    let fresh = host(None, 120.0);
    assert_eq!(size(&narrowed), size(&fresh));
    assert!(size(&narrowed).1 > wide.1, "the text wraps: {wide:?} -> {:?}", size(&narrowed));
}

/// A font that arrives after the first measure is used: the words are shaped again although the
/// label itself did not change.
#[test]
fn a_font_that_arrives_later_shapes_the_text_again() {
    const ORBITRON: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/Orbitron-Regular.ttf"));
    let make = || {
        Ui::new(App::default(), |app: &mut App| {
            SkiaLayout::new().fill().children((SkiaLabel::new(TEXT).font_size(20).font_family("Wide").assign(&mut app.label),))
        })
        .font_bytes("Default", FONT)
    };
    let mut late = Headless::new(make(), 600, 400, 1.0);
    late.settle();
    let before = size(&late);
    late.ui.fonts.add("Wide", ORBITRON);
    let root = late.ui.tree.root().unwrap();
    late.ui.tree.invalidate(root, Dirty::MEASURE);
    let label = late.ui.state.label;
    late.ui.tree.invalidate(label, Dirty::MEASURE);
    late.settle();
    let mut fresh = Headless::new(make(), 600, 400, 1.0);
    fresh.ui.fonts.add("Wide", ORBITRON);
    fresh.settle();
    assert_ne!(size(&late), before, "the new font is wider");
    assert_eq!(size(&late), size(&fresh));
}
