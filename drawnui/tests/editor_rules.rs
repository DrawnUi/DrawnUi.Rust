//! SkiaEditor: typing, deleting clusters, the caret and the selection by keys and pointer,
//! clipboard keys, Enter, password, max length, placeholder, scrolling to the caret, auto height,
//! the blink (asleep between blinks, no allocation), the handlers, the looks. The rules are
//! DrawnUi.React's SkiaEditor.ts (and the C# SkiaEditor it ports); positions come from the label's
//! own layout.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::controls::control_style::PrebuiltControlStyle;
use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{Modifiers, PointerKind};

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
    log: Vec<String>,
}

type Host = Headless<App>;

const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, meta: false };
const SHIFT: Modifiers = Modifiers { shift: true, ctrl: false, alt: false, meta: false };
const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };
const CTRL_SHIFT: Modifiers = Modifiers { shift: true, ctrl: true, alt: false, meta: false };

/// An editor 300 points wide at the top left, with its handlers logging.
fn host(editor: Build<SkiaEditor>) -> Host {
    host_at(editor, 1.0)
}

fn host_at(editor: Build<SkiaEditor>, scale: f32) -> Host {
    let editor = editor
        .font_size(16)
        .on_text_changed(|_me: &mut Mut<SkiaEditor>, app: &mut App, _cx, text: &str| app.log.push(format!("text {text}")))
        .on_cursor_moved(|me: &mut Mut<SkiaEditor>, app: &mut App, _cx| app.log.push(format!("cursor {} {}", me.cursor_position(), me.selection_length())))
        .on_text_submitted(|_me: &mut Mut<SkiaEditor>, app: &mut App, _cx, text: &str| app.log.push(format!("submit {text}")));
    let build = |app: &mut App| SkiaLayout::new().fill().children(editor.width_request(300).assign(&mut app.editor));
    let ui = Ui::new(App::default(), build).font_bytes("FontText", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, (400.0 * scale) as i32, (300.0 * scale) as i32, scale);
    host.settle();
    host
}

fn editor(host: &Host) -> &SkiaEditor {
    host.ui.tree.find(host.ui.state.editor).unwrap()
}

fn text(host: &Host) -> String {
    editor(host).p.text.clone()
}

fn caret(host: &Host) -> (usize, usize) {
    (editor(host).cursor_position(), editor(host).selection_length())
}

/// The editor's text label.
fn label(host: &Host) -> &SkiaLabel {
    let id = host.ui.state.editor;
    let children = host.ui.tree.children(id);
    host.ui.tree.find(children[1]).unwrap()
}

fn frame(host: &mut Host) {
    host.frame_after(16.0);
}

fn press(host: &mut Host, x: f32, y: f32) {
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Down, x, y, now);
    frame(host);
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Up, x, y, now);
    frame(host);
}

/// Focuses the editor with a press at its start.
fn focus(host: &mut Host) {
    press(host, 14.0, 12.0);
    assert!(editor(host).is_focused());
    host.ui.state.log.clear();
}

fn typed(host: &mut Host, text: &str) {
    host.ui.key(KeyKind::Char, "", text, NONE, false);
    frame(host);
}

fn key(host: &mut Host, name: &'static str, modifiers: Modifiers) {
    host.ui.key(KeyKind::Down, name, "", modifiers, false);
    host.ui.key(KeyKind::Up, name, "", modifiers, false);
    frame(host);
}

/// Pixel x of the caret before character `index`.
fn x_of(host: &Host, index: usize) -> f32 {
    label(host).text_lines().find(|l| index >= l.start && index <= l.end()).map_or(0.0, |l| l.x_of(index))
}

#[test]
fn typing_inserts_at_the_caret_and_the_handlers_hear_it() {
    let mut host = host(SkiaEditor::new());
    focus(&mut host);
    typed(&mut host, "Hello");
    assert_eq!(text(&host), "Hello");
    assert_eq!(host.ui.state.log, ["text Hello", "cursor 5 0"]);
    key(&mut host, "ArrowLeft", NONE);
    key(&mut host, "ArrowLeft", NONE);
    typed(&mut host, "X");
    assert_eq!((text(&host).as_str(), caret(&host)), ("HelXlo", (4, 0)));
    // Changes within one frame are reported once, with the text of that frame.
    host.ui.state.log.clear();
    host.ui.key(KeyKind::Char, "", "a", NONE, false);
    host.ui.key(KeyKind::Char, "", "b", NONE, false);
    frame(&mut host);
    assert_eq!(host.ui.state.log, ["text HelXablo", "cursor 6 0"]);
}

#[test]
fn backspace_and_delete_take_whole_clusters() {
    let mut host = host(SkiaEditor::new());
    focus(&mut host);
    // e + combining acute, a flag (two regional indicators), a family (ZWJ sequence), an emoji
    // with a skin tone.
    let clusters = ["e\u{301}", "\u{1F1EB}\u{1F1F7}", "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", "\u{1F44D}\u{1F3FD}"];
    for cluster in clusters {
        typed(&mut host, &format!("a{cluster}b"));
        key(&mut host, "ArrowLeft", NONE);
        key(&mut host, "Backspace", NONE);
        assert_eq!(text(&host), "ab", "{cluster:?}");
        typed(&mut host, cluster);
        // The arrows step over a cluster at once.
        key(&mut host, "ArrowLeft", NONE);
        key(&mut host, "Delete", NONE);
        assert_eq!(text(&host), "ab", "{cluster:?}");
        key(&mut host, "KeyA", CTRL);
        key(&mut host, "Delete", NONE);
        assert_eq!(text(&host), "");
    }
    // A line break is a cluster of its own.
    let mut host = self::host(SkiaEditor::new().max_lines(3));
    focus(&mut host);
    typed(&mut host, "a\nb");
    key(&mut host, "ArrowLeft", NONE);
    key(&mut host, "Backspace", NONE);
    assert_eq!(text(&host), "ab");
}

#[test]
fn shift_arrows_select_and_arrows_collapse() {
    let mut host = host(SkiaEditor::new().text("hello brave world"));
    focus(&mut host);
    key(&mut host, "Home", NONE);
    for _ in 0..5 {
        key(&mut host, "ArrowRight", SHIFT);
    }
    assert_eq!(caret(&host), (0, 5));
    assert_eq!(editor(&host).selected_text(), "hello");
    // Without Shift the selection collapses to the side of the move.
    key(&mut host, "ArrowRight", NONE);
    assert_eq!(caret(&host), (5, 0));
    // The moving edge crosses the edge that stays.
    key(&mut host, "ArrowRight", SHIFT);
    key(&mut host, "ArrowLeft", SHIFT);
    key(&mut host, "ArrowLeft", SHIFT);
    assert_eq!(caret(&host), (4, 1));
    key(&mut host, "ArrowLeft", NONE);
    assert_eq!(caret(&host), (4, 0));
    key(&mut host, "End", SHIFT);
    assert_eq!(caret(&host), (4, 13));
    key(&mut host, "Home", NONE);
    // Ctrl jumps words: to the end of the next one, to the start of the previous one.
    key(&mut host, "ArrowRight", CTRL);
    assert_eq!(caret(&host), (5, 0));
    key(&mut host, "ArrowRight", CTRL);
    assert_eq!(caret(&host), (11, 0));
    key(&mut host, "ArrowLeft", CTRL_SHIFT);
    assert_eq!((caret(&host), editor(&host).selected_text()), ((6, 5), "brave"));
    key(&mut host, "KeyA", CTRL);
    assert_eq!(caret(&host), (0, 17));
}

#[test]
fn cut_and_paste_keys() {
    let mut host = host(SkiaEditor::new().text("copy me"));
    focus(&mut host);
    key(&mut host, "KeyA", CTRL);
    key(&mut host, "KeyC", CTRL);
    assert_eq!(text(&host), "copy me");
    key(&mut host, "KeyX", CTRL);
    assert_eq!(text(&host), "");
    // The host answers Ctrl+V with typed text.
    key(&mut host, "KeyV", CTRL);
    typed(&mut host, "copy me");
    assert_eq!(text(&host), "copy me");
    // A pasted line break is a space on a single line.
    typed(&mut host, "\r\nnext");
    assert_eq!(text(&host), "copy me next");
}

#[test]
fn enter_submits_a_single_line_and_breaks_a_multiline_one() {
    let mut host = host(SkiaEditor::new().text("one"));
    focus(&mut host);
    key(&mut host, "Enter", NONE);
    assert_eq!(host.ui.state.log, ["submit one"]);
    assert!(editor(&host).is_focused(), "Enter keeps the focus (React StubPressEnter)");

    let mut host = self::host(SkiaEditor::new().max_lines(3).text("one"));
    focus(&mut host);
    key(&mut host, "End", NONE);
    key(&mut host, "Enter", NONE);
    typed(&mut host, "two");
    assert_eq!(text(&host), "one\ntwo");
    assert!(!host.ui.state.log.iter().any(|l| l.starts_with("submit")));

    // Send: Enter submits, Shift+Enter breaks the line.
    let mut host = self::host(SkiaEditor::new().max_lines(3).return_type(ReturnType::Send).text("chat"));
    focus(&mut host);
    key(&mut host, "End", NONE);
    key(&mut host, "Enter", SHIFT);
    key(&mut host, "Enter", NONE);
    assert_eq!(text(&host), "chat\n");
    assert_eq!(host.ui.state.log.last().unwrap(), "submit chat\n");
}

#[test]
fn password_max_length_and_placeholder() {
    let mut host = host(SkiaEditor::new().is_password(true).max_length(5).placeholder_text("Password"));
    let placeholder = host.ui.tree.children(host.ui.state.editor)[0];
    assert!(host.ui.tree.base(placeholder).unwrap().p.is_visible);
    focus(&mut host);
    typed(&mut host, "secret!");
    assert_eq!(text(&host), "secre");
    assert_eq!(label(&host).lines().next().unwrap().0, "\u{2022}".repeat(5));
    assert!(!host.ui.tree.base(placeholder).unwrap().p.is_visible);
    // A full editor takes no more; a selection makes room.
    typed(&mut host, "x");
    assert_eq!(text(&host), "secre");
    key(&mut host, "ArrowLeft", SHIFT);
    typed(&mut host, "xyz");
    assert_eq!(text(&host), "secrx");
}

#[test]
fn a_press_places_the_caret_a_drag_selects_a_double_press_selects_the_word() {
    let mut host = host(SkiaEditor::new().text("alpha beta gamma"));
    focus(&mut host);
    let y = 12.0;
    host.frame_after(600.0);
    let x = x_of(&host, 7) + 1.0;
    press(&mut host, x, y);
    assert_eq!(caret(&host), (7, 0));
    // Nearer the end of a glyph: after it.
    host.frame_after(600.0);
    let (a, b) = (x_of(&host, 7), x_of(&host, 8));
    press(&mut host, a + (b - a) * 0.8, y);
    assert_eq!(caret(&host), (8, 0));

    // Drag from 1 to 9.
    host.frame_after(600.0);
    let (from, to) = (x_of(&host, 1) + 1.0, x_of(&host, 9) + 1.0);
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Down, from, y, now);
    frame(&mut host);
    for i in 1..=4 {
        let now = host.time_ms();
        host.ui.pointer(PointerKind::Move, from + (to - from) * i as f32 / 4.0, y, now);
        frame(&mut host);
    }
    let now = host.time_ms();
    host.ui.pointer(PointerKind::Up, to, y, now);
    frame(&mut host);
    assert_eq!(caret(&host), (1, 8));

    // Two presses within 500 ms on "gamma".
    host.frame_after(600.0);
    let x = x_of(&host, 13);
    press(&mut host, x, y);
    press(&mut host, x, y);
    assert_eq!(editor(&host).selected_text(), "gamma");

    // Shift + press extends from the far edge.
    host.frame_after(600.0);
    let x = x_of(&host, 2) + 1.0;
    press(&mut host, x, y);
    host.ui.key(KeyKind::Down, "ShiftLeft", "", SHIFT, false);
    host.frame_after(600.0);
    let x = x_of(&host, 5) + 1.0;
    press(&mut host, x, y);
    assert_eq!(caret(&host), (2, 3));
}

#[test]
fn a_long_single_line_scrolls_to_the_caret() {
    let long = "The quick brown fox jumps over the lazy dog, twice over and once more";
    let mut host = host(SkiaEditor::new().text(long));
    focus(&mut host);
    let id = host.ui.state.editor;
    let offset = |host: &Host| host.ui.tree.base(id).unwrap().content_offset.x;
    let inner = 300.0 - 24.0;
    key(&mut host, "End", NONE);
    let label_width = x_of(&host, long.chars().count()) - x_of(&host, 0);
    assert!((offset(&host) + (label_width + 2.0 - inner)).abs() < 1.5, "{} {label_width}", offset(&host));
    key(&mut host, "Home", NONE);
    assert_eq!(offset(&host), 0.0);
    // Typing at the end keeps the caret in view once the text is laid out again.
    key(&mut host, "End", NONE);
    typed(&mut host, " end");
    frame(&mut host);
    let label_width = x_of(&host, long.chars().count() + 4) - x_of(&host, 0);
    assert!((offset(&host) + (label_width + 2.0 - inner)).abs() < 1.5, "{} {label_width}", offset(&host));
}

#[test]
fn multiline_wraps_moves_by_lines_and_grows_with_auto_height() {
    let mut host = host(SkiaEditor::new().max_lines(-1).auto_height(true).text("one"));
    let id = host.ui.state.editor;
    let height = |host: &Host| host.rect(id).height();
    let one = height(&host);
    focus(&mut host);
    key(&mut host, "End", NONE);
    typed(&mut host, "\ntwo\nthree");
    frame(&mut host);
    let line = label(&host).measured_line_height();
    assert!((height(&host) - one - 2.0 * line).abs() <= 1.0, "{} {one} {line}", height(&host));
    // Up and Down keep the x: from the end of "three" up into "two", then "one".
    key(&mut host, "ArrowUp", NONE);
    assert_eq!(caret(&host), (7, 0));
    key(&mut host, "ArrowUp", NONE);
    assert_eq!(caret(&host), (3, 0));
    // Down from the end of "one": the character of "two" nearest to that x.
    let x0 = x_of(&host, 3);
    key(&mut host, "ArrowDown", NONE);
    let k = caret(&host).0;
    let nearest = (4..=7).min_by(|a, b| (x_of(&host, *a) - x0).abs().total_cmp(&(x_of(&host, *b) - x0).abs())).unwrap();
    assert_eq!(k, nearest);
    // Words wrap at the width; every space stays.
    let words = "word word word word word word word word word word word";
    let mut host = self::host(SkiaEditor::new().max_lines(4).text(words));
    frame(&mut host);
    let lines: Vec<String> = label(&host).lines().map(|l| l.0.to_owned()).collect();
    assert!(lines.len() > 1 && lines[0].ends_with(' '), "{lines:?}");
    assert_eq!(lines.concat(), words);
    let (line, height) = (label(&host).measured_line_height(), host.rect(host.ui.state.editor).height());
    assert!((height - 4.0 * line - 16.0).abs() <= 1.0, "{height} {line}");
}

#[test]
fn the_app_sets_the_text_and_hears_it() {
    let mut host = host(SkiaEditor::new());
    focus(&mut host);
    typed(&mut host, "typed");
    host.ui.state.log.clear();
    let id = host.ui.state.editor;
    host.ui.tree.find_mut::<SkiaEditor>(id).unwrap().set_text("two\nlines");
    frame(&mut host);
    frame(&mut host);
    assert_eq!(text(&host), "two lines", "a single line has no line breaks");
    assert_eq!(caret(&host), (5, 0), "the caret stays where it was");
    assert_eq!(host.ui.state.log, ["text two lines"]);
    host.ui.tree.find_mut::<SkiaEditor>(id).unwrap().select_all();
    frame(&mut host);
    host.ui.tree.find_mut::<SkiaEditor>(id).unwrap().insert_at_cursor("new");
    frame(&mut host);
    frame(&mut host);
    assert_eq!((text(&host).as_str(), caret(&host)), ("new", (3, 0)));
    // Escape gives the focus up and drops the selection.
    key(&mut host, "KeyA", CTRL);
    key(&mut host, "Escape", NONE);
    assert!(!editor(&host).is_focused());
    assert_eq!(caret(&host), (0, 0));
}

/// Pixels of a color inside the editor.
fn pixels_of(host: &mut Host, color: Color) -> usize {
    let mut n = 0;
    for y in 0..40 {
        for x in 0..300 {
            if host.pixel(x, y) == color {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn the_caret_blinks_asleep_between_blinks_without_allocating() {
    let crimson = Color::from_rgb(0xDC, 0x14, 0x3C);
    let mut host = host(SkiaEditor::new().text("blink"));
    focus(&mut host);
    assert!(pixels_of(&mut host, crimson) > 10, "the caret shows on focus");
    frame(&mut host);
    // No frames until the blink.
    assert!(!host.ui.needs_frame());
    // One blink first: buffers reach their size.
    host.frame_after(host.ui.wake_at().unwrap() - host.time_ms());
    host.frame_after(host.ui.wake_at().unwrap() - host.time_ms());
    // Allocations in the blink frames themselves (reading pixels back is the test's).
    let mut allocated = 0;
    let mut shown = Vec::new();
    for _ in 0..4 {
        let wake = host.ui.wake_at().expect("a blink is due");
        assert!(wake - host.time_ms() <= 500.0 + 1e-6);
        let before = ALLOCATIONS.with(Cell::get);
        host.frame_after(wake - host.time_ms());
        allocated += ALLOCATIONS.with(Cell::get) - before;
        shown.push(pixels_of(&mut host, crimson) > 10);
        assert!(!host.ui.needs_frame());
    }
    assert_eq!(allocated, 0, "allocations in 4 blink frames");
    assert_eq!(shown, [false, true, false, true]);
    // Typing shows it at once.
    host.frame_after(host.ui.wake_at().unwrap() - host.time_ms());
    typed(&mut host, "!");
    assert!(pixels_of(&mut host, crimson) > 10);
    // Unfocused: no caret, no blink.
    key(&mut host, "Escape", NONE);
    frame(&mut host);
    assert_eq!(pixels_of(&mut host, crimson), 0);
    assert_eq!(host.ui.wake_at(), None);
}

#[test]
fn a_selection_is_painted_under_the_text() {
    let green = Color::from_rgb(0, 200, 0);
    let mut host = host(SkiaEditor::new().text("select").selection_color(green));
    focus(&mut host);
    key(&mut host, "KeyA", CTRL);
    let (a, b) = (x_of(&host, 0) as i32, x_of(&host, 6) as i32);
    let row: Vec<bool> = (a + 1..b - 1).map(|x| host.pixel(x, 9) == green).collect();
    assert!(row.iter().filter(|g| **g).count() > row.len() / 2, "{row:?}");
    assert_ne!(host.pixel(b + 3, 9), green);
}

#[test]
fn the_looks_fill_the_frame_and_the_app_wins() {
    let draw = |editor: Build<SkiaEditor>| {
        let mut host = host(editor);
        (host.pixel(150, 3), host.pixel(150, 0))
    };
    assert_eq!(draw(SkiaEditor::new()).0, Color::from_rgb(0xF2, 0xF3, 0xF5));
    assert_eq!(draw(SkiaEditor::new().control_style(PrebuiltControlStyle::Material3)).0, Color::from_rgb(0xE6, 0xE0, 0xE9));
    let (cupertino, border) = draw(SkiaEditor::new().control_style(PrebuiltControlStyle::Cupertino));
    assert_eq!(cupertino, Color::WHITE);
    assert!(border.r() < 235 && border.r() > 120, "{border:?}");
    let custom = SkiaEditor::new().control_style(PrebuiltControlStyle::Cupertino).background_color(Color::from_rgb(10, 20, 30));
    assert_eq!(draw(custom).0, Color::from_rgb(10, 20, 30));
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn typing_cost_on_2_kb() {
    use std::time::Instant;
    let words = "lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor ";
    let text: String = words.repeat(2048 / words.len() + 1)[..2048].to_owned();
    for (name, editor) in [("single line", SkiaEditor::new()), ("multiline", SkiaEditor::new().max_lines(12))] {
        let mut host = host(editor.text(text.clone()));
        focus(&mut host);
        key(&mut host, "Home", NONE);
        for _ in 0..500 {
            key(&mut host, "ArrowRight", NONE);
        }
        const KEYS: u32 = 200;
        let start = Instant::now();
        for i in 0..KEYS {
            if i % 2 == 0 {
                typed(&mut host, "x")
            } else {
                key(&mut host, "Backspace", NONE)
            }
        }
        let typing = start.elapsed().as_secs_f64() * 1e6 / KEYS as f64;
        let start = Instant::now();
        for i in 0..KEYS {
            key(&mut host, if i % 2 == 0 { "ArrowRight" } else { "ArrowLeft" }, NONE);
        }
        let moving = start.elapsed().as_secs_f64() * 1e6 / KEYS as f64;
        for _ in 0..10 {
            if host.ui.wake_at().is_some() {
                break;
            }
            frame(&mut host);
        }
        let wake = host.ui.wake_at().expect("a blink is due");
        let start = Instant::now();
        host.frame_after(wake - host.time_ms());
        let blink = start.elapsed().as_secs_f64() * 1e6;
        println!("{name}, 2048 chars: {typing:.0} us per typed key + frame, {moving:.0} us per arrow + frame, {blink:.0} us per blink frame");
    }
}

#[test]
fn at_scale_2_presses_and_scrolling_land_on_the_same_characters() {
    let long = "The quick brown fox jumps over the lazy dog, twice over and once more";
    let mut host = host_at(SkiaEditor::new().text(long), 2.0);
    press(&mut host, 28.0, 24.0);
    assert!(editor(&host).is_focused());
    host.frame_after(600.0);
    let x = x_of(&host, 10) + 2.0;
    press(&mut host, x, 24.0);
    assert_eq!(caret(&host), (10, 0));
    key(&mut host, "End", NONE);
    let id = host.ui.state.editor;
    let offset = host.ui.tree.base(id).unwrap().content_offset.x;
    let width = x_of(&host, long.chars().count()) - x_of(&host, 0);
    assert!((offset + (width + 4.0 - (600.0 - 48.0))).abs() < 2.0, "{offset} {width}");
}
