//! Common controls in every prebuilt look: one card per ControlStyle, same controls, only the
//! style changes (the Fiddle "Looks" preset). Ported from the React demo's LooksPage.tsx.

use drawnui::prelude::*;

use super::scrolling;
use crate::{App, hex};

const STYLES: [PrebuiltControlStyle; 5] = [
    PrebuiltControlStyle::Unset,
    PrebuiltControlStyle::Windows,
    PrebuiltControlStyle::Cupertino,
    PrebuiltControlStyle::Material,
    PrebuiltControlStyle::Material3,
];

/// What the page keeps.
pub struct State {
    /// What the control used last reported.
    pub(super) last: String,
    /// The style of the live card, an index into `STYLES`.
    pub(super) live: usize,
}

impl Default for State {
    fn default() -> Self {
        Self { last: "interact with any control".to_owned(), live: 0 }
    }
}

/// Builds the page.
pub fn build(_app: &mut App) -> Build<SkiaLayout> {
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        SkiaLabel::new("Common Controls").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new("SkiaSwitch, SkiaCheckbox, SkiaRadioButton, SkiaButton, SkiaProgress, SkiaSlider — the same tree per card, only ControlStyle changes (the Fiddle 'Looks' snippet).")
            .font_size(13)
            .text_color(hex(0xADB5BD))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center),
        SkiaLabel::new("")
            .font_size(13)
            .text_color(hex(0x6EA8FE))
            .horizontal_options(LayoutOptions::Center)
            .observe(|me, app: &App| me.set_text(format!("Last: {}", app.looks.last))),
        // A live card: ControlStyle changes on an already built tree rebuild the default content (C# RebuildDefaultContent).
        SkiaButton::new("")
            .horizontal_options(LayoutOptions::Center)
            .observe(|me, app: &App| me.set_text(format!("Live card: {} — tap to switch style", name(STYLES[app.looks.live]))))
            .on_tapped(|_me, app: &mut App, _cx| app.looks.live = (app.looks.live + 1) % STYLES.len()),
        card("Live", STYLES[0], true),
        card("Default", PrebuiltControlStyle::Unset, false),
        card("Windows — Fluent", PrebuiltControlStyle::Windows, false),
        card("Cupertino — iOS", PrebuiltControlStyle::Cupertino, false),
        card("Material — Android", PrebuiltControlStyle::Material, false),
        card("Material3 — Android", PrebuiltControlStyle::Material3, false),
    )))
}

/// The React name of a style.
fn name(style: PrebuiltControlStyle) -> &'static str {
    match style {
        PrebuiltControlStyle::Unset => "Unset",
        PrebuiltControlStyle::Windows => "Windows",
        PrebuiltControlStyle::Cupertino => "Cupertino",
        PrebuiltControlStyle::Material => "Material",
        PrebuiltControlStyle::Material3 => "Material3",
        PrebuiltControlStyle::Platform => "Platform",
    }
}

/// One card of controls in a style. The live card follows the page's pick; its title names it.
fn card(title: &'static str, style: PrebuiltControlStyle, live: bool) -> Build<SkiaShape> {
    // The group of the radio buttons is the card: the live one keeps its own name.
    let group = if live { "Live" } else { title };
    // Every control of the live card takes the picked style.
    fn follow<T: Control>(build: Build<T>, live: bool, set: fn(&mut Mut<'_, T>, PrebuiltControlStyle)) -> Build<T> {
        if live { build.observe(move |me, app: &App| set(me, STYLES[app.looks.live])) } else { build }
    }
    // Written into "Last" as the React page's log.
    let log = move |app: &mut App, what: String| app.looks.last = format!("{title} {what}");
    SkiaShape::new()
        .corner_radius(16)
        .background_color(hex(0xF5F5F5))
        .padding((18, 14))
        .fill_x()
        .use_cache(CacheType::Image)
        .children(SkiaStack::new().spacing(14).fill_x().children((
            follow(
                SkiaLabel::new(title).font_size(16).font_attributes(FontAttributes::Bold).text_color(hex(0x111827)),
                live,
                |me, style| me.set_text(format!("Live — {}", name(style))),
            ),
            SkiaRow::new().spacing(16).fill_x().children((
                follow(
                    SkiaSwitch::new()
                        .control_style(style)
                        .accessibility_label("Wi-Fi")
                        .is_toggled(true)
                        .vertical_options(LayoutOptions::Center)
                        .on_toggled(move |_me, app: &mut App, _cx, on| log(app, format!("switch: {on}"))),
                    live,
                    |me, style| me.set_control_style(style),
                ),
                follow(
                    SkiaCheckbox::new()
                        .control_style(style)
                        .accessibility_label("Remember me")
                        .is_toggled(true)
                        .vertical_options(LayoutOptions::Center)
                        .on_toggled(move |_me, app: &mut App, _cx, on| log(app, format!("checkbox: {on}"))),
                    live,
                    |me, style| me.set_control_style(style),
                ),
                radio("One", group, style, true, live, log),
                radio("Two", group, style, false, live, log),
            )),
            follow(
                SkiaButton::new("Button")
                    .control_style(style)
                    .horizontal_options(LayoutOptions::Start)
                    .on_tapped(move |_me, app: &mut App, _cx| log(app, "button tapped".to_owned())),
                live,
                |me, style| me.set_control_style(style),
            ),
            follow(SkiaProgress::new().control_style(style).accessibility_label("Download").value(65).fill_x(), live, |me, style| me.set_control_style(style)),
            follow(
                SkiaSlider::new()
                    .control_style(style)
                    .accessibility_label("Volume")
                    .end(65)
                    .fill_x()
                    .on_end_changed(move |_me, app: &mut App, _cx, value| log(app, format!("slider: {value:.0}"))),
                live,
                |me, style| me.set_control_style(style),
            ),
            // Range mode: two thumbs.
            follow(
                SkiaSlider::new()
                    .control_style(style)
                    .accessibility_label("Price range")
                    .enable_range(true)
                    .start(20)
                    .end(80)
                    .fill_x()
                    .on_start_changed(move |_me, app: &mut App, _cx, value| log(app, format!("range start: {value:.0}")))
                    .on_end_changed(move |_me, app: &mut App, _cx, value| log(app, format!("range end: {value:.0}"))),
                live,
                |me, style| me.set_control_style(style),
            ),
        )))
}

fn radio(
    text: &'static str,
    group: &'static str,
    style: PrebuiltControlStyle,
    on: bool,
    live: bool,
    log: impl Fn(&mut App, String) + Copy + 'static,
) -> Build<SkiaRadioButton> {
    let radio = SkiaRadioButton::new(text)
        .control_style(style)
        .is_toggled(on)
        .group_name(group)
        .vertical_options(LayoutOptions::Center)
        .on_toggled(move |_me, app: &mut App, _cx, on| {
            if on {
                log(app, format!("radio: {text}"));
            }
        });
    if live { radio.observe(|me, app: &App| me.set_control_style(STYLES[app.looks.live])) } else { radio }
}
