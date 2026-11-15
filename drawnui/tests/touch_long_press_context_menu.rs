//! A long press on touch raises the context menu once, after the LongPressing (drawnui-cross 6cb):
//! over a selection the selection stays, on plain selectable text the word is selected first, on
//! any other control the menu comes alone. A menu the app takes hides the drawn Copy button; one
//! it leaves keeps it. The browser's own long press menu (Android Chrome) makes no second one.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{ContextMenuSource, LONG_PRESS_MS, PointerKind};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const TEXT: &str = "Hello wonderful world";

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
    shape: Handle<SkiaShape>,
    /// What was selected in the label at each menu, and its source.
    menus: Vec<(String, ContextMenuSource)>,
    take: bool,
}

fn host(take: bool) -> Headless<App> {
    let ui = Ui::new(App { take, ..App::default() }, |app: &mut App| {
        SkiaLayout::column().fill().children((
            SkiaLabel::new(TEXT).font_size(20).margin(20).accessibility_text_selectable(true).assign(&mut app.label),
            SkiaShape::new().width_request(100).height_request(100).margin(20).background_color(Color::RED).assign(&mut app.shape),
        ))
    })
    .font_bytes("Default", FONT)
    .background(Color::BLACK)
    .on_context_menu(|app: &mut App, menu, cx| {
        let selected = cx.find::<SkiaLabel>(app.label).map(|l| l.selected_text()).unwrap_or_default();
        app.menus.push((selected, menu.source));
        app.take
    });
    let mut host = Headless::new(ui, 420, 300, 1.0);
    host.use_touch(true);
    host.settle();
    host
}

/// Canvas pixel inside character `index` of the label's first line.
fn at(host: &Headless<App>, index: usize) -> (f32, f32) {
    let label = host.ui.tree.find::<SkiaLabel>(host.ui.state.label).unwrap();
    let line = label.text_lines().next().unwrap();
    (line.rect.left + line.advances[..index].iter().sum::<f32>() + 2.0, line.rect.center_y())
}

fn selected(host: &Headless<App>) -> String {
    host.ui.tree.find::<SkiaLabel>(host.ui.state.label).unwrap().selected_text()
}

/// Down, held for the long press, still held (the menu comes while the finger is down).
fn hold(host: &mut Headless<App>, (x, y): (f32, f32)) {
    host.ui.pointer_touch(PointerKind::Down, x, y, host.time_ms());
    host.frame();
    host.frame_after(LONG_PRESS_MS);
}

fn release(host: &mut Headless<App>, (x, y): (f32, f32)) {
    host.ui.pointer_touch(PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
}

#[test]
fn a_long_press_on_plain_text_selects_the_word_then_raises_one_menu() {
    let mut host = host(false);
    let point = at(&host, 8);
    hold(&mut host, point);
    assert_eq!(host.ui.state.menus, vec![("wonderful".to_owned(), ContextMenuSource::Touch)]);
    release(&mut host, point);
    assert_eq!(selected(&host), "wonderful");
    assert_eq!(host.ui.state.menus.len(), 1, "one menu per press");
}

#[test]
fn a_long_press_over_the_selection_keeps_it() {
    let mut host = host(false);
    let first = at(&host, 8);
    hold(&mut host, first);
    release(&mut host, first);
    // Ctrl+A on the focused label: the whole text, then a long press inside it on another word.
    assert!(host.key_down("KeyA", Modifiers { ctrl: true, ..Modifiers::default() }));
    assert_eq!(selected(&host), TEXT);
    let second = at(&host, 18);
    hold(&mut host, second);
    release(&mut host, second);
    assert_eq!(selected(&host), TEXT, "the selection stayed");
    assert_eq!(host.ui.state.menus.last(), Some(&(TEXT.to_owned(), ContextMenuSource::Touch)));
    assert_eq!(host.ui.state.menus.len(), 2);
}

#[test]
fn a_long_press_on_another_control_raises_one_menu() {
    let mut host = host(false);
    let shape = host.rect(host.ui.state.shape);
    let point = (shape.center_x(), shape.center_y());
    hold(&mut host, point);
    release(&mut host, point);
    assert_eq!(host.ui.state.menus, vec![(String::new(), ContextMenuSource::Touch)]);
}

#[test]
fn the_browsers_own_menu_makes_no_second_one() {
    let mut host = host(false);
    let point = at(&host, 8);
    // Android Chrome: its contextmenu comes before the engine's long press is due.
    host.ui.pointer_touch(PointerKind::Down, point.0, point.1, host.time_ms());
    host.frame_after(500.0);
    host.ui.context_menu(point.0, point.1, ContextMenuSource::Touch, host.time_ms());
    assert_eq!(host.ui.state.menus, vec![("wonderful".to_owned(), ContextMenuSource::Touch)], "the word first, then the menu");
    host.frame_after(LONG_PRESS_MS);
    release(&mut host, point);
    assert_eq!(host.ui.state.menus.len(), 1, "one menu per press");
    // And when the browser asks after the engine's long press.
    let other = at(&host, 1);
    hold(&mut host, other);
    host.ui.context_menu(other.0, other.1, ContextMenuSource::Touch, host.time_ms());
    release(&mut host, other);
    assert_eq!(host.ui.state.menus.len(), 2, "one menu per press");
}

#[test]
fn a_menu_the_app_takes_hides_the_copy_button_one_it_leaves_keeps_it() {
    // The same selection either way: the frames differ by the Copy button only.
    let lit = |take: bool| {
        let mut host = host(take);
        let point = at(&host, 8);
        hold(&mut host, point);
        release(&mut host, point);
        assert_eq!(selected(&host), "wonderful");
        host.frame_after(16.0);
        let mut lit = 0;
        for y in 0..300 {
            for x in 0..420 {
                lit += (host.pixel(x, y) != Color::BLACK) as u32;
            }
        }
        lit
    };
    let (kept, hidden) = (lit(false), lit(true));
    assert!(kept > hidden + 200, "the Copy button shows when the app leaves the menu, not when it takes it: {kept} vs {hidden}");
}
