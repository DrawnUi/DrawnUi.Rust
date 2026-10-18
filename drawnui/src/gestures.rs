//! Pointer input turned into gestures: Down, Panning, Tapped, Up, LongPressing, the hover
//! (Pointer) family, the context menu, plus the mouse wheel.

use skia_safe::Point;

use crate::PointerKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GestureKind {
    Down,
    Panning,
    Tapped,
    Up,
    Wheel,
    /// The pointer stayed down without moving for `LONG_PRESS_MS` (DrawnUI LongPressing). Routed
    /// by the press location; the press goes on, its Up follows later.
    LongPressing,
    /// The mouse moves over the control with no button down (DrawnUI TouchActionResult.Pointer).
    /// Routed by position through every control under the pointer; a control that returns
    /// `Handled::Yes` keeps it from its children.
    Pointer,
    /// The pointer came over the control: it is now on the path of `Pointer` gestures. Delivered
    /// to the control alone, not routed (DrawnUI IsPointerOver = true).
    PointerEnter,
    /// The pointer left the control, or the window (DrawnUI IsPointerOver = false).
    PointerExit,
    /// A context menu was asked for at `location`: a right click, a long press on touch, the Menu
    /// key (React ContextMenuEventArgs). Routed like a tap: the deepest control with a
    /// `on_context_menu` handler takes it. `Gesture::source` says where it came from.
    ContextMenu,
}

/// Which mouse button pressed (AppoMobi.Gestures MouseButton). A touch or pen is `Left`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MouseButton {
    #[default]
    Left,
    Middle,
    Right,
    /// The browser back button (XButton1).
    Back,
    /// The browser forward button (XButton2).
    Forward,
}

/// Where a context menu request came from (React ContextMenuSource).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ContextMenuSource {
    #[default]
    Mouse,
    Touch,
    Keyboard,
}

/// One gesture event. Locations and distances are canvas pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Gesture {
    pub kind: GestureKind,
    pub location: Point,
    /// Where the pointer went down.
    pub start: Point,
    /// Movement since the previous event.
    pub delta: Point,
    /// Movement since Down.
    pub total: Point,
    /// When the input arrived, milliseconds on the frame clock.
    pub time_ms: f64,
    /// Pointer speed in pixels per second: of this move for Panning, of the last move for Up.
    pub velocity: Point,
    /// Wheel only, in notches (a touchpad sends fractions). Positive = toward the start: away from
    /// the user (the content moves down), or left for a horizontal event.
    pub wheel: f32,
    /// Wheel only: a horizontal event (a tilted wheel, the sideways part of a touchpad swipe). A
    /// vertical scroll leaves it to the scroll around it; a horizontal one takes both kinds.
    pub wheel_horizontal: bool,
    /// Up only: the press did not end, it was taken away (the browser took the touch, the window
    /// lost the pointer). Nothing may be started from it: no tap came before, no fling follows.
    pub cancelled: bool,
    /// The button of the press (Down, Panning, Tapped, Up, LongPressing). Every button taps: a
    /// control that wants the left one only checks it (React `Pointer.Button`).
    pub button: MouseButton,
    /// ContextMenu only.
    pub source: ContextMenuSource,
    /// The press is a finger, not a mouse or a pen (Down, Panning, Tapped, Up, LongPressing):
    /// selectable text waits for a long press there, a drag goes on to the scroll.
    pub touch: bool,
    /// Fingers down (C# NumberOfTouches): 1 for a mouse press or one finger, 0 without a press.
    pub touches: u32,
    /// Panning while two or more fingers are down: how they pinched and turned since the previous
    /// move (C# `Manipulation`). `None` for one finger.
    pub manipulation: Option<Manipulation>,
}

/// What two or more fingers did since the previous move (AppoMobi.Gestures ManipulationInfo):
/// pinch to zoom from `scale`, turn from `rotation`. Canvas pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Manipulation {
    /// The middle of the fingers now, and at the previous move.
    pub center: Point,
    pub previous_center: Point,
    /// The fingers' spread now over the previous one, minus 1: 0.05 = 5% further apart.
    pub scale: f32,
    /// Degrees the first two fingers turned since the previous move.
    pub rotation: f32,
    /// `scale` and `rotation` added up since the finger count last changed.
    pub scale_total: f32,
    pub rotation_total: f32,
    pub touches_count: u32,
}

/// The fingers down and what they did (AppoMobi.Gestures MultitouchTracker, after Mapsui).
/// Allocates nothing: up to `MAX_FINGERS` fingers, the rest are ignored.
#[derive(Default)]
pub(crate) struct Multitouch {
    fingers: [(u64, Point); Self::MAX_FINGERS],
    len: usize,
    state: Option<TouchState>,
    scale_total: f32,
    rotation_total: f32,
}

/// Where the fingers are: their middle, the first one's distance from it, the first two's angle.
#[derive(Clone, Copy, PartialEq)]
struct TouchState {
    center: Point,
    radius: f32,
    angle: f32,
    count: usize,
}

impl Multitouch {
    const MAX_FINGERS: usize = 10;

    pub fn count(&self) -> usize {
        self.len
    }

    /// The first finger came down: everything before is forgotten.
    pub fn restart(&mut self, finger: u64, at: Point) {
        self.len = 0;
        self.set(finger, at);
        (self.scale_total, self.rotation_total) = (0.0, 0.0);
        self.state = self.measure();
    }

    /// Every finger lifted.
    pub fn reset(&mut self) {
        self.len = 0;
        self.state = None;
    }

    /// A finger came down or moved. `Some` while two or more are down and they moved.
    pub fn moved(&mut self, finger: u64, at: Point) -> Option<Manipulation> {
        self.set(finger, at);
        self.calculate()
    }

    pub fn lifted(&mut self, finger: u64) {
        if let Some(i) = self.fingers[..self.len].iter().position(|f| f.0 == finger) {
            self.fingers.copy_within(i + 1..self.len, i);
            self.len -= 1;
        }
        self.calculate();
    }

    fn set(&mut self, finger: u64, at: Point) {
        match self.fingers[..self.len].iter_mut().find(|f| f.0 == finger) {
            Some(f) => f.1 = at,
            None if self.len < Self::MAX_FINGERS => {
                self.fingers[self.len] = (finger, at);
                self.len += 1;
            }
            None => {}
        }
    }

    fn measure(&self) -> Option<TouchState> {
        let fingers = &self.fingers[..self.len];
        let first = fingers.first()?.1;
        if fingers.len() == 1 {
            return Some(TouchState { center: first, radius: 0.0, angle: 0.0, count: 1 });
        }
        let sum = fingers.iter().fold(Point::default(), |s, f| s + f.1);
        let center = Point::new(sum.x / fingers.len() as f32, sum.y / fingers.len() as f32);
        let second = fingers[1].1;
        let angle = (second.y - first.y).atan2(second.x - first.x).to_degrees();
        Some(TouchState { center, radius: (first - center).length(), angle, count: fingers.len() })
    }

    /// The change since the last call (MultitouchTracker.Calculate): a new finger count starts the
    /// totals again and reports nothing.
    fn calculate(&mut self) -> Option<Manipulation> {
        let now = self.measure();
        let previous = std::mem::replace(&mut self.state, now);
        let (now, previous) = (now?, previous?);
        if now.count < 2 {
            return None;
        }
        if now.count != previous.count {
            (self.scale_total, self.rotation_total) = (0.0, 0.0);
            return None;
        }
        let scale = if previous.radius > 0.0 && now.radius != previous.radius { now.radius / previous.radius - 1.0 } else { 0.0 };
        // The turn the shorter way: crossing ±180° is 10°, not 350° (C# reports the jump).
        let rotation = (now.angle - previous.angle + 540.0).rem_euclid(360.0) - 180.0;
        self.scale_total += scale;
        self.rotation_total += rotation;
        if now == previous {
            return None;
        }
        Some(Manipulation {
            center: now.center,
            previous_center: previous.center,
            scale,
            rotation,
            scale_total: self.scale_total,
            rotation_total: self.rotation_total,
            touches_count: now.count as u32,
        })
    }
}

/// A release closer than this to the press (points, each axis) is a tap.
pub const TAPPED_CANCEL_MOVE_THRESHOLD_POINTS: f32 = 16.0;

/// A press held this long without moving past the tap threshold is a LongPressing (DrawnUI
/// `TouchEffect.LongPressTimeMsDefault`, the desktop head; React produces none).
pub const LONG_PRESS_MS: f64 = 1500.0;

/// A move's velocity is its displacement over this much time, the position that long before
/// interpolated between the press's last positions.
const VELOCITY_WINDOW_MS: f64 = 16.0;
/// Below this since the press, no velocity is measured yet: the first moves of a burst.
const VELOCITY_MIN_MS: f64 = 4.0;
/// Moves closer than this came in one burst: they are one position, at the burst's first time.
const BURST_MS: f64 = 1.0;

/// The last positions of a press with their times. Measured only against the previous move,
/// irregular delivery made the velocity worthless: WSLg hands the app moves in clumps (two 0.01 ms
/// apart every 15 ms, then uneven gaps), which gave millions of px/s, a fling at the 3000 pt/s
/// limit every time, and with a short minimum gap still releases 40 to 60 % off the hand's speed.
/// The displacement over the last `VELOCITY_WINDOW_MS`, the earlier position interpolated, matches
/// what evenly spaced input gives (checked on logged WSLg flings against them resampled at 8 ms)
/// and is the per-move velocity for evenly spaced moves, as before.
#[derive(Clone, Copy, Default)]
struct Trail {
    points: [(Point, f64); 16],
    len: usize,
}

impl Trail {
    fn push(&mut self, location: Point, time_ms: f64) {
        if let Some(last) = self.points[..self.len].last_mut().filter(|(_, ms)| time_ms - ms < BURST_MS) {
            last.0 = location;
            return;
        }
        if self.len == self.points.len() {
            self.points.copy_within(1.., 0);
            self.len -= 1;
        }
        self.points[self.len] = (location, time_ms);
        self.len += 1;
    }

    /// The velocity at `location`, `time_ms`, pixels per second; `fallback` right after the press.
    fn velocity(&self, location: Point, time_ms: f64, fallback: Point) -> Point {
        let mut points = &self.points[..self.len];
        // A move in the burst of the last position measures from the positions before it.
        if points.last().is_some_and(|(_, ms)| time_ms - ms < BURST_MS) {
            points = &points[..points.len() - 1];
        }
        let Some(&first) = points.first() else { return fallback };
        let since = time_ms - first.1;
        if since < VELOCITY_MIN_MS {
            return fallback;
        }
        let window = VELOCITY_WINDOW_MS.min(since);
        let at = time_ms - window;
        // The previous position is at least the window old: the move's own velocity, exactly.
        let &(last, last_ms) = points.last().expect("checked above");
        if last_ms <= at {
            let secs = ((time_ms - last_ms) / 1000.0) as f32;
            return Point::new((location.x - last.x) / secs, (location.y - last.y) / secs);
        }
        // The position at `at`: between the two positions around it (the current one included).
        let (mut from, mut previous) = (first.0, first);
        for &(point, ms) in points[1..].iter().chain(std::iter::once(&(location, time_ms))) {
            if ms >= at {
                let (p0, t0) = previous;
                let share = if ms - t0 < 1e-6 { 1.0 } else { ((at - t0) / (ms - t0)) as f32 };
                from = Point::new(p0.x + (point.x - p0.x) * share, p0.y + (point.y - p0.y) * share);
                break;
            }
            previous = (point, ms);
        }
        let secs = (window / 1000.0) as f32;
        Point::new((location.x - from.x) / secs, (location.y - from.y) / secs)
    }
}

/// The pointer while it is down.
#[derive(Clone, Copy)]
struct Pressed {
    start: Point,
    previous: Point,
    velocity: Point,
    trail: Trail,
    button: MouseButton,
    down_ms: f64,
    /// The long press fired, or a move past the threshold cancelled it.
    long_press_done: bool,
    /// The long press fired: the release is no tap (AppoMobi.Gestures `!IsLongPressing`).
    long_pressed: bool,
    touch: bool,
    /// A second finger came down: no tap and no long press from this press.
    multi: bool,
}

/// Single-pointer state machine. Allocates nothing.
#[derive(Default)]
pub(crate) struct Recognizer {
    down: Option<Pressed>,
}

impl Recognizer {
    /// A pointer is pressed now.
    pub fn is_pressed(&self) -> bool {
        self.down.is_some()
    }

    /// The frame time the long press of the current press is due at, if one is still pending.
    pub fn long_press_due(&self) -> Option<f64> {
        self.down.filter(|p| !p.long_press_done).map(|p| p.down_ms + LONG_PRESS_MS)
    }

    /// The LongPressing gesture when the press was held long enough at `time_ms`; fires once.
    pub fn long_press(&mut self, time_ms: f64) -> Option<Gesture> {
        let pressed = self.down.as_mut().filter(|p| !p.long_press_done && time_ms >= p.down_ms + LONG_PRESS_MS)?;
        (pressed.long_press_done, pressed.long_pressed) = (true, true);
        let (start, button, touch) = (pressed.start, pressed.button, pressed.touch);
        let zero = Point::default();
        Some(Gesture {
            kind: GestureKind::LongPressing,
            location: start,
            start,
            delta: zero,
            total: zero,
            time_ms,
            velocity: zero,
            wheel: 0.0,
            wheel_horizontal: false,
            cancelled: false,
            button,
            source: ContextMenuSource::Touch,
            touch,
            touches: 1,
            manipulation: None,
        })
    }

    /// A second finger came down during the press: it is no tap and no long press any more.
    pub fn multi_touch(&mut self) {
        if let Some(pressed) = self.down.as_mut() {
            (pressed.multi, pressed.long_press_done) = (true, true);
        }
    }

    /// A Panning where the press is, without moving it: another finger moved (a pinch).
    pub fn pan_in_place(&self, time_ms: f64) -> Option<Gesture> {
        let pressed = self.down.as_ref()?;
        let zero = Point::default();
        Some(Gesture {
            kind: GestureKind::Panning,
            location: pressed.previous,
            start: pressed.start,
            delta: zero,
            total: pressed.previous - pressed.start,
            time_ms,
            velocity: zero,
            wheel: 0.0,
            wheel_horizontal: false,
            cancelled: false,
            button: pressed.button,
            source: ContextMenuSource::Touch,
            touch: pressed.touch,
            touches: 1,
            manipulation: None,
        })
    }

    /// Feeds one pointer event; yields up to two gestures (a release can be Tapped then Up, but
    /// not after a long press).
    /// A Down while a button is held, and the Up of another button, are ignored: one press at a time.
    pub fn feed(&mut self, pointer: PointerKind, button: MouseButton, touch: bool, location: Point, time_ms: f64, scale: f32) -> [Option<Gesture>; 2] {
        let gesture = |kind, start: Point, previous: Point, velocity, button| Gesture {
            kind,
            location,
            start,
            delta: location - previous,
            total: location - start,
            time_ms,
            velocity,
            wheel: 0.0,
            wheel_horizontal: false,
            cancelled: kind == GestureKind::Up && pointer == PointerKind::Cancel,
            button,
            source: ContextMenuSource::Mouse,
            touch,
            touches: 1,
            manipulation: None,
        };
        let threshold = TAPPED_CANCEL_MOVE_THRESHOLD_POINTS * scale.max(0.1);
        match (pointer, self.down) {
            (PointerKind::Down, pressed) if pressed.is_none_or(|p| p.button == button) => {
                let zero = Point::default();
                let (start, previous, velocity, down_ms, long_press_done, long_pressed) = (location, location, zero, time_ms, false, false);
                let mut trail = Trail::default();
                trail.push(location, time_ms);
                self.down = Some(Pressed { start, previous, velocity, trail, button, down_ms, long_press_done, long_pressed, touch, multi: false });
                [Some(gesture(GestureKind::Down, location, location, zero, button)), None]
            }
            (PointerKind::Move, Some(Pressed { start, previous, velocity, mut trail, button, down_ms, long_press_done, long_pressed, multi, .. })) => {
                let velocity = trail.velocity(location, time_ms, velocity);
                trail.push(location, time_ms);
                let total = location - start;
                // Moved away from the press point: it is a pan, no long press comes (C# CancelDesktopLongPress).
                let long_press_done = long_press_done || total.x.abs() >= threshold || total.y.abs() >= threshold;
                self.down = Some(Pressed { start, previous: location, velocity, trail, button, down_ms, long_press_done, long_pressed, touch, multi });
                if location == previous {
                    return [None, None];
                }
                [Some(gesture(GestureKind::Panning, start, previous, velocity, button)), None]
            }
            (PointerKind::Up, Some(Pressed { start, previous, velocity, button: pressed, long_pressed, multi, .. })) if pressed == button => {
                self.down = None;
                let total = location - start;
                let up = Some(gesture(GestureKind::Up, start, previous, velocity, button));
                if !long_pressed && !multi && total.x.abs() < threshold && total.y.abs() < threshold {
                    [Some(gesture(GestureKind::Tapped, start, previous, velocity, button)), up]
                } else {
                    [up, None]
                }
            }
            // No tap and no speed: whatever was pressed is let go, nothing else happens.
            (PointerKind::Cancel, Some(Pressed { start, previous, button, .. })) => {
                self.down = None;
                [Some(gesture(GestureKind::Up, start, previous, Point::default(), button)), None]
            }
            _ => [None, None],
        }
    }
}

/// The release velocity of a pan (DrawnUI VelocityAccumulator): the average of the last moves,
/// newer ones weighing more. The control that flings owns one and decides which moves count.
#[derive(Clone, Copy, Default, Debug)]
pub struct VelocityAccumulator {
    /// (velocity, arrival time in ms), oldest first.
    samples: [(Point, f64); Self::MAX_SAMPLES],
    len: usize,
}

impl VelocityAccumulator {
    const MAX_SAMPLES: usize = 5;
    /// Older samples say nothing about the release: a finger that rested before lifting does not fling.
    const CONSIDERATION_TIMEFRAME_MS: f64 = 150.0;

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn capture(&mut self, velocity: Point, time_ms: f64) {
        // A move in the same burst as the last one replaces it: the first move of a burst carries
        // only part of the burst's distance (WSLg: half the hand's speed every other sample).
        if let Some(last) = self.samples[..self.len].last_mut().filter(|(_, ms)| time_ms - ms < BURST_MS) {
            last.0 = velocity;
            return;
        }
        if self.len == Self::MAX_SAMPLES {
            self.samples.copy_within(1.., 0);
            self.len -= 1;
        }
        self.samples[self.len] = (velocity, time_ms);
        self.len += 1;
    }

    /// The velocity at `now_ms`, each axis limited to `clamp_absolute` (0 = no limit).
    pub fn final_velocity(&self, now_ms: f64, clamp_absolute: f32) -> Point {
        let (mut sum, mut weights) = (Point::default(), 0.0);
        let relevant = self.samples[..self.len].iter().filter(|s| now_ms - s.1 <= Self::CONSIDERATION_TIMEFRAME_MS);
        for (i, (velocity, _)) in relevant.enumerate() {
            let weight = (i + 1) as f32;
            sum += *velocity * weight;
            weights += weight;
        }
        if weights == 0.0 {
            return Point::default();
        }
        let limit = if clamp_absolute == 0.0 { f32::INFINITY } else { clamp_absolute };
        Point::new((sum.x / weights).clamp(-limit, limit), (sum.y / weights).clamp(-limit, limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WSLg delivers a steady drag as moves in pairs 0.01 ms apart every 15 ms (6 px, then 9 px):
    /// the release velocity is the hand's, 15 px per 15 ms, not millions of px/s (per-move
    /// velocity) nor half of it (the first move of each burst counted on its own).
    #[test]
    fn moves_in_bursts_release_at_the_hands_speed() {
        let (mut recognizer, mut accumulator) = (Recognizer::default(), VelocityAccumulator::default());
        recognizer.feed(PointerKind::Down, MouseButton::Left, false, Point::default(), 0.0, 1.0);
        let (mut y, mut time) = (0.0, 0.0);
        for burst in 1..=8 {
            for (later, dy) in [(0.0, 6.0), (0.01, 9.0)] {
                (y, time) = (y + dy, burst as f64 * 15.0 + later);
                if let [Some(gesture), _] = recognizer.feed(PointerKind::Move, MouseButton::Left, false, Point::new(0.0, y), time, 1.0) {
                    accumulator.capture(gesture.velocity, time);
                }
            }
        }
        let release = accumulator.final_velocity(time + 5.0, 0.0);
        assert!((release.y - 1000.0).abs() < 2.0, "{release:?}");
    }
}

