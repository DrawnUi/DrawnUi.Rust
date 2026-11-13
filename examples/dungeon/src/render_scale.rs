//! The scene's density cap: the 3D scene and its post effect are drawn into a picture of the
//! scene's size in points times at most `max_density` pixels per point, keeping the aspect, and
//! scaled up to the screen with linear filtering; a tablet's bigger screen gets proportionally
//! more pixels. A screen whose density is within the cap draws the scene 1:1, straight to the
//! canvas. The HUD, the texts and the dialogs always stay at full resolution. Off by default; the
//! Dungeon turns it on for Android at 1 pixel per point. DUNGEON_MAXDENSITY changes it for tries
//! (0 = off).

/// The cap on Android, pixels per point.
#[cfg(target_os = "android")]
const ANDROID_MAX_DENSITY: f32 = 1.0;

pub struct RenderScale {
    /// Pixels per point of the scene at most; 0 = no cap.
    max_density: f32,
}

impl Default for RenderScale {
    fn default() -> Self {
        #[cfg(target_os = "android")]
        let fallback = ANDROID_MAX_DENSITY;
        #[cfg(not(target_os = "android"))]
        let fallback = 0.0;
        let max_density = std::env::var("DUNGEON_MAXDENSITY").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|d| *d >= 0.0).unwrap_or(fallback);
        Self { max_density }
    }
}

impl RenderScale {
    /// The scale of the scene's picture at `pixels` per point: 1 within the cap, else the share
    /// that brings it to the cap's pixels per point.
    pub fn value(&self, pixels: f32) -> f32 {
        if self.max_density <= 0.0 || pixels <= self.max_density { 1.0 } else { self.max_density / pixels }
    }

    /// Tells the scene's picture size, once per change (logcat tag `dungeon` on Android).
    pub fn log(&self, width: f32, height: f32, pixels: f32, scale: f32) {
        let line = format!(
            "dungeon: scene {:.0}x{:.0} px for a {:.0}x{:.0} px screen (cap {})",
            width * pixels * scale,
            height * pixels * scale,
            width * pixels,
            height * pixels,
            if self.max_density <= 0.0 { "off".to_string() } else { format!("{} px per point", self.max_density) }
        );
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

    /// Within the cap the scene draws 1:1; past it the scene gets the cap's pixels per point (both
    /// axes alike, so a bigger screen gets more pixels); no cap leaves it at 1.
    #[test]
    fn caps_the_pixels_per_point() {
        let cap = RenderScale { max_density: 1.0 };
        // A phone held upright, 360 x 715 points at 3: the scene 360 x 715 pixels.
        let s = cap.value(3.0);
        assert!((715.0 * 3.0 * s - 715.0).abs() < 0.01);
        assert!((cap.value(2.0) - 0.5).abs() < 1e-6, "a tablet at 2: half");
        assert_eq!(cap.value(1.0), 1.0, "at the cap: 1:1");
        assert_eq!(cap.value(0.75), 1.0, "below it: 1:1");
        assert_eq!(RenderScale { max_density: 0.0 }.value(3.0), 1.0, "no cap");
    }
}
