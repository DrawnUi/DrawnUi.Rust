//! Page text selection cases found by the React port (drawnui-cross 6ce): (a) a drag in the right
//! column of two columns never anchors at a left-column label; (b) a tap on the drawn Copy button
//! copies; (c) a press inside an open in-canvas popup (the app's Copy menu) keeps the page
//! selection; (d) a drag that selects one label copies with Ctrl+C.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{PointerKind, TouchEffect};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };

#[derive(Default)]
struct App {
    left: [Handle<SkiaLabel>; 3],
    right: [Handle<SkiaLabel>; 2],
    menu: Option<PopupId>,
}

fn label(text: &'static str, h: &mut Handle<SkiaLabel>) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(20).accessibility_text_selectable(true).assign(h)
}

/// Two columns: three labels at left, two at right with a gap between them; with `menu` the
/// app's own Copy menu.
fn host() -> Headless<App> {
    scene(true)
}

fn scene(menu: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let [l0, l1, l2] = &mut app.left;
        let [r0, r1] = &mut app.right;
        SkiaLayout::row().fill().padding(20).spacing(40).children((
            SkiaLayout::column().width_request(240).spacing(30).children((label("Left one here", l0), label("Left two here", l1), label("Left three here", l2))),
            SkiaLayout::column().width_request(240).spacing(90).children((label("Right one here", r0), label("Right two here", r1))),
        ))
        // The app's own Copy menu, drawn on the canvas.
        .on_context_menu(move |_me, app: &mut App, cx, at| {
            if !menu {
                return false;
            }
            let item = SkiaShape::new().width_request(120).height_request(40).background_color(Color::GRAY).on_tapped(|_me, app: &mut App, cx| {
                let text = cx.selected_text();
                cx.set_clipboard(text);
                if let Some(menu) = app.menu.take() {
                    cx.close_popup_window(menu);
                }
            });
            app.menu = Some(cx.open_popup_window(item, at.location, PopupWindowOptions::default()));
            true
        })
    })
    .font_bytes("Default", FONT)
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 640, 300, 1.0);
    host.settle();
    host
}

fn text(host: &Headless<App>, h: Handle<SkiaLabel>) -> String {
    host.ui.tree.find::<SkiaLabel>(h).unwrap().selected_text()
}

#[test]
fn a_drag_in_the_right_column_never_anchors_at_a_left_label() {
    let mut host = host();
    let (r0, r1) = (host.rect(host.ui.state.right[0]), host.rect(host.ui.state.right[1]));
    // From the gap between the right labels down into the second one.
    host.pan((r0.left + 20.0, (r0.bottom + r1.top) / 2.0), (r1.left + 60.0, r1.center_y()), 200.0, 6);
    let left: Vec<String> = host.ui.state.left.iter().map(|h| text(&host, *h)).collect();
    assert!(left.iter().all(String::is_empty), "nothing at left: {left:?}");
    assert!(!text(&host, host.ui.state.right[1]).is_empty(), "the right label under the drag");
    assert!(text(&host, host.ui.state.right[0]).is_empty());
}

#[test]
fn a_drag_that_selects_one_label_copies_with_ctrl_c() {
    let mut host = host();
    let r = host.rect(host.ui.state.left[0]);
    // From beside the label into it.
    host.pan((r.left - 10.0, r.center_y()), (r.left + 50.0, r.center_y()), 200.0, 6);
    let selected = text(&host, host.ui.state.left[0]);
    assert!(!selected.is_empty());
    assert!(host.key_down("KeyC", CTRL));
    assert_eq!(host.take_clipboard(), Some(selected));
}

#[test]
fn a_press_in_the_apps_copy_menu_keeps_the_page_selection() {
    let mut host = host();
    let (a, b) = (host.rect(host.ui.state.left[0]), host.rect(host.ui.state.left[1]));
    host.pan((a.left + 2.0, a.center_y()), (b.left + 50.0, b.center_y()), 200.0, 6);
    let both = format!("{}\n{}", text(&host, host.ui.state.left[0]), text(&host, host.ui.state.left[1]));
    assert!(host.right_click(b.left + 20.0, b.center_y()), "the menu opened");
    host.tap(b.left + 30.0, b.center_y() + 15.0);
    assert_eq!(host.take_clipboard(), Some(both), "the menu's Copy took the page selection");
}

#[test]
fn a_tap_on_the_drawn_copy_button_copies() {
    // No app menu: the long press shows the drawn Copy button.
    let mut host = scene(false);
    host.use_touch(true);
    let r = host.rect(host.ui.state.left[1]);
    let (x, y) = (r.left + 20.0, r.center_y());
    host.ui.pointer_touch(PointerKind::Down, x, y, host.time_ms());
    host.frame();
    host.frame_after(TouchEffect::long_press_time_ms_default() + 16.0);
    host.ui.pointer_touch(PointerKind::Up, x, y, host.time_ms());
    host.frame_after(16.0);
    let word = text(&host, host.ui.state.left[1]);
    assert_eq!(word, "Left");
    // The Copy button: the drawn dark pill near the selection (235 of 255 over the black page).
    let pill = Color::from_rgb(30, 34, 38);
    let near = |c: Color| (c.r() as i32 - pill.r() as i32).abs() < 6 && (c.g() as i32 - pill.g() as i32).abs() < 6 && (c.b() as i32 - pill.b() as i32).abs() < 6;
    let mut at = None;
    'find: for py in ((r.top - 60.0).max(0.0) as i32..(r.bottom + 60.0) as i32).step_by(2) {
        for px in (r.left as i32..(r.right + 120.0) as i32).step_by(2) {
            if near(host.pixel(px, py)) {
                at = Some((px as f32 + 6.0, py as f32 + 6.0));
                break 'find;
            }
        }
    }
    let (bx, by) = at.expect("the Copy button is drawn");
    host.tap(bx, by);
    assert_eq!(host.take_clipboard().as_deref(), Some("Left"));
}
