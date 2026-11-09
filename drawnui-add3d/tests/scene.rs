//! SkiaModel3D scene control (feature `model3d`): animations played once and back to the loop,
//! held, layered; taps that tell the node and material; nodes and materials by name.

use drawnui_add3d::prelude::*;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{MouseButton, PointerKind};

const DRAGON: &[u8] = include_bytes!("../../examples/model3d/assets/dragon.glb");
const IDLE: &str = "CharacterArmature|Flying_Idle";
const PUNCH: &str = "CharacterArmature|Punch";
const DEATH: &str = "CharacterArmature|Death";

#[derive(Default)]
struct App {
    boss: Handle<SkiaModel3D>,
    ended: Vec<String>,
    hits: Vec<ModelHit>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaModel3D::new(DRAGON)
            .animation(IDLE)
            .view_angles(0.0, 0.0)
            .fill()
            .assign(&mut app.boss)
            .on_animation_ended(|_me, app: &mut App, _cx, name| app.ended.push(name.to_owned()))
            .on_tapped_3d(|_me, app: &mut App, _cx, hit| app.hits.push(hit.clone())),))
    });
    let mut host = Headless::new(ui, 400, 400, 1.0);
    // The idle loop never settles: a few frames lay it out.
    for _ in 0..3 {
        host.frame_after(16.7);
    }
    host
}

fn frames(host: &mut Headless<App>, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        host.frame_after(16.7);
    }
}

/// A tap without settling (the idle loop never settles).
fn tap(host: &mut Headless<App>, x: f32, y: f32) {
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Down, MouseButton::Left, x, y, t);
    host.frame_after(16.7);
    let t = host.time_ms();
    host.ui.pointer_button(PointerKind::Up, MouseButton::Left, x, y, t);
    frames(host, 0.1);
}

fn current(host: &Headless<App>) -> String {
    host.ui.tree.find::<SkiaModel3D>(host.ui.state.boss).unwrap().current_animation().unwrap().to_owned()
}

#[test]
fn a_move_played_once_ends_and_goes_back_to_the_loop() {
    let mut host = host();
    assert_eq!(current(&host), IDLE);
    let boss = host.ui.state.boss;
    assert!(host.ui.tree.get_mut(boss).unwrap().play_once(PUNCH));
    assert!(!host.ui.tree.get_mut(boss).unwrap().play_once("no such move"));
    frames(&mut host, 0.3);
    assert_eq!(current(&host), PUNCH);
    assert!(host.ui.state.ended.is_empty());
    frames(&mut host, 3.0);
    assert_eq!(host.ui.state.ended, [PUNCH], "told once");
    assert_eq!(current(&host), IDLE, "back to idle");
    assert!(host.ui.needs_frame(), "idle keeps looping");
}

#[test]
fn a_held_move_stays_on_its_last_frame_and_frames_stop() {
    let mut host = host();
    let boss = host.ui.state.boss;
    assert!(host.ui.tree.get_mut(boss).unwrap().hold(DEATH));
    frames(&mut host, 4.0);
    assert_eq!(current(&host), DEATH);
    assert!(!host.ui.needs_frame(), "nothing moves any more");
}

#[test]
fn a_tap_on_the_boss_tells_the_node_and_material_a_tap_beside_it_nothing() {
    let mut host = host();
    tap(&mut host, 200.0, 200.0);
    let hit = host.ui.state.hits.last().cloned().expect("the boss was hit");
    assert!(!hit.node.is_empty() && !hit.material.is_empty(), "{hit:?}");
    tap(&mut host, 3.0, 3.0);
    assert_eq!(host.ui.state.hits.len(), 1, "the empty corner hits nothing");
    // A hidden material lets the ray through to what is behind it.
    let boss = host.ui.state.boss;
    assert!(host.ui.tree.get_mut(boss).unwrap().set_material_visible(&hit.material, false));
    tap(&mut host, 200.0, 200.0);
    assert!(host.ui.state.hits.iter().skip(1).all(|h| h.material != hit.material), "{:?}", host.ui.state.hits);
    // A hidden node is not hit at all.
    assert!(host.ui.tree.get_mut(boss).unwrap().set_node_visible(&hit.node, false));
    let before = host.ui.state.hits.len();
    tap(&mut host, 200.0, 200.0);
    assert_eq!(host.ui.state.hits.len(), before);
}

#[test]
fn nodes_and_materials_by_name() {
    let mut host = host();
    let boss = host.ui.state.boss;
    let (nodes, materials) = {
        let model = host.ui.tree.find::<SkiaModel3D>(boss).unwrap();
        (model.node_names(), model.material_names())
    };
    assert!(materials.iter().any(|m| m == "Dragon_Main"), "{materials:?}");
    let mut boss = host.ui.tree.get_mut(boss).unwrap();
    assert!(boss.set_material_color("Dragon_Main", Some(Color::from_rgb(40, 120, 220))));
    assert!(boss.set_material_color("Dragon_Main", None));
    assert!(!boss.set_material_color("Nothing", None));
    assert!(boss.set_node_translation(&nodes[0], [0.0, 1.0, 0.0]));
    assert!(boss.set_node_rotation(&nodes[0], [0.0, 90.0, 0.0]));
    assert!(boss.reset_node(&nodes[0]));
    assert!(!boss.set_node_visible("Nothing", false));
}

#[test]
fn an_image_goes_on_a_material_by_name() {
    let mut host = host();
    let boss = host.ui.state.boss;
    let mut surface = drawnui::skia::surfaces::raster_n32_premul((8, 4)).unwrap();
    surface.canvas().clear(Color::from_rgb(200, 30, 30));
    let drawn = Texture3D::from_image(&surface.image_snapshot()).expect("a raster image has pixels");
    assert_eq!(drawn.size(), (8, 4));
    assert!(Texture3D::decode(b"not an image").is_none());
    let mut boss = host.ui.tree.get_mut(boss).unwrap();
    assert!(boss.set_material_image("Dragon_Main", Some(drawn)));
    assert!(boss.set_material_image("Dragon_Main", None));
    assert!(!boss.set_material_image("Nothing", None));
}
