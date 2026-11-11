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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::f32::consts::TAU;

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
const CHAIR: &[u8] = include_bytes!("../assets/loungeDesignChair.glb");
const LAMP: &[u8] = include_bytes!("../assets/lampRoundFloor.glb");
const SOFA: &[u8] = include_bytes!("../assets/loungeSofa.glb");
const RADIO: &[u8] = include_bytes!("../assets/radio.glb");
const PLANT: &[u8] = include_bytes!("../assets/plantSmall2.glb");
const TV: &[u8] = include_bytes!("../assets/televisionVintage.glb");
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
const VIEW: (f32, f32) = (600.0, 380.0);
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
    b.add_rrect(RRect::new_rect_xy(Rect::from_wh(856.0, 540.0), 38.0, 38.0), None, None);
    b.detach()
}

/// A card: a front and a back half, each showing its own image, the back half turned round so its
/// image reads the right way from behind.
fn card() -> Model3D {
    let s = 0.004;
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
    let Some(typeface) = FontMgr::new().new_from_data(Data::new_copy(FONT), None) else { return };
    let font = Font::from_typeface(typeface, size);
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

/// The card's two faces.
fn card_faces() -> (Texture3D, Texture3D) {
    let front = painted(856, 540, |c, w, h| {
        c.draw_rect(Rect::from_wh(w, h), &linear((0.0, 0.0), (w, h), Color::from_rgb(40, 32, 120), Color::from_rgb(124, 58, 237)));
        c.draw_circle((w * 0.86, h * 0.1), 260.0, &fill(Color::from_argb(40, 255, 255, 255)));
        c.draw_circle((w * 0.1, h * 1.05), 200.0, &fill(Color::from_argb(28, 255, 255, 255)));
        text(c, "DrawnUI Bank", 56.0, 92.0, 42.0, Color::WHITE, false);
        // The chip.
        c.draw_round_rect(Rect::from_xywh(56.0, 170.0, 118.0, 88.0), 16.0, 16.0, &linear((56.0, 170.0), (174.0, 258.0), GOLD_LIGHT, GOLD));
        let mut lines = fill(Color::from_argb(120, 120, 80, 0));
        lines.set_style(skia::PaintStyle::Stroke).set_stroke_width(3.0);
        c.draw_round_rect(Rect::from_xywh(86.0, 190.0, 58.0, 48.0), 8.0, 8.0, &lines);
        // Contactless waves.
        let mut wave = fill(Color::from_argb(220, 255, 255, 255));
        wave.set_style(skia::PaintStyle::Stroke).set_stroke_width(6.0).set_stroke_cap(skia::PaintCap::Round);
        for (i, r) in [22.0, 38.0, 54.0].into_iter().enumerate() {
            let _ = i;
            c.draw_arc(Rect::from_xywh(212.0 - r, 214.0 - r, r * 2.0, r * 2.0), -45.0, 90.0, false, &wave);
        }
        text(c, "4242  4242  4242  4242", 56.0, 350.0, 50.0, Color::WHITE, false);
        text(c, "CARD HOLDER", 56.0, 420.0, 20.0, Color::from_argb(180, 255, 255, 255), false);
        text(c, "ALEX MORGAN", 56.0, 462.0, 34.0, Color::WHITE, false);
        text(c, "VALID THRU", 470.0, 420.0, 20.0, Color::from_argb(180, 255, 255, 255), false);
        text(c, "12/29", 470.0, 462.0, 34.0, Color::WHITE, false);
        // The bank's mark: a star in a ring.
        c.draw_circle((w - 110.0, h - 100.0), 58.0, &fill(Color::from_argb(60, 255, 255, 255)));
        let mut mark = star(5, 34.0, 15.0);
        mark = mark.make_offset((w - 110.0, h - 100.0));
        c.draw_path(&mark, &fill(GOLD));
    });
    let back = painted(856, 540, |c, w, h| {
        c.draw_rect(Rect::from_wh(w, h), &linear((0.0, 0.0), (w, h), Color::from_rgb(52, 44, 140), Color::from_rgb(40, 32, 120)));
        c.draw_rect(Rect::from_xywh(0.0, 56.0, w, 96.0), &fill(Color::from_rgb(18, 16, 28)));
        c.draw_round_rect(Rect::from_xywh(56.0, 200.0, 520.0, 72.0), 8.0, 8.0, &fill(Color::from_rgb(236, 234, 244)));
        text(c, "Alex Morgan", 76.0, 248.0, 34.0, Color::from_rgb(60, 60, 90), false);
        c.draw_round_rect(Rect::from_xywh(596.0, 200.0, 120.0, 72.0), 8.0, 8.0, &fill(Color::WHITE));
        text(c, "123", 656.0, 248.0, 34.0, Color::from_rgb(30, 30, 50), true);
        text(c, "Customer service +1 555 0100  ·  drawnui.net", 56.0, 350.0, 26.0, Color::from_argb(200, 255, 255, 255), false);
        text(c, "This card is a demo drawn in code.", 56.0, 392.0, 26.0, Color::from_argb(160, 255, 255, 255), false);
        c.draw_round_rect(Rect::from_xywh(w - 196.0, h - 150.0, 140.0, 94.0), 12.0, 12.0, &linear((w - 196.0, h - 150.0), (w - 56.0, h - 56.0), Color::from_rgb(180, 230, 255), Color::from_rgb(255, 180, 230)));
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

/// Upholstery swatches: name, the image on the "upholstery" material (CC0 textures from Poly Haven),
/// how matte it is (glTF roughness: leather shines a little), the swatch's color.
const SWATCHES: [(&str, &[u8], f32, Color); 4] = [
    ("Leather", LEATHER, 0.42, Color::from_rgb(92, 52, 26)),
    ("Wool", WOOL, 0.95, Color::from_rgb(122, 120, 114)),
    ("Velvet", VELVET, 0.7, Color::from_rgb(180, 40, 62)),
    ("Linen", LINEN, 1.0, Color::from_rgb(140, 172, 206)),
];

/// A row of the list: model, name, kind, price.
const PRODUCTS: [(&[u8], &str, &str); 6] =
    [(CHAIR, "Lounge chair", "Seating"), (LAMP, "Floor lamp", "Lighting"), (SOFA, "Sofa", "Seating"), (RADIO, "Radio", "Audio"), (PLANT, "Potted plant", "Decor"), (TV, "Vintage TV", "Audio & video")];
const ROWS: usize = 30;
const FINISHES: [&str; 5] = ["Classic", "Oak", "Studio", "Nordic", "Loft"];

/// Cities on the globe: name, latitude, longitude.
const CITIES: [(&str, f32, f32); 6] = [("New York", 40.7, -74.0), ("London", 51.5, -0.1), ("Tokyo", 35.7, 139.7), ("Sydney", -33.9, 151.2), ("Rio de Janeiro", -22.9, -43.2), ("Cape Town", -33.9, 18.4)];

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
    // Product viewer.
    viewer_slot: Handle<SkiaLayout>,
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
    // List.
    added: Vec<bool>,
    // Card.
    card: Handle<SkiaModel3D>,
    back_shown: bool,
    /// The flip, 0 to 1 while it turns.
    flip: f32,
    card_faces: Option<(Texture3D, Texture3D)>,
    // Globe.
    globe: Handle<SkiaModel3D>,
    pins: [Handle<SkiaShape>; 6],
    pin_names: [Handle<SkiaShape>; 6],
    open_pin: Option<usize>,
    earth: Option<Texture3D>,
    // Sign-up form.
    mascot: Handle<SkiaModel3D>,
    focused: Option<Field>,
    name: String,
    signed_up: bool,
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
        .children((SkiaLayout::column().spacing(10).width_request(PANEL).children((
            SkiaLabel::new(title).font_size(20).text_color(INK),
            SkiaLabel::new(caption).font_size(14).text_color(MUTED),
            SkiaLayout::column().fill_x().margin((0.0, 6.0, 0.0, 0.0)).children(content),
        )),))
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

/// A round marker anchored to a 3D point: a hotspot or a pin. Starts hidden until placed.
fn marker(size: f32, color: Color, name: &str) -> Build<SkiaShape> {
    SkiaShape::new()
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

/// A small label next to a marker. Starts hidden.
fn note(title: &str, text: &str) -> Build<SkiaShape> {
    SkiaShape::new()
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
fn place(cx: &mut Cx, on_screen: Vec<Option<PointOnScreen>>, markers: &[Handle<SkiaShape>], notes: &[Handle<SkiaShape>], open: Option<usize>, size: f32) {
    for (i, at) in on_screen.into_iter().enumerate() {
        let shown = at.filter(|p| !p.occluded);
        if let Some(mut m) = cx.get_mut(markers[i]) {
            m.set_is_visible(shown.is_some());
            if let Some(p) = shown {
                m.set_left(p.x - size / 2.0);
                m.set_top(p.y - size / 2.0);
            }
        }
        if let Some(mut n) = cx.get_mut(notes[i]) {
            n.set_is_visible(shown.is_some() && open == Some(i));
            if let Some(p) = shown {
                n.set_left(p.x + size / 2.0 + 6.0);
                n.set_top(p.y - 18.0);
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
    place(cx, on_screen, &app.spots[..n], &app.notes[..n], app.open_spot, 26.0);
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
    let stage = SkiaShape::new()
        .corner_radius(14)
        .background_color(STAGE)
        .width_request(VIEW.0)
        .height_request(VIEW.1)
        .is_clipped_to_bounds(true)
        .children((
            SkiaLayout::new().fill().assign(&mut app.viewer_slot).children((viewer(Source::App, &mut app.viewer),)),
            // Loading.
            SkiaLayout::column()
                .spacing(8)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center)
                .is_visible(false)
                .observe(|me, app: &App| me.set_is_visible(app.load == LoadState3D::Loading))
                .children((
                    SkiaLabel::new("Loading the model…").font_size(16).text_color(INK).horizontal_options(LayoutOptions::Center),
                    SkiaLabel::new(SOFA_URL).font_size(12).text_color(MUTED).horizontal_options(LayoutOptions::Center),
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
                .children((SkiaLayout::column().spacing(8).children((
                    SkiaLabel::new("The model could not be loaded").font_size(16).text_color(INK),
                    SkiaLabel::new(BROKEN_URL).font_size(12).text_color(MUTED),
                    SkiaButton::new("Show the sofa").background_color(BUTTON).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::App)),
                )),)),
            // Hotspots and their notes.
            SkiaLayout::new().fill().children(
                (0..3)
                    .flat_map(|i| {
                        [
                            marker(26.0, BUTTON, ["Seat", "Back", "Frame"][i]).assign(&mut app.spots[i]).on_tapped(move |_me, app: &mut App, cx| {
                                app.open_spot = if app.open_spot == Some(i) { None } else { Some(i) };
                                place_spots(app, cx, None);
                            }),
                            note(SPOTS[i].title, SPOTS[i].text).assign(&mut app.notes[i]),
                        ]
                    })
                    .collect::<Vec<_>>(),
            ),
            SkiaLabel::new("Drag to turn · pinch or scroll to zoom · double-tap to reset")
                .font_size(12)
                .text_color(MUTED)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::End)
                .margin((0.0, 0.0, 0.0, 10.0)),
        ));
    let side = SkiaLayout::column().spacing(12).fill_x().children((
        SkiaLabel::new("Walnut sofa").font_size(24).text_color(INK),
        SkiaLabel::new("$1,290").font_size(18).text_color(ACCENT),
        SkiaLabel::new("Tap a dot on the sofa to read about that part.").font_size(13).text_color(MUTED),
        SkiaLabel::new("Upholstery").font_size(14).text_color(INK).margin((0.0, 8.0, 0.0, 0.0)),
        SkiaLayout::row().spacing(14).children(
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
        SkiaLabel::new("Model").font_size(14).text_color(INK).margin((0.0, 8.0, 0.0, 0.0)),
        SkiaLayout::wrap().spacing(8).children((
            chip("In the app", |app| app.source == Source::App).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::App)),
            chip("From a URL", |app| app.source == Source::Url).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::Url)),
            chip("Broken URL", |app| app.source == Source::Broken).on_tapped(|_me, app: &mut App, cx| set_source(app, cx, Source::Broken)),
        )),
        SkiaButton::new("Reset view").background_color(STAGE).on_tapped(|_me, app: &mut App, cx| {
            if let Some(mut viewer) = cx.get_mut(app.viewer) {
                viewer.reset_view();
            }
        }),
    ));
    panel(
        "Product viewer",
        "A model from the app or from a URL, turned and zoomed, its upholstery swapped for real fabric images, with notes anchored to its parts.",
        (SkiaLayout::row().spacing(24).fill_x().children((stage, side)),),
    )
}

// ---------------------------------------------------------------- 2. 3D in a scrolling list

fn product_list(app: &mut App) -> Build<SkiaShape> {
    app.added = vec![false; ROWS];
    let rows: Vec<Build<SkiaLayout>> = (0..ROWS)
        .map(|i| {
            let (model, name, kind) = PRODUCTS[i % PRODUCTS.len()];
            let finish = FINISHES[(i / PRODUCTS.len()) % FINISHES.len()];
            let price = 49 + (i * 37) % 400;
            SkiaLayout::row().fill_x().spacing(16).padding((12.0, 10.0)).children((
                SkiaShape::new().corner_radius(12).background_color(STAGE).width_request(96).height_request(96).children((SkiaModel3D::new(model)
                    .view_angles(30.0 + i as f32 * 25.0, 18.0)
                    .auto_rotate(24.0)
                    .lock_pitch(true)
                    .fill(),)),
                SkiaLayout::column().spacing(4).vertical_options(LayoutOptions::Center).children((
                    SkiaLabel::new(format!("{name} · {finish}")).font_size(16).text_color(INK),
                    SkiaLabel::new(kind).font_size(13).text_color(MUTED),
                    SkiaLabel::new(format!("${price}")).font_size(15).text_color(ACCENT),
                )),
                SkiaButton::new("Add to cart")
                    .width_request(130)
                    .horizontal_options(LayoutOptions::End)
                    .vertical_options(LayoutOptions::Center)
                    .observe(move |me, app: &App| {
                        let added = app.added.get(i).copied().unwrap_or(false);
                        me.set_text(if added { "Added" } else { "Add to cart" });
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
        "Thirty rows with a 3D thumbnail each. A thumbnail turns only while it is on screen; drag it sideways to turn it, up or down to scroll.",
        (SkiaShape::new().corner_radius(12).background_color(PAGE).fill_x().height_request(420).is_clipped_to_bounds(true).children((SkiaScroll::new()
            .fill()
            .content(SkiaLayout::column().fill_x().spacing(2).children(rows)),)),),
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
        "A card drawn in code: it leans toward the pointer or finger, and flips to its back.",
        (SkiaLayout::row().spacing(24).fill_x().children((
            SkiaShape::new().corner_radius(14).background_color(STAGE).width_request(VIEW.0).height_request(VIEW.1).children((SkiaModel3D::from_model(&card())
                .view_angles(0.0, 0.0)
                .zoom(0.9)
                .tilt_to_pointer(14.0)
                .antialias(true)
                .material_image("front", front)
                .material_image("back", back)
                .fill()
                .assign(&mut app.card)
                .on_tapped_3d(|_me, app: &mut App, _cx, _hit| flip(app)),)),
            SkiaLayout::column().spacing(12).fill_x().children((
                SkiaLabel::new("DrawnUI Bank").font_size(24).text_color(INK),
                SkiaLabel::new("Both faces are images drawn with Skia in the app: name, number, chip and mark.").font_size(13).text_color(MUTED),
                SkiaLabel::new("Move the mouse over the card, or touch it, and it follows. Tap it or press Flip.").font_size(13).text_color(MUTED),
                button("Flip").observe(|me, app: &App| me.set_text(if app.back_shown { "Show the front" } else { "Flip" })).on_tapped(|_me, app: &mut App, _cx| flip(app)),
            )),
        )),),
    )
}

// ---------------------------------------------------------------- 4. globe with pins

fn place_pins(app: &App, cx: &mut Cx, model: Option<&SkiaModel3D>) {
    let points: Vec<[f32; 3]> = CITIES.iter().map(|c| on_globe(c.1, c.2)).collect();
    let on_screen = match model {
        Some(m) => positions(Some(m), &points),
        None => positions(cx.find(app.globe), &points),
    };
    place(cx, on_screen, &app.pins, &app.pin_names, app.open_pin, 18.0);
}

fn globe_with_pins(app: &mut App) -> Build<SkiaShape> {
    let earth = app.earth.clone().expect("decoded at build");
    let stage = SkiaShape::new().corner_radius(14).background_color(Color::from_rgb(8, 10, 22)).width_request(VIEW.0).height_request(VIEW.1).is_clipped_to_bounds(true).children((
        SkiaModel3D::from_model(&globe())
            .view_angles(-20.0, 18.0)
            .auto_rotate(6.0)
            .pinch_zoom(true)
            .min_zoom(0.6)
            .max_zoom(1.6)
            .reset_on_double_tap(true)
            .antialias(true)
            .material_image("earth", earth)
            .fill()
            .assign(&mut app.globe)
            .on_view_changed(|me, app: &mut App, cx| place_pins(app, cx, Some(me.control_mut()))),
        SkiaLayout::new().fill().children(
            (0..CITIES.len())
                .flat_map(|i| {
                    [
                        marker(18.0, DANGER, CITIES[i].0).assign(&mut app.pins[i]).on_tapped(move |_me, app: &mut App, cx| {
                            app.open_pin = if app.open_pin == Some(i) { None } else { Some(i) };
                            place_pins(app, cx, None);
                        }),
                        note(CITIES[i].0, &format!("{:.1}°, {:.1}°", CITIES[i].1, CITIES[i].2)).assign(&mut app.pin_names[i]),
                    ]
                })
                .collect::<Vec<_>>(),
        ),
    ));
    panel(
        "Globe with pins",
        "The Earth turning by itself. Pins stay on their cities and hide behind the globe; tap one for its name.",
        (SkiaLayout::row().spacing(24).fill_x().children((
            stage,
            SkiaLayout::column().spacing(12).fill_x().children((
                SkiaLabel::new("Six cities").font_size(24).text_color(INK),
                SkiaLabel::new("Each pin is a normal DrawnUI control placed over the 3D view at its city's point, moved when the globe turns.").font_size(13).text_color(MUTED),
                SkiaLabel::new("Drag to turn · pinch or scroll to zoom · double-tap to reset").font_size(13).text_color(MUTED),
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
                Some(Field::Name) => mascot.set_node_rotation("Head", bone(HEAD_REST, 10.0, -50.0, 0.0)),
                Some(Field::Email) => mascot.set_node_rotation("Head", bone(HEAD_REST, 22.0, -50.0, 0.0)),
                _ => mascot.reset_node("Head"),
            };
        }
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
    let form = SkiaShape::new().corner_radius(14).background_color(PAGE).padding(24).width_request(420).children((SkiaLayout::column().spacing(12).fill_x().children((
        SkiaLabel::new("Create your account").font_size(22).text_color(INK),
        SkiaLabel::new("A normal form; the mascot is one more control on the screen.").font_size(13).text_color(MUTED),
        form_field("Name", Field::Name).on_text_changed(|_me, app: &mut App, _cx, text| app.name = text.to_owned()),
        form_field("Email", Field::Email),
        form_field("Password", Field::Password).is_password(true),
        button("Sign up").fill_x().on_tapped(|_me, app: &mut App, cx| {
            app.signed_up = true;
            app.focused = None;
            pose_mascot(app, cx);
            if let Some(mut mascot) = cx.get_mut(app.mascot) {
                mascot.play_once("CharacterArmature|Wave");
            }
        }),
        SkiaLabel::new("").font_size(14).text_color(ACCENT).observe(|me, app: &App| {
            let who = if app.name.trim().is_empty() { "friend" } else { app.name.trim() };
            me.set_text(if app.signed_up { format!("Welcome aboard, {who}!") } else { String::new() });
        }),
    )),));
    panel(
        "3D next to normal UI",
        "A sign-up form with a mascot beside it: it watches the field you type in, covers its eyes for the password and waves when you sign up.",
        (SkiaLayout::row().spacing(24).fill_x().children((
            form,
            SkiaShape::new().corner_radius(14).background_color(STAGE).fill_x().height_request(VIEW.1).children((SkiaModel3D::new(AVATAR)
                .animation("CharacterArmature|Idle")
                .view_angles(-28.0, 8.0)
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

fn next_label(app: &mut App, cx: &mut Cx) {
    app.label = (app.label + 1) % app.labels.len();
    if let Some(mut product) = cx.get_mut(app.product) {
        product.set_material_image("box", Some(app.labels[app.label].clone()));
    }
}

// ---------------------------------------------------------------- the page

fn build(app: &mut App) -> Build<DrawnGame> {
    (app.mobs, app.dice, app.spin) = (40, Dice(7), 1.0);
    (app.labels, app.card_faces, app.earth) = (labels(), Some(card_faces()), Texture3D::decode(EARTH));
    app.fabrics = SWATCHES.iter().filter_map(|s| Texture3D::decode(s.1)).collect();
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
            SkiaShape::new().width_request(FIELD.0).height_request(FIELD.1).corner_radius(10).background_color(Color::from_rgb(58, 98, 74)).children((SkiaScene3D::new()
                .model("slime", MOB)
                .tilt(30.0)
                .fill()
                .assign(&mut app.field)
                .on_tapped_3d(|me, _app: &mut App, _cx, hit| {
                    me.play_once(&hit.object, "CharacterArmature|HitRecieve");
                }),)),
            SkiaLayout::row().spacing(8).margin((0.0, 10.0, 0.0, 0.0)).children(
                [10usize, 40, 100].into_iter().map(|n| chip(&format!("{n} mobs"), move |app| app.mobs == n).width_request(110).on_tapped(move |_me, app: &mut App, _cx| app.mobs = n)).collect::<Vec<_>>(),
            ),
        ),
    );
    DrawnGame::new().fill().start_loop(0.0).on_game_loop(|_me, app: &mut App, cx, delta| game_loop(app, cx, delta)).children((
        SkiaScroll::new().fill().content(
            SkiaLayout::column().fill_x().padding(24).spacing(20).children((
                SkiaLabel::new("3D, made in the app").font_size(30).text_color(INK).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("Where 3D belongs in a real app: products, lists, cards, maps and forms first, then the fun, then games.")
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
                    section("Micro-interactions", "Small 3D moments that answer a tap."),
                    micro,
                    section("Images on 3D", "Any image on a 3D surface, switched live: drawn in code here, or a photo or a file."),
                    images,
                    section("From model files", "Free 3D models with their own animations, driven by the app."),
                    files,
                    section("Games", "A 2D game field with 3D monsters on it."),
                    mobs,
                )),
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
