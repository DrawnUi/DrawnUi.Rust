//! Sound with drawnui-audio: a tone the app pushes as a stream (the way an emulator feeds its sound
//! chip, paced by how much is queued) and a chime a generator makes on the audio side, mixed with
//! their own volumes and a master volume. The same code runs on the desktop, on phones and in the
//! browser (there the sound starts with the first click, tap or key).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::f32::consts::TAU;

use drawnui::prelude::*;
use drawnui_audio::{Audio, AudioGenerator, AudioStream};

const PAGE: Color = Color::from_rgb(18, 18, 24);
const CARD: Color = Color::from_rgb(30, 31, 42);
const INK: Color = Color::from_rgb(236, 238, 245);
const MUTED: Color = Color::from_rgb(150, 156, 176);
const ACCENT: Color = Color::from_rgb(245, 184, 61);
const TITLE: &str = "FontTitle";
/// How much sound the tone keeps queued: the app tops it up every frame.
const TARGET_MS: f32 = 60.0;

struct App {
    audio: Option<Audio>,
    error: String,
    tone: Option<AudioStream>,
    chime: Option<AudioGenerator>,
    tone_on: bool,
    pitch: f32,
    phase: f32,
    queued_ms: f32,
}

impl App {
    fn new() -> App {
        let (audio, error) = match Audio::open() {
            Ok(audio) => (Some(audio), String::new()),
            Err(e) => (None, e.to_string()),
        };
        let tone = audio.as_ref().map(|a| a.stream(1, 120));
        let chime = audio.as_ref().map(|a| chime(a.sample_rate() as f32, a));
        if let Some(c) = &chime {
            c.set_volume(0.0);
        }
        App { audio, error, tone, chime, tone_on: true, pitch: 330.0, phase: 0.0, queued_ms: 0.0 }
    }

    fn status(&self) -> String {
        match &self.audio {
            None => format!("No sound output: {}", self.error),
            Some(a) if !a.is_running() => format!("{} Hz, {} channels · click, tap or press a key to start the sound", a.sample_rate(), a.channels()),
            Some(a) => format!("{} Hz, {} channels · playing", a.sample_rate(), a.channels()),
        }
    }
}

/// A bell struck twice a second: a decaying sine and its fifth, made on the audio side.
fn chime(rate: f32, audio: &Audio) -> AudioGenerator {
    let (mut t, period) = (0.0f32, 0.5f32);
    audio.generator(1, move |buffer| {
        for s in buffer.iter_mut() {
            let since = t % period;
            let strike = (t / period) as u32 % 4;
            let note = [880.0, 660.0, 990.0, 740.0][strike as usize];
            let envelope = (-since * 9.0).exp();
            *s = envelope * 0.25 * ((since * note * TAU).sin() + 0.4 * (since * note * 1.5 * TAU).sin());
            t += 1.0 / rate;
        }
    })
}

/// Every frame: top the tone's stream up to `TARGET_MS`, as an emulator feeds its sound chip.
fn feed(app: &mut App) {
    let (Some(audio), Some(tone)) = (&app.audio, &app.tone) else { return };
    let rate = audio.sample_rate() as f32;
    let mut chunk = [0.0f32; 256];
    while tone.queued_ms() < TARGET_MS {
        for s in chunk.iter_mut() {
            *s = if app.tone_on { (app.phase * TAU).sin() * 0.2 } else { 0.0 };
            app.phase = (app.phase + app.pitch / rate) % 1.0;
        }
        if tone.push_slice(&chunk) < chunk.len() {
            break;
        }
    }
    app.queued_ms = tone.queued_ms();
}

fn card(title: &str, text: &str, rows: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new().corner_radius(16).background_color(CARD).fill_x().padding(20).children((SkiaLayout::column().fill_x().spacing(14).children((
        SkiaLabel::new(title).font_size(20).font_family(TITLE).text_color(INK).accessibility_role(Aria::HEADING),
        SkiaLabel::new(text).font_size(14).text_color(MUTED),
        SkiaLayout::column().fill_x().spacing(12).children(rows),
    )),))
}

fn row(label: &str, control: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaLayout::row().fill_x().spacing(16).children((SkiaLabel::new(label).font_size(15).text_color(INK).width_request(90).vertical_options(LayoutOptions::Center), control))
}

fn build(_app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::new().fill().children((
        DrawnGame::new().width_request(1).height_request(1).start_loop(0.0).on_game_loop(|_me, app: &mut App, _cx, _delta| feed(app)),
        SkiaScroll::new().fill().content(
            SkiaLayout::column().fill_x().maximum_width_request(640).horizontal_options(LayoutOptions::Center).padding(24).spacing(20).children((
                SkiaLabel::new("DrawnUI Audio").font_size(30).font_family(TITLE).text_color(INK).accessibility_role(Aria::HEADING),
                SkiaLabel::new("").font_size(14).text_color(MUTED).observe(|me, app: &App| me.set_text(app.status())),
                card(
                    "Tone",
                    "A stream the app fills every frame, kept 60 ms ahead of the speaker.",
                    (
                        row("Play", SkiaSwitch::new().is_toggled(true).accessibility_label("Tone").on_toggled(|_me, app: &mut App, _cx, on| app.tone_on = on)),
                        row(
                            "Pitch",
                            SkiaSlider::new().fill_x().min(110).max(880).end(330).accessibility_label("Tone pitch").on_end_changed(|_me, app: &mut App, _cx, v| app.pitch = v),
                        ),
                        row(
                            "Volume",
                            SkiaSlider::new().fill_x().end(100).accessibility_label("Tone volume").on_end_changed(|_me, app: &mut App, _cx, v| {
                                if let Some(t) = &app.tone {
                                    t.set_volume(v / 100.0);
                                }
                            }),
                        ),
                        SkiaLabel::new("").font_size(13).text_color(MUTED).observe(|me, app: &App| me.set_text(format!("Queued: {:.0} ms", app.queued_ms))),
                    ),
                ),
                card(
                    "Chime",
                    "A generator: the sound is made on the audio side, the app only sets its volume.",
                    (
                        row(
                            "Play",
                            SkiaSwitch::new().accessibility_label("Chime").on_toggled(|_me, app: &mut App, _cx, on| {
                                if let Some(c) = &app.chime {
                                    c.set_volume(if on { 1.0 } else { 0.0 });
                                }
                            }),
                        ),
                    ),
                ),
                card(
                    "Master",
                    "The volume of everything that plays.",
                    (row(
                        "Volume",
                        SkiaSlider::new().fill_x().end(80).accessibility_label("Master volume").on_end_changed(|_me, app: &mut App, _cx, v| {
                            if let Some(a) = &app.audio {
                                a.set_volume(v / 100.0);
                            }
                        }),
                    ),),
                ),
                SkiaButton::new("Stop and clear the tone")
                    .background_color(ACCENT)
                    .text_color(Color::from_rgb(28, 22, 8))
                    .on_tapped(|_me, app: &mut App, _cx| {
                        app.tone_on = false;
                        if let Some(t) = &app.tone {
                            t.clear();
                        }
                    }),
            )),
        ),
    ))
}

fn main() {
    drawnui::run("DrawnUI Audio", || {
        let mut app = App::new();
        if let Some(a) = &app.audio {
            a.set_volume(0.8);
        }
        app.tone_on = true;
        Box::new(
            Ui::new(app, build)
                .font("Default", "assets/OpenSans-Regular.ttf")
                .font(TITLE, "assets/OpenSans-Semibold.ttf")
                .default_accessibility_role::<SkiaLabel>(Aria::TEXT)
                .default_accessibility_role::<SkiaButton>(Aria::BUTTON)
                .background(PAGE)
                // Silent while the app is in the background (phones, a hidden browser tab).
                .on_visibility_changed(|app: &mut App, visible, _cx| {
                    if let Some(a) = &app.audio {
                        a.set_paused(!visible);
                    }
                }),
        )
    });
}
