//! SkiaScene3D (feature `model3d`): many objects on one game field, placed in the layer's points,
//! each with its own animations, every model read once; taps tell which object was hit.
#![cfg(feature = "model3d")]

use drawnui::controls::model3d::SceneHit;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{MouseButton, PointerKind};

const DRAGON: &[u8] = include_bytes!("../../examples/model3d/assets/dragon.glb");
const PUNCH: &str = "CharacterArmature|Punch";

#[derive(Default)]
struct App {
    field: Handle<SkiaScene3D>,
    ended: Vec<(String, String)>,
    hits: Vec<SceneHit>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaScene3D::new()
            .model("dragon", DRAGON)
            .fill()
            .assign(&mut app.field)
            .on_animation_ended(|_me, app: &mut App, _cx, object, animation| app.ended.push((object.to_owned(), animation.to_owned())))
            .on_tapped_3d(|_me, app: &mut App, _cx, hit| app.hits.push(hit.clone())),))
    });
    let mut host = Headless::new(ui, 600, 400, 1.0);
    host.frame_after(16.7);
    host
}

fn frames(host: &mut Headless<App>, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        host.frame_after(16.7);
    }
}

fn tap(host: &mut Headless<App>, x: f32, y: f32) {
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Down, MouseButton::Left, x, y, t);
    host.frame_after(16.7);
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Up, MouseButton::Left, x, y, t);
    frames(host, 0.1);
}

#[test]
fn many_objects_each_with_its_own_animation_and_taps_by_object() {
    let mut host = host();
    let field = host.ui.state.field;
    {
        let mut layer = host.ui.tree.get_mut(field).unwrap();
        // A crowd: 30 dragons of one model, 60 points tall, standing in rows.
        for i in 0..30 {
            assert!(layer.add(&format!("mob{i}"), "dragon", 40.0 + (i % 10) as f32 * 55.0, 120.0 + (i / 10) as f32 * 110.0, 60.0));
        }
        assert!(!layer.add("x", "no such model", 0.0, 0.0, 10.0));
        assert!(layer.play_once("mob3", PUNCH));
    }
    assert_eq!(host.ui.tree.find::<SkiaScene3D>(field).unwrap().object_names().len(), 30);
    frames(&mut host, 3.0);
    assert_eq!(host.ui.state.ended, [("mob3".to_owned(), PUNCH.to_owned())], "only the one that punched");
    assert!(host.ui.needs_frame(), "the crowd keeps idling");

    // mob0 stands at (40, 120), 60 tall: its body is around (40, 95).
    tap(&mut host, 40.0, 95.0);
    assert_eq!(host.ui.state.hits.last().map(|h| h.object.as_str()), Some("mob0"), "{:?}", host.ui.state.hits);
    // Empty field between the rows hits nothing.
    tap(&mut host, 580.0, 390.0);
    assert_eq!(host.ui.state.hits.len(), 1);

    // Moved, it is hit where it now stands; removed, not at all.
    let mut layer = host.ui.tree.get_mut(field).unwrap();
    assert!(layer.set_position("mob0", 300.0, 380.0));
    assert!(layer.set_facing("mob0", 90.0));
    assert!(layer.remove("mob1"));
    drop(layer);
    tap(&mut host, 300.0, 355.0);
    assert_eq!(host.ui.state.hits.last().map(|h| h.object.as_str()), Some("mob0"));
    assert_eq!(host.ui.tree.find::<SkiaScene3D>(field).unwrap().object_names().len(), 29);
}

#[test]
fn a_still_layer_asks_for_no_frames() {
    let mut host = host();
    let field = host.ui.state.field;
    {
        let mut layer = host.ui.tree.get_mut(field).unwrap();
        layer.add("a", "dragon", 100.0, 200.0, 80.0);
        layer.set_playing(false);
    }
    frames(&mut host, 0.2);
    assert!(!host.ui.needs_frame());
}
