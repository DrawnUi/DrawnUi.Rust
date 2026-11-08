//! A 3D boss in a DrawnUI screen: a glTF model with its skeleton animations (SkiaModel3D, feature
//! `model3d`), drawn with the same GPU context as the UI around it. Drag the dragon to turn it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drawnui::prelude::*;

/// "Dragon Evolved" by Quaternius, CC0 (assets/dragon.glb.LICENSE.txt).
const DRAGON: &[u8] = include_bytes!("../assets/dragon.glb");

/// The boss's moves: the button caption and the glTF animation, in the order of a fight.
const MOVES: [(&str, &str); 8] = [
    ("Idle", "CharacterArmature|Flying_Idle"),
    ("Fly", "CharacterArmature|Fast_Flying"),
    ("Punch", "CharacterArmature|Punch"),
    ("Headbutt", "CharacterArmature|Headbutt"),
    ("Hit", "CharacterArmature|HitReact"),
    ("Death", "CharacterArmature|Death"),
    ("Yes", "CharacterArmature|Yes"),
    ("No", "CharacterArmature|No"),
];

const ACCENT: Color = Color::from_rgb(255, 122, 69);

#[derive(Default)]
struct App {
    boss: Handle<SkiaModel3D>,
    playing: &'static str,
    turning: bool,
}

fn move_button((caption, animation): (&'static str, &'static str)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .width_request(104)
        .background_color(Color::from_rgb(44, 40, 52))
        .observe(move |me, app: &App| me.set_background_color(if app.playing == caption { ACCENT } else { Color::from_rgb(44, 40, 52) }))
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut boss) = cx.get_mut(app.boss) {
                boss.play(animation);
            }
            app.playing = caption;
        })
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    app.playing = "Idle";
    SkiaLayout::new().fill().children((
        // The arena: a warm light behind the boss.
        SkiaShape::new().fill().background_color(Color::from_rgb(18, 14, 22)).fill_gradient(
            SkiaGradient { start_x_ratio: 0.5, start_y_ratio: 0.45, ..SkiaGradient::new(GradientType::Circular, vec![Color::from_rgb(92, 38, 30), Color::from_rgb(18, 14, 22)]) },
        ),
        SkiaLayout::column().fill().padding(24).spacing(12).children((
            SkiaLabel::new("BOSS FIGHT").font_size(30).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
            SkiaLabel::new("A glTF model with its skeleton animations, drawn in 3D inside DrawnUI. Drag it to turn it.")
                .font_size(15)
                .text_color(Color::from_rgb(190, 180, 200))
                .horizontal_text_alignment(TextAlignment::Center)
                .horizontal_options(LayoutOptions::Center),
            SkiaModel3D::new(DRAGON)
                .animation(MOVES[0].1)
                .view_angles(30.0, 8.0)
                .zoom(0.75)
                .fill_x()
                .vertical_options(LayoutOptions::Fill)
                .assign(&mut app.boss),
            SkiaLayout::wrap()
                .spacing(10)
                .horizontal_options(LayoutOptions::Center)
                .children(MOVES.iter().copied().map(move_button).collect::<Vec<_>>()),
            SkiaButton::new("Turn by itself: off")
                .horizontal_options(LayoutOptions::Center)
                .background_color(Color::from_rgb(60, 54, 72))
                .observe(|me, app: &App| me.set_text(if app.turning { "Turn by itself: on" } else { "Turn by itself: off" }))
                .on_tapped(|_me, app: &mut App, cx| {
                    app.turning = !app.turning;
                    if let Some(mut boss) = cx.get_mut(app.boss) {
                        boss.set_auto_rotate(if app.turning { 30.0 } else { 0.0 });
                    }
                }),
        )),
    ))
}

fn main() {
    drawnui::run("DrawnUI 3D", || {
        Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(Color::from_rgb(18, 14, 22)))
    });
}
