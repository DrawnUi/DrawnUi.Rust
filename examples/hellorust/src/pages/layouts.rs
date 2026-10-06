//! Every SkiaLayout type: Absolute (SkiaLayer), Column (SkiaStack), Row (SkiaRow), Grid (SkiaGrid),
//! Wrap (SkiaWrap), ImageComposite caching, ItemsSource on Wrap / Row / Grid with Split. Ported from
//! the React demo's LayoutsPage.tsx.

use drawnui::controls::layout::SkiaDecoratedGrid;
use drawnui::prelude::*;

use super::{column, scrolling};
use crate::{App, hex};

const BODY: Color = hex(0xDEE2E6);
const MUTED: Color = hex(0xADB5BD);
const WELL: Color = hex(0x1F2937);

const PALETTE: [u32; 8] = [0x0F3460, 0x533483, 0x1B4332, 0x7B2D26, 0x495057, 0x0D6EFD, 0xD63384, 0x2D6A4F];

const FACTS: [(&str, &str); 12] = [
    ("Engine", "Skia"),
    ("Language", "Rust"),
    ("Layouts", "5 types"),
    ("Cache", "4 kinds"),
    ("Gestures", "Unified"),
    ("Shaders", "SkSL"),
    ("Fonts", "Any TTF"),
    ("Animations", "60 fps"),
    ("Navigation", "SkiaShell"),
    ("Accessibility", "ARIA overlay"),
    ("License", "MIT"),
    ("Docs", "drawnui.net"),
];

/// What the page keeps: the templated layouts' item count and the Split / DynamicColumns picks,
/// and the spinning child of the composite layer.
pub struct State {
    pub(super) count: usize,
    pub(super) split: i32,
    dynamic: bool,
    pub(super) spinner: Handle<SkiaShape>,
    pub(super) angle: f32,
    composite: Handle<SkiaLayout>,
    /// The children of the composite layer, in order.
    children: Vec<ControlId>,
    /// "last record: … · N of 25 children redrawn".
    pub(super) info: String,
    /// What the readout was made of, to write it again only when it changes.
    last_record: Option<(bool, usize)>,
    /// Which children the last record drew again (indexes into `children`).
    pub(super) redrawn: Vec<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            count: 10,
            split: 3,
            dynamic: false,
            spinner: Handle::default(),
            angle: 0.0,
            composite: Handle::default(),
            children: Vec::new(),
            info: String::new(),
            last_record: None,
            redrawn: Vec::new(),
        }
    }
}

/// The page opened: the child of the composite layer turns 6 degrees every 40 ms.
pub fn opened(app: &mut App, cx: &mut Cx) {
    cx.after(app.layouts.spinner, 40, spin);
}

fn spin(app: &mut App, cx: &mut Cx) {
    let page = &mut app.layouts;
    page.angle = (page.angle + 6.0) % 360.0;
    if let Some(mut spinner) = cx.get_mut(page.spinner) {
        spinner.set_rotation(page.angle);
    }
    // What the composite's last record did (React LastCompositeRecord).
    if let Some(record) = cx.last_composite_record(page.composite) {
        let summary = (record.partial, record.children.len());
        if page.last_record != Some(summary) {
            page.last_record = Some(summary);
            let mode = if record.partial { "partial" } else { "full" };
            page.info = format!("last record: {mode} · {} of {} children redrawn", summary.1, page.children.len());
        }
        page.redrawn.clear();
        page.redrawn.extend(record.children.iter().filter_map(|id| page.children.iter().position(|child| child == id)));
    }
    cx.after(page.spinner, 40, spin);
}

/// Where the spinning child of the composite layer sits.
const COMPOSITE_SPINNER: (i32, i32, i32, i32) = (12 + 5 * 52 + 46 - 22, 12 + 40 + 15 - 22, 0, 0);

/// Where child `i` of the composite layer sits: two rows of twelve, then the spinner.
fn composite_margin(i: usize) -> (i32, i32, i32, i32) {
    if i == 24 {
        return COMPOSITE_SPINNER;
    }
    let (column, row) = ((i % 12) as i32, (i / 12) as i32);
    (12 + column * 52, 12 + row * 70, 0, 0)
}

/// One of the 24 still children of the composite layer.
fn composite_shape(i: usize) -> Build<SkiaShape> {
    SkiaShape::new()
        .shape_type(if i % 3 == 0 { ShapeType::Circle } else { ShapeType::Rectangle })
        .corner_radius(6)
        .width_request(40)
        .height_request(40)
        .background_color(hex(PALETTE[i % PALETTE.len()]))
        .margin(composite_margin(i))
        .use_cache(CacheType::Operations)
}

/// The outline of child `i` of the composite layer, shown while the last record drew it again.
fn outline(i: usize) -> Build<SkiaShape> {
    let size = if i == 24 { 44 } else { 40 };
    SkiaShape::new()
        .corner_radius(if i == 24 { 4 } else { 6 })
        .width_request(size)
        .height_request(size)
        .stroke_color(Color::WHITE)
        .stroke_width(2)
        .margin(composite_margin(i))
        .is_visible(false)
        .observe(move |me, app: &App| me.set_is_visible(app.layouts.redrawn.contains(&i)))
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    // Nothing here changes: every card is one bitmap, blitted while the page scrolls.
    scrolling(column().children((
        heading("SkiaLayout types", 24),
        SkiaLabel::new("One class, five Type values; the aliases preset Type (+ HorizontalOptions=Fill for SkiaStack / SkiaLayer / SkiaWrap / SkiaGrid). Children position themselves with HorizontalOptions / VerticalOptions / Margin, the container adds Spacing and Padding.")
            .font_size(13)
            .text_color(hex(0xD3D3D3))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center),
        heading("Absolute · SkiaLayer", 20),
        card(
            "Children overlap in one cell; alignment + Margin place them (WPF-style, no X/Y)",
            SkiaLayer::new().height_request(150).background_color(WELL).children((
                boxed("Start/Start", 0x0D6EFD),
                boxed("Center/Start", 0x6610F2).horizontal_options(LayoutOptions::Center),
                boxed("End/Start", 0xD63384).horizontal_options(LayoutOptions::End),
                boxed("Start/Center", 0xFD7E14).vertical_options(LayoutOptions::Center),
                boxed("Center/Center", 0x20C997).center(),
                boxed("End/Center", 0x0DCAF0).horizontal_options(LayoutOptions::End).vertical_options(LayoutOptions::Center),
                boxed("Start/End", 0x6EA8FE).vertical_options(LayoutOptions::End),
                boxed("Margin(0,0,0,12)", 0xFFC107)
                    .horizontal_options(LayoutOptions::Center)
                    .vertical_options(LayoutOptions::End)
                    .margin((0, 0, 0, 12)),
                boxed("End/End", 0xDC3545).horizontal_options(LayoutOptions::End).vertical_options(LayoutOptions::End),
            )),
        ),
        card(
            "Icon + text with an Absolute layer instead of a grid (cheaper): label gets the icon's width as Margin",
            SkiaLayer::new().children((
                SkiaShape::new().shape_type(ShapeType::Circle).background_color(hex(0x6EA8FE)).width_request(36).lock_ratio(1),
                SkiaLabel::new("margin((48, 0, 0, 0)), vertical_options(Center) — no second column to measure.")
                    .font_size(14)
                    .text_color(BODY)
                    .margin((48, 0, 0, 0))
                    .vertical_options(LayoutOptions::Center)
                    .fill_x(),
            )),
        ),
        card(
            "IsClippedToBounds — a child larger than its parent",
            SkiaWrap::new().spacing(24).children((
                clip_demo("overflows (default)", false),
                clip_demo("IsClippedToBounds", true),
                SkiaLayer::new()
                    .width_request(140)
                    .height_request(70)
                    .background_color(WELL)
                    .is_clipped_to_bounds(true)
                    .clip_effects(false)
                    .children((
                        SkiaShape::new()
                            .corner_radius(8)
                            .background_color(hex(0x20C997))
                            .width_request(100)
                            .height_request(40)
                            .center()
                            .shadows(SkiaShadow::new(hex(0x20C997)).x(0).y(0).blur(12).opacity(1)),
                        SkiaLabel::new("ClipEffects={false}").font_size(11).text_color(Color::WHITE).padding(6),
                    )),
            )),
        ),
        heading("Column · SkiaStack", 20),
        card(
            "ZIndex draws later (on top); HorizontalFillRatio/VerticalFillRatio = fraction of the box; Left/Top nudge the drawn output",
            SkiaLayer::new().height_request(120).background_color(hex(0x212529)).children((
                SkiaShape::new()
                    .corner_radius(8)
                    .background_color(hex(0x0D6EFD))
                    .fill()
                    .horizontal_fill_ratio(0.5)
                    .vertical_fill_ratio(0.75)
                    .z_index(2)
                    .children(centered("ZIndex=2 · FillRatio 0.5 × 0.75", 12, Color::WHITE)),
                SkiaShape::new()
                    .corner_radius(8)
                    .background_color(hex(0xD63384))
                    .width_request(220)
                    .height_request(70)
                    .margin((120, 30, 0, 0))
                    .z_index(1)
                    .children(centered("ZIndex=1, declared second", 12, Color::WHITE)),
                SkiaShape::new()
                    .corner_radius(8)
                    .background_color(hex(0x20C997))
                    .width_request(160)
                    .height_request(50)
                    .horizontal_options(LayoutOptions::End)
                    .vertical_options(LayoutOptions::End)
                    .left(-20)
                    .top(-10)
                    .z_index(3)
                    .children(centered("Left=-20 Top=-10 · ZIndex=3", 11, hex(0x1A1A2E))),
            )),
        ),
        card(
            "Vertical stack, Spacing between children, each child aligns horizontally on its own",
            SkiaStack::new().spacing(6).background_color(WELL).padding(8).children((
                boxed("Start (default)", 0x0D6EFD),
                boxed("HorizontalOptions=Center", 0x6610F2).horizontal_options(LayoutOptions::Center),
                boxed("HorizontalOptions=End", 0xD63384).horizontal_options(LayoutOptions::End),
                boxed("HorizontalOptions=Fill", 0x20C997).fill_x(),
                boxed("WidthRequest=200", 0xFD7E14).width_request(200),
            )),
        ),
        heading("Row · SkiaRow", 20),
        card(
            "Horizontal stack; the row is as tall as its tallest child, children align vertically",
            (SkiaRow::new().spacing(8).background_color(WELL).padding(8).fill_x().children((
                boxed("Start", 0x0D6EFD).height_request(70),
                boxed("Center", 0x6610F2).vertical_options(LayoutOptions::Center),
                boxed("End", 0xD63384).vertical_options(LayoutOptions::End),
                boxed("Fill", 0x20C997).fill_y(),
                boxed("Margin(16,0,0,0)", 0xFD7E14).vertical_options(LayoutOptions::Center).margin((16, 0, 0, 0)),
            )),
            // The React page's note. This engine, like the C# one, shares the rest of a Row among
            // its Fill children (test `csharp_row_fill_child_takes_what_is_left`).
            SkiaLabel::new("A Row gives children an infinite width: Fill on the main axis auto-sizes (MAUI stack semantics). Use SkiaGrid with a * column when something must take the remaining width.")
                .font_size(12)
                .text_color(MUTED)
                .fill_x()),
        ),
        // A tuple of children holds twelve at most: the rest of the page is one more tuple.
        (
            grids(),
            heading("Wrap · SkiaWrap", 20),
            card(
                "Type=Wrap · Spacing 8 · resize the window",
                SkiaWrap::new().spacing(8).children(
                    ["Absolute", "Column", "Row", "Wrap", "Grid", "SkiaStack", "SkiaRow", "SkiaLayer", "SkiaWrap", "SkiaGrid", "Spacing", "Padding", "Margin"]
                        .map(|text| {
                            SkiaShape::new()
                                .corner_radius(14)
                                .background_color(hex(0x373B3E))
                                .children(SkiaLabel::new(text).font_size(13).text_color(BODY).padding((12, 6)))
                        })
                        .into_iter()
                        .collect::<Vec<_>>(),
                ),
            ),
            heading("Caching · UseCache=ImageComposite", 20),
            // One child spins every 40 ms: the card is painted live, only the composite layer records.
            card_with(
                title("").observe(|me, app: &App| {
                    let info = if app.layouts.info.is_empty() { "…" } else { app.layouts.info.as_str() };
                    me.set_text(format!("SkiaLayer UseCache=\"ImageComposite\" · 24 shapes + 1 rotating · {info}"));
                }),
                (
                    SkiaLayer::new().height_request(150).fill_x().children((
                        SkiaLayer::new()
                            .use_cache(CacheType::ImageComposite)
                            .height_request(150)
                            .fill_x()
                            .background_color(hex(0x212529))
                            .assign(&mut app.layouts.composite)
                            .children((
                                (0..24)
                                    .map(|i| {
                                        let shape = composite_shape(i);
                                        app.layouts.children.push(shape.id());
                                        shape
                                    })
                                    .collect::<Vec<_>>(),
                                {
                                    let spinner = SkiaShape::new()
                                        .corner_radius(4)
                                        .width_request(44)
                                        .height_request(44)
                                        .background_color(hex(0xFFC107))
                                        .margin(COMPOSITE_SPINNER)
                                        .use_cache(CacheType::Operations)
                                        .z_index(5)
                                        .assign(&mut app.layouts.spinner);
                                    app.layouts.children.push(spinner.id());
                                    spinner
                                },
                            )),
                        // Outside the composite: a white outline = drawn again by the last record,
                        // everything else was kept as it was.
                        SkiaLayer::new()
                            .height_request(150)
                            .fill_x()
                            .input_transparent(true)
                            .children((0..25).map(outline).collect::<Vec<_>>()),
                    )),
                    SkiaLabel::new("White outlines = the children the last record repainted (the rotating one + every sibling its old and new bounds overlap); the others are kept in the cache surface untouched. RepaintComposition() from the spinning child marks it dirty in the composite parent (C# DirtyChildrenTracker); own content / measure changes record fully.")
                        .font_size(12)
                        .text_color(MUTED)
                        .fill_x(),
                ),
            )
            .use_cache(CacheType::None),
            heading("ItemsSource + ItemTemplate for Wrap / Row / Grid · Split", 20),
            templated(),
        ),
    )))
}

/// The templated layouts: ItemsSource + ItemTemplate on a Wrap, a Row, a decorated Grid and a Grid.
fn templated() -> impl IntoChildren {
    (
        // The buttons change the wrap and ripple: this card is painted live, not recorded again per frame.
        card_with(
            title("").observe(|me, app: &App| {
                let (count, split, dynamic) = (app.layouts.count, app.layouts.split, app.layouts.dynamic);
                me.set_text(format!("SkiaWrap ItemsSource ({count} recycled ChipCell) · Split={split} · DynamicColumns={dynamic}"));
            }),
            (
                SkiaWrap::new()
                    .spacing(8)
                    .observe(|me, app: &App| {
                        me.set_split(app.layouts.split);
                        me.set_dynamic_columns(app.layouts.dynamic);
                    })
                    .items(|app: &App| app.layouts.count, chip_cell, bind_chip),
                SkiaWrap::new().spacing(6).children((
                    [0, 2, 3, 4].map(split_button).into_iter().collect::<Vec<_>>(),
                    SkiaButton::new("")
                        .font_size(12)
                        .observe(|me, app: &App| {
                            let on = app.layouts.dynamic;
                            me.set_text(format!("DynamicColumns {}", if on { "on" } else { "off" }));
                            me.set_background_color(hex(if on { 0x533483 } else { 0x495057 }));
                        })
                        .on_tapped(|_me, app: &mut App, _cx| app.layouts.dynamic = !app.layouts.dynamic),
                    SkiaButton::new("+ item").background_color(hex(0x0D6EFD)).font_size(12).on_tapped(|_me, app: &mut App, _cx| app.layouts.count += 1),
                    SkiaButton::new("- item")
                        .background_color(hex(0x0D6EFD))
                        .font_size(12)
                        .on_tapped(|_me, app: &mut App, _cx| app.layouts.count = app.layouts.count.saturating_sub(1).max(1)),
                )),
            ),
        )
        .use_cache(CacheType::None),
        card(
            "SkiaRow ItemsSource (same cells, laid out horizontally, every item realized)",
            SkiaRow::new().spacing(8).items(|app: &App| app.layouts.count.min(5), chip_cell, bind_chip),
        ),
        card(
            "SkiaDecoratedGrid ItemsSource · Split=4 · ColumnSpacing / RowSpacing 1 · gradient lines in the spacing",
            SkiaDecoratedGrid::new()
                .split(4)
                .column_definitions("*,*,*,*")
                .column_spacing(1)
                .row_spacing(1)
                .background_color(hex(0x212529))
                .items(|_app: &App| FACTS.len(), text_cell, bind_text),
        ),
        card(
            "SkiaGrid ItemsSource · Split=3 · Invert (column-major)",
            SkiaGrid::new()
                .split(3)
                .invert(true)
                .column_definitions("*,*,*")
                .column_spacing(8)
                .row_spacing(8)
                .items(|app: &App| app.layouts.count, chip_cell, bind_chip),
        ),
    )
}

fn split_button(split: i32) -> Build<SkiaButton> {
    SkiaButton::new(if split == 0 { "Split 0 (flow)".to_owned() } else { format!("Split {split}") })
        .font_size(12)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.layouts.split == split { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, _cx| app.layouts.split = split)
}

/// Recycled chip cell for the templated layouts: visuals once, `bind_chip` per bind.
fn chip_cell() -> (Build<SkiaLayout>, (Handle<SkiaShape>, Handle<SkiaLabel>)) {
    let (mut shape, mut label) = (Handle::default(), Handle::default());
    let cell = SkiaLayout::new().fill_x().children(
        SkiaShape::new()
            .corner_radius(10)
            .fill_x()
            .assign(&mut shape)
            .children(SkiaLabel::new("").font_size(13).text_color(BODY).padding((12, 8)).horizontal_options(LayoutOptions::Center).assign(&mut label)),
    );
    (cell, (shape, label))
}

fn bind_chip(cell: &(Handle<SkiaShape>, Handle<SkiaLabel>), _app: &App, index: usize, cx: &mut Cx) {
    super::set_text(cx, cell.1, format!("Item {}", index + 1));
    if let Some(mut shape) = cx.get_mut(cell.0) {
        shape.set_background_color(hex(PALETTE[index % PALETTE.len()]));
    }
}

/// Text cell for the decorated grid: a title and a value, padded, no background: the grid lines
/// separate the cells.
fn text_cell() -> (Build<SkiaLayout>, (Handle<SkiaLabel>, Handle<SkiaLabel>)) {
    let (mut title, mut value) = (Handle::default(), Handle::default());
    let cell = SkiaLayout::new().layout_type(LayoutType::Column).padding((12, 10)).fill_x().children((
        SkiaLabel::new("").font_size(11).text_color(hex(0x8B95A1)).text_transform(TextTransform::Uppercase).assign(&mut title),
        SkiaLabel::new("").font_size(15).text_color(BODY).font_family("FontTextBold").assign(&mut value),
    ));
    (cell, (title, value))
}

fn bind_text(cell: &(Handle<SkiaLabel>, Handle<SkiaLabel>), _app: &App, index: usize, cx: &mut Cx) {
    let (title, value) = FACTS[index];
    super::set_text(cx, cell.0, title);
    super::set_text(cx, cell.1, value);
}

/// The "Grid · SkiaGrid" part of the page.
fn grids() -> impl IntoChildren {
    (
        heading("Grid · SkiaGrid", 20),
        card(
            "ColumnDefinitions=\"*, 2*, Auto\" RowDefinitions=\"Auto, 60\" · ColumnSpacing/RowSpacing 8",
            SkiaGrid::new().column_definitions("*, 2*, Auto").row_definitions("Auto, 60").column_spacing(8).row_spacing(8).children((
                cell("*", 0x0D6EFD, 0, 0),
                cell("2*", 0x6610F2, 1, 0),
                cell("Auto (this label)", 0xD63384, 2, 0),
                cell("Row 1 = 60pt", 0x20C997, 0, 1),
                cell("Column 1", 0xFD7E14, 1, 1),
                cell("Auto", 0xDC3545, 2, 1),
            )),
        ),
        card(
            "ColumnSpan / RowSpan",
            SkiaGrid::new().column_definitions("*, *, *").row_definitions("48, 48, 48").column_spacing(6).row_spacing(6).children((
                cell("ColumnSpan=2", 0x0D6EFD, 0, 0).column_span(2),
                cell("RowSpan=2", 0x6610F2, 2, 0).row_span(2),
                cell("0,1", 0x20C997, 0, 1),
                cell("1,1", 0xFD7E14, 1, 1),
                cell("ColumnSpan=3", 0xD63384, 0, 2).column_span(3),
            )),
        ),
        card(
            "Implicit tracks: no definitions, children reference Column/Row (DefaultColumnDefinition = Auto)",
            SkiaGrid::new().column_spacing(12).row_spacing(4).horizontal_options(LayoutOptions::Start).children((
                SkiaLabel::new("Name").font_size(14).text_color(MUTED).column(0).row(0),
                SkiaLabel::new("DrawnUI").font_size(14).text_color(Color::WHITE).column(1).row(0),
                SkiaLabel::new("Renderer").font_size(14).text_color(MUTED).column(0).row(1),
                SkiaLabel::new("Skia (OpenGL / WebGL2)").font_size(14).text_color(Color::WHITE).column(1).row(1),
                SkiaLabel::new("License").font_size(14).text_color(MUTED).column(0).row(2),
                SkiaLabel::new("MIT").font_size(14).text_color(Color::WHITE).column(1).row(2),
            )),
        ),
        card(
            "Icon + text pattern: Auto column for the icon, * for wrapping text",
            SkiaGrid::new().column_definitions("Auto, *").column_spacing(12).children((
                SkiaShape::new()
                    .shape_type(ShapeType::Circle)
                    .background_color(hex(0x6EA8FE))
                    .width_request(40)
                    .lock_ratio(1)
                    .vertical_options(LayoutOptions::Start)
                    .column(0)
                    .row(0),
                SkiaLabel::new("The star column takes whatever the Auto column leaves, and this label wraps inside it. The row is Auto, so it grows with the text — the same layout a MAUI Grid would produce.")
                    .font_size(14)
                    .text_color(BODY)
                    .fill_x()
                    .column(1)
                    .row(0),
            )),
        ),
    )
}

fn heading(text: &str, size: i32) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(size)
        .text_color(Color::WHITE)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 8, 0, 0))
}

/// A titled card: one bitmap, recorded again only when something in it changes.
fn card(text: &str, content: impl IntoChildren) -> Build<SkiaShape> {
    card_with(title(text), content)
}

/// A card whose title label is given, for a title that changes.
fn card_with(title: Build<SkiaLabel>, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(hex(0x2B3035))
        .fill_x()
        .use_cache(CacheType::Image)
        .children(SkiaStack::new().spacing(10).padding((16, 12)).children((title, content)))
}

fn title(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(12)
        .text_color(hex(0x6EA8FE))
        .font_attributes(FontAttributes::Bold)
        .text_transform(TextTransform::Uppercase)
}

fn centered(text: &str, size: i32, color: Color) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(size).text_color(color).center()
}

/// A colored cell with a centered caption, placed in a grid cell.
fn cell(text: &str, color: u32, column: i32, row: i32) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(6)
        .background_color(hex(color))
        .fill()
        .column(column)
        .row(row)
        .children(SkiaLabel::new(text).font_size(13).text_color(Color::WHITE).center().padding((8, 6)))
}

/// A small labeled box used by the stack / row / layer demos.
fn boxed(text: &str, color: u32) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(6)
        .background_color(hex(color))
        .children(SkiaLabel::new(text).font_size(12).text_color(Color::WHITE).center().padding((10, 6)))
}

fn clip_demo(caption: &str, clip: bool) -> Build<SkiaLayout> {
    SkiaLayer::new().width_request(140).height_request(70).background_color(WELL).is_clipped_to_bounds(clip).children((
        SkiaShape::new()
            .shape_type(ShapeType::Circle)
            .background_color(hex(0xD63384))
            .width_request(110)
            .lock_ratio(1)
            .horizontal_options(LayoutOptions::End)
            .vertical_options(LayoutOptions::End)
            .margin((0, 0, -30, -30)),
        SkiaLabel::new(caption).font_size(11).text_color(Color::WHITE).padding(6),
    ))
}
