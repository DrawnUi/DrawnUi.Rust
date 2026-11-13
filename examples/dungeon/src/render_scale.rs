//! The scene's pixel cap: the 3D scene and its post effect are drawn into a picture whose longer
//! side is at most `max_px` pixels, keeping the aspect, and scaled up to the screen with linear
//! filtering. A screen already within the cap draws the scene 1:1, straight to the canvas. The
//! HUD, the texts and the dialogs always stay at full resolution. Off by default; the Dungeon turns
//! it on for Android at 1280 pixels. DUNGEON_MAXPX changes it for tries (0 = off).

/// The cap on Android, pixels of the longer side.
#[cfg(target_os = "android")]
const ANDROID_MAX_PX: u32 = 1280;

pub struct RenderScale {
    /// Pixels of the scene's longer side at most; 0 = no cap.
    max_px: u32,
}

impl Default for RenderScale {
    fn default() -> Self {
        #[cfg(target_os = "android")]
        let fallback = ANDROID_MAX_PX;
        #[cfg(not(target_os = "android"))]
        let fallback = 0;
        let max_px = std::env::var("DUNGEON_MAXPX").ok().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(fallback);
        Self { max_px }
    }
}

impl RenderScale {
    /// The scale of the scene's picture for a screen of `width` x `height` points at `pixels`
    /// per point: 1 within the cap, else the share that brings the longer side to the cap.
    pub fn value(&self, width: f32, height: f32, pixels: f32) -> f32 {
        let long = width.max(height) * pixels;
        if self.max_px == 0 || long <= self.max_px as f32 { 1.0 } else { self.max_px as f32 / long }
    }

    /// Tells the scene's picture size, once per change (logcat tag `dungeon` on Android).
    pub fn log(&self, width: f32, height: f32, pixels: f32, scale: f32) {
        let line = format!(
            "dungeon: scene {:.0}x{:.0} px for a {:.0}x{:.0} px screen (cap {})",
            width * pixels * scale,
            height * pixels * scale,
            width * pixels,
            height * pixels,
            if self.max_px == 0 { "off".to_string() } else { format!("{} px", self.max_px) }
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

    /// Within the cap the scene draws 1:1; past it the longer side comes down to the cap, both axes
    /// alike; no cap leaves it at 1.
    #[test]
    fn caps_the_longer_side() {
        let cap = RenderScale { max_px: 1280 };
        // A phone held upright, 360 x 802 points at 3: 2406 pixels tall.
        let s = cap.value(360.0, 802.0, 3.0);
        assert!((802.0 * 3.0 * s - 1280.0).abs() < 0.01);
        assert!((360.0 * 3.0 * s - 360.0 * 1280.0 / 802.0).abs() < 0.01, "the aspect stays");
        assert_eq!(cap.value(1280.0, 720.0, 1.0), 1.0, "at the cap: 1:1");
        assert_eq!(cap.value(800.0, 600.0, 1.0), 1.0, "below it: 1:1");
        assert_eq!(RenderScale { max_px: 0 }.value(360.0, 802.0, 3.0), 1.0, "no cap");
    }
}
