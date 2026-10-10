//! `on_down` / `on_up` (DrawnUI SkiaButton Down / Up): a press and its release reaching a control,
//! with the point in its own points; each once per gesture.

use drawnui::prelude::*;
use drawnui::Detached;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    log: Vec<String>,
}

#[test]
fn a_tap_reports_down_tapped_and_up_once_each() {
    for button in [true, false] {
        let ui = Ui::new(App::default(), move |_| {
            let log = |what: &'static str| move |_me: &mut Mut<'_, SkiaLayout>, app: &mut App, _cx: &mut Cx<'_>, at: Point| app.log.push(format!("{what} {} {}", at.x, at.y));
            let target: Detached = if button {
                let b = SkiaButton::new("Go").width_request(100).height_request(40).margin(Thickness::new(20.0, 10.0, 0.0, 0.0));
                b.on_down(|_me, app: &mut App, _cx, at: Point| app.log.push(format!("down {} {}", at.x, at.y)))
                    .on_up(|_me, app: &mut App, _cx, at: Point| app.log.push(format!("up {} {}", at.x, at.y)))
                    .on_tapped(|_me, app: &mut App, _cx| app.log.push("tapped".into()))
                    .into()
            } else {
                // A plain layout: no child under the press takes it, so it is this one's.
                SkiaLayout::new()
                    .width_request(100)
                    .height_request(40)
                    .margin(Thickness::new(20.0, 10.0, 0.0, 0.0))
                    .background_color(Color::GRAY)
                    .on_down(log("down"))
                    .on_up(log("up"))
                    .on_tapped(|_me, app: &mut App, _cx| app.log.push("tapped".into()))
                    .into()
            };
            SkiaLayout::new().fill().children(target)
        });
        let mut host = Headless::new(ui, 300, 200, 1.0);
        host.settle();
        host.tap(30.0, 15.0);
        // The recognizer gives the tap before the release (React order); C# raises Up first.
        assert_eq!(host.ui.state.log, ["down 10 5", "tapped", "up 10 5"], "button: {button}");
    }
}
