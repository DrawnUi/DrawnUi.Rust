//! SkiaSvg Success / Error (DrawnUI): the picture of a source or markup is there, or not; the
//! handler runs on the next frame with the source.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const STAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#;

#[derive(Default)]
struct App {
    log: Vec<String>,
}

fn host(svg: Build<SkiaSvg>) -> Headless<App> {
    let svg = svg
        .width_request(40)
        .height_request(40)
        .on_success(|_me, app: &mut App, _cx, source| app.log.push(format!("success {source}")))
        .on_error(|_me, app: &mut App, _cx, source| app.log.push(format!("error {source}")));
    let ui = Ui::new(App::default(), |_| SkiaLayout::new().fill().children(svg));
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    host
}

#[test]
fn markup_reports_success_or_error() {
    let mut ok = host(SkiaSvg::from_string(STAR));
    ok.settle();
    assert_eq!(ok.ui.state.log, ["success "]);
    let mut bad = host(SkiaSvg::from_string("not svg"));
    bad.settle();
    assert_eq!(bad.ui.state.log, ["error "]);
}

#[test]
fn a_file_reports_success_or_error_when_it_arrives() {
    let mut ok = host(SkiaSvg::new("star.svg"));
    ok.deliver_assets(|_| Some(STAR.as_bytes().to_vec()));
    ok.settle();
    assert_eq!(ok.ui.state.log, ["success star.svg"]);
    let mut missing = host(SkiaSvg::new("missing.svg"));
    missing.deliver_assets(|_| None);
    missing.settle();
    assert_eq!(missing.ui.state.log, ["error missing.svg"]);
}
