//! SkiaEditor: drawn text input with caret, selection, placeholder, password, multiline and the
//! platform looks. Ported from the React demo's EditorPage.tsx.

use drawnui::prelude::*;

use super::{card, card_title, page_title, scrolling};
use crate::{App, hex};

const FALLBACK: &str = "FontSymbols,FontSymbols2,FontEmoji";

/// What the page keeps.
#[derive(Default)]
pub struct State {
    editor: Handle<SkiaEditor>,
    pub(super) text: String,
    /// "cursor … · selection … · … chars".
    status: String,
    pub(super) submitted: String,
    pub(super) focused: bool,
    pub(super) chat: Vec<String>,
    password_chars: usize,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    let page = &mut app.editor;
    scrolling(SkiaStack::new().spacing(16).padding((16, 16, 16, 120)).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        page_title("SkiaEditor"),
        card(
            card_title("").observe(|me, app: &App| {
                let page = &app.editor;
                me.set_text(format!(
                    "Single line — Text=\"{}\" · IsFocused={} · {} · submitted: \"{}\"",
                    page.text, page.focused, page.status, page.submitted
                ))
            }),
            (
                editor()
                    .placeholder_text("Type here, Enter submits")
                    .font_size(16)
                    .assign(&mut page.editor)
                    .on_text_changed(|me, app: &mut App, _cx, text| {
                        app.editor.text = text.to_owned();
                        describe(me, &mut app.editor);
                    })
                    .on_cursor_moved(|me, app: &mut App, _cx| describe(me, &mut app.editor))
                    .on_focus_changed(|_me, app: &mut App, _cx, focused| app.editor.focused = focused)
                    .on_text_submitted(|_me, app: &mut App, _cx, text| app.editor.submitted = text.to_owned()),
                SkiaWrap::new().spacing(8).children((
                    button("Focus", 0x0D6EFD, |app, cx| cx.focus(Some(app.editor.editor))),
                    button("SelectAll()", 0x495057, |app, cx| {
                        if let Some(mut editor) = cx.get_mut(app.editor.editor) {
                            editor.select_all();
                        }
                    }),
                    button("InsertAtCursor('🙂')", 0x495057, |app, cx| {
                        if let Some(mut editor) = cx.get_mut(app.editor.editor) {
                            editor.insert_at_cursor("🙂");
                        }
                    })
                    .font_family_fallback("FontSymbols,FontSymbols2,FontEmoji"),
                    button("Set Text", 0x495057, |app, cx| {
                        if let Some(mut editor) = cx.get_mut(app.editor.editor) {
                            editor.set_text("Hello from code");
                        }
                    }),
                    button("Clear", 0x495057, |app, cx| {
                        if let Some(mut editor) = cx.get_mut(app.editor.editor) {
                            editor.set_text("");
                        }
                    }),
                )),
                note("A focused editor is mirrored by the host's hidden text input: IME composition, the mobile soft keyboard, autocorrect, native paste / cut / undo land in the drawn editor through the same calls the physical keyboard uses."),
            ),
        ),
        card(
            card_title("ControlStyle — Cupertino, Material, Material3, Windows (C# ApplyControlStyleVisuals palettes)"),
            SkiaWrap::new().spacing(10).children((
                styled(PrebuiltControlStyle::Cupertino, "Cupertino"),
                styled(PrebuiltControlStyle::Material, "Material"),
                styled(PrebuiltControlStyle::Material3, "Material3"),
                styled(PrebuiltControlStyle::Windows, "Windows"),
            )),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "IsPassword — {} chars hidden behind bullets, KeyboardType Numeric / Email input modes",
                    app.editor.password_chars
                ))
            }),
            SkiaWrap::new().spacing(10).children((
                editor()
                    .is_password(true)
                    .placeholder_text("Password")
                    .width_request(220)
                    .on_text_changed(|_me, app: &mut App, _cx, text| app.editor.password_chars = text.chars().count()),
                editor().keyboard_type(KeyboardType::Numeric).placeholder_text("Numeric (inputmode)").width_request(220),
                editor().keyboard_type(KeyboardType::Email).placeholder_text("Email").width_request(220),
            )),
        ),
        card(
            card_title("Multiline — MaxLines={4}: Enter inserts a line, the box scrolls to the caret"),
            editor()
                .max_lines(4)
                .placeholder_text("Write a few lines… wrapping, arrows, Shift+arrows select, double tap / long press selects a word")
                .font_size(15),
        ),
        card(
            card_title("Multiline + AutoHeight — MaxLines={-1}: the editor grows with the text"),
            editor().max_lines(-1).auto_height(true).placeholder_text("Grows as you type").font_size(15).text("First line\nSecond line"),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "Chat input — MaxLines={{3}} ReturnType=\"Send\": Enter submits and keeps focus, Shift+Enter breaks the line · {} sent",
                    app.editor.chat.len()
                ))
            }),
            (
                // The last four messages.
                SkiaStack::new().spacing(6).children((0..4).map(message).collect::<Vec<_>>()),
                editor().max_lines(3).return_type(ReturnType::Send).placeholder_text("Message").font_size(15).on_text_submitted(
                    |me, app: &mut App, _cx, text| {
                        if !text.trim().is_empty() {
                            app.editor.chat.push(text.to_owned());
                            me.set_text("");
                        }
                    },
                ),
            ),
        ),
    )))
}

/// An editor with the page's glyph fallback (symbols and emoji).
fn editor() -> Build<SkiaEditor> {
    SkiaEditor::new().font_family_fallback(FALLBACK)
}

fn styled(style: PrebuiltControlStyle, name: &str) -> Build<SkiaEditor> {
    editor().control_style(style).placeholder_text(name).width_request(200)
}

/// The cursor, the selection and the length, as the React page's `describe`.
fn describe(editor: &Mut<'_, SkiaEditor>, page: &mut State) {
    page.status = format!(
        "cursor {} · selection {} · {} chars",
        editor.cursor_position(),
        editor.selection_length(),
        editor.p.text.chars().count()
    );
}

/// Chat bubble `slot` of the last four, hidden while there is no message for it.
fn message(slot: usize) -> Build<SkiaLabel> {
    SkiaLabel::new("")
        .font_size(13)
        .text_color(hex(0xDEE2E6))
        .background_color(hex(0x0F3460))
        .padding((10, 6))
        .horizontal_options(LayoutOptions::End)
        .is_visible(false)
        .observe(move |me, app: &App| {
            let chat = &app.editor.chat;
            let shown = chat.len().min(4);
            match (slot < shown).then(|| &chat[chat.len() - shown + slot]) {
                Some(text) => {
                    me.set_text(text.as_str());
                    me.set_is_visible(true);
                }
                None => me.set_is_visible(false),
            }
        })
}

fn button(caption: &str, color: u32, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption).background_color(hex(color)).font_size(13).on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

fn note(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(hex(0xADB5BD)).fill_x()
}
