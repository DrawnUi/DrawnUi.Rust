//! The pages of the demo and what they share: the scrolling page column and the cards.

pub mod a11y;
pub mod animations;
pub mod cells;
pub mod editor;
pub mod images;
pub mod keyboard;
pub mod layouts;
pub mod looks;
pub mod pong;
pub mod reorder;
pub mod root;
pub mod scroll;
pub mod shaders;
pub mod shapes;
pub mod shell;
pub mod snapping;
pub mod sprites;
pub mod svg;
pub mod text;
pub mod transforms;
pub mod uneven;

use drawnui::prelude::*;

use crate::catalog::Sample;
use crate::{App, hex};

/// Canvas and page background, same as the C#, React and WPF demos.
pub const PAGE_BACKGROUND: Color = hex(0x212529);

/// The page of a sample, built by the shell on every navigation to it, from a fresh state as C#
/// and React build a new page per navigation. The shell puts it under its nav bar.
// ponytail: one state per page type; the same page pushed twice shares it (the Shell page's
// "GoToAsync('shell') again"): the lower one's handles then point at the upper one's controls.
pub fn page(app: &mut App, sample: &'static Sample) -> Build<SkiaLayout> {
    match sample.route {
        "cells" => app.cells = Default::default(),
        "uneven" => app.uneven = Default::default(),
        "images" => app.images = Default::default(),
        "text" => app.text = Default::default(),
        "layouts" => app.layouts = Default::default(),
        "looks" => app.looks = Default::default(),
        "animations" => app.animations = Default::default(),
        "shell" => app.shell_page = Default::default(),
        "snapping" => app.snapping = Default::default(),
        "pong" => app.pong = Default::default(),
        "editor" => app.editor = Default::default(),
        "keyboard" => app.keyboard = Default::default(),
        "scroll" => app.scroll = Default::default(),
        "shaders" => app.shaders = Default::default(),
        "sprites" => app.sprites = Default::default(),
        "transforms" => app.transforms = Default::default(),
        "reorder" => app.reorder = Default::default(),
        "a11y" => app.a11y = Default::default(),
        _ => {}
    }
    app.opening.push(sample.route);
    (sample.build)(app)
}

/// Pages the shell built since the last call start what they run while open.
pub fn start(app: &mut App, cx: &mut Cx) {
    for route in std::mem::take(&mut app.opening) {
        opened(app, cx, route);
    }
}

/// Pages that run something while they are open start it here.
fn opened(app: &mut App, cx: &mut Cx, route: &str) {
    match route {
        "images" => images::opened(app, cx),
        "shaders" => shaders::opened(app, cx),
        "a11y" => a11y::opened(app, cx),
        "layouts" => layouts::opened(app, cx),
        _ => {}
    }
}

/// A key event goes to the page on top if it listens to the keyboard. True = used.
pub fn key(app: &mut App, event: &KeyEvent, cx: &mut Cx) -> bool {
    match app.route {
        Some("keyboard") => keyboard::key(app, event),
        Some("sprites") => sprites::key(app, event, cx),
        _ => false,
    }
}

/// A page that scrolls, built as the C# pages are: a layer, one vertical SkiaScroll filling it,
/// the page's stack as its content. Cache the content or its cards, never the scroll or this layer.
pub fn scrolling(content: Build<SkiaLayout>) -> Build<SkiaLayout> {
    // Horizontal drags go to the carousels and horizontal scrolls inside the page.
    SkiaLayer::new().fill().children(SkiaScroll::new().fill().ignore_wrong_direction(true).content(content))
}

/// The content column of the React pages: centered, at most 720 points wide, as wide as its
/// widest child; a Fill child makes it take the constraint.
pub fn column() -> Build<SkiaLayout> {
    SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720)
}

/// A titled card of the React pages (their `Card` component): the title label is given, so a
/// page can keep it changing.
pub fn card(title: Build<SkiaLabel>, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(hex(0x2B3035))
        .fill_x()
        .children(SkiaStack::new().spacing(10).padding((16, 12)).children((title, content)))
}

/// The title of a card: small, bold, uppercase, accent blue.
pub fn card_title(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(12)
        .text_color(hex(0x6EA8FE))
        .font_attributes(FontAttributes::Bold)
        .text_transform(TextTransform::Uppercase)
}

/// A page heading.
pub fn page_title(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center)
}

/// Sets the text of a label through its handle: what binding a recycled cell is made of.
pub fn set_text(cx: &mut Cx, label: Handle<SkiaLabel>, text: impl Into<String>) {
    if let Some(mut label) = cx.get_mut(label) {
        label.set_text(text.into());
    }
}

#[cfg(test)]
mod tests {
    //! The whole app on the headless host, driven by real taps, wheel and pans.
    use drawnui::testing::Headless;

    use super::*;
    use crate::catalog::SAMPLES;

    fn font(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/assets/{name}", env!("CARGO_MANIFEST_DIR"))).expect("font file")
    }

    /// The app in a window of 1000 x `height` points.
    fn host(height: i32) -> Headless<App> {
        host_sized(1000, height)
    }

    /// The app in a window of `width` x `height` points.
    fn host_sized(width: i32, height: i32) -> Headless<App> {
        let mut host = unsettled(width, height);
        host.settle();
        host
    }

    /// The app before its first frame.
    fn unsettled(width: i32, height: i32) -> Headless<App> {
        unsettled_at(width, height, 1.0)
    }

    /// The app on a canvas of `width` x `height` pixels at `scale` pixels per point.
    fn unsettled_at(width: i32, height: i32, scale: f32) -> Headless<App> {
        let mut ui = crate::configure(Ui::new(App::default(), crate::build))
            .font_bytes("FontText", &font("OpenSans-Regular.ttf"))
            .font_bytes("FontGame", &font("Orbitron-Regular.ttf"))
            .font_bytes("FontTextBold", &font("OpenSans-Semibold.ttf"))
            .font_bytes("FontSymbols", &font("NotoSansMathSymbols-Subset.ttf"))
            .font_bytes("FontSymbols2", &font("NotoSansSymbols2-Subset.ttf"))
            .font_bytes("FontEmoji", &font("NotoColorEmoji-Subset.ttf"))
            .background(PAGE_BACKGROUND);
        ui.fonts.add_weight("FontText", 600, &font("OpenSans-Semibold.ttf"));
        Headless::new(ui, width, height, scale)
    }

    /// Every visible control under `id`, depth first.
    fn visible(host: &Headless<App>, id: ControlId, out: &mut Vec<ControlId>) {
        let tree = &host.ui.tree;
        if tree.base(id).is_none_or(|b| !b.p.is_visible) {
            return;
        }
        out.push(id);
        for &child in tree.children(id) {
            visible(host, child, out);
        }
    }

    fn all(host: &Headless<App>) -> Vec<ControlId> {
        let mut out = Vec::new();
        visible(host, host.ui.tree.root().unwrap(), &mut out);
        out
    }

    /// Where a control is on screen: its rect moved by every scroll above it.
    fn on_screen(host: &Headless<App>, id: ControlId) -> Rect {
        let tree = &host.ui.tree;
        let (mut rect, mut current) = (host.rect(id), tree.parent(id));
        while let Some(parent) = current {
            rect = rect.with_offset(tree.base(parent).unwrap().content_offset);
            current = tree.parent(parent);
        }
        rect
    }

    /// The bytes of an app file; a cache-busting query (`?queue=3`) is not part of the path.
    fn file(source: &str) -> Option<Vec<u8>> {
        let path = source.split('?').next().unwrap_or(source);
        std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).ok()
    }

    /// Frames 16 ms apart (jumping to waiting timers) until nothing is pending, for at most
    /// `ms`: unlike `settle`, a page that animates forever is fine.
    fn run(host: &mut Headless<App>, ms: f64) {
        let end = host.time_ms() + ms;
        host.frame();
        while host.time_ms() < end {
            match (host.ui.needs_frame(), host.ui.wake_at()) {
                (true, _) => host.frame_after(16.0),
                (false, Some(wake)) => host.frame_after((wake - host.time_ms()).clamp(1.0, end - host.time_ms() + 1.0)),
                (false, None) => return,
            }
        }
    }

    /// Answers the image and asset loads the pages asked for with the files of the app folder,
    /// again while frames ask for more.
    fn deliver(host: &mut Headless<App>) {
        for _ in 0..8 {
            let (images, assets) = (host.deliver_images(file).len(), host.deliver_assets(file).len());
            run(host, 2000.0);
            if images + assets == 0 {
                return;
            }
        }
    }

    /// The visible label or button with this text.
    fn showing(host: &Headless<App>, text: &str) -> ControlId {
        let tree = &host.ui.tree;
        let shows = |id: &ControlId| {
            tree.find::<SkiaButton>(*id).is_some_and(|b| b.p.text == text) || tree.find::<SkiaLabel>(*id).is_some_and(|l| l.p.text == text)
        };
        all(host).into_iter().find(shows).unwrap_or_else(|| panic!("nothing shows \"{text}\""))
    }

    /// Scrolls the catalog so the card with this title is inside the window.
    fn scroll_into_view(host: &mut Headless<App>, text: &str) {
        let target = showing(host, text);
        let rect = on_screen(host, target);
        let height = host.rect(host.ui.tree.root().unwrap()).height();
        if rect.top >= 0.0 && rect.bottom <= height {
            return;
        }
        // The scroll is the catalog layer's child.
        let scroll = host.ui.tree.children(host.ui.state.catalog)[0];
        let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
        host.ui.tree.cx().scroll_to(scroll, 0.0, offset - (rect.center_y() - height / 2.0), 0);
        host.settle();
    }

    /// Taps the middle of the visible label or button with this text.
    fn tap(host: &mut Headless<App>, text: &str) {
        let rect = on_screen(host, showing(host, text));
        host.tap(rect.center_x(), rect.center_y());
    }

    /// Scrolls the nearest scroll above the visible label or button with this text so it is in
    /// the window, then taps it without `settle`.
    fn tap_on_page(host: &mut Headless<App>, text: &str) {
        into_view(host, text);
        tap_no_settle(host, text);
    }

    /// Scrolls the nearest scroll above the visible label or button with this text so it is in the window.
    fn into_view(host: &mut Headless<App>, text: &str) {
        let target = showing(host, text);
        let height = host.rect(host.ui.tree.root().unwrap()).height();
        let rect = on_screen(host, target);
        if rect.top < 0.0 || rect.bottom > height {
            let mut scroll = host.ui.tree.parent(target);
            while let Some(id) = scroll.filter(|id| host.ui.tree.find::<SkiaScroll>(*id).is_none()) {
                scroll = host.ui.tree.parent(id);
            }
            let scroll = scroll.expect("a scroll above the control");
            let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
            host.ui.tree.cx().scroll_to(scroll, 0.0, offset - (rect.center_y() - height / 2.0), 0);
            run(host, 500.0);
        }
    }

    /// A tap without `settle`, for while an endless animation runs.
    fn tap_no_settle(host: &mut Headless<App>, text: &str) {
        let rect = on_screen(host, showing(host, text));
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Down, rect.center_x(), rect.center_y(), now);
        host.ui.pointer(drawnui::PointerKind::Up, rect.center_x(), rect.center_y(), now);
        host.frame();
    }

    fn shot(host: &mut Headless<App>, name: &str) {
        if let Ok(folder) = std::env::var("HELLORUST_SHOTS") {
            host.save_png(&format!("{folder}/{}.png", name.replace(' ', "")));
        }
    }

    /// The item of the list row under a point of the screen (the gap below a row belongs to it),
    /// and where the middle of that row is.
    fn row_at(host: &Headless<App>, list: Handle<SkiaLayout>, x: f32, y: f32) -> (usize, f32) {
        let tree = &host.ui.tree;
        let cell = tree
            .children(list)
            .iter()
            .map(|cell| (*cell, on_screen(host, *cell)))
            .filter(|(cell, r)| tree.base(*cell).unwrap().p.is_visible && r.left <= x && x < r.right && r.top <= y)
            .max_by(|a, b| a.1.top.total_cmp(&b.1.top));
        let (cell, rect) = cell.expect("a row under the point");
        (tree.base(cell).unwrap().context_index.unwrap(), rect.center_y())
    }

    /// Taps the middle of the row under a point; returns its item.
    fn tap_row(host: &mut Headless<App>, list: Handle<SkiaLayout>, x: f32, y: f32) -> usize {
        let (item, middle) = row_at(host, list, x, y);
        host.tap(x, middle);
        item
    }

    /// Opens a page from the catalog and delivers what it loads; the page may animate forever.
    fn open_page(host: &mut Headless<App>, title: &str) {
        scroll_into_view(host, title);
        tap_no_settle(host, title);
        deliver(host);
        assert!(host.ui.state.route.is_some(), "{title} did not open");
    }

    /// A key pressed and released, with frames after both.
    fn press(host: &mut Headless<App>, key: &'static str) {
        host.ui.key(KeyKind::Down, key, "", Modifiers::default(), false);
        host.frame_after(16.0);
        host.ui.key(KeyKind::Up, key, "", Modifiers::default(), false);
        host.frame_after(16.0);
    }

    #[test]
    fn catalog_lists_the_react_samples_with_gradient_titles() {
        let host = host(700);
        let titles: Vec<&str> = SAMPLES.iter().map(|s| s.title).collect();
        assert_eq!(titles.len(), 20);
        assert_eq!((titles[0], titles[19]), ("Recycled cells", "Accessibility"));
        // Every card title is drawn with one of the six accent gradients, in turn.
        for (i, sample) in SAMPLES.iter().enumerate() {
            let label = showing(&host, sample.title);
            let gradient = host.ui.tree.base(label).unwrap().p.fill_gradient.clone().expect("a title gradient");
            assert_eq!(gradient.colors[0], root::title_gradient(i)[0], "{}", sample.title);
        }
    }

    #[test]
    fn keyboard_page_shows_keys_modifiers_and_typed_text() {
        let mut host = host(900);
        open_page(&mut host, "Keyboard Input");
        showing(&host, "Keyboard input ready");
        let shift = Modifiers { shift: true, ..Modifiers::default() };
        host.ui.key(KeyKind::Down, "KeyA", "", shift, false);
        host.ui.key(KeyKind::Char, "", "A", shift, false);
        host.frame_after(16.0);
        showing(&host, "Keyboard probe live");
        showing(&host, "Last key: down KeyA");
        showing(&host, "Modifiers: shift true, ctrl false, alt false");
        showing(&host, "KeyChar (printable, no Ctrl/Alt): \"A\"");
        host.ui.key(KeyKind::Up, "KeyA", "", Modifiers::default(), false);
        host.frame_after(16.0);
        showing(&host, "Last key: up KeyA");
        showing(&host, "Modifiers: shift false, ctrl false, alt false");
        // Recent events, newest first.
        showing(&host, "up KeyA");
        showing(&host, "down KeyA");
        shot(&mut host, "Keyboard Input keys");
    }

    #[test]
    fn sprites_warrior_walks_with_the_keys_and_attacks() {
        let mut host = host(1400);
        open_page(&mut host, "Sprites");
        assert!(host.ui.state.sprites.info.starts_with("8 frames"), "{}", host.ui.state.sprites.info);
        let player = host.ui.state.sprites.player;
        press(&mut host, "ArrowRight");
        run(&mut host, 400.0);
        let page = &host.ui.state.sprites;
        assert_eq!((page.col, page.row, page.warrior), (2, 1, sprites::Warrior::IdleRight));
        assert_eq!(host.ui.tree.base(player).unwrap().p.translation_x, 128.0);
        // A left key turns the warrior: its sheet is mirrored.
        press(&mut host, "KeyA");
        run(&mut host, 400.0);
        let page = &host.ui.state.sprites;
        assert_eq!((page.col, page.warrior), (1, sprites::Warrior::IdleLeft));
        assert_eq!(host.ui.tree.base(player).unwrap().p.scale_x, -1.0);
        // Space attacks for half a second, then the warrior stands again.
        press(&mut host, "Space");
        assert_eq!(host.ui.state.sprites.warrior, sprites::Warrior::WarLeft);
        shot(&mut host, "Sprites attack");
        run(&mut host, 600.0);
        assert_eq!(host.ui.state.sprites.warrior, sprites::Warrior::IdleLeft);
        // The edge stops the walk.
        press(&mut host, "ArrowUp");
        run(&mut host, 400.0);
        press(&mut host, "ArrowUp");
        run(&mut host, 400.0);
        assert_eq!(host.ui.state.sprites.row, 0);
    }

    #[test]
    fn layouts_templated_wrap_follows_count_and_split() {
        let mut host = host(4200);
        open_page(&mut host, "Layouts");
        showing(&host, "SkiaWrap ItemsSource (10 recycled ChipCell) · Split=3 · DynamicColumns=false");
        tap_no_settle(&mut host, "+ item");
        run(&mut host, 1000.0);
        assert_eq!(host.ui.state.layouts.count, 11);
        showing(&host, "Item 11");
        showing(&host, "SkiaWrap ItemsSource (11 recycled ChipCell) · Split=3 · DynamicColumns=false");
        tap_no_settle(&mut host, "Split 0 (flow)");
        run(&mut host, 1000.0);
        assert_eq!(host.ui.state.layouts.split, 0);
        // The decorated grid shows its twelve facts.
        showing(&host, "drawnui.net");
        shot(&mut host, "Layouts templated");
    }

    #[test]
    fn images_page_opens_with_the_photo_preloaded_by_the_catalog() {
        let mut host = host(900);
        let preloaded: Vec<String> = host.deliver_images(file).into_iter().map(|request| request.source).collect();
        assert!(preloaded.iter().any(|source| source == images::PHOTO), "preloaded {preloaded:?}");
        run(&mut host, 100.0);
        scroll_into_view(&mut host, "Images");
        tap_no_settle(&mut host, "Images");
        run(&mut host, 2000.0);
        assert_eq!(host.ui.state.route, Some("images"));
        let loaded: Vec<String> = host.deliver_images(file).into_iter().map(|request| request.source).collect();
        assert!(loaded.is_empty(), "the page loaded {loaded:?}");
    }

    #[test]
    fn images_tiles_move_every_50_ms() {
        let mut host = host(900);
        open_page(&mut host, "Images");
        let before = host.ui.state.images.offset;
        run(&mut host, 500.0);
        let moved = host.ui.state.images.offset - before;
        assert!((36.0..=44.0).contains(&moved), "moved {moved}");
    }

    #[test]
    fn gif_plays_stops_and_reports() {
        let mut host = host(1600);
        open_page(&mut host, "Lottie & GIF");
        let status = host.ui.state.animations.gif_status.clone();
        assert!(status.ends_with("ms, playing"), "{status}");
        // The GIF card's Stop: the second Stop of the page (the first one is the Lottie's).
        let stops: Vec<ControlId> = all(&host)
            .into_iter()
            .filter(|id| host.ui.tree.find::<SkiaButton>(*id).is_some_and(|b| b.p.text == "Stop"))
            .collect();
        let rect = on_screen(&host, stops[1]);
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Down, rect.center_x(), rect.center_y(), now);
        host.ui.pointer(drawnui::PointerKind::Up, rect.center_x(), rect.center_y(), now);
        // A GIF frame lasts 100 ms: the stopped run reports Finished at the next tick.
        run(&mut host, 200.0);
        assert_eq!(host.ui.state.animations.gif_status, "Finished");
        let gif = host.ui.state.animations.gif;
        assert!(!host.ui.tree.find::<SkiaGif>(gif).unwrap().player().is_playing());
    }

    #[test]
    fn a11y_toggles_report_pressed_in_the_overlay() {
        let mut host = host(1000);
        // What the web host does: it renders the overlay.
        host.ui.set_accessibility_enabled(true);
        open_page(&mut host, "Accessibility");
        tap_on_page(&mut host, "Sound: on");
        // The snapshot is rebuilt at most once a second.
        run(&mut host, 1100.0);
        host.frame_after(1100.0);
        showing(&host, "Sound: off");
        // The page reads the snapshot every 300 ms.
        let page = &host.ui.state.a11y;
        assert_eq!(page.last_activated, "sound");
        assert!(page.nodes > 10, "{} nodes", page.nodes);
        showing(&host, &format!("Nodes in the overlay: {} · focused: none · last activated: sound", page.nodes));
        let nodes = host.ui.accessibility_nodes();
        let sound = nodes.iter().find(|n| n.label == "Sound").expect("a Sound node");
        assert_eq!((&*sound.role, sound.is_pressed), (Aria::BUTTON, Some(false)));
        let settings = nodes.iter().find(|n| n.label == "Open settings").expect("the settings card");
        assert!(settings.can_interact);
        assert!(!nodes.iter().any(|n| n.role == Aria::PRESENTATION));
        // The selectable paragraph carries its drawn lines; a normal label carries none.
        let selectable = nodes.iter().find(|n| n.label.starts_with("This paragraph is drawn on the canvas")).expect("the selectable paragraph");
        assert!(selectable.text_lines.len() > 1, "{} lines", selectable.text_lines.len());
        let normal = nodes.iter().find(|n| n.label.starts_with("This one is a normal label")).expect("the normal label");
        assert!(normal.text_lines.is_empty());
    }

    #[test]
    fn reorder_buttons_and_a_drag_move_the_rows() {
        let mut host = host(900);
        open_page(&mut host, "Drag to reorder");
        let tags = |host: &Headless<App>| host.ui.state.reorder.items.iter().map(|item| item.tag).take(12).collect::<Vec<_>>();
        tap_no_settle(&mut host, "1st below 10th");
        run(&mut host, 500.0);
        assert_eq!(tags(&host)[9], "en-US");
        showing(&host, "moved 1st below 10th · offset 0 pt · es-ES, fr-FR, de-DE, it-IT, pt-BR…");
        tap_no_settle(&mut host, "Reverse");
        run(&mut host, 500.0);
        assert_eq!(tags(&host)[0], "af-ZA");
        showing(&host, "af-ZA");
        tap_no_settle(&mut host, "Reset");
        run(&mut host, 500.0);
        assert_eq!(tags(&host)[..3], ["en-US", "es-ES", "fr-FR"]);

        // A drag by the grip of the second row: carried two rows down and dropped there.
        let rows = host.ui.state.reorder.rows;
        let grip_of = |host: &mut Headless<App>, index: usize| {
            let rect = host.ui.tree.cx().item_rect(rows, index).expect("the row on screen");
            (rect.left + 23.0, rect.center_y())
        };
        let (x, y) = grip_of(&mut host, 1);
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Down, x, y, now);
        run(&mut host, 50.0);
        let ghost = host.ui.state.reorder.ghost;
        assert_eq!(host.ui.state.reorder.dragging, Some(2));
        assert!(host.ui.tree.base(ghost).unwrap().p.is_visible);
        shot(&mut host, "Drag to reorder lifted");
        // Two strides (a 44 pt row and the 6 pt gap) and a bit, in small steps.
        for step in 1..=11 {
            let now = host.time_ms();
            host.ui.pointer(drawnui::PointerKind::Move, x, y + 10.0 * step as f32, now);
            host.frame_after(16.0);
        }
        run(&mut host, 100.0);
        assert_eq!(host.ui.state.reorder.items[3].tag, "es-ES");
        // The list did not scroll: the grip took the pan.
        let scroll = host.ui.state.reorder.scroll;
        assert_eq!(host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y(), 0.0);
        shot(&mut host, "Drag to reorder carried");
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Up, x, y + 110.0, now);
        run(&mut host, 400.0);
        assert_eq!(host.ui.state.reorder.dragging, None);
        assert!(!host.ui.tree.base(ghost).unwrap().p.is_visible);
        assert_eq!(tags(&host)[..4], ["en-US", "fr-FR", "de-DE", "es-ES"]);
        assert!(host.ui.state.reorder.status.starts_with("dropped"), "{}", host.ui.state.reorder.status);

        // Held at the bottom edge the list scrolls under the ghost and the row keeps advancing.
        let (x, y) = grip_of(&mut host, 0);
        let bottom = host.ui.tree.base(scroll).unwrap().rect.bottom;
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Down, x, y, now);
        host.frame_after(16.0);
        let mut at = y;
        while at < bottom - 10.0 {
            at = (at + 20.0).min(bottom - 10.0);
            let now = host.time_ms();
            host.ui.pointer(drawnui::PointerKind::Move, x, at, now);
            host.frame_after(16.0);
        }
        run(&mut host, 1000.0);
        let position = host.ui.state.reorder.items.iter().position(|item| item.tag == "en-US").unwrap();
        assert!(position > 15, "en-US at {position}");
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Up, x, at, now);
        run(&mut host, 400.0);
        assert_eq!(host.ui.state.reorder.dragging, None);
        shot(&mut host, "Drag to reorder edge");

        // A drag beside the grip scrolls the list.
        let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
        host.pan((500.0, 600.0), (500.0, 400.0), 200.0, 10);
        run(&mut host, 1000.0);
        assert_ne!(host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y(), offset);
    }

    #[test]
    fn looks_live_card_follows_the_style_and_controls_report() {
        let mut host = host(1400);
        open_page(&mut host, "Common Controls");
        showing(&host, "Live — Unset");
        tap_on_page(&mut host, "Live card: Unset — tap to switch style");
        run(&mut host, 500.0);
        showing(&host, "Live — Windows");
        showing(&host, "Live card: Windows — tap to switch style");
        // The first "One" radio is the live card's; the second is the Default card's.
        let twos: Vec<ControlId> = all(&host)
            .into_iter()
            .filter(|id| host.ui.tree.find::<SkiaRadioButton>(*id).is_some_and(|r| r.p.text == "Two"))
            .collect();
        let rect = on_screen(&host, twos[1]);
        host.tap(rect.left + 5.0, rect.center_y());
        run(&mut host, 500.0);
        assert_eq!(host.ui.state.looks.last, "Default radio: Two");
        showing(&host, "Last: Default radio: Two");
    }

    #[test]
    fn shell_page_opens_popups_toasts_and_navigates_its_tabs() {
        let mut host = host(1400);
        open_page(&mut host, "Shell");
        showing(&host, "Route=shell · NavigationStack=[shell] · Popups=0 · Modals=0 · Toasts=0 ");
        tap_on_page(&mut host, "Open popup");
        run(&mut host, 600.0);
        showing(&host, "Route=shell · NavigationStack=[shell] · Popups=1 · Modals=0 · Toasts=0 · popup opened");
        shot(&mut host, "Shell popup");
        tap_no_settle(&mut host, "Close");
        run(&mut host, 600.0);
        tap_on_page(&mut host, "ShowToast('Saved!')");
        run(&mut host, 500.0);
        showing(&host, "Route=shell · NavigationStack=[shell] · Popups=0 · Modals=0 · Toasts=1 · popup opened");
        shot(&mut host, "Shell toast");
        run(&mut host, 5000.0);
        // The nested shell: a detail page pushed inside the Home tab, with arguments.
        tap_on_page(&mut host, "GoToAsync('detail?id=7')");
        run(&mut host, 600.0);
        showing(&host, "Detail id=7");
        showing(&host, "Tab 0 · stack [detail?id=7] · Arguments {\"id\":\"7\"}");
        let events = &host.ui.state.shell_page.events;
        assert!(events.iter().any(|e| e == "Navigated Push 'detail?id=7' view=SkiaLayout"), "{events:?}");
        // The app shell: another page over this one, then back.
        tap_on_page(&mut host, "GoToAsync('shapes')");
        run(&mut host, 600.0);
        assert_eq!(host.ui.state.route, Some("shapes"));
        host.ui.tree.cx().go_back(host.ui.state.shell, true);
        run(&mut host, 600.0);
        assert_eq!(host.ui.state.route, Some("shell"));
    }

    #[test]
    fn shapes_context_menu_shows_a_toast() {
        let mut host = host(1000);
        open_page(&mut host, "Shapes");
        let rect = on_screen(&host, showing(&host, "right-click me"));
        let scale = 1.0;
        assert!(host.ui.context_menu(rect.center_x() * scale, rect.center_y() * scale, ContextMenuSource::Mouse, host.time_ms()));
        run(&mut host, 500.0);
        let toasts = host.ui.tree.find::<SkiaShell>(host.ui.state.shell).unwrap().toasts_count();
        assert_eq!(toasts, 1);
        shot(&mut host, "Shapes context menu toast");
    }

    #[test]
    fn catalog_takes_one_column_on_a_phone() {
        let two = host(700);
        let first = on_screen(&two, showing(&two, "Recycled cells"));
        let second = on_screen(&two, showing(&two, "Uneven cells"));
        assert_eq!(first.top, second.top, "side by side");
        let one = host_sized(390, 800);
        assert_eq!(one.ui.state.columns, 1);
        let first = on_screen(&one, showing(&one, "Recycled cells"));
        let second = on_screen(&one, showing(&one, "Uneven cells"));
        assert!(second.top > first.bottom, "one under the other");
    }

    #[test]
    fn snapping_carousels_and_drawer_follow_the_buttons() {
        let mut host = host(1400);
        open_page(&mut host, "Carousel & Drawer");
        tap_on_page(&mut host, "Next →");
        run(&mut host, 1500.0);
        assert_eq!(host.ui.state.snapping.index, 1);
        tap_on_page(&mut host, "Set index 3");
        run(&mut host, 1500.0);
        assert_eq!(host.ui.state.snapping.index, 3);
        let status = all(&host)
            .into_iter()
            .filter_map(|id| host.ui.tree.find::<SkiaLabel>(id).map(|l| l.p.text.clone()))
            .find(|text| text.starts_with("Selected Index: "))
            .unwrap();
        assert!(status.starts_with("Selected Index: 3   ·   InTransition: false   ·   Looping disabled - bounded scroll   ·   Item"), "{status}");
        // The looped one wraps from the first slide to the last.
        let prevs: Vec<ControlId> = all(&host)
            .into_iter()
            .filter(|id| host.ui.tree.find::<SkiaButton>(*id).is_some_and(|b| b.p.text == "Prev"))
            .collect();
        let rect = on_screen(&host, prevs[0]);
        host.tap(rect.center_x(), rect.center_y());
        run(&mut host, 1500.0);
        assert_eq!(host.ui.state.snapping.loop_index, 11);
        shot(&mut host, "Carousel & Drawer moved");
        tap_on_page(&mut host, "Open drawer");
        run(&mut host, 1500.0);
        assert!(host.ui.state.snapping.open);
        showing(&host, "IsOpen: true");
        shot(&mut host, "Carousel & Drawer open");
    }

    /// The hello-app rule: nothing inside a card is cut at any width from 360 to 980 points. Run it
    /// before a publish: `cargo test --release -p hellorust -- --ignored nothing_is_cut`. Carousels,
    /// horizontal scrolls and the drawer are wider on purpose; the "SkiaRow ItemsSource" card shows
    /// a row of chips, which a phone cuts at its fifth.
    #[test]
    #[ignore]
    fn nothing_is_cut_at_phone_and_desktop_width() {
        fn walk(host: &Headless<App>, id: ControlId, width: f32, card: Option<Rect>, out: &mut Vec<String>) {
            let tree = &host.ui.tree;
            let Some(base) = tree.base(id) else { return };
            if !base.p.is_visible {
                return;
            }
            let wide = tree.find::<SkiaScroll>(id).is_some_and(|s| s.p.orientation != ScrollOrientation::Vertical)
                || tree.find::<SkiaCarousel>(id).is_some()
                || tree.find::<SkiaShaderCarousel>(id).is_some()
                || tree.find::<SkiaDrawer>(id).is_some()
                || tree.find::<SkiaLayout>(id).is_some_and(|l| l.items_count() > 0 && l.p.layout_type == LayoutType::Row);
            let r = on_screen(host, id);
            if r.width() > 0.5 && r.height() > 0.5 {
                let name = tree.find::<SkiaLabel>(id).map(|l| format!("label {:?}", l.p.text.chars().take(40).collect::<String>()))
                    .or_else(|| tree.find::<SkiaButton>(id).map(|b| format!("button {:?}", b.p.text)))
                    .unwrap_or_else(|| format!("{id:?}"));
                if r.right > width + 0.5 || r.left < -0.5 {
                    out.push(format!("past the window: {name} {r:?}"));
                } else if let Some(c) = card
                    && (r.right > c.right + 0.5 || r.left < c.left - 0.5)
                {
                    out.push(format!("past its card: {name} {r:?} card {c:?}"));
                }
            }
            if wide {
                return;
            }
            let is_card = tree.find::<SkiaShape>(id).is_some() && base.p.background_color == Some(hex(0x2B3035));
            let card = if is_card { Some(r) } else { card };
            for &child in tree.children(id) {
                walk(host, child, width, card, out);
            }
        }
        for width in [360, 980] {
            for sample in SAMPLES.iter() {
                let mut host = host_sized(width, 3200);
                open_page(&mut host, sample.title);
                run(&mut host, 1500.0);
                let mut out = Vec::new();
                walk(&host, host.ui.tree.root().unwrap(), width as f32, None, &mut out);
                assert!(out.is_empty(), "{} at {width}: {}", sample.route, out.join(" | "));
            }
        }
    }

    #[test]
    fn shader_carousel_and_composite_spinner_move() {
        let mut host = host(1200);
        open_page(&mut host, "Shaders");
        // The first slide shows when the page opens: its photo, not the empty card.
        let carousel = all(&host).into_iter().find(|id| host.ui.tree.find::<SkiaShaderCarousel>(*id).is_some()).unwrap();
        let rect = on_screen(&host, carousel);
        let mut colors = Vec::new();
        for (i, j) in (1..4).flat_map(|i| (1..4).map(move |j| (i, j))) {
            let (x, y) = (rect.left + rect.width() * i as f32 / 4.0, rect.top + rect.height() * j as f32 / 4.0);
            let color = host.pixel(x as i32, y as i32);
            if !colors.contains(&color) {
                colors.push(color);
            }
        }
        assert!(colors.len() > 3, "the carousel shows {} colors", colors.len());
        tap_on_page(&mut host, "Next ›");
        run(&mut host, 1500.0);
        let from_to = &host.ui.state.shaders.from_to;
        assert!(from_to.starts_with("· ") && from_to.contains(" → "), "{from_to}");
        tap_on_page(&mut host, "swirl");
        run(&mut host, 200.0);
        assert_eq!(host.ui.state.shaders.transition, "swirl");

        let mut host = host_sized(1000, 900);
        open_page(&mut host, "Layouts");
        // Only a layer that is drawn records.
        into_view(&mut host, "Caching · UseCache=ImageComposite");
        let spinner = host.ui.state.layouts.spinner;
        let before = host.ui.tree.base(spinner).unwrap().p.rotation;
        run(&mut host, 400.0);
        let after = host.ui.tree.base(spinner).unwrap().p.rotation;
        // Degrees turned, through 360.
        assert!((after - before).rem_euclid(360.0) > 30.0, "{before} -> {after}");
        // The composite layer draws again only the spinner and what its bounds overlap.
        let page = &host.ui.state.layouts;
        assert!(page.info.starts_with("last record: partial · "), "{}", page.info);
        assert!(page.info.ends_with(" of 25 children redrawn"), "{}", page.info);
        assert!(page.redrawn.contains(&24) && page.redrawn.len() < 25, "{:?}", page.redrawn);
        shot(&mut host, "Layouts composite");
    }

    #[test]
    fn pong_serves_moves_the_paddle_and_plays() {
        let mut host = host(900);
        open_page(&mut host, "Pong");
        assert_eq!(host.ui.state.pong.phase, pong::Phase::WaitingToStart);
        showing(&host, "TAP TO SERVE");
        // The arrows move the player's paddle; the game hears them without focus.
        let start = host.ui.state.pong.player_left;
        host.ui.key(KeyKind::Down, "ArrowLeft", "", Modifiers::default(), false);
        run(&mut host, 200.0);
        host.ui.key(KeyKind::Up, "ArrowLeft", "", Modifiers::default(), false);
        host.frame_after(16.0);
        let moved = start - host.ui.state.pong.player_left;
        assert!(moved > 50.0, "moved {moved}");
        // Space serves: the ball flies until someone scores.
        host.ui.key(KeyKind::Down, "Space", "", Modifiers::default(), false);
        host.frame_after(16.0);
        assert_eq!(host.ui.state.pong.phase, pong::Phase::Playing);
        shot(&mut host, "Pong playing");
        let mut scored = false;
        for _ in 0..60 {
            run(&mut host, 500.0);
            let page = &host.ui.state.pong;
            if page.player_score + page.ai_score > 0 {
                scored = true;
                break;
            }
        }
        assert!(scored, "nobody scored in 30 s");
        shot(&mut host, "Pong scored");
    }

    /// Types text into the focused control, a key and its character at a time.
    fn type_text(host: &mut Headless<App>, text: &str) {
        for c in text.chars() {
            let mut buffer = [0u8; 4];
            host.ui.key(KeyKind::Char, "", c.encode_utf8(&mut buffer), Modifiers::default(), false);
        }
        host.frame_after(16.0);
    }

    #[test]
    fn editor_types_submits_and_chats() {
        let mut host = host(1400);
        open_page(&mut host, "Editor");
        tap_on_page(&mut host, "Focus");
        run(&mut host, 300.0);
        assert!(host.ui.state.editor.focused);
        type_text(&mut host, "hi 🙂");
        assert_eq!(host.ui.state.editor.text, "hi 🙂");
        host.ui.key(KeyKind::Down, "Enter", "", Modifiers::default(), false);
        host.frame_after(16.0);
        assert_eq!(host.ui.state.editor.submitted, "hi 🙂");
        showing(&host, "Single line — Text=\"hi 🙂\" · IsFocused=true · cursor 4 · selection 0 · 4 chars · submitted: \"hi 🙂\"");
        shot(&mut host, "Editor typed");
        tap_on_page(&mut host, "Set Text");
        run(&mut host, 300.0);
        assert_eq!(host.ui.state.editor.text, "Hello from code");

        // The chat input: a tap focuses it, Enter sends and clears it.
        let chat = all(&host).into_iter().find(|id| host.ui.tree.find::<SkiaEditor>(*id).is_some_and(|e| e.p.placeholder_text == "Message")).unwrap();
        into_view(&mut host, "Multiline + AutoHeight — MaxLines={-1}: the editor grows with the text");
        // A focused editor blinks its caret: taps without settle.
        let rect = on_screen(&host, chat);
        let now = host.time_ms();
        host.ui.pointer(drawnui::PointerKind::Down, rect.center_x(), rect.center_y(), now);
        host.ui.pointer(drawnui::PointerKind::Up, rect.center_x(), rect.center_y(), now);
        run(&mut host, 300.0);
        type_text(&mut host, "yo");
        host.ui.key(KeyKind::Down, "Enter", "", Modifiers::default(), false);
        run(&mut host, 300.0);
        assert_eq!(host.ui.state.editor.chat, ["yo"]);
        assert_eq!(host.ui.tree.find::<SkiaEditor>(chat).unwrap().p.text, "");
        showing(&host, "yo");
        shot(&mut host, "Editor chat");
    }

    #[test]
    fn every_page_opens_scrolls_and_goes_back() {
        let mut host = host(700);
        shot(&mut host, "catalog");
        for sample in &SAMPLES {
            // The catalog scrolls: the card is brought into view first.
            scroll_into_view(&mut host, sample.title);
            // Some pages animate as long as they are open: no settle.
            tap_no_settle(&mut host, sample.title);
            deliver(&mut host);
            assert_eq!(host.ui.state.route, Some(sample.route), "{} did not open", sample.title);
            shot(&mut host, sample.title);
            // Every page is a scroll; one shorter than the window leaves the wheel alone (React).
            host.wheel(500.0, 400.0, -2.0);
            deliver(&mut host);
            shot(&mut host, &format!("{} wheel", sample.title));
            // For looking at a whole page: three more screens of it.
            if std::env::var("HELLORUST_SHOTS").is_ok() {
                for part in 2..5 {
                    host.wheel(500.0, 400.0, -4.0);
                    deliver(&mut host);
                    shot(&mut host, &format!("{} part {part}", sample.title));
                }
            }
            // Home is in the nav bar, drawn over the page: the last control that shows "Home" (the
            // Shell page has a Home tab too). A plain tap, then the catalog settles.
            let home = all(&host)
                .into_iter()
                .filter(|id| host.ui.tree.find::<SkiaButton>(*id).is_some_and(|b| b.p.text == "Home"))
                .last()
                .expect("the Home button");
            let rect = on_screen(&host, home);
            let now = host.time_ms();
            host.ui.pointer(drawnui::PointerKind::Down, rect.center_x(), rect.center_y(), now);
            host.ui.pointer(drawnui::PointerKind::Up, rect.center_x(), rect.center_y(), now);
            host.settle();
            assert!(host.ui.state.route.is_none());
            // Nothing is pending once the catalog is back: the app draws on demand.
            host.frame_after(16.0);
            assert!(!host.ui.needs_frame());
        }
    }

    #[test]
    fn cells_tap_the_row_under_the_pointer_and_jump() {
        let mut host = host(700);
        open_page(&mut host, "Recycled cells");
        let (list, point) = (host.ui.state.cells.feed, (500.0, 400.0));
        let visible_rows = |host: &Headless<App>| host.ui.tree.find::<SkiaLayout>(list).unwrap().visible_items().unwrap();

        // A tap, then the wheel, then a fling: each time the tapped item is the row under the pointer.
        let first = tap_row(&mut host, list, point.0, point.1);
        assert_eq!(host.ui.state.cells.last_tapped, Some(first + 1));
        assert!(host.wheel(point.0, point.1, -3.0));
        host.settle();
        let after_wheel = tap_row(&mut host, list, point.0, point.1);
        assert!(after_wheel > 5, "the wheel scrolled the list");
        assert_eq!(host.ui.state.cells.last_tapped, Some(after_wheel + 1));
        host.fling((500.0, 600.0), (500.0, 200.0), 120.0, 6);
        let after_fling = tap_row(&mut host, list, point.0, point.1);
        assert!(after_fling > after_wheel + 10, "the release flung the list");
        assert_eq!(host.ui.state.cells.last_tapped, Some(after_fling + 1));
        // The debug line follows the scroll.
        let debug = &host.ui.state.cells.debug;
        assert!(debug.starts_with("items 100000 visible ") && debug.ends_with(" fps"), "{debug}");

        tap(&mut host, "END");
        assert_eq!(visible_rows(&host).1, 99_999);
        tap(&mut host, "MIDDLE");
        assert_eq!(visible_rows(&host).0, 50_000);
        tap(&mut host, "FORWARD");
        assert_eq!(visible_rows(&host).0, 50_005);
        tap(&mut host, "HOME");
        assert_eq!(visible_rows(&host).0, 0);
        // A handful of cells serves all 100 000 rows.
        assert!(host.ui.tree.children(list).len() < 60, "{} cells", host.ui.tree.children(list).len());
    }

    #[test]
    fn uneven_items_are_the_csharp_ones_and_the_end_loads_more() {
        // Reference values printed by the C# MakeItem of UnevenCellsPage.cs.
        let first = uneven::make_item(1);
        assert_eq!(first.body, "Renders idle ui product idle pixel drawn ui product recycled time canvas idle itself itself catalog renders renders feed in catalog pixel.");
        assert_eq!(first.color, hex(0x6610F2));
        assert_eq!((uneven::make_item(200).body.len(), uneven::make_item(-29).body.len()), (263, 269));
        assert!(uneven::make_item(-29).body.starts_with("Timeline every idle measure itself"));

        let mut host = host(700);
        open_page(&mut host, "Uneven cells");
        // LoadMore is looked at when the list scrolls (React OnScrolled): the page opens with its 200.
        assert_eq!(host.ui.state.uneven.items.len(), 200);
        showing(&host, "200 uneven cells · MeasureVisible · LoadMore at both ends");
        showing(&host, "Post 1");
        // The end is within LoadMoreOffset: the next page of 100 arrives 400 ms later.
        tap(&mut host, "END");
        assert_eq!(host.ui.state.uneven.items.len(), 300);
        showing(&host, "300 uneven cells · MeasureVisible · LoadMore at both ends");
        // The jump landed on the last post there was; the new page is below it.
        showing(&host, "Post 200");
        shot(&mut host, "Uneven cells end");
        // Back at the start, inside LoadMoreTopOffset: 30 older posts arrive above, and the first
        // post stays where it was.
        tap(&mut host, "HOME");
        assert_eq!(host.ui.state.uneven.items.len(), 330);
        showing(&host, "Post 1");
    }

    #[test]
    fn scroll_page_inner_scrolls_move_track_and_refresh() {
        // Tall enough to show all seven cards.
        let mut host = host(2300);
        open_page(&mut host, "SkiaScroll");
        deliver(&mut host);
        // The page scroll first, then the eight scrolls of the cards.
        let scrolls: Vec<ControlId> = all(&host).into_iter().filter(|id| host.ui.tree.find::<SkiaScroll>(*id).is_some()).collect();
        assert_eq!(scrolls.len(), 9);
        let opened = (host.ui.state.scroll.snap_index, host.ui.state.scroll.track_index);
        for &scroll in &scrolls[1..] {
            host.ui.tree.cx().scroll_to(scroll, -212.0, -60.0, 0);
        }
        // One frame, not settled: the scroll bars are still showing.
        host.frame();
        let offsets: Vec<(f32, f32)> = scrolls[1..]
            .iter()
            .map(|id| host.ui.tree.find::<SkiaScroll>(*id).map(|s| (s.viewport_offset_x(), s.viewport_offset_y())).unwrap())
            .collect();
        let expected = [(0.0, -60.0), (0.0, -60.0), (0.0, -60.0), (0.0, -60.0), (-212.0, 0.0), (0.0, -60.0), (-212.0, 0.0), (0.0, -60.0)];
        assert_eq!(offsets, expected);
        // The strip moved by one tile: the next one is under the center of its viewport. The
        // list moved by 60 points: the second row is at its start. The titles follow.
        assert_eq!(opened, (None, None));
        assert_eq!((host.ui.state.scroll.snap_index, host.ui.state.scroll.track_index), (Some(2), Some(1)));
        host.frame();
        showing(&host, "SnapToChildren=\"Center\" + TrackIndexPosition=\"Center\" (horizontal) · CurrentIndex 2");
        showing(&host, "TrackIndexPosition=\"Start\" (vertical) · CurrentIndex 1 · SnapToChildren=\"Side\"");

        // IsRefreshing set by the app starts a refresh (the page scroll takes a pull); two seconds
        // later the handler ends it.
        let refreshing = |host: &Headless<App>| host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll.refresh_scroll).unwrap().p.is_refreshing;
        let refresh_scroll = host.ui.state.scroll.refresh_scroll;
        host.ui.tree.get_mut(refresh_scroll).unwrap().set_is_refreshing(true);
        host.frame();
        host.frame_after(16.0);
        assert!(refreshing(&host));
        assert_eq!(host.ui.state.scroll.refresh_state, "refreshing… (2 s)");
        // The content goes down to show the indicator and stays there.
        host.frame_after(600.0);
        shot(&mut host, "SkiaScroll inner scrolled");
        host.frame_after(2100.0);
        host.settle();
        assert!(!refreshing(&host));
        assert_eq!(host.ui.state.scroll.refresh_state, "done · pull again");
        // Nothing is pending after it.
        host.frame_after(16.0);
        assert!(!host.ui.needs_frame());
    }

    #[test]
    fn transforms_taps_count_and_animations_run_to_their_end() {
        // Tall enough to show the whole page.
        let mut host = host(1300);
        open_page(&mut host, "Transforms");
        // The button is rotated and scaled; its layout box center is still inside the drawn one.
        tap(&mut host, "Tapped 0×");
        tap(&mut host, "Tapped 1×");
        assert_eq!(host.ui.state.transforms.taps, 2);

        let logo = host.ui.state.transforms.logo;
        let props = |host: &Headless<App>| host.ui.tree.base(logo).unwrap().p.clone();
        // Fade: down to 0.15 and back, the second half started by on_finished.
        tap_no_settle(&mut host, "Fade");
        host.frame_after(300.0);
        assert_eq!(props(&host).opacity, 0.15);
        host.settle();
        assert_eq!(props(&host).opacity, 1.0);

        // Spin runs until stopped and asks for frames all the while.
        tap_no_settle(&mut host, "Spin");
        host.frame_after(300.0);
        assert_eq!(props(&host).rotation, 90.0);
        assert!(host.ui.needs_frame());
        tap_no_settle(&mut host, "Stop spin");
        host.frame_after(600.0); // the ripple of the button ends
        host.frame_after(16.0);
        assert!(!host.ui.needs_frame());
        assert!(host.ui.state.transforms.spin.is_none());
    }

    #[test]
    fn accessibility_paragraph_selects_a_word_with_a_double_click_and_copies_it() {
        let mut host = host(1600);
        open_page(&mut host, "Accessibility");
        run(&mut host, 1000.0);
        let paragraph = |host: &Headless<App>| {
            let tree = &host.ui.tree;
            all(host).into_iter().find(|id| tree.find::<SkiaLabel>(*id).is_some_and(|l| l.p.text.starts_with("This paragraph is drawn"))).expect("the selectable paragraph")
        };
        let label = paragraph(&host);
        let rect = on_screen(&host, label);
        let (x, y) = (rect.left + 40.0, rect.top + 9.0);
        let before = host.pixel(x as i32 + 8, y as i32 - 6);
        for _ in 0..2 {
            let now = host.time_ms();
            host.ui.pointer(drawnui::PointerKind::Down, x, y, now);
            host.frame_after(16.0);
            host.ui.pointer(drawnui::PointerKind::Up, x, y, host.time_ms());
            host.frame_after(16.0);
        }
        assert_eq!(host.ui.tree.find::<SkiaLabel>(label).unwrap().selected_text(), "paragraph");
        let after = host.pixel(x as i32 + 8, y as i32 - 6);
        assert!(after.b() > before.b() + 20, "the highlight is drawn over the word: {before:?} then {after:?}");
        let ctrl = Modifiers { ctrl: true, ..Modifiers::default() };
        host.ui.key(KeyKind::Down, "KeyC", "", ctrl, false);
        host.frame_after(16.0);
        assert_eq!(host.take_clipboard().as_deref(), Some("paragraph"));
    }

    /// The label of the node the keyboard is on.
    fn keyboard_on(host: &Headless<App>) -> Option<String> {
        let focused = host.ui.accessibility_focused()?;
        host.ui.accessibility_nodes().iter().find(|n| n.control == focused).map(|n| n.label.clone())
    }

    /// Tab (Shift+Tab with `back`) until the keyboard is on the node with this label.
    fn tab_to(host: &mut Headless<App>, label: &str) {
        for _ in 0..200 {
            host.ui.key(KeyKind::Down, "Tab", "", Modifiers::default(), false);
            run(host, 400.0);
            if keyboard_on(host).as_deref() == Some(label) {
                return;
            }
        }
        panic!("Tab never reached \"{label}\"");
    }

    fn key_on(host: &mut Headless<App>, key: &'static str) -> bool {
        let used = host.ui.key(KeyKind::Down, key, "", Modifiers::default(), false);
        host.ui.key(KeyKind::Up, key, "", Modifiers::default(), false);
        run(host, 400.0);
        used
    }

    fn keyboard_host() -> Headless<App> {
        let mut host = host(1000);
        host.ui.set_keyboard_navigation(true);
        host.ui.set_accessibility_enabled(true);
        run(&mut host, 1200.0);
        host
    }

    /// Pixels of the focus ring color on the ring's left stroke around a rect.
    fn ring_pixels(host: &mut Headless<App>, rect: Rect) -> usize {
        let x = (rect.left - 3.0) as i32;
        ((rect.top + 8.0) as i32..(rect.bottom - 8.0) as i32).filter(|y| host.pixel(x, *y) == hex(0x6EA8FE)).count()
    }

    /// A screen reader's ScrollIntoView brings the last slider of Common Controls on screen in one
    /// call.
    #[test]
    fn scroll_into_view_reaches_the_last_slider_in_one_call() {
        // A desktop window at 150 %.
        let mut host = unsettled_at(1227, 1033, 1.5);
        host.settle();
        host.ui.set_accessibility_enabled(true);
        open_page(&mut host, "Common Controls");
        run(&mut host, 1200.0);
        // The last slider the snapshot has (a screen past the viewport at most), as a screen
        // reader sees it.
        let last = host.ui.accessibility_nodes().iter().filter(|n| n.role == Aria::SLIDER).last().expect("a slider");
        let (node, last) = (last.id, last.control);
        assert!(on_screen(&host, last).bottom > 1033.0, "already on screen");
        host.ui.accessibility_scroll_into_view(node);
        run(&mut host, 1200.0);
        let rect = on_screen(&host, last);
        assert!(rect.top >= 0.0 && rect.bottom <= 1033.0, "{rect:?}");
        run(&mut host, 3000.0);
        // The snapshot follows (points).
        let read = host.ui.accessibility_nodes().iter().find(|n| n.id == node).expect("still there").rect;
        assert!(read.top >= 0.0 && read.bottom * 1.5 <= 1033.0, "{read:?}");
    }

    /// A page pushed from the catalog starts its work once it slid in, not during the slide.
    #[test]
    fn a_page_pushed_from_the_catalog_starts_its_work_after_the_slide() {
        let mut host = host(900);
        scroll_into_view(&mut host, "Shaders");
        tap_no_settle(&mut host, "Shaders");
        host.frame_after(16.0);
        host.frame_after(16.0);
        assert_eq!(host.ui.state.route, Some("shaders"));
        assert!(host.ui.state.shaders.running.is_empty(), "nothing runs while the page slides in");
        run(&mut host, 600.0);
        assert!(!host.ui.state.shaders.running.is_empty(), "the shaders run once it is in place");
    }

    /// A page opened from a link (the shell rebuilds the stack from the URL and reports no
    /// navigation) starts what it runs, as one opened from the catalog.
    #[test]
    fn pages_opened_from_a_link_start_their_work() {
        let opened = |hash: &str| {
            // As the browser opens the page: the history is on before the first frame.
            let mut host = unsettled(1000, 1000);
            host.ui.set_history_enabled(true);
            host.ui.set_accessibility_enabled(true);
            drawnui::App::location(&mut host.ui, hash, None);
            run(&mut host, 1500.0);
            host
        };
        let host = opened("#/a11y");
        assert_eq!(host.ui.state.route, Some("a11y"));
        assert!(host.ui.state.a11y.nodes > 0, "the live status counts the nodes");
        assert!(opened("#/images").ui.state.images.offset > 0.0, "the tiles move");
        assert!(opened("#/layouts").ui.state.layouts.angle > 0.0, "the spinner turns");
        assert!(!opened("#/shaders").ui.state.shaders.running.is_empty(), "the shaders run");
    }

    #[test]
    fn a_card_opened_with_enter_leaves_no_ring_over_the_new_page() {
        let mut host = keyboard_host();
        tab_to(&mut host, "Accessibility");
        let card = host.ui.accessibility_focused().unwrap();
        let rect = on_screen(&host, card);
        assert!(ring_pixels(&mut host, rect) > 0, "the ring around the card the keyboard is on");
        // As the desktop host: a frame for the key, then frames only while something animates or
        // a timer is due (the snapshot rebuild is one), none after.
        host.ui.key(KeyKind::Down, "Enter", "", Modifiers::default(), false);
        host.frame_after(16.0);
        let shown = |host: &Headless<App>| {
            let mut at = Some(card);
            while let Some(id) = at {
                if !host.ui.tree.base(id).is_some_and(|b| b.p.is_visible) {
                    return false;
                }
                at = host.ui.tree.parent(id);
            }
            true
        };
        let (end, mut hidden_frames) = (host.time_ms() + 2000.0, 0);
        while host.time_ms() < end {
            match (host.ui.needs_frame(), host.ui.wake_at()) {
                (true, _) => host.frame_after(16.0),
                (false, Some(wake)) => host.frame_after((wake - host.time_ms()).max(1.0)),
                (false, None) => break,
            }
            // Once the page under the new one is hidden, no frame draws the ring where the card was.
            if !shown(&host) {
                hidden_frames += 1;
                assert_eq!(ring_pixels(&mut host, rect), 0, "a ring where the card was, over the page that opened");
            }
        }
        assert_eq!(host.ui.state.route, Some("a11y"));
        assert!(hidden_frames > 0, "the catalog was hidden");
        host.ui.key(KeyKind::Down, "Tab", "", Modifiers::default(), false);
        run(&mut host, 400.0);
        let first = host.ui.accessibility_focused().expect("a node of the new page");
        assert_ne!(first, card, "Tab starts on the new page");
    }

    #[test]
    fn accessibility_page_groups_move_with_the_arrows() {
        let mut host = keyboard_host();
        tab_to(&mut host, "Accessibility");
        key_on(&mut host, "Enter");
        run(&mut host, 1500.0);

        // The toolbar: Left / Right.
        tab_to(&mut host, "Sound");
        assert!(key_on(&mut host, "ArrowRight"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("Dark mode"));
        assert!(!key_on(&mut host, "ArrowDown"), "a row leaves Up / Down");
        assert!(key_on(&mut host, "ArrowLeft"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("Sound"));

        // The list: Up / Down, End, no wrap; the toolbar was one stop.
        tab_to(&mut host, "Apple");
        assert!(key_on(&mut host, "ArrowDown"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("Banana"));
        assert!(key_on(&mut host, "End"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("Date"));
        assert!(key_on(&mut host, "ArrowDown"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("Date"));

        // The grid: all four arrows, Down by one row.
        tab_to(&mut host, "1");
        assert!(key_on(&mut host, "ArrowRight"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("2"));
        let two = host.rect(host.ui.accessibility_focused().unwrap());
        assert!(key_on(&mut host, "ArrowDown"));
        let below = host.rect(host.ui.accessibility_focused().unwrap());
        assert_eq!(below.center_x(), two.center_x(), "the same column");
        assert!(below.top > two.bottom, "the next row");
        assert!(key_on(&mut host, "ArrowUp"));
        assert_eq!(keyboard_on(&host).as_deref(), Some("2"));
        assert!(key_on(&mut host, "Enter"));
        assert_eq!(host.ui.state.a11y.last_activated, "2");
    }
}
