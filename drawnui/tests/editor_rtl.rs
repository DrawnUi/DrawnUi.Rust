//! The editor caret in right-to-left text (drawnui-cross RTL caret, the rules DrawnUi.React
//! follows too): the caret before character i is drawn at the trailing edge of character i-1 (at a
//! line's start at the leading edge of the first character); a tap goes to the grapheme boundary
//! whose caret is nearest; Left / Right move on screen; typed letters go in at the caret and the
//! caret stays after them in reading order; a selection covers the boxes of its characters.
//! Lines: pure Arabic, pure Hebrew, and "Hi مرحبا 123" (left to right, its digits with the Arabic).

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));
const ARABIC: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoSansArabic-Subset.ttf"));
const HEBREW: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/NotoSansHebrew-Subset.ttf"));
const EMOJI: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/NotoColorEmoji-Subset.ttf"));

const ARABIC_LINE: &str = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"; // مرحبا
const HEBREW_LINE: &str = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"; // שלום
const MIXED_LINE: &str = "Hi \u{0645}\u{0631}\u{062D}\u{0628}\u{0627} 123"; // Hi مرحبا 123

#[derive(Default)]
struct App {
    editor: Handle<SkiaEditor>,
}

type Host = Headless<App>;

fn scene(text: &'static str) -> Host {
    let build = move |app: &mut App| {
        SkiaLayout::new().fill().children(
            SkiaEditor::new().width_request(400).font_size(20).text(text).font_family_fallback("FontArabic,FontHebrew,FontEmoji").assign(&mut app.editor),
        )
    };
    let ui = Ui::new(App::default(), build)
        .font_bytes("FontText", FONT)
        .font_bytes("FontArabic", ARABIC)
        .font_bytes("FontHebrew", HEBREW)
        .font_bytes("FontEmoji", EMOJI)
        .background(Color::BLACK);
    let mut host = Headless::new(ui, 500, 200, 1.0);
    host.settle();
    host
}

fn editor(host: &Host) -> &SkiaEditor {
    host.ui.tree.find(host.ui.state.editor).unwrap()
}

/// The editor's text label.
fn label(host: &Host) -> &SkiaLabel {
    let children = host.ui.tree.children(host.ui.state.editor);
    host.ui.tree.find(children[1]).unwrap()
}

/// The caret x of every index of the (single) line.
fn carets(host: &Host) -> Vec<f32> {
    let line = label(host).text_lines().next().expect("a line");
    (line.start..=line.end()).map(|i| line.x_of(i)).collect()
}

fn line_rect(host: &Host) -> Rect {
    label(host).text_lines().next().unwrap().rect
}

fn focus(host: &mut Host) {
    let id = host.ui.state.editor.id();
    host.ui.focus(Some(id));
    host.frame_after(16.0);
}

/// A tap at caret x `x` (the line's coordinates are the screen's here) on the line, without
/// settling (the caret blinks); far enough from the last tap not to be a double tap.
fn tap_at(host: &mut Host, x: f32) {
    let y = line_rect(host).center_y();
    host.frame_after(600.0);
    host.ui.pointer(PointerKind::Down, x, y, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
}

fn key(host: &mut Host, name: &str) {
    host.press_key(name);
    host.frame_after(16.0);
}

/// Pure right-to-left lines: the caret starts at the right end (before the first letter) and moves
/// left with every letter; the end of the text is the line's left end.
#[test]
fn a_right_to_left_line_runs_its_caret_from_right_to_left() {
    for text in [ARABIC_LINE, HEBREW_LINE] {
        let host = scene(text);
        let line = line_rect(&host);
        let x = carets(&host);
        assert_eq!(x.len(), text.chars().count() + 1, "{text}");
        assert!((x[0] - line.right).abs() < 0.5, "{text}: index 0 at {} not at the right end {}", x[0], line.right);
        assert!((x[x.len() - 1] - line.left).abs() < 0.5, "{text}: the end at {} not at the left end {}", x[x.len() - 1], line.left);
        for k in 1..x.len() {
            assert!(x[k] <= x[k - 1] + 0.01, "{text}: index {k} at {} is right of index {} at {}", x[k], k - 1, x[k - 1]);
        }
    }
}

/// "Hi مرحبا 123" in a left to right paragraph shows as "Hi 123 مرحبا" (the digits stay with the
/// Arabic): the caret after "Hi " is right of "Hi ", the Arabic letters move it left from the
/// right end, and the end of the text is the right edge of "3", left of the Arabic.
#[test]
fn a_mixed_line_places_the_caret_at_the_trailing_edges() {
    let host = scene(MIXED_LINE);
    let line = line_rect(&host);
    let x = carets(&host);
    assert_eq!(x.len(), 13);
    assert!((x[0] - line.left).abs() < 0.5, "index 0 at {} not at the left end {}", x[0], line.left);
    assert!(x[1] > x[0] && x[2] > x[1] && x[3] > x[2], "\"Hi \" runs left to right: {:?}", &x[..4]);
    // After م (index 4): the left edge of م, the rightmost letter; the Arabic carets fall to the left.
    assert!(x[4] > x[3] && x[4] < line.right - 0.5, "after م at {}, line right {}", x[4], line.right);
    for k in 5..=8 {
        assert!(x[k] < x[k - 1], "index {k} at {} not left of {}", x[k], x[k - 1]);
    }
    // The digits are left of the Arabic and run left to right; the end is right of "3".
    assert!(x[11] > x[10] && x[12] > x[11], "the digits run left to right: {:?}", &x[9..]);
    assert!(x[12] < x[8], "the end of the text {} is left of the Arabic's end {}", x[12], x[8]);
    assert!(x[3] < x[10], "\"Hi \" is left of the digits");
}

/// A tap goes to the grapheme boundary whose caret is nearest: on every caret x the tap lands on an
/// index with that same x, and on the middle of a right-to-left letter's right half it gives that
/// letter's index.
#[test]
fn a_tap_finds_the_index_on_screen() {
    for text in [ARABIC_LINE, HEBREW_LINE, MIXED_LINE] {
        let mut host = scene(text);
        focus(&mut host);
        let x = carets(&host);
        for (k, &cx) in x.iter().enumerate() {
            tap_at(&mut host, cx);
            let got = editor(&host).cursor_position();
            assert!((x[got] - cx).abs() < 0.5, "{text}: a tap on index {k} at {cx} gave {got} at {}", x[got]);
        }
    }
    // The right half of the first Arabic letter (rightmost on screen) is its index 0.
    let mut host = scene(ARABIC_LINE);
    focus(&mut host);
    let x = carets(&host);
    tap_at(&mut host, x[0] - (x[0] - x[1]) * 0.25);
    assert_eq!(editor(&host).cursor_position(), 0);
    tap_at(&mut host, x[0] - (x[0] - x[1]) * 0.75);
    assert_eq!(editor(&host).cursor_position(), 1);
}

/// Typing at the end of Arabic text: the letter goes after the others in reading order, and the
/// caret stays after it, at the line's left end.
#[test]
fn typing_arabic_keeps_the_caret_after_the_new_letter() {
    let mut host = scene(ARABIC_LINE);
    focus(&mut host);
    // Focused from code, the caret is at the end of the text (C# 2285b422).
    assert_eq!(editor(&host).cursor_position(), 5);
    host.type_text("\u{0628}");
    assert_eq!(editor(&host).p.text, format!("{ARABIC_LINE}\u{0628}"));
    assert_eq!(editor(&host).cursor_position(), 6);
    let (line, x) = (line_rect(&host), carets(&host));
    assert!((x[6] - line.left).abs() < 0.5, "the caret {} not at the line's left end {}", x[6], line.left);
}

/// Left and Right move on screen: in Arabic, Left goes forward in the text (to the left on screen)
/// and Right back; in the mixed line every step goes to the nearest caret on that side.
#[test]
fn arrows_move_on_screen() {
    let mut host = scene(ARABIC_LINE);
    focus(&mut host);
    key(&mut host, "Home");
    assert_eq!(editor(&host).cursor_position(), 0);
    key(&mut host, "ArrowLeft");
    assert_eq!(editor(&host).cursor_position(), 1, "Left in Arabic goes on in the text");
    key(&mut host, "ArrowRight");
    assert_eq!(editor(&host).cursor_position(), 0);

    let mut host = scene(MIXED_LINE);
    focus(&mut host);
    key(&mut host, "Home");
    let x = carets(&host);
    let mut at = x[0];
    for step in 0..12 {
        let before = editor(&host).cursor_position();
        key(&mut host, "ArrowRight");
        let now = editor(&host).cursor_position();
        if now == before {
            break;
        }
        let nx = carets(&host)[now];
        if nx < at - 0.01 {
            // No caret further right on the line: the edge step goes one grapheme on in reading order.
            let rightmost = x.iter().copied().fold(f32::MIN, f32::max);
            assert!((at - rightmost).abs() < 0.01, "step {step}: Right went left from {at}, not the rightmost caret {rightmost}");
            assert_eq!(now, before + 1, "the edge step goes forward in a left to right paragraph");
            break;
        }
        at = nx;
    }
}

/// A selection across the direction change of the mixed line covers each character's box: the
/// space after "Hi" and the first Arabic letters are two separate boxes on the line.
#[test]
fn a_selection_covers_the_characters_on_screen() {
    let host = scene(MIXED_LINE);
    let line = label(&host).text_lines().next().unwrap();
    let boxes = line.boxes(2, 5); // " مر"
    assert_eq!(boxes.len(), 2, "{boxes:?}");
    assert!(boxes[0].1 <= boxes[1].0, "{boxes:?}");
    let all = line.boxes(0, 12);
    assert_eq!(all.len(), 1, "the whole line is one box: {all:?}");
}

/// A tap never stops inside a grapheme: 🙎🏽 (person + skin tone) is one stop.
#[test]
fn a_tap_never_lands_inside_an_emoji_sequence() {
    let mut host = scene("a\u{1F64E}\u{1F3FD}b");
    focus(&mut host);
    let line = line_rect(&host);
    let mut x = line.left;
    while x <= line.right + 4.0 {
        tap_at(&mut host, x);
        assert_ne!(editor(&host).cursor_position(), 2, "a tap at {x} landed between the person and the skin tone");
        x += 2.0;
    }
}

/// Left or Right without Shift collapses a selection to its edge on that side of the screen: in
/// Arabic the end of the text is on the left.
#[test]
fn an_arrow_collapses_a_selection_to_its_edge_on_screen() {
    let select_all = |host: &mut Host| {
        host.ui.key(KeyKind::Down, "KeyA", "", Modifiers { ctrl: true, ..Modifiers::default() }, false);
        host.frame_after(16.0);
    };
    let mut host = scene(ARABIC_LINE);
    focus(&mut host);
    select_all(&mut host);
    assert_eq!((editor(&host).cursor_position(), editor(&host).selection_length()), (0, 5));
    key(&mut host, "ArrowLeft");
    assert_eq!((editor(&host).cursor_position(), editor(&host).selection_length()), (5, 0), "Left goes to the left edge: the end");
    select_all(&mut host);
    key(&mut host, "ArrowRight");
    assert_eq!((editor(&host).cursor_position(), editor(&host).selection_length()), (0, 0), "Right goes to the right edge: the start");
}
