//! The gallery's look, as the .NET gallery's Theme.cs: the palette, cards raised from the page and
//! panels sunk into them, the raised 3D button with an engraved caption, the 3D switch, text that
//! can be selected and copied with its small menu, and links.

use drawnui::controls::layout::LayoutProps;
use drawnui::prelude::*;

use crate::App;

pub const PAGE: Color = Color::from_rgb(14, 16, 24);
pub const HEADER: Color = Color::from_rgb(19, 21, 32);
pub const CARD: Color = Color::from_rgb(24, 27, 40);
pub const LINE: Color = Color::from_rgb(40, 44, 62);
/// The panel a 3D piece stands on inside a card.
pub const STAGE: Color = Color::from_rgb(42, 47, 70);
pub const INK: Color = Color::from_rgb(242, 241, 248);
pub const MUTED: Color = Color::from_rgb(154, 152, 176);
/// The gallery's gold: lit buttons, the switches on, the selection.
pub const ACCENT: Color = Color::from_rgb(242, 179, 61);
/// A button at rest.
pub const QUIET: Color = Color::from_rgb(70, 76, 106);
/// Prices.
pub const GOLD: Color = Color::from_rgb(255, 196, 61);
pub const BAD: Color = Color::from_rgb(255, 99, 132);
pub const LINK_COLOR: Color = Color::from_rgb(255, 208, 110);
/// Titles, headings and button captions: OpenSans Semibold.
pub const TITLE: &str = "FontTextTitle";

/// A color moved toward white by `amount` (0..1): the hover lift.
pub fn lighter(c: Color, amount: f32) -> Color {
    let up = |v: u8| (v as f32 + (255.0 - v as f32) * amount).round() as u8;
    Color::from_argb(c.a(), up(c.r()), up(c.g()), up(c.b()))
}

/// A color moved toward black by `amount` (0..1).
pub fn darker(c: Color, amount: f32) -> Color {
    let down = |v: u8| (v as f32 * (1.0 - amount)).round() as u8;
    Color::from_argb(c.a(), down(c.r()), down(c.g()), down(c.b()))
}

/// The edge light of cards (raised) and of the panels sunk into them.
pub fn card_bevel() -> SkiaBevel {
    SkiaBevel::new(1.5).opacity(0.22)
}

/// A top-to-bottom gradient through `colors` at `positions`.
fn vertical(colors: [Color; 3], positions: [f32; 3]) -> SkiaGradient {
    SkiaGradient::new(GradientType::Linear, colors.to_vec()).color_positions(positions.to_vec())
}

/// A card raised from the page: lit top edge, shaded bottom edge.
pub fn raised(shape: Build<SkiaShape>) -> Build<SkiaShape> {
    shape.background_color(CARD).bevel_type(BevelType::Bevel).bevel(card_bevel())
}

/// A panel sunk into its card.
pub fn sunk(shape: Build<SkiaShape>) -> Build<SkiaShape> {
    shape.bevel_type(BevelType::Emboss).bevel(card_bevel())
}

// ---------------------------------------------------------------- Button3D

/// How far a button's face stands above the page, points: the side seen under it.
const DEPTH: f32 = 4.0;
const FACE_HEIGHT: f32 = 40.0;

/// The app's button, raised in 3D: a face lit from above (a light gradient and a bevel on its
/// edges) standing on its darker side, with a soft shadow under it. Pressed, the face sinks onto
/// its side; under the mouse it gets lighter. A button for the keyboard and screen readers;
/// `set_button` changes its caption and its state color (selected, active).
pub struct Button3D {
    layout: SkiaLayout,
    look: ButtonLook,
}

/// What a Button3D shows: its parts and their state.
#[derive(Clone)]
struct ButtonLook {
    face: Handle<SkiaShape>,
    /// The caption's engraved shadow above, its light edge below, the caption.
    captions: [Handle<SkiaLabel>; 3],
    fill: Color,
    text: String,
    hovered: bool,
    pressed: bool,
}

impl Has<LayoutProps> for Button3D {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for Button3D {}

/// A light face (the gold accent) takes dark ink, a dark face light ink.
fn light_face(c: Color) -> bool {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0 > 0.55
}

impl ButtonLook {
    /// The face's color, gradient, side and shadow, and how far it sank.
    fn face(&self) -> (Color, SkiaGradient, Vec<SkiaShadow>, f32) {
        let face = if self.hovered { lighter(self.fill, 0.14) } else { self.fill };
        let side = if self.pressed { 0.0 } else { DEPTH };
        // The foot: the face's own color, darker, right under it; then a soft shadow.
        let shadows = vec![SkiaShadow::new(darker(self.fill, 0.45)).y(side).blur(0).opacity(1), SkiaShadow::new(Color::BLACK).y(DEPTH + 3.0).blur(8).opacity(0.35)];
        (face, vertical([lighter(face, 0.2), face, darker(face, 0.1)], [0.0, 0.55, 1.0]), shadows, DEPTH - side)
    }

    /// Engraved: a dark face takes its light edge below, a light face its shadow above.
    fn caption_colors(&self) -> [Color; 3] {
        let light = light_face(self.fill);
        let ink = if light { Color::from_rgb(40, 28, 6) } else { INK.with_a(235) };
        [Color::BLACK.with_a(if light { 46 } else { 140 }), Color::WHITE.with_a(if light { 115 } else { 41 }), ink]
    }

    fn apply(&self, cx: &mut Cx) {
        let (color, gradient, shadows, sunk) = self.face();
        if let Some(mut f) = cx.get_mut(self.face) {
            f.set_background_color(color);
            f.set_fill_gradient(gradient);
            f.set_shadows(shadows);
            f.set_translation_y(sunk);
        }
        for (handle, color) in self.captions.iter().zip(self.caption_colors()) {
            if let Some(mut l) = cx.get_mut(*handle) {
                l.set_text_color(color);
                if l.p.text != self.text {
                    l.set_text(self.text.clone());
                }
            }
        }
    }
}

fn caption(text: &str, color: Color, offset: f32, handle: &mut Handle<SkiaLabel>) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_family(TITLE)
        .font_size(14)
        .text_color(color)
        .translation_y(offset)
        .center()
        // The button is the accessible node, read once: its captions are not.
        .accessibility_role(Aria::PRESENTATION)
        .assign(handle)
}

impl Button3D {
    /// `width` in points; `None` = as wide as its caption plus padding (a chip).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: &str, fill: Color, width: Option<f32>) -> Build<Button3D> {
        Button3D::outlined(text, fill, width, Color::TRANSPARENT, 0.0)
    }

    /// A Button3D whose face has an outline `stroke` points wide in `outline` (the Boss page's
    /// skin buttons, each in its skin's color).
    #[allow(clippy::new_ret_no_self)]
    pub fn outlined(text: &str, fill: Color, width: Option<f32>, outline: Color, stroke: f32) -> Build<Button3D> {
        let mut look = ButtonLook { face: Handle::default(), captions: [Handle::default(); 3], fill, text: text.to_owned(), hovered: false, pressed: false };
        let (color, gradient, shadows, _) = look.face();
        let [shade, edge, main] = look.caption_colors();
        let [h0, h1, h2] = &mut look.captions;
        let face = SkiaShape::new()
            .corner_radius(10)
            .height_request(FACE_HEIGHT)
            .horizontal_options(if width.is_some() { LayoutOptions::Fill } else { LayoutOptions::Start })
            .vertical_options(LayoutOptions::Start)
            .padding(if width.is_some() { Thickness::uniform(0.0) } else { Thickness::new(18.0, 0.0, 18.0, 0.0) })
            .bevel_type(BevelType::Bevel)
            .bevel(SkiaBevel::new(1.2).opacity(0.5))
            .stroke_color(outline)
            .stroke_width(stroke)
            .background_color(color)
            .fill_gradient(gradient)
            .shadows(shadows)
            .children((caption(text, shade, -1.0, h0), caption(text, edge, 1.0, h1), caption(text, main, 0.0, h2)))
            .assign(&mut look.face);
        let mut build = Build::new(Button3D { layout: SkiaLayout::default(), look })
            .height_request(FACE_HEIGHT + DEPTH)
            // Still between its state changes: one bitmap blit inside cards that redraw every frame
            // around a moving 3D view.
            .use_cache(CacheType::Image)
            .accessibility_role(Aria::BUTTON)
            .accessibility_label(text)
            .children((face,));
        if let Some(width) = width {
            build = build.width_request(width);
        }
        build
    }
}

impl Control for Button3D {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.look.apply(cx);
        self.layout.on_props_changed(cx);
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let look = &mut self.look;
        let (hovered, pressed) = match gesture.kind {
            GestureKind::PointerEnter => (true, look.pressed),
            GestureKind::PointerExit => (false, false),
            GestureKind::Down => (look.hovered || !gesture.touch, true),
            GestureKind::Up => (look.hovered && !gesture.touch, false),
            _ => (look.hovered, look.pressed),
        };
        if (hovered, pressed) != (look.hovered, look.pressed) {
            (look.hovered, look.pressed) = (hovered, pressed);
            look.apply(&mut cx.cx());
        }
        self.layout.on_gesture(cx, gesture)
    }
}

/// A Button3D's caption (its accessible name too) and its state color, changed from app code.
pub trait Button3DSet {
    fn set_text(&mut self, text: &str);
    fn set_fill(&mut self, fill: Color);
}

impl Button3DSet for Mut<'_, Button3D> {
    fn set_text(&mut self, text: &str) {
        if self.look.text != text {
            self.control_mut().look.text = text.to_owned();
            self.set_accessibility_label(text.to_owned());
            self.mark(Dirty::APPLY);
        }
    }

    fn set_fill(&mut self, fill: Color) {
        if self.look.fill != fill {
            self.control_mut().look.fill = fill;
            self.mark(Dirty::APPLY);
        }
    }
}

// ---------------------------------------------------------------- Switch3D

const KNOB: f32 = 28.0;
const SWITCH: (f32, f32) = (54.0, KNOB + 4.0);

/// The app's switch, in 3D like `Button3D`, the classic Android way: a thin track sunk into the
/// card and a bigger raised knob, lit from above, riding on it. On, the knob turns gold and the
/// track takes a deep gold; under the mouse both light up. A switch for the keyboard and screen
/// readers; the app turns it with `set_switch` from its tap.
pub struct Switch3D {
    layout: SkiaLayout,
    look: SwitchLook,
}

#[derive(Clone, Copy)]
struct SwitchLook {
    track: Handle<SkiaShape>,
    knob: Handle<SkiaShape>,
    on: bool,
    hovered: bool,
    /// Where the knob stands or slides to: on (the right end) or off.
    placed: bool,
}

impl SwitchLook {
    /// The knob's gradient and color, and the track's color.
    fn colors(&self) -> (SkiaGradient, Color, Color) {
        let lift = |c: Color| if self.hovered { lighter(c, 0.14) } else { c };
        let knob = lift(if self.on { ACCENT } else { Color::from_rgb(200, 202, 214) });
        let track = lift(if self.on { darker(ACCENT, 0.45) } else { darker(QUIET, 0.35) });
        (vertical([lighter(knob, 0.35), knob, darker(knob, 0.15)], [0.0, 0.5, 1.0]), knob, track)
    }

    fn apply(&self, cx: &mut Cx) {
        let (gradient, knob, track) = self.colors();
        if let Some(mut k) = cx.get_mut(self.knob) {
            k.set_fill_gradient(gradient);
            k.set_background_color(knob);
        }
        if let Some(mut t) = cx.get_mut(self.track) {
            t.set_background_color(track);
        }
    }
}

impl Has<LayoutProps> for Switch3D {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for Switch3D {}

impl Switch3D {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(on: bool, name: &str) -> Build<Switch3D> {
        let mut look = SwitchLook { track: Handle::default(), knob: Handle::default(), on, hovered: false, placed: on };
        let (gradient, knob_color, track_color) = look.colors();
        let track = sunk(SkiaShape::new().corner_radius(7).height_request(14).margin((6.0, 0.0)).fill_x().vertical_options(LayoutOptions::Center))
            .bevel(SkiaBevel::new(1.5).opacity(0.45))
            .background_color(track_color)
            .assign(&mut look.track);
        let rim = SkiaGradient::new(GradientType::Linear, vec![Color::WHITE.with_a(217), Color::WHITE.with_a(38), Color::BLACK.with_a(89)]).color_positions(vec![0.0, 0.5, 1.0]);
        let knob = SkiaShape::new()
            .shape_type(ShapeType::Circle)
            .width_request(KNOB)
            .height_request(KNOB)
            .vertical_options(LayoutOptions::Center)
            // Its rim catches the light on top and falls into shade below.
            .stroke_width(1.5)
            .stroke_color(Color::WHITE)
            .stroke_gradient(rim)
            .shadows(vec![SkiaShadow::new(Color::BLACK).y(3).blur(5).opacity(0.55)])
            .background_color(knob_color)
            .fill_gradient(gradient)
            .translation_x(if on { SWITCH.0 - KNOB } else { 0.0 })
            .assign(&mut look.knob);
        Build::new(Switch3D { layout: SkiaLayout::default(), look })
            .width_request(SWITCH.0)
            .height_request(SWITCH.1)
            .use_cache(CacheType::Image)
            .accessibility_role(Aria::SWITCH)
            .accessibility_label(name)
            .accessibility_is_pressed(Some(on))
            .children((track, knob))
    }
}

impl Control for Switch3D {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.look.apply(cx);
        if self.look.placed != self.look.on {
            // The knob slides over 200 ms.
            self.look.placed = self.look.on;
            cx.translate_to(self.look.knob, if self.look.on { SWITCH.0 - KNOB } else { 0.0 }, 0.0, 200.0, easing::cubic_out);
        }
        self.layout.on_props_changed(cx);
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let hovered = match gesture.kind {
            GestureKind::PointerEnter => true,
            GestureKind::PointerExit => false,
            _ => self.look.hovered,
        };
        if hovered != self.look.hovered {
            self.look.hovered = hovered;
            self.look.apply(&mut cx.cx());
        }
        self.layout.on_gesture(cx, gesture)
    }
}

/// A Switch3D turned on or off from app code: the knob slides over 200 ms, the colors follow.
pub trait Switch3DSet {
    fn set_on(&mut self, on: bool);
}

impl Switch3DSet for Mut<'_, Switch3D> {
    fn set_on(&mut self, on: bool) {
        if self.look.on != on {
            self.control_mut().look.on = on;
            self.set_accessibility_is_pressed(Some(on));
            self.mark(Dirty::APPLY);
        }
    }
}



// ---------------------------------------------------------------- selectable text, links

/// The gallery's text that can be selected and copied (titles, descriptions, prices): a mouse drag
/// or a double click selects, Ctrl+C copies; a right click or a touch long press on selected text
/// opens a small drawn menu, Copy or Cancel.
pub fn copyable(text: &str) -> Build<SkiaLabel> {
    text_menu(SkiaLabel::new(text).accessibility_text_selectable(true), None)
}

/// A text link: the link color, brighter under the mouse, a link for the keyboard and screen
/// readers; the main button or a tap opens it, a right click or a long press opens its menu (Copy
/// Link, and Copy over a selection).
pub fn link(text: &str, url: &'static str) -> Build<SkiaLabel> {
    let label = SkiaLabel::new(text)
        .font_size(14)
        .font_family(TITLE)
        .text_color(LINK_COLOR)
        .accessibility_text_selectable(true)
        .accessibility_role(Aria::LINK)
        .accessibility_label(text)
        .accessibility_hint(url)
        .on_hovered(|me, _app: &mut App, _cx, on| me.set_text_color(if on { INK } else { LINK_COLOR }))
        .on_tapped(move |_me, _app: &mut App, cx| {
            // A right click is for its menu.
            if cx.gesture().is_none_or(|g| g.is_primary()) {
                cx.open_url(url);
            }
        });
    text_menu(label, Some(url))
}

/// The small menu of selectable text: Copy (the whole page's selection), Copy Link on a link,
/// Cancel. Nothing selected and no link: the click is not the label's.
fn text_menu(label: Build<SkiaLabel>, url: Option<&'static str>) -> Build<SkiaLabel> {
    label.on_context_menu(move |me, app: &mut App, cx, menu| {
        let selected = !me.selected_text().is_empty() || !cx.selected_text().is_empty();
        if !selected && url.is_none() {
            return false;
        }
        let id = me.id();
        let mut items = Vec::new();
        if selected {
            items.push(menu_item("Copy", move |cx| {
                let page = cx.selected_text();
                let text = if page.is_empty() { cx.find::<SkiaLabel>(id).map(|l| l.selected_text()).unwrap_or_default() } else { page };
                cx.set_clipboard(text);
            }));
        }
        if let Some(url) = url {
            items.push(menu_item("Copy Link", move |cx| cx.set_clipboard(url)));
        }
        items.push(menu_item("Cancel", |cx| cx.clear_text_selection()));
        if let Some(open) = app.text_menu.take() {
            cx.close_popup_window(open);
        }
        let panel = SkiaShape::new()
            .corner_radius(10)
            .background_color(CARD)
            .stroke_color(LINE)
            .stroke_width(1)
            .width_request(150)
            .padding(4)
            .shadows(vec![SkiaShadow::new(Color::BLACK).y(4).blur(12).opacity(0.5)])
            .children((SkiaLayout::column().spacing(0).fill_x().accessibility_role(Aria::MENU).accessibility_label("Text").children(items),));
        app.text_menu = Some(cx.open_popup_window(panel, menu.location, PopupWindowOptions { cpu: true, ..Default::default() }));
        true
    })
}

fn menu_item(text: &str, act: impl Fn(&mut Cx) + 'static) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(7)
        .fill_x()
        .padding((12.0, 8.0))
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_can_interact(true)
        .accessibility_label(text)
        .children((SkiaLabel::new(text).font_size(14).text_color(INK).accessibility_role(Aria::PRESENTATION),))
        .on_hovered(|me, _app: &mut App, _cx, on| me.set_background_color(if on { QUIET } else { Color::TRANSPARENT }))
        .on_tapped(move |_me, app: &mut App, cx| {
            act(cx);
            if let Some(open) = app.text_menu.take() {
                cx.close_popup_window(open);
            }
        })
}

/// A section title with its caption under it.
pub fn section(title: &str, caption: &str) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(4).horizontal_options(LayoutOptions::Center).margin((0.0, 12.0, 0.0, 0.0)).children((
        copyable(title).font_size(22).font_family(TITLE).text_color(INK).horizontal_options(LayoutOptions::Center).accessibility_role(Aria::HEADING),
        copyable(caption).font_size(14).text_color(MUTED).horizontal_options(LayoutOptions::Center),
    ))
}
