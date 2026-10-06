//! Accessibility: an invisible ARIA overlay mirrors the drawn controls; Tab / Enter / Space and
//! screen readers reach them. Ported from the React demo's AccessibilityPage.tsx.

use drawnui::prelude::*;

use super::{page_title, scrolling};
use crate::{App, hex};

const BODY: Color = hex(0xDEE2E6);
const MUTED: Color = hex(0xADB5BD);

/// What the page keeps.
pub struct State {
    count: u32,
    pub(super) sound: bool,
    dark: bool,
    pub(super) last_activated: &'static str,
    /// Nodes in the overlay and the label of the one with the keyboard focus.
    pub(super) nodes: usize,
    focused: String,
    status: Handle<SkiaLabel>,
}

impl Default for State {
    fn default() -> Self {
        Self { count: 0, sound: true, dark: false, last_activated: "-", nodes: 0, focused: "none".to_owned(), status: Handle::default() }
    }
}

/// The page opened: it reads the engine's accessibility snapshot every 300 ms (focus is not part
/// of the snapshot, as in React).
pub fn opened(app: &mut App, cx: &mut Cx) {
    refresh(app, cx);
}

fn refresh(app: &mut App, cx: &mut Cx) {
    let nodes = cx.accessibility_nodes();
    let focused = cx.accessibility_focused().and_then(|id| nodes.iter().find(|node| node.control == id));
    let focused = focused.map_or("none".to_owned(), |node| node.label.clone());
    let page = &mut app.a11y;
    (page.nodes, page.focused) = (nodes.len(), focused);
    cx.after(page.status, 300, refresh);
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        page_title("Accessibility").accessibility_role(Aria::HEADING),
        SkiaLabel::new("Press Tab to move between the drawn controls, Enter or Space to activate. Screen readers see the same overlay: roles, labels, hints, pressed state, live regions.")
            .font_size(14)
            .text_color(hex(0xD3D3D3))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center),
        card(
            "Accessibility snapshot (Canvas.AccessibilityManager)",
            (
                SkiaLabel::new("")
                    .font_size(14)
                    .text_color(BODY)
                    .fill_x()
                    .accessibility_role(Aria::STATUS)
                    .accessibility_live(Aria::LIVE_POLITE)
                    .assign(&mut app.a11y.status)
                    .observe(|me, app: &App| {
                        let page = &app.a11y;
                        me.set_text(format!(
                            "Nodes in the overlay: {} · focused: {} · last activated: {}",
                            page.nodes, page.focused, page.last_activated
                        ))
                    }),
            ),
        ),
        card(
            "AccessibilityTextSelectable — real selectable text over the drawn label (opt-in)",
            (
                SkiaLabel::new("This paragraph is drawn on the canvas, but the accessibility overlay also carries it as real, invisible text in the same font and line positions. Select it with the mouse or touch, copy it with Ctrl+C, let a screen reader read it word by word — like any HTML paragraph. Off by default: pointer input over selectable text goes to the selection, not to the drawn control, so it is never turned on for buttons, carousels or anything gesture-driven.")
                    .font_size(14)
                    .text_color(BODY)
                    .fill_x()
                    .accessibility_text_selectable(true),
                SkiaLabel::new("This one is a normal label: exposed to screen readers, not selectable.").font_size(12).text_color(MUTED).fill_x(),
            ),
        ),
        card(
            "Buttons — label from Text, hint, custom label, disabled",
            (
                SkiaWrap::new().spacing(8).children((
                    SkiaButton::new("")
                        .background_color(hex(0x0D6EFD))
                        .accessibility_hint("Increments the counter")
                        .observe(|me, app: &App| me.set_text(format!("Tapped {}×", app.a11y.count)))
                        .on_tapped(|_me, app: &mut App, _cx| {
                            app.a11y.count += 1;
                            app.a11y.last_activated = "counter";
                        }),
                    SkiaButton::new("★")
                        .font_size(18)
                        .font_family_fallback("FontSymbols,FontSymbols2")
                        .background_color(hex(0x6610F2))
                        .width_request(48)
                        .accessibility_label("Favorite")
                        .accessibility_hint("Icon-only button: AccessibilityLabel replaces the glyph")
                        .on_tapped(|_me, app: &mut App, _cx| app.a11y.last_activated = "favorite"),
                    SkiaButton::new("Disabled")
                        .background_color(hex(0x495057))
                        .is_disabled(true)
                        .accessibility_hint("IsDisabled: no tab stop, not activatable"),
                )),
            ),
        ),
        card(
            "Toggles — AccessibilityIsPressed → aria-pressed",
            // One Tab stop, Left / Right move between the toggles.
            SkiaRow::new().spacing(8).accessibility_role(Aria::TOOLBAR).children((
                toggle("Sound", "Sound", "sound", |a| a.sound, |a| a.sound = !a.sound),
                toggle("Dark", "Dark mode", "dark", |a| a.dark, |a| a.dark = !a.dark),
            )),
        ),
        card(
            "Any control can be a node — SkiaShape as a button, image with a description",
            (
                SkiaWrap::new().spacing(12).children((
                    SkiaShape::new()
                        .corner_radius(12)
                        .background_color(hex(0x373B3E))
                        .stroke_color(hex(0x6EA8FE))
                        .stroke_width(1)
                        .animation_tapped(SkiaTouchAnimation::Ripple)
                        .accessibility_role(Aria::BUTTON)
                        .accessibility_label("Open settings")
                        .accessibility_hint("A SkiaShape with Tapped: role button, label and hint set explicitly")
                        .on_tapped(|_me, app: &mut App, _cx| app.a11y.last_activated = "settings card")
                        .children(SkiaStack::new().spacing(4).padding((16, 12)).children((
                            SkiaLabel::new("Settings")
                                .font_size(18)
                                .font_family("FontTextBold")
                                .text_color(Color::WHITE)
                                .accessibility_role(Aria::PRESENTATION),
                            SkiaLabel::new("inner labels are RolePresentation").font_size(12).text_color(MUTED).accessibility_role(Aria::PRESENTATION),
                        ))),
                    SkiaSvg::new("assets/images/drawnui.svg")
                        .width_request(72)
                        .lock_ratio(1)
                        .accessibility_role(Aria::IMG)
                        .accessibility_label("DrawnUI palette logo"),
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .background_color(hex(0xFFC107))
                        .width_request(48)
                        .lock_ratio(1)
                        .vertical_options(LayoutOptions::Center)
                        .accessibility_role(Aria::PRESENTATION),
                )),
                SkiaLabel::new("The yellow circle is decorative: AccessibilityRole=Aria.RolePresentation keeps it out of the tree.")
                    .font_size(12)
                    .text_color(MUTED)
                    .fill_x(),
            ),
        ),
        card(
            "Keyboard groups — one Tab stop, the arrow keys inside",
            (
                SkiaLabel::new("A container with a composite role (Aria.RoleList, RoleToolbar, RoleGrid...) is one Tab stop: the arrow keys move between its items, Home and End go to the first and the last, Enter or Space activates. Tab comes back to the item it left.")
                    .font_size(12)
                    .text_color(MUTED)
                    .fill_x(),
                SkiaLabel::new("Fruits — a list: Up and Down").font_size(13).text_color(BODY).fill_x(),
                SkiaStack::new()
                    .spacing(6)
                    .accessibility_role(Aria::LIST)
                    .accessibility_label("Fruits")
                    .children(["Apple", "Banana", "Cherry", "Date"].into_iter().map(|name| group_item(name, None)).collect::<Vec<_>>()),
                SkiaLabel::new("Numbers — a grid: all four arrows").font_size(13).text_color(BODY).fill_x(),
                SkiaWrap::new()
                    .spacing(6)
                    .accessibility_role(Aria::GRID)
                    .accessibility_label("Numbers")
                    .children(NUMBERS.into_iter().map(|number| group_item(number, Some(56.0))).collect::<Vec<_>>()),
            ),
        ),
        card(
            "Labels — read by default, opted out per control",
            (
                SkiaLabel::new("This label is announced: SkiaLabel.DefaultAccessibilityRole = Aria.RoleText was set once at startup.")
                    .font_size(14)
                    .text_color(BODY)
                    .fill_x(),
                SkiaLabel::new("This one is visible but hidden from assistive technology (RolePresentation).")
                    .font_size(14)
                    .text_color(MUTED)
                    .fill_x()
                    .accessibility_role(Aria::PRESENTATION),
                SkiaLabel::new("Heading level text").font_size(16).font_family("FontTextBold").text_color(Color::WHITE).accessibility_role(Aria::HEADING),
            ),
        ),
        card(
            "How it works",
            SkiaLabel::new("• <canvas> is aria-hidden; a DOM overlay mirrors accessible controls with role / aria-label / title / aria-pressed / aria-live / tabindex.\n• Snapshot rebuilt at most once per second from the arranged rects, so it follows scrolling.\n• Overlay has pointer-events:none — hover and gestures reach the canvas; keyboard and screen-reader activation are routed as a Tapped.\n• Same property names as DrawnUi (.NET): AccessibilityRole, AccessibilityLabel, AccessibilityHint, AccessibilityCanInteract, AccessibilityIsPressed, AccessibilityLive.")
                .font_size(13)
                .text_color(MUTED)
                .fill_x(),
        ),
    )))
}

/// A card that is a group node named by its title.
fn card(title: &str, content: impl IntoChildren) -> Build<SkiaShape> {
    super::card(super::card_title(title).font_family_fallback("FontSymbols,FontSymbols2").accessibility_role(Aria::HEADING), content)
        .accessibility_role(Aria::GROUP)
        .accessibility_label(title)
}

const NUMBERS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];

/// An item of a keyboard group: a SkiaShape button that reports itself in "last activated";
/// `width` None fills the row.
fn group_item(text: &'static str, width: Option<f32>) -> Build<SkiaShape> {
    let item = SkiaShape::new()
        .corner_radius(6)
        .background_color(hex(0x373B3E))
        .height_request(36)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .accessibility_can_interact(Some(true))
        .accessibility_label(text)
        .on_tapped(move |_me, app: &mut App, _cx| app.a11y.last_activated = text)
        .children(
            SkiaLabel::new(text)
                .font_size(14)
                .text_color(Color::WHITE)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION),
        );
    match width {
        Some(width) => item.width_request(width),
        None => item.fill_x(),
    }
}

/// An on / off button: aria-pressed follows it.
fn toggle(name: &'static str, label: &'static str, activated: &'static str, read: fn(&State) -> bool, flip: fn(&mut State)) -> Build<SkiaButton> {
    SkiaButton::new("")
        .accessibility_label(label)
        .observe(move |me, app: &App| {
            let on = read(&app.a11y);
            me.set_text(format!("{name}: {}", if on { "on" } else { "off" }));
            me.set_background_color(hex(if on { 0x20C997 } else { 0x495057 }));
            me.set_accessibility_is_pressed(Some(on));
        })
        .on_tapped(move |_me, app: &mut App, _cx| {
            flip(&mut app.a11y);
            app.a11y.last_activated = activated;
        })
}
