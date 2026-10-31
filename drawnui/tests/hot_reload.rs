//! Hot reload hands the app state to the new code: `hot_reload!`'s save reads the state of the
//! running `Ui<S>`, its make builds the new app with it. Handles in the state come back pointing
//! at nothing until the new tree assigns them; a state that no longer fits starts over.
#![cfg(feature = "hot-reload")]

use drawnui::prelude::*;
use drawnui::testing::Headless;
use serde::{Deserialize, Serialize};

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct App {
    count: i32,
    name: String,
    badge: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().children((
        SkiaLabel::new("").observe(|me, app: &App| me.set_text(format!("{} {}", app.name, app.count))),
        SkiaShape::new().width_request(10).height_request(10).assign(&mut app.badge),
    ))
}

drawnui::hot_reload!(App, |state| Ui::new(state, build).font_bytes("Default", FONT));

fn state(app: &dyn drawnui::App) -> &App {
    &app.as_any().and_then(|a| a.downcast_ref::<Ui<App>>()).expect("a Ui<App>").state
}

#[test]
fn the_new_app_starts_with_the_old_state() {
    let first = Ui::new(App { count: 7, name: "seven".into(), ..App::default() }, build).font_bytes("Default", FONT);
    let saved = drawnui_hot_save(&first);
    assert_eq!(saved.as_deref(), Some(r#"{"count":7,"name":"seven","badge":null}"#));

    let next = drawnui_hot_make(saved.as_deref());
    assert_eq!((state(&*next).count, state(&*next).name.as_str()), (7, "seven"));
}

#[test]
fn the_new_tree_assigns_the_handles_again() {
    let next = drawnui_hot_make(Some(r#"{"count":1,"name":"one","badge":null}"#));
    assert!(next.as_any().is_some_and(|a| a.is::<Ui<App>>()));
    // The Box<dyn App> is a Ui<App>: back to it, to run it headless.
    let ui = unsafe { *Box::from_raw(Box::into_raw(next) as *mut Ui<App>) };
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    let badge = host.ui.state.badge;
    assert!(host.ui.tree.get_mut(badge).is_some(), "the badge handle points at the new shape");
}

#[test]
fn a_state_that_no_longer_fits_starts_over() {
    let next = drawnui_hot_make(Some(r#"{"count":"not a number"}"#));
    assert_eq!(state(&*next).count, 0);
    // A missing field takes its default, the others are kept.
    let next = drawnui_hot_make(Some(r#"{"count":3}"#));
    assert_eq!((state(&*next).count, state(&*next).name.as_str()), (3, ""));
}

#[test]
fn another_app_saves_nothing() {
    let other = Ui::new(0_u8, |_: &mut u8| SkiaLabel::new("other"));
    assert_eq!(drawnui_hot_save(&other), None);
}
