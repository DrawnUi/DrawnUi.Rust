//! Dungeon Run: a first-person runner drawn with DrawnUI for Rust. The dungeon is one SkMesh a
//! frame, lit by its SkSL fragment program (`scene`), under an SkSL post effect. Everything else
//! is ordinary DrawnUI over it: a SkiaShell whose popups are the dialogs, SkiaButtons, shapes and
//! labels for the HUD and the title, in the look of the HUD (dark fills, amber lines and glow). The
//! panel of a dialog is a shader too: a moving field in a frame whose lights run around it.
//! One source for the desktop window and the browser.

use drawnui::prelude::*;

mod plate;
mod scene;
mod shaders;

use plate::Plate;
use scene::{HEALTH, Hint, Input, Phase, Scene};

/// The amber of every line, caption and frame of the interface.
const ACCENT: u32 = 0xFFFF_B060;
/// What hurts.
const HOT: u32 = 0xFFFF_3B6B;
const GOOD: u32 = 0xFF6C_FF5A;
/// The arrows of the help come from this fallback font.
const SYMBOLS: &str = "FontSymbols";

#[derive(Default)]
pub struct App {
    shell: Handle<SkiaShell>,
    scene: Handle<Scene>,
    title: Handle<SkiaLayout>,
    score: Handle<SkiaLabel>,
    hud: Handle<SkiaLayout>,
    /// The frame rate, top left, on the title only (the distance is there during a run).
    fps: Handle<SkiaLabelFps>,
    /// Help and pause, top right.
    buttons: Handle<SkiaLayout>,
    /// The prompt (JUMP, CHANGE LANE, a power-up's name): its glow layer, its label, its jump in
    /// and the seconds it has been pulsing.
    prompt_box: Handle<SkiaLayout>,
    prompt: Handle<SkiaLabel>,
    prompt_pop: f32,
    prompt_time: f32,
    /// The light dim and blur under a dialog.
    veil: Handle<SkiaBackdrop>,
    /// The shader panel of the open dialog, and what keeps its time running.
    dialog_panel: Handle<SkiaLayout>,
    dialog_time: Option<AnimationId>,
    /// The ten cells of the health, one per tenth.
    cells: [Handle<SkiaShape>; 10],
    score_box: Handle<SkiaLayout>,
    shown_distance: u32,
    /// 1 when points were just won, fading: the score jumps.
    pop: f32,
    input: Input,
    /// The lane key that is held (-1 left, 1 right) and the seconds to its next step.
    held: i32,
    hold_wait: f32,
    /// The press already steered: its release is not a tap.
    swiped: bool,
    /// Lanes the drag in progress has stepped (right positive).
    pan_steps: i32,
    /// A dialog is open: the world stands still.
    paused: bool,
    /// The open dialog is the help (closing it during a run brings the pause back).
    help_open: bool,
    /// What the labels show.
    phase: Phase,
    shown_score: u32,
    shown_health: f32,
    /// The canvas is too narrow for one row of HUD.
    narrow: bool,
    /// Points: how high a dialog may be.
    canvas_height: f32,
    canvas_width: f32,
    /// The game's name on the title: smaller on a narrow screen.
    name: Handle<SkiaLabel>,
    hint: Hint,
    /// The big label in the middle: what it shows (3, 2, 1, 0 = GO, -2 = GAME OVER, -1 = nothing), its jump, the seconds GO stays.
    count: Handle<SkiaLayout>,
    count_label: Handle<SkiaLabel>,
    shown_count: i32,
    count_pop: f32,
    go: f32,
    /// Over everything after GAME OVER: its frozen picture, burning away.
    curtain: Handle<SkiaLayout>,
    curtain_on: bool,
    shown_burn: f32,
    /// The health flash, the surge, the color of its rays, the orb pulse and the ghost's flash
    /// the post effect was last given.
    shown_fx2: [f32; 7],
    /// The mark of a row of orbs (x2, x3) by the score, and the seconds it still shows.
    streak: Handle<SkiaLabel>,
    streak_time: f32,
    /// Seconds the prompt still shows a power-up's name.
    notice: f32,
    /// How often the jump and the lane prompts were shown; three times teach it.
    taught: [u8; 2],
    /// The ghost has the run: no prompt over it.
    ghosted: bool,
    /// Seconds run without a hit, and how many times that was praised.
    clean: f32,
    cheers: u32,
}

/// Said after every stretch of running without a hit.
const CHEERS: [&str; 8] = ["WELL DONE", "NICE RUN", "YOU ARE KILLING IT", "UNSTOPPABLE", "ON FIRE", "FLAWLESS", "KEEP GOING", "SMOOTH"];
/// Seconds of clean running between two cheers.
const CHEER_EVERY: f32 = 12.0;

/// Points of sideways drag per lane.
const PAN_STEP: f32 = 44.0;
/// A phone or a tablet: no keyboard, so the texts speak of gestures only.
const MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));
/// A drag that went this far (points) was a drag, not a tap, whatever its release says: the
/// engine's own tap threshold (TAPPED_CANCEL_MOVE_THRESHOLD_POINTS).
const TAP_SLOP: f32 = 16.0;

/// A held lane key steps again after this long, then at this pace (the system's own key repeat
/// waits half a second: too slow for a run).
const HOLD_FIRST: f32 = 0.2;
const HOLD_NEXT: f32 = 0.13;

fn post_effect() -> SkiaShaderEffect {
    SkiaShaderEffect::new()
        .shader_code(shaders::POST)
        .uniform("uFx2", &[0.0, 0.0, 0.0, 0.0])
        .uniform("uRay", &[1.0, 0.55, 0.22])
        .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: post SkSL: {error}"))
}

/// A color (0xAARRGGBB) as the red, green and blue a shader's tint takes.
fn tint(color: u32) -> [f32; 3] {
    [(color >> 16 & 0xFF) as f32 / 255.0, (color >> 8 & 0xFF) as f32 / 255.0, (color & 0xFF) as f32 / 255.0]
}

/// A lane change or a jump; on the title, a jump (a tap, Space, Enter) starts the run, a drag or
/// a lane key does not.
fn act(app: &mut App, cx: &mut Cx, left: bool, right: bool, jump: bool) {
    if app.paused {
        return;
    }
    match app.phase {
        Phase::Attract if jump => {
            if let Some(mut scene) = cx.get_mut(app.scene) {
                scene.control_mut().world.start();
            }
        }
        Phase::Playing => {
            app.input.left |= left;
            app.input.right |= right;
            app.input.jump |= jump;
        }
        // GAME OVER need not be waited out.
        Phase::Over => {
            if let Some(mut scene) = cx.get_mut(app.scene) {
                scene.control_mut().world.to_title();
            }
        }
        _ => {}
    }
}

fn open_dialog(app: &mut App, cx: &mut Cx, dialog: fn(&mut App) -> Build<SkiaLayout>) {
    let content = dialog(app);
    cx.open_popup(app.shell, content, PopupOptions { show_overlay: false, ..PopupOptions::default() });
    if let Some(running) = app.dialog_time.take() {
        cx.stop_animation(running);
    }
    app.dialog_time = Some(cx.animate_shaders(app.dialog_panel));
}

/// Escape, P or the pause button: a run pauses (home or resume); the title, which has nothing to
/// pause, shows how to play; an open dialog closes.
fn menu(app: &mut App, cx: &mut Cx) {
    match app.phase {
        _ if app.paused => close_dialog(app, cx),
        Phase::Playing | Phase::Countdown => open_dialog(app, cx, pause_dialog),
        Phase::Attract => open_dialog(app, cx, help_dialog),
        _ => {}
    }
}

/// Opens a dialog, or closes the one that is open.
fn toggle_dialog(app: &mut App, cx: &mut Cx, dialog: fn(&mut App) -> Build<SkiaLayout>) {
    if app.paused {
        cx.close_popup(app.shell, true);
    } else {
        open_dialog(app, cx, dialog);
    }
}

fn key_down(app: &mut App, cx: &mut Cx, event: &KeyEvent) -> bool {
    if event.repeat {
        return true;
    }
    // A button a keyboard user tabbed to takes Enter and Space itself.
    if matches!(event.key, "Enter" | "NumpadEnter" | "Space") && cx.accessibility_focused().is_some() {
        return false;
    }
    match event.key {
        // In a dialog, Enter and Space press its main button.
        "Enter" | "NumpadEnter" | "Space" if app.paused => close_dialog(app, cx),
        "F1" => toggle_dialog(app, cx, help_dialog),
        "Escape" | "KeyP" => menu(app, cx),
        // Android's Back during a run pauses it; otherwise the shell closes the dialog, and at the
        // title the app goes to the background as Android apps do.
        "BrowserBack" if !app.paused && matches!(app.phase, Phase::Playing | Phase::Countdown) => menu(app, cx),
        "ArrowLeft" | "KeyA" => {
            (app.held, app.hold_wait) = (-1, HOLD_FIRST);
            act(app, cx, true, false, false)
        }
        "ArrowRight" | "KeyD" => {
            (app.held, app.hold_wait) = (1, HOLD_FIRST);
            act(app, cx, false, true, false)
        }
        "ArrowUp" | "KeyW" | "Space" | "Enter" => act(app, cx, false, false, true),
        _ => return false,
    }
    true
}

/// The held lane key was released (the other one may have taken over since).
fn key_up(app: &mut App, event: &KeyEvent) -> bool {
    match event.key {
        "ArrowLeft" | "KeyA" if app.held < 0 => app.held = 0,
        "ArrowRight" | "KeyD" if app.held > 0 => app.held = 0,
        _ => return false,
    }
    true
}

/// The pointer, a finger or the mouse alike: a drag steers, a tap jumps.
fn gesture(app: &mut App, cx: &mut Cx, gesture: &Gesture, scale: f32) -> bool {
    match gesture.kind {
        GestureKind::Down => (app.swiped, app.pan_steps) = (false, 0),
        // A drag steers, with a finger or the mouse: a lane for every `PAN_STEP` points the
        // pointer has gone sideways since the press, there and back.
        GestureKind::Panning => {
            // A finger that went past the tap threshold and came back ends within it: the engine
            // reports a Tapped then, but the player dragged, so no jump on this release.
            let (dx, dy) = (gesture.total.x / scale, gesture.total.y / scale);
            if dx.abs() >= TAP_SLOP || dy.abs() >= TAP_SLOP {
                app.swiped = true;
            }
            let steps = (dx / PAN_STEP).trunc() as i32;
            if steps != app.pan_steps {
                let right = steps > app.pan_steps;
                app.pan_steps += if right { 1 } else { -1 };
                app.swiped = true;
                act(app, cx, !right, right, false);
            }
        }
        // A tap or a click that steered nothing is a jump, wherever it lands.
        GestureKind::Tapped if !app.swiped => act(app, cx, false, false, true),
        _ => {}
    }
    true
}

/// The score jumps in a violet glow for a moment: the glow shader is on it only that long.
fn flash_score(app: &mut App, cx: &mut Cx) {
    app.pop = 1.0;
    if let Some(mut score) = cx.get_mut(app.score_box)
        && score.effect::<SkiaShaderEffect>().is_none()
    {
        score.add_visual_effect(
            SkiaShaderEffect::new()
                .shader_code(shaders::GLOW)
                .uniform("uGlow", &[1.0])
                .uniform("uWash", &[0.7])
                .uniform("uTint", &[0.62, 0.22, 1.0])
                .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: glow SkSL: {error}")),
        );
    }
}

/// One frame: the world moves, the scene draws again, the HUD follows.
fn tick(app: &mut App, cx: &mut Cx, delta: f32) {
    // On a narrow screen (a phone held upright) the health does not fit between the score and the
    // buttons: it goes under them.
    let canvas = cx.canvas_size();
    app.canvas_height = canvas.height;
    if canvas.width != app.canvas_width {
        app.canvas_width = canvas.width;
        // The big texts fit the width they have.
        if let Some(mut name) = cx.get_mut(app.name) {
            name.set_font_size(if canvas.width < 440.0 { 32.0 } else { 44.0 });
        }
        if let Some(mut count) = cx.get_mut(app.count) {
            count.set_width_request(canvas.width.min(760.0));
        }
    }
    let narrow = canvas.width < NARROW;
    if narrow != app.narrow {
        app.narrow = narrow;
        if let Some(mut hud) = cx.get_mut(app.hud) {
            hud.set_margin((0, if narrow { 62 } else { 16 }, 0, 0));
        }
    }
    if app.paused {
        return;
    }
    if app.held != 0 {
        app.hold_wait -= delta;
        if app.hold_wait <= 0.0 {
            app.hold_wait = HOLD_NEXT;
            (app.input.left, app.input.right) = (app.held < 0, app.held > 0);
        }
    }
    let Some(mut scene) = cx.get_mut(app.scene) else { return };
    let world = &mut scene.control_mut().world;
    // A frame that came late (a hidden tab) is not a jump through the walls.
    world.step(delta.min(0.05), &mut app.input);
    let (phase, orbs, health, hint, young) = (world.phase, world.orbs(), world.health, world.hint(), world.young());
    let world_killer = world.killer;
    let meters = world.distance() / 10 * 10;
    let count = world.count();
    let (burn, closing) = (world.burn, world.closing());
    let pickup = world.take_pickup();
    let fx = [world.rush(), world.warp, world.flash, world.time];
    let light = world.light();
    // The ghost's blackout, plus 2 while its touch is felt (the post effect's haunt).
    let haunt = if world.ghost > 0.0 || world.killer.is_some() { 2.0 } else { 0.0 };
    let fx2 = [world.heal, world.surge_glow, light[0], light[1], light[2], world.orb_pulse, world.ghost + haunt];
    let streak = world.streak;
    if let Some(effect) = scene.effect_mut::<SkiaShaderEffect>() {
        effect.set_uniform("uFx", &fx);
        if fx2 != app.shown_fx2 {
            app.shown_fx2 = fx2;
            effect.set_uniform("uFx2", &[fx2[0], fx2[1], fx2[5], fx2[6]]);
            effect.set_uniform("uRay", &fx2[2..5]);
        }
    }
    scene.mark(Dirty::DRAW);

    if phase != app.phase {
        if app.phase == Phase::Countdown && phase == Phase::Playing {
            app.go = 0.7;
        }
        if phase == Phase::Countdown {
            // A new run: the readouts start over.
            (app.shown_score, app.shown_distance) = (0, 0);
            app.clean = 0.0;
            if let Some(mut label) = cx.get_mut(app.score) {
                label.set_text("0 M");
            }
        }
        app.phase = phase;
        let on_title = phase == Phase::Attract;
        if let Some(mut title) = cx.get_mut(app.title) {
            title.set_is_visible(on_title);
        }
        if let Some(mut fps) = cx.get_mut(app.fps) {
            fps.set_opacity(if on_title { 1.0 } else { 0.0 });
        }
        let opacity = if on_title { 0.0 } else { 1.0 };
        if let Some(mut hud) = cx.get_mut(app.hud) {
            hud.set_opacity(opacity);
        }
        if let Some(mut score) = cx.get_mut(app.score_box) {
            score.set_opacity(opacity);
        }
    }
    // The curtain: shown a moment before GAME OVER ends, it keeps that picture (its shader takes
    // its texture once), then burns it away over the title's run.
    let curtain_on = closing || burn > 0.0;
    let hole = if closing { 0.0 } else { 1.0 - burn };
    if let Some(mut curtain) = cx.get_mut(app.curtain) {
        if curtain_on != app.curtain_on {
            app.curtain_on = curtain_on;
            curtain.set_is_visible(curtain_on);
            if !curtain_on && let Some(effect) = curtain.effect_mut::<SkiaShaderEffect>() {
                effect.release_frozen_snapshot();
            }
        }
        if curtain_on && hole != app.shown_burn {
            app.shown_burn = hole;
            if let Some(effect) = curtain.effect_mut::<SkiaShaderEffect>() {
                effect.set_uniform("uBurn", &[hole]);
            }
        }
    }
    // The countdown: each number jumps in and settles; GO stays a moment into the run.
    app.go = (app.go - delta).max(0.0);
    let show = match count {
        Some(n) => n as i32,
        None if phase == Phase::Over => -2,
        None if app.go > 0.0 => 0,
        None => -1,
    };
    if show != app.shown_count {
        app.shown_count = show;
        app.count_pop = 1.0;
        if show != -1 && let Some(mut label) = cx.get_mut(app.count_label) {
            label.set_font_size(if show != -2 { 120.0 } else if app.canvas_width < 520.0 { 38.0 } else { 64.0 });
            label.set_text(match show {
                0 => "GO".to_owned(),
                -2 => "GAME OVER".to_owned(),
                n => n.to_string(),
            });
        }
        if let Some(mut layer) = cx.get_mut(app.count) {
            layer.set_is_visible(show != -1);
            layer.set_opacity(1.0);
        }
    }
    if app.count_pop > 0.0 {
        app.count_pop = (app.count_pop - delta * 2.4).max(0.0);
        if let Some(mut layer) = cx.get_mut(app.count) {
            let scale = 1.0 + 0.7 * app.count_pop * app.count_pop;
            layer.set_scale_x(scale);
            layer.set_scale_y(scale);
        }
    }
    if health != app.shown_health {
        if health < app.shown_health {
            app.clean = 0.0;
        }
        app.shown_health = health;
        // Green, amber, red; a cell per tenth, lit from the left.
        let color = Color::new(if health > 0.6 { GOOD } else if health > 0.3 { 0xFFFF_B030 } else { 0xFFFF_3B30 });
        let lit = (health * 10.0).round() as usize;
        if let Some(mut hud) = cx.get_mut(app.hud) {
            hud.set_accessibility_label(format!("Health {} percent", lit * 10));
        }
        for (i, cell) in app.cells.into_iter().enumerate() {
            if let Some(mut cell) = cx.get_mut(cell) {
                let on = i < lit;
                cell.set_background_color(if on { color } else { Color::new(CELL_OFF) });
                cell.set_shadows(if on { vec![SkiaShadow::new(color).x(0).y(0).blur(7).opacity(0.9)] } else { Vec::new() });
            }
        }
    }
    // An orb taken: the distance jumps in a violet flash.
    if phase == Phase::Playing && orbs != app.shown_score {
        let won = orbs > app.shown_score;
        app.shown_score = orbs;
        if won {
            flash_score(app, cx);
            // The second and third orb of a row say so.
            if streak >= 2 {
                app.streak_time = 0.9;
                if let Some(mut label) = cx.get_mut(app.streak) {
                    label.set_text(format!("x{streak}"));
                    label.set_opacity(1.0);
                }
            }
        }
    }
    if pickup.is_some() && phase == Phase::Playing {
        flash_score(app, cx);
    }
    if phase == Phase::Playing && meters != app.shown_distance {
        app.shown_distance = meters;
        if let Some(mut label) = cx.get_mut(app.score) {
            label.set_text(format!("{meters} M"));
            label.set_accessibility_label(format!("Distance {meters} meters"));
        }
    }
    if app.streak_time > 0.0 {
        app.streak_time = (app.streak_time - delta).max(0.0);
        if let Some(mut label) = cx.get_mut(app.streak) {
            label.set_opacity((app.streak_time / 0.3).min(1.0));
        }
    }
    if app.pop > 0.0 {
        app.pop = (app.pop - delta * 3.0).max(0.0);
        let scale = 1.0 + 0.35 * app.pop * app.pop;
        if let Some(mut label) = cx.get_mut(app.score) {
            label.set_scale_x(scale);
            label.set_scale_y(scale);
        }
        if let Some(mut score) = cx.get_mut(app.score_box) {
            // The flash is over: no shader left over the score.
            if app.pop <= 0.0 {
                score.clear_visual_effects();
            } else if let Some(effect) = score.effect_mut::<SkiaShaderEffect>() {
                effect.set_uniform("uGlow", &[app.pop]);
            }
        }
    }
    if phase == Phase::Playing {
        app.clean += delta;
    }
    // The ghost's kill takes the prompt away; a power-up says its name for a moment; the lessons wait.
    if world_killer.is_some() != app.ghosted {
        app.ghosted = world_killer.is_some();
        if app.ghosted {
            (app.notice, app.hint) = (0.0, Hint::None);
            show_prompt(app, cx, None);
        }
    }
    if let Some(power) = pickup.filter(|_| phase == Phase::Playing) {
        (app.notice, app.hint) = (1.3, Hint::None);
        show_prompt(app, cx, Some(if power == HEALTH { Prompt::Health } else { Prompt::Surge }));
    } else if app.notice > 0.0 {
        app.notice -= delta;
        if app.notice <= 0.0 {
            show_prompt(app, cx, None);
        }
    } else if phase == Phase::Playing && app.clean >= CHEER_EVERY && hint == Hint::None {
        // A stretch without a hit: a word of praise, in the prompt's slot while no lesson needs it.
        app.clean = 0.0;
        app.cheers += 1;
        app.notice = 1.3;
        show_prompt(app, cx, Some(Prompt::Cheer(CHEERS[(app.cheers.wrapping_mul(7).wrapping_add(meters)) as usize % CHEERS.len()])));
    } else if hint != app.hint {
        app.hint = hint;
        let (prompt, lesson) = match hint {
            Hint::Jump => (Prompt::Jump, Some(0)),
            Hint::Lane => (Prompt::Lane, Some(1)),
            Hint::None => (Prompt::Jump, None),
        };
        // Every hazard of a young run is announced; later the first three of each kind teach it.
        let show = lesson.is_some_and(|i| {
            app.taught[i] += u8::from(app.taught[i] < 4);
            young || app.taught[i] <= 3
        });
        show_prompt(app, cx, show.then_some(prompt));
    }
    // The prompt jumps in like the countdown, then its glow beats.
    if app.prompt_pop > 0.0 || app.prompt_time > 0.0 {
        app.prompt_pop = (app.prompt_pop - delta * 2.4).max(0.0);
        app.prompt_time += delta;
        if let Some(mut layer) = cx.get_mut(app.prompt_box) {
            let scale = 1.0 + 0.7 * app.prompt_pop * app.prompt_pop;
            layer.set_scale_x(scale);
            layer.set_scale_y(scale);
            if let Some(effect) = layer.effect_mut::<SkiaShaderEffect>() {
                effect.set_uniform("uGlow", &[0.55 + 0.45 * app.prompt_pop + 0.3 * (app.prompt_time * 9.0).sin()]);
            }
        }
    }
}

/// What the big prompt says, in its own color: a lesson or a power-up's name.
#[derive(Clone, Copy)]
enum Prompt {
    Jump,
    Lane,
    Health,
    Surge,
    Cheer(&'static str),
}

/// Shows the prompt (text, gradient and glow of its kind, jumping in) or hides it.
fn show_prompt(app: &mut App, cx: &mut Cx, prompt: Option<Prompt>) {
    if let Some(prompt) = prompt {
        let (text, colors, tint) = match prompt {
            Prompt::Jump => ("JUMP", [0xFFF4_E8FF, 0xFFB0_70FF, 0xFF70_20E0], [0.62, 0.22, 1.0]),
            Prompt::Lane => ("CHANGE LANE", [0xFFE8_FFFF, 0xFF40_E0FF, 0xFF10_90E0], [0.15, 0.85, 1.0]),
            // Health shows during its own green flash: white letters with a green glow stay readable in it.
            Prompt::Health => ("HEALTH UP", [0xFFFF_FFFF, 0xFFF0_FFE8, 0xFFA0_E890], [0.42, 1.0, 0.35]),
            Prompt::Surge => ("SURGE", [0xFFFF_FFFF, 0xFF90_F0FF, 0xFF30_B0FF], [0.5, 0.9, 1.0]),
            Prompt::Cheer(text) => (text, [0xFFFF_F4D0, 0xFFFF_C050, 0xFFD0_6810], tint(ACCENT)),
        };
        if let Some(mut label) = cx.get_mut(app.prompt) {
            label.set_text(text);
            label.set_fill_gradient(fire(colors));
        }
        (app.prompt_pop, app.prompt_time) = (1.0, 0.001);
        if let Some(mut layer) = cx.get_mut(app.prompt_box) {
            layer.set_is_visible(true);
            if let Some(effect) = layer.effect_mut::<SkiaShaderEffect>() {
                effect.set_uniform("uTint", &tint);
            }
        }
    } else {
        (app.prompt_pop, app.prompt_time) = (0.0, 0.0);
        if let Some(mut layer) = cx.get_mut(app.prompt_box) {
            layer.set_is_visible(false);
        }
    }
}

/// The gradient of the heavy letters (the countdown, GAME OVER, the title, the prompts): light at
/// the top, deep at the bottom.
fn fire(colors: [u32; 3]) -> SkiaGradient {
    SkiaGradient::new(GradientType::Linear, colors.map(Color::new)).angle(0.0)
}

fn line(text: &str, size: i32, color: u32) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(size).text_color(Color::new(color)).font_family_fallback(SYMBOLS)
}

/// What every button is made of: a dark plate whose thin line is white at rest and amber, with a
/// glow, under the pointer or a press (`Plate`).
fn plate() -> Build<Plate> {
    Plate::new(Color::new(0x66FF_FFFF), Color::new(ACCENT))
        .corner_radius(2)
        .background_color(Color::new(0xE608_0B14))
        .stroke_width(1.55)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        // A Tab stop a screen reader calls a button; Enter and Space press it.
        .accessibility_role(Aria::BUTTON)
}

/// A button's caption: the button itself carries the name a screen reader says.
fn caption(text: &str, size: i32) -> Build<SkiaLabel> {
    line(text, size, 0xE6FF_FFFF).accessibility_role(Aria::PRESENTATION)
}

/// The main action: the plate with a lit LED at its left edge.
fn button(text: &str) -> Build<Plate> {
    plate().height_request(40).accessibility_label(text).children((
        SkiaShape::new()
            .width_request(3)
            .height_request(18)
            .margin((10, 0, 0, 0))
            .vertical_options(LayoutOptions::Center)
            .background_color(Color::new(0xFFFF_E6C0))
            .shadows(SkiaShadow::new(Color::new(ACCENT)).x(0).y(0).blur(7).opacity(1)),
        caption(text, 14).center().margin((26, 0, 20, 0)),
    ))
}

/// Any other action.
fn ghost(text: &str) -> Build<Plate> {
    plate().height_request(40).accessibility_label(text).children(caption(text, 14).center().margin((20, 0, 20, 0)))
}

/// A square button of the HUD.
fn key(text: &str, name: &str) -> Build<Plate> {
    plate().width_request(40).height_request(40).accessibility_label(name).children(caption(text, 15).center())
}

/// Canvas widths under this (points) have no room for the score, the health and the buttons in one row.
const NARROW: f32 = 620.0;

/// A cell of the health that is out.
const CELL_OFF: u32 = 0x8010_1420;

/// Points around a dialog's panel that its glow needs (the shader counts with the same numbers).
const DIALOG_GLOW: i32 = 26;
const DIALOG_WIDTH: f32 = 452.0;

/// A dialog: its panel, frame and glow are one animated shader; the content lies over it.
fn dialog(app: &mut App, title: &str, content: impl IntoChildren) -> Build<SkiaLayout> {
    // As wide as it likes on a desktop, as wide as the screen lets it on a phone.
    let width = DIALOG_WIDTH.min(app.canvas_width - 4.0).max(280.0);
    SkiaLayer::new().width_request(width).horizontal_options(LayoutOptions::Center).accessibility_role(Aria::DIALOG).accessibility_label(title).children((
        SkiaLayer::new().fill().assign(&mut app.dialog_panel).visual_effect(
            SkiaShaderEffect::new()
                .shader_code(shaders::DIALOG)
                .uniform("uWidth", &[width])
                .use_background(UseBackground::Never)
                .auto_create_input_texture(false)
                .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: dialog SkSL: {error}")),
        ),
        SkiaStack::new().spacing(9).margin(DIALOG_GLOW).padding(if app.narrow { 14 } else { 24 }).children((
            // A lit LED bar.
            SkiaShape::new()
                .width_request(44)
                .height_request(3)
                .corner_radius(1.5)
                .background_color(Color::new(0xFFFF_E6C0))
                .shadows(SkiaShadow::new(Color::new(ACCENT)).x(0).y(0).blur(9).opacity(1))
                .margin((0, 2, 0, 4)),
            line(title, 22, ACCENT).margin((0, 0, 0, 4)),
            content,
        )),
    ))
}

/// A heading inside a dialog.
fn section(text: &str, color: u32) -> Build<SkiaLabel> {
    line(text, 12, color).margin((0, 10, 0, 0))
}

/// One thing of the dungeon: how it looks, its name, what to do about it. On a narrow screen
/// the name and the action do not share a line (GREEN CROSS / RESTORES HEALTH overlapped).
fn thing(narrow: bool, swatch: Build<SkiaShape>, name: &str, action: &str, color: u32) -> Build<SkiaLayout> {
    let action = line(action, 13, color).horizontal_options(LayoutOptions::End);
    let row = SkiaLayer::new().height_request(22).children((
        SkiaLayer::new().width_request(26).fill_y().children(swatch.center()),
        line(name, 13, 0xFFFF_FFFF).margin((38, 0, 0, 0)).vertical_options(LayoutOptions::Center),
    ));
    if narrow {
        SkiaStack::new().spacing(0).children((row, action.margin((0, -4, 0, 0))))
    } else {
        row.children(action.vertical_options(LayoutOptions::Center))
    }
}

/// One control: what to press or do, and what it does.
fn pair(input: &str, action: &str) -> Build<SkiaLayout> {
    SkiaLayer::new().height_request(20).children((
        line(input, 13, ACCENT).vertical_options(LayoutOptions::Center),
        line(action, 13, 0xFFFF_FFFF).horizontal_options(LayoutOptions::End).vertical_options(LayoutOptions::Center),
    ))
}

fn swatch(width: i32, height: i32, color: u32, glow: bool) -> Build<SkiaShape> {
    let shape = SkiaShape::new().width_request(width).height_request(height).background_color(Color::new(color));
    if glow { shape.shadows(SkiaShadow::new(Color::new(color)).x(0).y(0).blur(8).opacity(1)) } else { shape }
}

/// What to do, what not to do, and only then the controls: keyboard, then touch and mouse. The
/// text scrolls when the screen is too low for it; the last line and the button always show.
fn help_dialog(app: &mut App) -> Build<SkiaLayout> {
    // What the dialog needs around the text: its glow, padding, title, last line and button.
    let room = (app.canvas_height - 300.0).max(110.0);
    let narrow = app.narrow;
    app.help_open = true;
    dialog(
        app,
        "HOW TO PLAY",
        (
            SkiaScroll::new().fill_x().maximum_height_request(room).content(SkiaStack::new().spacing(9).children((
                (
                    section("YOUR GOAL", GOOD).margin(0),
                    line("RUN AS FAR AS YOU CAN. THE RUN NEVER STOPS.", 11, 0xD9FF_FFFF),
                    line("COLLECT WHAT GLOWS ON YOUR WAY.", 11, 0xD9FF_FFFF),
                    thing(narrow, swatch(14, 14, 0xFFC0_60FF, true).shape_type(ShapeType::Circle), "ORB", "A LITTLE HEALTH", GOOD),
                    thing(narrow, swatch(14, 14, 0xFF4D_FF66, true).shape_type(ShapeType::Circle), "GREEN CROSS", "RESTORES HEALTH", GOOD),
                    thing(narrow, swatch(14, 14, 0xFF4D_E6FF, true).shape_type(ShapeType::Circle), "SURGE", "FAST, NOTHING HURTS", GOOD),
                ),
                (
                    section("DO NOT TOUCH", HOT),
                    line("THESE DRAIN YOUR HEALTH. ZERO ENDS THE RUN.", 11, 0xD9FF_FFFF),
                    thing(narrow, swatch(10, 18, 0xFF3A_3D4A, false).stroke_color(Color::new(0xFFFF_8A30)).stroke_width(1.55), "PILLAR", "MOVE TO A FREE LANE", HOT),
                    thing(narrow, swatch(22, 4, 0xFFFF_4030, true), "RED BEAM", "JUMP OVER IT", HOT),
                    thing(narrow, swatch(22, 9, 0xFFE8_3A0A, true), "LAVA", "JUMP OVER IT", HOT),
                    thing(narrow, swatch(14, 18, 0xFFC8_D8FF, true), "GHOST", "CHANGE LANE. IT HURTS MOST", HOT),
                ),
                controls(),
            ))),
            line("DRAWNUI FOR RUST SAMPLE", 11, ACCENT).margin((0, 12, 0, 0)),
            button("OK")
                .horizontal_options(LayoutOptions::End)
                .margin((0, 6, 0, 0))
                .on_tapped(|_me, app: &mut App, cx| close_dialog(app, cx)),
        ),
    )
}

/// The controls section of the help: a phone has no keys, so it hears only of gestures.
fn controls() -> Build<SkiaLayout> {
    if MOBILE {
        SkiaStack::new().spacing(9).children((
            section("TOUCH", ACCENT),
            pair("DRAG LEFT OR RIGHT", "CHANGE LANE"),
            pair("TAP", "JUMP"),
        ))
    } else {
        SkiaStack::new().spacing(9).children((
            section("KEYBOARD", ACCENT),
            pair("← →", "CHANGE LANE"),
            pair("↑ / SPACE", "JUMP"),
            section("TOUCH OR MOUSE", ACCENT),
            pair("DRAG LEFT OR RIGHT", "CHANGE LANE"),
            pair("TAP OR CLICK", "JUMP"),
        ))
    }
}

/// The open dialog closes; the help read during a run gives way to the pause again.
fn close_dialog(app: &mut App, cx: &mut Cx) {
    let back_to_pause = app.help_open && matches!(app.phase, Phase::Playing | Phase::Countdown);
    cx.close_popup(app.shell, !back_to_pause);
    if back_to_pause {
        open_dialog(app, cx, pause_dialog);
    }
}

fn pause_dialog(app: &mut App) -> Build<SkiaLayout> {
    app.help_open = false;
    dialog(
        app,
        "PAUSED",
        SkiaRow::new().spacing(14).horizontal_options(LayoutOptions::End).margin((0, 12, 0, 0)).children((
            ghost("HELP").on_tapped(|_me, app: &mut App, cx| {
                cx.close_popup(app.shell, false);
                open_dialog(app, cx, help_dialog);
            }),
            // Leaves the run for the title.
            ghost("HOME").on_tapped(|_me, app: &mut App, cx| {
                cx.close_popup(app.shell, true);
                if let Some(mut scene) = cx.get_mut(app.scene) {
                    scene.control_mut().world.to_title();
                }
            }),
            button("RESUME").on_tapped(|_me, app: &mut App, cx| cx.close_popup(app.shell, true)),
        )),
    )
}

fn build(app: &mut App) -> Build<SkiaShell> {
    let centered = |text: &str, size: i32, color: u32| {
        line(text, size, color).horizontal_options(LayoutOptions::Center).horizontal_text_alignment(TextAlignment::Center)
    };
    // The game takes every press that lands on it, so what is tapped lies over it, not in it.
    let game = DrawnGame::new()
        .fill()
        .background_color(Color::BLACK)
        .start_loop(0.0)
        .on_game_loop(|_me, app: &mut App, cx, delta| tick(app, cx, delta))
        .on_key_down(|_me, app: &mut App, cx, event| key_down(app, cx, event))
        .on_key_up(|_me, app: &mut App, _cx, event| key_up(app, event))
        .consume_gestures(|me, app: &mut App, cx, g| {
            let scale = me.base().scale.max(0.1);
            gesture(app, cx, g, scale)
        })
        // Draws every frame: no cache. The post effect reads what it drew.
        .children(Scene::new().fill().assign(&mut app.scene).visual_effect(post_effect()));
    // The health, top center: ten slanted cells, one per tenth. One cached image, drawn again only
    // when the health changes.
    let cells: Vec<Build<SkiaShape>> = app
        .cells
        .iter_mut()
        .map(|slot| {
            SkiaShape::new()
                .width_request(22)
                .height_request(14)
                .corner_radius(1)
                .skew_x(-18.0)
                .background_color(Color::new(GOOD))
                .stroke_color(Color::new(0x66FF_FFFF))
                .stroke_width(1)
                .shadows(SkiaShadow::new(Color::new(GOOD)).x(0).y(0).blur(7).opacity(0.9))
                .assign(slot)
        })
        .collect();
    let hud = SkiaStack::new()
        .spacing(7)
        .width_request(266)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 16, 0, 0))
        .opacity(0.0)
        .input_transparent(true)
        .use_cache(CacheType::Image)
        // Read as one value, said again when it changes.
        .accessibility_role(Aria::STATUS)
        .accessibility_label("Health 100 percent")
        .accessibility_live("polite")
        .assign(&mut app.hud)
        .children(SkiaRow::new().spacing(5).children(cells));
    // The distance, top left, in steps of ten meters: quiet (soft white, no glow) so it does not
    // pull the eye from the run; an orb makes it jump in a violet flash (`flash_score`).
    let score = SkiaLayer::new()
        .width_request(260)
        .height_request(60)
        .horizontal_options(LayoutOptions::Start)
        .margin((8, 4, 0, 0))
        .opacity(0.0)
        .input_transparent(true)
        .use_cache(CacheType::Image)
        .assign(&mut app.score_box)
        .children(
            SkiaLabel::new("0 M")
                .font_family("FontScore")
                .font_size(30)
                .text_color(Color::new(0xCCFF_FFFF))
                .stroke_color(Color::new(0x9900_0000))
                .stroke_width(1)
                .anchor_x(0.0)
                .anchor_y(0.0)
                .margin((14, 8, 0, 0))
                .assign(&mut app.score),
        );
    let streak = SkiaLabel::new("x2")
        .font_family("FontScore")
        .font_size(16)
        .text_color(Color::new(0xFFC9_8BFF))
        .margin((23, 52, 0, 0))
        .opacity(0.0)
        .input_transparent(true)
        .assign(&mut app.streak);
    // The countdown and GAME OVER, in the middle: heavy letters, yellow to red, in a red glow.
    let count = SkiaLayer::new()
        .width_request(760)
        .height_request(220)
        .horizontal_options(LayoutOptions::Center)
        .vertical_options(LayoutOptions::Center)
        .is_visible(false)
        .input_transparent(true)
        .use_cache(CacheType::Image)
        .visual_effect(
            SkiaShaderEffect::new()
                .shader_code(shaders::GLOW)
                .uniform("uGlow", &[1.0])
                .uniform("uWash", &[0.0])
                .uniform("uTint", &[1.0, 0.16, 0.04])
                .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: glow SkSL: {error}")),
        )
        .assign(&mut app.count)
        .children(
            SkiaLabel::new("3")
                .font_family("FontScore")
                .font_size(120)
                .text_color(Color::WHITE)
                .fill_gradient(fire([0xFFFF_E870, 0xFFFF_8A20, 0xFFE0_1810]))
                .stroke_color(Color::new(0xFF30_0404))
                .stroke_width(3)
                .center()
                .horizontal_text_alignment(TextAlignment::Center)
                .accessibility_live("assertive")
                .assign(&mut app.count_label),
        );
    let curtain = SkiaLayer::new().fill().is_visible(false).input_transparent(true).assign(&mut app.curtain).visual_effect(
        SkiaShaderEffect::new()
            .shader_code(shaders::CURTAIN)
            .use_background(UseBackground::Once)
            .uniform("uBurn", &[0.0])
            .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: curtain SkSL: {error}")),
    );
    let fps = SkiaLabelFps::new().font_size(12).padding((8, 4, 8, 4)).margin((16, 16, 0, 0)).assign(&mut app.fps);
    let buttons = SkiaRow::new()
        .spacing(8)
        .horizontal_options(LayoutOptions::End)
        .margin((0, 14, 16, 0))
        .assign(&mut app.buttons)
        .children((
            key("?", "Help").on_tapped(|_me, app: &mut App, cx| toggle_dialog(app, cx, help_dialog)),
            key("II", "Pause").on_tapped(|_me, app: &mut App, cx| menu(app, cx)),
        ));
    // The prompt, above the middle: heavy letters in the color of what it asks, in a glow of the
    // same color (`show_prompt`); its own cached image under the glow shader, like the countdown.
    let prompt = SkiaLayer::new()
        .width_request(760)
        .height_request(110)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 96, 0, 0))
        .is_visible(false)
        .input_transparent(true)
        .use_cache(CacheType::Image)
        .visual_effect(
            SkiaShaderEffect::new()
                .shader_code(shaders::GLOW)
                .uniform("uGlow", &[1.0])
                .uniform("uWash", &[0.0])
                .uniform("uTint", &[0.62, 0.22, 1.0])
                .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: glow SkSL: {error}")),
        )
        .assign(&mut app.prompt_box)
        .children(
            SkiaLabel::new("")
                .font_family("FontScore")
                .font_size(44)
                .text_color(Color::WHITE)
                .stroke_color(Color::new(0xFF14_0820))
                .stroke_width(2.5)
                .center()
                .horizontal_text_alignment(TextAlignment::Center)
                .accessibility_live("assertive")
                .assign(&mut app.prompt),
        );
    // Still over a scene that draws every frame: one cached image to blit.
    let title = SkiaStack::new()
        .spacing(14)
        .horizontal_options(LayoutOptions::Center)
        .vertical_options(LayoutOptions::Center)
        .input_transparent(true)
        .use_cache(CacheType::Image)
        .assign(&mut app.title)
        .children((
            // The name, white in a glow of the interface's accent color: its own cached image under the glow shader.
            SkiaLayer::new()
                .height_request(120)
                .margin((0, -32, 0, -34))
                .use_cache(CacheType::Image)
                .visual_effect(
                    SkiaShaderEffect::new()
                        .shader_code(shaders::GLOW)
                        .uniform("uGlow", &[0.5])
                        .uniform("uWash", &[0.0])
                        .uniform("uTint", &tint(ACCENT))
                        .on_compilation_error(|_me, _app: &mut App, _cx, error: &str| eprintln!("dungeon: glow SkSL: {error}")),
                )
                .children(
                    // The same heavy letters as GAME OVER, gold to amber instead of fire.
                    SkiaLabel::new("DUNGEON RUN")
                        .font_family("FontScore")
                        .font_size(44)
                        .text_color(Color::WHITE)
                        .fill_gradient(fire([0xFFFF_F4D0, 0xFFFF_C050, 0xFFD0_6810]))
                        .stroke_color(Color::new(0xFF2A_1404))
                        .stroke_width(2.5)
                        .horizontal_options(LayoutOptions::Center)
                        .vertical_options(LayoutOptions::Center)
                        .horizontal_text_alignment(TextAlignment::Center)
                        .assign(&mut app.name),
                ),
            centered("DRAWNUI FOR RUST SAMPLE", 14, ACCENT),
            centered(if MOBILE { "TAP TO PLAY" } else { "TAP OR PRESS SPACE TO PLAY" }, 11, 0x99FF_FFFF),
        ));
    SkiaShell::new()
        .assign(&mut app.shell)
        // A dialog over the run stops it.
        .on_changed(|me, app: &mut App, cx| {
            app.paused = me.popups_count() > 0;
            app.held = 0;
            if let Some(mut veil) = cx.get_mut(app.veil) {
                veil.set_is_visible(app.paused);
            }
            // Under a dialog the HUD's buttons are gone, so Tab stays in the dialog (the engine's
            // popups do not keep the keyboard to themselves).
            if let Some(mut buttons) = cx.get_mut(app.buttons) {
                buttons.set_is_visible(!app.paused);
            }
        })
        .root(SkiaLayer::new().fill().children((
            game,
            hud,
            fps,
            score,
            streak,
            buttons,
            prompt,
            count,
            title,
            curtain,
            SkiaBackdrop::new().blur(1.5).background_color(Color::new(0x7300_0000)).fill().input_transparent(true).is_visible(false).assign(&mut app.veil),
        )))
}

/// Runs the game: the desktop window, the browser canvas.
pub fn run() {
    drawnui::run_sized("Dungeon Run", 1100.0, 700.0, || {
        Box::new(
            Ui::new(App::default(), build)
                .font("FontGame", "assets/Orbitron-Regular.ttf")
                .font("FontScore", "assets/Orbitron-Black.ttf")
                .font_fallback(SYMBOLS, "assets/NotoSansMathSymbols-Subset.ttf")
                // Every label is text to a screen reader.
                .default_accessibility_role::<SkiaLabel>(Aria::TEXT)
                .background(Color::BLACK)
                // A game: every touch is its own. In a browser no pull-down or rubber band of the page
                // takes a drag (DrawnUI Gestures = Lock).
                .gestures(GesturesMode::Lock),
        )
    });
}
