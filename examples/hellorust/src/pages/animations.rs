//! SkiaLottie and SkiaGif, the frame players. Ported from the React demo's AnimationsPage.tsx.

use drawnui::prelude::*;

use super::{card, card_title, page_title, scrolling};
use crate::{App, hex};

const GIF: &str = "assets/images/banana.gif";
const SHIELD: &str = "assets/lottie/shield.json";
const OK: &str = "assets/lottie/ok.json";

/// What the page keeps.
pub struct State {
    lottie: Handle<SkiaLottie>,
    pub(super) status: String,
    pub(super) speed: f32,
    pub(super) toggle: bool,
    pub(super) gif: Handle<SkiaGif>,
    pub(super) gif_status: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            lottie: Handle::default(),
            status: "loading…".to_owned(),
            speed: 1.0,
            toggle: false,
            gif: Handle::default(),
            gif_status: "loading…".to_owned(),
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    // The GIFs play all the time: nothing above them is cached.
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        page_title("Lottie & GIF"),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!("SkiaLottie — Source=\"lottie/shield.json\" Repeat={{-1}} · {}", app.animations.status))
            }),
            SkiaWrap::new().spacing(16).children((
                SkiaLottie::new(SHIELD)
                    .width_request(160)
                    .height_request(160)
                    .repeat(-1)
                    .assign(&mut app.animations.lottie)
                    .observe(|me, app: &App| me.set_speed_ratio(app.animations.speed))
                    .on_success(|me, app: &mut App, cx, _source| {
                        let frames = cx.find::<SkiaLottie>(me).map_or(0.0, |lottie| lottie.total_frames());
                        app.animations.status = format!("loaded, {frames} frames, playing");
                    })
                    .on_error(|_me, app: &mut App, _cx, source| app.animations.status = format!("error: {source} did not load"))
                    .on_started(|_me, app: &mut App, _cx| app.animations.status = "Started".to_owned())
                    .on_finished(|_me, app: &mut App, _cx| app.animations.status = "Finished".to_owned()),
                SkiaStack::new().spacing(8).vertical_options(LayoutOptions::Center).fill_x().children((
                    SkiaWrap::new().spacing(8).children((
                        lottie_button("Start", 0x0D6EFD, |lottie| lottie.start()),
                        lottie_button("Stop", 0x6C757D, |lottie| lottie.stop()),
                        lottie_button("Seek(30)", 0x6C757D, |lottie| {
                            lottie.stop();
                            lottie.seek(30);
                        }),
                        lottie_button("GoToEnd", 0x6C757D, |lottie| {
                            lottie.stop();
                            lottie.go_to_end();
                        }),
                    )),
                    SkiaWrap::new().spacing(8).children((
                        SkiaLabel::new("SpeedRatio").font_size(13).text_color(hex(0xADB5BD)).vertical_options(LayoutOptions::Center),
                        [0.5, 1.0, 2.0].map(speed_button).into_iter().collect::<Vec<_>>(),
                    )),
                    note("The vector animation is rendered every frame into its cache: ImageDoubleBuffered on the desktop (the bitmap is made off the frame thread), a recorded picture in the browser; the animator is the C# RangeAnimator over InPoint..OutPoint."),
                )),
            )),
        ),
        card(
            card_title("ColorTint / Colors — colors replaced in the JSON before parsing (C# ApplyTint)"),
            SkiaWrap::new().spacing(12).children((
                small(OK),
                small(OK).color_tint(hex(0x20C997)),
                small(OK).colors(vec![hex(0xD63384), hex(0xFFC107)]),
                small(SHIELD).color_tint(hex(0x0DCAF0)).speed_ratio(0.5),
            )),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!("IsOn toggle — AutoPlay={{false}}, DefaultFrame=0 / DefaultFrameWhenOn=-1 · IsOn={}", app.animations.toggle))
            }),
            SkiaWrap::new().spacing(16).children((
                SkiaLottie::new(OK)
                    .width_request(90)
                    .height_request(90)
                    .auto_play(false)
                    .default_frame(0)
                    .default_frame_when_on(-1)
                    .observe(|me, app: &App| me.set_is_on(app.animations.toggle)),
                SkiaButton::new("")
                    .background_color(hex(0x6610F2))
                    .vertical_options(LayoutOptions::Center)
                    .observe(|me, app: &App| me.set_text(if app.animations.toggle { "IsOn = false" } else { "IsOn = true" }))
                    .on_tapped(|_me, app: &mut App, _cx| app.animations.toggle = !app.animations.toggle),
                note("Stopped animations show DefaultFrame, or DefaultFrameWhenOn (-1 = last frame) when IsOn: the C# recipe for animated checkboxes.")
                    .vertical_options(LayoutOptions::Center),
            )),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!("SkiaGif — Source=\"images/banana.gif\" Aspect=AspectFitFill · {}", app.animations.gif_status))
            }),
            SkiaWrap::new().spacing(16).children((
                SkiaGif::new(GIF)
                    .width_request(140)
                    .height_request(140)
                    .repeat(-1)
                    .background_color(hex(0x212529))
                    .assign(&mut app.animations.gif)
                    .on_started(|me, app: &mut App, cx| {
                        let (frames, ms) = cx
                            .find::<SkiaGif>(me)
                            .and_then(|gif| gif.animation())
                            .map_or((0, 0), |frames| (frames.images.len(), frames.duration_ms()));
                        app.animations.gif_status = format!("{frames} frames, {ms} ms, playing");
                    })
                    .on_finished(|_me, app: &mut App, _cx| app.animations.gif_status = "Finished".to_owned())
                    .on_error(|_me, app: &mut App, _cx, source| app.animations.gif_status = format!("error: {source} did not load")),
                SkiaGif::new(GIF)
                    .width_request(70)
                    .height_request(140)
                    .repeat(-1)
                    .speed_ratio(2)
                    .aspect(TransformAspect::AspectCover)
                    .background_color(hex(0x212529)),
                SkiaStack::new().spacing(8).vertical_options(LayoutOptions::Center).fill_x().children((
                    SkiaWrap::new().spacing(8).children((
                        button("Start", 0x0D6EFD, |app, cx| cx.start_frames(app.animations.gif)),
                        button("Stop", 0x6C757D, |app, cx| cx.stop_frames(app.animations.gif)),
                        button("Seek(-1)", 0x6C757D, |app, cx| {
                            cx.stop_frames(app.animations.gif);
                            cx.seek_frames(app.animations.gif, -1.0);
                        }),
                    )),
                    note("Every frame is decoded once, off the frame thread; the player runs over 0..DurationMs and picks the frame by time, like C# GifAnimation."),
                )),
            )),
        ),
    )))
}

fn note(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(hex(0xADB5BD)).fill_x()
}

/// A 90 x 90 animation that loops.
fn small(source: &str) -> Build<SkiaLottie> {
    SkiaLottie::new(source).width_request(90).height_request(90).repeat(-1)
}

fn lottie_button(caption: &str, color: u32, action: fn(&mut Mut<'_, SkiaLottie>)) -> Build<SkiaButton> {
    button(caption, color, move |app, cx| {
        if let Some(mut lottie) = cx.get_mut(app.animations.lottie) {
            action(&mut lottie);
        }
    })
}

fn speed_button(speed: f32) -> Build<SkiaButton> {
    SkiaButton::new(format!("{speed}x"))
        .font_size(13)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.animations.speed == speed { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, _cx| app.animations.speed = speed)
}

fn button(caption: &str, color: u32, action: impl Fn(&mut App, &mut Cx) + 'static) -> Build<SkiaButton> {
    SkiaButton::new(caption).background_color(hex(color)).font_size(13).on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}
