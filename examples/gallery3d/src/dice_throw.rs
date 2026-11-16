//! A throw of two dice in physics, stepped along as it is shown (the .NET gallery's DiceThrow):
//! rigid cubes with rounded corners (edge 1, mass 1) thrown across the felt, falling under
//! gravity, bouncing and sliding on their corners with restitution and Coulomb friction, kept on
//! the felt by its edges and bumping into each other, until both lie still. The face each die ends
//! on is the result, as with real dice.

use glam::{Quat, Vec2, Vec3};

use crate::Random;

#[derive(Clone, Copy, Default)]
struct Body {
    position: Vec3,
    turn: Quat,
    velocity: Vec3,
    spin: Vec3,
    /// Time it has lain on the felt barely moving; past a moment it rests and moves no more.
    quiet: f32,
    /// Time it has stood still on an edge or a corner: past a moment it tips over, as a real die does.
    propped: f32,
    resting: bool,
}

pub const STEP: f32 = 1.0 / 360.0;
/// Gravity in die edges per second squared: as for a die of about 20 cm, which a die on this felt
/// reads as, so it falls and rolls at the pace the eye expects.
const GRAVITY: f32 = 50.0;
pub const TABLE: f32 = -0.5;
/// How far the dice centers may go (die edges from the middle), so they stay in view on narrow
/// screens too. The felt shows no rail there, so a die reaching it only loses its way out, and
/// never leans on it.
pub const EDGE_X: f32 = 1.15;
pub const EDGE_Z: f32 = 0.5;
const RESTITUTION: f32 = 0.5;
const FRICTION: f32 = 0.3;
const THROW_UP: f32 = 3.6;
const THROW_ACROSS: f32 = 3.0;
const SPIN_MIN: f32 = 16.0;
/// Cube, mass 1, edge 1: inertia 1/6.
const INVERSE_INERTIA: f32 = 6.0;
/// Dice centers keep this apart: faces never sink into each other.
const BUMP: f32 = 1.25;

pub struct DiceThrow {
    radius: f32,
    corners: [Vec3; 8],
    dice: [Body; 2],
    time: f32,
    owed: f32,
    /// Both dice lie still; `faces` holds the result.
    pub done: bool,
    pub faces: [usize; 2],
}

impl DiceThrow {
    /// `rounding`: the dice's corner radius in die edges.
    pub fn new(rounding: f32) -> DiceThrow {
        let core = 0.5 - rounding;
        let corners = std::array::from_fn(|i| Vec3::new(if i & 1 == 0 { -core } else { core }, if i & 2 == 0 { -core } else { core }, if i & 4 == 0 { -core } else { core }));
        DiceThrow { radius: rounding, corners, dice: [Body::default(); 2], time: 0.0, owed: 0.0, done: true, faces: [1, 1] }
    }

    pub fn position(&self, die: usize) -> Vec3 {
        self.dice[die].position
    }

    pub fn turn(&self, die: usize) -> Quat {
        self.dice[die].turn
    }

    /// Throws both dice from where they lie: up, across the felt (`direction`: -1 left, 1 right,
    /// 0 either) and spinning around a random axis. `run` then moves them on.
    pub fn throw(&mut self, at: [Vec3; 2], turn: [Quat; 2], direction: i32, random: &mut Random) {
        let direction = if direction == 0 { (random.next() * 2.0) as i32 * 2 - 1 } else { direction } as f32;
        // The die ahead goes faster than the one behind it, so they rarely run into each other and end apart.
        let first_ahead = (at[0].x - at[1].x) * direction > 0.0;
        self.dice[0] = thrown(at[0], turn[0], -0.85, direction, if first_ahead { 1.0 } else { 0.55 }, random);
        self.dice[1] = thrown(at[1], turn[1], 0.85, direction, if first_ahead { 0.55 } else { 1.0 }, random);
        (self.time, self.owed) = (0.0, 0.0);
        self.done = false;
    }

    /// Moves the throw on by `seconds` of real time, in fixed physics steps; when both dice lie
    /// still (or after 4 s) each is laid exactly flat and `done` turns true.
    pub fn run(&mut self, seconds: f32) {
        if self.done {
            return;
        }
        // A long frame never runs away with the physics.
        self.owed = (self.owed + seconds).min(0.1);
        while self.owed >= STEP && !self.done {
            self.owed -= STEP;
            self.time += STEP;
            self.advance();
            if self.dice[0].resting && self.dice[1].resting || self.time >= 4.0 {
                for i in 0..2 {
                    self.faces[i] = settle(&mut self.dice[i]);
                }
                self.done = true;
            }
        }
    }

    fn advance(&mut self) {
        let mut touching = [0; 2];
        let mut corners = [Vec3::ZERO; 16];
        for i in 0..2 {
            let d = &mut self.dice[i];
            d.velocity.y -= GRAVITY * STEP;
            // The corners do not move while the contacts are solved: turned once per step.
            for c in 0..8 {
                corners[i * 8 + c] = d.position + d.turn * self.corners[c];
            }
            for c in &corners[i * 8..i * 8 + 8] {
                if c.y - self.radius - TABLE < 0.02 {
                    touching[i] += 1;
                }
            }
        }

        // A few passes over the corner contacts, so a die landing on an edge or a face shares the impulse.
        for pass in 0..3 {
            for i in 0..2 {
                if self.dice[i].resting {
                    continue;
                }
                for corner in corners[i * 8..i * 8 + 8].iter().copied() {
                    let depth = TABLE - (corner.y - self.radius);
                    if depth < -0.02 {
                        continue;
                    }
                    // A corner a hair above the felt may come only that close this step; one sunk in is eased out.
                    let allowed = if depth < 0.0 { depth / STEP } else { (depth - 0.002).max(0.0) * 0.2 / STEP };
                    touch(&mut self.dice[i], corner - Vec3::new(0.0, self.radius, 0.0), Vec3::Y, allowed);
                }
            }

            // The two dice bump as two balls.
            let between = self.dice[1].position - self.dice[0].position;
            let distance = between.length();
            if distance < BUMP && distance > 1e-4 {
                let n = between / distance;
                let closing = (self.dice[1].velocity - self.dice[0].velocity).dot(n);
                if closing < 0.0 {
                    let j = -(1.0 + 0.4) * closing / 2.0;
                    self.dice[0].velocity -= j * n;
                    self.dice[1].velocity += j * n;
                    (self.dice[0].resting, self.dice[1].resting) = (false, false);
                }
                if pass == 0 {
                    let apart = (BUMP - distance) / 2.0 * n;
                    self.dice[0].position -= apart;
                    self.dice[1].position += apart;
                }
            }
        }

        for i in 0..2 {
            let time = self.time;
            let d = &mut self.dice[i];
            if d.resting {
                (d.velocity, d.spin) = (Vec3::ZERO, Vec3::ZERO);
                continue;
            }
            // Flat on the felt and barely moving for a moment: it rests (a die never creeps on its own).
            let quiet = touching[i] >= 3 && d.velocity.length_squared() < 0.15 * 0.15 && d.spin.length_squared() < 0.6 * 0.6;
            d.quiet = if quiet { d.quiet + STEP } else { 0.0 };
            // Balanced on an edge, which a real die never manages for long: the smallest touch tips it.
            let propped = touching[i] < 3 && d.position.y < 0.3 && d.velocity.length_squared() < 0.25 * 0.25 && d.spin.length_squared() < 0.6 * 0.6;
            d.propped = if propped { d.propped + STEP } else { 0.0 };
            if d.propped > 0.2 {
                let a = time * 7.0 + i as f32;
                d.spin += Vec3::new(a.cos(), 0.0, a.sin()) * 1.5;
                d.propped = 0.0;
            }
            if d.quiet > 0.15 {
                d.resting = true;
                (d.velocity, d.spin) = (Vec3::ZERO, Vec3::ZERO);
                continue;
            }
            // Only while it flies or rolls fast: a die slowly tipping over is left alone.
            if d.velocity.length_squared() > 1.0 {
                edge(&mut d.position.x, &mut d.velocity.x, EDGE_X);
                edge(&mut d.position.z, &mut d.velocity.z, EDGE_Z);
            }
            d.position += d.velocity * STEP;
            let w = d.spin * STEP * 0.5;
            d.turn = (d.turn + Quat::from_xyzw(w.x, w.y, w.z, 0.0) * d.turn).normalize();
        }
    }
}

fn thrown(at: Vec3, turn: Quat, home: f32, direction: f32, pace: f32, random: &mut Random) -> Body {
    let mut range = |a: f32, b: f32| a + random.next() * (b - a);
    // Both the same way, as from one hand, each toward a spot of its own half of the felt (so they
    // end apart and never live at one edge over many throws), at its own pace.
    let target = Vec2::new(home + direction * range(0.0, 0.3), range(-0.35, 0.35));
    let across = direction * pace * range(THROW_ACROSS * 0.7, THROW_ACROSS) + (target.x - at.x) * 2.2;
    let axis = Vec3::new(range(-1.0, 1.0), range(-0.4, 0.4), range(-1.0, 1.0)).normalize();
    let up = range(THROW_UP, THROW_UP * 1.3);
    let spin = range(SPIN_MIN, SPIN_MIN * 1.5);
    Body { position: at, turn, velocity: Vec3::new(across, up, (target.y - at.z) * 2.2), spin: axis * spin, ..Body::default() }
}

/// The end of the room a die has: past it, its way further out is braked hard (no bounce off a
/// rail nobody sees). Nothing pushes it back, so a die there still tips over freely and never
/// stays propped on an edge.
fn edge(position: &mut f32, velocity: &mut f32, limit: f32) {
    if *position > limit && *velocity > 0.0 || *position < -limit && *velocity < 0.0 {
        *velocity *= 0.8;
    }
}

/// A corner touching the felt: an impulse that stops it going in (bouncing a part back when
/// fast), then friction.
fn touch(d: &mut Body, point: Vec3, n: Vec3, allowed: f32) {
    let r = point - d.position;
    let velocity = d.velocity + d.spin.cross(r);
    let vn = velocity.dot(n);
    let target = if vn < -1.5 { (-RESTITUTION * vn).max(allowed) } else { allowed };
    if vn >= target {
        return;
    }
    let rn = r.cross(n);
    let j = (target - vn) / (1.0 + INVERSE_INERTIA * rn.length_squared());
    d.velocity += j * n;
    d.spin += INVERSE_INERTIA * r.cross(j * n);

    let velocity = d.velocity + d.spin.cross(r);
    let slide = velocity - velocity.dot(n) * n;
    let speed = slide.length();
    if speed < 1e-5 {
        return;
    }
    let t = slide / speed;
    let rt = r.cross(t);
    let jt = (speed / (1.0 + INVERSE_INERTIA * rt.length_squared())).min(FRICTION * j);
    d.velocity -= jt * t;
    d.spin -= INVERSE_INERTIA * r.cross(jt * t);
}

const FACE_AXES: [(Vec3, usize); 6] = [(Vec3::X, 3), (Vec3::NEG_X, 4), (Vec3::Y, 1), (Vec3::NEG_Y, 6), (Vec3::Z, 2), (Vec3::NEG_Z, 5)];

/// Lays a die exactly flat on the face nearest the felt (keeping its turn around the vertical);
/// its face up.
fn settle(d: &mut Body) -> usize {
    let mut best = FACE_AXES[0];
    let mut height = f32::MIN;
    for f in FACE_AXES {
        let y = (d.turn * f.0).y;
        if y > height {
            (height, best) = (y, f);
        }
    }
    let up = (d.turn * best.0).normalize();
    let axis = up.cross(Vec3::Y);
    if axis.length_squared() > 1e-10 {
        d.turn = (Quat::from_axis_angle(axis.normalize(), up.y.clamp(-1.0, 1.0).acos()) * d.turn).normalize();
    }
    d.position.y = 0.0;
    (d.velocity, d.spin) = (Vec3::ZERO, Vec3::ZERO);
    best.1
}
