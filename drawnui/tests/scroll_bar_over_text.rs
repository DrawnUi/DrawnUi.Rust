//! A draggable scroll bar over a scroll that holds selectable text: a mouse drag on the bar moves
//! the page and selects nothing (DrawnUi.Net dfb8f7a3).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    labels: Vec<Handle<SkiaLabel>>,
}

#[test]
fn a_drag_on_the_bar_scrolls_and_selects_nothing() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        app.labels = vec![Handle::default(); 40];
        SkiaScroll::new()
            .fill()
            .assign(&mut app.scroll)
            .scroll_bar(SkiaScrollBar::new().is_draggable(true).thickness(6))
            .content(SkiaLayout::column().fill_x().padding(20).spacing(16).children(
                app.labels.iter_mut().enumerate().map(|(i, h)| SkiaLabel::new(format!("Selectable line number {i}")).font_size(18).fill_x().accessibility_text_selectable(true).assign(h)).collect::<Vec<_>>(),
            ))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    // Show the bar (it fades in on a scroll), then drag its thumb down from the top.
    host.wheel(200.0, 200.0, -1.0);
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let before = host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y();
    host.pan((396.0, 20.0), (396.0, 200.0), 300.0, 10);
    for _ in 0..30 {
        host.frame_after(16.0);
    }
    let after = host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y();
    assert!(after < before - 50.0, "the bar moved the page: {before} -> {after}");
    let selected: Vec<String> = host.ui.state.labels.iter().map(|h| host.ui.tree.find::<SkiaLabel>(*h).unwrap().selected_text()).filter(|t| !t.is_empty()).collect();
    assert!(selected.is_empty(), "nothing selected: {selected:?}");
}
