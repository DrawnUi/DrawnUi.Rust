//! SkiaShell: navigation, the page and tab transitions frame by frame, popups, modals, toasts.
//! Durations and curves are those of the React SkiaShell (`src/react/SkiaShell.tsx`): pages slide
//! linearly over 200 ms (TranslateToAsync, PagesAnimationSpeed), tabs over 150 ms with the C#
//! SkiaViewSwitcher `Custom` back-ease, popups fade from 0.1 and scale from 0.5 over 250 ms, a modal
//! is a SkiaDrawer (CubicInOut, 140 ms), toasts slide 300 ms in and 250 ms out.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use drawnui::{App as _, Base, HistoryOp, PointerKind};
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(|a| a.get())
}

#[derive(Default)]
struct App {
    shell: Handle<SkiaShell>,
    events: Vec<String>,
    /// The routes the factories built, with their arguments.
    built: Vec<String>,
    cancel_next: bool,
    /// On `Navigated` to this route, go on to "b" from inside the handler.
    chain_from: Option<&'static str>,
    /// Toasts counted by `on_changed`, after every change.
    toasts_seen: Vec<usize>,
}

type Host = Headless<App>;

const W: f32 = 400.0;
const H: f32 = 300.0;

fn page(color: Color) -> Build<SkiaLayout> {
    SkiaLayer::new().fill().background_color(color)
}

fn events(shell: Build<SkiaShell>) -> Build<SkiaShell> {
    shell
        .on_navigating(|_me, app: &mut App, _cx, e| {
            if std::mem::take(&mut app.cancel_next) {
                e.cancel = true;
            }
            let cancelled = if e.cancel { " CANCELLED" } else { "" };
            app.events.push(format!("Navigating {:?} '{}'{cancelled}", e.source, e.route));
        })
        .on_navigated(|me, app: &mut App, cx, e| {
            app.events.push(format!("Navigated {:?} '{}'", e.source, e.route));
            if app.chain_from.is_some_and(|r| r == e.route) {
                cx.go_to(me.id(), "b", false);
            }
        })
        .on_route_changed(|_me, app: &mut App, _cx, route| app.events.push(format!("RouteChanged '{route}'")))
        .on_changed(|me, app: &mut App, _cx| {
            if app.toasts_seen.last() != Some(&me.toasts_count()) {
                app.toasts_seen.push(me.toasts_count());
            }
        })
}

fn routes(shell: Build<SkiaShell>) -> Build<SkiaShell> {
    shell
        .route("a", |app: &mut App, args| {
            app.built.push(build_route("a", args));
            page(Color::RED)
        })
        .route("b", |app: &mut App, args| {
            app.built.push(build_route("b", args));
            page(Color::GREEN)
        })
        .titles([("a", "Page A")])
}

/// A 400 x 300 point shell with the root in blue.
fn pages() -> Host {
    host_of(|app| events(routes(SkiaShell::new().assign(&mut app.shell))).root(page(Color::BLUE)))
}

/// Tabs "a" (red root) and "b" (green root), sliding.
fn tabs() -> Host {
    host_of(|app| {
        let shell = SkiaShell::new().assign(&mut app.shell).animate_tabs(true).tabs([("a", "A"), ("b", "B")]);
        events(routes(shell).route("detail", |_app: &mut App, _| page(Color::WHITE)))
    })
}

fn host_of(build: impl FnOnce(&mut App) -> Build<SkiaShell>) -> Host {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, W as i32, H as i32, 1.0);
    host.settle();
    host
}

fn shell(host: &Host) -> &SkiaShell {
    host.ui.tree.find(host.ui.state.shell).unwrap()
}

fn stack(host: &Host) -> Vec<String> {
    shell(host).navigation_stack().map(str::to_owned).collect()
}

/// The layer of the tab shown `i`-th in paint order.
fn tab_layer(host: &Host, i: usize) -> ControlId {
    let pages = host.ui.tree.children(host.ui.state.shell)[0];
    host.ui.tree.children(pages)[i]
}

/// The root layer, then the page hosts of a tab layer.
fn tab_children(host: &Host, layer: ControlId) -> Vec<ControlId> {
    host.ui.tree.children(layer).to_vec()
}

fn base(host: &Host, id: ControlId) -> &Base {
    host.ui.tree.base(id).unwrap()
}

fn visible(host: &Host, id: ControlId) -> bool {
    base(host, id).p.is_visible
}

/// The layer that holds the popups, modals or toasts: 1, 2, 3 (after the pages, no tab bar).
fn overlays(host: &Host, layer: usize) -> Vec<ControlId> {
    let id = host.ui.tree.children(host.ui.state.shell)[layer];
    host.ui.tree.children(id).to_vec()
}

fn idle(host: &Host) -> bool {
    !host.ui.needs_frame() && host.ui.wake_at().is_none()
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.01
}

fn assert_frames(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(close(*a, *e), "frame {i}: {a} instead of {e} ({actual:?})");
    }
}

fn args(pairs: &[(&str, &str)]) -> ShellArguments {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn taken(host: &mut Host) -> Vec<String> {
    std::mem::take(&mut host.ui.state.events)
}

// ---------------------------------------------------------------- navigation

#[test]
fn routes_push_pop_and_report_in_the_react_order() {
    let mut host = pages();
    let id = host.ui.state.shell;
    // RouteChanged runs once at mount, as React's effect.
    assert_eq!(taken(&mut host), ["RouteChanged ''"]);
    assert_eq!(shell(&host).route(), "");
    assert!(!shell(&host).can_go_back());

    host.ui.tree.cx().go_to(id, "a", false);
    host.settle();
    assert_eq!(taken(&mut host), ["Navigating Push 'a'", "RouteChanged 'a'", "Navigated Push 'a'"]);
    assert_eq!(stack(&host), ["a"]);
    let layer = tab_layer(&host, 0);
    let [root, a] = tab_children(&host, layer)[..] else { panic!() };
    // Only the top shows.
    assert!(!visible(&host, root) && visible(&host, a));
    assert_eq!(host.pixel(200, 150), Color::RED);

    // A query and explicit arguments, which win over the query.
    host.ui.tree.cx().go_to_with(id, "b?id=7&note=two+words", false, args(&[("from", "a")]));
    host.settle();
    assert_eq!(shell(&host).route(), "b?id=7&note=two+words&from=a");
    assert_eq!(shell(&host).arguments(), args(&[("id", "7"), ("note", "two words"), ("from", "a")]));
    host.ui.tree.cx().go_to_with(id, "b?id=7", false, args(&[("id", "42")]));
    host.settle();
    assert_eq!(stack(&host), ["a", "b?id=7&note=two+words&from=a", "b?id=42"]);
    assert_eq!(host.ui.state.built, ["a", "b?id=7&note=two+words&from=a", "b?id=42"]);
    taken(&mut host);

    // Back: the one below shows again.
    host.ui.tree.cx().go_back(id, false);
    host.settle();
    assert_eq!(
        taken(&mut host),
        ["Navigating Pop 'b?id=7&note=two+words&from=a'", "RouteChanged 'b?id=7&note=two+words&from=a'", "Navigated Pop 'b?id=7&note=two+words&from=a'"]
    );
    assert_eq!(host.pixel(200, 150), Color::GREEN);

    // Home: every page goes at once, only RouteChanged is reported (React PopToRootAsync).
    host.ui.tree.cx().pop_to_root(id);
    host.settle();
    assert_eq!(taken(&mut host), ["RouteChanged ''"]);
    assert_eq!(stack(&host), Vec::<String>::new());
    assert_eq!(tab_children(&host, layer), [root]);
    assert!(visible(&host, root));
    assert_eq!(host.pixel(200, 150), Color::BLUE);

    // Navigating can cancel.
    host.ui.state.cancel_next = true;
    host.ui.tree.cx().go_to(id, "a", true);
    host.settle();
    assert_eq!(taken(&mut host), ["Navigating Push 'a' CANCELLED"]);
    assert_eq!(shell(&host).route(), "");
    // Back with nothing open does nothing.
    host.ui.tree.cx().go_back(id, true);
    host.settle();
    assert!(taken(&mut host).is_empty());
    assert!(idle(&host));
}

#[test]
fn a_handler_of_the_shell_navigates_it() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.state.chain_from = Some("a");
    host.ui.tree.cx().go_to(id, "a", false);
    host.settle();
    assert_eq!(stack(&host), ["a", "b"]);
}

#[test]
#[should_panic(expected = "route 'nope' is not registered")]
fn an_unknown_route_panics() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "nope?x=1", false);
    host.frame();
}

#[test]
fn route_strings_round_trip() {
    let arguments = args(&[("q", "a&b=c d%"), ("é", "1")]);
    let route = build_route("search", &arguments);
    assert_eq!(route, "search?q=a%26b%3Dc+d%25&%C3%A9=1");
    assert_eq!(split_route(&route), ("search", arguments));
    assert_eq!(split_route("plain"), ("plain", vec![]));
}

// ---------------------------------------------------------------- transitions

/// Frames 16 ms apart until nothing is pending: `read` after every frame, up to its last change.
fn trace(host: &mut Host, read: impl Fn(&Host) -> f32) -> Vec<f32> {
    let mut out = Vec::new();
    while host.ui.needs_frame() && out.len() < 200 {
        host.frame_after(16.0);
        out.push(read(host));
    }
    while out.len() > 1 && out[out.len() - 1] == out[out.len() - 2] {
        out.pop();
    }
    out
}

#[test]
fn a_page_slides_in_and_out_linearly_over_200_ms() {
    let mut host = pages();
    let id = host.ui.state.shell;
    taken(&mut host);
    host.ui.tree.cx().go_to(id, "a", true);
    // The frame of the request builds the page, off screen at the width of the shell.
    host.frame();
    let layer = tab_layer(&host, 0);
    let [root, a] = tab_children(&host, layer)[..] else { panic!() };
    assert_eq!(base(&host, a).p.translation_x, W);
    assert_eq!(taken(&mut host), ["Navigating Push 'a'", "RouteChanged 'a'"]);
    assert!(visible(&host, root));

    // React: TranslationX = width, then TranslateToAsync(0, 0, 200), linear: x = 400 (1 - t / 200).
    let x = trace(&mut host, |h| h.ui.tree.base(a).unwrap().p.translation_x);
    let expected: Vec<f32> = (0..=12).map(|i| W * (1.0 - 16.0 * i as f32 / 200.0)).chain([0.0]).collect();
    assert_frames(&x, &expected);
    // The frame after the end: the root is hidden, Navigated is reported.
    assert!(!visible(&host, root));
    assert_eq!(taken(&mut host), ["Navigated Push 'a'"]);
    assert!(idle(&host));

    // Halfway out, the root shows on the left of the page.
    host.ui.tree.cx().go_back(id, true);
    host.frame();
    assert!(visible(&host, root));
    let mut x = Vec::new();
    for _ in 0..7 {
        host.frame_after(16.0);
        x.push(base(&host, a).p.translation_x);
    }
    // x = 400 t / 200.
    assert_frames(&x, &[0.0, 32.0, 64.0, 96.0, 128.0, 160.0, 192.0]);
    assert_eq!(host.pixel(150, 150), Color::BLUE);
    assert_eq!(host.pixel(300, 150), Color::RED);
    // The stack keeps the page until it is out, as React.
    assert_eq!(shell(&host).route(), "a");
    host.settle();
    assert_eq!(tab_children(&host, layer), [root]);
    assert_eq!(taken(&mut host), ["Navigating Pop ''", "RouteChanged ''", "Navigated Pop ''"]);
    assert!(idle(&host));
}

#[test]
fn back_while_a_page_slides_in_turns_it_around() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", true);
    host.frame();
    for _ in 0..6 {
        host.frame_after(16.0);
    }
    let a = *tab_children(&host, tab_layer(&host, 0)).last().unwrap();
    let at = base(&host, a).p.translation_x;
    assert!(close(at, 240.0), "{at}");
    host.ui.tree.cx().go_back(id, true);
    host.frame();
    host.frame_after(16.0);
    // From where it stood, out to the right.
    assert!(close(base(&host, a).p.translation_x, 240.0));
    host.frame_after(16.0);
    assert!(close(base(&host, a).p.translation_x, 240.0 + 160.0 * 0.08));
    host.settle();
    assert_eq!(shell(&host).route(), "");
    assert_eq!(tab_children(&host, tab_layer(&host, 0)).len(), 1);
    assert!(idle(&host));
}

#[test]
fn transition_frames_allocate_nothing() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", true);
    host.frame();
    host.frame_after(16.0);
    let before = allocations();
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    assert_eq!(allocations() - before, 0, "page slide");
    host.settle();

    host.ui.tree.cx().open_popup(id, SkiaShape::new().width_request(200).height_request(100), PopupOptions::default());
    host.frame();
    host.frame_after(16.0);
    let before = allocations();
    for _ in 0..10 {
        host.frame_after(16.0);
    }
    assert_eq!(allocations() - before, 0, "popup");
    host.settle();

    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), ModalOptions::default());
    host.frame();
    host.frame_after(16.0);
    host.frame_after(16.0);
    let before = allocations();
    // The drawer moves for 140 ms: the frames before its end.
    for _ in 0..5 {
        host.frame_after(16.0);
    }
    assert_eq!(allocations() - before, 0, "modal");
    host.settle();

    let mut host = tabs();
    let id = host.ui.state.shell;
    host.ui.tree.cx().select_tab(id, 1);
    host.frame();
    host.frame_after(16.0);
    let before = allocations();
    for _ in 0..6 {
        host.frame_after(16.0);
    }
    assert_eq!(allocations() - before, 0, "tab switch");
    host.settle();
}

/// A page of 40 labels in a column, in a 400 x 700 point window at scale 2.
fn heavy() -> Host {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let rows = || (0..40).map(|i| SkiaLabel::new(format!("Row {i} of a page that slides")).font_size(16).text_color(Color::WHITE)).collect::<Vec<_>>();
        SkiaShell::new()
            .assign(&mut app.shell)
            .route("page", move |_app: &mut App, _| SkiaStack::new().padding(16).children(rows()))
            .root(SkiaStack::new().padding(16).children(rows()))
    })
    .font_bytes("Default", FONT);
    let mut host = Headless::new(ui, 800, 1400, 2.0);
    host.settle();
    host
}

#[test]
#[ignore = "a measurement; run it in release with --nocapture"]
fn frame_time_of_transitions() {
    const RUNS: u32 = 20;
    let mut host = heavy();
    let id = host.ui.state.shell;
    let measure = |host: &mut Host, what: &str, start: &dyn Fn(&mut Host), end: &dyn Fn(&mut Host)| {
        let (mut total, mut frames) = (0.0, 0);
        for _ in 0..RUNS {
            start(host);
            host.frame();
            let t = std::time::Instant::now();
            while host.ui.needs_frame() {
                host.frame_after(16.0);
                frames += 1;
            }
            total += t.elapsed().as_secs_f64();
            end(host);
            host.settle();
        }
        println!("{what}: {:.0} us per frame ({} frames a run)", total * 1e6 / frames as f64, frames / RUNS);
    };
    let still = std::time::Instant::now();
    for _ in 0..100 {
        host.ui.tree.invalidate(id, Dirty::REPAINT);
        host.frame_after(16.0);
    }
    println!("a frame of the root page, nothing moving: {:.0} us", still.elapsed().as_secs_f64() * 1e6 / 100.0);
    measure(&mut host, "page slides in", &|h| h.ui.tree.cx().go_to(id, "page", true), &|h| h.ui.tree.cx().pop_to_root(id));
    measure(&mut host, "page slides out", &|h| {
        h.ui.tree.cx().go_to(id, "page", false);
        h.settle();
        h.ui.tree.cx().go_back(id, true);
    }, &|_| {});
    let card = || SkiaShape::new().width_request(300).height_request(200).background_color(Color::WHITE);
    measure(&mut host, "popup opens (backdrop blur 6)", &|h| h.ui.tree.cx().open_popup(id, card(), PopupOptions::default()), &|h| {
        h.ui.tree.cx().close_all_popups(id)
    });
    let plain = PopupOptions { show_overlay: false, ..PopupOptions::default() };
    measure(&mut host, "popup opens (no overlay)", &|h| h.ui.tree.cx().open_popup(id, card(), plain), &|h| h.ui.tree.cx().close_all_popups(id));
    measure(&mut host, "modal slides up (backdrop blur 6)", &|h| h.ui.tree.cx().push_modal(id, card(), ModalOptions::default()), &|h| {
        h.ui.tree.cx().pop_modal(id, false)
    });
}

#[test]
fn tabs_slide_with_the_back_ease_and_keep_their_stacks() {
    let mut host = tabs();
    let id = host.ui.state.shell;
    assert_eq!(taken(&mut host), ["RouteChanged ''"]);
    // The first tab's root was built from its route on the first frame.
    assert_eq!(host.ui.state.built, ["a"]);
    assert_eq!(host.pixel(200, 100), Color::RED);
    // The tab bar is 56 points at the bottom.
    assert_ne!(host.pixel(200, 270), Color::RED);

    host.ui.tree.cx().go_to(id, "detail", false);
    host.settle();
    assert_eq!(stack(&host), ["detail"]);

    host.ui.tree.cx().select_tab(id, 1);
    host.frame();
    let (old, new) = (tab_layer(&host, 0), tab_layer(&host, 1));
    assert_eq!(host.ui.state.built, ["a", "b"]);
    // C# SelectRightTab: the new root from 0.75 of the width, fading in; the old one out to the left.
    let mut frames = Vec::new();
    while host.ui.needs_frame() && frames.len() < 30 {
        host.frame_after(16.0);
        let (n, o) = (base(&host, new).p.clone(), base(&host, old).p.clone());
        frames.push((n.translation_x, n.opacity, o.translation_x));
    }
    let ease = |x: f32| (x - 1.0) * (x - 1.0) * (1.55 * (x - 1.0) + 0.55) + 1.0;
    for (i, (x, opacity, old_x)) in frames.iter().take(10).enumerate() {
        let t = (16.0 * i as f32 / 150.0).min(1.0);
        let v = if t == 1.0 { 1.0 } else { ease(t) };
        assert!(close(*x, 300.0 * (1.0 - v)), "frame {i}: {x}");
        assert!(close(*opacity, 0.001 + 0.999 * t), "frame {i}: {opacity}");
        assert!(close(*old_x, -400.0 * v), "frame {i}: {old_x}");
    }
    // It overshoots past the end before it settles, as the back-ease does.
    assert!(frames.iter().any(|f| f.0 < -1.0));
    host.settle();
    assert!(!visible(&host, old) && visible(&host, new));
    assert_eq!(base(&host, old).p.translation_x, 0.0);
    assert_eq!(shell(&host).selected_tab(), 1);
    assert_eq!(stack(&host), Vec::<String>::new());
    assert_eq!(taken(&mut host), ["Navigating Push 'detail'", "RouteChanged 'detail'", "Navigated Push 'detail'", "RouteChanged ''"]);

    // Back to the first tab by its button: its stack is still there, its layer comes to the front.
    host.tap(100.0, H - 28.0);
    assert_eq!(shell(&host).selected_tab(), 0);
    assert_eq!(stack(&host), ["detail"]);
    assert_eq!(tab_layer(&host, 1), old);
    assert_eq!(host.ui.state.built, ["a", "b"]);
    host.ui.tree.cx().pop_tab_to_root(id);
    host.settle();
    assert_eq!(stack(&host), Vec::<String>::new());
    assert!(idle(&host));
}

#[test]
fn nav_bar_back_and_home() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", false);
    host.ui.tree.cx().go_to(id, "b", false);
    host.ui.tree.cx().go_to(id, "a", false);
    host.settle();
    assert_eq!(stack(&host), ["a", "b", "a"]);
    // "‹  Back" on the left of the 56 point bar.
    host.tap(40.0, 28.0);
    assert_eq!(stack(&host), ["a", "b"]);
    // "Home" on the right.
    host.tap(W - 40.0, 28.0);
    assert_eq!(stack(&host), Vec::<String>::new());
    assert!(idle(&host));
}

// ---------------------------------------------------------------- browser history, safe area

/// The shell of `pages()` on a host that keeps a browser history, opened at `hash`.
fn browser(hash: &str) -> Host {
    let ui = Ui::new(App::default(), |app: &mut App| events(routes(SkiaShell::new().assign(&mut app.shell))).root(page(Color::BLUE)));
    let mut host = Headless::new(ui.font_bytes("Default", FONT).background(Color::BLACK), W as i32, H as i32, 1.0);
    host.ui.set_history_enabled(true);
    host.ui.location(hash, None);
    host.settle();
    host
}

fn push(depth: u32, hash: Option<&str>) -> HistoryOp {
    HistoryOp::Push { depth, hash: hash.map(str::to_owned) }
}

/// The browser moved to the entry pushed with `depth`, showing `hash`.
fn browse(host: &mut Host, hash: &str, depth: u32) {
    host.ui.location(hash, Some(depth));
    host.settle();
}

#[test]
fn the_browser_history_follows_pages_popups_and_modals() {
    let mut host = browser("");
    let id = host.ui.state.shell;
    assert_eq!(host.take_history(), []);
    host.ui.tree.cx().go_to(id, "a", true);
    host.ui.tree.cx().go_to(id, "b?id=7", false);
    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), ModalOptions::default());
    host.ui.tree.cx().open_popup(id, card(), PopupOptions::default());
    host.settle();
    // One entry each: pages carry the stack in the hash (routes as encodeURIComponent), overlays keep the URL.
    assert_eq!(host.take_history(), [push(1, Some("#/a")), push(2, Some("#/a/b%3Fid%3D7")), push(3, None), push(4, None)]);
    taken(&mut host);

    // GoBack asks the browser; the popup closes when it answers.
    host.ui.tree.cx().go_back(id, true);
    host.settle();
    assert_eq!(host.take_history(), [HistoryOp::Back]);
    assert_eq!(shell(&host).popups_count(), 1);
    browse(&mut host, "#/a/b%3Fid%3D7", 3);
    assert_eq!((shell(&host).popups_count(), shell(&host).modals_count()), (0, 1));
    // The browser's own Back button: the modal, then the page.
    browse(&mut host, "#/a/b%3Fid%3D7", 2);
    assert_eq!(shell(&host).modals_count(), 0);
    browse(&mut host, "#/a", 1);
    assert_eq!(stack(&host), ["a"]);
    assert_eq!(
        taken(&mut host),
        [
            "Navigating Pop 'b?id=7'",
            "Navigated Pop 'b?id=7'",
            "Navigating Pop 'b?id=7'",
            "Navigated Pop 'b?id=7'",
            "Navigating Pop 'a'",
            "RouteChanged 'a'",
            "Navigated Pop 'a'"
        ]
    );
    assert_eq!(host.take_history(), []);

    // Forward: the stack is rebuilt from the hash, without animation or Navigating.
    browse(&mut host, "#/a/b%3Fid%3D7", 2);
    assert_eq!(stack(&host), ["a", "b?id=7"]);
    assert_eq!(host.take_history(), [HistoryOp::Replace { depth: 2, hash: Some("#/a/b%3Fid%3D7".into()) }]);
    assert_eq!(taken(&mut host), ["RouteChanged 'b?id=7'"]);

    // A Back that Navigating cancels gives the browser its entry back.
    host.ui.state.cancel_next = true;
    browse(&mut host, "#/a", 1);
    assert_eq!(stack(&host), ["a", "b?id=7"]);
    assert_eq!(host.take_history(), [push(2, Some("#/a/b%3Fid%3D7"))]);

    // The nav bar's Back goes through the browser too.
    host.tap(40.0, 28.0);
    assert_eq!(host.take_history(), [HistoryOp::Back]);
    assert_eq!(stack(&host).len(), 2);
    browse(&mut host, "#/a", 1);
    assert_eq!(stack(&host), ["a"]);

    // Home: the page entries go, the URL loses its hash.
    host.ui.tree.cx().pop_to_root(id);
    host.settle();
    assert_eq!(host.take_history(), [HistoryOp::Replace { depth: 0, hash: Some(String::new()) }]);
    assert_eq!(stack(&host), Vec::<String>::new());
    assert!(idle(&host));
}

#[test]
fn a_deep_link_opens_its_pages() {
    let mut host = browser("#/a/b%3Fid%3D7/nope");
    // Routes that are not registered are left out; the pages are built without animation or Navigating.
    assert_eq!(stack(&host), ["a", "b?id=7"]);
    assert_eq!(host.ui.state.built, ["a", "b?id=7"]);
    assert_eq!(host.take_history(), [HistoryOp::Replace { depth: 2, hash: Some("#/a/b%3Fid%3D7".into()) }]);
    assert_eq!(taken(&mut host), ["RouteChanged 'b?id=7'"]);
    assert_eq!(host.pixel(200, 150), Color::GREEN);
    // Without a browser history the hash is not looked at, and Back pops at once.
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", false);
    host.ui.tree.cx().go_back(id, false);
    host.settle();
    assert_eq!(host.take_history(), []);
    assert_eq!(stack(&host), Vec::<String>::new());
}

#[test]
fn the_bars_make_room_for_the_safe_area() {
    let notch = Thickness { left: 0.0, top: 20.0, right: 0.0, bottom: 30.0 };
    // Unset, fullscreen: the safe area the host reports.
    let ui = Ui::new(App::default(), |app: &mut App| {
        let shell = SkiaShell::new().assign(&mut app.shell).tabs([("a", "A"), ("b", "B")]);
        routes(shell)
    })
    .mobile_fullscreen(true);
    let mut host = Headless::new(ui.font_bytes("Default", FONT), W as i32, H as i32, 1.0);
    host.ui.safe_insets(notch);
    host.settle();
    let id = host.ui.state.shell;
    let bar = host.ui.tree.children(id)[1];
    assert_eq!(host.rect(bar), Rect::new(0.0, H - 86.0, W, H));
    assert_eq!(base(&host, tab_layer(&host, 0)).p.margin.bottom, 86.0);
    host.ui.tree.cx().go_to(id, "a", false);
    host.settle();
    let host_id = tab_children(&host, tab_layer(&host, 0))[1];
    let [content, nav] = host.ui.tree.children(host_id)[..] else { panic!() };
    assert_eq!(host.rect(nav).height(), 76.0);
    assert_eq!(host.rect(content).top, 76.0);

    // Set: the bars there are move.
    host.ui.tree.get_mut(host.ui.state.shell).unwrap().set_insets(Thickness::ZERO);
    host.settle();
    assert_eq!(host.rect(bar), Rect::new(0.0, H - 56.0, W, H));
    assert_eq!(host.rect(nav).height(), 56.0);
    assert_eq!(host.rect(content).top, 56.0);
    assert_eq!(base(&host, tab_layer(&host, 0)).p.margin.bottom, 56.0);
}

// ---------------------------------------------------------------- popups, modals, toasts

/// A 200 x 100 point card, centered in the popup.
fn card() -> Build<SkiaShape> {
    SkiaShape::new().width_request(200).height_request(100).background_color(Color::WHITE)
}

#[test]
fn a_popup_scales_in_closes_on_a_tap_outside_and_not_inside() {
    let mut host = pages();
    let id = host.ui.state.shell;
    taken(&mut host);
    host.ui.tree.cx().open_popup(id, card(), PopupOptions::default());
    host.frame();
    let wrapper = overlays(&host, 2)[0];
    let content = host.ui.tree.children(wrapper)[1];
    // React: opacity 0.1 -> 1 and scale 0.5 -> 1 in 250 ms, linear.
    let frames = trace(&mut host, |h| h.ui.tree.base(wrapper).unwrap().p.opacity);
    let expected: Vec<f32> = (0..16).map(|i| 0.1 + 0.9 * (16.0 * i as f32 / 250.0)).chain([1.0]).collect();
    assert_frames(&frames, &expected);
    assert_eq!(base(&host, content).p.scale_x, 1.0);
    assert_eq!(taken(&mut host), ["Navigating Push ''", "Navigated Push ''"]);
    assert_eq!(shell(&host).popups_count(), 1);
    assert!(idle(&host));

    // Inside the card: nothing.
    host.tap(120.0, 110.0);
    assert_eq!(shell(&host).popups_count(), 1);
    // Outside: it fades and shrinks away.
    host.tap(20.0, 20.0);
    assert_eq!(shell(&host).popups_count(), 0);
    assert!(overlays(&host, 2).is_empty());
    assert_eq!(taken(&mut host), ["Navigating Pop ''", "Navigated Pop ''"]);
    assert!(idle(&host));

    // While it scales in, a tap is outside where the card is not drawn yet.
    host.ui.tree.cx().open_popup(id, card(), PopupOptions::default());
    host.frame();
    host.frame_after(16.0);
    host.tap(120.0, 110.0);
    assert_eq!(shell(&host).popups_count(), 0);

    // Not closable by a tap outside; not animated; GoBack closes it.
    let fixed = PopupOptions { close_when_background_tapped: false, animated: false, ..PopupOptions::default() };
    host.ui.tree.cx().open_popup(id, card(), fixed);
    host.settle();
    assert_eq!(base(&host, overlays(&host, 2)[0]).p.opacity, 1.0);
    host.tap(20.0, 20.0);
    assert_eq!(shell(&host).popups_count(), 1);
    host.ui.tree.cx().go_back(id, true);
    host.settle();
    assert_eq!(shell(&host).popups_count(), 0);
    assert!(idle(&host));
}

#[test]
fn go_back_closes_the_popup_then_the_modal_then_the_page() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", false);
    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), ModalOptions::default());
    host.frame();
    let drawer = host.ui.tree.children(overlays(&host, 1)[0])[1];
    // A SkiaDrawer from the bottom, closed under the bottom edge until it was laid out.
    assert_eq!(base(&host, drawer).p.translation_y, H);
    let y = trace(&mut host, |h| h.ui.tree.base(drawer).unwrap().p.translation_y);
    assert_drawer(&y, H, 0.0);
    assert_eq!(host.pixel(200, 150), Color::YELLOW);
    host.ui.tree.cx().open_popup(id, card(), PopupOptions::default());
    host.settle();
    assert_eq!((shell(&host).popups_count(), shell(&host).modals_count()), (1, 1));
    taken(&mut host);

    host.ui.tree.cx().go_back(id, true);
    host.settle();
    assert_eq!((shell(&host).popups_count(), shell(&host).modals_count(), stack(&host).len()), (0, 1, 1));
    host.ui.tree.cx().go_back(id, true);
    let y = trace(&mut host, |h| h.ui.tree.base(drawer).map_or(f32::NAN, |b| b.p.translation_y));
    // It slides down, then goes.
    let end = y.iter().position(|y| y.is_nan()).unwrap();
    assert_drawer(&y[..end], 0.0, H);
    assert_eq!((shell(&host).modals_count(), stack(&host).len()), (0, 1));
    assert_eq!(host.pixel(200, 150), Color::RED);
    host.ui.tree.cx().go_back(id, true);
    host.settle();
    assert_eq!(stack(&host).len(), 0);
    assert_eq!(
        taken(&mut host),
        [
            "Navigating Pop 'a'",
            "Navigated Pop 'a'",
            "Navigating Pop 'a'",
            "Navigated Pop 'a'",
            "Navigating Pop ''",
            "RouteChanged ''",
            "Navigated Pop ''"
        ]
    );
    assert!(idle(&host));
}

/// A drawer snap without a velocity (React SkiaDrawer: 1500 points/s gives 0.7 x 0.2 s = 140 ms,
/// CubicInOut) from `from` to `to`, frame by frame from the last frame at `from`.
fn assert_drawer(y: &[f32], from: f32, to: f32) {
    let cubic = |x: f32| if x < 0.5 { 4.0 * x * x * x } else { (x - 1.0) * (2.0 * x - 2.0).powi(2) + 1.0 };
    let start = y.iter().position(|y| *y != from).expect("it moves") - 1;
    for (i, v) in y[start..].iter().enumerate() {
        let t = 16.0 * i as f32 / 140.0;
        let expected = if t >= 1.0 { to } else { from + (to - from) * cubic(t) };
        assert!(close(*v, expected), "frame {i}: {v} instead of {expected} ({:?})", &y[start..]);
    }
    assert_eq!(*y.last().unwrap(), to);
}

#[test]
fn a_modal_is_dragged_down_to_close_with_use_gestures() {
    let draggable = ModalOptions { use_gestures: true, ..ModalOptions::default() };
    let mut host = pages();
    let id = host.ui.state.shell;
    // Without use_gestures a drag leaves it where it is.
    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), ModalOptions::default());
    host.settle();
    host.fling((200.0, 60.0), (200.0, 260.0), 150.0, 10);
    assert_eq!(shell(&host).modals_count(), 1);
    host.ui.tree.cx().pop_modal(id, false);
    host.settle();

    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), draggable);
    host.settle();
    taken(&mut host);
    // Down two thirds and let go: it snaps closed and goes, with no events (React userClosed).
    host.fling((200.0, 60.0), (200.0, 260.0), 150.0, 10);
    assert_eq!(shell(&host).modals_count(), 0);
    assert!(overlays(&host, 1).is_empty());
    assert!(taken(&mut host).is_empty());
    assert!(idle(&host));

    // With a browser history the drag goes through its Back; the answer removes it.
    let mut host = browser("");
    let id = host.ui.state.shell;
    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), draggable);
    host.settle();
    host.fling((200.0, 60.0), (200.0, 260.0), 150.0, 10);
    assert_eq!(host.take_history(), [push(1, None), HistoryOp::Back]);
    assert_eq!(shell(&host).modals_count(), 1);
    taken(&mut host);
    browse(&mut host, "", 0);
    assert_eq!(shell(&host).modals_count(), 0);
    assert_eq!(taken(&mut host), ["Navigating Pop ''", "Navigated Pop ''"]);
    assert!(idle(&host));
}

#[test]
fn a_toast_slides_up_waits_without_frames_and_leaves() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().show_toast(id, "**Saved!** in bold", 1000);
    let shown = host.time_ms();
    host.frame();
    let toast = overlays(&host, 3)[0];
    let frames = trace(&mut host, |h| h.ui.tree.base(toast).unwrap().p.opacity);
    // Up from its own height and in, 300 ms linear.
    let expected: Vec<f32> = (0..19).map(|i| 16.0 * i as f32 / 300.0).chain([1.0]).collect();
    assert_frames(&frames, &expected);
    let height = host.rect(toast).height();
    // One line of 16 points plus 24 points around it.
    assert!(height > 60.0 && height < 80.0, "{height}");
    assert_eq!(host.rect(toast).bottom, H);
    assert_eq!(base(&host, toast).p.translation_y, 0.0);
    // No frames while it shows; the host wakes at its time.
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(shown + 1000.0));
    host.settle();
    assert_eq!(shell(&host).toasts_count(), 0);
    assert!(overlays(&host, 3).is_empty());
    // What an app label showing the count sees.
    assert_eq!(host.ui.state.toasts_seen, [0, 1, 0]);
    assert!(idle(&host));

    // A new toast takes the place of the one showing; close_all_toasts clears at once.
    host.ui.tree.cx().show_toast(id, "one", 4000);
    host.frame();
    host.ui.tree.cx().show_toast_content(id, card(), 4000);
    host.frame();
    assert_eq!(shell(&host).toasts_count(), 1);
    assert_eq!(overlays(&host, 3).len(), 1);
    host.ui.tree.cx().close_all_toasts(id);
    host.frame();
    assert_eq!(shell(&host).toasts_count(), 0);
    host.settle();
    assert!(idle(&host));
}

#[test]
fn nothing_below_an_overlay_takes_a_press() {
    let mut host = pages();
    let id = host.ui.state.shell;
    host.ui.tree.cx().go_to(id, "a", false);
    host.ui.tree.cx().push_modal(id, page(Color::YELLOW), ModalOptions::default());
    host.settle();
    // The nav bar's Back is under the modal.
    host.ui.pointer(PointerKind::Down, 40.0, 28.0, host.time_ms());
    host.frame();
    host.ui.pointer(PointerKind::Up, 40.0, 28.0, host.time_ms());
    host.settle();
    assert_eq!((shell(&host).modals_count(), stack(&host).len()), (1, 1));
}

/// DrawnUI MobileIsFullscreen. Fullscreen: a shell without tabs keeps its root page and its
/// pushed pages out of the whole safe area, the navigation bar included (Pong's field went under
/// an Android navigation bar). Not fullscreen (the default): the root lies inside the safe area,
/// the shell adds nothing, and the safe area is still reported (DrawnUI Super.Screen insets).
#[test]
fn pages_keep_out_of_the_safe_area_fullscreen_or_not() {
    let notch = Thickness { left: 10.0, top: 20.0, right: 5.0, bottom: 30.0 };
    let shell_ui = || Ui::new(App::default(), |app: &mut App| routes(SkiaShell::new().assign(&mut app.shell)).root(page(Color::BLUE)));
    let mut host = Headless::new(shell_ui().mobile_fullscreen(true).font_bytes("Default", FONT), W as i32, H as i32, 1.0);
    host.ui.safe_insets(notch);
    host.settle();
    let layer = tab_layer(&host, 0);
    let root = tab_children(&host, layer)[0];
    assert_eq!(host.rect(root), Rect::new(10.0, 20.0, W - 5.0, H - 30.0));
    host.ui.tree.cx().go_to(host.ui.state.shell, "a", false);
    host.settle();
    let page = tab_children(&host, layer)[1];
    let [content, nav] = host.ui.tree.children(page)[..] else { panic!() };
    assert_eq!(host.rect(nav), Rect::new(0.0, 0.0, W, 76.0));
    assert_eq!(host.rect(content), Rect::new(10.0, 76.0, W - 5.0, H - 30.0));
    // Page "a" is red; under the navigation bar shows the page host's background, not the page.
    assert_eq!(host.pixel(200, (H - 40.0) as i32), Color::RED);
    assert_ne!(host.pixel(200, (H - 10.0) as i32), Color::RED, "the page went under the navigation bar");

    let mut host = Headless::new(shell_ui().font_bytes("Default", FONT), W as i32, H as i32, 1.0);
    host.ui.safe_insets(notch);
    host.settle();
    assert_eq!(host.ui.tree.cx().safe_insets(), notch);
    assert_eq!(host.rect(host.ui.state.shell), Rect::new(10.0, 20.0, W - 5.0, H - 30.0));
    let root = tab_children(&host, tab_layer(&host, 0))[0];
    assert_eq!(host.rect(root), Rect::new(10.0, 20.0, W - 5.0, H - 30.0));
    host.ui.tree.cx().go_to(host.ui.state.shell, "a", false);
    host.settle();
    let page = tab_children(&host, tab_layer(&host, 0))[1];
    let [content, nav] = host.ui.tree.children(page)[..] else { panic!() };
    assert_eq!(host.rect(nav), Rect::new(10.0, 20.0, W - 5.0, 76.0));
    assert_eq!(host.rect(content), Rect::new(10.0, 76.0, W - 5.0, H - 30.0));
}


/// The hand shows over what a tap acts on, never over a popup's background (its wrapper takes
/// taps only to close the popup) nor over a disabled button: the cursor follows the control's own
/// "can interact" answer, as the accessibility snapshot does (React: accessible tappable controls).
#[test]
fn a_popup_background_and_a_disabled_button_show_no_hand() {
    let mut host = pages();
    let id = host.ui.state.shell;
    let (mut text, mut go, mut off) = (Handle::<SkiaLabel>::default(), Handle::<SkiaButton>::default(), Handle::<SkiaButton>::default());
    let card = SkiaLayout::column()
        .width_request(200)
        .background_color(Color::WHITE)
        .children((
            SkiaLabel::new("Plain text").assign(&mut text),
            SkiaButton::new("Go").assign(&mut go).on_tapped(|_me, _app: &mut App, _cx| {}),
            SkiaButton::new("Off").is_disabled(true).assign(&mut off).on_tapped(|_me, _app: &mut App, _cx| {}),
        ));
    let close = PopupOptions { close_when_background_tapped: true, ..PopupOptions::default() };
    host.ui.tree.cx().open_popup(id, card, close);
    host.settle();
    let at = |host: &mut Host, id: ControlId| {
        let rect = host.rect(id);
        host.hover(rect.center_x(), rect.center_y());
        host.ui.cursor()
    };
    assert_eq!(at(&mut host, go.id()), Cursor::Pointer, "a button");
    assert_eq!(at(&mut host, text.id()), Cursor::Default, "text in the popup");
    assert_eq!(at(&mut host, off.id()), Cursor::Default, "a disabled button");
    host.hover(5.0, 5.0);
    assert_eq!(host.ui.cursor(), Cursor::Default, "the background beside the popup");
    // A tap there still closes the popup.
    host.tap(5.0, 5.0);
    host.settle();
    assert_eq!(shell(&host).popups_count(), 0);
}
