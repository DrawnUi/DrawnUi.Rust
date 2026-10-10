//! The dungeon: the rules of the run (`World`) and the control that draws it (`Scene`).
//! The corridor is straight in the world; the camera bends it with depth (the endless runner
//! trick), so the rules stay simple and far to near is always the drawing order. Every frame the
//! visible quads are clipped at the near plane and projected on the CPU into ONE SkMesh; its
//! fragment program does the materials, the torches and the fog.

use std::cell::{OnceCell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

use drawnui::prelude::*;
use drawnui::skia::{
    Blender, Data, Mesh, MeshSpecification, Paint,
    mesh::{Attribute, Mode, Varying, attribute, varying},
    meshes,
};

use crate::shaders::{MESH_FS, MESH_VS};

/// Length of a corridor segment: one row of the run.
const SEG: f32 = 2.0;
const HALF_WIDTH: f32 = 3.0;
const HEIGHT: f32 = 5.0;
/// Distance between the three lanes.
const LANE: f32 = 2.0;
/// Segments drawn ahead; the fog hides the end.
const VISIBLE: i64 = 50;
const NEAR: f32 = 0.2;
/// The shader works in coordinates moved back by a multiple of this, so they stay small. A
/// multiple of two torch distances: the torches keep their walls.
const REBASE: f64 = 40.0;
/// Floats per vertex: position 2, then three float4.
const FLOATS: usize = 14;
/// How long GAME OVER stays, and how long its frozen picture takes to burn away over the title.
/// The dungeon ends here: a doorway of light. The last hazards stand 100 units before it.
pub const EXIT: f64 = 3000.0;
/// From this far before the door the way out is in sight (the corridor is straight by then).
const EXIT_SIGHT: f64 = 300.0;
const EXIT_ROW: i64 = (EXIT / 2.0) as i64;
const BURN_SECONDS: f32 = 1.4;
/// A zone of one palette.
const ZONE: f64 = 400.0;

/// Torch, accent (lava, runes) and fog colors per zone.
const PALETTES: [[[f32; 3]; 3]; 3] = [
    [[1.0, 0.55, 0.22], [1.0, 0.33, 0.04], [0.045, 0.02, 0.03]],
    [[0.35, 0.65, 1.0], [0.15, 0.85, 1.0], [0.012, 0.03, 0.07]],
    [[0.55, 1.0, 0.3], [0.7, 1.0, 0.08], [0.02, 0.05, 0.022]],
];

fn hash(i: i64, salt: u64) -> u32 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    (x >> 33) as u32
}

fn hash01(i: i64, salt: u64) -> f32 {
    (hash(i, salt) & 0xFFFF) as f32 / 65536.0
}

/// What stands in a segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Empty,
    /// An orb in this lane (-1, 0, 1).
    Orb(i32),
    /// Pillars in the lanes of the mask (bit = lane + 1): change lane.
    Pillars(u8),
    /// A beam across the corridor: jump.
    Beam,
    /// A lava pit: jump.
    Lava,
    /// A power-up in this lane: `HEALTH` or `SURGE`.
    Power(i32, u8),
}

/// Gives 15 percent of the health back.
pub const HEALTH: u8 = 0;
/// What one orb gives back: ten orbs fill one cell of the health (the green cross fills two).
const ORB_HEALTH: f32 = 0.01;
/// Four seconds much faster, through everything unhurt.
pub const SURGE: u8 = 1;
/// The dungeon ghost: it hangs in the orbs' lane like a power-up and takes a third of the health.
pub const GHOST: u8 = 2;
/// A golden shield: the next pillar or ghost costs nothing (beams and lava still burn).
pub const SHIELD: u8 = 3;

/// The lane the orbs of a group of five rows hang in; the pillars after them leave it free.
fn orb_lane(group: i64) -> i32 {
    (hash(group, 1) % 3) as i32 - 1
}

/// The ghost's lane: any, but never the one lane a double pillar row leaves free, neither the
/// row just before it (no time to step aside) nor the row just after it (no lane to step into).
fn ghost_lane(group: i64) -> i32 {
    let only_free = |i: i64| match row(i) {
        Row::Pillars(mask) => (-1..=1).filter(|lane| mask & lane_bit(*lane) == 0).collect::<Vec<_>>().first().copied().filter(|_| mask.count_ones() == 2),
        _ => None,
    };
    let (before, after) = (only_free(group * 5), only_free((group + 1) * 5));
    let mut lane = (hash(group, 10) % 3) as i32 - 1;
    while Some(lane) == before || Some(lane) == after {
        lane = if lane == 1 { -1 } else { lane + 1 };
    }
    lane
}

fn lane_bit(lane: i32) -> u8 {
    1 << (lane + 1)
}

/// The run gets its full share of hazards (seven event rows in eight) from this row on; before it
/// the share climbs from a third, and the first jumps come at `JUMPS_FROM` (about 13 seconds in).
const RAMP_UNTIL: i64 = 304;
const JUMPS_FROM: i64 = 90;
/// Seconds before a hazard its prompt shows (longer while the run is young).
const HINT_LEAD: f32 = 1.2;
/// Seconds of running that cost one cell of health.
const DRAIN_SECONDS: f32 = 4.0;
/// How long a shield holds.
const SHIELD_SECONDS: f32 = 10.0;

/// The run is a function of the row number: an event every fifth row, orbs in between.
fn row(i: i64) -> Row {
    if i < 24 || i >= EXIT_ROW - 50 {
        return Row::Empty;
    }
    let group = i.div_euclid(5);
    match i.rem_euclid(5) {
        0 => {
            // A calm start: few hazards, lane changes before jumps, the full share after a while.
            let ramp = ((i - 24) as f32 / (RAMP_UNTIL - 24) as f32).clamp(0.0, 1.0);
            if hash01(group, 7) > 0.3 + 0.7 * ramp {
                return Row::Empty;
            }
            // Of a hundred event rows: 38 pillars, 20 beams, 21 lava, the rest empty.
            let h = hash(group, 2);
            let roll = h % 100;
            if roll < 38 || (roll < 79 && i < JUMPS_FROM) {
                let others = match orb_lane(group - 1) {
                    -1 => [0, 1],
                    0 => [-1, 1],
                    _ => [-1, 0],
                };
                return Row::Pillars(match (h >> 8) % 3 {
                    0 => lane_bit(others[0]),
                    1 => lane_bit(others[1]),
                    _ => lane_bit(others[0]) | lane_bit(others[1]),
                });
            }
            match roll {
                38..=57 => Row::Beam,
                58..=78 => Row::Lava,
                _ => Row::Empty,
            }
        }
        // Now and then the middle orb of a group is a power-up: health in one group of twelve, a
        // surge in one of twenty-four (it goes through everything: more made the run too easy),
        // the ghost in 18 % of the groups after the first thirty rows, in any lane.
        3 => match hash(group, 6) % 24 {
            _ if i > 30 && hash(group, 9) % 100 < 18 => Row::Power(ghost_lane(group), GHOST),
            0 | 1 => Row::Power(orb_lane(group), HEALTH),
            2 => Row::Power(orb_lane(group), SURGE),
            3 | 4 => Row::Power(orb_lane(group), SHIELD),
            _ => Row::Orb(orb_lane(group)),
        },
        2 | 4 => Row::Orb(orb_lane(group)),
        _ => Row::Empty,
    }
}

/// How much lava lights the boundary before segment `j`.
fn lava_glow(j: i64) -> f32 {
    let lava = |i: i64| row(i) == Row::Lava;
    if lava(j) || lava(j - 1) {
        1.0
    } else if lava(j + 1) || lava(j - 2) {
        0.35
    } else {
        0.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// The game plays itself behind the title.
    #[default]
    Attract,
    /// Three, two, one: the runner stands, nothing is taken from the player yet.
    Countdown,
    Playing,
    /// The health is gone: the runner goes down.
    Dead,
    /// Down: GAME OVER stays until a key or a tap brings the title back.
    Over,
}

/// What a new player should do about what comes next.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Hint {
    #[default]
    None,
    Jump,
    Lane,
}

/// What the player asked for since the last step.
#[derive(Default)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

pub struct World {
    pub phase: Phase,
    /// Distance run.
    z: f64,
    x: f32,
    y: f32,
    vy: f32,
    lane: i32,
    speed: f32,
    pub time: f32,
    orbs: u32,
    /// 1 = unhurt; a pillar takes two tenths, a beam or lava one; the run ends at 0.
    pub health: f32,
    /// Seconds left in which nothing hurts, after a hit.
    invulnerable: f32,
    /// The distance of the last run and the best one.
    pub last: u32,
    pub last_distance: u32,
    pub best: u32,
    /// Rows whose orb was taken, the newest 64.
    collected: Vec<i64>,
    /// Orbs taken in a row (their rows follow each other), and the row of the last one.
    pub streak: u32,
    last_orb: i64,
    /// The border's violet pulse of an orb, fading.
    pub orb_pulse: f32,
    dead_timer: f32,
    /// 1 at a hit, fading.
    pub flash: f32,
    /// 1 at a portal (a new zone, a start), fading.
    pub warp: f32,
    shake: f32,
    zone: i64,
    /// Sideways speed, smoothed: the camera leans into a lane change.
    lean: f32,
    pilot_wait: f32,
    /// Seconds of countdown left.
    count: f32,
    /// 0 to 1: how far the runner went down after the last hit.
    fall: f32,
    /// 1 when health was just picked up, fading: the green flash and its sparks.
    pub heal: f32,
    /// 1 when the ghost just took the run, fading: the pale flash.
    pub ghost: f32,
    /// The row of the ghost that took the run: it stays, over the fallen runner.
    pub killer: Option<i64>,
    /// 0..1: a ghost is near ahead: the dungeon darkens. `revealed`: the ghost that lunged last.
    pub dread: f32,
    revealed: Option<i64>,
    /// Ghosts the runner has gone past (hit or not) in this run.
    pub ghosts_passed: u32,
    /// Hits taken in this run (the drain is not a hit).
    pub hits: u32,
    /// Seconds the shield still holds (ten from its pickup); `shield_burst`: 1 when it just took a
    /// hit, fading (the golden flash).
    pub shield: f32,
    pub shield_burst: f32,
    /// The runner reached the exit: the run ends in light, not on the floor. `run_time`: seconds
    /// of the run; `best_time`: the fastest escape of the session; `escape`: 0 to 1, the white-out.
    pub escaped: bool,
    /// How far the world outside the door is lifted toward white (eyes used to the dark): the
    /// darker the corridor around the runner, the more. 0.3 where the opening comes into sight,
    /// 0.15 at the door, settling to 0 in about a second out there (full contrast).
    pub glare: f32,
    pub run_time: f32,
    pub best_time: Option<f32>,
    pub escape: f32,
    /// 0 to 1, eased: how strong the rays of a surge are (full at first, fading with its time left).
    pub surge_glow: f32,
    /// 1 to 0 after GAME OVER: how much of its frozen picture still covers the title's run.
    pub burn: f32,
    /// 1 = full speed; a pillar stops the runner (0), a beam or lava slows it, then it picks up.
    pace: f32,
    /// Seconds of surge left.
    surge: f32,
    /// The power-up taken in the last step, until the app asks.
    pickup: Option<u8>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            phase: Phase::Attract,
            z: 0.0,
            x: 0.0,
            y: 0.0,
            vy: 0.0,
            lane: 0,
            speed: 16.0,
            time: 0.0,
            orbs: 0,
            health: 1.0,
            invulnerable: 0.0,
            last: 0,
            last_distance: 0,
            best: 0,
            collected: Vec::with_capacity(64),
            streak: 0,
            last_orb: -10,
            orb_pulse: 0.0,
            dead_timer: 0.0,
            flash: 0.0,
            warp: 0.0,
            shake: 0.0,
            zone: 0,
            lean: 0.0,
            pilot_wait: 0.0,
            count: 0.0,
            fall: 0.0,
            burn: 0.0,
            heal: 0.0,
            ghost: 0.0,
            killer: None,
            dread: 0.0,
            revealed: None,
            ghosts_passed: 0,
            hits: 0,
            shield: 0.0,
            shield_burst: 0.0,
            escaped: false,
            glare: 0.3,
            run_time: 0.0,
            best_time: None,
            escape: 0.0,
            surge_glow: 0.0,
            pace: 1.0,
            surge: 0.0,
            pickup: None,
        }
    }
}

impl World {
    /// How many orbs were taken (the third of a row counts double).
    pub fn orbs(&self) -> u32 {
        self.orbs
    }

    /// How far the run got, in units of the dungeon (shown as meters).
    pub fn distance(&self) -> u32 {
        self.z as u32
    }

    /// The run is still in its calm start: every hazard comes with its prompt.
    pub fn young(&self) -> bool {
        self.z < RAMP_UNTIL as f64 * 2.0
    }

    /// 0..1: how fast the run is, for the effects.
    pub fn rush(&self) -> f32 {
        if self.surge > 0.0 { 1.0 } else { ((self.speed - 12.0) / 15.0).clamp(0.0, 1.0) }
    }

    /// The move the nearest hazard ahead asks for, from `HINT_LEAD` seconds before it (more while
    /// the run is young). Event rows are ten units apart, so several are looked at: the lead is
    /// longer than the way to the next one.
    pub fn hint(&self) -> Hint {
        if self.phase != Phase::Playing {
            return Hint::None;
        }
        let lead = self.speed * if self.young() { HINT_LEAD * 1.3 } else { HINT_LEAD };
        let first = (self.z / 2.0).floor() as i64;
        for i in first..first + 40 {
            let distance = (i as f64 * 2.0 + 1.0 - self.z) as f32;
            if distance > lead {
                return Hint::None;
            }
            let hint = match row(i) {
                Row::Beam | Row::Lava if self.y <= 0.0 => Hint::Jump,
                Row::Pillars(mask) if mask & lane_bit(self.lane) != 0 => Hint::Lane,
                _ => Hint::None,
            };
            if hint != Hint::None && distance >= 0.0 {
                return hint;
            }
        }
        Hint::None
    }

    /// An orb is taken: a little health back (a tenth of what the green cross gives), the borders
    /// pulse, stronger for every next orb of a row; the third orb of a row counts double.
    fn take_orb(&mut self, row: i64) {
        self.streak = if row == self.last_orb + 1 { self.streak + 1 } else { 1 };
        self.last_orb = row;
        let count = if self.streak == 3 { 2 } else { 1 };
        self.orbs += count;
        self.health = (self.health + ORB_HEALTH * count as f32).min(1.0);
        let power = 1.0 + 0.3 * (self.streak.min(3) - 1) as f32;
        self.orb_pulse = 0.55 * power;
    }

    /// The power-up taken since the last call.
    pub fn take_pickup(&mut self) -> Option<u8> {
        self.pickup.take()
    }

    fn restart(&mut self, phase: Phase) {
        (self.z, self.x, self.y, self.vy, self.lane, self.orbs, self.zone) = (0.0, 0.0, 0.0, 0.0, 0, 0, 0);
        (self.health, self.invulnerable, self.surge, self.pace, self.fall) = (1.0, 0.0, 0.0, 1.0, 0.0);
        self.collected.clear();
        (self.ghost, self.killer, self.dread, self.revealed, self.ghosts_passed, self.hits) = (0.0, None, 0.0, None, 0, 0);
        (self.escaped, self.run_time, self.escape, self.glare) = (false, 0.0, 0.0, 0.3);
        (self.shield, self.shield_burst) = (0.0, 0.0);
        (self.streak, self.last_orb, self.orb_pulse) = (0, -10, 0.0);
        self.phase = phase;
    }

    /// The health is gone: the runner goes down.
    fn die(&mut self) {
        self.health = 0.0;
        self.phase = Phase::Dead;
        // Under the ghost the runner lies a while longer.
        self.dead_timer = if self.killer.is_some() { 2.6 } else { 1.6 };
        (self.last, self.last_distance) = (self.distance(), self.distance());
        self.best = self.best.max(self.last);
    }

    /// A new run, through a portal.
    pub fn start(&mut self) {
        self.restart(Phase::Countdown);
        (self.count, self.warp) = (3.0, 0.35);
    }

    /// Back to the title and its own run.
    pub fn to_title(&mut self) {
        self.restart(Phase::Attract);
        self.burn = 1.0;
    }

    /// Seconds GAME OVER (or the view from outside) has been standing.
    pub fn over_for(&self) -> f32 {
        if self.phase == Phase::Over { -self.dead_timer } else { 0.0 }
    }

    /// The number the countdown shows: 3, 2, 1.
    pub fn count(&self) -> Option<u32> {
        (self.phase == Phase::Countdown).then(|| self.count.ceil().max(1.0) as u32)
    }

    pub fn step(&mut self, dt: f32, input: &mut Input) {
        self.time += dt;
        self.flash = (self.flash - dt * 1.6).max(0.0);
        self.warp = (self.warp - dt * 1.1).max(0.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.invulnerable = (self.invulnerable - dt).max(0.0);
        self.heal = (self.heal - dt * 1.1).max(0.0);
        self.ghost = (self.ghost - dt * 0.7).max(0.0);
        self.shield_burst = (self.shield_burst - dt * 1.4).max(0.0);
        let glare = if self.z >= EXIT { 0.0 } else { 0.15 + 0.15 * ((EXIT - self.z) / EXIT_SIGHT).clamp(0.0, 1.0) as f32 };
        self.glare += (glare - self.glare) * (1.0 - (-dt * 2.0).exp());
        if self.phase == Phase::Playing {
            self.shield = (self.shield - dt).max(0.0);
        }
        // The nearest ghost ahead: dread grows over the last three seconds to it, and when it is
        // less than a second away it reveals itself with a jolt.
        let mut dread: f32 = 0.0;
        if self.phase == Phase::Playing {
            let first = (self.z / 2.0).floor() as i64;
            for i in first..first + 60 {
                if let Row::Power(_, GHOST) = row(i) {
                    let distance = (i as f64 * 2.0 + 1.0 - self.z) as f32;
                    if distance > 0.0 {
                        dread = (1.0 - distance / (self.speed * 3.0)).clamp(0.0, 1.0);
                        if distance < self.speed * 0.9 && self.revealed != Some(i) {
                            self.revealed = Some(i);
                            (self.shake, self.warp) = (self.shake.max(0.6), self.warp.max(0.7));
                        }
                    }
                    break;
                }
            }
        } else if self.killer.is_some() {
            dread = 1.0;
        }
        self.dread += (dread - self.dread) * (1.0 - (-dt * 6.0).exp());
        if let Some(i) = self.revealed
            && self.z > i as f64 * 2.0 + 1.5
        {
            self.ghosts_passed += 1;
            self.revealed = None;
        }
        self.orb_pulse = (self.orb_pulse - dt * 4.5).max(0.0);

        // Full for the first second of a surge, then fading with the time left: faint rays say it is almost over.
        let surging = if self.phase != Phase::Dead { (self.surge / 3.0).clamp(0.0, 1.0) } else { 0.0 };
        self.surge_glow += (surging - self.surge_glow) * (1.0 - (-dt * 5.0).exp());
        if self.surge_glow < 0.004 {
            self.surge_glow = 0.0;
        }
        if self.phase != Phase::Over {
            self.burn = (self.burn - dt / BURN_SECONDS).max(0.0);
        }
        // Down, the picture stays red.
        self.flash = self.flash.max(0.3 * self.fall);
        let mut input = std::mem::take(input);
        match self.phase {
            Phase::Dead => {
                input = Input::default();
                self.speed *= (-dt * 4.0).exp();
                self.dead_timer -= dt;
                if self.escaped {
                    // Drifts on into the light, slowing.
                    self.speed *= (-dt * 1.2).exp();
                    self.escape = (self.escape + dt / 1.4).min(1.0);
                } else {
                    self.fall = (self.fall + dt / 1.1).min(1.0);
                }
                if self.dead_timer <= 0.0 {
                    self.phase = Phase::Over;
                }
            }
            Phase::Over => {
                input = Input::default();
                self.speed = if self.escaped { self.speed * (-dt * 1.2).exp() } else { 0.0 };
                self.dead_timer -= dt;
                if self.escaped {
                    self.escape = (self.escape + dt / 1.4).min(1.0);
                }
                // GAME OVER and the view from outside stay until a key or a tap (`to_title`).
            }
            Phase::Countdown => {
                (input, self.speed) = (Input::default(), 0.0);
                let before = self.count.ceil();
                self.count -= dt;
                if self.count <= 0.0 {
                    // Off, through a portal.
                    (self.phase, self.warp) = (Phase::Playing, 1.0);
                } else if self.count.ceil() != before {
                    self.warp = self.warp.max(0.35);
                }
            }
            Phase::Attract => {
                input = self.pilot(dt);
                self.speed = 16.0;
                // The title's run never shows the end: it starts over before the exit's light
                // (which begins 600 units before the door).
                if self.z >= EXIT - 700.0 {
                    self.restart(Phase::Attract);
                }
            }
            Phase::Playing => {
                self.speed = (12.0 + self.z as f32 * 0.012).min(24.0);
                self.run_time += dt;
                // The run wears the runner down: a cell of health every DRAIN_SECONDS. The orbs
                // are what keeps it up.
                self.health -= dt / (DRAIN_SECONDS * 10.0);
                if self.health < 0.005 {
                    self.die();
                }
                if self.z >= EXIT + 1.0 {
                    // Out, into the light: the run ends like a death without the fall.
                    (self.escaped, self.phase, self.dead_timer, self.warp) = (true, Phase::Dead, 1.8, 1.0);
                    (self.last, self.last_distance) = (self.distance(), self.distance());
                    self.best_time = Some(self.best_time.map_or(self.run_time, |best| best.min(self.run_time)));
                }
            }
        }
        if self.surge > 0.0 && self.phase != Phase::Dead {
            self.surge -= dt;
            self.speed *= 1.5;
        }

        if input.left {
            self.lane = (self.lane - 1).max(-1);
        }
        if input.right {
            self.lane = (self.lane + 1).min(1);
        }
        if input.jump && self.y <= 0.0 {
            self.vy = 7.6;
        }
        let before = self.x;
        self.x += (self.lane as f32 * LANE - self.x) * (1.0 - (-dt * 13.0).exp());
        let sideways = (self.x - before) / dt.max(1e-4);
        self.lean += (sideways * 0.018 - self.lean) * (1.0 - (-dt * 10.0).exp());
        self.y += self.vy * dt;
        self.vy -= 22.0 * dt;
        if self.y <= 0.0 {
            (self.y, self.vy) = (0.0, self.vy.max(0.0));
        }

        // The runner is a point half a unit ahead of the camera; what it passed through this step.
        self.pace = (self.pace + dt / 1.1).min(1.0);
        let (p0, p1) = (self.z + 0.5, self.z + (self.speed * self.pace * self.pace * dt) as f64 + 0.5);
        self.z = p1 - 0.5;
        if matches!(self.phase, Phase::Playing | Phase::Attract) {
            for i in (p0 / 2.0).floor() as i64..=(p1 / 2.0).floor() as i64 {
                let z0 = i as f64 * 2.0;
                let hits = |a: f64, b: f64| p1 >= z0 + a && p0 <= z0 + b;
                let near = |lane: i32, reach: f32| (self.x - lane as f32 * LANE).abs() < reach;
                let kind = row(i);
                let damage = match kind {
                    Row::Pillars(mask) if hits(0.4, 1.6) && (-1..=1).any(|lane| mask & lane_bit(lane) != 0 && near(lane, 1.0)) => 0.2,
                    Row::Beam if hits(0.85, 1.15) && self.y < 0.75 => 0.1,
                    Row::Lava if hits(0.15, 1.85) && self.y < 0.05 => 0.1,
                    // The ghost is not collected: a surge or the grace after a hit passes through it,
                    // and the one that takes the run stays over the fallen runner.
                    Row::Power(lane, GHOST) if hits(0.5, 1.5) && near(lane, 0.9) && self.y < 1.7 => 0.3,
                    Row::Orb(lane) | Row::Power(lane, _) if hits(0.5, 1.5) && near(lane, 0.9) && self.y < 1.7 && !self.collected.contains(&i) => {
                        if self.collected.len() >= 64 {
                            self.collected.remove(0);
                        }
                        self.collected.push(i);
                        match kind {
                            Row::Power(_, HEALTH) => (self.health, self.heal) = ((self.health + 0.15).min(1.0), 1.0),
                            Row::Power(_, SHIELD) => self.shield = SHIELD_SECONDS,
                            Row::Power(..) => (self.surge, self.warp) = (4.0, 1.0),
                            _ => self.take_orb(i),
                        }
                        if let Row::Power(_, power) = kind {
                            self.pickup = Some(power);
                        }
                        0.0
                    }
                    _ => 0.0,
                };
                // The title's run cannot be hurt, a surge goes through everything.
                let shielded = self.shield > 0.0 && matches!(kind, Row::Pillars(_) | Row::Power(_, GHOST));
                if damage > 0.0 && self.phase == Phase::Playing && self.invulnerable <= 0.0 && self.surge <= 0.0 && shielded {
                    // The shield takes a pillar or a ghost (not a beam, not lava): nothing lost, no
                    // stumble, a golden burst.
                    (self.shield, self.shield_burst, self.invulnerable, self.shake) = (0.0, 1.0, 0.8, 0.4);
                } else if damage > 0.0 && self.phase == Phase::Playing && self.invulnerable <= 0.0 && self.surge <= 0.0 {
                    self.health -= damage;
                    self.hits += 1;
                    (self.flash, self.shake, self.invulnerable) = (0.4 + damage * 2.0, 0.5 + damage * 2.5, 0.8);
                    if let Row::Power(lane, GHOST) = kind {
                        // The ghost: a blackout, no red flash. When it takes the last of the health it
                        // stays in front of the camera, over the runner going down.
                        (self.ghost, self.flash, self.shake, self.warp) = (1.0, 0.0, 1.2, 1.0);
                        if self.health < 0.005 {
                            self.killer = Some(i);
                            (self.z, self.lane, self.x, self.pace) = (z0 - 2.2, lane, lane as f32 * LANE, 0.0);
                        } else {
                            self.pace = 0.6;
                        }
                    } else if let Row::Pillars(mask) = kind {
                        // A pillar stops the run: thrown back in front of it and aside into the
                        // nearest free lane, then up to speed again.
                        self.pace = 0.0;
                        self.z = z0 - 0.7;
                        if let Some(free) = (-1..=1).filter(|lane| mask & lane_bit(*lane) == 0).min_by_key(|lane| (lane - self.lane).abs()) {
                            self.lane = free;
                        }
                    } else {
                        self.pace = 0.6;
                    }
                    if self.health < 0.005 {
                        self.die();
                    }
                    if self.pace == 0.0 {
                        break;
                    }
                }
            }
        }

        let zone = (self.z / ZONE).floor() as i64;
        if zone != self.zone {
            (self.zone, self.warp) = (zone, 1.0);
        }
    }

    /// The title's player: follows the orbs (their lane is the one the next pillars leave free)
    /// and jumps what must be jumped.
    fn pilot(&mut self, dt: f32) -> Input {
        let mut input = Input::default();
        self.pilot_wait -= dt;
        // Inside an event row it still wants the lane that row leaves free.
        let group = ((self.z / 2.0).floor() as i64 - 1).div_euclid(5);
        let event = (group + 1) * 5;
        let distance = (event as f64 * 2.0 + 1.0 - self.z) as f32;
        if matches!(row(event), Row::Beam | Row::Lava) && distance > 0.0 && distance < self.speed * 0.36 + 0.3 {
            input.jump = true;
        }
        let mut want = orb_lane(group);
        // A ghost, or a shield it has no use for, in the wanted lane: the next lane over until it
        // is passed.
        let avoid = group * 5 + 3;
        if let Row::Power(lane, GHOST | SHIELD) = row(avoid)
            && lane == want
            && self.z < avoid as f64 * 2.0 + 1.5
        {
            want = if want == 1 { 0 } else { want + 1 };
        }
        if want != self.lane && self.pilot_wait <= 0.0 {
            (input.left, input.right) = (want < self.lane, want > self.lane);
            self.pilot_wait = 0.16;
        }
        input
    }

    /// The color of the torches where the runner is: the light of the zone.
    pub fn light(&self) -> [f32; 3] {
        self.palette()[0]
    }

    /// The color of the orbs where the runner is (the zone's accent, channels turned: `uAccent.gbr`).
    pub fn orb_color(&self) -> [f32; 3] {
        let accent = self.palette()[1];
        [accent[1], accent[2], accent[0]]
    }

    /// Torch, accent and fog of where the runner is; the last tenth of a zone turns into the next.
    fn palette(&self) -> [[f32; 3]; 3] {
        let at = self.z / ZONE;
        let zone = at.floor();
        let t = (((at - zone) as f32 - 0.9) / 0.1).clamp(0.0, 1.0);
        let (a, b) = (PALETTES[zone as usize % PALETTES.len()], PALETTES[(zone as usize + 1) % PALETTES.len()]);
        std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] + (b[i][j] - a[i][j]) * t))
    }
}

/// The drawn dungeon. The game loop steps `world` and marks the control to draw again.
pub struct Scene {
    pub world: World,
    spec: OnceCell<Option<MeshSpecification>>,
    /// The frame's vertices, kept for its allocation.
    vertices: RefCell<Vec<f32>>,
}

impl Scene {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<Scene> {
        Build::new(Scene { world: World::default(), spec: OnceCell::new(), vertices: RefCell::new(Vec::with_capacity(4096 * FLOATS)) })
    }
}

/// One line per problem, not one per frame.
fn report(what: &str, error: &str) {
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        eprintln!("dungeon: {what}: {error}");
    }
}

fn bytes(floats: &[f32]) -> &[u8] {
    // SAFETY: any f32 is 4 valid bytes, and u8 has no alignment.
    unsafe { std::slice::from_raw_parts(floats.as_ptr().cast(), std::mem::size_of_val(floats)) }
}

impl Control for Scene {
    fn paint(&self, cx: &mut PaintCx) {
        let spec = self.spec.get_or_init(|| {
            let attributes = [
                Attribute::new(attribute::Type::Float2, 0, "position"),
                Attribute::new(attribute::Type::Float4, 8, "a"),
                Attribute::new(attribute::Type::Float4, 24, "b"),
                Attribute::new(attribute::Type::Float4, 40, "c"),
            ];
            let varyings = [
                Varying::new(varying::Type::Float4, "a"),
                Varying::new(varying::Type::Float4, "b"),
                Varying::new(varying::Type::Float4, "c"),
            ];
            MeshSpecification::make(&attributes, FLOATS * 4, &varyings, MESH_VS, MESH_FS).map_err(|e| report("mesh SkSL", &e)).ok()
        });
        let Some(spec) = spec else { return };
        let (world, rect) = (&self.world, cx.rect);
        let [torch, accent, fog] = world.palette();
        // A ghost near: the torches dim, the fog thickens and darkens. The exit's daylight: from
        // 600 units before it, the whole corridor brightens toward it (ambient, torches, the far
        // end), the walls and the floor catch the light from the door; stronger out of the door.
        let dim = 1.0 - 0.55 * world.dread;
        let exit_light = ((world.z - (EXIT - 600.0)) / 600.0).clamp(0.0, 1.0) as f32 * (1.0 + 0.8 * world.escape);
        let far = (exit_light * 0.9).min(1.0);
        let sky = [1.0, 0.9, 0.7];
        let fog = [fog[0] * dim * (1.0 - far) + sky[0] * far, fog[1] * dim * (1.0 - far) + sky[1] * far, fog[2] * dim * (1.0 - far) + sky[2] * far];

        // What no quad covers is the fog at the end of the corridor (the sky past the exit): the
        // same color the shader fogs to, so the last drawn row has no seam.
        let mut paint = Paint::default();
        paint.set_color(Color::from_argb(255, (fog[0].min(1.0) * 255.0) as u8, (fog[1].min(1.0) * 255.0) as u8, (fog[2].min(1.0) * 255.0) as u8));
        cx.canvas.draw_rect(rect, &paint);

        let mut vertices = self.vertices.borrow_mut();
        vertices.clear();
        let base = (world.z / REBASE).floor() * REBASE;
        let on_ground = if world.y <= 0.0 { 1.0 } else { 0.0 };
        let camera = [
            world.x * 0.92 + (world.time * 90.0).sin() * 0.12 * world.shake,
            // Going down, the camera sinks to the floor and tips over.
            1.55 - 1.2 * world.fall * world.fall + world.y * 0.9 + (world.z as f32 * 1.1).sin() * 0.05 * on_ground + (world.time * 70.0).cos() * 0.1 * world.shake,
            (world.z - base) as f32,
        ];
        let d = world.z as f32;
        // The corridor straightens over 600 to 300 units before the exit: from there the way out
        // is dead ahead, and the long stretch to it is drawn as straight quads.
        let straight = ((EXIT - EXIT_SIGHT - world.z) / 300.0).clamp(0.0, 1.0) as f32;
        let bend = (
            (0.0032 * (d * 0.011).sin() + 0.0016 * (d * 0.027 + 1.3).sin()) * straight,
            (0.0018 * (d * 0.007 + 0.5).sin() - 0.0006) * straight,
        );
        let mut builder = Builder {
            out: &mut vertices,
            camera,
            bend,
            roll: (world.lean + bend.0 * 18.0 + 0.6 * world.fall * world.fall).sin_cos(),
            center: (rect.center_x(), rect.center_y()),
            focal: (rect.height() * 0.5).min(rect.width() * 0.75),
            bounds: Rect::new(f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        };
        builder.corridor(world, base);
        let bounds = builder.bounds;
        let count = vertices.len() / FLOATS;
        if count == 0 {
            return;
        }
        let Some(buffer) = meshes::make_vertex_buffer(bytes(&vertices)) else { return };
        let bright = dim * (1.0 + 0.7 * exit_light);
        // Near the exit the air clears (daylight), so the opening reads from far.
        let ramp = ((world.z - (EXIT - 600.0)) / 600.0).clamp(0.0, 1.0) as f32;
        let uniforms = [
            world.time, camera[0], camera[1], camera[2],
            torch[0] * bright, torch[1] * bright, torch[2] * bright, (EXIT - base) as f32,
            accent[0], accent[1], accent[2], exit_light,
            fog[0], fog[1], fog[2], 0.028 * (1.0 - 0.4 * ramp) + 0.08 * world.dread,
            world.glare, 0.0, 0.0, 0.0,
        ];
        let mesh = Mesh::make(spec.clone(), Mode::Triangles, buffer, count, 0, Data::new_copy(bytes(&uniforms)), &[], bounds);
        let mesh = match mesh {
            Ok(mesh) => mesh,
            Err(e) => return report("mesh", &e),
        };
        // The mesh color is multiplied by the paint color.
        paint.set_color(Color::WHITE);
        cx.canvas.save();
        cx.canvas.clip_rect(rect, None, None);
        cx.canvas.draw_mesh(&mesh, None::<Blender>, &paint);
        cx.canvas.restore();
    }
}

/// Turns world quads into the frame's triangles.
struct Builder<'a> {
    out: &'a mut Vec<f32>,
    /// In the shader's (rebased) coordinates.
    camera: [f32; 3],
    /// Sideways and upward bend per squared unit of depth.
    bend: (f32, f32),
    /// Sine and cosine of the camera roll.
    roll: (f32, f32),
    center: (f32, f32),
    focal: f32,
    bounds: Rect,
}

impl Builder<'_> {
    /// A quad given by its corners (uv 0,0 / 1,0 / 1,1 / 0,1): clipped at the near plane, projected.
    fn quad(&mut self, corners: [[f32; 3]; 4], material: f32, glow: [f32; 4], normal: [f32; 3], seed: f32) {
        const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        // Per corner: view xyz, world xyz, uv, glow.
        let mut view = [[0f32; 9]; 4];
        for (k, p) in corners.iter().enumerate() {
            let (dx, dy, dz) = (p[0] - self.camera[0], p[1] - self.camera[1], p[2] - self.camera[2]);
            let depth = dz.max(0.0) * dz.max(0.0);
            let (bx, by) = (dx + self.bend.0 * depth, dy + self.bend.1 * depth);
            let (sin, cos) = self.roll;
            view[k] = [bx * cos - by * sin, bx * sin + by * cos, dz, p[0], p[1], p[2], UV[k][0], UV[k][1], glow[k]];
        }
        if view.iter().all(|v| v[2] < NEAR) {
            return;
        }
        let mut polygon = [[0f32; 9]; 6];
        let mut n = 0;
        for k in 0..4 {
            let (a, b) = (view[k], view[(k + 1) % 4]);
            if a[2] >= NEAR {
                polygon[n] = a;
                n += 1;
            }
            if (a[2] >= NEAR) != (b[2] >= NEAR) {
                let t = (NEAR - a[2]) / (b[2] - a[2]);
                polygon[n] = std::array::from_fn(|j| a[j] + (b[j] - a[j]) * t);
                n += 1;
            }
        }
        for k in 1..n.saturating_sub(1) {
            for v in [polygon[0], polygon[k], polygon[k + 1]] {
                let w = 1.0 / v[2];
                let (x, y) = (self.center.0 + self.focal * v[0] * w, self.center.1 - self.focal * v[1] * w);
                self.bounds = Rect::new(self.bounds.left.min(x), self.bounds.top.min(y), self.bounds.right.max(x), self.bounds.bottom.max(y));
                self.out.extend_from_slice(&[
                    x, y,
                    v[3] * w, v[4] * w, v[5] * w, w,
                    v[6] * w, v[7] * w, material, v[8] * w,
                    normal[0], normal[1], normal[2], seed,
                ]);
            }
        }
    }

    /// A sprite facing the run.
    fn sprite(&mut self, x: f32, y: (f32, f32), z: f32, half_width: f32, material: f32, seed: f32) {
        let (x0, x1) = (x - half_width, x + half_width);
        self.quad([[x0, y.0, z], [x1, y.0, z], [x1, y.1, z], [x0, y.1, z]], material, [0.0; 4], [0.0, 0.0, -1.0], seed);
    }

    /// Far to near: each segment's floor, ceiling and walls, then what stands in it.
    fn corridor(&mut self, world: &World, base: f64) {
        const W: f32 = HALF_WIDTH;
        const H: f32 = HEIGHT;
        let first = (world.z / 2.0).floor() as i64;
        // The way out, once in sight: the corridor simply ends, open. Behind the opening, the
        // world outside (one wide quad far off: sky, sun, clouds, mountains in the shader) and the
        // stone floor going on as a terrace; the stretch of corridor past the drawn rows up to the
        // door as four long quads (the corridor is straight by then).
        let door = EXIT_ROW as f64 * 2.0 - base;
        if world.z >= EXIT - EXIT_SIGHT {
            let sky = (door + 90.0) as f32;
            self.quad([[-400.0, -40.0, sky], [400.0, -40.0, sky], [400.0, 200.0, sky], [-400.0, 200.0, sky]], 13.0, [0.0; 4], [0.0, 0.0, -1.0], 0.0);
            let (t0, t1) = (door as f32, (door + 24.0) as f32);
            self.quad([[-30.0, 0.0, t0], [30.0, 0.0, t0], [30.0, 0.0, t1], [-30.0, 0.0, t1]], 0.0, [0.0; 4], [0.0, 1.0, 0.0], 0.0);
            let near = first + VISIBLE;
            if near < EXIT_ROW {
                let (z0, z1) = ((near as f64 * 2.0 - base) as f32, door as f32);
                self.quad([[-W, 0.0, z0], [W, 0.0, z0], [W, 0.0, z1], [-W, 0.0, z1]], 0.0, [0.0; 4], [0.0, 1.0, 0.0], 0.0);
                self.quad([[-W, H, z0], [W, H, z0], [W, H, z1], [-W, H, z1]], 1.0, [0.0; 4], [0.0, -1.0, 0.0], 0.0);
                self.quad([[-W, 0.0, z0], [-W, 0.0, z1], [-W, H, z1], [-W, H, z0]], 2.0, [0.0; 4], [1.0, 0.0, 0.0], 0.0);
                self.quad([[W, 0.0, z0], [W, 0.0, z1], [W, H, z1], [W, H, z0]], 2.0, [0.0; 4], [-1.0, 0.0, 0.0], 0.0);
            }
        }
        for i in (first..first + VISIBLE).rev() {
            if i >= EXIT_ROW {
                continue;
            }
            let z0 = (i as f64 * 2.0 - base) as f32;
            let z1 = z0 + SEG;
            let (g0, g1) = (lava_glow(i), lava_glow(i + 1));
            let kind = row(i);
            let floor = if kind == Row::Lava { 3.0 } else { 0.0 };
            self.quad([[-W, 0.0, z0], [W, 0.0, z0], [W, 0.0, z1], [-W, 0.0, z1]], floor, [g0 * 0.6, g0 * 0.6, g1 * 0.6, g1 * 0.6], [0.0, 1.0, 0.0], 0.0);
            self.quad([[-W, H, z0], [W, H, z0], [W, H, z1], [-W, H, z1]], 1.0, [g0 * 0.06, g0 * 0.06, g1 * 0.06, g1 * 0.06], [0.0, -1.0, 0.0], 0.0);
            let wall = [g0, g1, g1 * 0.1, g0 * 0.1];
            self.quad([[-W, 0.0, z0], [-W, 0.0, z1], [-W, H, z1], [-W, H, z0]], 2.0, wall, [1.0, 0.0, 0.0], 0.0);
            self.quad([[W, 0.0, z0], [W, 0.0, z1], [W, H, z1], [W, H, z0]], 2.0, wall, [-1.0, 0.0, 0.0], 0.0);

            // A torch every fifth segment, on alternating walls (the shader lights from the same places).
            if i.rem_euclid(5) == 0 {
                let k = i.div_euclid(5);
                let side = if k.rem_euclid(2) == 0 { 1.0 } else { -1.0 };
                self.sprite(side * 2.62, (2.5, 4.3), z0 + 0.05, 0.75, 6.0, hash01(k, 3));
            }
            match kind {
                Row::Pillars(mask) => {
                    // The pillar farthest to the side first.
                    let mut lanes = [-1, 0, 1];
                    let off = |lane: i32| (lane as f32 * LANE - self.camera[0]).abs();
                    lanes.sort_by(|a, b| off(*b).total_cmp(&off(*a)));
                    for lane in lanes {
                        if mask & lane_bit(lane) == 0 {
                            continue;
                        }
                        let (x0, x1) = (lane as f32 * LANE - 0.85, lane as f32 * LANE + 0.85);
                        let (za, zb) = (z0 + 0.4, z0 + 1.6);
                        let seed = hash01(i * 4 + lane as i64, 4);
                        if self.camera[0] < x0 {
                            self.quad([[x0, 0.0, zb], [x0, 0.0, za], [x0, H, za], [x0, H, zb]], 4.0, [0.0; 4], [-1.0, 0.0, 0.0], seed);
                        }
                        if self.camera[0] > x1 {
                            self.quad([[x1, 0.0, za], [x1, 0.0, zb], [x1, H, zb], [x1, H, za]], 4.0, [0.0; 4], [1.0, 0.0, 0.0], seed);
                        }
                        self.quad([[x0, 0.0, za], [x1, 0.0, za], [x1, H, za], [x0, H, za]], 4.0, [0.0; 4], [0.0, 0.0, -1.0], seed);
                    }
                }
                Row::Beam => self.sprite(0.0, (0.1, 1.0), z0 + 1.0, W, 7.0, 0.0),
                Row::Power(lane, GHOST) => {
                    // Far: a faint shimmer. Under a second away it reveals itself (`reveal`, 0 to 1:
                    // eyes open, it swells and lunges at the camera). The one that took the run
                    // leans in over the fallen camera (1 + `fall`). The glow slot carries the value.
                    let distance = (i as f64 * 2.0 + 1.0 - world.z) as f32;
                    let reveal = (1.0 - distance / (world.speed.max(8.0) * 0.9)).clamp(0.0, 1.0);
                    let (state, lunge, loom) = if world.killer == Some(i) { (1.0 + world.fall, 0.0, world.fall) } else { (reveal, reveal * reveal, 0.0) };
                    let y = 1.35 + 0.1 * (world.time * 1.3 + i as f32).sin() - 0.3 * lunge - 0.75 * loom;
                    let half = 1.0 + 0.5 * lunge + 0.2 * loom;
                    let (x0, x1) = (lane as f32 * LANE - half, lane as f32 * LANE + half);
                    let z = z0 + 1.0 - 0.6 * lunge;
                    self.quad([[x0, y - half, z], [x1, y - half, z], [x1, y + half, z], [x0, y + half, z]], 10.0, [state; 4], [0.0, 0.0, -1.0], hash01(i, 5));
                }
                Row::Power(lane, power) if !world.collected.contains(&i) => {
                    let y = 1.05 + 0.12 * (world.time * 3.0 + i as f32).sin();
                    let material = if power == SHIELD { 14.0 } else { 8.0 + power as f32 };
                    self.sprite(lane as f32 * LANE, (y - 0.75, y + 0.75), z0 + 1.0, 0.75, material, hash01(i, 5));
                }
                Row::Orb(lane) if !world.collected.contains(&i) => {
                    let y = 1.0 + 0.12 * (world.time * 3.0 + i as f32).sin();
                    self.sprite(lane as f32 * LANE, (y - 0.6, y + 0.6), z0 + 1.0, 0.6, 5.0, hash01(i, 5));
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The title's player survives by the rules a real player dies by, at the speeds it runs at.
    #[test]
    fn the_pilot_survives() {
        let mut world = World::default();
        world.start();
        for _ in 0..60 * 30 {
            let mut input = world.pilot(1.0 / 60.0);
            world.step(1.0 / 60.0, &mut input);
            assert_eq!(world.health, 1.0, "hit at {} (row {:?})", world.z, row((world.z / 2.0) as i64));
        }
        assert!(world.orbs > 20, "orbs {}", world.orbs);
    }

    /// Pillars never close the lane the orbs before them lead to.
    #[test]
    fn pillars_leave_the_orb_lane_free() {
        for i in 0..5000 {
            if let Row::Pillars(mask) = row(i * 5) {
                assert_eq!(mask & lane_bit(orb_lane(i - 1)), 0);
                assert_ne!(mask, 0b111);
            }
        }
    }
}
