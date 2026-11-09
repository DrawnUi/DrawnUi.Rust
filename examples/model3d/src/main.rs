//! A 3D boss in a DrawnUI screen: a glTF model with its skeleton animations (SkiaModel3D, the
//! drawnui-add3d add-on), drawn with the same GPU context as the UI around it. The app drives the scene: moves
//! played once and back to idle, a tap on the boss that hits it, its skin and eyes recolored by
//! material name. Drag the dragon to turn it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drawnui::prelude::*;
use drawnui_add3d::prelude::*;

/// "Dragon Evolved" by Quaternius, CC0 (assets/dragon.glb.LICENSE.txt).
const DRAGON: &[u8] = include_bytes!("../assets/dragon.glb");

const IDLE: &str = "CharacterArmature|Flying_Idle";
const HIT: &str = "CharacterArmature|HitReact";

/// How a move plays.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Loops until another move.
    Loop,
    /// Plays once, then back to the loop.
    Once,
    /// Plays once and stays on its last frame.
    Hold,
}

/// The boss's moves: the caption, the glTF animation and how it plays, in the order of a fight.
const MOVES: [(&str, &str, Kind); 7] = [
    ("Idle", IDLE, Kind::Loop),
    ("Fly", "CharacterArmature|Fast_Flying", Kind::Loop),
    ("Punch", "CharacterArmature|Punch", Kind::Once),
    ("Headbutt", "CharacterArmature|Headbutt", Kind::Once),
    ("Yes", "CharacterArmature|Yes", Kind::Once),
    ("No", "CharacterArmature|No", Kind::Once),
    ("Death", "CharacterArmature|Death", Kind::Hold),
];

/// Skin colors for the dragon's main material; `None` = the file's own.
const SKINS: [(&str, Option<Color>); 4] = [
    ("Ember", None),
    ("Frost", Some(Color::from_rgb(70, 150, 230))),
    ("Venom", Some(Color::from_rgb(90, 190, 80))),
    ("Shadow", Some(Color::from_rgb(70, 60, 90))),
];

const ACCENT: Color = Color::from_rgb(255, 122, 69);
const BUTTON: Color = Color::from_rgb(44, 40, 52);
const MUTED: Color = Color::from_rgb(190, 180, 200);

#[derive(Default)]
struct App {
    boss: Handle<SkiaModel3D>,
    /// The looping move, by caption.
    looping: &'static str,
    skin: &'static str,
    rage: bool,
    turning: bool,
    /// What just happened, for the status line.
    news: String,
}

fn caption_of(animation: &str) -> &'static str {
    MOVES.iter().find(|m| m.1 == animation).map_or(if animation == HIT { "Hit" } else { "Move" }, |m| m.0)
}

fn move_button((caption, animation, kind): (&'static str, &'static str, Kind)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .width_request(100)
        .background_color(BUTTON)
        .observe(move |me, app: &App| me.set_background_color(if app.looping == caption { ACCENT } else { BUTTON }))
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut boss) = cx.get_mut(app.boss) {
                match kind {
                    Kind::Loop => {
                        boss.play(animation);
                        app.looping = caption;
                    }
                    Kind::Once => {
                        boss.play_once(animation);
                    }
                    Kind::Hold => {
                        boss.hold(animation);
                        app.looping = caption;
                    }
                }
            }
            app.news = format!("{caption}…");
        })
}

fn skin_button((caption, color): (&'static str, Option<Color>)) -> Build<SkiaButton> {
    let swatch = color.unwrap_or(Color::from_rgb(190, 70, 30));
    SkiaButton::new(caption)
        .width_request(100)
        .background_color(BUTTON)
        .stroke_color(swatch)
        .stroke_width(2)
        .observe(move |me, app: &App| me.set_background_color(if app.skin == caption { swatch } else { BUTTON }))
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut boss) = cx.get_mut(app.boss) {
                boss.set_material_color("Dragon_Main", color);
            }
            app.skin = caption;
        })
}

fn label(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(13).text_color(MUTED).horizontal_options(LayoutOptions::Center)
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    (app.looping, app.skin) = ("Idle", "Ember");
    app.news = "Tap the dragon to hit it.".into();
    SkiaLayout::new().fill().children((
        // The arena: a warm light behind the boss.
        SkiaShape::new().fill().background_color(Color::from_rgb(18, 14, 22)).fill_gradient(SkiaGradient {
            start_x_ratio: 0.5,
            start_y_ratio: 0.42,
            ..SkiaGradient::new(GradientType::Circular, vec![Color::from_rgb(92, 38, 30), Color::from_rgb(18, 14, 22)])
        }),
        SkiaLayout::column().fill().padding(20).spacing(10).children((
            SkiaLabel::new("BOSS FIGHT").font_size(30).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
            SkiaLabel::new("A glTF model drawn in 3D inside DrawnUI, driven by the app. Drag it to turn it, tap it to hit it.")
                .font_size(15)
                .text_color(MUTED)
                .horizontal_text_alignment(TextAlignment::Center)
                .horizontal_options(LayoutOptions::Center),
            SkiaModel3D::new(DRAGON)
                .animation(IDLE)
                .view_angles(30.0, 8.0)
                .zoom(0.75)
                .fill_x()
                .vertical_options(LayoutOptions::Fill)
                .assign(&mut app.boss)
                .on_tapped_3d(|me, app: &mut App, _cx, hit| {
                    // A hit plays once and goes back to the loop; a dead dragon does not react.
                    if app.looping != "Death" {
                        me.play_once(HIT);
                    }
                    app.news = format!("Hit on {}", hit.material.replace('_', " "));
                })
                .on_animation_ended(|_me, app: &mut App, _cx, name| app.news = format!("{} finished", caption_of(name))),
            SkiaLabel::new("")
                .font_size(15)
                .text_color(Color::WHITE)
                .horizontal_options(LayoutOptions::Center)
                .observe(|me, app: &App| me.set_text(app.news.clone())),
            label("MOVES"),
            SkiaLayout::wrap().spacing(8).horizontal_options(LayoutOptions::Center).children(MOVES.iter().copied().map(move_button).collect::<Vec<_>>()),
            label("SKIN AND EYES (materials by name)"),
            SkiaLayout::wrap().spacing(8).horizontal_options(LayoutOptions::Center).children((
                SKINS.iter().copied().map(skin_button).collect::<Vec<_>>(),
                SkiaButton::new("Rage: off")
                    .width_request(124)
                    .background_color(Color::from_rgb(60, 54, 72))
                    .observe(|me, app: &App| {
                        me.set_text(if app.rage { "Rage: on" } else { "Rage: off" });
                        me.set_background_color(if app.rage { Color::from_rgb(200, 30, 40) } else { Color::from_rgb(60, 54, 72) });
                    })
                    .on_tapped(|_me, app: &mut App, cx| {
                        app.rage = !app.rage;
                        if let Some(mut boss) = cx.get_mut(app.boss) {
                            boss.set_material_color("Eye_White", app.rage.then_some(Color::from_rgb(255, 40, 30)));
                        }
                    }),
                SkiaButton::new("Turn: off")
                    .width_request(124)
                    .background_color(Color::from_rgb(60, 54, 72))
                    .observe(|me, app: &App| me.set_text(if app.turning { "Turn: on" } else { "Turn: off" }))
                    .on_tapped(|_me, app: &mut App, cx| {
                        app.turning = !app.turning;
                        if let Some(mut boss) = cx.get_mut(app.boss) {
                            boss.set_auto_rotate(if app.turning { 30.0 } else { 0.0 });
                        }
                    }),
            )),
        )),
    ))
}

fn main() {
    drawnui::run("DrawnUI 3D", || {
        Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(Color::from_rgb(18, 14, 22)))
    });
}
