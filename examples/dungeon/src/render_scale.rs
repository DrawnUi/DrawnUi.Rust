//! Adaptive render scale, as a video player adapts its quality: the 3D scene and its post effect are
//! drawn into a smaller picture and scaled up to the screen when the device cannot draw them at the
//! display's rate. The HUD, the texts and the dialogs always stay at full resolution.

/// Seconds of frames judged together, and of good windows before a step up.
const WINDOW: f32 = 2.0;
const UP_AFTER: f32 = 10.0;
const STEPS: [f32; 3] = [1.0, 0.75, 0.5];

/// Picks the render scale of the 3D scene from the frame intervals: 1.0, 0.75 or 0.5. It starts at
/// 1.0. When more than half of the frames of a 2 s window miss the display's rate, it steps down;
/// after 10 s of windows where almost every frame keeps the rate, it steps up. A step down within
/// 10 s after a step up keeps the lower step for the rest of the session, so the scale never
/// pumps. Allocates nothing; the caller resizes only on a step.
pub struct RenderScale {
    /// A fixed scale for tests (0 = adaptive), from DUNGEON_SCALE (desktop).
    forced: f32,
    step: usize,
    window: f32,
    good: f32,
    since_up: f32,
    frames: u32,
    slow: u32,
    locked: bool,
}

impl Default for RenderScale {
    fn default() -> Self {
        let forced = std::env::var("DUNGEON_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).filter(|s| *s > 0.0).map_or(0.0, |s| s.clamp(0.5, 1.0));
        Self { forced, step: 0, window: 0.0, good: 0.0, since_up: f32::MAX, frames: 0, slow: 0, locked: false }
    }
}

impl RenderScale {
    /// The scale now.
    pub fn value(&self) -> f32 {
        if self.forced > 0.0 { self.forced } else { STEPS[self.step] }
    }

    /// One frame of `interval` seconds at a display of `rate` Hz. Returns true when the scale changed.
    pub fn frame(&mut self, interval: f32, rate: f32) -> bool {
        if self.forced > 0.0 || rate <= 0.0 {
            return false;
        }
        self.since_up += interval;
        // A pause, a load or a step's own resize is no measure of the drawing: start the window again.
        if interval > 0.25 || interval <= 0.0 {
            (self.window, self.frames, self.slow) = (0.0, 0, 0);
            return false;
        }
        self.window += interval;
        self.frames += 1;
        // Missed the display's rate: an interval of 1.5 refreshes or more.
        if interval > 1.5 / rate {
            self.slow += 1;
        }
        if self.window < WINDOW {
            return false;
        }
        let slow = self.slow as f32 / self.frames as f32;
        let window = self.window;
        (self.window, self.frames, self.slow) = (0.0, 0, 0);
        if slow > 0.5 {
            self.good = 0.0;
            if self.step == STEPS.len() - 1 {
                return false;
            }
            if self.since_up < UP_AFTER {
                self.locked = true;
            }
            self.step += 1;
            self.log(&format!("down, {:.0} % of the frames missed {rate:.0} Hz{}", slow * 100.0, if self.locked { ", stays there" } else { "" }));
            return true;
        }
        self.good = if slow < 0.1 { self.good + window } else { 0.0 };
        if self.good < UP_AFTER || self.step == 0 || self.locked {
            return false;
        }
        (self.good, self.since_up) = (0.0, 0.0);
        self.step -= 1;
        self.log(&format!("up, {UP_AFTER:.0} s at {rate:.0} Hz"));
        true
    }

    fn log(&self, why: &str) {
        let line = format!("dungeon: render scale {:.0} % ({why})", STEPS[self.step] * 100.0);
        // Android: logcat, tag `dungeon` (stdout goes nowhere there).
        #[cfg(target_os = "android")]
        {
            unsafe extern "C" {
                fn __android_log_write(priority: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
            }
            if let Ok(text) = std::ffi::CString::new(line) {
                // SAFETY: two NUL-terminated strings that live through the call; 4 = ANDROID_LOG_INFO.
                unsafe { __android_log_write(4, c"dungeon".as_ptr(), text.as_ptr()) };
            }
        }
        #[cfg(not(target_os = "android"))]
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adaptive() -> RenderScale {
        RenderScale { forced: 0.0, ..RenderScale::default() }
    }

    /// Steps down after a slow window, up after 10 s of good ones, and stays down when it drops
    /// again soon after a step up.
    #[test]
    fn steps_down_up_and_locks() {
        let mut s = adaptive();
        let run = |s: &mut RenderScale, interval: f32, seconds: f32| {
            let mut changed = 0;
            for _ in 0..(seconds / interval) as usize {
                changed += s.frame(interval, 60.0) as u32;
            }
            changed
        };
        // Fast frames stay at 1.0; so do occasional hitches (one slow frame in ten).
        assert_eq!(run(&mut s, 1.0 / 60.0, 6.0), 0);
        for _ in 0..30 {
            assert!(!s.frame(1.0 / 30.0, 60.0));
            for _ in 0..9 {
                assert!(!s.frame(1.0 / 60.0, 60.0));
            }
        }
        assert_eq!(s.value(), 1.0);
        // A pause (a long frame) starts the window again, so the slow frames fill whole windows.
        assert!(!s.frame(0.3, 60.0));
        assert_eq!(run(&mut s, 1.0 / 30.0, 2.1), 1);
        assert_eq!(s.value(), 0.75);
        assert_eq!(run(&mut s, 1.0 / 30.0, 2.1), 1);
        assert_eq!(s.value(), 0.5);
        // The floor.
        assert_eq!(run(&mut s, 1.0 / 30.0, 4.1), 0);
        assert_eq!(s.value(), 0.5);
        // Good frames: up after 10 s.
        assert_eq!(run(&mut s, 1.0 / 60.0, 9.0), 0);
        assert_eq!(run(&mut s, 1.0 / 60.0, 3.0), 1);
        assert_eq!(s.value(), 0.75);
        // Slow again within 10 s of the step up (the open window of good frames first): down, and
        // locked there.
        assert_eq!(run(&mut s, 1.0 / 30.0, 4.1), 1);
        assert_eq!(s.value(), 0.5);
        assert_eq!(run(&mut s, 1.0 / 60.0, 30.0), 0);
        assert_eq!(s.value(), 0.5);
        // A pause starts the window again: no step from one long frame.
        let mut t = adaptive();
        assert!(!t.frame(1.0, 60.0));
        assert_eq!(t.value(), 1.0);
    }
}
