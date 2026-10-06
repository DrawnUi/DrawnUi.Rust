//! Pong, the DrawnUi Pong.Shared sample (MAUI, WPF, OpenTK, Blazor and pure-WASM heads) on this
//! engine: a DrawnGame with a game loop, sprites moved by Left / Top, an AI paddle, keyboard and
//! touch input. The 360 x 640 field is fitted to the page by a RescalingLayout (rendering scale,
//! not a transform). Ported from the React demo's PongPage.tsx and pong/*.ts, themselves ports of
//! PongGame.cs, PongGame.Loop.cs, PongAI.cs and the sprites.

use std::f32::consts::PI;

use drawnui::prelude::*;

use crate::{App, hex};

const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 640.0;
const PADDLE_SPEED: f32 = 420.0;
const PADDLE_MARGIN: f32 = 40.0;
const WIN_SCORE: u32 = 7;
const BALL_SIZE: f32 = 14.0;
const PADDLE_WIDTH: f32 = 80.0;
const PADDLE_HEIGHT: f32 = 16.0;
const BALL_SPEED: f32 = 300.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Phase {
    #[default]
    WaitingToStart,
    Playing,
    Scored,
    GameOver,
}

/// A collision rect in field space (GameExtensions.GetHitBox): Left / Top and the sprite's size.
#[derive(Clone, Copy, Default)]
struct HitBox {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl HitBox {
    fn of(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self { left, top, right: left + width, bottom: top + height }
    }
    /// SkiaSharp SKRect.IntersectsWith.
    fn intersects(&self, other: &HitBox) -> bool {
        self.left < other.right && other.left < self.right && self.top < other.bottom && other.top < self.bottom
    }
    fn mid_x(&self) -> f32 {
        (self.left + self.right) / 2.0
    }
    fn mid_y(&self) -> f32 {
        (self.top + self.bottom) / 2.0
    }
    fn width(&self) -> f32 {
        self.right - self.left
    }
}

/// The ball (BallSprite): where it is, where it goes, and the oscillation guard that unsticks a
/// ball bouncing between the same two angles.
#[derive(Default)]
struct Ball {
    left: f32,
    top: f32,
    angle: f32,
    speed: f32,
    moving: bool,
    history: [Option<f32>; 3],
    oscillations: u32,
}

impl Ball {
    fn hit_box(&self) -> HitBox {
        HitBox::of(self.left, self.top, BALL_SIZE, BALL_SIZE)
    }

    fn set_angle(&mut self, angle: f32, random: &mut Random) {
        self.angle = clamp_angle_from_horizontal(angle);
        if !self.moving {
            self.history = [None; 3];
            self.oscillations = 0;
            return;
        }
        self.history = [Some(self.angle), self.history[0], self.history[1]];
        let [Some(h1), Some(h2), Some(h3)] = self.history else { return };
        let tolerance = 0.01;
        if (h1 - h3).abs() < tolerance && (h2 - h1).abs() > tolerance {
            self.oscillations += 1;
            if self.oscillations >= 6 {
                let nudge = (random.next() - 0.5) * 0.4;
                self.angle = clamp_angle_from_horizontal(self.angle + nudge);
                self.history = [None; 3];
                self.oscillations = 0;
            }
        } else {
            self.oscillations = 0;
        }
    }

    fn update_position(&mut self, delta: f32) {
        if delta <= 0.0 || !self.moving {
            return;
        }
        self.left += self.speed * self.angle.cos() * delta;
        self.top += self.speed * self.angle.sin() * delta;
    }
}

/// Keeps the ball from flying (almost) horizontally: at least PI / 10 away from 0 and PI.
fn clamp_angle_from_horizontal(angle: f32) -> f32 {
    let min = PI / 10.0;
    let two_pi = 2.0 * PI;
    let mut normalized = angle % two_pi;
    if normalized <= -PI {
        normalized += two_pi;
    } else if normalized > PI {
        normalized -= two_pi;
    }
    let near_zero = normalized.abs() < min;
    let near_pi = normalized.abs() > PI - min;
    if !near_zero && !near_pi {
        return normalized;
    }
    let sign = if normalized < 0.0 { -1.0 } else { 1.0 };
    if near_zero { sign * min } else { sign * (PI - min) }
}

/// Math.random for the game: xorshift, seeded from the clock.
struct Random(u64);

impl Default for Random {
    fn default() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        Self(nanos | 1)
    }
}

impl Random {
    /// 0 <= x < 1.
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The AI of PongAI.cs at Medium difficulty.
#[derive(Default)]
struct Ai {
    target_x: f32,
    reaction_timer: f32,
    mistake_timer: f32,
    decision_change_timer: f32,
    decision_change_interval: f32,
    movement_smoothing_timer: f32,
    making_mistake: bool,
    mistake_direction: f32,
    is_moving: bool,
    last_movement: f32,
}

// Medium: reaction 0.06..0.22 s, accuracy 0.84, mistakes 10 % of decisions for 0.25..0.5 s,
// decisions every 1.8 s at first, movement smoothing 0.1 s.
const REACTION_MIN: f32 = 0.06;
const REACTION_MAX: f32 = 0.22;
const ACCURACY: f32 = 0.84;
const MISTAKE_PROBABILITY: f32 = 0.10;
const MISTAKE_MIN: f32 = 0.25;
const MISTAKE_MAX: f32 = 0.5;
const DECISION_INTERVAL: f32 = 1.8;
const SMOOTHING: f32 = 0.10;

/// What the page keeps: the game state and the controls it moves.
#[derive(Default)]
pub struct State {
    game: Handle<DrawnGame>,
    ball_view: Handle<SkiaShape>,
    player_view: Handle<SkiaShape>,
    ai_view: Handle<SkiaShape>,
    score_label: Handle<SkiaLabel>,
    message_label: Handle<SkiaLabel>,
    ball: Ball,
    pub(super) player_left: f32,
    ai_left: f32,
    pub(super) player_score: u32,
    pub(super) ai_score: u32,
    player_movement: f32,
    ai_movement: f32,
    pub(super) phase: Phase,
    phase_timer: f32,
    ai_serves: bool,
    auto_serve_timer: f32,
    ai_wander_timer: f32,
    ai_wander_dir: f32,
    player_has_moved: bool,
    last_scorer_player: bool,
    ai: Ai,
    random: Random,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    let page = &mut app.pong;
    page.player_left = (WIDTH - PADDLE_WIDTH) / 2.0;
    page.ai_left = (WIDTH - PADDLE_WIDTH) / 2.0;
    reset_ball(page, true);
    page.ai.decision_change_interval = DECISION_INTERVAL;
    reset_ai(page);
    let ball = (page.ball.left, page.ball.top);
    SkiaLayer::new().background_color(hex(0x0A0F1E)).fill().children((
        SkiaLabel::new("← → or drag to move, tap / Space to serve · first to 7")
            .font_family_fallback("FontSymbols,FontSymbols2")
            .font_size(13)
            .text_color(hex(0xADB5BD))
            .horizontal_options(LayoutOptions::Center)
            .margin((12, 8, 12, 0)),
        SkiaLayer::new().fill().margin((0, 36, 0, 0)).children(RescalingLayout::new(WIDTH, HEIGHT).children(
            DrawnGame::new()
                .width_request(WIDTH)
                .height_request(HEIGHT)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center)
                // Colors.DarkGreen
                .background_color(hex(0x006400))
                .assign(&mut page.game)
                .start_loop(0.0)
                .on_game_loop(|_me, app: &mut App, cx, delta| game_loop(app, cx, delta))
                .on_key_down(|_me, app: &mut App, cx, event| key_down(app, cx, event.key))
                .on_key_up(|_me, app: &mut App, _cx, event| key_up(app, event.key))
                // The field takes its presses: a pan steers the paddle, a tap serves.
                .consume_gestures(|me, app: &mut App, cx, gesture| match gesture.kind {
                    GestureKind::Panning => {
                        let velocity = gesture.velocity.x / me.base().scale.max(0.1);
                        app.pong.player_movement = if velocity.abs() > 5.0 { velocity.signum() } else { 0.0 };
                        true
                    }
                    GestureKind::Up => {
                        app.pong.player_movement = 0.0;
                        true
                    }
                    GestureKind::Tapped => {
                        serve(app, cx, false);
                        false
                    }
                    _ => false,
                })
                .children((
                    SkiaShape::new()
                        .background_color(Color::TRANSPARENT)
                        .stroke_color(hex(0xFEFEFE))
                        .stroke_width(2)
                        .fill(),
                    paddle(0xFF2222).left(page.ai_left).top(PADDLE_MARGIN).assign(&mut page.ai_view),
                    paddle(0x4CC9F0).left(page.player_left).top(HEIGHT - PADDLE_MARGIN - PADDLE_HEIGHT).assign(&mut page.player_view),
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .height_request(BALL_SIZE)
                        .lock_ratio(1)
                        .stroke_color(Color::WHITE)
                        .stroke_width(2)
                        .background_color(hex(0xFFFF00))
                        .bevel_type(BevelType::Bevel)
                        .bevel(SkiaBevel::new(3).light_color(Color::WHITE).shadow_color(hex(0x333333)).opacity(0.33))
                        .use_cache(CacheType::Image)
                        .left(ball.0)
                        .top(ball.1)
                        .assign(&mut page.ball_view),
                    SkiaLabel::new("0 : 0")
                        .font_family("FontGame")
                        .font_size(28)
                        .text_color(Color::WHITE)
                        .horizontal_options(LayoutOptions::Center)
                        .margin((0.0, HEIGHT / 2.0 - 24.0, 0.0, 0.0))
                        .horizontal_text_alignment(TextAlignment::Center)
                        .assign(&mut page.score_label),
                    SkiaLabel::new("TAP TO SERVE")
                        .font_family("FontGame")
                        .font_size(14)
                        .text_color(Color::new(0xCCFFFFFF))
                        .horizontal_options(LayoutOptions::Center)
                        .margin((0.0, HEIGHT / 2.0 + 12.0, 0.0, 0.0))
                        .horizontal_text_alignment(TextAlignment::Center)
                        .assign(&mut page.message_label),
                )),
        )),
    ))
}

/// A paddle sprite: 80 x 16, rounded, with a bevel. Its own bitmap, moved by `left`.
fn paddle(color: u32) -> Build<SkiaShape> {
    SkiaShape::new()
        .height_request(PADDLE_HEIGHT)
        .width_request(PADDLE_WIDTH)
        .corner_radius(PADDLE_HEIGHT / 2.0)
        .background_color(hex(color))
        .stroke_color(hex(0xCCCCFF))
        .stroke_width(2)
        .bevel_type(BevelType::Bevel)
        .bevel(SkiaBevel::new(4).light_color(Color::WHITE).shadow_color(hex(0x333333)).opacity(0.33))
        .use_cache(CacheType::Image)
}

/// The window was hidden or shown: a hidden page gets no frames, and the frame clock restarts
/// when it comes back, else the first frame would carry the whole hidden time.
pub fn visibility(app: &mut App, cx: &mut Cx, visible: bool) {
    if let Some(mut game) = cx.get_mut(app.pong.game) {
        if visible { game.resume() } else { game.pause() }
    }
}

fn key_down(app: &mut App, cx: &mut Cx, key: &str) -> bool {
    match key {
        "ArrowLeft" => app.pong.player_movement = -1.0,
        "ArrowRight" => app.pong.player_movement = 1.0,
        "Space" | "ArrowUp" | "ArrowDown" | "Enter" => serve(app, cx, false),
        _ => return false,
    }
    true
}

fn key_up(app: &mut App, key: &str) -> bool {
    let page = &mut app.pong;
    match key {
        "ArrowLeft" if page.player_movement < 0.0 => page.player_movement = 0.0,
        "ArrowRight" if page.player_movement > 0.0 => page.player_movement = 0.0,
        _ => return false,
    }
    true
}

fn reset_ball(page: &mut State, player_serves: bool) {
    page.ball.left = (WIDTH - BALL_SIZE) / 2.0;
    page.ball.top = if player_serves { HEIGHT - PADDLE_MARGIN - PADDLE_HEIGHT - BALL_SIZE } else { PADDLE_MARGIN + PADDLE_HEIGHT - 1.0 };
    page.ball.moving = false;
    let spread = (page.random.next() - 0.5) * 0.6;
    let angle = if player_serves { PI / 2.0 + spread } else { -PI / 2.0 + spread };
    page.ball.set_angle(angle, &mut page.random);
}

fn reset_paddles(page: &mut State) {
    page.player_left = (WIDTH - PADDLE_WIDTH) / 2.0;
    page.ai_left = (WIDTH - PADDLE_WIDTH) / 2.0;
}

fn set_message(cx: &mut Cx, page: &State, text: &str) {
    super::set_text(cx, page.message_label, text);
}

fn update_score(cx: &mut Cx, page: &State) {
    super::set_text(cx, page.score_label, format!("{} : {}", page.ai_score, page.player_score));
}

fn serve(app: &mut App, cx: &mut Cx, by_ai: bool) {
    serve_page(&mut app.pong, cx, by_ai);
}

/// PongGame.Serve: the ball goes, or a finished game starts over.
fn serve_page(page: &mut State, cx: &mut Cx, by_ai: bool) {
    if page.phase == Phase::WaitingToStart && (!page.ai_serves || by_ai) {
        page.ball.moving = true;
        page.ball.speed = BALL_SPEED;
        page.phase = Phase::Playing;
        set_message(cx, page, "");
        reset_ai(page);
    } else if page.phase == Phase::GameOver {
        (page.player_score, page.ai_score) = (0, 0);
        page.ai_serves = false;
        page.player_has_moved = false;
        page.ai_wander_dir = 0.0;
        update_score(cx, page);
        reset_ball(page, true);
        reset_paddles(page);
        page.phase = Phase::WaitingToStart;
        set_message(cx, page, "TAP TO SERVE");
    }
}

/// One frame of the game (PongGame.Loop.cs), then the sprites are moved to the new positions.
fn game_loop(app: &mut App, cx: &mut Cx, delta: f32) {
    step(&mut app.pong, cx, delta);
    let page = &app.pong;
    for (view, left) in [(page.player_view, page.player_left), (page.ai_view, page.ai_left)] {
        if let Some(mut paddle) = cx.get_mut(view) {
            paddle.set_left(left);
        }
    }
    if let Some(mut ball) = cx.get_mut(page.ball_view) {
        ball.set_left(page.ball.left);
        ball.set_top(page.ball.top);
    }
}

fn step(page: &mut State, cx: &mut Cx, delta: f32) {
    match page.phase {
        Phase::WaitingToStart => {
            if page.ai_serves {
                page.auto_serve_timer -= delta;
                wander(page, delta, 0.25, 0.25, 0.35);
                page.ai_left = move_paddle(page.ai_left, page.ai_wander_dir, delta);
                page.player_left = move_paddle(page.player_left, page.player_movement, delta);
                page.ball.left = page.ai_left + (PADDLE_WIDTH - BALL_SIZE) / 2.0;
                if page.auto_serve_timer <= 0.0 {
                    serve_page(page, cx, true);
                }
            } else {
                if page.player_movement != 0.0 {
                    page.player_has_moved = true;
                }
                page.player_left = move_paddle(page.player_left, page.player_movement, delta);
                page.ball.left = page.player_left + (PADDLE_WIDTH - BALL_SIZE) / 2.0;
                if page.player_has_moved {
                    wander(page, delta, 0.3, 0.5, 0.7);
                    page.ai_left = move_paddle(page.ai_left, page.ai_wander_dir * 0.3, delta);
                }
            }
            return;
        }
        Phase::Scored => {
            page.phase_timer -= delta;
            if page.phase_timer <= 0.0 {
                if page.player_score >= WIN_SCORE || page.ai_score >= WIN_SCORE {
                    page.phase = Phase::GameOver;
                    let text = if page.player_score >= WIN_SCORE { "YOU WIN!\nTAP TO RESTART" } else { "AI WINS!\nTAP TO RESTART" };
                    set_message(cx, page, text);
                } else {
                    let player_serves = page.last_scorer_player;
                    page.ai_serves = !player_serves;
                    page.auto_serve_timer = 1.5;
                    page.player_has_moved = false;
                    page.ai_wander_dir = 0.0;
                    reset_paddles(page);
                    reset_ball(page, player_serves);
                    page.phase = Phase::WaitingToStart;
                    set_message(cx, page, if page.ai_serves { "" } else { "TAP TO SERVE" });
                }
            }
            return;
        }
        Phase::GameOver => return,
        Phase::Playing => {}
    }

    page.ball.update_position(delta);
    if page.ball.left < 0.0 {
        page.ball.left = 0.0;
        let angle = PI - page.ball.angle;
        page.ball.set_angle(angle, &mut page.random);
    } else if page.ball.left + BALL_SIZE > WIDTH {
        page.ball.left = WIDTH - BALL_SIZE;
        let angle = PI - page.ball.angle;
        page.ball.set_angle(angle, &mut page.random);
    }

    let ball_hit = page.ball.hit_box();
    let max_deviation = PI * 0.27;
    let max_speed = BALL_SPEED * 2.0;

    let player_hit = HitBox::of(page.player_left, HEIGHT - PADDLE_MARGIN - PADDLE_HEIGHT, PADDLE_WIDTH, PADDLE_HEIGHT);
    if ball_hit.intersects(&player_hit) && page.ball.angle > 0.0 {
        let hit = (ball_hit.mid_x() - player_hit.left) / player_hit.width();
        page.ball.set_angle(-PI / 2.0 + (hit - 0.5) * max_deviation * 2.0, &mut page.random);
        if page.ball.angle.sin() > 0.0 {
            let angle = -page.ball.angle;
            page.ball.set_angle(angle, &mut page.random);
        }
        page.ball.top = player_hit.top - BALL_SIZE;
        page.ball.speed = (page.ball.speed + 20.0).min(max_speed);
    }

    let ai_hit = HitBox::of(page.ai_left, PADDLE_MARGIN, PADDLE_WIDTH, PADDLE_HEIGHT);
    if ball_hit.intersects(&ai_hit) && page.ball.angle < 0.0 {
        let hit = (ball_hit.mid_x() - ai_hit.left) / ai_hit.width();
        page.ball.set_angle(PI / 2.0 + (hit - 0.5) * max_deviation * 2.0, &mut page.random);
        if page.ball.angle.sin() < 0.0 {
            let angle = -page.ball.angle;
            page.ball.set_angle(angle, &mut page.random);
        }
        page.ball.top = ai_hit.bottom;
        page.ball.speed = (page.ball.speed + 20.0).min(max_speed);
    }

    if page.ball.top + BALL_SIZE < 0.0 {
        page.player_score += 1;
        update_score(cx, page);
        score(page, cx, true);
        return;
    }
    if page.ball.top > HEIGHT {
        page.ai_score += 1;
        update_score(cx, page);
        score(page, cx, false);
        return;
    }

    page.player_left = move_paddle(page.player_left, page.player_movement, delta);
    page.ai_left = move_paddle(page.ai_left, page.ai_movement, delta);
    update_ai(page, delta);
}

/// The AI paddle drifting while waiting for a serve (both wander blocks of PongGame.Loop.cs).
fn wander(page: &mut State, delta: f32, stay_probability: f32, min_secs: f32, range_secs: f32) {
    page.ai_wander_timer -= delta;
    if page.ai_wander_timer <= 0.0 {
        page.ai_wander_dir = if page.random.next() < stay_probability {
            0.0
        } else if page.random.next() < 0.5 {
            -1.0
        } else {
            1.0
        };
        page.ai_wander_timer = min_secs + page.random.next() * range_secs;
    }
    if page.ai_left <= 0.0 && page.ai_wander_dir < 0.0 {
        page.ai_wander_dir = 1.0;
    }
    if page.ai_left + PADDLE_WIDTH >= WIDTH && page.ai_wander_dir > 0.0 {
        page.ai_wander_dir = -1.0;
    }
}

fn score(page: &mut State, cx: &mut Cx, by_player: bool) {
    page.last_scorer_player = by_player;
    page.ball.moving = false;
    page.phase = Phase::Scored;
    page.phase_timer = 1.5;
    set_message(cx, page, if by_player { "POINT!" } else { "AI SCORES!" });
}

fn move_paddle(left: f32, direction: f32, delta: f32) -> f32 {
    if direction == 0.0 {
        return left;
    }
    (left + direction * PADDLE_SPEED * delta).clamp(0.0, WIDTH - PADDLE_WIDTH)
}

// ---------------------------------------------------------------- the AI (PongAI.cs)

fn reaction_time(random: &mut Random) -> f32 {
    random.next() * (REACTION_MAX - REACTION_MIN) + REACTION_MIN
}

fn reset_ai(page: &mut State) {
    let ai = &mut page.ai;
    ai.reaction_timer = reaction_time(&mut page.random);
    ai.mistake_timer = 0.0;
    ai.decision_change_timer = ai.decision_change_interval;
    ai.movement_smoothing_timer = 0.0;
    ai.making_mistake = false;
    ai.mistake_direction = 0.0;
    ai.is_moving = false;
    ai.last_movement = 0.0;
    page.ai_movement = 0.0;
}

fn set_ai_movement(page: &mut State, direction: f32) {
    if direction == page.ai.last_movement {
        return;
    }
    page.ai.last_movement = direction;
    page.ai.is_moving = direction != 0.0;
    page.ai_movement = direction;
}

fn move_toward_target(page: &mut State) {
    if page.ai.movement_smoothing_timer > 0.0 {
        return;
    }
    page.ai.movement_smoothing_timer = SMOOTHING / 2.0;
    let distance = page.ai.target_x - page.ai_left;
    if distance.abs() < PADDLE_WIDTH * 0.15 {
        set_ai_movement(page, 0.0);
    } else {
        set_ai_movement(page, distance.signum());
    }
}

fn update_ai(page: &mut State, delta: f32) {
    if !page.ball.moving {
        set_ai_movement(page, 0.0);
        return;
    }
    page.ai.reaction_timer -= delta;
    page.ai.movement_smoothing_timer -= delta;

    if page.ai.making_mistake {
        page.ai.mistake_timer -= delta;
        if page.ai.mistake_timer <= 0.0 {
            page.ai.making_mistake = false;
            page.ai.reaction_timer = reaction_time(&mut page.random);
            set_ai_movement(page, 0.0);
        } else {
            let direction = page.ai.mistake_direction;
            set_ai_movement(page, direction);
        }
        return;
    }

    page.ai.decision_change_timer -= delta;
    if page.ai.decision_change_timer <= 0.0 {
        if page.random.next() < MISTAKE_PROBABILITY {
            page.ai.making_mistake = true;
            page.ai.mistake_timer = page.random.next() * (MISTAKE_MAX - MISTAKE_MIN) + MISTAKE_MIN;
            page.ai.mistake_direction = if page.random.next() < 0.7 {
                if page.random.next() < 0.5 { -1.0 } else { 1.0 }
            } else {
                0.0
            };
        }
        page.ai.decision_change_interval = 0.5 + page.random.next();
        page.ai.decision_change_timer = page.ai.decision_change_interval;
    }
    if page.ai.making_mistake {
        let direction = page.ai.mistake_direction;
        set_ai_movement(page, direction);
        return;
    }

    let ball = &page.ball;
    let coming_up = ball.angle.sin() < 0.0;
    if coming_up && page.ai.reaction_timer <= 0.0 {
        let (velocity_x, velocity_y) = (ball.angle.cos() * ball.speed, ball.angle.sin() * ball.speed);
        // The hit box is in field space, the same origin as Left / Top.
        let paddle_center_y = PADDLE_MARGIN + PADDLE_HEIGHT / 2.0;
        let time_to_intersect = (paddle_center_y - ball.hit_box().mid_y()) / velocity_y;
        if time_to_intersect > 0.0 {
            let mut predicted = ball.hit_box().mid_x() + velocity_x * time_to_intersect;
            // Bounces off the side walls, bounded.
            for _ in 0..16 {
                if predicted < 0.0 {
                    predicted = -predicted;
                } else if predicted > WIDTH {
                    predicted = 2.0 * WIDTH - predicted;
                } else {
                    break;
                }
            }
            let max_error = (1.0 - ACCURACY) * PADDLE_WIDTH;
            let error = (page.random.next() * 2.0 - 1.0) * max_error;
            page.ai.target_x = predicted + error - PADDLE_WIDTH / 2.0;
            page.ai.reaction_timer = reaction_time(&mut page.random);
            move_toward_target(page);
        }
    } else if !coming_up && page.ai.movement_smoothing_timer <= 0.0 {
        let center_x = (WIDTH - PADDLE_WIDTH) / 2.0;
        if (page.ai_left - center_x).abs() > PADDLE_WIDTH {
            page.ai.target_x = center_x;
            move_toward_target(page);
        } else if page.ai.is_moving && page.random.next() < 0.3 {
            set_ai_movement(page, 0.0);
        }
        page.ai.movement_smoothing_timer = SMOOTHING;
    }
}
