//! A gallery of tiny 3D animations inside a DrawnUI screen (feature `model3d`): a loader, an
//! achievement, a celebration burst, a reward chest, an avatar, and a field of walking mobs drawn
//! by one 3D layer (`SkiaScene3D`). Every animation is made in the app from a free 3D model.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drawnui::prelude::*;

// The models: CC0, see assets/MODELS-LICENSE.txt.
const COIN: &[u8] = include_bytes!("../assets/coin.glb");
const TROPHY: &[u8] = include_bytes!("../assets/trophy.glb");
const GEM: &[u8] = include_bytes!("../assets/gem.glb");
const CHEST: &[u8] = include_bytes!("../assets/gift.glb");
const AVATAR: &[u8] = include_bytes!("../assets/avatar.glb");
const MOB: &[u8] = include_bytes!("../assets/mob.glb");

const BACKGROUND: Color = Color::from_rgb(20, 18, 28);
const CARD: Color = Color::from_rgb(34, 31, 46);
const LINE: Color = Color::from_rgb(52, 48, 68);
const TEXT: Color = Color::from_rgb(238, 234, 246);
const MUTED: Color = Color::from_rgb(168, 160, 186);
const ACCENT: Color = Color::from_rgb(255, 176, 64);

/// The field of mobs: its size in points.
const FIELD: (f32, f32) = (1000.0, 300.0);

/// A small random number generator: the same burst and crowd every run.
#[derive(Default)]
struct Dice(u32);

impl Dice {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// One piece of the celebration burst.
#[derive(Clone, Copy, Default)]
struct Piece {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    spin: f32,
    turn: f32,
}

/// A mob walking across the field.
#[derive(Clone, Copy, Default)]
struct Walker {
    x: f32,
    y: f32,
    speed: f32,
}

#[derive(Default)]
struct App {
    coin: Handle<SkiaModel3D>,
    paused: bool,
    trophy: Handle<SkiaModel3D>,
    unlocked: bool,
    chest: Handle<SkiaModel3D>,
    open: bool,
    avatar: Handle<SkiaModel3D>,
    burst: Handle<SkiaScene3D>,
    pieces: Vec<Piece>,
    /// Seconds until the next burst throws itself.
    next_burst: f32,
    field: Handle<SkiaScene3D>,
    walkers: Vec<Walker>,
    /// Mobs asked for; the field is filled again when it differs from what walks.
    mobs: usize,
    dice: Dice,
}

/// A gallery tile: a title, a line on what it shows, the 3D control, and its button.
fn tile(title: &str, caption: &str, model: impl IntoChildren, button: Build<SkiaButton>) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(300)
        .corner_radius(14)
        .background_color(CARD)
        .stroke_color(LINE)
        .stroke_width(1)
        .padding(16)
        .children((SkiaLayout::column().fill_x().spacing(8).children((
            SkiaLabel::new(title).font_size(18).text_color(TEXT),
            // Two lines for every caption: the tiles line up.
            SkiaLabel::new(caption).font_size(13).text_color(MUTED).height_request(36),
            SkiaLayout::new().fill_x().height_request(190).children(model),
            button.horizontal_options(LayoutOptions::Center),
        )),))
}

fn button(caption: &str) -> Build<SkiaButton> {
    SkiaButton::new(caption).width_request(150).background_color(Color::from_rgb(64, 58, 86))
}

/// Starts a burst: coins and gems thrown up from the bottom of the tile.
fn celebrate(app: &mut App, cx: &mut Cx) {
    let Some(mut burst) = cx.get_mut(app.burst) else { return };
    burst.clear();
    app.pieces.clear();
    for i in 0..18 {
        let d = &mut app.dice;
        let piece = Piece { x: 134.0, y: 185.0, vx: (d.next() - 0.5) * 420.0, vy: -250.0 - d.next() * 130.0, spin: d.next() * 360.0, turn: 200.0 + d.next() * 400.0 };
        burst.add(&format!("p{i}"), if i % 3 == 0 { "gem" } else { "coin" }, piece.x, piece.y, if i % 3 == 0 { 38.0 } else { 34.0 });
        app.pieces.push(piece);
    }
}

/// One frame of the burst and the walking mobs.
fn game_loop(app: &mut App, cx: &mut Cx, delta: f32) {
    // The celebration throws itself every few seconds, so the gallery is alive at rest.
    app.next_burst -= delta;
    if app.next_burst <= 0.0 {
        app.next_burst = 3.5;
        celebrate(app, cx);
    }
    if let Some(mut burst) = cx.get_mut(app.burst) {
        for (i, p) in app.pieces.iter_mut().enumerate() {
            (p.vy, p.x, p.y, p.spin) = (p.vy + 520.0 * delta, p.x + p.vx * delta, p.y + p.vy * delta, p.spin + p.turn * delta);
            // The floor of the tile: the pieces bounce and settle, spinning down.
            if p.y > 185.0 {
                (p.y, p.vy, p.vx, p.turn) = (185.0, -p.vy * 0.45, p.vx * 0.7, p.turn * 0.7);
            }
            // And its sides.
            if !(20.0..=248.0).contains(&p.x) {
                (p.x, p.vx) = (p.x.clamp(20.0, 248.0), -p.vx * 0.6);
            }
            let name = format!("p{i}");
            burst.set_position(&name, p.x, p.y);
            burst.set_facing(&name, p.spin);
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

fn build(app: &mut App) -> Build<SkiaLayout> {
    app.mobs = 40;
    app.dice = Dice(7);
    SkiaLayout::new().fill().children((
        SkiaScroll::new().fill().content(
            SkiaLayout::column().fill_x().padding(24).spacing(20).children((
                SkiaLabel::new("3D, made in the app").font_size(30).text_color(TEXT).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("Small animations from free 3D models, drawn inside a DrawnUI screen. Tap the buttons, the avatar and the mobs.")
                    .font_size(15)
                    .text_color(MUTED)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center),
                SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((
                    tile(
                        "Loader",
                        "A coin that spins while something loads.",
                        SkiaModel3D::new(COIN).view_angles(0.0, 10.0).zoom(1.1).auto_rotate(240.0).orbit(false).fill().assign(&mut app.coin),
                        button("Pause").observe(|me, app: &App| me.set_text(if app.paused { "Spin" } else { "Pause" })).on_tapped(|_me, app: &mut App, cx| {
                            app.paused = !app.paused;
                            if let Some(mut coin) = cx.get_mut(app.coin) {
                                coin.set_auto_rotate(if app.paused { 0.0 } else { 240.0 });
                            }
                        }),
                    ),
                    tile(
                        "Achievement",
                        "A trophy that pops in when unlocked.",
                        SkiaModel3D::new(TROPHY).view_angles(20.0, 12.0).auto_rotate(40.0).orbit(false).fill().assign(&mut app.trophy),
                        button("Unlock").on_tapped(|_me, app: &mut App, cx| {
                            app.unlocked = true;
                            cx.scale_to(app.trophy, 0.2, 0.2, 0, easing::linear);
                            cx.scale_to(app.trophy, 1.0, 1.0, 600, easing::spring_out);
                        }),
                    ),
                    tile(
                        "Celebration",
                        "Coins and gems thrown up after a win: one 3D layer, many pieces.",
                        SkiaScene3D::new().model("coin", COIN).model("gem", GEM).tilt(25.0).fill().assign(&mut app.burst),
                        button("Celebrate").on_tapped(|_me, app: &mut App, cx| {
                            app.next_burst = 3.5;
                            celebrate(app, cx);
                        }),
                    ),
                    tile(
                        "Reward",
                        "A chest that opens and closes.",
                        SkiaModel3D::new(CHEST).animation("Chest_Open").playing(false).view_angles(25.0, 18.0).orbit(false).fill().assign(&mut app.chest),
                        button("Open").observe(|me, app: &App| me.set_text(if app.open { "Close" } else { "Open" })).on_tapped(|_me, app: &mut App, cx| {
                            app.open = !app.open;
                            if let Some(mut chest) = cx.get_mut(app.chest) {
                                chest.hold(if app.open { "Chest_Open" } else { "Chest_Close" });
                            }
                        }),
                    ),
                    tile(
                        "Avatar",
                        "A character that idles, and waves when tapped.",
                        SkiaModel3D::new(AVATAR)
                            .animation("CharacterArmature|Idle")
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
                )),
                // The field of mobs: one 3D layer under the game loop.
                SkiaShape::new()
                    .corner_radius(14)
                    .background_color(Color::from_rgb(28, 40, 34))
                    .stroke_color(LINE)
                    .stroke_width(1)
                    .padding(16)
                    .horizontal_options(LayoutOptions::Center)
                    .children((SkiaLayout::column().spacing(8).width_request(FIELD.0).children((
                        SkiaLabel::new("Mobs on a 2D field").font_size(18).text_color(TEXT),
                        SkiaLabel::new("Many 3D monsters from one model, moved like sprites and drawn by one layer in one pass. Tap one to hit it.")
                            .font_size(13)
                            .text_color(MUTED),
                        DrawnGame::new()
                            .width_request(FIELD.0)
                            .height_request(FIELD.1)
                            .background_color(Color::from_rgb(52, 86, 60))
                            .start_loop(0.0)
                            .on_game_loop(|_me, app: &mut App, cx, delta| game_loop(app, cx, delta))
                            .children((SkiaScene3D::new()
                                .model("slime", MOB)
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
                                        .observe(move |me, app: &App| me.set_background_color(if app.mobs == n { ACCENT } else { Color::from_rgb(64, 58, 86) }))
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
    drawnui::run("DrawnUI 3D Gallery", || Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(BACKGROUND)));
}
