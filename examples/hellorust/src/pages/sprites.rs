//! SkiaSprite spritesheets and a SkiaSpriteSet warrior on a tile board, moved with the keyboard.
//! Ported from the React demo's SpritesPage.tsx and WarriorSprite.ts (FastRepro SpriteTestPage
//! and SpriteSnappedBoardPage).

use drawnui::prelude::*;

use super::{card, card_title, page_title, scrolling};
use crate::{App, hex};

const TILE: i32 = 64;
const COLS: i32 = 9;
const ROWS: i32 = 4;

/// The warrior's states (WarriorSprite.WState): what it does and which way it faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Warrior {
    #[default]
    IdleRight,
    IdleLeft,
    WalkRight,
    WalkLeft,
    WarRight,
    WarLeft,
}

impl Warrior {
    /// The sprite set state: 0 idle, 1 walk, 2 war.
    fn state(self) -> i32 {
        match self {
            Warrior::IdleRight | Warrior::IdleLeft => 0,
            Warrior::WalkRight | Warrior::WalkLeft => 1,
            Warrior::WarRight | Warrior::WarLeft => 2,
        }
    }

    fn faces_left(self) -> bool {
        matches!(self, Warrior::IdleLeft | Warrior::WalkLeft | Warrior::WarLeft)
    }
}

/// What the page keeps.
pub struct State {
    sprite: Handle<SkiaSprite>,
    pub(super) info: String,
    fps: i32,
    playing: bool,
    pub(super) player: Handle<SkiaSpriteSet>,
    /// The tile the warrior stands on.
    pub(super) col: i32,
    pub(super) row: i32,
    pub(super) warrior: Warrior,
    moving: bool,
    facing_left: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sprite: Handle::default(),
            info: "loading…".to_owned(),
            fps: 15,
            playing: true,
            player: Handle::default(),
            col: 1,
            row: 1,
            warrior: Warrior::IdleRight,
            moving: false,
            facing_left: false,
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    // Sprites play all the time: nothing above them is cached, and neither are they (a stepped
    // frame draws faster than it records).
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        page_title("Sprites"),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!("SkiaSprite — Source=\"anims/BlueWarrior/Warrior_Idle.png\" Columns={{8}} Rows={{1}} · {}", app.sprites.info))
            }),
            // A wrap, not a row: a row measures the controls column with an endless width and the card
            // cut its buttons and caption (React 80a1629); here the column drops to its own line.
            SkiaWrap::new().spacing(16).children((
                sprite("assets/anims/BlueWarrior/Warrior_Idle.png", 8, 15)
                    .assign(&mut app.sprites.sprite)
                    .observe(|me, app: &App| me.set_frames_per_second(app.sprites.fps))
                    .on_success(|me, app: &mut App, cx, _source| {
                        if let Some(sprite) = cx.find::<SkiaSprite>(me) {
                            let (width, height) = sprite.frame_size();
                            app.sprites.info = format!(
                                "{} frames · {width}×{height} px · {} ms",
                                sprite.total_frames(),
                                sprite.duration_ms().round()
                            );
                        }
                    })
                    .on_error(|_me, app: &mut App, _cx, source| app.sprites.info = format!("error: {source} did not load")),
                sprite("assets/anims/RedWarrior/Warrior_Attack1.png", 4, 8),
                sprite("assets/anims/Trees/Tree1.png", 8, 6),
                SkiaStack::new().spacing(8).vertical_options(LayoutOptions::Center).fill_x().children((
                    SkiaWrap::new().spacing(8).children((
                        SkiaButton::new("Pause")
                            .background_color(hex(0x0D6EFD))
                            .font_size(13)
                            .observe(|me, app: &App| me.set_text(if app.sprites.playing { "Pause" } else { "Play" }))
                            .on_tapped(|_me, app: &mut App, cx| {
                                let Some(mut sprite) = cx.get_mut(app.sprites.sprite) else { return };
                                if sprite.control_mut().is_playing() {
                                    sprite.stop();
                                    app.sprites.playing = false;
                                } else {
                                    sprite.start();
                                    app.sprites.playing = true;
                                }
                            }),
                        [5, 15, 30].map(fps_button).into_iter().collect::<Vec<_>>(),
                        SkiaButton::new("Seek(0)").background_color(hex(0x495057)).font_size(13).on_tapped(|_me, app: &mut App, cx| {
                            if let Some(mut sprite) = cx.get_mut(app.sprites.sprite) {
                                sprite.stop();
                                sprite.seek(0);
                            }
                            app.sprites.playing = false;
                        }),
                    )),
                    note("Frames are cut from the sheet by Columns × Rows, transparent borders trimmed per frame (C# SpriteFrameImage), nearest sampling; the animator runs 0..DurationMs and picks the frame by time."),
                )),
            )),
        ),
        card(
            card_title("").font_family_fallback("FontSymbols,FontSymbols2").observe(|me, app: &App| {
                let page = &app.sprites;
                me.set_text(format!(
                    "SkiaSpriteSet warrior on a tile board — arrows / WASD move, Space attacks · tile {},{} · {:?}",
                    page.col, page.row, page.warrior
                ))
            }),
            (
                board(app),
                SkiaWrap::new().spacing(8).horizontal_options(LayoutOptions::Center).children((
                    move_button("← Left", -1, 0),
                    move_button("↑ Up", 0, -1),
                    move_button("↓ Down", 0, 1),
                    move_button("Right →", 1, 0),
                    SkiaButton::new("Attack (Space)").background_color(hex(0xD63384)).font_size(13).on_tapped(|_me, app: &mut App, cx| attack(app, cx)),
                )),
                note("The warrior is a SkiaSpriteSet: define(0 idle, 1 run, 2 attack) with the FastRepro sheets; its state picks the sheet and a left-facing state mirrors it; the move is a translate_to to the target tile while the walk state plays."),
            ),
        ),
    )))
}

/// A 160 x 160 sprite that plays forever.
fn sprite(source: &str, columns: i32, fps: i32) -> Build<SkiaSprite> {
    SkiaSprite::new(source)
        .columns(columns)
        .rows(1)
        .frames_per_second(fps)
        .repeat(-1)
        .width_request(160)
        .height_request(160)
        .background_color(hex(0x212529))
}

fn fps_button(fps: i32) -> Build<SkiaButton> {
    SkiaButton::new(format!("{fps} fps"))
        .font_size(13)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.sprites.fps == fps { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, _cx| app.sprites.fps = fps)
}

fn move_button(caption: &str, dx: i32, dy: i32) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .font_family_fallback("FontSymbols,FontSymbols2")
        .background_color(hex(0x0F3460))
        .font_size(13)
        .on_tapped(move |_me, app: &mut App, cx| walk(app, cx, dx, dy))
}

fn note(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(hex(0xADB5BD)).fill_x()
}

/// The 9 x 4 tile board: checkered grass, two trees, a red warrior and the player.
fn board(app: &mut App) -> Build<SkiaLayout> {
    let tiles = (0..COLS * ROWS)
        .map(|i| {
            let (col, row) = (i % COLS, i / COLS);
            SkiaShape::new()
                .width_request(TILE)
                .height_request(TILE)
                .background_color(hex(if (col + row) % 2 == 0 { 0x2D6A4F } else { 0x40916C }))
                .margin((col * TILE, row * TILE, 0, 0))
        })
        .collect::<Vec<_>>();
    SkiaLayer::new()
        .width_request(COLS * TILE)
        .height_request(ROWS * TILE)
        .background_color(hex(0x1B4332))
        .horizontal_options(LayoutOptions::Center)
        .is_clipped_to_bounds(true)
        .children((
            // The grass never changes: one bitmap under the sprites.
            SkiaLayer::new().width_request(COLS * TILE).height_request(ROWS * TILE).use_cache(CacheType::Image).children(tiles),
            board_sprite("assets/anims/Trees/Tree1.png", 6).margin((5 * TILE, 0, 0, 0)),
            board_sprite("assets/anims/Trees/Tree2.png", 5).margin((7 * TILE, 2 * TILE, 0, 0)),
            board_sprite("assets/anims/RedWarrior/Warrior_Idle.png", 15).margin((6 * TILE, 3 * TILE, 0, 0)).scale_x(-1).z_index(9),
            SkiaSpriteSet::new()
                .define(0, "assets/anims/BlueWarrior/Warrior_Idle.png", 8, 1, 15, -1, true)
                .define(1, "assets/anims/BlueWarrior/Warrior_Run.png", 6, 1, 15, -1, true)
                .define(2, "assets/anims/BlueWarrior/Warrior_Attack1.png", 4, 1, 8, -1, true)
                .width_request(TILE)
                .height_request(TILE)
                .z_index(10)
                .translation_x(TILE)
                .translation_y(TILE)
                .assign(&mut app.sprites.player),
        ))
}

/// A one-tile sprite on the board, playing forever. `columns` is 8 for every sheet placed here.
fn board_sprite(source: &str, fps: i32) -> Build<SkiaSprite> {
    SkiaSprite::new(source).columns(8).rows(1).frames_per_second(fps).repeat(-1).width_request(TILE).height_request(TILE)
}

/// Shows a warrior state: its sheet, mirrored when it faces left.
fn show(app: &mut App, cx: &mut Cx, warrior: Warrior) {
    app.sprites.warrior = warrior;
    if let Some(mut player) = cx.get_mut(app.sprites.player) {
        player.set_state(warrior.state());
        player.set_scale_x(if warrior.faces_left() { -1 } else { 1 });
    }
}

fn idle(app: &mut App, cx: &mut Cx) {
    let warrior = if app.sprites.facing_left { Warrior::IdleLeft } else { Warrior::IdleRight };
    show(app, cx, warrior);
}

/// Walks one tile, unless a move is running; at the edge the warrior only turns.
fn walk(app: &mut App, cx: &mut Cx, dx: i32, dy: i32) {
    let page = &mut app.sprites;
    if page.moving {
        return;
    }
    let (col, row) = ((page.col + dx).clamp(0, COLS - 1), (page.row + dy).clamp(0, ROWS - 1));
    if dx != 0 {
        page.facing_left = dx < 0;
    }
    if (col, row) == (page.col, page.row) {
        idle(app, cx);
        return;
    }
    page.moving = true;
    let walking = if page.facing_left { Warrior::WalkLeft } else { Warrior::WalkRight };
    let player = page.player;
    show(app, cx, walking);
    let step = cx.translate_to(player, col * TILE, row * TILE, 220, easing::linear);
    cx.on_finished(step, move |app: &mut App, cx| {
        (app.sprites.col, app.sprites.row) = (col, row);
        app.sprites.moving = false;
        idle(app, cx);
    });
}

fn attack(app: &mut App, cx: &mut Cx) {
    if app.sprites.moving {
        return;
    }
    let warring = if app.sprites.facing_left { Warrior::WarLeft } else { Warrior::WarRight };
    show(app, cx, warring);
    cx.after(app.sprites.player, 500, idle);
}

/// A key while the page is open: arrows and WASD walk, Space attacks.
pub fn key(app: &mut App, event: &KeyEvent, cx: &mut Cx) -> bool {
    if event.kind != KeyKind::Down {
        return false;
    }
    let (dx, dy) = match event.key {
        "ArrowLeft" | "KeyA" => (-1, 0),
        "ArrowRight" | "KeyD" => (1, 0),
        "ArrowUp" | "KeyW" => (0, -1),
        "ArrowDown" | "KeyS" => (0, 1),
        "Space" => {
            attack(app, cx);
            return true;
        }
        _ => return false,
    };
    walk(app, cx, dx, dy);
    true
}
