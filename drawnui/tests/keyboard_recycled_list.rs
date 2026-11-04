//! Arrow keys, Home, End, PageUp and PageDown in a recycled list reach every item, also the ones
//! that are not realized yet: the scroll brings the item in, then it gets the keyboard (C#
//! SkiaAccessibilityManager MoveInGroup / ProcessGroupFocus). 100 000 rows, as the hello app's Cells.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const ROWS: usize = 100_000;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaScroll::new().fill().assign(&mut app.scroll).content(
            SkiaStack::new()
                .recycling_template(RecyclingTemplate::Enabled)
                .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                .spacing(8)
                .accessibility_role(Aria::LIST)
                .items(
                    |_: &App| ROWS,
                    || {
                        let mut button = Handle::default();
                        (SkiaButton::new("").fill_x().height_request(40).assign(&mut button), button)
                    },
                    |button: &Handle<SkiaButton>, _: &App, index, cx| {
                        if let Some(mut b) = cx.get_mut(*button) {
                            b.set_text(format!("Contact {}", index + 1));
                        }
                    },
                ),
        )
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 600, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

/// The control the keyboard is on: its name and its rect on screen.
fn keyboard_on(host: &Headless<App>) -> Option<(String, Rect)> {
    let focused = host.ui.accessibility_focused()?;
    let node = host.ui.accessibility_nodes().iter().find(|n| n.control == focused)?;
    Some((node.label.clone(), node.rect))
}

fn press(host: &mut Headless<App>, key: &str) {
    host.press_key(key);
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    host.frame_after(1100.0);
}

fn on_screen(host: &Headless<App>, label: &str) {
    let (name, rect) = keyboard_on(host).unwrap_or_else(|| panic!("nothing has the keyboard, {label} expected"));
    assert_eq!(name, label);
    assert!(rect.top >= -0.5 && rect.bottom <= 600.5, "{label} at {rect:?}");
}

#[test]
fn keys_reach_rows_that_are_not_realized_yet() {
    let mut host = host();
    press(&mut host, "Tab");
    on_screen(&host, "Contact 1");
    press(&mut host, "End");
    on_screen(&host, &format!("Contact {ROWS}"));
    press(&mut host, "ArrowUp");
    on_screen(&host, &format!("Contact {}", ROWS - 1));
    press(&mut host, "Home");
    on_screen(&host, "Contact 1");
    // A page is the viewport over the row height (C# PageSize): 600 / 40 = 15 rows.
    press(&mut host, "PageDown");
    on_screen(&host, "Contact 16");
    for _ in 0..30 {
        press(&mut host, "ArrowDown");
    }
    on_screen(&host, "Contact 46");
}
