//! A gallery of tiny 3D animations inside a DrawnUI screen (feature `model3d`).
//! - Made in code, flat: a coin and bouncing dots for loading, a medal for an achievement, a check
//!   for success, a heart for a like, falling confetti; smooth shapes in flat colors with outlines,
//!   animated by the app.
//! - From model files: an avatar, a reward chest, and a field of walking mobs drawn by one 3D layer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::f32::consts::TAU;

use drawnui::prelude::*;
use drawnui::skia::{Path, PathBuilder};

// Model files: CC0, see assets/MODELS-LICENSE.txt.
const CHEST: &[u8] = include_bytes!("../assets/gift.glb");
const AVATAR: &[u8] = include_bytes!("../assets/avatar.glb");
const MOB: &[u8] = include_bytes!("../assets/mob.glb");

const PAGE: Color = Color::from_rgb(246, 244, 251);
const CARD: Color = Color::WHITE;
const LINE: Color = Color::from_rgb(228, 224, 240);
const INK: Color = Color::from_rgb(34, 30, 52);
const MUTED: Color = Color::from_rgb(110, 104, 130);
const BUTTON: Color = Color::from_rgb(92, 76, 230);
/// The illustrations' outline.
const OUTLINE: Color = Color::from_rgb(43, 37, 64);
const GOLD: Color = Color::from_rgb(255, 196, 61);
const GOLD_LIGHT: Color = Color::from_rgb(255, 222, 122);
const GREY: Color = Color::from_rgb(196, 192, 206);
const CONFETTI: [Color; 5] = [
    Color::from_rgb(255, 99, 132),
    Color::from_rgb(255, 196, 61),
    Color::from_rgb(76, 201, 140),
    Color::from_rgb(92, 140, 255),
    Color::from_rgb(178, 102, 255),
];

/// The confetti tile and the mobs' field, in points.
const RAIN: (f32, f32) = (268.0, 190.0);
const FIELD: (f32, f32) = (1000.0, 280.0);

// ---------------------------------------------------------------- shapes made in code

fn star(points: usize, outer: f32, inner: f32) -> Path {
    let mut b = PathBuilder::new();
    for i in 0..points * 2 {
        let (r, a) = (if i % 2 == 0 { outer } else { inner }, i as f32 * TAU / (points * 2) as f32 - TAU / 4.0);
        let p = (r * a.cos(), r * a.sin());
        if i == 0 { b.move_to(p) } else { b.line_to(p) };
    }
    b.close();
    b.detach()
}

fn heart() -> Path {
    let mut b = PathBuilder::new();
    b.move_to((50.0, 88.0));
    b.cubic_to((20.0, 65.0), (0.0, 45.0), (0.0, 28.0));
    b.cubic_to((0.0, 12.0), (12.0, 0.0), (27.0, 0.0));
    b.cubic_to((38.0, 0.0), (46.0, 7.0), (50.0, 15.0));
    b.cubic_to((54.0, 7.0), (62.0, 0.0), (73.0, 0.0));
    b.cubic_to((88.0, 0.0), (100.0, 12.0), (100.0, 28.0));
    b.cubic_to((100.0, 45.0), (80.0, 65.0), (50.0, 88.0));
    b.close();
    b.detach()
}

fn check() -> Path {
    let mut b = PathBuilder::new();
    b.move_to((8.0, 52.0));
    for p in [(22.0, 38.0), (40.0, 56.0), (78.0, 18.0), (92.0, 32.0), (40.0, 84.0)] {
        b.line_to(p);
    }
    b.close();
    b.detach()
}

/// A gold coin standing up, a star on each face.
fn coin() -> Model3D {
    Model3D::new()
        .part("rim", Mesh3D::cylinder(1.0, 0.22), GOLD)
        .turned(90.0, 0.0, 0.0)
        .part("face", Mesh3D::cylinder(0.78, 0.34), GOLD_LIGHT)
        .inside("rim")
        .part("star", Mesh3D::extrude(&star(5, 0.5, 0.21), 0.06), GOLD)
        .inside("rim")
        .turned(-90.0, 0.0, 0.0)
        .at(0.0, 0.19, 0.0)
        .part("star_back", Mesh3D::extrude(&star(5, 0.5, 0.21), 0.06), GOLD)
        .inside("rim")
        .turned(90.0, 0.0, 0.0)
        .at(0.0, -0.19, 0.0)
}

/// Three dots in a row: "d0", "d1", "d2".
fn dots() -> Model3D {
    let colors = [Color::from_rgb(92, 76, 230), Color::from_rgb(255, 99, 132), Color::from_rgb(255, 196, 61)];
    (0..3).fold(Model3D::new(), |m, i| m.part(format!("d{i}"), Mesh3D::sphere(0.3), colors[i]).at((i as f32 - 1.0) * 0.9, 0.0, 0.0))
}

/// A medal on two ribbons.
fn medal() -> Model3D {
    Model3D::new()
        .part("ribbon_left", Mesh3D::cuboid(0.55, 1.5, 0.06), Color::from_rgb(255, 99, 132))
        .at(-0.38, 1.05, -0.12)
        .turned(0.0, 0.0, 22.0)
        .part("ribbon_right", Mesh3D::cuboid(0.55, 1.5, 0.06), Color::from_rgb(92, 140, 255))
        .at(0.38, 1.05, -0.12)
        .turned(0.0, 0.0, -22.0)
        .part("disc", Mesh3D::cylinder(1.0, 0.22), GOLD)
        .turned(90.0, 0.0, 0.0)
        .part("inner", Mesh3D::cylinder(0.76, 0.34), GOLD_LIGHT)
        .turned(90.0, 0.0, 0.0)
        .part("star", Mesh3D::extrude(&star(5, 0.52, 0.22), 0.08), Color::WHITE)
        .at(0.0, 0.0, 0.2)
}

/// A green badge with a white check.
fn badge() -> Model3D {
    Model3D::new()
        .part("badge", Mesh3D::cylinder(1.0, 0.3), Color::from_rgb(76, 201, 140))
        .turned(90.0, 0.0, 0.0)
        .part("check", Mesh3D::extrude(&check(), 14.0), Color::WHITE)
        .scaled(0.013, 0.013, 0.013)
        .at(0.0, 0.0, 0.2)
}

fn heart_model() -> Model3D {
    Model3D::new().part("heart", Mesh3D::extrude(&heart(), 22.0), Color::from_rgb(255, 77, 109))
}

/// A strip of paper and a star for the confetti; the part to recolor is "piece".
fn paper() -> Model3D {
    Model3D::new().part("piece", Mesh3D::cuboid(1.0, 0.45, 0.06), Color::WHITE)
}

fn confetti_star() -> Model3D {
    Model3D::new().part("piece", Mesh3D::extrude(&star(5, 0.6, 0.26), 0.12), Color::WHITE)
}

// ---------------------------------------------------------------- the app

/// A small random number generator: the same confetti and crowd every run.
#[derive(Default)]
struct Dice(u32);

impl Dice {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Clone, Copy, Default)]
struct Piece {
    x: f32,
    y: f32,
    fall: f32,
    sway: f32,
    spin: f32,
    facing: f32,
}

#[derive(Clone, Copy, Default)]
struct Walker {
    x: f32,
    y: f32,
    speed: f32,
}

#[derive(Default)]
struct App {
    time: f32,
    coin: Handle<SkiaModel3D>,
    paused: bool,
    dots: Handle<SkiaModel3D>,
    /// The dots' own clock: it stops while they wait.
    bounce: f32,
    resting: bool,
    medal: Handle<SkiaModel3D>,
    unlocked: bool,
    /// The medal's spin after unlocking, 0 to 1.
    spin: f32,
    badge: Handle<SkiaModel3D>,
    /// The check's growth after "Done", 0 to 1.
    grow: f32,
    heart: Handle<SkiaModel3D>,
    liked: bool,
    /// The heart's pop after a like, 1 to 0.
    pop: f32,
    rain: Handle<SkiaScene3D>,
    pieces: Vec<Piece>,
    chest: Handle<SkiaModel3D>,
    open: bool,
    avatar: Handle<SkiaModel3D>,
    field: Handle<SkiaScene3D>,
    walkers: Vec<Walker>,
    mobs: usize,
    dice: Dice,
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn tile(title: &str, caption: &str, model: impl IntoChildren, button: Build<SkiaButton>) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(300)
        .corner_radius(16)
        .background_color(CARD)
        .stroke_color(LINE)
        .stroke_width(1)
        .padding(16)
        .children((SkiaLayout::column().fill_x().spacing(8).children((
            SkiaLabel::new(title).font_size(18).text_color(INK),
            SkiaLabel::new(caption).font_size(13).text_color(MUTED).height_request(36),
            SkiaLayout::new().fill_x().height_request(RAIN.1).children(model),
            button.horizontal_options(LayoutOptions::Center),
        )),))
}

fn button(caption: &str) -> Build<SkiaButton> {
    SkiaButton::new(caption).width_request(150).background_color(BUTTON)
}

/// A flat illustration: flat colors, a dark outline, still unless the app moves it.
fn flat(model: &Model3D) -> Build<SkiaModel3D> {
    SkiaModel3D::from_model(model).look(Look::Flat).outline(2.5, OUTLINE).orbit(false).view_angles(0.0, 0.0).fill()
}

fn section(title: &str, caption: &str) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(4).horizontal_options(LayoutOptions::Center).children((
        SkiaLabel::new(title).font_size(22).text_color(INK).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new(caption).font_size(14).text_color(MUTED).horizontal_options(LayoutOptions::Center),
    ))
}

fn game_loop(app: &mut App, cx: &mut Cx, delta: f32) {
    app.time += delta;
    let t = app.time;
    if !app.resting {
        app.bounce += delta;
    }
    if let Some(mut dots) = cx.get_mut(app.dots) {
        for i in 0..3 {
            let y = if app.resting { 0.0 } else { 0.5 * (app.bounce * 7.0 - i as f32 * 0.8).sin().max(0.0) };
            dots.set_node_translation(&format!("d{i}"), [(i as f32 - 1.0) * 0.9, y, 0.0]);
        }
    }
    if app.spin < 1.0 {
        app.spin = (app.spin + delta / 0.9).min(1.0);
        if let Some(mut medal) = cx.get_mut(app.medal) {
            medal.set_view_angles(360.0 * ease_out(app.spin), 0.0);
        }
    }
    if app.grow < 1.0 {
        app.grow = (app.grow + delta / 0.35).min(1.0);
        if let Some(mut badge) = cx.get_mut(app.badge) {
            let s = 0.013 * ease_out(app.grow);
            badge.set_node_scale("check", [s, s, s]);
        }
    }
    app.pop = (app.pop - delta * 3.0).max(0.0);
    if let Some(mut heart) = cx.get_mut(app.heart) {
        let beat = 1.0 + 0.05 * (t * 5.0).sin().max(0.0) + 0.35 * app.pop;
        heart.set_node_scale("heart", [beat, beat, 1.0]);
    }
    if app.pieces.is_empty()
        && let Some(mut rain) = cx.get_mut(app.rain)
    {
        for i in 0..28 {
            let d = &mut app.dice;
            let p = Piece { x: d.next() * RAIN.0, y: d.next() * RAIN.1, fall: 40.0 + d.next() * 50.0, sway: d.next() * TAU, spin: 120.0 + d.next() * 300.0, facing: d.next() * 360.0 };
            let name = format!("c{i}");
            rain.add(&name, if i % 4 == 0 { "star" } else { "paper" }, p.x, p.y, if i % 4 == 0 { 18.0 } else { 11.0 });
            rain.set_material_color(&name, "piece", Some(CONFETTI[i % CONFETTI.len()]));
            app.pieces.push(p);
        }
    }
    if let Some(mut rain) = cx.get_mut(app.rain) {
        for (i, p) in app.pieces.iter_mut().enumerate() {
            (p.y, p.facing) = (p.y + p.fall * delta, p.facing + p.spin * delta);
            if p.y > RAIN.1 + 16.0 {
                p.y = -12.0;
            }
            let name = format!("c{i}");
            rain.set_position(&name, p.x + (t * 2.0 + p.sway).sin() * 10.0, p.y);
            rain.set_facing(&name, p.facing);
        }
    }
    if app.walkers.len() != app.mobs
        && let Some(mut field) = cx.get_mut(app.field)
    {
        field.clear();
        app.walkers.clear();
        for i in 0..app.mobs {
            let d = &mut app.dice;
            let w = Walker { x: d.next() * FIELD.0, y: 70.0 + d.next() * (FIELD.1 - 80.0), speed: (if d.next() < 0.5 { -1.0 } else { 1.0 }) * (30.0 + d.next() * 50.0) };
            let name = format!("m{i}");
            field.add(&name, "slime", w.x, w.y, 46.0);
            field.play(&name, "CharacterArmature|Walk");
            field.set_speed(&name, 0.8 + d.next() * 0.5);
            field.set_facing(&name, if w.speed > 0.0 { 90.0 } else { -90.0 });
            app.walkers.push(w);
        }
    }
    if let Some(mut field) = cx.get_mut(app.field) {
        for (i, w) in app.walkers.iter_mut().enumerate() {
            w.x += w.speed * delta;
            if w.x < -30.0 || w.x > FIELD.0 + 30.0 {
                w.speed = -w.speed;
                field.set_facing(&format!("m{i}"), if w.speed > 0.0 { 90.0 } else { -90.0 });
            }
            field.set_position(&format!("m{i}"), w.x, w.y);
        }
    }
}

fn medal_colors(app: &App, cx: &mut Cx) {
    if let Some(mut medal) = cx.get_mut(app.medal) {
        for (part, color) in [("disc", GREY), ("inner", Color::from_rgb(222, 219, 230)), ("star", Color::from_rgb(244, 242, 248))] {
            medal.set_material_color(part, (!app.unlocked).then_some(color));
        }
    }
}

fn build(app: &mut App) -> Build<DrawnGame> {
    (app.mobs, app.dice, app.spin, app.grow) = (40, Dice(7), 1.0, 1.0);
    DrawnGame::new().fill().start_loop(0.0).on_game_loop(|_me, app: &mut App, cx, delta| game_loop(app, cx, delta)).children((
        SkiaScroll::new().fill().content(
            SkiaLayout::column().fill_x().padding(24).spacing(20).children((
                SkiaLabel::new("3D, made in the app").font_size(30).text_color(INK).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("3D pieces and characters built and animated inside a DrawnUI screen.")
                    .font_size(15)
                    .text_color(MUTED)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center),
                section("Made in code", "Smooth shapes drawn flat with an outline: the drawn look for 3D pieces, a few lines each."),
                SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((
                    tile(
                        "Loading",
                        "A coin turning while something loads.",
                        flat(&coin()).auto_rotate(200.0).assign(&mut app.coin),
                        button("Pause").observe(|me, app: &App| me.set_text(if app.paused { "Spin" } else { "Pause" })).on_tapped(|_me, app: &mut App, cx| {
                            app.paused = !app.paused;
                            if let Some(mut coin) = cx.get_mut(app.coin) {
                                coin.set_auto_rotate(if app.paused { 0.0 } else { 200.0 });
                            }
                        }),
                    ),
                    tile(
                        "Loading dots",
                        "Three dots that bounce one after another.",
                        flat(&dots()).assign(&mut app.dots),
                        button("Pause").observe(|me, app: &App| me.set_text(if app.resting { "Bounce" } else { "Pause" })).on_tapped(|_me, app: &mut App, _cx| app.resting = !app.resting),
                    ),
                    tile(
                        "Achievement",
                        "A medal that lights up and spins when unlocked.",
                        flat(&medal()).assign(&mut app.medal).on_tapped_3d(|_me, app: &mut App, cx, _hit| {
                            app.spin = 0.0;
                            medal_colors(app, cx);
                        }),
                        button("Unlock").observe(|me, app: &App| me.set_text(if app.unlocked { "Lock" } else { "Unlock" })).on_tapped(|_me, app: &mut App, cx| {
                            app.unlocked = !app.unlocked;
                            medal_colors(app, cx);
                            if app.unlocked {
                                app.spin = 0.0;
                                cx.scale_to(app.medal, 0.6, 0.6, 0, easing::linear);
                                cx.scale_to(app.medal, 1.0, 1.0, 500, easing::spring_out);
                            }
                        }),
                    ),
                    tile(
                        "Success",
                        "A badge that pops in, then its check grows.",
                        flat(&badge()).assign(&mut app.badge),
                        button("Done").on_tapped(|_me, app: &mut App, cx| {
                            app.grow = 0.0;
                            if let Some(mut badge) = cx.get_mut(app.badge) {
                                badge.set_node_scale("check", [0.0, 0.0, 0.0]);
                            }
                            cx.scale_to(app.badge, 0.3, 0.3, 0, easing::linear);
                            cx.scale_to(app.badge, 1.0, 1.0, 450, easing::spring_out);
                        }),
                    ),
                    tile(
                        "Like",
                        "A heart that beats, and jumps when liked.",
                        flat(&heart_model()).assign(&mut app.heart).on_tapped_3d(|me, app: &mut App, _cx, _hit| {
                            app.liked = !app.liked;
                            app.pop = if app.liked { 1.0 } else { 0.0 };
                            me.set_material_color("heart", (!app.liked).then_some(GREY));
                        }),
                        button("Like").observe(|me, app: &App| me.set_text(if app.liked { "Unlike" } else { "Like" })).on_tapped(|_me, app: &mut App, cx| {
                            app.liked = !app.liked;
                            app.pop = if app.liked { 1.0 } else { 0.0 };
                            if let Some(mut heart) = cx.get_mut(app.heart) {
                                heart.set_material_color("heart", (!app.liked).then_some(GREY));
                            }
                        }),
                    ),
                    tile(
                        "Celebration",
                        "Confetti and stars falling: one 3D layer, many pieces.",
                        SkiaScene3D::new()
                            .model_from("paper", &paper())
                            .model_from("star", &confetti_star())
                            .look(Look::Flat)
                            .outline(1.5, OUTLINE)
                            .tilt(20.0)
                            .fill()
                            .assign(&mut app.rain),
                        button("Hooray").on_tapped(|_me, app: &mut App, _cx| {
                            // A fresh shower: every piece starts again above the tile.
                            for p in &mut app.pieces {
                                p.y = -12.0 - app.dice.next() * RAIN.1;
                            }
                        }),
                    ),
                )),
                section("From model files", "Free 3D models with their own animations, driven by the app."),
                SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((
                    tile(
                        "Avatar",
                        "A character that idles, and waves when tapped.",
                        SkiaModel3D::new(AVATAR)
                            .animation("CharacterArmature|Idle")
                            .look(Look::Toon)
                            .outline(2.0, OUTLINE)
                            .view_angles(15.0, 5.0)
                            .fill()
                            .assign(&mut app.avatar)
                            .on_tapped_3d(|me, _app: &mut App, _cx, _hit| {
                                me.play_once("CharacterArmature|Wave");
                            }),
                        button("Wave").on_tapped(|_me, app: &mut App, cx| {
                            if let Some(mut avatar) = cx.get_mut(app.avatar) {
                                avatar.play_once("CharacterArmature|Wave");
                            }
                        }),
                    ),
                    tile(
                        "Reward",
                        "A chest that opens and closes.",
                        SkiaModel3D::new(CHEST)
                            .animation("Chest_Open")
                            .playing(false)
                            .look(Look::Toon)
                            .outline(2.0, OUTLINE)
                            .view_angles(25.0, 18.0)
                            .orbit(false)
                            .fill()
                            .assign(&mut app.chest),
                        button("Open").observe(|me, app: &App| me.set_text(if app.open { "Close" } else { "Open" })).on_tapped(|_me, app: &mut App, cx| {
                            app.open = !app.open;
                            if let Some(mut chest) = cx.get_mut(app.chest) {
                                chest.hold(if app.open { "Chest_Open" } else { "Chest_Close" });
                            }
                        }),
                    ),
                )),
                SkiaShape::new()
                    .corner_radius(16)
                    .background_color(CARD)
                    .stroke_color(LINE)
                    .stroke_width(1)
                    .padding(16)
                    .horizontal_options(LayoutOptions::Center)
                    .children((SkiaLayout::column().spacing(8).width_request(FIELD.0).children((
                        SkiaLabel::new("Mobs on a 2D field").font_size(18).text_color(INK),
                        SkiaLabel::new("Many 3D monsters from one model, moved like sprites and drawn by one layer in one pass. Tap one to hit it.")
                            .font_size(13)
                            .text_color(MUTED),
                        SkiaShape::new()
                            .width_request(FIELD.0)
                            .height_request(FIELD.1)
                            .corner_radius(10)
                            .background_color(Color::from_rgb(208, 236, 214))
                            .children((SkiaScene3D::new()
                                .model("slime", MOB)
                                .look(Look::Toon)
                                .outline(1.5, OUTLINE)
                                .tilt(30.0)
                                .fill()
                                .assign(&mut app.field)
                                .on_tapped_3d(|me, _app: &mut App, _cx, hit| {
                                    me.play_once(&hit.object, "CharacterArmature|HitRecieve");
                                }),)),
                        SkiaLayout::row().spacing(8).children(
                            [10usize, 40, 100]
                                .into_iter()
                                .map(|n| {
                                    SkiaButton::new(format!("{n} mobs"))
                                        .width_request(110)
                                        .observe(move |me, app: &App| me.set_background_color(if app.mobs == n { BUTTON } else { Color::from_rgb(150, 142, 190) }))
                                        .on_tapped(move |_me, app: &mut App, _cx| app.mobs = n)
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )),)),
            )),
        ),
        SkiaLabelFps::new()
            .vertical_options(LayoutOptions::End)
            .horizontal_options(LayoutOptions::End)
            .margin((0, 0, 8, 8))
            .background_color(Color::from_rgb(139, 0, 0))
            .text_color(Color::WHITE),
    ))
}

fn main() {
    drawnui::run("DrawnUI 3D Gallery", || Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(PAGE)));
}
