//! SkiaCarousel and SkiaDrawer, the snapping layouts: swipe or drag, snap by velocity, state set
//! by code. Ported from the React demo's SnappingPage.tsx and SlideCell.ts.

use drawnui::prelude::*;

use super::{card, card_title, page_title, set_text};
use crate::{App, hex};

/// The Sandbox MainPageCarousels palette: color and caption color per slide.
const SLIDES: [(u32, u32); 4] = [(0xE94560, 0xFFFFFF), (0x0F3460, 0xFFFFFF), (0x533483, 0xFFFFFF), (0xA8DF8E, 0x1A1A2E)];
const PEEK: [u32; 4] = [0x0D6EFD, 0x6610F2, 0xD63384, 0x20C997];
const LOOP_COLORS: [u32; 6] = [0x0D6EFD, 0x6610F2, 0xD63384, 0x20C997, 0xFD7E14, 0x0DCAF0];
/// The slides of the DynamicSize carousel: caption, color, height in points.
const DYNAMIC: [(&str, u32, i32); 3] = [("80 pt", 0x0D6EFD, 80), ("160 pt", 0x6610F2, 160), ("110 pt", 0x20C997, 110)];
const SPEEDS: [f32; 3] = [0.5, 1.0, 2.0];

/// What the page keeps.
pub struct State {
    carousel: Handle<SkiaCarousel>,
    loop_carousel: Handle<SkiaCarousel>,
    drawer: Handle<SkiaDrawer>,
    // The playground.
    pub(super) index: usize,
    looped: bool,
    bounces: bool,
    speed: f32,
    in_transition: bool,
    preload: bool,
    sides: i32,
    vertical: bool,
    appeared: String,
    // The other cards.
    peek_index: usize,
    pub(super) loop_index: usize,
    dynamic_index: usize,
    pub(super) open: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            carousel: Handle::default(),
            loop_carousel: Handle::default(),
            drawer: Handle::default(),
            index: 0,
            looped: false,
            bounces: false,
            speed: 1.0,
            in_transition: false,
            preload: true,
            sides: 40,
            vertical: false,
            appeared: String::new(),
            peek_index: 1,
            loop_index: 0,
            dynamic_index: 0,
            open: false,
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    let page = &mut app.snapping;
    SkiaLayer::new().fill().children((
        // The slides move all the time: nothing above them is cached.
        SkiaScroll::new().fill().content(
            SkiaStack::new()
                .spacing(16)
                .padding((16, 16, 16, 260))
                .horizontal_options(LayoutOptions::Center)
                .maximum_width_request(720)
                .children((
                    page_title("Carousel & Drawer"),
                    playground(page),
                    card(
                        card_title("").observe(|me, app: &App| {
                            me.set_text(format!("SidesOffset={{40}} Spacing={{12}} — neighbors peek in · SelectedIndex={}", app.snapping.peek_index))
                        }),
                        SkiaCarousel::new()
                            .height_request(140)
                            .sides_offset(40)
                            .spacing(12)
                            .selected_index(1)
                            .on_selected_index_changed(|_me, app: &mut App, _cx, index| app.snapping.peek_index = index)
                            .children(
                                PEEK.iter()
                                    .enumerate()
                                    .map(|(i, color)| {
                                        SkiaShape::new().corner_radius(12).background_color(hex(*color)).children(
                                            SkiaLabel::new(format!("Slide {}", i + 1)).font_size(20).text_color(Color::WHITE).center(),
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                    ),
                    card(
                        card_title("").observe(|me, app: &App| {
                            me.set_text(format!(
                                "IsLooped + ItemsSource/ItemTemplate (12 recycled cells) · SelectedIndex={}",
                                app.snapping.loop_index
                            ))
                        }),
                        (
                            SkiaCarousel::new()
                                .height_request(130)
                                .is_looped(true)
                                .sides_offset(30)
                                .spacing(10)
                                .linear_speed_ms(350)
                                .assign(&mut page.loop_carousel)
                                .items(|_app: &App| 12, slide_cell, bind_loop_slide)
                                .on_selected_index_changed(|_me, app: &mut App, _cx, index| app.snapping.loop_index = index),
                            SkiaWrap::new().spacing(8).children((
                                SkiaButton::new("Prev").background_color(hex(0x0D6EFD)).on_tapped(|_me, app: &mut App, cx| {
                                    if let Some(mut carousel) = cx.get_mut(app.snapping.loop_carousel) {
                                        carousel.go_prev();
                                    }
                                }),
                                SkiaButton::new("Next").background_color(hex(0x0D6EFD)).on_tapped(|_me, app: &mut App, cx| {
                                    if let Some(mut carousel) = cx.get_mut(app.snapping.loop_carousel) {
                                        carousel.go_next();
                                    }
                                }),
                                SkiaLabel::new("Wraps last → first both ways (virtual anchors); LinearSpeedMs=350 = one slide per 350 ms without Bounces; cells are recycled through ItemTemplate.")
                                    .font_family_fallback("FontSymbols,FontSymbols2")
                                    .font_size(12)
                                    .text_color(hex(0xADB5BD))
                                    .vertical_options(LayoutOptions::Center)
                                    .fill_x(),
                            )),
                        ),
                    ),
                    card(
                        card_title("").observe(|me, app: &App| {
                            me.set_text(format!(
                                "DynamicSize — auto height follows the selected slide · SelectedIndex={}",
                                app.snapping.dynamic_index
                            ))
                        }),
                        (
                            SkiaCarousel::new()
                                .dynamic_size(true)
                                .vertical_options(LayoutOptions::Start)
                                .bounces(true)
                                .items(|_app: &App| DYNAMIC.len(), slide_cell, bind_dynamic_slide)
                                .on_selected_index_changed(|_me, app: &mut App, _cx, index| app.snapping.dynamic_index = index),
                            note("No HeightRequest: the carousel measures the selected cell (80 / 160 / 110 pt) and re-measures on every index change."),
                        ),
                    ),
                    card(
                        card_title("SkiaDrawer — drag the header below, or:"),
                        (
                            SkiaRow::new().spacing(8).children((
                                SkiaButton::new("")
                                    .background_color(hex(0x6610F2))
                                    .observe(|me, app: &App| me.set_text(if app.snapping.open { "Close drawer" } else { "Open drawer" }))
                                    .on_tapped(|_me, app: &mut App, cx| {
                                        if let Some(mut drawer) = cx.get_mut(app.snapping.drawer) {
                                            let open = drawer.control_mut().is_open();
                                            drawer.set_is_open(!open);
                                        }
                                    }),
                                SkiaLabel::new("")
                                    .font_size(14)
                                    .text_color(hex(0xDEE2E6))
                                    .vertical_options(LayoutOptions::Center)
                                    .observe(|me, app: &App| me.set_text(format!("IsOpen: {}", app.snapping.open))),
                            )),
                            note("Direction=FromBottom HeaderSize=56, sits in a SkiaLayer with VerticalOptions=End; snaps by velocity, Bounces enabled."),
                        ),
                    ),
                )),
        ),
        // The drawer lives in its own full-size layer over the page, anchored to the bottom edge.
        SkiaLayer::new().fill().children(drawer(page)),
    ))
}

/// The Sandbox MainPageCarousels playground: toggles, programmatic moves, indicators, events.
fn playground(page: &mut State) -> Build<SkiaShape> {
    card(
        card_title("SkiaCarousel playground (Sandbox MainPageCarousels)").font_family_fallback("FontSymbols,FontSymbols2"),
        (
            SkiaWrap::new().spacing(8).children((
                toggle("IsLooped", |p| p.looped, |p| p.looped = !p.looped),
                toggle("Bounces", |p| p.bounces, |p| p.bounces = !p.bounces),
                toggle("PreloadNeighboors", |p| p.preload, |p| p.preload = !p.preload),
                toggle("IsVertical", |p| p.vertical, |p| p.vertical = !p.vertical),
                SkiaButton::new("")
                    .background_color(hex(0x495057))
                    .font_size(13)
                    .observe(|me, app: &App| me.set_text(format!("SidesOffset: {}", app.snapping.sides)))
                    .on_tapped(|_me, app: &mut App, _cx| {
                        let sides = &mut app.snapping.sides;
                        *sides = match *sides {
                            40 => 0,
                            0 => 20,
                            _ => 40,
                        };
                    }),
            )),
            SkiaWrap::new().spacing(8).children((
                move_button("← Prev", |carousel, _| carousel.go_prev()),
                move_button("Next →", |carousel, _| carousel.go_next()),
                move_button("ScrollTo(2)", |carousel, _| carousel.scroll_to(2, true)),
                move_button("ScrollTo(0, no anim)", |carousel, _| carousel.scroll_to(0, false)),
                move_button("Set index 3", |_, page| page.index = 3),
            )),
            SkiaCarousel::new()
                .height_request(250)
                .background_color(hex(0x16213E))
                .spacing(20)
                .assign(&mut page.carousel)
                .observe({
                    // Like a React prop, the index is set when the app's value changes, not every time:
                    // a swipe or GoNext moves the carousel away from it.
                    let mut applied = None;
                    move |me, app: &App| {
                        let page = &app.snapping;
                        if applied != Some(page.index) {
                            applied = Some(page.index);
                            me.set_selected_index(page.index);
                        }
                    }
                })
                .observe(|me, app: &App| {
                    let page = &app.snapping;
                    me.set_is_looped(page.looped);
                    me.set_bounces(page.bounces);
                    me.set_swipe_speed(page.speed);
                    me.set_sides_offset(page.sides);
                    me.set_preload_neighboors(page.preload);
                    me.set_is_vertical(page.vertical);
                })
                .on_selected_index_changed(|_me, app: &mut App, _cx, index| app.snapping.index = index)
                .on_transition_changed(|_me, app: &mut App, _cx, moving| app.snapping.in_transition = moving)
                .on_item_appearing(|_me, app: &mut App, _cx, index| app.snapping.appeared = format!("ItemAppearing {index}"))
                .on_item_disappearing(|_me, app: &mut App, _cx, index| app.snapping.appeared = format!("ItemDisappearing {index}"))
                .children(
                    SLIDES
                        .iter()
                        .enumerate()
                        .map(|(i, (color, text))| {
                            SkiaShape::new().background_color(hex(*color)).use_cache(CacheType::Operations).children(
                                SkiaLabel::new((i + 1).to_string()).font_size(60).font_family("FontTextBold").text_color(hex(*text)).center(),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
            // Indicators: the selected dot stretches to 24 like the DataTrigger in the Sandbox.
            SkiaRow::new().spacing(8).horizontal_options(LayoutOptions::Center).children(
                SLIDES
                    .iter()
                    .enumerate()
                    .map(|(i, (color, _))| {
                        SkiaShape::new()
                            .corner_radius(4)
                            .height_request(8)
                            .background_color(hex(*color))
                            .observe(move |me, app: &App| me.set_width_request(if app.snapping.index == i { 24 } else { 8 }))
                    })
                    .collect::<Vec<_>>(),
            ),
            SkiaLabel::new("").font_size(13).text_color(hex(0xADB5BD)).fill_x().observe(|me, app: &App| {
                let page = &app.snapping;
                let looping = if page.looped { "Looping enabled - infinite scroll" } else { "Looping disabled - bounded scroll" };
                me.set_text(format!(
                    "Selected Index: {}   ·   InTransition: {}   ·   {looping}   ·   {}",
                    page.index, page.in_transition, page.appeared
                ));
            }),
            SkiaWrap::new().spacing(8).vertical_options(LayoutOptions::Center).children((
                SkiaLabel::new("Swipe Speed").font_size(14).text_color(Color::WHITE).vertical_options(LayoutOptions::Center),
                SPEEDS.map(speed_button).into_iter().collect::<Vec<_>>(),
                SkiaLabel::new("")
                    .font_size(12)
                    .text_color(hex(0xADB5BD))
                    .vertical_options(LayoutOptions::Center)
                    .observe(|me, app: &App| me.set_text(format!("Current: {:.1}x", app.snapping.speed))),
            )),
        ),
    )
}

fn toggle(text: &'static str, read: fn(&State) -> bool, flip: fn(&mut State)) -> Build<SkiaButton> {
    SkiaButton::new("")
        .font_size(13)
        .observe(move |me, app: &App| {
            let on = read(&app.snapping);
            me.set_text(format!("{text}: {}", if on { "On" } else { "Off" }));
            me.set_background_color(hex(if on { 0x20C997 } else { 0x495057 }));
            me.set_text_color(if on { hex(0x1A1A2E) } else { Color::WHITE });
        })
        .on_tapped(move |_me, app: &mut App, _cx| flip(&mut app.snapping))
}

fn move_button(caption: &str, action: fn(&mut Mut<'_, SkiaCarousel>, &mut State)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .font_family_fallback("FontSymbols,FontSymbols2")
        .background_color(hex(0x0F3460))
        .font_size(13)
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut carousel) = cx.get_mut(app.snapping.carousel) {
                action(&mut carousel, &mut app.snapping);
            }
        })
}

fn speed_button(speed: f32) -> Build<SkiaButton> {
    SkiaButton::new(format!("{speed:.1}x"))
        .font_size(13)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.snapping.speed == speed { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, _cx| app.snapping.speed = speed)
}

fn note(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(hex(0xADB5BD)).fill_x()
}

/// The controls of a recycled slide that a bind writes (React SlideCell).
#[derive(Default)]
struct SlideHandles {
    cell: Handle<SkiaLayout>,
    shape: Handle<SkiaShape>,
    label: Handle<SkiaLabel>,
    sub: Handle<SkiaLabel>,
}

/// Recycled carousel slide: visuals built once, a bind on every rebind.
fn slide_cell() -> (Build<SkiaLayout>, SlideHandles) {
    let mut handles = SlideHandles::default();
    let cell = SkiaLayout::new().assign(&mut handles.cell).children(
        SkiaShape::new().corner_radius(12).fill().assign(&mut handles.shape).children((
            SkiaLabel::new("").font_size(22).font_family("FontTextBold").text_color(Color::WHITE).center().assign(&mut handles.label),
            SkiaLabel::new("")
                .font_size(12)
                .text_color(Color::new(0xAAFFFFFF))
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::End)
                .margin((0, 0, 0, 10))
                .assign(&mut handles.sub),
        )),
    );
    (cell, handles)
}

fn bind_slide(slide: &SlideHandles, cx: &mut Cx, title: String, color: u32, index: usize) {
    if let Some(mut shape) = cx.get_mut(slide.shape) {
        shape.set_background_color(hex(color));
    }
    set_text(cx, slide.label, title);
    set_text(cx, slide.sub, format!("recycled cell · index {index}"));
}

fn bind_loop_slide(slide: &SlideHandles, _app: &App, index: usize, cx: &mut Cx) {
    bind_slide(slide, cx, format!("Item {}", index + 1), LOOP_COLORS[index % LOOP_COLORS.len()], index);
}

fn bind_dynamic_slide(slide: &SlideHandles, _app: &App, index: usize, cx: &mut Cx) {
    let (title, color, height) = DYNAMIC[index];
    bind_slide(slide, cx, title.to_owned(), color, index);
    if let Some(mut cell) = cx.get_mut(slide.cell) {
        cell.set_height_request(height);
    }
}

/// The drawer: a white sheet with a blue header to drag, snapping open or closed.
fn drawer(page: &mut State) -> Build<SkiaDrawer> {
    SkiaDrawer::new()
        .direction(DrawerDirection::FromBottom)
        .header_size(56)
        .height_request(320)
        .vertical_options(LayoutOptions::End)
        .horizontal_options(LayoutOptions::Fill)
        .bounces(true)
        .assign(&mut page.drawer)
        .on_is_open_changed(|_me, app: &mut App, _cx, open| app.snapping.open = open)
        .children(
            SkiaShape::new()
                .corner_radius((20, 20, 0, 0))
                .background_color(hex(0xF5F5F5))
                .fill()
                .shadows(SkiaShadow::new(Color::BLACK).x(0).y(-2).blur(8).opacity(0.4))
                .children(SkiaStack::new().spacing(0).children((
                    // Top corners only, like a MAUI CornerRadius="20,20,0,0".
                    SkiaShape::new().corner_radius((20, 20, 0, 0)).background_color(hex(0x0D6EFD)).height_request(56).fill_x().children((
                        SkiaShape::new()
                            .corner_radius(3)
                            .background_color(Color::WHITE)
                            .width_request(44)
                            .height_request(5)
                            .horizontal_options(LayoutOptions::Center)
                            .margin((0, 8, 0, 0)),
                        SkiaLabel::new("Drag me")
                            .font_size(16)
                            .font_family("FontTextBold")
                            .text_color(Color::WHITE)
                            .center()
                            .margin((0, 10, 0, 0)),
                    )),
                    SkiaStack::new().spacing(12).padding(20).children((
                        SkiaLabel::new("Drawer content").font_size(20).font_family("FontTextBold").text_color(hex(0x111827)),
                        SkiaLabel::new("Everything inside is a normal drawn tree: buttons keep working, the drawer only takes vertical drags. Release with a flick to snap open or closed.")
                            .font_size(14)
                            .text_color(hex(0x374151))
                            .fill_x(),
                        SkiaButton::new("Close").control_style(PrebuiltControlStyle::Material).on_tapped(|_me, app: &mut App, cx| {
                            if let Some(mut drawer) = cx.get_mut(app.snapping.drawer) {
                                drawer.close();
                            }
                        }),
                    )),
                ))),
        )
}
