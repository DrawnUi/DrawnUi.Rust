//! Designer actions (`Ui::designer_action`, DrawnUI DesignerActions): named states the DrawnUI
//! Designer jumps the app to. Kept in declaration order, a path replaces in place, actions added and
//! removed while the app runs, and nothing runs until asked.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct Game {
    level: u32,
    runs: u32,
}

fn host() -> Headless<Game> {
    let ui = Ui::new(Game::default(), |_| SkiaLayout::new().fill())
        .designer_action("Levels/Level 1", |g: &mut Game, _cx| g.level = 1)
        .designer_action("Levels/Level 2", |g: &mut Game, _cx| g.level = 2)
        .designer_action("Dialogs/Options", |g: &mut Game, cx| {
            g.runs += 1;
            // A screen offers an action while it is shown.
            cx.add_designer_action("Dialogs/Close options", |g: &mut Game, cx: &mut Cx| {
                g.runs += 10;
                cx.remove_designer_action("Dialogs/Close options");
            });
        })
        .designer_action("Levels/Level 1", |g: &mut Game, _cx| g.level = 100);
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host
}

fn paths(host: &Headless<Game>) -> Vec<&str> {
    host.ui.designer_actions().collect()
}

#[test]
fn actions_keep_their_order_and_a_path_replaces_in_place() {
    let host = host();
    assert_eq!(paths(&host), ["Levels/Level 1", "Levels/Level 2", "Dialogs/Options"]);
    assert_eq!((host.ui.state.level, host.ui.state.runs), (0, 0), "nothing runs until asked");
}

#[test]
fn running_an_action_changes_the_state_and_actions_come_and_go() {
    let mut host = host();
    assert!(host.ui.run_designer_action("Levels/Level 1"));
    assert_eq!(host.ui.state.level, 100, "the replacing closure");
    assert!(host.ui.needs_frame());
    assert!(!host.ui.run_designer_action("Levels/Level 9"));

    let before = host.ui.designer_actions_revision();
    assert!(host.ui.run_designer_action("Dialogs/Options"));
    assert_ne!(host.ui.designer_actions_revision(), before, "the list changed");
    assert_eq!(paths(&host), ["Levels/Level 1", "Levels/Level 2", "Dialogs/Options", "Dialogs/Close options"]);
    // Still there after it ran.
    assert!(host.ui.run_designer_action("Dialogs/Options"));
    assert_eq!(host.ui.state.runs, 2);

    // An action that removes itself is gone after it ran.
    assert!(host.ui.run_designer_action("Dialogs/Close options"));
    assert_eq!(host.ui.state.runs, 12);
    assert_eq!(paths(&host), ["Levels/Level 1", "Levels/Level 2", "Dialogs/Options"]);
}
