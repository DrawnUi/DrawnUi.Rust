//! Tooltips & Menus, ported from HelloMaui's TooltipsMenusPage: a desktop menu bar, a context menu
//! (right-click anywhere on the page) and a dropdown in popup windows, then tooltips on any control,
//! icon buttons showing their accessibility label.

use drawnui::Detached;
use drawnui::prelude::*;

use super::{column, page_title, scrolling};
use crate::{App, hex};

const CARD: Color = hex(0x2B3035);
const BUTTON: Color = hex(0x373B3E);
const BUTTON_HOVER: Color = hex(0x495057);
const MUTED: Color = hex(0xADB5BD);
const ACCENT: Color = hex(0x6EA8FE);
/// The menus' panel, edge and the line under the mouse.
const PANEL: Color = hex(0x262B3B);
const EDGE: Color = hex(0x3B4256);
const HIGHLIGHT: Color = hex(0x343B52);
/// Selected: the open menu's title and the keyboard's place, edged in the accent.
pub(crate) const SELECTED: Color = hex(0x1E3A66);

/// Every mouse button taps (DrawnUI); what opens or picks here acts on the primary one only, so a
/// right click on a button opens the page's context menu alone. Keyboard and touch are primary.
fn primary(cx: &Cx) -> bool {
    cx.gesture().is_none_or(|gesture| gesture.button == MouseButton::Left)
}

/// Menus are drawn on the CPU: on Windows their window then has per-pixel alpha, so the round
/// corners show what is behind them.
fn menu_window() -> PopupWindowOptions {
    PopupWindowOptions { cpu: true, ..Default::default() }
}

const ICON_NEW: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm4 18H6V4h7v5h5zM11 11h2v3h3v2h-3v3h-2v-3H8v-2h3z'/></svg>";
const ICON_OPEN: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M20 6h-8l-2-2H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2m0 12H4V8h16z'/></svg>";
const ICON_SAVE: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M17 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V7zm2 16H5V5h11.17L19 7.83zM12 12a3 3 0 1 0 0 6 3 3 0 0 0 0-6M6 6h9v4H6z'/></svg>";
const ICON_SETTINGS: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M19.4 13a7.5 7.5 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.6 7.6 0 0 0-1.7-1L15 3h-4l-.4 2.9a7.6 7.6 0 0 0-1.7 1l-2.5-1-2 3.5L6.6 11a7.5 7.5 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1a7.6 7.6 0 0 0 1.7 1L11 21h4l.4-2.9a7.6 7.6 0 0 0 1.7-1l2.5 1 2-3.5zM13 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7'/></svg>";
const ICON_DELETE: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6zm2-10h8v10H8zm7.5-5-1-1h-5l-1 1H5v2h14V4z'/></svg>";
const ICON_INFO: &str = "<svg viewBox='0 0 24 24'><path fill='#fff' d='M11 7h2v2h-2zm0 4h2v6h-2zm1-9a10 10 0 1 0 0 20 10 10 0 0 0 0-20m0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16'/></svg>";

/// One line of a menu bar menu; `text` `None` = a separator. `key` + `ctrl`: its shortcut.
struct Entry {
    text: Option<&'static str>,
    shortcut: Option<&'static str>,
    key: &'static str,
    ctrl: bool,
}

const fn line(text: &'static str, shortcut: &'static str, key: &'static str) -> Entry {
    Entry { text: Some(text), shortcut: Some(shortcut), key, ctrl: true }
}

const SEPARATOR: Entry = Entry { text: None, shortcut: None, key: "", ctrl: false };

struct BarMenu {
    title: &'static str,
    /// Alt + this key opens it.
    accelerator: &'static str,
    entries: &'static [Entry],
}

static MENUS: [BarMenu; 4] = [
    BarMenu {
        title: "File",
        accelerator: "KeyF",
        entries: &[line("New", "Ctrl+N", "KeyN"), line("Open…", "Ctrl+O", "KeyO"), line("Save", "Ctrl+S", "KeyS"), SEPARATOR, line("Close", "Ctrl+W", "KeyW")],
    },
    BarMenu {
        title: "Edit",
        accelerator: "KeyE",
        entries: &[
            line("Undo", "Ctrl+Z", "KeyZ"),
            line("Redo", "Ctrl+Y", "KeyY"),
            SEPARATOR,
            line("Cut", "Ctrl+X", "KeyX"),
            line("Copy", "Ctrl+C", "KeyC"),
            line("Paste", "Ctrl+V", "KeyV"),
        ],
    },
    BarMenu {
        title: "View",
        accelerator: "KeyV",
        entries: &[
            line("Zoom in", "Ctrl+=", "Equal"),
            line("Zoom out", "Ctrl+-", "Minus"),
            SEPARATOR,
            Entry { text: Some("Full screen"), shortcut: Some("F11"), key: "F11", ctrl: false },
        ],
    },
    BarMenu {
        title: "Help",
        accelerator: "KeyH",
        entries: &[Entry { text: Some("Documentation"), shortcut: Some("F1"), key: "F1", ctrl: false }, SEPARATOR, Entry { text: Some("About"), shortcut: None, key: "", ctrl: false }],
    },
];

/// The lines of a menu (separators left out): (entry index, entry).
fn lines(menu: usize) -> impl Iterator<Item = (usize, &'static Entry)> {
    MENUS[menu].entries.iter().enumerate().filter(|(_, entry)| entry.text.is_some())
}

#[derive(Default)]
pub struct State {
    titles: [Handle<SkiaShape>; 4],
    /// The title under the mouse.
    hovered: Option<usize>,
    /// The menu bar menu open and its popup.
    open: Option<usize>,
    bar_popup: Option<PopupId>,
    /// The highlighted line of the open menu (its index among the lines).
    highlighted: Option<usize>,
    /// The open menu's lines, for the screen readers' focus.
    items: Vec<Handle<SkiaShape>>,
    /// Enter or Space on a title: the menu it opens has its first line highlighted.
    keyboard_open: bool,
    /// The title the keyboard is on with no menu open (Alt alone put it on the bar).
    bar_title: Option<usize>,
    /// The keyboard drives the menus: its place reads selected and a resting mouse moves nothing.
    keys: bool,
    /// Mouse moves seen (a hover without one is a menu opening under a resting mouse), and their
    /// count when the keyboard took the menus: a real move gives them back to the mouse.
    pointer_at: Option<(f32, f32)>,
    pointer_moves: u32,
    keys_moves: u32,
    /// Where the keyboard was before the bar took it.
    bar_return: Option<Option<ControlId>>,
    /// Alt is down with nothing else.
    alt_alone: bool,
    command: String,
    /// The context menu or the dropdown open.
    menu: Option<PopupId>,
    picked: Option<&'static str>,
}

#[cfg(test)]
impl State {
    pub(crate) fn open(&self) -> Option<usize> {
        self.open
    }

    pub(crate) fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    pub(crate) fn title(&self, index: usize) -> ControlId {
        self.titles[index].into()
    }

    pub(crate) fn bar_title(&self) -> Option<usize> {
        self.bar_title
    }

    pub(crate) fn keys(&self) -> bool {
        self.keys
    }
}

impl State {
    /// The mouse has the menus: the keyboard does not drive them, or the mouse moved since it did.
    fn mouse_takes_over(&self) -> bool {
        !self.keys || self.pointer_moves != self.keys_moves
    }

    /// The keyboard takes the menus.
    fn keys_on(&mut self) {
        (self.keys, self.keys_moves) = (true, self.pointer_moves);
    }
}

/// Counts real mouse moves (the page's and the menus'); it uses nothing.
fn note_pointer(app: &mut App, g: &Gesture) -> bool {
    let state = &mut app.menus;
    if g.kind == GestureKind::Pointer && state.pointer_at != Some((g.location.x, g.location.y)) {
        state.pointer_at = Some((g.location.x, g.location.y));
        state.pointer_moves = state.pointer_moves.wrapping_add(1);
    }
    false
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    scrolling(column().children((
        page_title("Tooltips & Menus").accessibility_role(Aria::PRESENTATION),
        SkiaLabel::new(
            "A desktop menu bar, a context menu (right-click anywhere on the page) and a dropdown, then tooltips: rest the mouse on a control, move along a row and the next one shows at once; a click, the wheel or a key closes it.",
        )
        .font_size(14)
        .text_color(hex(0xD3D3D3))
        .fill_x()
        .horizontal_text_alignment(TextAlignment::Center),
        card(
            "Menu bar",
            "Click a menu, or press Alt with its first letter (Alt+F, Alt+E, Alt+V, Alt+H). Alt alone puts the keyboard on the bar: Left and Right go round the titles, Down, Enter or Space open one. In a menu, Up and Down go round its lines, Left and Right open the menu before or after, Enter picks, Escape closes one level and Alt leaves. The shortcuts (Ctrl+N, Ctrl+S, F1...) work with the menus closed too.",
            (
                SkiaShape::new().fill_x().corner_radius(8).background_color(BUTTON).padding(4).children(
                    SkiaRow::new()
                        .spacing(2)
                        .accessibility_role(Aria::MENU_BAR)
                        .accessibility_label("Menu bar")
                        .children((0..MENUS.len()).map(|i| title(i, &mut app.menus.titles[i])).collect::<Vec<_>>()),
                ),
                SkiaLabel::new("")
                    .font_size(14)
                    .text_color(Color::WHITE)
                    .accessibility_role(Aria::STATUS)
                    .accessibility_live(Aria::LIVE_POLITE)
                    .observe(|me, app: &App| {
                        let text = if app.menus.command.is_empty() { "Last command: none" } else { app.menus.command.as_str() };
                        me.set_text(text.to_owned());
                    }),
            ),
        ),
        card(
            "Context menu and dropdown",
            "Right-click anywhere on the page: the menu opens where you clicked. The button opens a list below it. A click outside or Escape closes them.",
            (
                text_button("Choose a fruit").on_tapped(|me, app: &mut App, cx| {
                    if !primary(cx) {
                        return;
                    }
                    let fruits = ["Apple", "Banana", "Cherry", "Grape", "Mango", "Orange", "Peach", "Pear"];
                    open_menu_popup(app, cx, menu("Fruits", &fruits), me.id().into());
                }),
                SkiaLabel::new("")
                    .font_size(14)
                    .text_color(Color::WHITE)
                    .accessibility_role(Aria::STATUS)
                    .accessibility_live(Aria::LIVE_POLITE)
                    .observe(|me, app: &App| {
                        me.set_text(match app.menus.picked {
                            Some(item) => format!("Picked: {item}"),
                            None => "Nothing picked yet".to_owned(),
                        })
                    }),
            ),
        ),
        card(
            "Icon toolbar",
            "No Tooltip set: the text is each button's AccessibilityLabel (TooltipsFromAccessibility, switch below).",
            (
                SkiaRow::new().spacing(6).accessibility_role(Aria::TOOLBAR).accessibility_label("File tools").children((
                    icon_button(ICON_NEW, "New file"),
                    icon_button(ICON_OPEN, "Open"),
                    icon_button(ICON_SAVE, "Save"),
                    icon_button(ICON_SETTINGS, "Settings"),
                    icon_button(ICON_DELETE, "Delete"),
                )),
                SkiaRow::new().spacing(12).children((
                    // The Hello palette, not the library's default accent.
                    SkiaSwitch::new()
                        .is_toggled(true)
                        .accessibility_label("Tooltips from accessibility text")
                        .vertical_options(LayoutOptions::Center)
                        .color_frame_on(hex(0x0D6EFD))
                        .color_frame_off(BUTTON_HOVER)
                        .color_thumb_on(Color::WHITE)
                        .color_thumb_off(Color::WHITE)
                        .on_toggled(|_me, _app: &mut App, cx, on| cx.set_tooltips_from_accessibility(on)),
                    SkiaLabel::new("Tooltips from accessibility text")
                        .font_size(14)
                        .text_color(Color::WHITE)
                        .vertical_options(LayoutOptions::Center)
                        .accessibility_role(Aria::PRESENTATION),
                )),
            ),
        ),
        card(
            "Explicit tooltips",
            "Set with .tooltip(...). The last one is long: it wraps at the maximum width.",
            SkiaWrap::new().spacing(10).children((
                text_button("Publish").tooltip("Uploads the current version"),
                text_button("Preview").tooltip("Opens a live preview"),
                text_button("Share").tooltip("Copies a link to this project"),
                text_button("Long text").tooltip("This tooltip is long on purpose: it wraps at the maximum width of 320 points, so you can see how a tooltip with several lines looks."),
            )),
        ),
        card(
            "Near the window edge",
            "Rest on the icon at the right. On Windows the tooltip opens in a window of its own and can go past the edge of the app window; on phones, tablets and in the browser it stays inside the app.",
            SkiaLayer::new().fill_x().height_request(44).children(
                icon_button(ICON_INFO, "Information")
                    .horizontal_options(LayoutOptions::End)
                    .tooltip("This tooltip opens near the right edge of the window and is long enough to reach it"),
            ),
        ),
        card(
            "Scrolling closes it",
            "Rest on a row, then scroll this list with the wheel.",
            SkiaShape::new().fill_x().height_request(220).corner_radius(8).background_color(hex(0x212529)).is_clipped_to_bounds(true).children(
                SkiaScroll::new().fill().content(
                    SkiaStack::new().spacing(1).children(
                        (1..=30)
                            .map(|i| {
                                SkiaLayer::new().height_request(36).fill_x().background_color(CARD).tooltip(format!("Details of row {i}")).children(
                                    SkiaLabel::new(format!("Row {i}")).font_size(14).text_color(Color::WHITE).margin((12, 0)).vertical_options(LayoutOptions::Center),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
            ),
        ),
    )))
    // Real mouse moves over the page: the mouse takes the menus back from the keyboard.
    .consume_gestures(|_me, app: &mut App, _cx, g| note_pointer(app, g))
    // Right-click anywhere on the page: the Edit menu where it was.
    .on_context_menu(|_me, app: &mut App, cx, at| {
        let items = ["Cut", "Copy", "Paste", "Select all", "Settings…"];
        open_menu_popup(app, cx, menu("Edit", &items), at.location.into());
        true
    })
}

/// Opens the context menu or the dropdown, closing the one open: they share `menu`, so an open one
/// would otherwise be left on screen with nothing to close it (the Menu key opens the context menu
/// without a press that closes popups).
fn open_menu_popup(app: &mut App, cx: &mut Cx, content: Build<SkiaShape>, anchor: PopupAnchor) {
    if let Some(open) = app.menus.menu.take() {
        cx.close_popup_window(open);
    }
    app.menus.menu = Some(cx.open_popup_window(content, anchor, menu_window()));
}

/// The page shows icon buttons' accessibility labels as tooltips (the switch turns it off).
pub fn opened(_app: &mut App, cx: &mut Cx) {
    cx.set_tooltips_from_accessibility(true);
}

/// A popup closed, for any reason (a click outside, the window losing the keyboard): the menu bar
/// lets go.
pub fn popup_closed(app: &mut App, id: PopupId, cx: &mut Cx) {
    if app.menus.bar_popup == Some(id) {
        app.menus.bar_popup = None;
        leave_bar(app, cx);
    }
    if app.menus.menu == Some(id) {
        app.menus.menu = None;
    }
}

/// A menu bar title: a click opens its menu; while a menu is open, the mouse over another title
/// opens that one. Its first letter is underlined: Alt + it opens the menu. Open, or the keyboard
/// on it: the selected look edged in the accent; the mouse over it: the hover look.
fn title(index: usize, handle: &mut Handle<SkiaShape>) -> Build<SkiaShape> {
    let text = MENUS[index].title;
    SkiaShape::new()
        .corner_radius(6)
        .padding((12, 6))
        .stroke_width(1)
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_can_interact(Some(true))
        .accessibility_label(text)
        .observe(move |me, app: &App| {
            let state = &app.menus;
            let open = state.open == Some(index);
            let selected = open || state.bar_title == Some(index);
            let hover = !selected && !state.keys && state.hovered == Some(index);
            me.set_background_color(match () {
                _ if selected => SELECTED,
                _ if hover => BUTTON_HOVER,
                _ => Color::TRANSPARENT,
            });
            me.set_stroke_color(if selected { ACCENT } else { Color::TRANSPARENT });
            me.set_accessibility_is_expanded(Some(open));
        })
        .on_hovered(move |_me, app: &mut App, cx, on| {
            if on {
                app.menus.hovered = Some(index);
                // While the keyboard drives the bar, a mouse resting on a title moves nothing (a menu
                // opening or closing near it is no move); a real move takes over.
                if app.menus.mouse_takes_over() && app.menus.open.is_some_and(|open| open != index) {
                    app.menus.keys = false;
                    open_menu(app, cx, index, false);
                }
            } else if app.menus.hovered == Some(index) {
                app.menus.hovered = None;
            }
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            if !primary(cx) {
                return;
            }
            // Opened from the keyboard: its first line highlighted, as with Alt + its letter.
            let keyboard = std::mem::take(&mut app.menus.keyboard_open) || cx.gesture().is_none();
            app.menus.keys = keyboard;
            if app.menus.open == Some(index) {
                leave_bar(app, cx);
            } else {
                open_menu(app, cx, index, keyboard);
            }
        })
        // Enter or Space activate the title (the engine's keyboard, a screen reader's overlay):
        // the tap that follows comes from the keyboard.
        .on_key_down(|_me, app: &mut App, _cx, event| {
            app.menus.keyboard_open = matches!(event.key, "Enter" | "NumpadEnter" | "Space");
            false
        })
        .assign(handle)
        .children(
            SkiaLabel::new("")
                .font_size(14)
                .text_color(Color::WHITE)
                .accessibility_role(Aria::PRESENTATION)
                .spans((TextSpan::new(&text[..1]).underline(true), TextSpan::new(&text[1..]))),
        )
}

/// Opens menu `index` under its title; `keyboard`: its first line is lit and has the screen
/// readers' focus.
fn open_menu(app: &mut App, cx: &mut Cx, index: usize, keyboard: bool) {
    if let Some(popup) = app.menus.bar_popup.take() {
        cx.close_popup_window(popup);
    }
    let menu = &MENUS[index];
    let mut items = Vec::new();
    let content = SkiaShape::new()
        .corner_radius(8)
        .background_color(PANEL)
        .stroke_color(EDGE)
        .stroke_width(1.5)
        .width_request(240)
        .padding(4)
        .accessibility_role(Aria::MENU)
        .accessibility_label(menu.title)
        .consume_gestures(|_me, app: &mut App, _cx, g| note_pointer(app, g))
        .children(SkiaStack::new().spacing(0).children(
            menu.entries
                .iter()
                .enumerate()
                .map(|(entry, it)| match it.text {
                    None => Detached::from(SkiaShape::new().height_request(1).fill_x().background_color(EDGE).margin((8, 4))),
                    Some(_) => {
                        let mut handle = Handle::default();
                        let item = bar_item(index, entry).assign(&mut handle);
                        items.push(handle);
                        Detached::from(item)
                    }
                })
                .collect::<Vec<_>>(),
        ));
    app.menus.bar_popup = Some(cx.open_popup_window(content, app.menus.titles[index], menu_window()));
    (app.menus.open, app.menus.bar_title, app.menus.items) = (Some(index), None, items);
    (app.menus.highlighted, app.menus.keyboard_open) = (None, false);
    if keyboard {
        light(app, cx, Some(0));
    }
}

/// The keyboard lights line `line` of the open menu; screen readers follow it.
fn light(app: &mut App, cx: &mut Cx, line: Option<usize>) {
    app.menus.highlighted = line;
    if let Some(item) = line.and_then(|l| app.menus.items.get(l).copied()) {
        cx.set_accessibility_focus(item);
    }
}

/// The line the arrow `key` moves to from `from`, round past either end; Home and End the first
/// and the last (every line of these menus can act).
fn step(count: usize, from: Option<usize>, key: &str) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match (key, from) {
        ("Home", _) | ("ArrowDown", None) => 0,
        ("End", _) | ("ArrowUp", None) => last,
        ("ArrowDown", Some(f)) => if f >= last { 0 } else { f + 1 },
        (_, Some(f)) => if f == 0 { last } else { f - 1 },
        _ => 0,
    })
}

/// One line of a menu bar menu: its text, its shortcut at the right. Lit by the keyboard: the
/// selected look edged in the accent; under the mouse: the hover look.
fn bar_item(menu: usize, entry: usize) -> Build<SkiaShape> {
    let it = &MENUS[menu].entries[entry];
    let line = lines(menu).position(|(e, _)| e == entry).unwrap_or(0);
    SkiaShape::new()
        .corner_radius(6)
        .fill_x()
        .padding((12, 7))
        .stroke_width(1)
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_can_interact(Some(true))
        .accessibility_label(it.text.unwrap_or_default())
        .observe(move |me, app: &App| {
            let lit = app.menus.highlighted == Some(line);
            let selected = lit && app.menus.keys;
            me.set_background_color(match () {
                _ if selected => SELECTED,
                _ if lit => HIGHLIGHT,
                _ => Color::TRANSPARENT,
            });
            me.set_stroke_color(if selected { ACCENT } else { Color::TRANSPARENT });
        })
        .on_hovered(move |_me, app: &mut App, _cx, on| {
            // A menu opening under a resting mouse leaves the keyboard's line lit.
            if on && app.menus.mouse_takes_over() {
                app.menus.keys = false;
                app.menus.highlighted = Some(line);
            }
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            if primary(cx) {
                run(app, cx, menu, entry);
            }
        })
        .children((
            SkiaLabel::new(it.text.unwrap_or_default()).font_size(14).text_color(Color::WHITE).accessibility_role(Aria::PRESENTATION),
            SkiaLabel::new(it.shortcut.unwrap_or_default())
                .font_size(13)
                .text_color(MUTED)
                .horizontal_options(LayoutOptions::End)
                .accessibility_role(Aria::PRESENTATION),
        ))
}

/// Closes the open menu; the keyboard stays where it is.
fn close_menu(app: &mut App, cx: &mut Cx) {
    if let Some(popup) = app.menus.bar_popup.take() {
        cx.close_popup_window(popup);
    }
    (app.menus.open, app.menus.highlighted) = (None, None);
    app.menus.items.clear();
}

/// The keyboard comes to the menu bar: it takes the keys from a text field and remembers where they
/// were, to give them back.
fn enter_bar(app: &mut App, cx: &mut Cx) {
    if app.menus.bar_return.is_none() {
        app.menus.bar_return = Some(cx.focused());
        cx.focus(Some(app.menus.titles[0]));
    }
    app.menus.keys_on();
}

/// The menu bar lets go: the menu closed, the keyboard back where it was.
fn leave_bar(app: &mut App, cx: &mut Cx) {
    close_menu(app, cx);
    (app.menus.bar_title, app.menus.keys, app.menus.keyboard_open) = (None, false, false);
    if let Some(back) = app.menus.bar_return.take() {
        cx.focus(back);
    }
}

/// The keyboard on title `index` of the bar, no menu open; screen readers follow it.
fn light_title(app: &mut App, cx: &mut Cx, index: usize) {
    app.menus.bar_title = Some(index);
    cx.set_accessibility_focus(app.menus.titles[index]);
}

fn run(app: &mut App, cx: &mut Cx, menu: usize, entry: usize) {
    let (title, it) = (MENUS[menu].title, &MENUS[menu].entries[entry]);
    let text = it.text.unwrap_or_default();
    app.menus.command = match it.shortcut {
        Some(shortcut) => format!("Last command: {title} › {text} ({shortcut})"),
        None => format!("Last command: {title} › {text}"),
    };
    leave_bar(app, cx);
}

/// The menu bar's keys, before the page's others, as in Windows menus. Alt alone puts the keyboard
/// on the bar (or gives it back); Alt + a first letter opens that menu. On the bar: Left and Right
/// go round the titles, Down, Enter or Space open one. In a menu: Up, Down, Home and End move round
/// its lines, Left and Right open the menu before or after, Enter or Space run the line, Escape
/// closes the menu and leaves the keyboard on its title. The shortcuts work with the menus closed.
/// True = used.
pub fn key(app: &mut App, event: &KeyEvent, cx: &mut Cx) -> bool {
    let modifiers = event.modifiers();
    let alt = matches!(event.key, "AltLeft" | "AltRight");
    if event.kind == KeyKind::Up {
        // Alt let go with nothing pressed with it: onto the bar, or back from it.
        if !alt || !std::mem::take(&mut app.menus.alt_alone) {
            return false;
        }
        if app.menus.open.is_some() || app.menus.bar_title.is_some() {
            leave_bar(app, cx);
        } else {
            enter_bar(app, cx);
            light_title(app, cx, 0);
        }
        return true;
    }
    if event.kind != KeyKind::Down {
        return false;
    }
    // The canvas's keyboard navigation already pressed the control the keyboard is on (a title,
    // a line): nothing more to do with this Enter or Space.
    if event.handled && matches!(event.key, "Enter" | "NumpadEnter" | "Space") {
        return false;
    }
    // Escape: the canvas closed the open menu already; the keyboard stays on its title.
    if event.key == "Escape"
        && event.handled
        && let Some(open) = app.menus.open
    {
        enter_bar(app, cx);
        close_menu(app, cx);
        light_title(app, cx, open);
        return true;
    }
    app.menus.alt_alone = alt && !modifiers.ctrl && !modifiers.shift && !modifiers.meta;
    if modifiers.alt && !modifiers.ctrl {
        let Some(index) = MENUS.iter().position(|m| m.accelerator == event.key) else { return false };
        enter_bar(app, cx);
        open_menu(app, cx, index, true);
        return true;
    }
    if modifiers.ctrl || modifiers.meta {
        return run_shortcut(app, cx, event.key, modifiers.ctrl);
    }
    let n = MENUS.len();
    let open = app.menus.open.filter(|_| app.menus.bar_popup.is_some_and(|p| cx.is_popup_window_open(p)));
    let Some(open) = open else {
        let Some(t) = app.menus.bar_title else { return run_shortcut(app, cx, event.key, false) };
        match event.key {
            "ArrowLeft" => light_title(app, cx, (t + n - 1) % n),
            "ArrowRight" => light_title(app, cx, (t + 1) % n),
            "ArrowDown" | "Enter" | "NumpadEnter" | "Space" => open_menu(app, cx, t, true),
            "Escape" => leave_bar(app, cx),
            _ => return false,
        }
        return true;
    };
    if !matches!(event.key, "ArrowDown" | "ArrowUp" | "Home" | "End" | "ArrowLeft" | "ArrowRight" | "Enter" | "NumpadEnter" | "Space" | "Escape") {
        return run_shortcut(app, cx, event.key, false);
    }
    // A menu the mouse opened: the keyboard comes along.
    enter_bar(app, cx);
    match event.key {
        "ArrowRight" => open_menu(app, cx, (open + 1) % n, true),
        "ArrowLeft" => open_menu(app, cx, (open + n - 1) % n, true),
        "Enter" | "NumpadEnter" | "Space" => match app.menus.highlighted.and_then(|l| lines(open).nth(l)) {
            Some((entry, _)) => run(app, cx, open, entry),
            None => {}
        },
        "Escape" => {
            close_menu(app, cx);
            light_title(app, cx, open);
        }
        key => {
            let to = step(lines(open).count(), app.menus.highlighted, key);
            light(app, cx, to);
        }
    }
    true
}

/// A line's shortcut (Ctrl+N, F1...) runs it with the menus closed or open.
fn run_shortcut(app: &mut App, cx: &mut Cx, key: &str, ctrl: bool) -> bool {
    for (menu, it) in MENUS.iter().enumerate() {
        if let Some(entry) = it.entries.iter().position(|e| e.text.is_some() && !e.key.is_empty() && e.key == key && e.ctrl == ctrl) {
            run(app, cx, menu, entry);
            return true;
        }
    }
    false
}

/// A menu as in the popups example: a rounded panel of lines; picking one closes it.
fn menu(name: &'static str, items: &[&'static str]) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(PANEL)
        .stroke_color(EDGE)
        .stroke_width(1.5)
        .width_request(200)
        .padding(4)
        .accessibility_role(Aria::MENU)
        .accessibility_label(name)
        .children(
            SkiaStack::new().spacing(0).children(
                items
                    .iter()
                    .map(|&text| {
                        SkiaShape::new()
                            .corner_radius(6)
                            .fill_x()
                            .padding((12, 7))
                            .accessibility_role(Aria::MENU_ITEM)
                            .accessibility_can_interact(Some(true))
                            .accessibility_label(text)
                            .on_hovered(|me, _app: &mut App, _cx, on| me.set_background_color(if on { HIGHLIGHT } else { Color::TRANSPARENT }))
                            .on_tapped(move |_me, app: &mut App, cx| {
                                if !primary(cx) {
                                    return;
                                }
                                app.menus.picked = Some(text);
                                if let Some(menu) = app.menus.menu.take() {
                                    cx.close_popup_window(menu);
                                }
                            })
                            .children(SkiaLabel::new(text).font_size(15).text_color(Color::WHITE).accessibility_role(Aria::PRESENTATION))
                    })
                    .collect::<Vec<_>>(),
            ),
        )
}

/// A card of the page: a caption in the accent color, a note, then its content. Static: re-recorded
/// only when a control inside changes.
fn card(title: &'static str, note: &'static str, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .fill_x()
        .corner_radius(12)
        .background_color(CARD)
        .stroke_color(hex(0x373B3E))
        .stroke_width(1)
        .padding((20, 16))
        .use_cache(CacheType::Operations)
        .accessibility_role(Aria::GROUP)
        .accessibility_label(title)
        .children(SkiaStack::new().spacing(12).children((
            SkiaLabel::new(title)
                .font_size(12)
                .font_attributes(FontAttributes::Bold)
                .text_transform(TextTransform::Uppercase)
                .text_color(ACCENT)
                .accessibility_role(Aria::HEADING),
            SkiaLabel::new(note).font_size(13).text_color(MUTED).fill_x(),
            content,
        )))
}

/// An icon button without text: its tooltip is its accessibility label.
fn icon_button(svg: &'static str, label: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(40)
        .height_request(40)
        .corner_radius(8)
        .background_color(BUTTON)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .accessibility_can_interact(Some(true))
        .accessibility_label(label)
        .on_hovered(|me, _app: &mut App, _cx, on| me.set_background_color(if on { BUTTON_HOVER } else { BUTTON }))
        .on_tapped(|_me, _app: &mut App, _cx| {})
        .children(
            SkiaSvg::from_string(svg)
                .width_request(20)
                .height_request(20)
                .tint_color(Color::WHITE)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::Center),
        )
}

/// A text button in the page's colors.
fn text_button(text: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(BUTTON)
        .padding((16, 9))
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .accessibility_can_interact(Some(true))
        .accessibility_label(text)
        .on_hovered(|me, _app: &mut App, _cx, on| me.set_background_color(if on { BUTTON_HOVER } else { BUTTON }))
        .on_tapped(|_me, _app: &mut App, _cx| {})
        .children(SkiaLabel::new(text).font_size(14).text_color(Color::WHITE))
}
