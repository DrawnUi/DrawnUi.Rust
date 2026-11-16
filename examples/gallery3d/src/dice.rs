//! A small game (the .NET gallery's GalleryPage.Dice): two dice made in code, a rounded cube
//! wearing its six faces in one image (pips drawn with Skia), thrown in physics (`DiceThrow`) on
//! green casino felt; the faces they land on are the result. Flick them sideways, tap them or
//! press Roll.

use std::cell::RefCell;
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::skia::{Canvas, Point, gradient, gradient::Interpolation, Color4f, TileMode};
use drawnui_add3d::prelude::*;
use glam::{Quat, Vec3};

use crate::dice_throw::DiceThrow;

use crate::theme::*;
use crate::{App, fill, painted, panel};

pub struct DiceState {
    view: Handle<SkiaModel3D>,
    result: Handle<SkiaLabel>,
    /// The throw: where the dice lie and how they are turned, shared with the roll under way.
    throw: Rc<RefCell<DiceThrow>>,
    /// The dice as they lie between throws.
    at: [Vec3; 2],
    turn: [Quat; 2],
    /// The images are on and the dice placed.
    dressed: bool,
    /// A throw ran: the dice lie where it left them.
    thrown: bool,
    roll: Rc<std::cell::Cell<Option<AnimationId>>>,
    random: crate::Random,
}

impl Default for DiceState {
    fn default() -> Self {
        DiceState {
            view: Handle::default(),
            result: Handle::default(),
            throw: Rc::new(RefCell::new(DiceThrow::new(ROUNDING))),
            at: [Vec3::new(-0.8, 0.0, 0.0), Vec3::new(0.8, 0.0, 0.0)],
            turn: [Quat::from_axis_angle(Vec3::Y, 0.35), Quat::from_axis_angle(Vec3::Y, -0.26)],
            dressed: false,
            thrown: false,
            roll: Rc::default(),
            random: crate::Random::default(),
        }
    }
}

const DICE_HEIGHT: f32 = 280.0;
/// The longest a throw runs; the animation stops itself when the dice lie still.
const THROW_MS: f32 = 4000.0;
const ROUNDING: f32 = 0.14;
/// The shadows lie a little toward the viewer and right: away from the key light, up and to the left.
const SHADOW_SHIFT: (f32, f32) = (0.12, 0.15);

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

/// Two dice, each a rounded cube wearing its faces, and a soft shadow on the table under it (it
/// stays on the table while the die is in the air).
fn two_dice() -> Model3D {
    let mut model = Model3D::new();
    for d in 0..2 {
        let x = if d == 0 { -0.8 } else { 0.8 };
        // A little toward the viewer and right: away from the key light, up and to the left.
        model = model
            .part(format!("shadow{d}"), Mesh3D::cuboid(1.45, 0.001, 1.45), Color::WHITE.with_a(252))
            .at(x + SHADOW_SHIFT.0, -0.505, SHADOW_SHIFT.1)
            .part(format!("die{d}"), Mesh3D::rounded_cuboid(1.0, 1.0, 1.0, ROUNDING, true), Color::WHITE)
            .at(x, 0.0, 0.0);
    }
    model
}

/// A die's place and turn; its shadow under it on the table, wider and lighter as the die rises.
fn place(view: &mut Mut<'_, SkiaModel3D>, d: usize, at: Vec3, turn: Quat) {
    view.set_node_rotation_quat(&format!("die{d}"), turn.to_array());
    view.set_node_translation(&format!("die{d}"), at.to_array());
    let lift = at.y.max(0.0);
    let spread = 1.0 + lift * 0.6;
    view.set_node_translation(&format!("shadow{d}"), [at.x + SHADOW_SHIFT.0, -0.505, at.z + SHADOW_SHIFT.1]);
    view.set_node_scale(&format!("shadow{d}"), [spread, 1.0, spread]);
    let alpha = (0.99 - lift * 0.9).clamp(0.3, 0.99);
    view.set_material_color(&format!("shadow{d}"), Some(Color::WHITE.with_a((alpha * 255.0) as u8)));
}

/// Throws both dice from where they lie (toward `direction`: -1 left, 1 right, 0 either). The
/// throw is physics (`DiceThrow`) stepped along with each frame at its real pace, so nothing is
/// worked out up front; the faces they end on are the result.
fn roll(app: &mut App, cx: &mut Cx, direction: i32) {
    let dice = &mut app.dice;
    if let Some(running) = dice.roll.take() {
        cx.stop_animation(running);
    }
    {
        let mut throw = dice.throw.borrow_mut();
        // From where the last throw left them (or where they are now, mid-throw).
        if dice.thrown {
            dice.at = [throw.position(0), throw.position(1)];
            dice.turn = [throw.turn(0), throw.turn(1)];
        }
        throw.throw(dice.at, dice.turn, direction, &mut dice.random);
        dice.thrown = true;
    }
    if let Some(mut label) = cx.get_mut(dice.result) {
        label.set_text("Rolling...");
    }
    let (view, result, throw, running) = (dice.view, dice.result, dice.throw.clone(), dice.roll.clone());
    let mut last = 0.0;
    let id = cx.animate(view, THROW_MS, easing::linear, move |t, cx| {
        let mut throw = throw.borrow_mut();
        let now = t * THROW_MS / 1000.0;
        throw.run(now - last);
        last = now;
        let Some(mut v) = cx.get_mut(view) else { return };
        for d in 0..2 {
            place(&mut v, d, throw.position(d), throw.turn(d));
        }
        if !throw.done {
            return;
        }
        // Both dice lie still: the result, and nothing moves any more.
        let [a, b] = throw.faces;
        v.set_accessibility_label(format!("Two dice showing {a} and {b}"));
        if let Some(mut label) = cx.get_mut(result) {
            label.set_text(if a == b { format!("Doubles! {a} + {b} = {}", a + b) } else { format!("{a} + {b} = {}", a + b) });
        }
        if let Some(id) = running.take() {
            cx.stop_animation(id);
        }
    });
    dice.roll.set(Some(id));
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
    // A new throw each visit: seeded from the clock.
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(20_261_116, |d| d.subsec_nanos() ^ d.as_secs() as u32);
    dice.random = crate::Random(seed);
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
                    }
                    for d in 0..2 {
                        place(me, d, app.dice.at[d], app.dice.turn[d]);
                    }
                })
                // A flick sideways rolls them; the page keeps its vertical drags.
                .consume_gestures(|me, app: &mut App, cx, g| match g.kind {
                    GestureKind::Panning => g.total.x.abs() > g.total.y.abs(),
                    GestureKind::Up => {
                        let scale = me.base().scale.max(0.1);
                        if g.total.x.abs() > 40.0 * scale && g.total.x.abs() > g.total.y.abs() {
                            roll(app, cx, g.total.x.signum() as i32);
                        }
                        false
                    }
                    _ => false,
                })
                .on_tapped_3d(|_me, app: &mut App, cx, _hit| roll(app, cx, 0)),)),
            SkiaLayout::row().spacing(16).children((
                Button3D::new("Roll", ACCENT, Some(120.0)).on_tapped(|_me, app: &mut App, cx| roll(app, cx, 0)),
                copyable("Roll the dice").font_size(18).font_family(TITLE).text_color(INK).vertical_options(LayoutOptions::Center).assign(&mut dice.result),
            )),
        )),),
    )
}
