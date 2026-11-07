//! The system's light or dark appearance (`Ui::on_system_theme_changed`, `Cx::system_theme`).

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    heard: Vec<SystemTheme>,
    read: Option<SystemTheme>,
}

fn host(handler: bool) -> Headless<App> {
    let mut ui = Ui::new(App::default(), |_| SkiaLayout::new().fill());
    if handler {
        ui = ui.on_system_theme_changed(|app: &mut App, theme, cx| {
            app.heard.push(theme);
            app.read = cx.system_theme();
        });
    }
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    host
}

#[test]
fn the_app_hears_the_theme_at_start_and_once_per_change() {
    let mut host = host(true);
    assert!(host.ui.state.heard.is_empty(), "nothing before the host knows");
    host.system_theme(SystemTheme::Dark);
    host.system_theme(SystemTheme::Dark);
    assert_eq!(host.ui.state.heard, [SystemTheme::Dark]);
    assert_eq!(host.ui.state.read, Some(SystemTheme::Dark));
    assert!(host.ui.needs_frame(), "the app may restyle");
    host.settle();
    host.system_theme(SystemTheme::Light);
    host.system_theme(SystemTheme::Dark);
    assert_eq!(host.ui.state.heard, [SystemTheme::Dark, SystemTheme::Light, SystemTheme::Dark]);
}

#[test]
fn without_a_handler_nothing_runs_or_redraws() {
    let mut host = host(false);
    host.system_theme(SystemTheme::Dark);
    assert!(!host.ui.needs_frame());
}
