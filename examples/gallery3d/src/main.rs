//! A gallery of 3D inside DrawnUI screens (the drawnui-add3d add-on), in the order an app reaches
//! for it: the reasons an app adds 3D first, the fun after, games last. The same sections,
//! strings and behavior as the .NET gallery (DrawnUi.Blazor.Add3d, Add3dGallery).
//! - Product viewer: a model from the app or from a url (loading and error states), drag, pinch or
//!   wheel, double tap to reset, hotspots anchored to the model, color swatches.
//! - 3D in a scrolling list: thumbnails that turn only while on screen, sideways drags turn them,
//!   up and down drags scroll.
//! - Card flip: a card drawn in code that leans toward the pointer and flips.
//! - Globe with pins: the Earth with city pins that hide on the far side.
//! - 3D next to normal UI: a sign-up form with a mascot that watches the field, covers its eyes for
//!   the password and waves on submit.
//! - Micro-interactions, images on 3D, model files and games (a field of walking mobs).
//!
//! A header with two tabs: the gallery and the boss fight (a glTF dragon with its animations,
//! driven by the app: moves, a tap that hits it, skin and eyes recolored by material name).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::f32::consts::TAU;

use drawnui::Detached;
use drawnui::prelude::*;
use drawnui::skia::{
    self, Canvas, Color4f, Data, Font, FontMgr, Paint, Path, PathBuilder, Point, RRect, Rect, TileMode,
    gradient::{self, Gradient, Interpolation},
};
use drawnui_add3d::prelude::*;
use glam::{EulerRot, Quat};

// Model files and the Earth image: public domain, see assets/MODELS-LICENSE.txt.
const CHEST: &[u8] = include_bytes!("../assets/gift.glb");
const AVATAR: &[u8] = include_bytes!("../assets/avatar.glb");
const MOB: &[u8] = include_bytes!("../assets/mob.glb");
const DRAGON: &[u8] = include_bytes!("../assets/dragon.glb");
const LOUNGE_CHAIR: &[u8] = include_bytes!("../assets/lounge_chair.glb");
const ARMCHAIR: &[u8] = include_bytes!("../assets/armchair.glb");
const OTTOMAN: &[u8] = include_bytes!("../assets/ottoman.glb");
const COFFEE_TABLE: &[u8] = include_bytes!("../assets/coffee_table.glb");
const DRAWER_CABINET: &[u8] = include_bytes!("../assets/drawer_cabinet.glb");
const EARTH: &[u8] = include_bytes!("../assets/earth.jpg");
const SOFA_HQ: &[u8] = include_bytes!("../assets/sofa.glb");
const LEATHER: &[u8] = include_bytes!("../assets/leather.jpg");
const WOOL: &[u8] = include_bytes!("../assets/wool.jpg");
const VELVET: &[u8] = include_bytes!("../assets/velvet.jpg");
const LINEN: &[u8] = include_bytes!("../assets/linen.jpg");
const FONT: &[u8] = include_bytes!("../assets/OpenSans-Regular.ttf");

/// The product viewer's model from a url: the app's site serves it (the browser fetches it,
/// desktop reads it beside the exe); the broken one shows the error state.
const SOFA_URL: &str = "assets/sofa.glb";
const BROKEN_URL: &str = "assets/no-such-model.glb";

const PAGE: Color = Color::from_rgb(14, 16, 24);
const CARD: Color = Color::from_rgb(24, 27, 40);
const LINE: Color = Color::from_rgb(40, 44, 62);
/// The panel a piece stands on inside its card.
const STAGE: Color = Color::from_rgb(42, 47, 70);
const INK: Color = Color::from_rgb(242, 241, 248);
const MUTED: Color = Color::from_rgb(154, 152, 176);
const BUTTON: Color = Color::from_rgb(124, 108, 242);
const ACCENT: Color = Color::from_rgb(255, 196, 61);
const DANGER: Color = Color::from_rgb(255, 99, 132);
const HEADER: Color = Color::from_rgb(19, 21, 32);
/// Narrower canvases put the tabs under the app's name.
const HEADER_ONE_LINE: f32 = 560.0;
/// The header tabs: the shown one in the accent, the other quiet.
const TAB_ON: Color = Color::from_rgb(124, 108, 242);
const TAB_OFF: Color = Color::from_rgb(52, 56, 78);
/// The medal's resting turn: it spins from here and lands here.
const MEDAL_YAW: f32 = 20.0;
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

/// The confetti tile, a wide panel and its 3D stage, the mobs' field, in points.
const RAIN: (f32, f32) = (268.0, 190.0);
const PANEL: f32 = 1000.0;
const VIEW: (f32, f32) = (620.0, 420.0);
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

/// A white box: its label shows as it is.
fn product_box() -> Model3D {
    Model3D::new().part("box", Mesh3D::cuboid(1.0, 1.0, 1.0), Color::WHITE)
}

/// A bank card's outline, 85.6 x 54 mm as 856 x 540 path units, round corners.
fn card_outline() -> Path {
    let mut b = PathBuilder::new();
    b.add_rrect(RRect::new_rect_xy(Rect::from_wh(1024.0, 646.0), 46.0, 46.0), None, None);
    b.detach()
}

/// A card: a front and a back half, each showing its own image, the back half turned round so its
/// image reads the right way from behind.
fn card() -> Model3D {
    // 8.56 x 5.4 units, as the .NET card.
    let s = 8.56 / 1024.0;
    Model3D::new()
        .part("front", Mesh3D::extrude(&card_outline(), 4.0), Color::WHITE)
        .scaled(s, s, s)
        .at(0.0, 0.0, 2.0 * s)
        .part("back", Mesh3D::extrude(&card_outline(), 4.0), Color::WHITE)
        .scaled(s, s, s)
        .turned(0.0, 180.0, 0.0)
        .at(0.0, 0.0, -2.0 * s)
}

fn globe() -> Model3D {
    Model3D::new().part("earth", Mesh3D::sphere(1.0), Color::WHITE)
}

// ---------------------------------------------------------------- images drawn in code

/// An image drawn with Skia, ready to put on a 3D surface.
fn painted(width: i32, height: i32, draw: impl FnOnce(&Canvas, f32, f32)) -> Texture3D {
    let mut surface = skia::surfaces::raster_n32_premul((width, height)).expect("a raster surface");
    draw(surface.canvas(), width as f32, height as f32);
    Texture3D::from_image(&surface.image_snapshot()).expect("a raster image has pixels")
}

fn fill(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_color(color);
    paint
}

fn linear(from: (f32, f32), to: (f32, f32), a: Color, b: Color) -> Paint {
    let colors = [Color4f::from(a), Color4f::from(b)];
    let gradient = Gradient::new(gradient::Colors::new(&colors, None, TileMode::Clamp, None), Interpolation::default());
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_shader(gradient::shaders::linear_gradient((Point::from(from), Point::from(to)), &gradient, None));
    paint
}

/// Text at `x` (left, or centered when `center`), its baseline at `y`.
fn text(canvas: &Canvas, text: &str, x: f32, y: f32, size: f32, color: Color, center: bool) {
    styled_text(canvas, text, x, y, size, color, center, false);
}

/// `text`, optionally slanted like italic. Subpixel-positioned and antialiased.
#[allow(clippy::too_many_arguments)]
fn styled_text(canvas: &Canvas, text: &str, x: f32, y: f32, size: f32, color: Color, center: bool, italic: bool) {
    let Some(typeface) = FontMgr::new().new_from_data(Data::new_copy(FONT), None) else { return };
    let mut font = Font::from_typeface(typeface, size);
    font.set_subpixel(true).set_edging(skia::font::Edging::AntiAlias);
    if italic {
        font.set_skew_x(-0.22);
    }
    let paint = fill(color);
    let (width, _) = font.measure_str(text, Some(&paint));
    canvas.draw_str(text, (if center { x - width / 2.0 } else { x }, y), &font, &paint);
}

/// A product box's label: a fruit on a gradient, a frame and the flavor's name.
fn label(name: &str, top: Color, bottom: Color, fruit: Color) -> Texture3D {
    painted(512, 512, |c, w, h| {
        c.draw_rect(Rect::from_wh(w, h), &linear((0.0, 0.0), (0.0, h), top, bottom));
        let mut frame = fill(Color::from_argb(200, 255, 255, 255));
        frame.set_style(skia::PaintStyle::Stroke).set_stroke_width(10.0);
        c.draw_round_rect(Rect::from_xywh(26.0, 26.0, w - 52.0, h - 52.0), 36.0, 36.0, &frame);
        c.draw_circle((w / 2.0, 214.0), 112.0, &fill(fruit));
        c.draw_circle((w / 2.0 - 38.0, 176.0), 30.0, &fill(Color::from_argb(90, 255, 255, 255)));
        c.draw_oval(Rect::from_xywh(w / 2.0 + 6.0, 70.0, 70.0, 34.0), &fill(Color::from_rgb(76, 201, 140)));
        text(c, name, w / 2.0, 410.0, 76.0, Color::WHITE, true);
        text(c, "made in code", w / 2.0, 456.0, 26.0, Color::from_argb(220, 255, 255, 255), true);
    })
}

fn labels() -> Vec<Texture3D> {
    vec![
        label("BERRY", Color::from_rgb(255, 99, 132), Color::from_rgb(124, 58, 237), Color::from_rgb(190, 24, 93)),
        label("MINT", Color::from_rgb(52, 211, 153), Color::from_rgb(13, 110, 253), Color::from_rgb(16, 185, 129)),
        label("SUN", Color::from_rgb(255, 214, 10), Color::from_rgb(253, 126, 20), Color::from_rgb(255, 159, 28)),
    ]
}

/// The card's two faces, 1024 x 646, as the .NET card: "Drawn Rewards Gold".
fn card_faces() -> (Texture3D, Texture3D) {
    let hex = |rgb: u32| Color::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    let front = painted(1024, 646, |c, w, h| {
        let colors = [Color4f::from(hex(0x261E5C)), Color4f::from(hex(0x7C6CF2)), Color4f::from(hex(0xFF6384))];
        let gradient = Gradient::new(gradient::Colors::new(&colors, None, TileMode::Clamp, None), Interpolation::default());
        let mut paint = Paint::default();
        paint.set_shader(gradient::shaders::linear_gradient((Point::new(0.0, 0.0), Point::new(w, h)), &gradient, None));
        c.draw_rect(Rect::from_wh(w, h), &paint);
        // A soft white glow at the top right.
        let glow = [Color4f::from(Color::from_argb(90, 255, 255, 255)), Color4f::from(Color::from_argb(0, 255, 255, 255))];
        let glow = Gradient::new(gradient::Colors::new(&glow, None, TileMode::Clamp, None), Interpolation::default());
        let mut soft = Paint::default();
        soft.set_shader(gradient::shaders::radial_gradient((Point::new(w * 0.84, h * 0.1), 330.0), &glow, None));
        c.draw_rect(Rect::from_wh(w, h), &soft);
        // Two white rings and the name, top left; the tier, top right.
        let mut ring = fill(Color::WHITE);
        ring.set_style(skia::PaintStyle::Stroke).set_stroke_width(5.0);
        c.draw_circle((84.0, 92.0), 24.0, &ring);
        c.draw_circle((114.0, 92.0), 24.0, &ring);
        text(c, "DRAWN REWARDS", 158.0, 106.0, 38.0, Color::WHITE, false);
        let gold = hex(0xFFD678);
        let (gw, _) = Font::from_typeface(FontMgr::new().new_from_data(Data::new_copy(FONT), None).expect("the gallery font"), 44.0).measure_str("GOLD", None);
        text(c, "GOLD", w - 64.0 - gw, 108.0, 44.0, gold, false);
        // The chip.
        c.draw_round_rect(Rect::from_xywh(64.0, 196.0, 136.0, 104.0), 18.0, 18.0, &linear((64.0, 196.0), (200.0, 300.0), GOLD_LIGHT, GOLD));
        let mut lines = fill(Color::from_argb(130, 120, 80, 0));
        lines.set_style(skia::PaintStyle::Stroke).set_stroke_width(3.0);
        c.draw_round_rect(Rect::from_xywh(98.0, 220.0, 68.0, 56.0), 10.0, 10.0, &lines);
        c.draw_line((64.0, 248.0), (98.0, 248.0), &lines);
        c.draw_line((166.0, 248.0), (200.0, 248.0), &lines);
        text(c, "4821  0730  5512  9046", 64.0, 420.0, 60.0, Color::WHITE, false);
        let dim = Color::from_argb(190, 255, 255, 255);
        text(c, "MEMBER", 64.0, 520.0, 24.0, dim, false);
        text(c, "ALEX MORGAN", 64.0, 568.0, 40.0, Color::WHITE, false);
        text(c, "VALID THRU", 700.0, 520.0, 24.0, dim, false);
        text(c, "08/29", 700.0, 568.0, 40.0, Color::WHITE, false);
    });
    let back = painted(1024, 646, |c, w, h| {
        c.draw_rect(Rect::from_wh(w, h), &fill(hex(0x1E1A3E)));
        // The magnetic stripe.
        c.draw_rect(Rect::from_xywh(0.0, 60.0, w, 112.0), &fill(Color::from_rgb(10, 10, 14)));
        // The signature strip and the CVV.
        c.draw_round_rect(Rect::from_xywh(64.0, 226.0, 620.0, 84.0), 8.0, 8.0, &fill(Color::from_rgb(232, 228, 240)));
        styled_text(c, "Alex Morgan", 92.0, 282.0, 40.0, Color::from_rgb(52, 48, 84), false, true);
        c.draw_round_rect(Rect::from_xywh(708.0, 226.0, 140.0, 84.0), 8.0, 8.0, &fill(Color::WHITE));
        text(c, "123", 778.0, 282.0, 40.0, Color::from_rgb(30, 28, 52), true);
        text(c, "12,480 points", 64.0, 420.0, 52.0, Color::WHITE, false);
        text(c, "Show this card at checkout to collect points.", 64.0, 478.0, 30.0, Color::from_argb(200, 255, 255, 255), false);
        text(c, "drawnui.net", 64.0, h - 64.0, 34.0, hex(0xFFD678), false);
    });
    (front, back)
}

// ---------------------------------------------------------------- the app's data

/// A point to explain on the product, in the model's units.
struct Hotspot {
    point: [f32; 3],
    title: &'static str,
    text: &'static str,
}

/// On the sofa (assets/sofa.glb), a hair above its surface.
const SPOTS: [Hotspot; 3] = [
    Hotspot { point: [0.3, 0.386, 0.05], title: "Seat", text: "Feather-wrapped cushions" },
    Hotspot { point: [0.0, 0.65, -0.252], title: "Back", text: "Hand-sewn, deep buttoned" },
    Hotspot { point: [0.76, 0.5, 0.183], title: "Frame", text: "Carved walnut" },
];

/// The product viewer's model: from the app, from a url, from a url that fails.
#[derive(Clone, Copy, Default, PartialEq)]
enum Source {
    #[default]
    App,
    Url,
    Broken,
}

/// Upholstery swatches, cheapest first: name, the image on the "upholstery" material (CC0 textures
/// from Poly Haven), how matte it is (glTF roughness: leather shines a little), the swatch's color,
/// the sofa's price in it.
const SWATCHES: [(&str, &[u8], f32, Color, f32); 4] = [
    ("Linen", LINEN, 1.0, Color::from_rgb(140, 172, 206), 1190.0),
    ("Wool", WOOL, 0.95, Color::from_rgb(122, 120, 114), 1290.0),
    ("Velvet", VELVET, 0.7, Color::from_rgb(180, 40, 62), 1490.0),
    ("Leather", LEATHER, 0.42, Color::from_rgb(92, 52, 26), 1890.0),
];

/// "$1,290".
fn dollars(amount: f32) -> String {
    let n = amount.round() as u32;
    if n >= 1000 { format!("${},{:03}", n / 1000, n % 1000) } else { format!("${n}") }
}

/// A row of the list: model, name, kind, price.
/// The shop's furniture (Poly Haven, CC0): model, name, detail, price in its first finish.
const PRODUCTS: [(&[u8], &str, &str, u32); 6] = [
    (SOFA_HQ, "Walnut sofa", "Three-seater, carved walnut frame", 1190),
    (LOUNGE_CHAIR, "Lounge chair", "Leather on a bent plywood shell", 890),
    (ARMCHAIR, "Armchair", "Carved frame, linen seat", 640),
    (OTTOMAN, "Ottoman", "Soft leather on wooden feet", 290),
    (COFFEE_TABLE, "Coffee table", "Stone top on an oak frame", 420),
    (DRAWER_CABINET, "Drawer cabinet", "Six drawers with brass pulls", 760),
];
const ROWS: usize = 30;
/// Finishes: name, the tint over the model's own colors (`None` = as it is), what it adds to the price.
const FINISHES: [(&str, Option<Color>, u32); 5] = [
    ("Natural", None, 0),
    ("Honey", Some(Color::from_rgb(255, 232, 196)), 20),
    ("Smoke", Some(Color::from_rgb(190, 196, 210)), 40),
    ("Sage", Some(Color::from_rgb(204, 232, 210)), 60),
    ("Espresso", Some(Color::from_rgb(150, 134, 122)), 80),
];

/// Cities on the globe: name, latitude, longitude.
const CITIES: [(&str, f32, f32); 6] = [("Tokyo", 35.7, 139.7), ("Sydney", -33.9, 151.2), ("Cape Town", -33.9, 18.4), ("London", 51.5, -0.1), ("New York", 40.7, -74.0), ("Rio de Janeiro", -22.9, -43.2)];

/// A point on the globe (radius 1) for a latitude and longitude: the sphere wraps its image once
/// around (the image's left edge, longitude -180, at angle 360) from the north pole down.
fn on_globe(lat: f32, lon: f32) -> [f32; 3] {
    let (lat, angle) = (lat.to_radians(), (1.0 - (lon + 180.0) / 360.0) * TAU);
    // A hair above the surface: the pin stands on it, not in it.
    let r = 1.002;
    [r * lat.cos() * angle.cos(), r * lat.sin(), r * lat.cos() * angle.sin()]
}

/// A small random number generator: the same confetti and crowd every run.
#[derive(Default, Clone, Copy)]
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

/// Which field of the sign-up form has the keyboard.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Email,
    Password,
}

#[derive(Default)]
struct App {
    time: f32,
    /// The screen shown: 0 the gallery, 1 the boss fight.
    tab: usize,
    /// The canvas width, points: cards and the header follow it.
    width: f32,
    game: Handle<DrawnGame>,
    // Boss fight.
    boss: Handle<SkiaModel3D>,
    /// The looping move, by caption.
    boss_looping: &'static str,
    boss_skin: &'static str,
    boss_rage: bool,
    boss_turning: bool,
    /// What just happened, for the status line.
    boss_news: String,
    // Product viewer.
    viewer_slot: Handle<SkiaLayout>,
    spinner: Handle<SkiaShape>,
    load_error: String,
    viewer: Handle<SkiaModel3D>,
    source: Source,
    load: LoadState3D,
    swatch: usize,
    spots: [Handle<SkiaShape>; 3],
    notes: [Handle<SkiaShape>; 3],
    open_spot: Option<usize>,
    /// The upholstery images, in swatch order.
    fabrics: Vec<Texture3D>,
    /// The viewer's model wears the chosen swatch.
    dressed: bool,
    /// The price on the label: it counts to the chosen fabric's.
    price_shown: f32,
    // List.
    added: Vec<bool>,
    thumbs: Vec<Handle<SkiaModel3D>>,
    /// Rows whose finish tint is on.
    tinted: Vec<usize>,
    /// Thumbnails on screen now.
    thumbs_drawn: usize,
    // Card.
    card: Handle<SkiaModel3D>,
    back_shown: bool,
    /// The flip, 0 to 1 while it turns.
    flip: f32,
    card_faces: Option<(Texture3D, Texture3D)>,
    // Globe.
    globe: Handle<SkiaModel3D>,
    pins: [Handle<SkiaLayout>; 6],
    /// Each pin's pulse, grown and faded by the frame loop.
    pulses: [Handle<SkiaShape>; 6],
    pin_names: [Handle<SkiaShape>; 6],
    open_pin: Option<usize>,
    earth: Option<Texture3D>,
    spin_paused: bool,
    /// Turning the globe to a city: from (yaw, pitch), to, seconds into it.
    globe_glide: Option<((f32, f32), (f32, f32), f32)>,
    // Sign-up form.
    mascot: Handle<SkiaModel3D>,
    focused: Option<Field>,
    name: String,
    email: String,
    password_len: usize,
    signed_up: bool,
    problem: Option<&'static str>,
    // Micro-interactions.
    medal: Handle<SkiaModel3D>,
    unlocked: bool,
    /// The medal's spin after unlocking, 0 to 1.
    spin: f32,
    heart: Handle<SkiaModel3D>,
    liked: bool,
    /// The heart's pop after a like, 1 to 0.
    pop: f32,
    rain: Handle<SkiaScene3D>,
    pieces: Vec<Piece>,
    // Images on 3D.
    product: Handle<SkiaModel3D>,
    labels: Vec<Texture3D>,
    label: usize,
    smooth_edges: bool,
    // Model files.
    chest: Handle<SkiaModel3D>,
    open: bool,
    avatar: Handle<SkiaModel3D>,
    // Games.
    field: Handle<SkiaScene3D>,
    /// The side-scroller's moving layers, far to near, and how far it has panned.
    scenery: [[Handle<SkiaLayout>; 2]; 4],
    pan: f32,
    walkers: Vec<Walker>,
    mobs: usize,
    dice: Dice,
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

// ---------------------------------------------------------------- building blocks

fn tile(title: &str, caption: &str, model: impl IntoChildren, footer: impl IntoChildren) -> Build<SkiaShape> {
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
            SkiaShape::new().corner_radius(12).background_color(STAGE).fill_x().height_request(RAIN.1).children(model),
            SkiaLayout::row().spacing(8).horizontal_options(LayoutOptions::Center).children(footer),
        )),))
}

/// A wide card for one screen: a title, a caption, and what it shows.
fn panel(title: &str, caption: &str, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(16)
        .background_color(CARD)
        .stroke_color(LINE)
        .stroke_width(1)
        .padding(20)
        .horizontal_options(LayoutOptions::Center)
        .maximum_width_request(PANEL + 42.0)
        .children((SkiaLayout::column().spacing(10).fill_x().children((
            SkiaLabel::new(title).font_size(20).text_color(INK),
            SkiaLabel::new(caption).font_size(14).text_color(MUTED),
            SkiaLayout::column().fill_x().margin((0.0, 6.0, 0.0, 0.0)).children(content),
        )),))
}

/// The width a card's content has on a canvas `width` points wide: the page and card paddings
/// taken off, at most the desktop card.
fn card_inner(width: f32) -> f32 {
    (width - 2.0 * 24.0 - 2.0 * 21.0).min(PANEL)
}

/// Two-column cards stay side by side while their content is at least this wide; under it the
/// 3D view goes on top and the panel below, at full width.
const TWO_COLUMNS: f32 = 720.0;

/// A height for the mascot's view when the form and the mascot stack.
const STACKED_VIEW: f32 = 320.0;

/// A list row keeps its button beside the text while the card's content is at least this wide;
/// narrower, the button goes under the text.
const ROW_BESIDE: f32 = 420.0;

fn two_columns(app: &App) -> bool {
    app.width <= 0.0 || card_inner(app.width) >= TWO_COLUMNS
}

/// A two-column card body: a 3D view and its panel side by side, stacked when narrow.
fn split(view: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaLayout::row().spacing(24).fill_x().observe(|me, app: &App| me.set_layout_type(if two_columns(app) { LayoutType::Row } else { LayoutType::Column })).children(view)
}

/// The 3D view of a split card: `width` x `height` beside the panel; when the card stacks, the
/// full width at the same proportions, so the model is framed as on a desktop.
fn left_column<T: Control>(b: Build<T>, width: f32, height: f32) -> Build<T> {
    b.width_request(width).height_request(height).observe(move |me, app: &App| {
        let wide = two_columns(app);
        me.set_width_request(if wide { width } else { -1.0 });
        me.set_horizontal_options(if wide { LayoutOptions::Start } else { LayoutOptions::Fill });
        me.set_height_request(if wide { height } else { (card_inner(app.width) * height / width).round() });
    })
}

fn button(caption: &str) -> Build<SkiaButton> {
    SkiaButton::new(caption).width_request(150).background_color(BUTTON)
}

/// A small choice button: filled when chosen (`chosen` reads the app state).
fn chip(caption: &str, chosen: impl Fn(&App) -> bool + 'static) -> Build<SkiaButton> {
    SkiaButton::new(caption).observe(move |me, app: &App| me.set_background_color(if chosen(app) { BUTTON } else { STAGE }))
}

/// A piece made in code in real 3D (the default Lit look: shaded by a light, in perspective), turned
/// so its depth shows; still unless the app moves it.
fn shaded(model: &Model3D, yaw: f32, pitch: f32) -> Build<SkiaModel3D> {
    SkiaModel3D::from_model(model).orbit(false).view_angles(yaw, pitch).fill()
}

fn section(title: &str, caption: &str) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(4).horizontal_options(LayoutOptions::Center).margin((0.0, 12.0, 0.0, 0.0)).children((
        SkiaLabel::new(title).font_size(22).text_color(INK).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new(caption).font_size(14).text_color(MUTED).horizontal_options(LayoutOptions::Center),
    ))
}

/// A round marker anchored to a 3D point: a hotspot or a pin, with an optional number. Starts
/// hidden until placed.
fn marker(size: f32, color: Color, name: &str, number: Option<usize>) -> Build<SkiaShape> {
    SkiaShape::new()
        .children(number.map(|n| SkiaLabel::new(n.to_string()).font_size(13).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center).vertical_options(LayoutOptions::Center)).into_iter().collect::<Vec<_>>())
        .shape_type(ShapeType::Circle)
        .width_request(size)
        .height_request(size)
        .background_color(color)
        .stroke_color(Color::WHITE)
        .stroke_width(3)
        .is_visible(false)
        .accessibility_role(Aria::BUTTON)
        .accessibility_label(name)
        .on_hovered(|me, _app: &mut App, _cx, on| {
            me.set_scale_x(if on { 1.2 } else { 1.0 });
            me.set_scale_y(if on { 1.2 } else { 1.0 });
        })
}

/// A note's width: notes are centered over their marker before their first layout.
const NOTE_WIDTH: f32 = 210.0;

/// A small label over a marker. Starts hidden.
fn note(title: &str, text: &str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(NOTE_WIDTH)
        .corner_radius(10)
        .background_color(Color::from_argb(235, 18, 20, 30))
        .stroke_color(LINE)
        .stroke_width(1)
        .padding((12.0, 8.0))
        .is_visible(false)
        .children((SkiaLayout::column().spacing(2).children((
            SkiaLabel::new(title).font_size(14).text_color(INK),
            SkiaLabel::new(text).font_size(12).text_color(MUTED),
        )),))
}

/// Where points of a model are on screen now (`None` while it is not there).
fn positions(model: Option<&SkiaModel3D>, points: &[[f32; 3]]) -> Vec<Option<PointOnScreen>> {
    match model {
        Some(m) if m.load_state() == LoadState3D::Ready => points.iter().map(|p| m.screen_position(*p)).collect(),
        _ => vec![None; points.len()],
    }
}

/// Places markers and their labels over a model: on the point when it shows, hidden behind it.
/// Places markers and their notes over a model: a marker's `anchor` (its point, from its top left)
/// on the 3D point when that shows, hidden behind it; a note `rise` points above the point.
#[allow(clippy::too_many_arguments)]
fn place<T: Control>(cx: &mut Cx, on_screen: Vec<Option<PointOnScreen>>, markers: &[Handle<T>], notes: &[Handle<SkiaShape>], open: Option<usize>, anchor: (f32, f32), note_width: f32, rise: f32) {
    for (i, at) in on_screen.into_iter().enumerate() {
        let shown = at.filter(|p| !p.occluded);
        if let Some(mut m) = cx.get_mut(markers[i]) {
            m.set_is_visible(shown.is_some());
            if let Some(p) = shown {
                m.set_left(p.x - anchor.0);
                m.set_top(p.y - anchor.1);
            }
        }
        if let Some(mut n) = cx.get_mut(notes[i]) {
            n.set_is_visible(shown.is_some() && open == Some(i));
            if let Some(p) = shown {
                n.set_left(p.x - note_width / 2.0);
                n.set_top(p.y - rise);
            }
        }
    }
}

// ---------------------------------------------------------------- 1. product viewer

fn spots(source: Source) -> &'static [Hotspot] {
    if source == Source::Broken { &[] } else { &SPOTS }
}

/// Places the hotspots; `model` is the viewer when the call comes from its own handler (it cannot
/// be looked up then), else it is found by its handle.
fn place_spots(app: &App, cx: &mut Cx, model: Option<&SkiaModel3D>) {
    let points: Vec<[f32; 3]> = spots(app.source).iter().map(|s| s.point).collect();
    let n = points.len();
    let on_screen = match model {
        Some(m) => positions(Some(m), &points),
        None => positions(cx.find(app.viewer), &points),
    };
    place(cx, on_screen, &app.spots[..n], &app.notes[..n], app.open_spot, (13.0, 13.0), NOTE_WIDTH, 71.0);
    for i in n..3 {
        for h in [app.spots[i], app.notes[i]] {
            if let Some(mut m) = cx.get_mut(h) {
                m.set_is_visible(false);
            }
        }
    }
}

/// The upholstery's image and shine for the chosen swatch.
fn dress(viewer: &mut Mut<'_, SkiaModel3D>, app: &App) {
    viewer.set_material_image("upholstery", app.fabrics.get(app.swatch).cloned());
    viewer.set_material_color("upholstery", Some(Color::WHITE));
    viewer.set_material_roughness("upholstery", Some(SWATCHES[app.swatch].2));
}

fn apply_swatch(app: &App, cx: &mut Cx) {
    if let Some(mut viewer) = cx.get_mut(app.viewer) {
        dress(&mut viewer, app);
    }
}

fn viewer(source: Source, handle: &mut Handle<SkiaModel3D>) -> Build<SkiaModel3D> {
    let model = match source {
        Source::App => SkiaModel3D::new(SOFA_HQ),
        Source::Url => SkiaModel3D::from_url(SOFA_URL),
        Source::Broken => SkiaModel3D::from_url(BROKEN_URL),
    };
    model
        .view_angles(28.0, 14.0)
        .zoom(0.95)
        .pinch_zoom(true)
        .min_zoom(0.55)
        .max_zoom(1.8)
        .reset_on_double_tap(true)
        .antialias(true)
        .fill()
        .assign(handle)
        .on_view_changed(|me, app: &mut App, cx| {
            // The first layout of a model given as bytes: dress it (a url's dresses when it loads).
            if !app.dressed && me.control_mut().load_state() == LoadState3D::Ready {
                app.dressed = true;
                dress(me, app);
            }
            place_spots(app, cx, Some(me.control_mut()));
        })
        .on_load_state(|me, app: &mut App, cx, state| {
            app.load = state;
            app.load_error = me.control_mut().error().unwrap_or("not found").to_owned();
            if state == LoadState3D::Ready {
                app.dressed = true;
                dress(me, app);
            }
            place_spots(app, cx, Some(me.control_mut()));
        })
}

fn set_source(app: &mut App, cx: &mut Cx, source: Source) {
    app.source = source;
    app.open_spot = None;
    app.dressed = false;
    app.load = if source == Source::App { LoadState3D::Ready } else { LoadState3D::Loading };
    let old: ControlId = app.viewer.into();
    let new = viewer(source, &mut app.viewer);
    cx.replace_child(old, new);
    apply_swatch(app, cx);
}

fn product_viewer(app: &mut App) -> Build<SkiaShape> {
    let stage = left_column(SkiaShape::new(), VIEW.0, VIEW.1)
        .corner_radius(14)
        .background_color(STAGE)
        .is_clipped_to_bounds(true)
        .children((
            SkiaLayout::new().fill().assign(&mut app.viewer_slot).children((viewer(Source::App, &mut app.viewer),)),
            // Loading: a turning arc.
            SkiaLayout::column()
                .spacing(12)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center)
                .is_visible(false)
                .observe(|me, app: &App| me.set_is_visible(app.load == LoadState3D::Loading))
                .children((
                    SkiaShape::new()
                        .shape_type(ShapeType::Arc)
                        .width_request(40)
                        .height_request(40)
                        .stroke_color(BUTTON)
                        .stroke_width(4)
                        .value2(270.0)
                        .horizontal_options(LayoutOptions::Center)
                        .assign(&mut app.spinner),
                    SkiaLabel::new("Loading the sofa…").font_size(16).text_color(INK).horizontal_options(LayoutOptions::Center),
                )),
            // Failed.
            SkiaShape::new()
                .corner_radius(12)
                .background_color(CARD)
                .stroke_color(DANGER)
                .stroke_width(1)
                .padding(18)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center)
                .is_visible(false)
                .observe(|me, app: &App| me.set_is_visible(app.load == LoadState3D::Failed))
                .children((SkiaLayout::row().spacing(14).children((
                    SkiaShape::new().shape_type(ShapeType::Circle).width_request(36).height_request(36).background_color(DANGER).children((SkiaLabel::new("!")
                        .font_size(22)
                        .text_color(Color::WHITE)
                        .horizontal_options(LayoutOptions::Center)
                        .vertical_options(LayoutOptions::Center),)),
                    SkiaLayout::column().spacing(8).children((
                        SkiaLabel::new("The model could not be loaded").font_size(16).text_color(INK),
                        SkiaLabel::new("").font_size(12).text_color(MUTED).observe(|me, app: &App| me.set_text(format!("{BROKEN_URL}: {}", app.load_error))),
                        SkiaButton::new("Use the app's copy").background_color(BUTTON).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::App)),
                    )),
                )),)),
            // Hotspots and their notes.
            SkiaLayout::new().fill().children(
                (0..3)
                    .flat_map(|i| {
                        [
                            marker(26.0, BUTTON, SPOTS[i].title, Some(i + 1)).assign(&mut app.spots[i]).on_tapped(move |_me, app: &mut App, cx| {
                                app.open_spot = if app.open_spot == Some(i) { None } else { Some(i) };
                                place_spots(app, cx, None);
                            }),
                            note(SPOTS[i].title, SPOTS[i].text).assign(&mut app.notes[i]),
                        ]
                    })
                    .collect::<Vec<_>>(),
            ),
            SkiaLabel::new("Drag to turn  ·  pinch or scroll to zoom  ·  double-tap to reset")
                .font_size(12)
                .text_color(MUTED)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::End)
                .margin((0.0, 0.0, 0.0, 10.0)),
        ));
    let side = SkiaLayout::column().spacing(12).fill_x().children((
        SkiaLabel::new("Walnut sofa").font_size(24).text_color(INK),
        SkiaLabel::new("Three-seater, carved walnut frame").font_size(13).text_color(MUTED),
        SkiaLabel::new(dollars(SWATCHES[0].4)).font_size(18).text_color(ACCENT).observe(|me, app: &App| me.set_text(dollars(app.price_shown))),
        SkiaLabel::new("FABRIC").font_size(12).text_color(MUTED).margin((0.0, 8.0, 0.0, 0.0)),
        SkiaLayout::wrap().spacing(14).children(
            (0..SWATCHES.len())
                .map(|i| {
                    SkiaLayout::column().spacing(4).width_request(56).children((
                        SkiaShape::new()
                            .shape_type(ShapeType::Circle)
                            .width_request(40)
                            .height_request(40)
                            .horizontal_options(LayoutOptions::Center)
                            .background_color(SWATCHES[i].3)
                            .stroke_width(3)
                            .accessibility_role(Aria::RADIO)
                            .accessibility_label(SWATCHES[i].0)
                            .observe(move |me, app: &App| me.set_stroke_color(if app.swatch == i { Color::WHITE } else { LINE }))
                            .on_hovered(|me, _app: &mut App, _cx, on| {
                                me.set_scale_x(if on { 1.12 } else { 1.0 });
                                me.set_scale_y(if on { 1.12 } else { 1.0 });
                            })
                            .on_tapped(move |_me, app: &mut App, cx| {
                                app.swatch = i;
                                apply_swatch(app, cx);
                            }),
                        SkiaLabel::new(SWATCHES[i].0).font_size(12).text_color(MUTED).horizontal_options(LayoutOptions::Center),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
        SkiaLabel::new("MODEL").font_size(12).text_color(MUTED).margin((0.0, 8.0, 0.0, 0.0)),
        SkiaLayout::wrap().spacing(8).children((
            chip("In the app", |app| app.source == Source::App).accessibility_role(Aria::RADIO).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::App)),
            chip("From a link", |app| app.source == Source::Url).accessibility_role(Aria::RADIO).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::Url)),
            chip("Broken link", |app| app.source == Source::Broken).accessibility_role(Aria::RADIO).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::Broken)),
        )),
        SkiaButton::new("Reset view").background_color(STAGE).on_tapped(|_me, app: &mut App, cx| {
            if let Some(mut viewer) = cx.get_mut(app.viewer) {
                viewer.reset_view();
            }
        }),
    ));
    panel(
        "Product viewer",
        "A real product model on a shop page: turn it, zoom in, tap the dots, pick a fabric.",
        (split((stage, side)),),
    )
}

// ---------------------------------------------------------------- 2. 3D in a scrolling list

fn product_list(app: &mut App) -> Build<SkiaShape> {
    app.added = vec![false; ROWS];
    app.thumbs = vec![Handle::default(); ROWS];
    let rows: Vec<Build<SkiaLayout>> = (0..ROWS)
        .map(|i| {
            let (model, name, detail, price) = PRODUCTS[i % PRODUCTS.len()];
            let (finish, tint, extra) = FINISHES[i / PRODUCTS.len()];
            let mut thumb = SkiaModel3D::new(model).view_angles(32.0, 14.0).lock_pitch(true).fill().assign(&mut app.thumbs[i]);
            if let Some(tint) = tint {
                // Every piece is one material: the finish tints all of it.
                thumb = thumb.on_view_changed(move |me, app: &mut App, _cx| {
                    if !app.tinted.contains(&i) {
                        app.tinted.push(i);
                        if let Some(material) = me.control_mut().material_names().first().cloned() {
                            me.set_material_color(&material, Some(tint));
                        }
                    }
                });
            }
            SkiaLayout::row()
                .fill_x()
                .minimum_height_request(84)
                .spacing(16)
                .observe(|me, app: &App| me.set_layout_type(if app.width <= 0.0 || card_inner(app.width) >= ROW_BESIDE { LayoutType::Row } else { LayoutType::Wrap }))
                .padding((12.0, 8.0))
                .on_hovered(move |_me, app: &mut App, cx, on| {
                    // A row turns its piece while it is pointed at.
                    if let Some(mut thumb) = cx.get_mut(app.thumbs[i]) {
                        thumb.set_auto_rotate(if on { 40.0 } else { 0.0 });
                    }
                })
                .children((
                    SkiaShape::new().corner_radius(10).background_color(STAGE).width_request(68).height_request(68).vertical_options(LayoutOptions::Center).children((thumb,)),
                    SkiaLayout::column().spacing(2).vertical_options(LayoutOptions::Center).children((
                        SkiaLabel::new(format!("{name}, {finish}")).font_size(15).text_color(INK),
                        SkiaLabel::new(if i >= 15 { format!("{detail} · large") } else { detail.to_owned() }).font_size(12).text_color(MUTED),
                        SkiaLabel::new(dollars((price + extra + if i >= 15 { 120 } else { 0 }) as f32)).font_size(14).text_color(ACCENT),
                    )),
                    SkiaButton::new("Add")
                        .width_request(120)
                        .horizontal_options(LayoutOptions::End)
                        .vertical_options(LayoutOptions::Center)
                        .observe(move |me, app: &App| {
                            let added = app.added.get(i).copied().unwrap_or(false);
                            me.set_text(if added { "In the cart" } else { "Add" });
                            me.set_background_color(if added { STAGE } else { BUTTON });
                        })
                        .on_tapped(move |_me, app: &mut App, _cx| {
                            if let Some(added) = app.added.get_mut(i) {
                                *added = !*added;
                            }
                        }),
                ))
        })
        .collect();
    panel(
        "3D in a scrolling list",
        "Thirty pieces of furniture, each with a live 3D thumbnail: point at a row to turn it, or drag a thumbnail sideways. Rows off screen draw nothing.",
        (
            SkiaLayout::wrap().spacing(8).fill_x().margin((0.0, 0.0, 0.0, 8.0)).children((
                SkiaLabel::new("Shop").font_size(18).text_color(INK),
                SkiaLabel::new("")
                    .font_size(13)
                    .text_color(MUTED)
                    .horizontal_options(LayoutOptions::End)
                    .vertical_options(LayoutOptions::Center)
                    .observe(|me, app: &App| me.set_text(format!("3D thumbnails drawn now: {} of {ROWS}", app.thumbs_drawn))),
            )),
            SkiaShape::new().corner_radius(12).background_color(PAGE).fill_x().height_request(440).is_clipped_to_bounds(true).children((SkiaScroll::new()
                .fill()
                .content(SkiaLayout::column().fill_x().spacing(2).children(rows)),)),
        ),
    )
}

// ---------------------------------------------------------------- 3. card flip

/// Starts a half turn to the other face, unless one is under way.
fn flip(app: &mut App) {
    if app.flip >= 1.0 {
        app.flip = 0.0;
    }
}

fn card_flip(app: &mut App) -> Build<SkiaShape> {
    let (front, back) = app.card_faces.clone().expect("made at build");
    app.flip = 1.0;
    panel(
        "Card flip",
        "A loyalty card in 3D: it leans toward the pointer and turns over to show its back.",
        (split((
            left_column(SkiaShape::new(), VIEW.0, VIEW.1).corner_radius(14).background_color(STAGE).children((SkiaModel3D::from_model(&card())
                .view_angles(0.0, 0.0)
                .zoom(0.74)
                .tilt_to_pointer(14.0)
                .antialias(true)
                .material_image("front", front)
                .material_image("back", back)
                .fill()
                .assign(&mut app.card)
                .on_tapped_3d(|_me, app: &mut App, _cx, _hit| flip(app)),)),
            SkiaLayout::column().spacing(12).fill_x().children((
                SkiaLabel::new("Drawn Rewards Gold").font_size(24).text_color(INK),
                SkiaLabel::new("12,480 points · member since 2021").font_size(14).text_color(MUTED),
                SkiaLabel::new("Both faces are images drawn in code with Skia and put on the card's two halves.").font_size(13).text_color(MUTED),
                button("Show the back").width_request(170).observe(|me, app: &App| me.set_text(if app.back_shown { "Show the front" } else { "Show the back" })).on_tapped(|_me, app: &mut App, _cx| flip(app)),
            )),
        )),),
    )
}

// ---------------------------------------------------------------- 4. globe with pins

/// Stops the spin and glides (0.6 s) the globe round until the city faces the viewer.
fn turn_to_city(app: &mut App, cx: &mut Cx, city: usize) {
    let (_, lat, lon) = CITIES[city];
    let [x, _, z] = on_globe(lat, lon);
    // The camera's yaw puts it at (sin yaw, cos yaw) round the globe: right in front of the city.
    let to = (x.atan2(z).to_degrees(), lat.clamp(-50.0, 60.0));
    let from = cx.find::<SkiaModel3D>(app.globe).map_or(to, |g| (g.view().0, g.view().1));
    // The short way round.
    let to = (from.0 + ((to.0 - from.0 + 540.0).rem_euclid(360.0) - 180.0), to.1);
    app.globe_glide = Some((from, to, 0.0));
    app.spin_paused = true;
    if let Some(mut globe) = cx.get_mut(app.globe) {
        globe.set_auto_rotate(0.0);
    }
    place_pins(app, cx, None);
}

/// A pin's point from its top left (the tip), and how far above the point its label sits.
const PIN_ANCHOR: (f32, f32) = (16.0, 43.0);
const PIN_LABEL_RISE: f32 = 78.0;
const PIN_RED: Color = Color::from_rgb(234, 67, 53);
/// The city label's width: labels are centered before their first layout.
const CHIP_WIDTH: f32 = 130.0;

/// A map pin for a city: a glossy teardrop with a soft shadow and a white dot inside, and a ring
/// pulsing where it stands (its handle goes to `pulse`).
fn pin(name: &str, pulse: &mut Handle<SkiaShape>) -> Build<SkiaLayout> {
    SkiaLayout::new().is_visible(false).accessibility_role(Aria::BUTTON).accessibility_label(name).width_request(32).height_request(48).children((
        SkiaShape::new().shape_type(ShapeType::Ellipse).width_request(30).height_request(10).left(1).top(38).stroke_color(PIN_RED).stroke_width(2).assign(pulse),
        SkiaShape::new()
            .shape_type(ShapeType::Path)
            .path_data("M16 0 C7.2 0 0 7 0 15.6 C0 27 16 43 16 43 C16 43 32 27 32 15.6 C32 7 24.8 0 16 0 Z")
            .width_request(32)
            .height_request(43)
            .background_color(PIN_RED)
            .fill_gradient(SkiaGradient::new(GradientType::Linear, [Color::from_rgb(255, 112, 96), Color::from_rgb(200, 40, 32)]).start_x_ratio(0.3).start_y_ratio(0.0).end_x_ratio(0.7).end_y_ratio(1.0))
            .shadows(SkiaShadow::new(Color::BLACK).x(0).y(3).blur(5).opacity(0.45)),
        SkiaShape::new().shape_type(ShapeType::Circle).width_request(12).height_request(12).left(10).top(10).background_color(Color::WHITE),
        SkiaShape::new().shape_type(ShapeType::Ellipse).width_request(10).height_request(5).left(8).top(4).background_color(Color::from_argb(90, 255, 255, 255)),
    ))
}

/// A clean label chip for a city.
fn chip_label(name: &str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(CHIP_WIDTH)
        .height_request(30)
        .corner_radius(15)
        .background_color(Color::from_rgb(250, 250, 252))
        .shadows(SkiaShadow::new(Color::BLACK).x(0).y(2).blur(6).opacity(0.35))
        .is_visible(false)
        .children((SkiaLabel::new(name).font_size(14).text_color(Color::from_rgb(30, 32, 44)).horizontal_options(LayoutOptions::Center).vertical_options(LayoutOptions::Center),))
}

fn place_pins(app: &App, cx: &mut Cx, model: Option<&SkiaModel3D>) {
    let points: Vec<[f32; 3]> = CITIES.iter().map(|c| on_globe(c.1, c.2)).collect();
    let on_screen = match model {
        Some(m) => positions(Some(m), &points),
        None => positions(cx.find(app.globe), &points),
    };
    place(cx, on_screen, &app.pins, &app.pin_names, app.open_pin, PIN_ANCHOR, CHIP_WIDTH, PIN_LABEL_RISE);
}

fn globe_with_pins(app: &mut App) -> Build<SkiaShape> {
    let earth = app.earth.clone().expect("decoded at build");
    let stage = left_column(SkiaShape::new(), VIEW.0, VIEW.1).corner_radius(14).background_color(Color::from_rgb(8, 10, 22)).is_clipped_to_bounds(true).children((
        SkiaModel3D::from_model(&globe())
            .view_angles(-20.0, 15.0)
            .zoom(0.9)
            .auto_rotate(10.0)
            .pinch_zoom(true)
            .min_zoom(0.6)
            .max_zoom(1.6)
            .reset_on_double_tap(true)
            .antialias(true)
            .material_image("earth", earth)
            .fill()
            .assign(&mut app.globe)
            .on_view_changed(|me, app: &mut App, cx| place_pins(app, cx, Some(me.control_mut()))),
        // The pins, then their labels over every pin.
        SkiaLayout::new().fill().children((
            (0..CITIES.len())
                .map(|i| {
                    pin(CITIES[i].0, &mut app.pulses[i]).assign(&mut app.pins[i]).on_tapped(move |_me, app: &mut App, cx| {
                        app.open_pin = if app.open_pin == Some(i) { None } else { Some(i) };
                        place_pins(app, cx, None);
                    })
                })
                .collect::<Vec<_>>(),
            (0..CITIES.len()).map(|i| chip_label(CITIES[i].0).assign(&mut app.pin_names[i])).collect::<Vec<_>>(),
        )),
    ));
    panel(
        "Globe with pins",
        "Drag the Earth to turn it. Pins stay on their cities and hide behind the globe; tap one for its name.",
        (split((
            stage,
            SkiaLayout::column().spacing(10).fill_x().children((
                SkiaLabel::new("Offices").font_size(24).text_color(INK),
                SkiaLabel::new("Pick a city to turn the globe to it, or tap a pin.").font_size(13).text_color(MUTED),
                SkiaLayout::wrap().spacing(8).children(
                    (0..CITIES.len())
                        .map(|i| {
                            chip(CITIES[i].0, move |app| app.open_pin == Some(i)).accessibility_role(Aria::RADIO).on_tapped(move |_me, app: &mut App, cx| {
                                app.open_pin = Some(i);
                                turn_to_city(app, cx, i);
                            })
                        })
                        .collect::<Vec<_>>(),
                ),
                SkiaButton::new("Pause spin").background_color(STAGE).observe(|me, app: &App| me.set_text(if app.spin_paused { "Spin" } else { "Pause spin" })).on_tapped(|_me, app: &mut App, cx| {
                    app.spin_paused = !app.spin_paused;
                    app.globe_glide = None;
                    if let Some(mut globe) = cx.get_mut(app.globe) {
                        globe.set_auto_rotate(if app.spin_paused { 0.0 } else { 10.0 });
                    }
                }),
                SkiaLabel::new("Image: NASA Blue Marble").font_size(12).text_color(MUTED),
            )),
        )),),
    )
}

// ---------------------------------------------------------------- 5. 3D next to normal UI

/// A bone's rotation from its rest pose (the file's), turned by degrees around its own x, y and z,
/// as the euler angles `set_node_rotation` takes.
fn bone(rest: [f32; 4], x: f32, y: f32, z: f32) -> [f32; 3] {
    let q = Quat::from_array(rest) * Quat::from_euler(EulerRot::XYZ, x.to_radians(), y.to_radians(), z.to_radians());
    let (a, b, c) = q.to_euler(EulerRot::XYZ);
    [a.to_degrees(), b.to_degrees(), c.to_degrees()]
}

const HEAD_REST: [f32; 4] = [-0.056, 0.0, 0.0, 0.998];
const UPPER_L_REST: [f32; 4] = [0.527, 0.504, 0.474, -0.494];
const UPPER_R_REST: [f32; 4] = [0.527, -0.504, -0.474, -0.494];
const LOWER_L_REST: [f32; 4] = [-0.029, 0.262, -0.281, 0.923];
const LOWER_R_REST: [f32; 4] = [-0.029, -0.262, 0.281, 0.923];
const EAR1_L_REST: [f32; 4] = [0.095, -0.049, -0.194, 0.975];
const EAR1_R_REST: [f32; 4] = [0.095, 0.049, 0.193, 0.975];
const EAR2_L_REST: [f32; 4] = [-0.237, -0.034, -0.011, 0.971];
const EAR2_R_REST: [f32; 4] = [-0.223, 0.028, 0.012, 0.974];

/// The mascot watches the field that has the keyboard, and covers its eyes for the password.
fn pose_mascot(app: &App, cx: &mut Cx) {
    let Some(mut mascot) = cx.get_mut(app.mascot) else { return };
    match app.focused {
        Some(Field::Password) => {
            // Its arms are too short to reach its eyes: the long ears fold down over them, the
            // hands go up to the cheeks (poses found by forward kinematics on the file's bones).
            mascot.reset_node("Head");
            mascot.set_node_rotation("Ear1.L", bone(EAR1_L_REST, 150.0, 0.0, 0.0));
            mascot.set_node_rotation("Ear1.R", bone(EAR1_R_REST, 150.0, 0.0, 0.0));
            mascot.set_node_rotation("Ear2.L", bone(EAR2_L_REST, -30.0, 0.0, 0.0));
            mascot.set_node_rotation("Ear2.R", bone(EAR2_R_REST, -30.0, 0.0, 0.0));
            mascot.set_node_rotation("UpperArm.L", bone(UPPER_L_REST, -150.0, -150.0, 60.0));
            mascot.set_node_rotation("LowerArm.L", bone(LOWER_L_REST, 0.0, 0.0, 60.0));
            mascot.set_node_rotation("UpperArm.R", bone(UPPER_R_REST, 30.0, 30.0, 120.0));
            mascot.set_node_rotation("LowerArm.R", bone(LOWER_R_REST, 0.0, 0.0, -60.0));
        }
        focused => {
            for part in ["UpperArm.L", "UpperArm.R", "LowerArm.L", "LowerArm.R", "Ear1.L", "Ear1.R", "Ear2.L", "Ear2.R"] {
                mascot.reset_node(part);
            }
            match focused {
                // The form is to the mascot's right (the viewer's left): turn there, and down the fields.
                // Along the text: from the field's start (far left) toward its end as it grows.
                Some(Field::Name) => mascot.set_node_rotation("Head", bone(HEAD_REST, 10.0, along(app.name.chars().count()), 0.0)),
                Some(Field::Email) => mascot.set_node_rotation("Head", bone(HEAD_REST, 22.0, along(app.email.chars().count()), 0.0)),
                _ => mascot.reset_node("Head"),
            };
        }
    }
}

/// The mascot's view yaw: its body already faces the form.
const MASCOT_YAW: f32 = 32.0;

/// The head's turn toward the caret, on top of the body's: -30 degrees at the start of a field
/// (its left end, farther away), -10 at 30 letters.
fn along(letters: usize) -> f32 {
    -30.0 + 20.0 * (letters as f32 / 30.0).min(1.0)
}

/// What stops a sign-up, if anything.
fn sign_up_problem(app: &App) -> Option<&'static str> {
    if app.name.trim().is_empty() {
        Some("Please tell us your name.")
    } else if !app.email.contains('@') || !app.email.contains('.') {
        Some("That email does not look right.")
    } else if app.password_len < 8 {
        Some("Use at least 8 characters for the password.")
    } else {
        None
    }
}

fn form_field(placeholder: &str, which: Field) -> Build<SkiaEditor> {
    SkiaEditor::new()
        .placeholder_text(placeholder)
        .accessibility_label(placeholder)
        .fill_x()
        .height_request(44)
        .padding((12.0, 10.0))
        .font_size(15)
        .text_color(INK)
        .background_color(STAGE)
        .corner_radius(10)
        .on_focus_changed(move |_me, app: &mut App, cx, focused| {
            if focused {
                app.focused = Some(which);
            } else if app.focused == Some(which) {
                app.focused = None;
            }
            pose_mascot(app, cx);
        })
}

fn sign_up(app: &mut App) -> Build<SkiaShape> {
    let form = SkiaShape::new()
        .corner_radius(14)
        .background_color(PAGE)
        .padding(24)
        .width_request(420)
        .observe(|me, app: &App| {
            let wide = two_columns(app);
            me.set_width_request(if wide { 420.0 } else { -1.0 });
            me.set_horizontal_options(if wide { LayoutOptions::Start } else { LayoutOptions::Fill });
        })
        .children((SkiaLayout::column().spacing(12).fill_x().children((
        SkiaLabel::new("Create your account").font_size(22).text_color(INK),
        SkiaLabel::new("It takes a minute. Our mascot will not peek at your password.").font_size(13).text_color(MUTED),
        form_field("Name", Field::Name).on_text_changed(|_me, app: &mut App, cx, text| {
            app.name = text.to_owned();
            pose_mascot(app, cx);
        }),
        form_field("Email", Field::Email).on_text_changed(|_me, app: &mut App, cx, text| {
            app.email = text.to_owned();
            pose_mascot(app, cx);
        }),
        form_field("Password", Field::Password).is_password(true).on_text_changed(|_me, app: &mut App, _cx, text| app.password_len = text.chars().count()),
        button("Sign up").fill_x().on_tapped(|_me, app: &mut App, cx| {
            app.problem = sign_up_problem(app);
            app.signed_up = app.problem.is_none();
            app.focused = None;
            pose_mascot(app, cx);
            if let Some(mut mascot) = cx.get_mut(app.mascot) {
                mascot.play_once(if app.signed_up { "CharacterArmature|Wave" } else { "CharacterArmature|No" });
            }
        }),
        SkiaLabel::new("").font_size(14).text_color(ACCENT).observe(|me, app: &App| {
            me.set_text(match (app.signed_up, app.problem) {
                (true, _) => format!("Welcome aboard, {}!", app.name.trim()),
                (false, Some(problem)) => problem.to_owned(),
                _ => String::new(),
            });
            me.set_text_color(if app.signed_up { ACCENT } else { DANGER });
        }),
    )),));
    panel(
        "3D next to normal UI",
        "A plain sign-up form with a 3D mascot beside it: it watches what you type and looks away from your password.",
        (split((
            form,
            SkiaShape::new()
                .corner_radius(14)
                .background_color(STAGE)
                .fill_x()
                .height_request(VIEW.1)
                .observe(|me, app: &App| me.set_height_request(if two_columns(app) { VIEW.1 } else { STACKED_VIEW }))
                .children((SkiaModel3D::new(AVATAR)
                .animation("CharacterArmature|Idle")
                // Seen from its left: it faces the form, which is to the left on screen.
                .view_angles(MASCOT_YAW, 8.0)
                .orbit(false)
                .fill()
                .assign(&mut app.mascot),)),
        )),),
    )
}

// ---------------------------------------------------------------- the frame loop

fn game_loop(app: &mut App, cx: &mut Cx, delta: f32) {
    app.time += delta;
    let t = app.time;
    if app.load == LoadState3D::Loading
        && let Some(mut spinner) = cx.get_mut(app.spinner)
    {
        spinner.set_value1((t * 360.0) % 360.0);
    }
    for (i, pulse) in app.pulses.iter().enumerate() {
        // Each pin's pulse grows and fades over 1.6 s, the pins out of step.
        let phase = (t / 1.6 + i as f32 * 0.17).fract();
        if let Some(mut ring) = cx.get_mut(*pulse) {
            let grow = 0.4 + phase;
            ring.set_scale_x(grow);
            ring.set_scale_y(grow);
            ring.set_opacity(1.0 - phase);
        }
    }
    let drawn = app.thumbs.iter().filter(|h| !cx.visible_rect(**h).is_empty()).count();
    if drawn != app.thumbs_drawn {
        app.thumbs_drawn = drawn;
    }
    let price = SWATCHES[app.swatch].4;
    if app.price_shown != price {
        // Counts at 2,800 dollars a second: the largest jump (Linen to Leather) takes a quarter second.
        let step = (price - app.price_shown).signum() * (700.0 * 4.0 * delta).max(1.0);
        app.price_shown = if (price - app.price_shown).abs() <= step.abs() { price } else { app.price_shown + step };
    }
    if let Some((from, to, s)) = app.globe_glide {
        let s = (s + delta / 0.6).min(1.0);
        let v = ease_in_out(s);
        if let Some(mut globe) = cx.get_mut(app.globe) {
            globe.set_view_angles(from.0 + (to.0 - from.0) * v, from.1 + (to.1 - from.1) * v);
        }
        app.globe_glide = (s < 1.0).then_some((from, to, s));
    }
    if app.flip < 1.0 {
        app.flip = (app.flip + delta / 0.7).min(1.0);
        let from = if app.back_shown { 180.0 } else { 0.0 };
        let yaw = from + 180.0 * ease_in_out(app.flip);
        if let Some(mut card) = cx.get_mut(app.card) {
            card.set_view_angles(yaw, 0.0);
        }
        if app.flip >= 1.0 {
            app.back_shown = !app.back_shown;
        }
    }
    if app.spin < 1.0 {
        app.spin = (app.spin + delta / 0.9).min(1.0);
        if let Some(mut medal) = cx.get_mut(app.medal) {
            medal.set_view_angles(MEDAL_YAW + 360.0 * ease_out(app.spin), 12.0);
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
            let w = Walker { x: d.next() * FIELD.0, y: GROUND.0 + 10.0 + d.next() * (GROUND.1 - GROUND.0 - 10.0), speed: (if d.next() < 0.5 { -1.0 } else { 1.0 }) * (30.0 + d.next() * 50.0) };
            let name = format!("m{i}");
            field.add(&name, "slime", w.x, w.y, 46.0);
            field.play(&name, "CharacterArmature|Walk");
            field.set_speed(&name, 0.8 + d.next() * 0.5);
            field.set_facing(&name, if w.speed > 0.0 { 90.0 } else { -90.0 });
            app.walkers.push(w);
        }
    }
    // The scene pans: every layer slides by its share, the ground (and the mobs on it) the most.
    app.pan += PAN_SPEED * delta;
    for (pair, share) in app.scenery.iter().zip(PARALLAX) {
        let shift = (app.pan * share).rem_euclid(FIELD.0);
        for (copy, layer) in pair.iter().enumerate() {
            if let Some(mut layer) = cx.get_mut(*layer) {
                // Whole device pixels: the two copies meet on a pixel edge, with no blended seam.
                let scale = layer.base().scale.max(0.01);
                layer.set_translation_x(((copy as f32 * FIELD.0 - shift) * scale).round() / scale);
            }
        }
    }
    if let Some(mut field) = cx.get_mut(app.field) {
        for (i, w) in app.walkers.iter_mut().enumerate() {
            // Walking on the ground, which moves with the pan; off one side, back in at the other.
            w.x += (w.speed - PAN_SPEED) * delta;
            if w.x < -40.0 {
                w.x += FIELD.0 + 80.0;
            } else if w.x > FIELD.0 + 40.0 {
                w.x -= FIELD.0 + 80.0;
            }
            field.set_position(&format!("m{i}"), w.x, w.y);
        }
    }
}

// ---------------------------------------------------------------- the mobs' side-scroller

/// Where the mobs walk: the top and bottom of the ground band, field points.
const GROUND: (f32, f32) = (196.0, 262.0);
/// How fast the scene pans, points per second, and each layer's share of it (far ones move less).
const PAN_SPEED: f32 = 22.0;
const PARALLAX: [f32; 4] = [0.08, 0.2, 0.45, 1.0];

/// A ridge line across the field (1000 points) down to `bottom`, as a shape: its waves repeat every
/// 1000 points, so two copies side by side join seamlessly. A shape stretches its path's own box over
/// its rect, so the rect is set to exactly that box.
fn ridge(base: f32, waves: &[(f32, f32, f32)], bottom: f32, colors: (Color, Color)) -> Build<SkiaShape> {
    let y = |x: f32| base - waves.iter().map(|(amp, period, phase)| amp * (x / period * TAU + phase).sin()).sum::<f32>();
    let points: Vec<(f32, f32)> = (0..=100).map(|i| (i as f32 * 10.0, y(i as f32 * 10.0))).collect();
    let top = points.iter().map(|p| p.1).fold(bottom, f32::min);
    let mut d = format!("M0 {bottom}");
    for (x, y) in &points {
        d.push_str(&format!(" L{x:.0} {y:.1}"));
    }
    d.push_str(&format!(" L1000 {bottom} Z"));
    SkiaShape::new()
        .shape_type(ShapeType::Path)
        .path_data(d)
        .width_request(FIELD.0)
        .height_request(bottom - top)
        .top(top)
        .background_color(colors.0)
        .fill_gradient(gradient(colors.0, colors.1))
}

/// A layer the field's size, cached as one picture: the frame loop only slides it (with its twin
/// beside it).
fn layer(children: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(FIELD.0).height_request(FIELD.1).use_cache(CacheType::Operations).children(children)
}

fn gradient(from: Color, to: Color) -> SkiaGradient {
    SkiaGradient::new(GradientType::Linear, [from, to]).start_x_ratio(0.5).start_y_ratio(0.0).end_x_ratio(0.5).end_y_ratio(1.0)
}

/// One copy of each moving layer, far to near: clouds, far hills, near hills, the ground and its path.
fn scenery_layers() -> [Build<SkiaLayout>; 4] {
    let h = FIELD.1;
    let cloud = |x: f32, y: f32, k: f32| {
        SkiaLayout::new().left(x).top(y).children((
            SkiaShape::new().shape_type(ShapeType::Ellipse).width_request(90.0 * k).height_request(30.0 * k).top(14.0 * k).background_color(Color::from_argb(220, 255, 255, 255)),
            SkiaShape::new().shape_type(ShapeType::Circle).width_request(44.0 * k).height_request(44.0 * k).left(18.0 * k).background_color(Color::from_argb(230, 255, 255, 255)),
            SkiaShape::new().shape_type(ShapeType::Circle).width_request(32.0 * k).height_request(32.0 * k).left(46.0 * k).top(8.0 * k).background_color(Color::from_argb(230, 255, 255, 255)),
        ))
    };
    [
        layer([(60.0, 26.0, 1.0), (340.0, 52.0, 0.7), (610.0, 18.0, 1.2), (860.0, 60.0, 0.8)].map(|(x, y, k)| cloud(x, y, k)).into_iter().collect::<Vec<_>>()),
        layer((ridge(150.0, &[(22.0, 1000.0, 0.3), (12.0, 500.0, 1.7), (5.0, 250.0, 0.4)], h, (Color::from_rgb(150, 190, 196), Color::from_rgb(176, 210, 210))),)),
        layer((ridge(190.0, &[(26.0, 1000.0, 2.1), (14.0, 333.33, 0.9), (6.0, 200.0, 2.5)], h, (Color::from_rgb(98, 160, 104), Color::from_rgb(70, 128, 80))),)),
        layer((
            // Grass, a lighter rim along its top, and the dirt path the mobs walk on.
            ridge(GROUND.0 - 6.0, &[(3.0, 500.0, 0.0), (2.0, 125.0, 1.3)], h, (Color::from_rgb(112, 178, 72), Color::from_rgb(64, 120, 48))),
            ridge(GROUND.0 + 22.0, &[(4.0, 1000.0, 0.8), (3.0, 250.0, 2.2)], GROUND.1 + 4.0, (Color::from_rgb(214, 182, 128), Color::from_rgb(190, 152, 98))),
        )),
    ]
}

/// Sky and sun standing still, then each moving layer twice, side by side (their handles go to
/// `layers`), painted with gradients and smooth outlines.
fn side_scroller(layers: &mut [[Handle<SkiaLayout>; 2]; 4]) -> Vec<Detached> {
    let mut children: Vec<Detached> = vec![
        SkiaShape::new().fill().background_color(Color::from_rgb(120, 190, 240)).fill_gradient(gradient(Color::from_rgb(92, 164, 232), Color::from_rgb(214, 238, 250))).into(),
        SkiaShape::new()
            .shape_type(ShapeType::Circle)
            .width_request(70)
            .height_request(70)
            .left(760)
            .top(34)
            .background_color(Color::from_rgb(255, 236, 170))
            .shadows(SkiaShadow::new(Color::from_rgb(255, 230, 150)).x(0).y(0).blur(26).opacity(0.9))
            .into(),
    ];
    // Far layers first, the two copies of each depth together.
    let (mut left, mut right) = (scenery_layers().into_iter(), scenery_layers().into_iter());
    for handles in layers.iter_mut() {
        let (a, b) = (left.next().expect("four layers"), right.next().expect("four layers"));
        children.push(a.assign(&mut handles[0]).into());
        children.push(b.translation_x(FIELD.0).assign(&mut handles[1]).into());
    }
    children
}

fn medal_colors(app: &App, cx: &mut Cx) {
    if let Some(mut medal) = cx.get_mut(app.medal) {
        for (part, color) in [("disc", GREY), ("inner", Color::from_rgb(222, 219, 230)), ("star", Color::from_rgb(244, 242, 248))] {
            medal.set_material_color(part, (!app.unlocked).then_some(color));
        }
    }
}

fn next_label(app: &mut App, cx: &mut Cx) {
    app.label = (app.label + 1) % app.labels.len();
    if let Some(mut product) = cx.get_mut(app.product) {
        product.set_material_image("box", Some(app.labels[app.label].clone()));
    }
}

// ---------------------------------------------------------------- the page

/// The boss fight's dragon: idle and hit animations.
const BOSS_IDLE: &str = "CharacterArmature|Flying_Idle";
const BOSS_HIT: &str = "CharacterArmature|HitReact";

/// How a move plays.
#[derive(Clone, Copy, PartialEq)]
enum MoveKind {
    /// Loops until another move.
    Loop,
    /// Plays once, then back to the loop.
    Once,
    /// Plays once and stays on its last frame.
    Hold,
}

/// The boss's moves: the caption, the glTF animation and how it plays, in the order of a fight.
const MOVES: [(&str, &str, MoveKind); 7] = [
    ("Idle", BOSS_IDLE, MoveKind::Loop),
    ("Fly", "CharacterArmature|Fast_Flying", MoveKind::Loop),
    ("Punch", "CharacterArmature|Punch", MoveKind::Once),
    ("Headbutt", "CharacterArmature|Headbutt", MoveKind::Once),
    ("Yes", "CharacterArmature|Yes", MoveKind::Once),
    ("No", "CharacterArmature|No", MoveKind::Once),
    ("Death", "CharacterArmature|Death", MoveKind::Hold),
];

/// Skin colors for the dragon's main material; `None` = the file's own.
const SKINS: [(&str, Option<Color>); 4] = [
    ("Ember", None),
    ("Frost", Some(Color::from_rgb(70, 150, 230))),
    ("Venom", Some(Color::from_rgb(90, 190, 80))),
    ("Shadow", Some(Color::from_rgb(70, 60, 90))),
];

const EMBER: Color = Color::from_rgb(190, 70, 30);
const BOSS_BUTTON: Color = Color::from_rgb(44, 40, 52);
const MOVES_ACCENT: Color = Color::from_rgb(255, 122, 69);
const BOSS_TOGGLE: Color = Color::from_rgb(60, 54, 72);
const RAGE: Color = Color::from_rgb(200, 30, 40);
const BOSS_MUTED: Color = Color::from_rgb(190, 180, 200);

fn move_caption(animation: &str) -> &'static str {
    MOVES.iter().find(|m| m.1 == animation).map_or(if animation == BOSS_HIT { "Hit" } else { "Move" }, |m| m.0)
}

fn boss_caption(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(13).text_color(BOSS_MUTED).horizontal_options(LayoutOptions::Center).accessibility_role(Aria::HEADING)
}

fn move_button((caption, animation, kind): (&'static str, &'static str, MoveKind)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .width_request(100)
        .background_color(BOSS_BUTTON)
        .observe(move |me, app: &App| me.set_background_color(if app.boss_looping == caption { MOVES_ACCENT } else { BOSS_BUTTON }))
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut boss) = cx.get_mut(app.boss) {
                match kind {
                    MoveKind::Loop => {
                        boss.play(animation);
                        app.boss_looping = caption;
                    }
                    MoveKind::Once => {
                        boss.play_once(animation);
                    }
                    MoveKind::Hold => {
                        boss.hold(animation);
                        app.boss_looping = caption;
                    }
                }
            }
            app.boss_news = format!("{caption}…");
        })
}

fn skin_button((caption, color): (&'static str, Option<Color>)) -> Build<SkiaButton> {
    let swatch = color.unwrap_or(EMBER);
    SkiaButton::new(caption)
        .width_request(100)
        .background_color(BOSS_BUTTON)
        .stroke_color(swatch)
        .stroke_width(2)
        .accessibility_role(Aria::RADIO)
        .observe(move |me, app: &App| {
            let on = app.boss_skin == caption;
            me.set_background_color(if on { swatch } else { BOSS_BUTTON });
            me.set_accessibility_is_pressed(Some(on));
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut boss) = cx.get_mut(app.boss) {
                boss.set_material_color("Dragon_Main", color);
            }
            app.boss_skin = caption;
        })
}

/// The boss fight: a 3D dragon in a DrawnUI screen, driven by the app (the .NET gallery's BossPage).
fn boss_page(app: &mut App) -> Build<SkiaLayout> {
    (app.boss_looping, app.boss_skin) = ("Idle", "Ember");
    app.boss_news = "Tap the dragon to hit it.".into();
    SkiaLayout::new().fill().children((
        // The arena: a warm light behind the boss, drawn on the GPU once and kept there.
        SkiaShape::new().fill().background_color(Color::from_rgb(18, 14, 22)).use_cache(CacheType::GPU).fill_gradient(SkiaGradient {
            start_x_ratio: 0.5,
            start_y_ratio: 0.42,
            ..SkiaGradient::new(GradientType::Circular, vec![Color::from_rgb(92, 38, 30), Color::from_rgb(18, 14, 22)])
        }),
        SkiaGrid::new().fill().padding(20).row_spacing(10).row_definitions("Auto,*,Auto,Auto").children((
            SkiaLayout::column().spacing(8).fill_x().use_cache(CacheType::Image).row(0).children((
                SkiaLabel::new("BOSS FIGHT").font_size(30).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center).accessibility_role(Aria::HEADING),
                SkiaLabel::new("A glTF model drawn in 3D inside DrawnUI, driven by the app. Drag it to turn it, tap it to hit it.")
                    .font_size(15)
                    .text_color(BOSS_MUTED)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center),
            )),
            SkiaModel3D::new(DRAGON)
                .animation(BOSS_IDLE)
                .view_angles(30.0, 8.0)
                .zoom(0.75)
                .fill()
                .row(1)
                .assign(&mut app.boss)
                .on_tapped_3d(|me, app: &mut App, _cx, hit| {
                    // A hit plays once and goes back to the loop; a dead dragon does not react.
                    if app.boss_looping != "Death" {
                        me.play_once(BOSS_HIT);
                    }
                    app.boss_news = format!("Hit on {}", hit.material.replace('_', " "));
                })
                .on_animation_ended(|_me, app: &mut App, _cx, name| app.boss_news = format!("{} finished", move_caption(name))),
            SkiaLabel::new("")
                .font_size(15)
                .text_color(Color::WHITE)
                .horizontal_options(LayoutOptions::Center)
                .accessibility_live(Aria::LIVE_POLITE)
                .row(2)
                .observe(|me, app: &App| me.set_text(app.boss_news.clone())),
            SkiaLayout::column().spacing(10).fill_x().use_cache(CacheType::Image).row(3).children((
                boss_caption("MOVES"),
                SkiaLayout::wrap()
                    .spacing(8)
                    .horizontal_options(LayoutOptions::Center)
                    .accessibility_role(Aria::TOOLBAR)
                    .accessibility_label("Moves")
                    .children(MOVES.iter().copied().map(move_button).collect::<Vec<_>>()),
                boss_caption("SKIN AND EYES (materials by name)"),
                SkiaLayout::wrap().spacing(8).horizontal_options(LayoutOptions::Center).accessibility_role(Aria::TOOLBAR).accessibility_label("Skin and eyes").children((
                    SKINS.iter().copied().map(skin_button).collect::<Vec<_>>(),
                    SkiaButton::new("Rage: off")
                        .width_request(124)
                        .background_color(BOSS_TOGGLE)
                        .observe(|me, app: &App| {
                            me.set_text(if app.boss_rage { "Rage: on" } else { "Rage: off" });
                            me.set_background_color(if app.boss_rage { RAGE } else { BOSS_TOGGLE });
                            me.set_accessibility_is_pressed(Some(app.boss_rage));
                        })
                        .on_tapped(|_me, app: &mut App, cx| {
                            app.boss_rage = !app.boss_rage;
                            if let Some(mut boss) = cx.get_mut(app.boss) {
                                boss.set_material_color("Eye_White", app.boss_rage.then_some(Color::from_rgb(255, 40, 30)));
                            }
                        }),
                    SkiaButton::new("Turn: off")
                        .width_request(124)
                        .background_color(BOSS_TOGGLE)
                        .observe(|me, app: &App| {
                            me.set_text(if app.boss_turning { "Turn: on" } else { "Turn: off" });
                            me.set_accessibility_is_pressed(Some(app.boss_turning));
                        })
                        .on_tapped(|_me, app: &mut App, cx| {
                            app.boss_turning = !app.boss_turning;
                            if let Some(mut boss) = cx.get_mut(app.boss) {
                                boss.set_auto_rotate(if app.boss_turning { 30.0 } else { 0.0 });
                            }
                        }),
                )),
            )),
        )),
    ))
}

/// A header tab: filled when its screen shows; the hidden screen asks for no frames.
fn tab(caption: &str, index: usize) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .width_request(120)
        .background_color(if index == 0 { TAB_ON } else { TAB_OFF })
        .accessibility_role(Aria::TAB)
        .observe(move |me, app: &App| {
            me.set_background_color(if app.tab == index { TAB_ON } else { TAB_OFF });
            me.set_accessibility_is_pressed(Some(app.tab == index));
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            app.tab = index;
            if let Some(mut game) = cx.get_mut(app.game) {
                if index == 0 { game.start_loop(0.0) } else { game.stop_loop() }
            }
        })
}

/// The whole app: a header with the two screens' tabs over one drawing surface, the frame rate in
/// a corner (the .NET gallery's GalleryRoot).
fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::new().fill().background_color(PAGE).children((
        SkiaGrid::new().fill().row_spacing(0).row_definitions("Auto,*").children((
            // Static chrome over screens that draw every frame: blitted, not drawn again.
            SkiaLayout::new()
                .fill_x()
                .height_request(64)
                .padding((24, 0))
                .background_color(HEADER)
                .use_cache(CacheType::Image)
                .row(0)
                .observe(|me, app: &App| {
                    let one_line = app.width <= 0.0 || app.width >= HEADER_ONE_LINE;
                    me.set_layout_type(if one_line { LayoutType::Absolute } else { LayoutType::Column });
                    me.set_height_request(if one_line { 64.0 } else { -1.0 });
                    me.set_padding(if one_line { Thickness::new(24.0, 0.0, 24.0, 0.0) } else { Thickness::new(16.0, 12.0, 16.0, 12.0) });
                    me.set_spacing(10.0);
                })
                .children((
                SkiaLayout::row().spacing(12).vertical_options(LayoutOptions::Center).children((
                    SkiaShape::new()
                        .corner_radius(8)
                        .width_request(32)
                        .height_request(32)
                        .vertical_options(LayoutOptions::Center)
                        .fill_gradient(SkiaGradient {
                            start_x_ratio: 0.0,
                            start_y_ratio: 0.0,
                            end_x_ratio: 1.0,
                            end_y_ratio: 1.0,
                            ..SkiaGradient::new(GradientType::Linear, vec![TAB_ON, Color::from_rgb(255, 99, 132)])
                        })
                        .children(SkiaLabel::new("3D").font_size(13).text_color(Color::WHITE).center()),
                    SkiaLayout::column().spacing(0).vertical_options(LayoutOptions::Center).children((
                        SkiaLabel::new("DrawnUI 3D").font_size(17).text_color(INK).accessibility_role(Aria::HEADING),
                        SkiaLabel::new("The drawnui-add3d add-on for DrawnUI for Rust").font_size(12).text_color(MUTED),
                    )),
                )),
                SkiaLayout::row()
                    .spacing(8)
                    .horizontal_options(LayoutOptions::End)
                    .vertical_options(LayoutOptions::Center)
                    .accessibility_role(Aria::TAB_LIST)
                    .accessibility_label("Screens")
                    .children((tab("Gallery", 0), tab("Boss fight", 1))),
            )),
            SkiaLayout::new().fill().row(1).children((
                gallery(app).assign(&mut app.game).observe(|me, app: &App| me.set_is_visible(app.tab == 0)),
                boss_page(app).is_visible(false).observe(|me, app: &App| me.set_is_visible(app.tab == 1)),
            )),
        )),
        SkiaLabelFps::new()
            .vertical_options(LayoutOptions::End)
            .horizontal_options(LayoutOptions::End)
            .margin((0, 0, 8, 8))
            .background_color(Color::from_rgb(139, 0, 0))
            .text_color(Color::WHITE),
    ))
}

/// The gallery screen.
fn gallery(app: &mut App) -> Build<DrawnGame> {
    (app.mobs, app.dice, app.spin) = (40, Dice(7), 1.0);
    (app.labels, app.card_faces, app.earth) = (labels(), Some(card_faces()), Texture3D::decode(EARTH));
    app.fabrics = SWATCHES.iter().filter_map(|s| Texture3D::decode(s.1)).collect();
    app.price_shown = SWATCHES[0].4;
    let micro = SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((
        tile(
            "Like",
            "A heart that beats, and jumps when liked.",
            shaded(&heart_model(), -24.0, 12.0).assign(&mut app.heart).on_tapped_3d(|me, app: &mut App, _cx, _hit| {
                app.liked = !app.liked;
                app.pop = if app.liked { 1.0 } else { 0.0 };
                me.set_material_color("heart", (!app.liked).then_some(GREY));
            }),
            (button("Like").observe(|me, app: &App| me.set_text(if app.liked { "Unlike" } else { "Like" })).on_tapped(|_me, app: &mut App, cx| {
                app.liked = !app.liked;
                app.pop = if app.liked { 1.0 } else { 0.0 };
                if let Some(mut heart) = cx.get_mut(app.heart) {
                    heart.set_material_color("heart", (!app.liked).then_some(GREY));
                }
            }),),
        ),
        tile(
            "Celebration",
            "Confetti and stars falling: one 3D layer, many pieces.",
            SkiaScene3D::new().model_from("paper", &paper()).model_from("star", &confetti_star()).tilt(20.0).fill().assign(&mut app.rain),
            (button("Hooray").on_tapped(|_me, app: &mut App, _cx| {
                // A fresh shower: every piece starts again above the tile.
                for p in &mut app.pieces {
                    p.y = -12.0 - app.dice.next() * RAIN.1;
                }
            }),),
        ),
        tile(
            "Achievement",
            "A medal that lights up and spins when unlocked.",
            shaded(&medal(), MEDAL_YAW, 12.0).assign(&mut app.medal).on_tapped_3d(|_me, app: &mut App, cx, _hit| {
                app.spin = 0.0;
                medal_colors(app, cx);
            }),
            (button("Unlock").observe(|me, app: &App| me.set_text(if app.unlocked { "Lock" } else { "Unlock" })).on_tapped(|_me, app: &mut App, cx| {
                app.unlocked = !app.unlocked;
                medal_colors(app, cx);
                if app.unlocked {
                    app.spin = 0.0;
                    cx.scale_to(app.medal, 0.6, 0.6, 0, easing::linear);
                    cx.scale_to(app.medal, 1.0, 1.0, 500, easing::spring_out);
                }
            }),),
        ),
    ));
    let images = SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((tile(
        "Product box",
        "A box with its label: tap it or switch the design.",
        shaded(&product_box(), 28.0, 18.0)
            .auto_rotate(40.0)
            .material_image("box", app.labels[0].clone())
            .assign(&mut app.product)
            .on_tapped_3d(|_me, app: &mut App, cx, _hit| next_label(app, cx)),
        (
            SkiaButton::new("Next design").width_request(120).background_color(BUTTON).on_tapped(|_me, app: &mut App, cx| next_label(app, cx)),
            SkiaButton::new("Edges: hard").width_request(120).background_color(STAGE).observe(|me, app: &App| me.set_text(if app.smooth_edges { "Edges: smooth" } else { "Edges: hard" })).on_tapped(
                |_me, app: &mut App, cx| {
                    app.smooth_edges = !app.smooth_edges;
                    if let Some(mut product) = cx.get_mut(app.product) {
                        product.set_antialias(app.smooth_edges);
                    }
                },
            ),
        ),
    ),));
    let files = SkiaLayout::wrap().spacing(16).horizontal_options(LayoutOptions::Center).children((
        tile(
            "Avatar",
            "A character that idles, and waves when tapped.",
            SkiaModel3D::new(AVATAR).animation("CharacterArmature|Idle").view_angles(25.0, 10.0).fill().assign(&mut app.avatar).on_tapped_3d(|me, _app: &mut App, _cx, _hit| {
                me.play_once("CharacterArmature|Wave");
            }),
            (button("Wave").on_tapped(|_me, app: &mut App, cx| {
                if let Some(mut avatar) = cx.get_mut(app.avatar) {
                    avatar.play_once("CharacterArmature|Wave");
                }
            }),),
        ),
        tile(
            "Reward",
            "A chest that opens and closes.",
            SkiaModel3D::new(CHEST).animation("Chest_Open").playing(false).view_angles(30.0, 20.0).orbit(false).fill().assign(&mut app.chest),
            (button("Open").observe(|me, app: &App| me.set_text(if app.open { "Close" } else { "Open" })).on_tapped(|_me, app: &mut App, cx| {
                app.open = !app.open;
                if let Some(mut chest) = cx.get_mut(app.chest) {
                    chest.hold(if app.open { "Chest_Open" } else { "Chest_Close" });
                }
            }),),
        ),
    ));
    let mobs = panel(
        "Mobs on a 2D field",
        "Many 3D monsters from one model, moved like sprites and drawn by one layer in one pass. Tap one to hit it.",
        (
            // The whole field scaled down to the card when the card is narrower than it.
            SkiaShape::new()
                .fill_x()
                .maximum_width_request(FIELD.0)
                .height_request(FIELD.1)
                .observe(|me, app: &App| me.set_height_request(if app.width <= 0.0 { FIELD.1 } else { (FIELD.1 * card_inner(app.width).min(FIELD.0) / FIELD.0).round() }))
                .corner_radius(10)
                .is_clipped_to_bounds(true)
                .background_color(Color::from_rgb(120, 190, 240))
                .children((RescalingLayout::new(FIELD.0, FIELD.1).children((
                SkiaLayout::new().fill().children(side_scroller(&mut app.scenery)),
                SkiaScene3D::new()
                    .model("slime", MOB)
                    .tilt(30.0)
                    .fill()
                    .assign(&mut app.field)
                    .on_tapped_3d(|me, _app: &mut App, _cx, hit| {
                        me.play_once(&hit.object, "CharacterArmature|HitRecieve");
                    }),
            )),)),
            SkiaLayout::wrap().spacing(8).margin((0.0, 10.0, 0.0, 0.0)).children(
                [10usize, 40, 100].into_iter().map(|n| chip(&format!("{n} mobs"), move |app| app.mobs == n).width_request(110).on_tapped(move |_me, app: &mut App, _cx| app.mobs = n)).collect::<Vec<_>>(),
            ),
        ),
    );
    DrawnGame::new().fill().start_loop(0.0).on_game_loop(|_me, app: &mut App, cx, delta| game_loop(app, cx, delta)).children((
        SkiaScroll::new().fill().content(
            SkiaLayout::column().fill_x().padding(24).spacing(20).children((
                SkiaLabel::new("3D, made in the app").font_size(30).text_color(INK).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("Where 3D earns its place in an app: products, lists, cards, maps and forms first, then the fun.")
                    .font_size(15)
                    .text_color(MUTED)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center),
                product_viewer(app),
                product_list(app),
                card_flip(app),
                globe_with_pins(app),
                sign_up(app),
                SkiaLayout::column().fill_x().spacing(20).children((
                    section("Micro-interactions", "Small 3D touches that answer a tap."),
                    micro,
                    section("Images on 3D", "Any image on a 3D surface, switched live: drawn in code here, or a photo or a file."),
                    images,
                    section("From model files", "Free 3D models with their own animations, driven by the app."),
                    files,
                    section("Games", "Many 3D characters on one field, moved like sprites and drawn by one layer in one pass."),
                    mobs,
                )),
            )),
        ),
    ))
}

fn main() {
    drawnui::run("DrawnUI 3D Gallery", || {
        Box::new(
            Ui::new(App::default(), build)
                .font("Default", "assets/OpenSans-Regular.ttf")
                .background(PAGE)
                .on_canvas_resized(|app: &mut App, size, _cx| app.width = size.width),
        )
    });
}
