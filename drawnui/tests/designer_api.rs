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
