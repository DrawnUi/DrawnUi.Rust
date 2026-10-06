//! SkiaImage: one source, every TransformAspect side by side, the built-in effects, custom
//! filters, tiling, the preload queue and alignment. Ported from the React demo's ImagesPage.tsx.

use drawnui::prelude::*;

use drawnui::skia::{color_filters, image_filters};

use super::scrolling;
use crate::{App, hex};

pub const PHOTO: &str = "assets/images/baboon.jpg";

const ASPECTS: [TransformAspect; 9] = [
    TransformAspect::AspectCover,
    TransformAspect::AspectFit,
    TransformAspect::AspectFill,
    TransformAspect::AspectFitFill,
    TransformAspect::Fill,
    TransformAspect::Fit,
    TransformAspect::FitFill,
    TransformAspect::Cover,
    TransformAspect::None,
];

/// Preload queue demo: 8 distinct urls of the same photo (cache-busting query) through the queue.
const PRELOAD: [&str; 8] = [
    "assets/images/glass2.jpg?queue=0",
    "assets/images/glass2.jpg?queue=1",
    "assets/images/glass2.jpg?queue=2",
    "assets/images/glass2.jpg?queue=3",
    "assets/images/glass2.jpg?queue=4",
    "assets/images/glass2.jpg?queue=5",
    "assets/images/glass2.jpg?queue=6",
    "assets/images/glass2.jpg?queue=7",
];

/// A red / blue channel swap (the React page's `ColorFilter.MakeMatrix`).
const SWAP_RED_BLUE: [f32; 20] = [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// What the page keeps.
pub struct State {
    tiles: Handle<SkiaImageTiles>,
    /// Points the tiles are shifted by, 4 more every 50 ms.
    pub(super) offset: f32,
    preload: Handle<SkiaButton>,
    /// What the preload queue does, for the heading.
    pub(super) queue: String,
    /// Most loads in flight at once during the run.
    peak: usize,
    started_ms: f64,
    /// The 10 ms poll of the counts while a run goes on.
    poll: Option<AnimationId>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tiles: Handle::default(),
            offset: 0.0,
            preload: Handle::default(),
            queue: "idle".to_owned(),
            peak: 0,
            started_ms: 0.0,
            poll: None,
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    scrolling(
        // The pictures do not change once loaded: every section of them is one bitmap, so their
        // color filters and the blur run once, not on every scrolled frame. The tiles move and
        // are painted live.
        SkiaStack::new().spacing(16).padding(16).children((
            SkiaLabel::new("SkiaImage · Aspect").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
            SkiaLabel::new("Same 512×512 photo in a 220×120 box. Overflow is clipped to the box.")
                .font_size(13)
                .text_color(hex(0xD3D3D3))
                .horizontal_options(LayoutOptions::Center),
            flow(ASPECTS.map(aspect_tile).into_iter().collect::<Vec<_>>()),
            heading("Effects · AddEffect, Blur, Zoom, offsets"),
            flow((
                effect("BlackAndWhite", |i| i.add_effect(SkiaImageEffect::BlackAndWhite)),
                effect("Sepia", |i| i.add_effect(SkiaImageEffect::Sepia)),
                effect("Pastel", |i| i.add_effect(SkiaImageEffect::Pastel)),
                effect("InvertColors", |i| i.add_effect(SkiaImageEffect::InvertColors)),
                effect("Tint #0D6EFD Multiply", |i| {
                    i.add_effect(SkiaImageEffect::Tint).color_tint(hex(0x0D6EFD)).effect_blend_mode(BlendMode::Multiply)
                }),
                effect("Darken={80}", |i| i.add_effect(SkiaImageEffect::Darken).darken(80)),
                effect("Lighten={80}", |i| i.add_effect(SkiaImageEffect::Lighten).lighten(80)),
                effect("Contrast={1.5}", |i| i.add_effect(SkiaImageEffect::Contrast).contrast(1.5)),
                effect("Saturation={2}", |i| i.add_effect(SkiaImageEffect::Saturation).saturation(2)),
                // A tuple of children holds twelve at most.
                (effect("Blur={3}", |i| i.blur(3)),
                effect("ZoomX/Y={1.8}", |i| i.zoom_x(1.8).zoom_y(1.8)),
                effect("HorizontalOffset={-40}", |i| i.horizontal_offset(-40).aspect(TransformAspect::AspectFit)),
                effect("HSL Gamma=0.6 (hue) Sat=1 Bright=0.5", |i| {
                    i.add_effect(SkiaImageEffect::HSL).background_color(Color::WHITE).gamma(0.6).saturation(1).brightness(0.5)
                })),
            )),
            heading("Custom filters · AddEffect=\"Custom\" + PaintColorFilter / PaintImageFilter"),
            flow((
                tile(
                    "PaintColorFilter = MakeMatrix (R↔B)",
                    SkiaImage::new(PHOTO)
                        .width_request(160)
                        .height_request(100)
                        .add_effect(SkiaImageEffect::Custom)
                        .paint_color_filter(color_filters::matrix_row_major(&SWAP_RED_BLUE, None)),
                ),
                tile(
                    "PaintImageFilter = MakeDilate(4)",
                    SkiaImage::new(PHOTO).width_request(160).height_request(100).paint_image_filter(image_filters::dilate((4.0, 4.0), None, None).expect("a dilate filter")),
                ),
            )),
            heading("SkiaImageTiles · TileWidth/TileHeight, TileOffsetX animates"),
            SkiaImageTiles::new(PHOTO)
                .tile_width(64)
                .tile_height(64)
                .horizontal_options(LayoutOptions::Center)
                .width_request(400)
                .height_request(160)
                .background_color(Color::BLACK)
                .assign(&mut app.images.tiles),
            heading("").observe(|me, app: &App| me.set_text(format!("SkiaImageManager preload queue · {}", app.images.queue))),
            SkiaButton::new("PreloadImages(8 urls, Low)")
                .background_color(hex(0x0D6EFD))
                .font_size(13)
                .horizontal_options(LayoutOptions::Center)
                .assign(&mut app.images.preload)
                .on_tapped(|_me, app: &mut App, cx| preload(app, cx)),
            (
                heading("Alignment inside the box"),
                flow((
                    aligned(TransformAspect::AspectFit, DrawImageAlignment::Start),
                    aligned(TransformAspect::Fit, DrawImageAlignment::Center),
                    aligned(TransformAspect::Fit, DrawImageAlignment::End),
                )),
            ),
        )),
    )
}

fn heading(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(20)
        .text_color(Color::WHITE)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 12, 0, 0))
}

/// The tiles of one section: a centered wrap capped at 720 points, one bitmap.
fn flow(tiles: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaWrap::new()
        .spacing(16)
        .horizontal_options(LayoutOptions::Center)
        .maximum_width_request(720)
        .use_cache(CacheType::Image)
        .children(tiles)
}

/// Loads the 8 urls again through the manager's queue, 5 in flight at most, and reports how the
/// queue ran (React PreloadImages with a 10 ms poll of RunningCount / QueuedCount).
fn preload(app: &mut App, cx: &mut Cx) {
    for source in PRELOAD {
        cx.images().clear_source(source);
    }
    let page = &mut app.images;
    if let Some(poll) = page.poll.take() {
        cx.stop_animation(poll);
    }
    (page.peak, page.started_ms) = (0, cx.time_ms());
    page.poll = Some(cx.after(page.preload, 10, poll));
    cx.preload_images_then(PRELOAD, |app: &mut App, cx| {
        let page = &mut app.images;
        if let Some(poll) = page.poll.take() {
            cx.stop_animation(poll);
        }
        let ms = cx.time_ms() - page.started_ms;
        page.queue = format!("8 loaded in {ms:.0} ms · peak in flight {} (MaxParallelLoads={})", page.peak, drawnui::Images::MAX_IN_FLIGHT);
    });
}

fn poll(app: &mut App, cx: &mut Cx) {
    let (running, queued) = (cx.images().in_flight(), cx.images().queued());
    let page = &mut app.images;
    page.peak = page.peak.max(running);
    page.queue = format!("running {running} · queued {queued} · peak {}", page.peak);
    page.poll = Some(cx.after(page.preload, 10, poll));
}

/// The page opened: the tiles start moving, 4 points every 50 ms as the React page's interval.
pub fn opened(app: &mut App, cx: &mut Cx) {
    cx.after(app.images.tiles, 50, tick);
}

fn tick(app: &mut App, cx: &mut Cx) {
    app.images.offset += 4.0;
    let offset = app.images.offset;
    if let Some(mut tiles) = cx.get_mut(app.images.tiles) {
        tiles.set_tile_offset_x(offset);
        tiles.set_tile_offset_y(offset / 2.0);
    }
    cx.after(app.images.tiles, 50, tick);
}

fn aspect_tile(aspect: TransformAspect) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(4).width_request(220).children((
        SkiaImage::new(PHOTO).width_request(220).height_request(120).aspect(aspect).background_color(Color::BLACK),
        SkiaLabel::new(format!("{aspect:?}")).font_size(15).text_color(Color::WHITE),
        SkiaLabel::new(format!("Aspect=\"{aspect:?}\"")).font_size(12).text_color(hex(0x94A3B8)),
    ))
}

fn tile(caption: &str, visual: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaStack::new()
        .spacing(4)
        .width_request(160)
        // The fallback draws the arrow of "R↔B".
        .children((visual, SkiaLabel::new(caption).font_size(12).text_color(hex(0x94A3B8)).font_family_fallback("FontSymbols,FontSymbols2")))
}

/// The photo in a 160 x 100 box with one effect set on it.
fn effect(title: &str, configure: impl FnOnce(Build<SkiaImage>) -> Build<SkiaImage>) -> Build<SkiaLayout> {
    tile(title, configure(SkiaImage::new(PHOTO).width_request(160).height_request(100).background_color(Color::BLACK)))
}

/// The photo in a 120 x 120 box, aligned the same way on both axes.
fn aligned(aspect: TransformAspect, alignment: DrawImageAlignment) -> Build<SkiaImage> {
    SkiaImage::new(PHOTO)
        .width_request(120)
        .height_request(120)
        .aspect(aspect)
        .horizontal_alignment(alignment)
        .vertical_alignment(alignment)
        .background_color(Color::BLACK)
}
