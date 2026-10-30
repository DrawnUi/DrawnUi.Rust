//! Text shaping (C# SkiaLabel DrawShapedText, HarfBuzzSharp there; harfrust, HarfBuzz in Rust,
//! here) for the text that needs it: complex scripts whose letters join or reorder (Arabic,
//! Hebrew marks, Indic, Thai...) and emoji sequences (ZWJ families, skin tones, flags, keycaps).
//! Everything else keeps the plain path: one glyph per character, no shaping cost.
//!
//! Fonts are read through their Skia typeface table by table (only the tables shaping asks for,
//! never the outlines), one harfrust font per typeface, kept for the life of the thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use harfrust::{Buffer, Direction, ShapeOptions, ShaperFont};
use skia_safe::{Font, GlyphId, Point, Typeface};

/// C# SkiaLabel.UnicodeNeedsShaping, plus the characters that make emoji sequences and the
/// right-to-left scripts that C# leaves out (Hebrew, Arabic supplements and presentation forms).
pub(crate) fn needs_shaping(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x08FF        // Hebrew, Arabic, Syriac, Arabic Supplement, Thaana, NKo, Samaritan, Mandaic, Arabic Extended
        | 0x0900..=0x0DFF      // Devanagari .. Sinhala
        | 0x0E00..=0x0FFF      // Thai, Lao, Tibetan
        | 0x1000..=0x109F      // Myanmar
        | 0x10A0..=0x10FF      // Georgian
        | 0x1100..=0x11FF      // Hangul Jamo
        | 0x1200..=0x137F      // Ethiopic
        | 0x1780..=0x18AF      // Khmer, Mongolian
        | 0x200C..=0x200D      // ZWNJ, ZWJ
        | 0x20E3               // keycap
        | 0x2600..=0x27BF      // symbols that have emoji forms
        | 0xFB1D..=0xFDFF      // Hebrew and Arabic presentation forms A
        | 0xFE00..=0xFE0F      // variation selectors (VS15 / VS16)
        | 0xFE70..=0xFEFF      // Arabic presentation forms B
        | 0x1F000..=0x1FAFF    // emoji, regional indicators (flags), skin tones
        | 0xE0020..=0xE007F    // tag sequences (subdivision flags)
    )
}

/// Characters that belong to the glyph before them (an emoji sequence or a mark): they take the
/// font of that glyph, so a sequence stays in one run (C# JoinsPreviousGlyph).
pub(crate) fn joins_previous(c: char) -> bool {
    matches!(c as u32, 0x200D | 0x20E3 | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F)
}

/// Right-to-left letters: a paragraph with one of them gets the bidirectional pass.
pub(crate) fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}

/// One shaped run: glyphs positioned from x = 0 on the baseline, in visual order, and the advance
/// of every character of the text in logical order (a cluster's advance on its first character).
#[derive(Default)]
pub(crate) struct Shaped {
    pub glyphs: Vec<GlyphId>,
    pub positions: Vec<Point>,
    pub advances: Vec<f32>,
    pub width: f32,
}

thread_local! {
    static FONTS: RefCell<HashMap<u32, Option<harfrust::Font>>> = RefCell::new(HashMap::new());
    static BUFFER: RefCell<Option<Buffer>> = const { RefCell::new(None) };
}

/// The harfrust font of a typeface, made once: its tables are copied from Skia when shaping
/// first asks for them.
fn shaping_font(typeface: &Typeface) -> Option<harfrust::Font> {
    let id = typeface.unique_id();
    FONTS.with_borrow_mut(|fonts| {
        fonts
            .entry(id)
            .or_insert_with(|| {
                let typeface = typeface.clone();
                let tables: Arc<dyn Fn(harfrust::Tag) -> Option<harfrust::font::Blob> + Send + Sync> = Arc::new(move |tag| {
                    let data = typeface.copy_table_data(u32::from_be_bytes(tag.to_be_bytes()))?;
                    Some(data.as_bytes().to_vec().into())
                });
                harfrust::Font::new(tables, 0)
            })
            .clone()
    })
}

/// Shapes `text` with `font` (one font, one direction: the caller splits runs by font and by
/// bidirectional level), `spacing` pixels after every character. `None` when the font cannot be
/// read for shaping: the caller keeps its plain glyphs.
pub(crate) fn shape(font: &Font, text: &str, rtl: bool, spacing: f32, out: &mut Shaped) -> Option<()> {
    let typeface = font.typeface();
    let shaping = shaping_font(&typeface)?;
    let upem = typeface.units_per_em().filter(|u| *u > 0)? as f32;
    let scale = font.size() / upem;
    let shaper = ShaperFont::new(&shaping);
    BUFFER.with_borrow_mut(|slot| {
        let mut buffer = slot.take().unwrap_or_else(Buffer::new);
        buffer.reset();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
        let shaped = harfrust::shape(&shaper, &mut buffer, ShapeOptions::new()).is_ok();
        if shaped {
            out.glyphs.clear();
            out.positions.clear();
            out.advances.clear();
            // Character index of every byte that starts a character.
            let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
            out.advances.resize(starts.len(), 0.0);
            let char_of = |byte: u32| starts.partition_point(|b| (*b as u32) < byte).min(starts.len().saturating_sub(1));
            let mut x = 0.0;
            for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
                out.glyphs.push(info.glyph_id as GlyphId);
                out.positions.push(Point::new(x + pos.x_offset as f32 * scale, -(pos.y_offset as f32) * scale));
                let advance = pos.x_advance as f32 * scale;
                x += advance;
                out.advances[char_of(info.cluster)] += advance;
            }
            // Character spacing after every character, as the plain path does.
            if spacing != 0.0 {
                // Visual order: the spacing of the characters between two clusters goes between them.
                let mut extra = 0.0;
                let mut last: Option<usize> = None;
                for (k, info) in buffer.glyph_infos().iter().enumerate() {
                    let at = char_of(info.cluster);
                    if let Some(before) = last.filter(|b| *b != at) {
                        extra += spacing * before.abs_diff(at) as f32;
                    }
                    last = Some(at);
                    out.positions[k].x += extra;
                }
                out.advances.iter_mut().for_each(|a| *a += spacing);
            }
            out.width = out.advances.iter().sum();
        }
        *slot = Some(buffer);
        shaped.then_some(())
    })
}
