//! Typefaces by alias and weight. Web has no system fonts: there every font comes from bytes.

use std::cell::RefCell;
use std::collections::HashMap;

use skia_safe::font::Edging;
use skia_safe::{Data, Font, FontMgr, FontStyle, Typeface};

/// Weight of a face registered without one (CSS regular).
const REGULAR: i32 = 400;
/// From this weight on a lighter face gets synthetic bold, as DrawnUi.React (C# Font.Embolden).
const BOLD: i32 = 600;
/// Synthetic italic, as upstream (`SKFont.SkewX`).
const ITALIC_SKEW: f32 = -0.25;

thread_local! {
    /// One font manager for the engine: `FontMgr::new()` makes a new platform manager each call,
    /// and on Android that parses the system font list (/system/etc/fonts.xml) again.
    static FONT_MGR: FontMgr = FontMgr::new();
}

/// The engine's font manager (a reference-count bump).
pub(crate) fn font_mgr() -> FontMgr {
    FONT_MGR.with(FontMgr::clone)
}

/// The registered font faces: by alias, several weights per alias.
pub struct Fonts {
    /// The system default where there is one (desktop); `None` on the web.
    system: Option<Typeface>,
    /// The first alias ever registered: the default font, whatever order the files arrive in.
    default_alias: Option<String>,
    /// Faces of an alias by weight, in registration order.
    named: HashMap<String, Vec<(i32, Typeface)>>,
    /// The system face found for a character, or none (`match_character`).
    by_character: RefCell<HashMap<char, Option<Typeface>>>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            system: font_mgr().legacy_make_typeface(None, FontStyle::normal()),
            default_alias: None,
            named: HashMap::new(),
            by_character: RefCell::default(),
        }
    }
}

impl Fonts {
    /// Names an alias before its file is there. The first alias named is the default font.
    pub fn register(&mut self, alias: &str) {
        if self.default_alias.is_none() {
            self.default_alias = Some(alias.to_owned());
        }
    }

    /// Adds a font from file bytes under an alias at the regular weight (registering the alias
    /// if it is new).
    pub fn add(&mut self, alias: &str, bytes: &[u8]) -> bool {
        self.add_weight(alias, REGULAR, bytes)
    }

    /// Adds a face of an alias at a weight (100..900): `FontWeight` and bold pick the nearest
    /// registered weight (DrawnUI `fonts.AddFont(source, alias, weight)`).
    pub fn add_weight(&mut self, alias: &str, weight: i32, bytes: &[u8]) -> bool {
        self.add_face(alias, weight, bytes, true)
    }

    /// `add_weight`; `may_be_default` false never makes the alias the default font (a font the
    /// web page hands in: only labels that name it use it).
    pub(crate) fn add_face(&mut self, alias: &str, weight: i32, bytes: &[u8], may_be_default: bool) -> bool {
        let Some(typeface) = font_mgr().new_from_data(Data::new_copy(bytes), None) else { return false };
        if may_be_default {
            self.register(alias);
        }
        let faces = self.named.entry(alias.to_owned()).or_default();
        match faces.iter_mut().find(|(w, _)| *w == weight) {
            Some(face) => face.1 = typeface,
            None => faces.push((weight, typeface)),
        }
        true
    }

    /// The alias an empty family stands for: the first one registered.
    pub fn default_alias(&self) -> Option<&str> {
        self.default_alias.as_deref()
    }

    /// The typeface of a family at the regular weight; an unknown or empty family gives the
    /// default font.
    pub fn typeface(&self, family: &str) -> Option<&Typeface> {
        self.resolve(family, 0).map(|(typeface, _)| typeface)
    }

    /// The face of a family nearest to `weight` (0 = regular) and the weight it has. An unknown or
    /// empty family gives the default font.
    pub fn resolve(&self, family: &str, weight: i32) -> Option<(&Typeface, i32)> {
        let faces = self
            .named
            .get(family)
            .or_else(|| self.named.get(self.default_alias.as_deref()?))
            .filter(|faces| !faces.is_empty());
        let Some(faces) = faces else { return self.system.as_ref().map(|t| (t, REGULAR)) };
        let target = if weight > 0 { weight } else { REGULAR };
        let (w, typeface) = faces.iter().min_by_key(|(w, _)| (w - target).abs())?;
        Some((typeface, *w))
    }

    /// A font for the family (or the default) at a size in pixels, regular and upright.
    pub fn font(&self, family: &str, size: f32) -> Option<Font> {
        self.font_for(family, 0, false, size)
    }

    /// A font at a weight (0 = regular) and slant, size in pixels. A weight of 600 or more on a
    /// lighter face is emboldened; italic without an italic face is a skew, as DrawnUi.React.
    pub fn font_for(&self, family: &str, weight: i32, italic: bool, size: f32) -> Option<Font> {
        let (typeface, has) = self.resolve(family, weight)?;
        Some(font(typeface, has, weight, italic, size))
    }

    /// A system face with a glyph for `c`, for text no registered font can draw (C#
    /// `SkiaFontManager.MatchCharacter`); none on the web, which has no system fonts. Cached per
    /// character.
    pub fn match_character(&self, c: char) -> Option<Typeface> {
        if let Some(found) = self.by_character.borrow().get(&c) {
            return found.clone();
        }
        // A platform may answer with a default face that lacks the glyph (C#: the iOS case).
        let found = font_mgr().match_family_style_character("", FontStyle::normal(), &[], c as i32).filter(|t| t.unichar_to_glyph(c as i32) != 0);
        self.by_character.borrow_mut().insert(c, found.clone());
        found
    }

    /// `font_for` with a face from `match_character`.
    pub(crate) fn font_from(&self, typeface: &Typeface, weight: i32, italic: bool, size: f32) -> Font {
        font(typeface, *typeface.font_style().weight(), weight, italic, size)
    }
}

/// A font of a face that has the weight `has`, for text at `weight` and slant.
fn font(typeface: &Typeface, has: i32, weight: i32, italic: bool, size: f32) -> Font {
    {
        let mut font = Font::from_typeface(typeface, size);
        font.set_subpixel(true);
        // Unhinted advances: FreeType (Linux, Android, the browser) rounds a hinted advance to whole
        // pixels ("One label, many styles:" at 16: 172 instead of 171.344), which DirectWrite and
        // CanvasKit never do, so text would wrap differently there. Only measurement changes.
        font.set_linear_metrics(true);
        // As upstream (Super.FontSubPixelRendering). Only a surface with a known pixel geometry renders LCD text.
        font.set_edging(Edging::SubpixelAntiAlias);
        if weight >= BOLD && has < BOLD {
            font.set_embolden(true);
        }
        if italic {
            font.set_skew_x(ITALIC_SKEW);
        }
        font
    }
}
