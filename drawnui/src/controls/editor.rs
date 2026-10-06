//! SkiaEditor: a drawn text input (DrawnUi.React SkiaEditor.ts, C# SkiaEditor with its Blazor
//! keyboard). The text is a SkiaLabel child that keeps every space, next to a placeholder label;
//! the editor paints its frame, the selection under the text and a blinking caret above it.
//! Single line (`max_lines` 1: the text scrolls sideways to the caret) or multiline (wraps, scrolls
//! down to the caret, `auto_height` grows up to `max_lines`). Keys and typed text come through
//! `Control::on_key` while the editor has the focus; a Down focuses it and places the caret, a
//! drag selects, a double tap or a long press selects the word.
// ponytail: grapheme clusters are approximated (combining marks, ZWJ sequences, variation
// selectors, skin tones, flags, keycaps); a Unicode segmentation table would make them exact.

use std::any::Any;

use skia_safe::{ClipOp, Color, Paint, PaintStyle, Point, RRect, Rect, Size};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx, part, part_mut};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::label::{LabelBuild, LabelSet, LineBreakMode, SkiaLabel, TextAlignment, TextLine};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::gestures::{Gesture, GestureKind, TAPPED_CANCEL_MOVE_THRESHOLD_POINTS};
use crate::keyboard::{Cursor, KeyEvent, KeyKind};
use crate::tree::{Build, ControlId, Cx, Handle, Mut, Raw, Tree, wrong_state};
use crate::types::{CacheType, Dirty, LayoutOptions, SkiaGradient, Thickness};
use crate::ui::Aria;
use crate::{paint, props};

/// The caret is shown, then hidden, this long each.
const BLINK_MS: f64 = 500.0;
/// A second Down this soon after the first, near it, selects the word (a double click).
const DOUBLE_TAP_MS: f64 = 500.0;
/// Caret width, points.
const CARET: f32 = 2.0;

/// What Enter does on a multiline editor, and the key label a soft keyboard shows (C# ReturnType).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ReturnType {
    /// The platform's key.
    Default,
    /// Enter breaks a multiline editor's line.
    #[default]
    Done,
    /// As Done, labeled Go.
    Go,
    /// As Done, labeled Next.
    Next,
    /// As Done, labeled Search.
    Search,
    /// Enter submits a multiline editor too; Shift+Enter or Alt+Enter breaks the line.
    Send,
}

/// The soft keyboard a host would open (C# SkiaEditorKeyboard). The desktop and web hosts do not
/// read it yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KeyboardType {
    /// Text.
    #[default]
    Default,
    /// Digits.
    Numeric,
    /// Digits and a decimal separator.
    Decimal,
    /// A phone pad.
    Phone,
    /// Text with @ at hand.
    Email,
}

props!(EditorProps, EditorBuild, EditorSet {
    /// The text. Single line: line breaks become spaces.
    text / set_text: String = String::new(), MEASURE_APPLY;
    /// Shown while the text is empty.
    placeholder_text / set_placeholder_text: String = String::new(), MEASURE_APPLY;
    /// The placeholder text color.
    placeholder_color / set_placeholder_color: Color = Color::from_rgb(0x9A, 0xA0, 0xA6), APPLY;
    /// Where the placeholder sits in the line.
    placeholder_horizontal_alignment / set_placeholder_horizontal_alignment: TextAlignment = TextAlignment::Start, APPLY;
    /// Points.
    font_size / set_font_size: f32 = 12.0, MEASURE_APPLY;
    /// A registered alias; empty = the default font.
    font_family / set_font_family: String = String::new(), MEASURE_APPLY;
    /// Aliases for glyphs the font lacks, comma separated (the label's `font_family_fallback`).
    font_family_fallback / set_font_family_fallback: String = String::new(), MEASURE_APPLY;
    /// What the fonts lack (CJK, emoji, symbols) is drawn with a system font that has it (C#
    /// UseUnicode, default on: the editor's label is a SkiaRichLabel without markdown).
    use_unicode / set_use_unicode: bool = true, MEASURE_APPLY;
    /// 100..900; 0 = regular.
    font_weight / set_font_weight: i32 = 0, MEASURE_APPLY;
    /// Black by default, as DrawnUi.React.
    text_color / set_text_color: Color = Color::BLACK, APPLY;
    /// Painted on the glyphs instead of `text_color`.
    text_gradient / set_text_gradient: Option<Box<SkiaGradient>> = None, APPLY;
    /// Multiplies the height of a line.
    line_height / set_line_height: f32 = 1.0, MEASURE_APPLY;
    /// Where the text sits when it is shorter than the field.
    horizontal_text_alignment / set_horizontal_text_alignment: TextAlignment = TextAlignment::Start, MEASURE_APPLY;
    /// Where the lines sit when they are fewer than the field holds.
    vertical_text_alignment / set_vertical_text_alignment: TextAlignment = TextAlignment::Start, MEASURE_APPLY;
    /// 1 = single line; any other value is multiline, the height of that many lines (-1 = one,
    /// or all of them with `auto_height`).
    max_lines / set_max_lines: i32 = 1, MEASURE_APPLY;
    /// Multiline: the height follows the lines, up to `max_lines`.
    auto_height / set_auto_height: bool = false, MEASURE;
    /// Every character is shown as a bullet.
    is_password / set_is_password: bool = false, MEASURE_APPLY;
    /// Characters the text may hold; -1 = no limit. Typing and pasting stop there (not in
    /// DrawnUi.React nor C#: MAUI Entry.MaxLength).
    max_length / set_max_length: i32 = -1, MEASURE_APPLY;
    /// Send makes Enter submit a multiline editor.
    return_type / set_return_type: ReturnType = ReturnType::Done, NONE;
    /// A hint for a soft keyboard.
    keyboard_type / set_keyboard_type: KeyboardType = KeyboardType::Default, NONE;
    /// `None` = the style's.
    cursor_color / set_cursor_color: Option<Color> = None, DRAW;
    /// False hides the caret.
    can_show_cursor / set_can_show_cursor: bool = true, DRAW;
    /// Painted under the selected text.
    selection_color / set_selection_color: Color = Color::from_argb(0x55, 0x90, 0xCF, 0xFE), DRAW;
    /// Points; -1 = the style's.
    corner_radius / set_corner_radius: f32 = -1.0, DRAW;
    /// `None` = the style's.
    stroke_color / set_stroke_color: Option<Color> = None, DRAW;
    /// Points; -1 = the style's.
    stroke_width / set_stroke_width: f32 = -1.0, DRAW;
    /// The look for what the app left unset: fill, corners, border, caret color.
    control_style / set_control_style: PrebuiltControlStyle = PrebuiltControlStyle::Unset, DRAW;
});

/// What a style gives an editor where the app left the value unset (C# ApplyControlStyleVisuals
/// palettes, DrawnUi.React).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EditorLook {
    /// The fill without a background color.
    pub background: Color,
    /// Points.
    pub corner_radius: f32,
    /// The border; transparent = none.
    pub stroke_color: Color,
    /// Points.
    pub stroke_width: f32,
    /// The caret.
    pub cursor_color: Color,
}

impl EditorLook {
    /// The look of a style; `Platform` resolves first.
    pub fn of(style: PrebuiltControlStyle) -> EditorLook {
        use PrebuiltControlStyle::*;
        let look = |bg: u32, corner_radius, stroke: u32, stroke_width, cursor: u32| EditorLook {
            background: Color::new(0xFF00_0000 | bg),
            corner_radius,
            stroke_color: Color::new(stroke),
            stroke_width,
            cursor_color: Color::new(0xFF00_0000 | cursor),
        };
        match style.resolve() {
            Cupertino => look(0xFFFFFF, 10.0, 0xFFC7C7CC, 1.0, 0x007AFF),
            Material => look(0xEFEBF4, 4.0, 0, 0.0, 0x2196F3),
            Material3 => look(0xE6E0E9, 4.0, 0, 0.0, 0x6750A4),
            Windows => look(0xFFFFFF, 2.0, 0xFF8A8A8A, 1.0, 0x0078D4),
            _ => look(0xF2F3F5, 8.0, 0, 0.0, 0xDC143C),
        }
    }
}

type TextHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &str)>;
type MovedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;

/// Handler calls waiting for the next frame (they need the app state).
const TEXT_CHANGED: u8 = 1;
const CURSOR_MOVED: u8 = 2;
const SUBMITTED: u8 = 4;

/// A drawn text input (DrawnUI SkiaEditor).
pub struct SkiaEditor {
    layout: SkiaLayout,
    /// The editor properties.
    pub p: EditorProps,
    id: ControlId,
    label: Handle<SkiaLabel>,
    placeholder: Handle<SkiaLabel>,
    /// The caret, which is the left edge of the selection, and the selection's length:
    /// characters of the text.
    cursor: usize,
    selection: usize,
    /// Characters in the text.
    chars: usize,
    /// Shift + arrows: the edge that stays and the one that moves.
    keyboard_edges: Option<(usize, usize)>,
    /// A press is selecting from this character.
    drag_anchor: Option<usize>,
    focused: bool,
    shift: bool,
    caret_on: bool,
    /// Frame time of the next blink, milliseconds; 0 = restart the blink.
    blink_at: f64,
    ticking: bool,
    /// How far the text is scrolled, pixels.
    scroll: Point,
    /// The caret moved or the text changed: bring it into view after the next layout.
    reveal: bool,
    /// Time and place of the last Down, for a double tap.
    last_down: Option<(f64, Point)>,
    pending: u8,
    /// The text `on_text_changed` last reported.
    reported: String,
    /// Where a handler's text is copied to (the handler may change the editor).
    scratch: String,
    text_changed: Option<TextHandler>,
    cursor_moved: Option<MovedHandler>,
    submitted: Option<TextHandler>,
}

impl SkiaEditor {
    /// An empty single-line editor: padding 12 x 8 points, filling the width, cached as
    /// Operations (DrawnUi.React).
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaEditor> {
        let label = SkiaLabel::new("").keep_spaces_on_line_breaks(true).accessibility_role(Aria::PRESENTATION).tag("EditorLabel");
        let placeholder = SkiaLabel::new("").is_visible(false).accessibility_role(Aria::PRESENTATION).tag("EditorPlaceholder");
        let editor = SkiaEditor {
            layout: SkiaLayout::default(),
            p: EditorProps::default(),
            id: Handle::<SkiaEditor>::default().id(),
            label: label.handle(),
            placeholder: placeholder.handle(),
            cursor: 0,
            selection: 0,
            chars: 0,
            keyboard_edges: None,
            drag_anchor: None,
            focused: false,
            shift: false,
            caret_on: true,
            blink_at: 0.0,
            ticking: false,
            scroll: Point::default(),
            reveal: false,
            last_down: None,
            pending: 0,
            reported: String::new(),
            scratch: String::new(),
            text_changed: None,
            cursor_moved: None,
            submitted: None,
        };
        let mut build = Build::new(editor).padding((12, 8)).horizontal_options(LayoutOptions::Fill).use_cache(CacheType::Operations);
        let id = build.id();
        build.control_mut().id = id;
        build.push_child(placeholder);
        build.push_child(label);
        build
    }

    /// The caret, characters from the start: the left edge of the selection when there is one
    /// (DrawnUi.React counts UTF-16 units).
    pub fn cursor_position(&self) -> usize {
        self.cursor
    }

    /// Characters selected after `cursor_position`.
    pub fn selection_length(&self) -> usize {
        self.selection
    }

    /// The editor has the keyboard focus (`Cx::focus` gives it).
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Not a single-line editor.
    pub fn is_multiline(&self) -> bool {
        self.p.max_lines != 1
    }

    /// The selected text.
    pub fn selected_text(&self) -> &str {
        let (a, b) = (byte_at(&self.p.text, self.cursor), byte_at(&self.p.text, self.cursor + self.selection));
        &self.p.text[a..b]
    }

    /// The look of `control_style`.
    pub fn look(&self) -> EditorLook {
        EditorLook::of(self.p.control_style)
    }

    fn cursor_color(&self) -> Color {
        self.p.cursor_color.unwrap_or_else(|| self.look().cursor_color)
    }

    fn corner_radius(&self) -> f32 {
        if self.p.corner_radius >= 0.0 { self.p.corner_radius } else { self.look().corner_radius }
    }

    fn stroke(&self) -> (Color, f32) {
        let look = self.look();
        let width = if self.p.stroke_width >= 0.0 { self.p.stroke_width } else { look.stroke_width };
        (self.p.stroke_color.unwrap_or(look.stroke_color), width)
    }

    fn inner(rect: Rect, padding: Thickness, scale: f32) -> Rect {
        Rect::new(rect.left + padding.left * scale, rect.top + padding.top * scale, rect.right - padding.right * scale, rect.bottom - padding.bottom * scale)
    }

    // ---------------------------------------------------------------- editing

    /// The caret moved or the selection changed: the caret shows, comes into view, and
    /// `on_cursor_moved` runs.
    fn moved(&mut self) -> Dirty {
        self.caret_on = true;
        self.blink_at = 0.0;
        self.reveal = true;
        self.pending |= CURSOR_MOVED;
        Dirty::DRAW_APPLY
    }

    fn select(&mut self, start: usize, end: usize) -> Dirty {
        let (s, e) = (start.min(end).min(self.chars), start.max(end).min(self.chars));
        self.cursor = s;
        self.selection = e - s;
        self.moved()
    }

    /// Replaces the selection with `value` (typing, pasting, deleting); `max_length` cuts it.
    fn replace_selection(&mut self, value: &str) -> Dirty {
        let multiline = self.is_multiline();
        let room = if self.p.max_length >= 0 { (self.p.max_length as usize).saturating_sub(self.chars - self.selection) } else { usize::MAX };
        let (a, b) = (byte_at(&self.p.text, self.cursor), byte_at(&self.p.text, self.cursor + self.selection));
        let inserted;
        if value.contains(['\r', '\n', '\u{2029}']) {
            // Line breaks as the text keeps them: `\n`, a space on a single line.
            let mut normalized = String::with_capacity(value.len());
            let mut chars = value.chars().peekable();
            while let Some(c) = chars.next() {
                let c = match c {
                    '\r' if chars.peek() == Some(&'\n') => continue,
                    '\r' | '\n' | '\u{2029}' => if multiline { '\n' } else { ' ' },
                    c => c,
                };
                normalized.push(c);
            }
            let cut = byte_at(&normalized, room);
            inserted = normalized[..cut].chars().count();
            self.p.text.replace_range(a..b, &normalized[..cut]);
        } else {
            let cut = byte_at(value, room);
            inserted = value[..cut].chars().count();
            if a == b && cut == 0 {
                // A full editor, nothing selected.
                return Dirty::NONE;
            }
            self.p.text.replace_range(a..b, &value[..cut]);
        }
        self.chars = self.chars - self.selection + inserted;
        self.cursor += inserted;
        self.selection = 0;
        self.keyboard_edges = None;
        self.moved();
        self.pending |= TEXT_CHANGED;
        Dirty::MEASURE_APPLY
    }

    /// Deletes a character cluster before (`forward` false) or after the caret, or the selection.
    fn delete(&mut self, forward: bool) -> Dirty {
        if self.selection > 0 {
            return self.replace_selection("");
        }
        let text = &self.p.text;
        let at = byte_at(text, self.cursor);
        let (a, b) = if forward { (at, cluster_end(text, at)) } else { (cluster_start(text, at), at) };
        if a == b {
            return Dirty::NONE;
        }
        let removed = text[a..b].chars().count();
        self.p.text.replace_range(a..b, "");
        self.chars -= removed;
        if !forward {
            self.cursor -= removed;
        }
        self.keyboard_edges = None;
        self.moved();
        self.pending |= TEXT_CHANGED;
        Dirty::MEASURE_APPLY
    }

    /// Arrows, Home, End (DrawnUi.React StubMoveCursor): with Shift the moving edge follows and
    /// the other stays; without, a selection collapses to the side of the move.
    fn move_caret(&mut self, to: impl Fn(&Self, usize) -> usize, backwards: bool, extend: bool) -> Dirty {
        if extend {
            let (stay, moving) = self.keyboard_edges.unwrap_or((self.cursor, self.cursor));
            let moving = to(self, moving).min(self.chars);
            self.keyboard_edges = Some((stay, moving));
            self.cursor = stay.min(moving);
            self.selection = stay.max(moving) - self.cursor;
            return self.moved();
        }
        self.cursor = match (self.selection > 0, backwards) {
            (true, true) => self.cursor,
            (true, false) => self.cursor + self.selection,
            (false, _) => to(self, self.cursor).min(self.chars),
        };
        self.selection = 0;
        self.keyboard_edges = None;
        self.moved()
    }

    /// Enter (DrawnUi.React StubPressEnter): a single line submits; a multiline editor breaks
    /// the line unless `return_type` is Send (then Shift or Alt break it).
    fn press_enter(&mut self, alt: bool, shift: bool) -> Dirty {
        if self.is_multiline() && (alt || shift || self.p.return_type != ReturnType::Send) {
            return self.replace_selection("\n");
        }
        self.pending |= SUBMITTED;
        Dirty::APPLY
    }

    /// The word around a character, or the character itself when it is not a letter, a digit or
    /// `_` (C# SelectWord).
    fn select_word(&mut self, index: usize) -> Dirty {
        if self.chars == 0 {
            return Dirty::NONE;
        }
        let i = index.min(self.chars - 1);
        let text = &self.p.text;
        let at = |k: usize| text[byte_at(text, k)..].chars().next().unwrap_or(' ');
        if !is_word(at(i)) {
            return self.select(i, i + 1);
        }
        let (mut s, mut e) = (i, i + 1);
        while s > 0 && is_word(at(s - 1)) {
            s -= 1;
        }
        while e < self.chars && is_word(at(e)) {
            e += 1;
        }
        self.select(s, e)
    }

    /// Ctrl + arrow: the start of this or the previous word, or the end of this or the next one.
    fn word_edge(&self, from: usize, backwards: bool) -> usize {
        let text = &self.p.text;
        let mut i = from;
        if backwards {
            let before = |k: usize| text[..byte_at(text, k)].chars().next_back().unwrap_or(' ');
            while i > 0 && !is_word(before(i)) {
                i -= 1;
            }
            while i > 0 && is_word(before(i)) {
                i -= 1;
            }
        } else {
            let after = |k: usize| text[byte_at(text, k)..].chars().next().unwrap_or(' ');
            while i < self.chars && !is_word(after(i)) {
                i += 1;
            }
            while i < self.chars && is_word(after(i)) {
                i += 1;
            }
        }
        i
    }

    /// The caret one cluster to the left or the right (DrawnUi.React moves by UTF-16 units).
    fn next_cluster(&self, index: usize, backwards: bool) -> usize {
        let text = &self.p.text;
        let at = byte_at(text, index);
        if backwards {
            index - text[cluster_start(text, at)..at].chars().count()
        } else {
            index + text[at..cluster_end(text, at)].chars().count()
        }
    }

    /// A key while focused (SkiaEditor.Blazor OnKeyDown / KeyChar).
    fn key(&mut self, event: &KeyEvent, label: Option<&SkiaLabel>) -> Option<Dirty> {
        let m = event.modifiers();
        let ctrl = m.ctrl || m.meta;
        self.shift = m.shift;
        if event.kind == KeyKind::Char {
            return Some(self.replace_selection(event.text));
        }
        if event.kind != KeyKind::Down {
            return None;
        }
        Some(match event.key {
            "Backspace" => self.delete(false),
            "Delete" => self.delete(true),
            "Enter" | "NumpadEnter" => self.press_enter(m.alt, m.shift),
            "ArrowLeft" if ctrl => self.move_caret(|me, i| me.word_edge(i, true), true, m.shift),
            "ArrowRight" if ctrl => self.move_caret(|me, i| me.word_edge(i, false), false, m.shift),
            "ArrowLeft" => self.move_caret(|me, i| me.next_cluster(i, true), true, m.shift),
            "ArrowRight" => self.move_caret(|me, i| me.next_cluster(i, false), false, m.shift),
            "ArrowUp" | "ArrowDown" if self.is_multiline() => {
                let up = event.key == "ArrowUp";
                let Some(to) = label.and_then(|l| self.vertical_target(l, up)) else { return Some(Dirty::NONE) };
                self.move_caret(|_, _| to, up, false)
            }
            "Home" => self.move_caret(|_, _| 0, true, m.shift),
            "End" => self.move_caret(|me, _| me.chars, false, m.shift),
            "KeyA" if ctrl => {
                self.keyboard_edges = Some((0, self.chars));
                self.select(0, self.chars)
            }
            _ => return None,
        })
    }

    /// Up / Down: the character under the caret's x on the neighboring line (C# HandleVerticalArrow).
    fn vertical_target(&self, label: &SkiaLabel, up: bool) -> Option<usize> {
        let caret = caret_rect(label, self.cursor, 1.0)?;
        let mut current = None;
        let mut target = None;
        let mut previous = None;
        for (i, line) in label.text_lines().enumerate() {
            if current.is_none() && caret.top >= line.rect.top && caret.top < line.rect.bottom {
                current = Some(i);
                if up {
                    target = previous;
                    break;
                }
                continue;
            }
            if current.is_some() {
                target = Some(line.rect);
                break;
            }
            previous = Some(line.rect);
        }
        let line = target?;
        Some(index_at(label, caret.left, line.center_y()))
    }

    /// Scroll that brings the caret into `inner` (pixels); the label is laid out.
    fn scroll_to_caret(&mut self, label: &SkiaLabel, inner: Rect, label_size: Size, scale: f32) {
        if let Some(caret) = caret_rect(label, self.cursor + self.selection, scale) {
            // Where the caret is in the unscrolled text, from the padded box's corner.
            let (x, y) = (caret.left - inner.left, caret.top - inner.top);
            if x - self.scroll.x < 0.0 {
                self.scroll.x = x;
            } else if x + caret.width() - self.scroll.x > inner.width() {
                self.scroll.x = x + caret.width() - inner.width();
            }
            if y - self.scroll.y < 0.0 {
                self.scroll.y = y;
            } else if y + caret.height() - self.scroll.y > inner.height() {
                self.scroll.y = y + caret.height() - inner.height();
            }
        }
        // The caret after the last character stays in view (DrawnUi.React clamps to the text).
        self.scroll.x = self.scroll.x.min(label_size.width + CARET * scale - inner.width()).max(0.0);
        self.scroll.y = self.scroll.y.min(label_size.height - inner.height()).max(0.0);
    }

    /// Runs the handler work a change left and keeps the blink going (see `tick`).
    fn wake(&mut self, tree: &mut Tree) {
        if self.pending == 0 && !self.focused {
            return;
        }
        if self.ticking {
            animators::sleep(tree, self.id, 0.0);
        } else {
            self.ticking = true;
            animators::start_frame(tree, self.id, tick);
        }
    }

    /// The text and the look into the labels (C# UpdateLabel).
    fn update_labels(&mut self, cx: &mut Cx) {
        let (multiline, p) = (self.is_multiline(), &self.p);
        if let Some(mut label) = cx.get_mut(self.label) {
            let l = label.control_mut();
            let mut measure = false;
            if p.is_password {
                if l.p.text.chars().count() != self.chars || l.p.text.chars().any(|c| c != '\u{2022}') {
                    l.p.text.clear();
                    l.p.text.extend(std::iter::repeat_n('\u{2022}', self.chars));
                    measure = true;
                }
            } else if l.p.text != p.text {
                l.p.text.clone_from(&p.text);
                measure = true;
            }
            measure |= sync_string(&mut l.p.font_family, &p.font_family);
            measure |= sync_string(&mut l.p.font_family_fallback, &p.font_family_fallback);
            measure |= std::mem::replace(&mut l.p.system_font_fallback, p.use_unicode) != p.use_unicode;
            if measure {
                label.mark(Dirty::MEASURE);
            }
            label.set_font_size(p.font_size);
            label.set_font_weight(p.font_weight);
            label.set_text_color(p.text_color);
            label.set_line_height(p.line_height);
            label.set_horizontal_text_alignment(p.horizontal_text_alignment);
            label.set_vertical_text_alignment(p.vertical_text_alignment);
            label.set_line_break_mode(if multiline { LineBreakMode::WordWrap } else { LineBreakMode::NoWrap });
            label.set_horizontal_options(if multiline { LayoutOptions::Fill } else { LayoutOptions::Start });
            if label.base().p.fill_gradient != p.text_gradient {
                label.set_fill_gradient(p.text_gradient.clone());
            }
        }
        let show = self.chars == 0 && !p.placeholder_text.is_empty();
        if let Some(mut placeholder) = cx.get_mut(self.placeholder) {
            let l = placeholder.control_mut();
            let mut measure = sync_string(&mut l.p.text, &p.placeholder_text);
            measure |= sync_string(&mut l.p.font_family, &p.font_family);
            measure |= sync_string(&mut l.p.font_family_fallback, &p.font_family_fallback);
            measure |= std::mem::replace(&mut l.p.system_font_fallback, p.use_unicode) != p.use_unicode;
            if measure {
                placeholder.mark(Dirty::MEASURE);
            }
            placeholder.set_font_size(p.font_size);
            placeholder.set_font_weight(p.font_weight);
            placeholder.set_text_color(p.placeholder_color);
            placeholder.set_line_height(p.line_height);
            placeholder.set_horizontal_text_alignment(p.placeholder_horizontal_alignment);
            placeholder.set_max_lines(if multiline { -1 } else { 1 });
            placeholder.set_horizontal_options(LayoutOptions::Fill);
            placeholder.set_is_visible(show);
        }
    }
}

/// Copies `from` into `to` when they differ, keeping `to`'s buffer. True when it changed.
fn sync_string(to: &mut String, from: &str) -> bool {
    if to == from {
        return false;
    }
    to.clear();
    to.push_str(from);
    true
}

/// Byte offset of character `index` (the end past the last one).
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(b, _)| b)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A character that joins the one before it into one cluster.
fn extends(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F // combining marks, keycap
        | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF // variation selectors
        | 0x1F3FB..=0x1F3FF // skin tones
        | 0xE0020..=0xE007F // tags
        | 0x200D) // zero-width joiner
}

fn regional(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Byte offset past the cluster that starts at `at`.
fn cluster_end(text: &str, at: usize) -> usize {
    let mut chars = text[at..].char_indices().peekable();
    let Some((_, first)) = chars.next() else { return at };
    let mut end = at + first.len_utf8();
    if regional(first) && let Some(&(i, c)) = chars.peek().filter(|(_, c)| regional(*c)) {
        end = at + i + c.len_utf8();
        chars.next();
    }
    while let Some(&(i, c)) = chars.peek() {
        if !extends(c) {
            break;
        }
        chars.next();
        end = at + i + c.len_utf8();
        // A joiner takes the next character with it.
        if c == '\u{200D}' && let Some((i, c)) = chars.next() {
            end = at + i + c.len_utf8();
        }
    }
    end
}

/// Byte offset where the cluster that ends at `at` starts. Clusters never cross a line break.
fn cluster_start(text: &str, at: usize) -> usize {
    let mut start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    if start == at {
        return text[..at].char_indices().next_back().map_or(0, |(b, _)| b);
    }
    loop {
        let end = cluster_end(text, start);
        if end >= at {
            return start;
        }
        start = end;
    }
}

/// The caret before character `index` of a laid-out label (DrawnUi.React CaretRectLabelPx): at
/// the glyph that starts there, else after the one that ends there. Pixels, label coordinates.
fn caret_rect(label: &SkiaLabel, index: usize, scale: f32) -> Option<Rect> {
    let mut end_of_line = None;
    let mut first = None;
    for line in label.text_lines() {
        let r = line.rect;
        first.get_or_insert(r);
        if index >= line.start && index < line.end() {
            return Some(Rect::from_xywh(line.x_of(index), r.top, CARET * scale, r.height()));
        }
        if index == line.end() && end_of_line.is_none() {
            end_of_line = Some(Rect::from_xywh(line.x_of(index), r.top, CARET * scale, r.height()));
        }
    }
    end_of_line.or_else(|| first.map(|r| Rect::from_xywh(r.left, r.top, CARET * scale, r.height())))
}

/// The character nearest to a point (C# GetCursorPosition, DrawnUi.React HitTestIndex).
fn index_at(label: &SkiaLabel, x: f32, y: f32) -> usize {
    let mut chosen: Option<TextLine> = None;
    for line in label.text_lines() {
        let below = y >= line.rect.bottom;
        chosen = Some(line);
        if !below {
            break;
        }
    }
    let Some(line) = chosen else { return 0 };
    let mut left = line.rect.left;
    if x <= left {
        return line.start;
    }
    for (k, w) in line.advances.iter().enumerate() {
        if x < left + w {
            return line.start + k + (x >= left + w / 2.0) as usize;
        }
        left += w;
    }
    line.end()
}

impl Has<EditorProps> for SkiaEditor {
    fn part(&self) -> &EditorProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut EditorProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaEditor {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Build<SkiaEditor> {
    /// Runs after the text changed, by typing or by the app, with the new text (C# TextChanged).
    /// Changes within one frame are reported once, on the next frame.
    pub fn on_text_changed<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaEditor>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        self.control_mut().text_changed = Some(Box::new(move |raw, state, cx, text| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx, text)
        }));
        self
    }

    /// Runs after the caret or the selection moved (DrawnUi.React CursorMoved), once per frame.
    pub fn on_cursor_moved<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaEditor>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.control_mut().cursor_moved = Some(Box::new(move |raw, state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx)
        }));
        self
    }

    /// Runs when Enter submits (C# TextSubmitted), with the text.
    pub fn on_text_submitted<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaEditor>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        self.control_mut().submitted = Some(Box::new(move |raw, state, cx, text| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx, text)
        }));
        self
    }
}

impl Mut<'_, SkiaEditor> {
    fn apply(&mut self, change: impl FnOnce(&mut SkiaEditor) -> Dirty) {
        let dirty = change(self.control_mut());
        self.mark(dirty);
    }

    /// Moves the caret; the selection keeps its length where it fits.
    pub fn set_cursor_position(&mut self, index: usize) {
        self.apply(|e| {
            let s = index.min(e.chars);
            e.select(s, s + e.selection)
        });
    }

    /// Selects `length` characters after the caret.
    pub fn set_selection_length(&mut self, length: usize) {
        self.apply(|e| e.select(e.cursor, e.cursor + length));
    }

    /// Selects from `start` to `end` (characters, either order).
    pub fn set_selection(&mut self, start: usize, end: usize) {
        self.apply(|e| e.select(start, end));
    }

    /// Selects the whole text.
    pub fn select_all(&mut self) {
        self.apply(|e| e.select(0, e.chars));
    }

    /// Selects the word around a character.
    pub fn select_word(&mut self, index: usize) {
        self.apply(|e| e.select_word(index));
    }

    /// Types `value` over the selection.
    pub fn insert_at_cursor(&mut self, value: &str) {
        self.apply(|e| e.replace_selection(value));
    }

    /// Deletes the selected text.
    pub fn delete_selection(&mut self) {
        self.apply(|e| if e.selection > 0 { e.replace_selection("") } else { Dirty::NONE });
    }

    /// Puts the selected text on the clipboard.
    pub fn copy_selection(&mut self, cx: &mut Cx) {
        if self.selection > 0 {
            cx.set_clipboard(self.selected_text().to_owned());
        }
    }

    /// Puts the selected text on the clipboard and deletes it.
    pub fn cut_selection(&mut self, cx: &mut Cx) {
        self.copy_selection(cx);
        self.delete_selection();
    }

    /// Types the clipboard's text over the selection when the host delivers it (it arrives as
    /// typed text, while the editor has the focus).
    pub fn paste_from_clipboard(&mut self, cx: &mut Cx) {
        cx.request_paste();
    }

    /// C# Submit: a multiline editor that does not send breaks the line; otherwise
    /// `on_text_submitted` runs and a single-line editor gives up the focus.
    pub fn submit(&mut self, cx: &mut Cx) {
        if self.is_multiline() && self.p.return_type != ReturnType::Send {
            self.insert_at_cursor("\n");
            return;
        }
        self.apply(|e| {
            e.pending |= SUBMITTED;
            Dirty::APPLY
        });
        if !self.is_multiline() && self.focused {
            cx.focus(None::<ControlId>);
        }
    }
}

/// The editor's frame animator: runs the handlers changes left (they need the app state), and
/// blinks the caret while focused, asleep between blinks.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut result = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return result };
    let mut queue = Vec::new();
    let mut redraw = false;
    let mut wake_at = None;
    if let Some(control) = node.kind.as_deref_mut() {
        for event in [TEXT_CHANGED, CURSOR_MOVED, SUBMITTED] {
            let Some(editor) = part_mut::<SkiaEditor>(control) else { break };
            if editor.pending & event == 0 {
                continue;
            }
            editor.pending &= !event;
            let mut text = std::mem::take(&mut editor.scratch);
            text.clone_from(&editor.p.text);
            if event == TEXT_CHANGED {
                editor.reported.clone_from(&editor.p.text);
            }
            let handler = match event {
                TEXT_CHANGED => editor.text_changed.take().map(|h| (h, true)),
                SUBMITTED => editor.submitted.take().map(|h| (h, false)),
                _ => None,
            };
            let moved = if event == CURSOR_MOVED { editor.cursor_moved.take() } else { None };
            if let Some((mut handler, changed)) = handler {
                handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, &text);
                result.state_touched = true;
                if let Some(editor) = part_mut::<SkiaEditor>(control) {
                    if changed { editor.text_changed = Some(handler) } else { editor.submitted = Some(handler) }
                }
            }
            if let Some(mut handler) = moved {
                handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree });
                result.state_touched = true;
                if let Some(editor) = part_mut::<SkiaEditor>(control) {
                    editor.cursor_moved = Some(handler);
                }
            }
            if let Some(editor) = part_mut::<SkiaEditor>(control) {
                editor.scratch = text;
            }
        }
        if let Some(editor) = part_mut::<SkiaEditor>(control) {
            if editor.focused {
                if editor.blink_at == 0.0 {
                    editor.blink_at = time_ms + BLINK_MS;
                } else if time_ms >= editor.blink_at {
                    editor.caret_on = !editor.caret_on;
                    editor.blink_at = time_ms + BLINK_MS;
                    redraw = true;
                }
                wake_at = Some(editor.blink_at);
            }
            // A handler may have left more work; it asked for a wake then.
            result.keep = editor.focused || editor.pending != 0;
            editor.ticking = result.keep;
        }
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    if redraw {
        cx.tree.invalidate(id, Dirty::DRAW);
    }
    if let Some(at) = wake_at.filter(|_| result.keep) {
        let pending = cx.tree.find::<SkiaEditor>(id).is_some_and(|e| e.pending != 0);
        animators::sleep(cx.tree, id, if pending { 0.0 } else { at });
    }
    result
}

impl Control for SkiaEditor {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The text normalized and clamped, the labels updated, the handlers scheduled.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        let multiline = self.is_multiline();
        let text = &mut self.p.text;
        if text.contains(['\r', '\u{2029}']) || (!multiline && text.contains('\n')) {
            let normalized = text.replace("\r\n", "\n").replace(['\r', '\u{2029}'], "\n");
            *text = if multiline { normalized } else { normalized.replace('\n', " ") };
        }
        if self.p.max_length >= 0 && text.chars().nth(self.p.max_length as usize).is_some() {
            text.truncate(byte_at(text, self.p.max_length as usize));
        }
        self.chars = text.chars().count();
        self.cursor = self.cursor.min(self.chars);
        self.selection = self.selection.min(self.chars - self.cursor);
        if self.p.text != self.reported {
            self.pending |= TEXT_CHANGED;
            self.reveal = true;
        }
        self.update_labels(cx);
        if self.blink_at == 0.0 && self.focused {
            self.caret_on = true;
        }
        self.wake(cx.tree);
    }

    /// The label's lines and padding (DrawnUi.React MeasureAbsolute): `max_lines` lines high, or
    /// the lines there are with `auto_height`; as wide as the constraint.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, _height: f32) -> Size {
        let scale = cx.scale;
        let padding = cx.base().p.padding;
        let (px, py) = (padding.horizontal() * scale, padding.vertical() * scale);
        let inner_w = if width.is_finite() { (width - px).max(0.0) } else { f32::INFINITY };
        let multiline = self.is_multiline();
        let text = cx.measure_child(self.label.id(), if multiline { inner_w } else { f32::INFINITY }, f32::INFINITY);
        let placeholder_shown = cx.child_base(self.placeholder.id()).p.is_visible;
        if placeholder_shown {
            cx.measure_child(self.placeholder.id(), inner_w, f32::INFINITY);
        }
        let label = cx.tree.find::<SkiaLabel>(self.label).map_or((0.0, 1), |l| (l.measured_line_height(), l.lines_count()));
        let mut line = label.0;
        if line <= 0.0 {
            line = (self.p.font_size.max(1.0) * 1.2 * self.p.line_height.max(1.0)).ceil() * scale;
        }
        let mut lines = if self.p.max_lines > 0 { self.p.max_lines as usize } else { 1 };
        if self.p.auto_height && multiline {
            let actual = label.1.max(1);
            lines = if self.p.max_lines > 0 { actual.min(self.p.max_lines as usize) } else { actual };
        }
        let w = if width.is_finite() { width } else { text.width + px };
        Size::new(w, (line * lines as f32).ceil() + py)
    }

    /// The label at the padded box, aligned there when it is smaller, scrolled by the content
    /// offset; the placeholder at the padded box.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        let scale = cx.scale;
        let inner = Self::inner(cx.base().rect, cx.base().p.padding, scale);
        let size = cx.child_base(self.label.id()).measured;
        let multiline = self.is_multiline();
        let offset = |align: TextAlignment, free: f32| match align {
            TextAlignment::Start => 0.0,
            TextAlignment::Center => (free / 2.0).max(0.0),
            TextAlignment::End => free.max(0.0),
        };
        let x = if multiline { 0.0 } else { offset(self.p.horizontal_text_alignment, inner.width() - size.width) };
        let y = offset(self.p.vertical_text_alignment, inner.height() - size.height);
        let width = if multiline { inner.width() } else { size.width };
        cx.arrange_child(self.label.id(), Rect::from_xywh(inner.left + x, inner.top + y, width, size.height));
        if cx.child_base(self.placeholder.id()).p.is_visible {
            cx.arrange_child(self.placeholder.id(), inner);
        }
        let box_size = Size::new(width + x, size.height + y);
        if let Some(label) = cx.tree.find::<SkiaLabel>(self.label) {
            if self.reveal {
                self.scroll_to_caret(label, inner, box_size, scale);
            } else {
                self.scroll.x = self.scroll.x.min(box_size.width + CARET * scale - inner.width()).max(0.0);
                self.scroll.y = self.scroll.y.min(box_size.height - inner.height()).max(0.0);
            }
        }
        self.reveal = false;
        cx.base_mut().content_offset = Point::new(-self.scroll.x, -self.scroll.y);
    }

    fn paint_background(&self, cx: &mut PaintCx) {
        let radius = self.corner_radius() * cx.scale;
        let frame = RRect::new_rect_xy(cx.rect, radius, radius);
        let unset = cx.base().p.background_color.is_none() && cx.base().p.fill_gradient.is_none();
        let fill = paint::background_paint(cx, cx.rect, (0.0, 0.0)).or_else(|| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(self.look().background);
            unset.then_some(paint)
        });
        if let Some(fill) = fill {
            cx.canvas.draw_rrect(frame, &fill);
        }
    }

    /// The selection under the text, the text inside the frame, the border, the caret above.
    fn paint(&self, cx: &mut PaintCx) {
        let scale = cx.scale;
        let base = cx.base();
        let inner = Self::inner(cx.rect, base.p.padding, scale);
        // From layout coordinates (the label's) to where this paint draws, scrolled.
        let shift = Point::new(cx.rect.left - base.rect.left - self.scroll.x, cx.rect.top - base.rect.top - self.scroll.y);
        let label = cx.node(self.label.id()).and_then(|n| part::<SkiaLabel>(n.kind.as_deref()?));
        let canvas = cx.canvas;
        if self.focused && self.selection > 0 && let Some(label) = label {
            let (start, end) = (self.cursor, self.cursor + self.selection);
            let mut paint = Paint::default();
            paint.set_color(self.p.selection_color);
            canvas.save();
            canvas.clip_rect(inner, ClipOp::Intersect, true);
            for line in label.text_lines() {
                let (s, e) = (start.max(line.start), end.min(line.end()));
                if s >= e && !(s == e && line.advances.is_empty() && start <= line.start && line.start < end) {
                    continue;
                }
                let (left, right) = (line.x_of(s), line.x_of(e).max(line.x_of(s) + CARET * scale));
                canvas.draw_rect(Rect::new(left, line.rect.top, right, line.rect.bottom).with_offset(shift), &paint);
            }
            canvas.restore();
        }
        let radius = self.corner_radius() * scale;
        let (stroke_color, stroke_width) = self.stroke();
        let stroke = if stroke_color.a() > 0 { stroke_width * scale } else { 0.0 };
        let mut frame = RRect::new_rect_xy(cx.rect, radius, radius);
        frame.inset((stroke / 2.0, stroke / 2.0));
        canvas.save();
        canvas.clip_rrect(frame, ClipOp::Intersect, true);
        cx.paint_children();
        canvas.restore();
        let canvas = cx.canvas;
        if stroke > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(stroke);
            paint.set_color(stroke_color);
            canvas.draw_rrect(frame, &paint);
        }
        if self.focused && self.p.can_show_cursor && self.caret_on
            && let Some(caret) = label.and_then(|l| caret_rect(l, self.cursor + self.selection, scale))
        {
            let mut paint = Paint::default();
            paint.set_color(self.cursor_color());
            canvas.save();
            canvas.clip_rect(inner, ClipOp::Intersect, true);
            canvas.draw_rect(caret.with_offset(shift), &paint);
            canvas.restore();
        }
    }

    /// Down focuses and places the caret (Shift extends the selection), a drag selects, a double
    /// tap or a long press selects the word (C# ProcessGestures).
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let point = cx.point + self.scroll;
        let label = cx.tree.find::<SkiaLabel>(self.label);
        let dirty = match gesture.kind {
            GestureKind::Down => {
                let Some(label) = label else { return Handled::Yes };
                let pos = index_at(label, point.x, point.y);
                let scale = cx.base().scale;
                let near = |at: Point| (at - gesture.location).length() < TAPPED_CANCEL_MOVE_THRESHOLD_POINTS * scale;
                let double = self.last_down.is_some_and(|(t, at)| gesture.time_ms - t < DOUBLE_TAP_MS && near(at));
                self.last_down = if double { None } else { Some((gesture.time_ms, gesture.location)) };
                let dirty = if double {
                    let dirty = self.select_word(pos);
                    self.drag_anchor = Some(self.cursor);
                    dirty
                } else if self.shift && self.focused {
                    // The far edge of the selection stays (C#).
                    let (left, right) = (self.cursor, self.cursor + self.selection);
                    let anchor = if self.selection > 0 { if pos - left.min(pos) <= right.max(pos) - pos { right } else { left } } else { self.cursor };
                    self.drag_anchor = Some(anchor);
                    self.keyboard_edges = Some((anchor, pos));
                    self.select(anchor, pos)
                } else {
                    self.drag_anchor = Some(pos);
                    self.keyboard_edges = None;
                    self.select(pos, pos)
                };
                if !self.focused {
                    cx.focus();
                }
                dirty
            }
            GestureKind::Panning => {
                let (Some(label), Some(anchor)) = (label, self.drag_anchor) else { return Handled::Yes };
                let pos = index_at(label, point.x, point.y);
                if (self.cursor, self.cursor + self.selection) == (anchor.min(pos), anchor.max(pos)) {
                    return Handled::Yes;
                }
                self.select(anchor, pos)
            }
            GestureKind::LongPressing => {
                let Some(label) = label else { return Handled::Yes };
                let start = gesture.start + self.scroll - (gesture.location - cx.point);
                let dirty = self.select_word(index_at(label, start.x, start.y));
                self.drag_anchor = Some(self.cursor);
                if !self.focused {
                    cx.focus();
                }
                dirty
            }
            GestureKind::Up => {
                self.drag_anchor = None;
                return Handled::Yes;
            }
            GestureKind::Tapped => return Handled::Yes,
            _ => return Handled::No,
        };
        self.after_change(cx, dirty);
        Handled::Yes
    }

    fn on_key(&mut self, cx: &mut GestureCx, event: &KeyEvent) -> bool {
        let ctrl = event.modifiers().ctrl || event.modifiers().meta;
        if event.kind == KeyKind::Down && ctrl {
            match event.key {
                "KeyC" | "KeyX" if self.selection > 0 => {
                    cx.cx().set_clipboard(self.selected_text().to_owned());
                    if event.key == "KeyX" {
                        let dirty = self.replace_selection("");
                        self.after_change(cx, dirty);
                    }
                    return true;
                }
                "KeyC" | "KeyX" => return true,
                "KeyV" => {
                    cx.cx().request_paste();
                    return true;
                }
                _ => {}
            }
        }
        if event.kind == KeyKind::Down && event.key == "Escape" {
            cx.unfocus();
            return true;
        }
        let label = cx.tree.find::<SkiaLabel>(self.label);
        let Some(dirty) = self.key(event, label) else {
            // Up and Down stay with the text while editing, also on one line (C# OnAccessibilityKey):
            // a group around the editor does not move.
            return event.kind == KeyKind::Down && matches!(event.key, "ArrowUp" | "ArrowDown");
        };
        self.after_change(cx, dirty);
        true
    }

    fn on_focus_changed(&mut self, cx: &mut GestureCx, focused: bool) {
        self.focused = focused;
        self.caret_on = true;
        self.blink_at = 0.0;
        if !focused {
            self.selection = 0;
            self.drag_anchor = None;
            self.keyboard_edges = None;
            self.shift = false;
        }
        cx.invalidate(Dirty::DRAW);
        self.wake(cx.tree);
    }

    fn wants_text_input(&self) -> bool {
        true
    }

    /// The I-beam over the editor.
    fn cursor(&self, _local: Point) -> Option<Cursor> {
        Some(Cursor::Text)
    }

    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::TEXT_BOX)
    }

    /// The text, or the placeholder; a password field says its placeholder only (DrawnUi.React
    /// speaks the password).
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        let text = if self.p.text.is_empty() || self.p.is_password { &self.p.placeholder_text } else { &self.p.text };
        (!text.is_empty()).then(|| text.as_str().into())
    }

    /// A tab stop without a tapped handler (C# SkiaEditor).
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(true)
    }
}

impl SkiaEditor {
    /// After an edit or a move from a gesture or a key: the caret comes into view now when only
    /// it moved (the layout is current), after the next layout when the text changed.
    fn after_change(&mut self, cx: &mut GestureCx, dirty: Dirty) {
        if dirty.is_empty() {
            return;
        }
        // The handlers run in this frame's animator tick.
        self.wake(cx.tree);
        if !dirty.contains(Dirty::MEASURE) && self.reveal {
            let base = cx.base();
            let (inner, scale) = (Self::inner(base.rect, base.p.padding, base.scale), base.scale);
            if let (Some(label), Some(label_base)) = (cx.tree.find::<SkiaLabel>(self.label), cx.tree.base(self.label)) {
                let r = label_base.rect;
                let size = Size::new(r.right - inner.left, r.bottom - inner.top);
                self.scroll_to_caret(label, inner, size, scale);
                self.reveal = false;
                cx.tree.set_content_offset(cx.id, Point::new(-self.scroll.x, -self.scroll.y));
            }
        }
        cx.invalidate(dirty);
    }
}
