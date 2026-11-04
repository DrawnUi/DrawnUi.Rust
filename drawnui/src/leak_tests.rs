//! Closed pages, popups, modals and popup windows leave nothing behind: after N open / close
//! rounds every list the tree keeps per control is back at its size after the first round.

use crate::prelude::*;
use crate::testing::Headless;

#[derive(Default)]
struct App {
    shell: Handle<SkiaShell>,
    page: Handle<SkiaLayout>,
}

fn button(name: &'static str) -> Build<SkiaShape> {
    SkiaShape::new().width_request(80).height_request(30).accessibility_role(Aria::BUTTON).accessibility_label(name).on_tapped(|_me, _app: &mut App, _cx| {})
}

/// What the tree holds per control.
fn counts(host: &Headless<App>) -> Vec<(&'static str, usize)> {
    let t = &host.ui.tree;
    vec![
        ("live controls", t.nodes.iter().flatten().count()),
        ("arena slots", t.nodes.len()),
        ("animators", t.animators.len()),
        ("key listeners", t.key_listeners.len()),
        ("history listeners", t.history_listeners.len()),
        ("visibility listeners", t.app_visibility_listeners.len()),
        ("rotation listeners", t.device_rotation_listeners.len()),
        ("trackers", t.trackers.len()),
        ("movers", t.movers.len()),
        ("popup windows", t.popups.len()),
        ("accessibility nodes", t.accessibility.len()),
        ("modal registry", crate::tree::MODALS.with_borrow(|m| m.len())),
    ]
}

fn round(host: &mut Headless<App>) {
    let shell = host.ui.state.shell;
    host.ui.tree.cx().go_to(shell, "page", false);
    host.settle();
    // An animation the app paused on the page, left paused when the page closes.
    let page = host.ui.state.page;
    let animation = host.ui.tree.cx().animate(page, 10_000.0, crate::easing::linear, |_, _| {});
    host.ui.tree.cx().pause_animation(animation);
    host.ui.tree.cx().open_popup(shell, button("In popup"), PopupOptions { animated: false, ..Default::default() });
    host.settle();
    host.ui.tree.cx().close_popup(shell, false);
    host.settle();
    host.ui.tree.cx().push_modal(shell, button("In modal"), ModalOptions { animated: false, ..Default::default() });
    host.settle();
    host.ui.tree.cx().pop_modal(shell, false);
    host.settle();
    let opener: ControlId = page.into();
    let window = host.ui.tree.cx().open_popup_window(SkiaLayout::column().children((button("In menu"),)), opener, PopupWindowOptions::default());
    host.settle();
    host.ui.tree.cx().close_popup_window(window);
    host.settle();
    host.ui.tree.cx().go_back(shell, false);
    host.settle();
    host.frame_after(1100.0);
}

#[test]
fn closed_pages_popups_and_modals_leave_nothing_behind() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaShell::new()
            .assign(&mut app.shell)
            .route("page", |app: &mut App, _| SkiaLayout::column().fill().assign(&mut app.page).children((button("On page"), SkiaLabel::new("Keys").listen_keys().listen_history().listen_app_visibility().listen_device_rotation())))
            .root(SkiaLayout::column().fill().children((button("Root"),)))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    round(&mut host);
    let first = counts(&host);
    for _ in 0..20 {
        round(&mut host);
    }
    assert_eq!(counts(&host), first);
}
