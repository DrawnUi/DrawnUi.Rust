//! A Grid's Auto row holding a label that wraps at the grid's width gets the height of every line
//! (the gallery's Boss fight header on a 390 pt phone).

use drawnui::controls::layout::SkiaGrid;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    label: Handle<SkiaLabel>,
    below: Handle<SkiaShape>,
}

#[test]
fn an_auto_row_is_as_tall_as_its_wrapped_label() {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaGrid::new().fill().padding(20).row_spacing(10).row_definitions("Auto,*").children((
            SkiaLayout::column().spacing(8).fill_x().use_cache(CacheType::Image).row(0).children((
                SkiaLabel::new("BOSS FIGHT").font_size(30).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("A glTF model drawn in 3D inside DrawnUI, driven by the app. Drag it to turn it, tap it to hit it.")
                    .font_size(15)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center)
                    .assign(&mut app.label),
            )),
            SkiaShape::new().fill().row(1).assign(&mut app.below),
        ))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 390, 844, 1.0);
    host.settle();
    let (label, below) = (host.rect(host.ui.state.label), host.rect(host.ui.state.below));
    assert!(label.width() <= 350.0 + 0.5, "the label keeps to the grid's width: {label:?}");
    assert!(label.height() > 30.0, "wrapped on more than one line: {label:?}");
    assert!(below.top >= label.bottom, "the next row starts under the whole label: label {label:?}, below {below:?}");
}

#[derive(Default)]
struct Boss {
    head: Handle<SkiaLayout>,
    dragon: Handle<SkiaShape>,
    news: Handle<SkiaLabel>,
    bottom: Handle<SkiaLayout>,
    moves: Handle<SkiaLayout>,
    skins: Handle<SkiaLayout>,
}

/// The gallery's Boss fight page: a header, the dragon on the star row, a news line, the buttons.
fn boss(width: i32, height: i32, scale: f32) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    // The .NET gallery's UiButton: a fixed width, 40 tall, its caption at 14 with no side padding.
    let button = |caption: &'static str, w: f32| SkiaButton::new(caption).width_request(w).height_request(40).font_size(14).padding((0.0, 0.0));
    let ui = Ui::new(Boss::default(), |app: &mut Boss| {
        SkiaGrid::new().fill().padding(20).row_spacing(10).row_definitions("Auto,*,Auto,Auto").children((
            SkiaLayout::column().spacing(8).fill_x().use_cache(CacheType::Image).row(0).assign(&mut app.head).children((
                SkiaLabel::new("BOSS FIGHT").font_size(30).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("A glTF model drawn in 3D inside DrawnUI, driven by the app. Drag it to turn it, tap it to hit it.")
                    .font_size(15)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .horizontal_options(LayoutOptions::Center),
            )),
            SkiaShape::new().fill().row(1).assign(&mut app.dragon),
            SkiaLabel::new("Tap the dragon to hit it.").font_size(15).horizontal_options(LayoutOptions::Center).row(2).assign(&mut app.news),
            SkiaLayout::column().spacing(10).fill_x().use_cache(CacheType::Image).row(3).assign(&mut app.bottom).children((
                SkiaLabel::new("MOVES").font_size(13).horizontal_options(LayoutOptions::Center),
                SkiaLayout::wrap()
                    .spacing(8)
                    .horizontal_options(LayoutOptions::Center)
                    .assign(&mut app.moves)
                    .children(["Idle", "Fly", "Punch", "Headbutt", "Yes", "No", "Death"].map(|c| button(c, 100.0)).into_iter().collect::<Vec<_>>()),
                SkiaLabel::new("SKIN AND EYES (materials by name)").font_size(13).horizontal_options(LayoutOptions::Center),
                SkiaLayout::wrap().spacing(8).horizontal_options(LayoutOptions::Center).assign(&mut app.skins).children((
                    button("Ember", 100.0),
                    button("Frost", 100.0),
                    button("Venom", 100.0),
                    button("Shadow", 100.0),
                    button("Rage: off", 124.0),
                    button("Turn: off", 124.0),
                )),
            )),
        ))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, width, height, scale);
    host.settle();
    let s = &host.ui.state;
    (host.rect(s.head), host.rect(s.dragon), host.rect(s.news), host.rect(s.bottom), host.rect(s.moves), host.rect(s.skins))
}

/// The rows of the Boss fight page as the C# engine lays them out with the same rule (the
/// numbers of the .NET gallery, 2026-11-12): Auto rows at their column's real width, the star row
/// takes what is left.
#[test]
fn the_boss_page_rows_match_the_dotnet_layout() {
    let near = |what: &str, got: f32, want: f32| assert!((got - want).abs() <= 3.0, "{what}: {got} pt, the C# engine gives {want}");
    // An iPhone 14 below the gallery's header and tabs: 390 x 731.3 points at 3 pixels a point.
    let scale = 3.0;
    let (head, dragon, news, bottom, moves, skins) = boss(1170, 2194, scale);
    let pt = |v: f32| v / scale;
    near("header width", pt(head.width()), 350.0);
    near("header height (the subtitle on 2 lines)", pt(head.height()), 88.0);
    near("dragon height", pt(dragon.height()), 216.7);
    near("news height", pt(news.height()), 20.0);
    near("buttons height", pt(bottom.height()), 336.7);
    near("moves height (3 rows)", pt(moves.height()), 136.0);
    near("skin and eyes height (3 rows)", pt(skins.height()), 136.0);
    assert!(pt(bottom.bottom) <= 731.3 - 20.0 + 1.0, "every button on screen: the buttons end at {}", pt(bottom.bottom));
    // A 1280 x 796 desktop window below the gallery's 61 point header: everything on one row, the
    // dragon takes the rest.
    let (_, dragon, _, _, moves, skins) = boss(1280, 735, 1.0);
    near("desktop dragon width", dragon.width(), 1240.0);
    near("desktop dragon height", dragon.height(), 430.0);
    near("desktop moves (one row)", moves.height(), 40.0);
    near("desktop skin and eyes (one row)", skins.height(), 40.0);
}
