//! A recycled cell cached as ImageDoubleBuffered and bound to another item never shows the
//! previous item's bitmap while the new one is made: the placeholder shows until it is back
//! (DrawnUi.Net, Aromarket). A cell with `destroy_cache_on_context_change` off keeps showing its
//! last pixels meanwhile (a chat).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const COLORS: [Color; 7] = [
    Color::from_rgb(255, 0, 0),
    Color::from_rgb(0, 200, 0),
    Color::from_rgb(0, 0, 255),
    Color::from_rgb(255, 200, 0),
    Color::from_rgb(200, 0, 200),
    Color::from_rgb(0, 200, 200),
    Color::from_rgb(120, 60, 0),
];
const ROW: f32 = 50.0;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
}

fn host(destroy: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaScroll::new().fill().assign(&mut app.scroll).content(
            SkiaStack::new().fill_x().spacing(0).recycling_template(RecyclingTemplate::Enabled).measure_items_strategy(MeasuringStrategy::MeasureFirst).items(
                |_: &App| 500,
                move || {
                    let mut cell = Handle::default();
                    (SkiaShape::new().fill_x().height_request(ROW).use_cache(CacheType::ImageDoubleBuffered).destroy_cache_on_context_change(destroy).assign(&mut cell), cell)
                },
                |cell: &Handle<SkiaShape>, _: &App, index, cx| {
                    if let Some(mut c) = cx.get_mut(*cell) {
                        c.set_background_color(Some(COLORS[index % COLORS.len()]));
                    }
                },
            ),
        )
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    host
}

/// The colors each sampled row shows on the frames while the bitmaps are out, then at the end.
fn rebind(destroy: bool) -> (Vec<Vec<Color>>, Vec<Color>) {
    let mut host = host(destroy);
    host.hold_bakes(true);
    // Far enough that every cell is bound to another item.
    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().set_viewport_offset_y(scroll, -10.0 * ROW);
    let rows: Vec<i32> = (0..12).map(|r| r * 25 + 12).collect();
    let mut held = Vec::new();
    for _ in 0..4 {
        host.frame_after(16.0);
        held.push(rows.iter().map(|y| host.pixel(100, *y)).collect::<Vec<_>>());
    }
    // The bitmaps come back: what each row shows from now on is its item.
    host.deliver_bakes();
    host.hold_bakes(false);
    host.settle();
    let truth: Vec<Color> = rows.iter().map(|y| host.pixel(100, *y)).collect();
    (held, truth)
}

#[test]
fn a_cell_bound_to_another_item_never_shows_the_previous_items_bitmap() {
    let (held, truth) = rebind(true);
    assert!(truth.iter().all(|c| COLORS.contains(c)), "every row shows an item at the end: {truth:?}");
    for (frame, shown) in held.iter().enumerate() {
        for (row, (shown, item)) in shown.iter().zip(&truth).enumerate() {
            let other_item = COLORS.contains(shown) && shown != item;
            assert!(!other_item, "frame {frame}, sample {row}: another item's color {shown:?} over {item:?}");
        }
    }
}


#[test]
fn a_cell_that_keeps_its_pixels_shows_the_last_bitmap_until_the_new_one_is_back() {
    let (held, truth) = rebind(false);
    // Every row shows an item's bitmap at once, no placeholder (chat-like lists).
    assert!(held[0].iter().all(|c| COLORS.contains(c)), "{:?}", held[0]);
    assert!(truth.iter().all(|c| COLORS.contains(c)));
}
