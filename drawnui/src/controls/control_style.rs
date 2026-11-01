//! The look a styled control builds its default content with (DrawnUI PrebuiltControlStyle /
//! UsingControlStyle). `Platform` resolves at compile time from the target OS; the React engine
//! resolves it from the browser's user agent.

/// The accent of the flat DrawnUI look (`PrebuiltControlStyle::Unset`): blue #3B82F6, the DrawnUI
/// Designer's and Agent Signals' accent (C# `DefaultStyle.Accent`).
pub(crate) const DEFAULT_ACCENT: u32 = 0x3B82F6;

/// DrawnUI PrebuiltControlStyle: which platform look a `SkiaButton`, `SkiaSwitch`, `SkiaCheckbox`,
/// `SkiaRadioButton`, `SkiaProgress` or `SkiaSlider` is drawn in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrebuiltControlStyle {
    /// The flat DrawnUI look: blue accent, neutral grays.
    #[default]
    Unset,
    /// The look of the platform the app runs on: iOS / macOS = Cupertino, Android = Material,
    /// Windows = Windows, anything else = `Unset`.
    Platform,
    Cupertino,
    Material,
    Material3,
    Windows,
}

impl PrebuiltControlStyle {
    /// The look that is drawn: `Platform` replaced by the platform's own (DrawnUI UsingControlStyle).
    pub fn resolve(self) -> PrebuiltControlStyle {
        if self != PrebuiltControlStyle::Platform {
            return self;
        }
        if cfg!(any(target_os = "ios", target_os = "macos")) {
            PrebuiltControlStyle::Cupertino
        } else if cfg!(target_os = "android") {
            PrebuiltControlStyle::Material
        } else if cfg!(target_os = "windows") {
            PrebuiltControlStyle::Windows
        } else {
            PrebuiltControlStyle::Unset
        }
    }
}
