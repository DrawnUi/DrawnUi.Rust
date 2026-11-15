//! A focused editor never takes the wheel from the page (DrawnUi.Net 62608ca7): a hover over the
//! field is no gesture, so it does not make the editor the owner the wheel goes to first.

use drawnui::prelude::*;
use drawnui::PointerKind;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    editor: Handle<SkiaEditor>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().fill_x().padding(20).spacing(20).children((
            SkiaEditor::new().width_request(300).height_request(44).assign(&mut app.editor),
            SkiaLayout::new().fill_x().height_request(3000).background_color(Color::YELLOW),
        )))
    });
    let mut host = Headless::new(ui.background(Color::WHITE), 400, 600, 1.0);
    host.settle();
    host
}

fn offset_y(host: &Headless<App>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

/// Frames until the scroll stands still.
fn settle(host: &mut Headless<App>) {
    let mut previous = offset_y(host);
    let mut still = 0;
    for _ in 0..200 {
        host.frame_after(16.0);
        let now = offset_y(host);
        still = if (now - previous).abs() < 0.01 { still + 1 } else { 0 };
        previous = now;
        if still >= 3 {
            break;
        }
    }
}

#[test]
fn the_page_scrolls_by_wheel_after_an_editor_took_the_focus() {
    let mut host = host();
    // Focus the field by a tap, then the pointer rests on it (hover moves).
    // (Not `tap`: it settles, and the caret blinks from now on.)
    host.ui.pointer(PointerKind::Down, 150.0, 42.0, host.time_ms());
    host.frame_after(16.0);
    host.ui.pointer(PointerKind::Up, 150.0, 42.0, host.time_ms());
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    assert!(host.ui.tree.find::<SkiaEditor>(host.ui.state.editor).unwrap().is_focused(), "the editor has the focus");
    host.hover(150.0, 40.0);
    host.hover(160.0, 44.0);

    // The wheel over the field scrolls the page.
    let start = offset_y(&host);
    host.wheel(160.0, 44.0, -1.0);
    settle(&mut host);
    let after_field = offset_y(&host);
    assert!(after_field < start - 1.0, "the wheel over the focused field did not scroll the page: {start} -> {after_field}");

    // And away from it.
    host.hover(200.0, 400.0);
    host.wheel(200.0, 400.0, -1.0);
    settle(&mut host);
    let after_away = offset_y(&host);
    assert!(after_away < after_field - 1.0, "the wheel away from the field did not scroll the page: {after_field} -> {after_away}");
}
