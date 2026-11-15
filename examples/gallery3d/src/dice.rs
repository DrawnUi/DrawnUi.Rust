//! A small game (the .NET gallery's GalleryPage.Dice): two dice made in code, a rounded cube
//! wearing its six faces in one image (pips drawn with Skia), that tumble, hop and land on a number
//! on green casino felt. Flick them sideways, tap them or press Roll.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::skia::{Canvas, Point, gradient, gradient::Interpolation, Color4f, TileMode};
use drawnui_add3d::prelude::*;
use glam::{Quat, Vec3};

use crate::theme::*;
use crate::{App, fill, painted, panel};

/// Each die's turn now (x, z) and the turn of its holder around the vertical (yaw), degrees.
type Pose = (f32, f32, f32);

#[derive(Default)]
pub struct DiceState {
    view: Handle<SkiaModel3D>,
    result: Handle<SkiaLabel>,
    /// The dice as they stand, shared with the roll under way.
    poses: Rc<RefCell<[Pose; 2]>>,
    /// The images are on and the dice placed.
    dressed: bool,
    roll: Option<AnimationId>,
    random: crate::Random,
}

const DICE_HEIGHT: f32 = 280.0;
const ROLL_MS: f32 = 1700.0;
/// The part of a roll spent tumbling; the rest is the die settling on the face it landed on, on a spring.
const TUMBLING: f32 = 0.8;
const ROUNDING: f32 = 0.14;
const DIE_X: [f32; 2] = [-0.8, 0.8];
/// The shadows lie a little toward the viewer and right: away from the key light, up and to the left.
const SHADOW_SHIFT: (f32, f32) = (0.12, 0.15);

/// The turn (x, z) that puts a face on top: 1 starts on top, 6 under it, 2 in front, 5 behind,
/// 3 right, 4 left.
const FACE_UP: [(f32, f32); 6] = [(0.0, 0.0), (-90.0, 0.0), (0.0, 90.0), (0.0, -90.0), (90.0, 0.0), (180.0, 0.0)];

/// A die's faces in one image, one 256 px cell each, in the order `Mesh3D::rounded_cuboid` lays
/// an atlas: +X, -X, +Y on top; -Y, +Z, -Z below.
const ATLAS_FACES: [usize; 6] = [3, 4, 1, 6, 2, 5];

fn die_faces() -> Texture3D {
    painted(768, 512, |c, _, _| {
        c.clear(Color::from_rgb(250, 246, 236));
        for (f, value) in ATLAS_FACES.iter().enumerate() {
            c.save();
            c.translate(((f % 3 * 256) as f32, (f / 3 * 256) as f32));
            pips(c, *value);
            c.restore();
        }
    })
}

/// One face's pips on a 256 px square: drilled into the face, darker inside, a light lip at the
/// lower edge.
fn pips(c: &Canvas, value: usize) {
    // Where the pips go on a 3 x 3 grid, as on a real die.
    let cells: &[usize] = match value {
        1 => &[4],
        2 => &[0, 8],
        3 => &[0, 4, 8],
        4 => &[0, 2, 6, 8],
        5 => &[0, 2, 4, 6, 8],
        _ => &[0, 2, 3, 5, 6, 8],
    };
    let radius = if value == 1 { 34.0 } else { 22.0 };
    let pip = if value == 1 { Color::from_rgb(196, 30, 58) } else { Color::from_rgb(28, 28, 38) };
    for cell in cells {
        let x = 128.0 + (cell % 3) as f32 * 64.0 - 64.0;
        let y = 128.0 + (cell / 3) as f32 * 64.0 - 64.0;
        c.draw_circle((x, y + 2.5), radius, &fill(Color::from_argb(200, 255, 255, 255)));
        c.draw_circle((x, y), radius, &fill(pip));
        c.draw_circle((x, y - radius * 0.25), radius * 0.75, &fill(Color::from_argb(70, 0, 0, 0)));
    }
}

/// A soft dark spot that fades to nothing at its edge: a die's shadow on the table.
fn shadow_spot() -> Texture3D {
    painted(128, 128, |c, w, h| {
        let colors = [Color::from_argb(255, 0, 0, 0), Color::from_argb(215, 0, 0, 0), Color::from_argb(90, 0, 0, 0), Color::from_argb(0, 0, 0, 0)].map(Color4f::from);
        let stops = [0.0, 0.35, 0.7, 1.0];
        let gradient = gradient::Gradient::new(gradient::Colors::new(&colors, Some(&stops), TileMode::Clamp, None), Interpolation::default());
        let shader = gradient::shaders::radial_gradient((Point::new(w / 2.0, h / 2.0), w / 2.0), &gradient, None);
        let mut paint = drawnui::skia::Paint::default();
        paint.set_anti_alias(true);
        paint.set_shader(shader);
        c.draw_rect(drawnui::skia::Rect::from_wh(w, h), &paint);
    })
}

/// Two dice, each a holder that turns it around the vertical and lifts it, a rounded cube wearing
/// its faces, and a soft shadow on the table under it (it stays on the table while the die hops).
fn two_dice() -> Model3D {
    let mut model = Model3D::new();
    for (d, x) in DIE_X.iter().enumerate() {
        model = model
            .part(format!("shadow{d}"), Mesh3D::cuboid(1.45, 0.001, 1.45), Color::WHITE.with_a(252))
            .at(x + SHADOW_SHIFT.0, -0.505, SHADOW_SHIFT.1)
            .part(format!("holder{d}"), Mesh3D::cuboid(0.01, 0.01, 0.01), Color::WHITE)
            .at(*x, 0.0, 0.0)
            .part(format!("die{d}"), Mesh3D::rounded_cuboid(1.0, 1.0, 1.0, ROUNDING, true), Color::WHITE)
            .inside(format!("holder{d}"));
    }
    model
}

/// How high a die's center must be for its lowest point to touch the table, turned by (x, z).
fn resting_lift(x: f32, z: f32) -> f32 {
    let turn = Quat::from_axis_angle(Vec3::X, x.to_radians()) * Quat::from_axis_angle(Vec3::Z, z.to_radians());
    let core = 0.5 - ROUNDING;
    let lowest = (0..8)
        .map(|i| (turn * Vec3::new(if i & 1 == 0 { -core } else { core }, if i & 2 == 0 { -core } else { core }, if i & 4 == 0 { -core } else { core })).y)
        .fold(0.0f32, f32::min);
    -lowest + ROUNDING - 0.5
}

/// A die's place: turned, lifted to rest on the table plus its hop, its shadow wider and lighter
/// as it rises.
fn place(view: &mut Mut<'_, SkiaModel3D>, d: usize, (x, z, yaw): Pose, hop: f32) {
    let lift = resting_lift(x, z) + hop;
    view.set_node_rotation(&format!("die{d}"), [x, 0.0, z]);
    view.set_node_rotation(&format!("holder{d}"), [0.0, yaw, 0.0]);
    view.set_node_translation(&format!("holder{d}"), [DIE_X[d], lift, 0.0]);
    let spread = 1.0 + lift * 0.6;
    view.set_node_scale(&format!("shadow{d}"), [spread, 1.0, spread]);
    let alpha = (0.99 - lift * 0.9).clamp(0.3, 0.99);
    view.set_material_color(&format!("shadow{d}"), Some(Color::WHITE.with_a((alpha * 255.0) as u8)));
}

/// A die's angle during a roll: it keeps turning fast, loses speed to friction but never crawls
/// (its last turn is still at 40% of its average speed), hits its face and settles on it like a
/// damped spring: it rocks back off the face and in again, each swing smaller, and stops.
fn tumble(from: f32, to: f32, t: f32) -> f32 {
    let d = to - from;
    if t < TUMBLING {
        let u = t / TUMBLING;
        return from + d * u * (1.6 - 0.6 * u);
    }
    // A damped spring started by the impact, back off the face first.
    let s = (t - TUMBLING) / (1.0 - TUMBLING);
    let (zeta, cycles) = (0.28f32, 2.2f32);
    let wd = std::f32::consts::TAU * cycles;
    let w = wd / (1.0 - zeta * zeta).sqrt();
    let amplitude = d.abs().min(7.0);
    to - d.signum() * amplitude * (-zeta * w * s).exp() * (wd * s).sin()
}

/// Hops off the table: three falls under gravity, each lower and shorter, done before the die lands.
fn bounces(t: f32) -> f32 {
    const HEIGHTS: [f32; 3] = [0.32, 0.12, 0.04];
    // The time of a hop grows with the square root of its height, as for a falling body.
    let total: f32 = HEIGHTS.iter().map(|h| h.sqrt()).sum();
    let mut at = t / (TUMBLING * 0.95) * total;
    for h in HEIGHTS {
        let span = h.sqrt();
        if at < span {
            let u = at / span;
            return 4.0 * h * u * (1.0 - u);
        }
        at -= span;
    }
    0.0
}

/// Picks both results, then tumbles each die into its face-up turn with a few hops, from where it
/// stands.
fn roll(app: &mut App, cx: &mut Cx) {
    let dice = &mut app.dice;
    if let Some(running) = dice.roll.take() {
        cx.stop_animation(running);
    }
    let mut pick = |n: usize| (dice.random.next() * n as f32) as usize % n;
    let values = [1 + pick(6), 1 + pick(6)];
    let from = *dice.poses.borrow();
    let mut to = [(0.0, 0.0, 0.0); 2];
    for d in 0..2 {
        let up = FACE_UP[values[d] - 1];
        // Two or three whole turns on top of the landing turn, alternately forward and back.
        let spins = 360.0 * (2 + pick(2)) as f32 * if d == 0 { 1.0 } else { -1.0 };
        to[d] = (up.0 + spins, up.1 + 360.0 * (1 + pick(2)) as f32, from[d].2 + pick(121) as f32 - 60.0);
    }
    if let Some(mut label) = cx.get_mut(dice.result) {
        label.set_text("Rolling...");
    }
    let (view, result, poses) = (dice.view, dice.result, dice.poses.clone());
    dice.roll = Some(cx.animate(view, ROLL_MS, easing::linear, move |t, cx| {
        let hop = bounces(t);
        let Some(mut v) = cx.get_mut(view) else { return };
        for d in 0..2 {
            let p = (tumble(from[d].0, to[d].0, t), tumble(from[d].1, to[d].1, t), tumble(from[d].2, to[d].2, t));
            poses.borrow_mut()[d] = p;
            place(&mut v, d, p, hop);
        }
        if t >= 1.0 {
            for d in 0..2 {
                poses.borrow_mut()[d] = (to[d].0 % 360.0, to[d].1 % 360.0, to[d].2 % 360.0);
            }
            let sum = values[0] + values[1];
            v.set_accessibility_label(format!("Two dice showing {} and {}", values[0], values[1]));
            drop(v);
            if let Some(mut label) = cx.get_mut(result) {
                label.set_text(if values[0] == values[1] { format!("Doubles! {} + {} = {sum}", values[0], values[1]) } else { format!("{} + {} = {sum}", values[0], values[1]) });
            }
        }
    }));
}

/// The dice's table: green casino felt, lit in its middle (a lamp over the table, the light
/// falling off softly into the felt's own green) and sunk into the card.
fn felt(dice: impl IntoChildren) -> Build<SkiaShape> {
    sunk(
        SkiaShape::new().corner_radius(14).fill_x().height_request(DICE_HEIGHT).background_color(Color::from_rgb(17, 82, 50)).fill_gradient(SkiaGradient {
            start_x_ratio: 0.5,
            start_y_ratio: 0.45,
            ..SkiaGradient::new(GradientType::Circular, vec![Color::from_rgb(36, 132, 80), Color::from_rgb(25, 106, 64), Color::from_rgb(17, 82, 50)]).color_positions(vec![0.0, 0.6, 1.0])
        }),
    )
    .children(dice)
}

/// The dice card.
pub fn card(app: &mut App) -> Build<SkiaShape> {
    let dice = &mut app.dice;
    *dice.poses.borrow_mut() = [(0.0, 0.0, 20.0), (0.0, 0.0, -15.0)];
    dice.random = crate::Random(20_261_116);
    let (faces, shadow) = (die_faces(), shadow_spot());
    panel(
        "Dice roll",
        "Two dice made in code: a rounded cube wearing one image of its six faces, pips drawn in Skia. Flick them sideways, tap them or press Roll.",
        (SkiaLayout::column().fill_x().spacing(12).children((
            felt((SkiaModel3D::from_model(&two_dice())
                .orbit(false)
                .view_angles(0.0, 32.0)
                // Room for a die turned on its corner and hopping.
                .frame_margin(0.5)
                .antialias(true)
                .fill()
                .accessibility_role(Aria::IMG)
                .accessibility_label("Two dice")
                .assign(&mut dice.view)
                .on_view_changed(move |me, app: &mut App, _cx| {
                    if std::mem::replace(&mut app.dice.dressed, true) {
                        return;
                    }
                    for d in 0..2 {
                        me.set_material_image(&format!("die{d}"), Some(faces.clone()));
                        // Polished plastic: a soft shine.
                        me.set_material_roughness(&format!("die{d}"), Some(0.3));
                        me.set_material_image(&format!("shadow{d}"), Some(shadow.clone()));
                        me.set_material_visible(&format!("holder{d}"), false);
                    }
                    let poses = *app.dice.poses.borrow();
                    for (d, pose) in poses.into_iter().enumerate() {
                        place(me, d, pose, 0.0);
                    }
                })
                // A flick sideways rolls them; the page keeps its vertical drags.
                .consume_gestures(|me, app: &mut App, cx, g| match g.kind {
                    GestureKind::Panning => g.total.x.abs() > g.total.y.abs(),
                    GestureKind::Up => {
                        let scale = me.base().scale.max(0.1);
                        if g.total.x.abs() > 40.0 * scale && g.total.x.abs() > g.total.y.abs() {
                            roll(app, cx);
                        }
                        false
                    }
                    _ => false,
                })
                .on_tapped_3d(|_me, app: &mut App, cx, _hit| roll(app, cx)),)),
            SkiaLayout::row().spacing(16).children((
                Button3D::new("Roll", ACCENT, Some(120.0)).on_tapped(|_me, app: &mut App, cx| roll(app, cx)),
                copyable("Roll the dice").font_size(18).font_family(TITLE).text_color(INK).vertical_options(LayoutOptions::Center).assign(&mut dice.result),
            )),
        )),),
    )
}
