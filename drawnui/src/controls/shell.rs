//! SkiaShell: navigation for a drawn app, a port of the React SkiaShell (`src/react/SkiaShell.tsx`),
//! which mirrors DrawnUI SkiaShell and SkiaViewSwitcher: routed pages that slide in over a root
//! (or over the root of a tab; each tab keeps its own stack), a nav bar with Back, title and Home,
//! popups over a dimmed page, modals in a SkiaDrawer from the bottom, toasts.
//!
//! Handlers ask for navigation through `Cx` (`cx.go_to(shell, "shapes", true)`). The shell carries
//! it out on its own frame, where it has the app state for the route factories and its event
//! handlers; asked from input, that is the same frame. No async: what C# awaits comes as the
//! `on_navigated` event.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use skia_safe::Color;

use crate::animators::{self, AnimationId, FrameTick, ValueAnimator, easing};
use crate::control::{Control, GestureCx, Handled, Has, part_mut};
use crate::controls::backdrop::{BackdropBuild, SkiaBackdrop};
use crate::controls::button::{ButtonBuild, ButtonSet, SkiaButton};
use crate::controls::drawer::{DrawerBuild, DrawerDirection, DrawerSet, SkiaDrawer};
use crate::controls::label::{LabelBuild, SkiaLabel, TextAlignment};
use crate::controls::layout::{GridLength, LayoutBuild, SkiaGrid, SkiaLayer, SkiaLayout};
use crate::controls::rich_label::SkiaRichLabel;
use crate::controls::shape::{ShapeBuild, SkiaShape};
use crate::controls::snapping_layout::{SnappingBuild, SnappingSet};
use crate::gestures::{Gesture, GestureKind};
use crate::props;
use crate::tree::{Build, ControlId, Cx, Detached, Handle, IntoChildren, Mut, Raw, Tree, wrong_state};
use crate::types::{CacheType, IntoProp, LayoutOptions, Thickness};
use crate::ui::{Aria, HistoryOp};

const fn rgb(v: u32) -> Color {
    Color::new(0xFF00_0000 | v)
}

/// Background of pages, nav bar and tab bar (React defaults).
const PAGE: Color = rgb(0x212529);
/// Back / Home captions and the selected tab.
const ACCENT: Color = rgb(0x6EA8FE);
/// The hairline under the nav bar and over the tab bar.
const LINE: Color = rgb(0x343A40);
/// DrawnUI SkiaViewSwitcher `Custom` easing: a back-ease with side coefficient 0.55, for tabs.
fn tabs_easing(x: f32) -> f32 {
    (x - 1.0) * (x - 1.0) * ((0.55 + 1.0) * (x - 1.0) + 0.55) + 1.0
}
/// Toast slide + fade in and out (C# ShowToast).
const TOAST_IN_MS: f32 = 300.0;
const TOAST_OUT_MS: f32 = 250.0;
/// Requests carried out per frame at most; the rest waits for the next frame, so a handler that
/// navigates on every navigation cannot hang one.
const REQUESTS_PER_FRAME: usize = 32;

// ---------------------------------------------------------------- routes and arguments

/// What a page gets besides the app state: the query of its route and the arguments given with it
/// (C# GoToAsync arguments), name and value, in the order given. Values are text.
pub type ShellArguments = Vec<(String, String)>;

/// `"name?a=1&b=2"` into the name and its arguments (React SplitRoute). Values are decoded
/// as a query string (`+` is a space, `%XX` a byte).
pub fn split_route(route: &str) -> (&str, ShellArguments) {
    let Some((name, query)) = route.split_once('?') else { return (route, Vec::new()) };
    let mut arguments = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        set_argument(&mut arguments, decode(key, true), decode(value, true));
    }
    (name, arguments)
}

/// The name and arguments back into a route, `"name?a=1"` (React BuildRoute).
pub fn build_route(name: &str, arguments: &[(String, String)]) -> String {
    let mut route = name.to_owned();
    for (i, (key, value)) in arguments.iter().enumerate() {
        route.push(if i == 0 { '?' } else { '&' });
        encode(&mut route, key, true);
        route.push('=');
        encode(&mut route, value, true);
    }
    route
}

/// Sets an argument, in place when the name is there already.
fn set_argument(arguments: &mut ShellArguments, key: String, value: String) {
    match arguments.iter_mut().find(|(k, _)| *k == key) {
        Some((_, v)) => *v = value,
        None => arguments.push((key, value)),
    }
}

/// `form`: application/x-www-form-urlencoded, as URLSearchParams writes it; else as
/// encodeURIComponent.
fn encode(out: &mut String, text: &str, form: bool) {
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b'!' | b'~' | b'\'' | b'(' | b')' if !form => out.push(b as char),
            b' ' if form => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
}

/// `form`: `+` is a space too.
fn decode(text: &str, form: bool) -> String {
    let (bytes, mut out, mut i) = (text.as_bytes(), Vec::with_capacity(text.len()), 0);
    while i < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        match bytes[i] {
            b'+' if form => out.push(b' '),
            b'%' if hex(i + 1).is_some() && hex(i + 2).is_some() => {
                out.push((hex(i + 1).unwrap_or(0) * 16 + hex(i + 2).unwrap_or(0)) as u8);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------- options and events

/// C# OpenPopupAsync parameters. `PopupOptions::default()` is the React default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupOptions {
    /// Scales and fades in and out.
    pub animated: bool,
    /// A tap outside the content closes the popup.
    pub close_when_background_tapped: bool,
    /// Dims the page behind.
    pub show_overlay: bool,
    /// The dim color; `None` = `SkiaShell::POPUP_BACKGROUND_COLOR`.
    pub background_color: Option<Color>,
}

impl Default for PopupOptions {
    fn default() -> Self {
        Self { animated: true, close_when_background_tapped: true, show_overlay: true, background_color: None }
    }
}

/// C# PushModalAsync parameters. `ModalOptions::default()` is the React default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalOptions {
    /// The modal can be dragged down to close (its SkiaDrawer's `responds_to_gestures`).
    pub use_gestures: bool,
    /// Slides up and down.
    pub animated: bool,
    /// Blurs and dims the page behind (C# freezeBackground; the frozen screenshot is not ported).
    pub freeze_background: bool,
}

impl Default for ModalOptions {
    fn default() -> Self {
        Self { use_gestures: false, animated: true, freeze_background: true }
    }
}

/// Which way a navigation goes (C# NavigationSource).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationSource {
    /// Something comes up front.
    Push,
    /// Something goes away.
    Pop,
}

/// Before a page, popup or modal is pushed or popped (C# SkiaShellNavigatingArgs): set `cancel`
/// to stop it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellNavigatingArgs {
    /// The route the shell shows afterwards: the pushed one, the one below, or the current one
    /// for a popup or a modal.
    pub route: String,
    /// Push or pop.
    pub source: NavigationSource,
    /// Set to true to stop the navigation.
    pub cancel: bool,
    /// The control that goes away (a pop); `None` for a push.
    pub view: Option<ControlId>,
    /// The page that stays below a push, or the control that goes away (a pop).
    pub previous: Option<ControlId>,
}

/// After a page, popup or modal went up front or was removed (C# SkiaShellNavigatedArgs).
#[derive(Clone, Debug, PartialEq)]
pub struct ShellNavigatedArgs {
    /// The route the shell shows now.
    pub route: String,
    /// Push or pop.
    pub source: NavigationSource,
    /// The page host, the popup or modal content that came up, or the page below after a pop.
    pub view: Option<ControlId>,
}

type Factory = Rc<dyn Fn(&mut dyn Any, &ShellArguments) -> Detached>;
type NavigatingHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &mut ShellNavigatingArgs)>;
type NavigatedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &ShellNavigatedArgs)>;
type RouteChangedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &str)>;
type ChangedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;

// ---------------------------------------------------------------- the control

props!(ShellProps, ShellBuild, ShellSet {
    /// Nav bar height over a pushed page, points; the top inset comes on top. Read when a page is
    /// built.
    nav_bar_height / set_nav_bar_height: f32 = 56.0, NONE;
    /// Nav bar background.
    nav_bar_color / set_nav_bar_color: Color = PAGE, NONE;
    /// Milliseconds a page takes to slide in or out (DrawnUI SkiaViewSwitcher.PagesAnimationSpeed).
    pages_animation_speed / set_pages_animation_speed: f32 = 200.0, NONE;
    /// Background of pushed pages: they slide over what is below.
    page_background_color / set_page_background_color: Color = PAGE, NONE;
    /// Tab bar height, points; the bottom inset comes on top. Read when the shell is mounted.
    tab_bar_height / set_tab_bar_height: f32 = 56.0, NONE;
    /// Tab bar background.
    tab_bar_color / set_tab_bar_color: Color = PAGE, NONE;
    /// Slide and fade between tabs (DrawnUI SkiaViewSwitcher.AnimateTabs).
    animate_tabs / set_animate_tabs: bool = false, NONE;
    /// Milliseconds of a tab switch (DrawnUI TabsAnimationSpeed).
    tabs_animation_speed / set_tabs_animation_speed: f32 = 150.0, NONE;
    /// Safe area, points (React `Insets`): the nav bar grows by the top one, the tab bar by the
    /// bottom one, pages and overlays keep out of the sides and the bottom. `None` = the safe area
    /// content keeps out of itself (`Cx::content_insets`: the platform's when
    /// `Ui::mobile_fullscreen`, zero otherwise), followed when it changes; setting it fixes them.
    insets / set_insets: Option<Thickness> = None, APPLY;
    /// The pages live in the URL hash (`#/a/b`) and the browser's Back button closes the top
    /// popup, then the top modal, then pops the page (React UseBrowserHistory). Only on a host
    /// with a history (the browser); read when the shell is mounted.
    use_browser_history / set_use_browser_history: bool = true, NONE;
});

impl IntoProp<Option<Thickness>> for Thickness {
    fn into_prop(self) -> Option<Thickness> {
        Some(self)
    }
}

/// The layers the shell is made of, in paint order.
#[derive(Clone, Copy)]
struct Parts {
    /// Holds one layer per tab (one without tabs): its root, then its pushed pages.
    pages: ControlId,
    modals: ControlId,
    popups: ControlId,
    toasts: ControlId,
}

struct Stack {
    layer: ControlId,
    root: ControlId,
    pages: Vec<Page>,
}

struct Page {
    /// With its query.
    route: String,
    arguments: ShellArguments,
    host: ControlId,
    /// The slide running on it.
    slide: Option<AnimationId>,
    /// It slides out; it stays in the stack until it is gone, as in React.
    popping: bool,
}

/// A popup, modal or toast.
struct Overlay {
    wrapper: ControlId,
    /// Popup: the centered layer around the content. Modal: the panel that slides.
    content: ControlId,
    animation: Option<AnimationId>,
    closing: bool,
    animated: bool,
    /// It has an entry in the browser's history.
    history: bool,
}

enum Request {
    Start,
    GoTo { route: String, animated: bool, arguments: ShellArguments },
    GoBack { animated: bool },
    PopToRoot,
    PopTabToRoot,
    SelectTab(usize),
    OpenPopup { content: Detached, options: PopupOptions },
    ClosePopup { wrapper: Option<ControlId>, animated: bool },
    CloseAllPopups,
    PushModal { content: Detached, options: ModalOptions },
    PopModal { animated: bool },
    ShowToast { content: Detached, ms: f32 },
    CloseAllToasts,
    HistoryMoved { depth: u32, hash: String },
    Done(Done),
}

/// An animation reached its end.
#[derive(Clone, Copy)]
enum Done {
    Pushed(ControlId),
    Popped(ControlId),
    TabShown(usize),
    PopupOpened(ControlId),
    PopupClosed(ControlId),
    ModalOpened(ControlId),
    ModalClosed(ControlId),
    ToastDue(ControlId),
    ToastClosed(ControlId),
}

/// Navigation host: routes, per-tab page stacks, popups, modals, toasts (DrawnUI SkiaShell, as the
/// React SkiaShell). Build it with `SkiaShell::new()`, register pages with `route`, give the root
/// with `root` or the tabs with `tabs`, and navigate with the `Cx` methods (`go_to`, `go_back`,
/// `open_popup`, ...). Read its state with `find::<SkiaShell>`: `route`, `navigation_stack`, ...
pub struct SkiaShell {
    layout: SkiaLayout,
    /// The shell's own properties.
    pub p: ShellProps,
    me: ControlId,
    routes: Vec<(String, Factory)>,
    titles: Vec<(String, String)>,
    /// (route, title) per tab.
    tabs: Vec<(String, String)>,
    /// The root content, until the shell is mounted.
    root: Vec<Detached>,
    parts: Option<Parts>,
    /// One per tab (one without tabs); a tab gets its layer when it is first selected.
    stacks: Vec<Option<Stack>>,
    selected: usize,
    /// An animated tab switch: its animation and the tab that leaves.
    tab_switch: Option<(AnimationId, usize)>,
    /// Caption button and indicator of every tab.
    tab_bar: Vec<(Handle<SkiaButton>, ControlId)>,
    popups: Vec<Overlay>,
    modals: Vec<Overlay>,
    toasts: Vec<Overlay>,
    requests: VecDeque<Request>,
    ticking: bool,
    /// The route `on_route_changed` reported last.
    route_seen: Option<String>,
    /// The browser's history is used; its entries this shell pushed, oldest first.
    history_on: bool,
    history: Vec<HistoryKind>,
    /// The safe area the bars were built or moved for.
    applied_insets: Option<Thickness>,
    tab_bar_id: Option<ControlId>,
    on_navigating: Option<NavigatingHandler>,
    on_navigated: Option<NavigatedHandler>,
    on_route_changed: Option<RouteChangedHandler>,
    on_changed: Option<ChangedHandler>,
}

impl SkiaShell {
    /// Toast background (C# ToastBackgroundColor).
    pub const TOAST_BACKGROUND_COLOR: Color = Color::new(0xCC00_0000);
    /// Toast text (C# ToastTextColor).
    pub const TOAST_TEXT_COLOR: Color = Color::WHITE;
    /// Points.
    pub const TOAST_TEXT_SIZE: f32 = 16.0;
    /// Points around the toast text.
    pub const TOAST_TEXT_MARGINS: f32 = 24.0;
    /// The dim behind popups and modals (C# PopupBackgroundColor).
    pub const POPUP_BACKGROUND_COLOR: Color = Color::new(0x6600_0000);
    /// Points of blur behind popups and modals (C# PopupsBackgroundBlur).
    pub const POPUPS_BACKGROUND_BLUR: f32 = 6.0;
    /// Milliseconds a popup takes to open or close (C# PopupsAnimationSpeed).
    pub const POPUPS_ANIMATION_SPEED: f32 = 250.0;
    /// Caption and indicator of the selected tab.
    pub const TAB_SELECTED_COLOR: Color = ACCENT;
    /// Caption of the other tabs.
    pub const TAB_COLOR: Color = rgb(0xADB5BD);

    /// A shell filling its parent, with no routes yet.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaShell> {
        let shell = SkiaShell {
            layout: SkiaLayout::default(),
            p: ShellProps::default(),
            me: Handle::<SkiaShell>::default().id(),
            routes: Vec::new(),
            titles: Vec::new(),
            tabs: Vec::new(),
            root: Vec::new(),
            parts: None,
            stacks: Vec::new(),
            selected: 0,
            tab_switch: None,
            tab_bar: Vec::new(),
            popups: Vec::new(),
            modals: Vec::new(),
            toasts: Vec::new(),
            requests: VecDeque::new(),
            ticking: false,
            route_seen: None,
            history_on: false,
            history: Vec::new(),
            applied_insets: None,
            tab_bar_id: None,
            on_navigating: None,
            on_navigated: None,
            on_route_changed: None,
            on_changed: None,
        };
        let mut build = Build::new(shell).fill();
        let id = build.id();
        build.control_mut().me = id;
        build
    }

    fn stack(&self) -> Option<&Stack> {
        self.stacks.get(self.selected)?.as_ref()
    }

    /// The current route with its query; "" while the root shows (React `Route`).
    pub fn route(&self) -> &str {
        self.stack().and_then(|s| s.pages.last()).map_or("", |p| &p.route)
    }

    /// The arguments of the current page (React `Arguments`).
    pub fn arguments(&self) -> &[(String, String)] {
        self.stack().and_then(|s| s.pages.last()).map_or(&[], |p| &p.arguments)
    }

    /// The routes pushed on the current tab, oldest first; the root is not one (React `NavigationStack`).
    pub fn navigation_stack(&self) -> impl Iterator<Item = &str> {
        self.stack().into_iter().flat_map(|s| s.pages.iter().map(|p| p.route.as_str()))
    }

    /// A page, a modal or a popup is open: `go_back` has something to close.
    pub fn can_go_back(&self) -> bool {
        self.stack().is_some_and(|s| !s.pages.is_empty()) || !self.popups.is_empty() || !self.modals.is_empty()
    }

    /// Popups open, closing ones included.
    pub fn popups_count(&self) -> usize {
        self.popups.len()
    }

    pub fn modals_count(&self) -> usize {
        self.modals.len()
    }

    pub fn toasts_count(&self) -> usize {
        self.toasts.len()
    }

    /// The tab shown (DrawnUI SkiaViewSwitcher.SelectedIndex); 0 without tabs.
    pub fn selected_tab(&self) -> usize {
        self.selected
    }

    fn title(&self, name: &str) -> String {
        self.titles.iter().find(|(r, _)| r == name).map_or(name, |(_, t)| t.as_str()).to_owned()
    }

    fn factory(&self, name: &str) -> Option<Factory> {
        self.routes.iter().find(|(r, _)| r == name).map(|(_, f)| f.clone())
    }

    /// Builds the layers at mount: pages (with the first tab), tab bar, modals, popups, toasts.
    fn mount_parts(&mut self, cx: &mut Cx) {
        let me = self.me;
        let insets = self.p.insets.unwrap_or(cx.content_insets());
        self.applied_insets = Some(insets);
        let pages = cx.add_child(me, SkiaLayer::new().fill());
        if !self.tabs.is_empty() {
            let bar = self.tab_bar_build(insets);
            self.tab_bar_id = Some(cx.add_child(me, bar));
        }
        // Modals, popups and toasts keep out of the safe area on every side.
        let overlays = || SkiaLayer::new().fill().margin(insets);
        let (modals, popups, toasts) = (cx.add_child(me, overlays()), cx.add_child(me, overlays()), cx.add_child(me, overlays()));
        self.parts = Some(Parts { pages, modals, popups, toasts });
        self.stacks = (0..self.tabs.len().max(1)).map(|_| None).collect();
        let root = std::mem::take(&mut self.root);
        self.stacks[0] = Some(mount_stack(cx, pages, root, true, self.tab_look(insets), insets));
        self.history_on = self.p.use_browser_history && cx.has_history();
        if self.history_on {
            cx.listen_history(me, true);
        }
        if !self.tabs.is_empty() || self.on_route_changed.is_some() || self.history_on {
            // The first tab's root comes from its route, the first route is reported, a deep link
            // builds its pages: that needs the app state, so it is done on the shell's first frame.
            self.requests.push_front(Request::Start);
        }
    }

    /// For a shell with tabs: the room left for the tab bar and the background of a tab's root.
    fn tab_look(&self, insets: Thickness) -> Option<(f32, Color)> {
        (!self.tabs.is_empty()).then(|| (self.p.tab_bar_height + insets.bottom, self.p.page_background_color))
    }

    /// The bars follow a new safe area.
    fn apply_insets(&mut self, cx: &mut Cx) {
        let insets = self.p.insets.unwrap_or(cx.content_insets());
        if self.applied_insets == Some(insets) {
            return;
        }
        self.applied_insets = Some(insets);
        let (bar, nav_top) = (self.p.tab_bar_height + insets.bottom, self.p.nav_bar_height + insets.top);
        if let Some(mut tab_bar) = self.tab_bar_id.and_then(|id| cx.any_mut(id)) {
            tab_bar.set_height_request(bar);
            tab_bar.set_padding((0.0, 0.0, 0.0, insets.bottom));
        }
        let tabs = !self.tabs.is_empty();
        let (root_margin, content_margin, nav_padding) = self.page_insets(insets);
        for stack in self.stacks.iter().flatten() {
            if tabs && let Some(mut layer) = cx.any_mut(stack.layer) {
                layer.set_margin((0.0, 0.0, 0.0, bar));
            }
            if let Some(mut root) = cx.any_mut(stack.root) {
                root.set_margin(root_margin);
            }
            for page in &stack.pages {
                let [content, nav, ..] = *cx.tree.children(page.host) else { continue };
                if let Some(mut content) = cx.any_mut(content) {
                    content.set_margin(content_margin);
                }
                if let Some(mut nav) = cx.any_mut(nav) {
                    nav.set_height_request(nav_top);
                    nav.set_padding(nav_padding);
                }
            }
        }
        if let Some(parts) = self.parts {
            for layer in [parts.modals, parts.popups, parts.toasts] {
                if let Some(mut layer) = cx.any_mut(layer) {
                    layer.set_margin(insets);
                }
            }
        }
    }

    /// Where pages keep out of the safe area: the root page's margin, a pushed page's content
    /// margin (under its nav bar) and its nav bar's padding (the bar's color goes under the status
    /// bar and to the sides, its buttons do not). The bottom one belongs to the tab bar when there
    /// is one.
    fn page_insets(&self, insets: Thickness) -> (Thickness, Thickness, Thickness) {
        let bottom = if self.tabs.is_empty() { insets.bottom } else { 0.0 };
        let nav_top = self.p.nav_bar_height + insets.top;
        (
            Thickness::new(insets.left, insets.top, insets.right, bottom),
            Thickness::new(insets.left, nav_top, insets.right, bottom),
            Thickness::new(insets.left, insets.top, insets.right, 0.0),
        )
    }

    fn tab_bar_build(&mut self, insets: Thickness) -> Build<SkiaLayout> {
        let me = self.me;
        let mut cells = Vec::with_capacity(self.tabs.len());
        for (i, (_, title)) in self.tabs.iter().enumerate() {
            let selected = i == self.selected;
            let mut button = Handle::default();
            let indicator = SkiaShape::new()
                .height_request(3)
                .width_request(36)
                .corner_radius(2)
                .background_color(Self::TAB_SELECTED_COLOR)
                .horizontal_options(LayoutOptions::Center)
                .vertical_options(LayoutOptions::End)
                .input_transparent(true)
                .is_visible(selected);
            let indicator_id = indicator.id();
            let caption = SkiaButton::new(title.as_str())
                .assign(&mut button)
                .fill()
                .background_color(Color::TRANSPARENT)
                .corner_radius(0)
                .font_size(13)
                .font_family(if selected { "FontTextBold" } else { "" })
                .text_color(if selected { Self::TAB_SELECTED_COLOR } else { Self::TAB_COLOR })
                .accessibility_role(Aria::BUTTON)
                .accessibility_label(title.as_str());
            self.tab_bar.push((button, indicator_id));
            cells.push(SkiaLayer::new().column(i as i32).fill().children((tapped(caption, move |cx| cx.select_tab(me, i)), indicator)));
        }
        let p = &self.p;
        SkiaLayer::new()
            .height_request(p.tab_bar_height + insets.bottom)
            .vertical_options(LayoutOptions::End)
            .background_color(p.tab_bar_color)
            .block_gestures_below(true)
            .padding((0.0, 0.0, 0.0, insets.bottom))
            .children((
                SkiaLayer::new().height_request(1).background_color(LINE),
                SkiaGrid::new().column_definitions(vec![GridLength::STAR; self.tabs.len()]).fill().children(cells),
            ))
    }

    /// A pushed page: the content under a nav bar with Back, the title and Home (React PageHost).
    fn page_host(&self, title: &str, page: Detached, insets: Thickness) -> Build<SkiaLayout> {
        let (me, p) = (self.me, &self.p);
        let nav_top = p.nav_bar_height + insets.top;
        let (_, content_margin, nav_padding) = self.page_insets(insets);
        let nav_button = |text: &str| {
            SkiaButton::new(text)
                .background_color(Color::TRANSPARENT)
                .text_color(ACCENT)
                .font_size(16)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::BUTTON)
                .accessibility_label(text.trim_start_matches(['‹', ' ']))
        };
        SkiaLayer::new().fill().background_color(p.page_background_color).block_gestures_below(true).children((
            SkiaLayer::new().fill().margin(content_margin).children(page),
            // Static chrome: one bitmap; the ripple of a button is drawn over it.
            SkiaLayer::new()
                .height_request(nav_top)
                .background_color(p.nav_bar_color)
                .padding(nav_padding)
                .use_cache(CacheType::Image)
                .children((
                    tapped(nav_button("‹  Back").margin((8, 0)), move |cx| cx.go_back(me, true)),
                    SkiaLabel::new(title)
                        .font_size(18)
                        .font_family("FontTextBold")
                        .text_color(Color::WHITE)
                        .fill_x()
                        .horizontal_text_alignment(TextAlignment::Center)
                        .vertical_options(LayoutOptions::Center)
                        .max_lines(1)
                        .margin((96, 0))
                        .accessibility_role(Aria::HEADING),
                    tapped(nav_button("Home").horizontal_options(LayoutOptions::End).margin((0, 0, 8, 0)), move |cx| {
                        cx.pop_to_root(me)
                    }),
                    SkiaLayer::new().height_request(1).vertical_options(LayoutOptions::End).background_color(LINE),
                )),
        ))
    }
}

impl Has<ShellProps> for SkiaShell {
    fn part(&self) -> &ShellProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ShellProps {
        &mut self.p
    }
}

impl Control for SkiaShell {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    fn on_props_changed(&mut self, cx: &mut Cx) {
        if self.parts.is_none() {
            self.mount_parts(cx);
        } else {
            self.apply_insets(cx);
        }
        if !self.requests.is_empty() && !std::mem::replace(&mut self.ticking, true) {
            animators::start_frame(cx.tree, self.me, tick);
        }
    }

    /// The browser went back or forward: done on the shell's frame, with the app state.
    fn on_history(&mut self, cx: &mut GestureCx, depth: u32, hash: &str) {
        self.requests.push_back(Request::HistoryMoved { depth, hash: hash.to_owned() });
        if !std::mem::replace(&mut self.ticking, true) {
            animators::start_frame(cx.tree, self.me, tick);
        }
    }
}

impl Build<SkiaShell> {
    /// Registers a page: `factory(app, arguments)` builds it each time the route is navigated to.
    pub fn route<S: Any, B: Into<Detached>>(
        mut self,
        name: &str,
        factory: impl Fn(&mut S, &ShellArguments) -> B + 'static,
    ) -> Self {
        let factory: Factory = Rc::new(move |state, arguments| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            factory(state, arguments).into()
        });
        self.control_mut().routes.push((name.to_owned(), factory));
        self
    }

    /// Nav bar titles by route name; a route without one shows its name.
    pub fn titles<'a>(mut self, titles: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let own = &mut self.control_mut().titles;
        own.extend(titles.into_iter().map(|(r, t)| (r.to_owned(), t.to_owned())));
        self
    }

    /// The bottom tab bar: (route, title) per tab. A tab's root is built from its route when the
    /// tab is first shown, and each tab keeps its own page stack (DrawnUI SkiaViewSwitcher tabs).
    pub fn tabs<'a>(mut self, tabs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let own = &mut self.control_mut().tabs;
        own.extend(tabs.into_iter().map(|(r, t)| (r.to_owned(), t.to_owned())));
        self
    }

    /// What shows while no page is pushed (the React children of SkiaShell). Unused with tabs.
    pub fn root(mut self, content: impl IntoChildren) -> Self {
        content.push_into(&mut self.control_mut().root);
        self
    }

    /// Before a page, popup or modal is pushed or popped (C# Navigating): set `cancel` to stop it.
    /// A navigation of this shell asked for inside runs after it.
    pub fn on_navigating<S: Any>(
        mut self,
        mut f: impl FnMut(&mut Mut<'_, SkiaShell>, &mut S, &mut Cx<'_>, &mut ShellNavigatingArgs) + 'static,
    ) -> Self {
        self.control_mut().on_navigating = Some(Box::new(move |me, state, cx, e| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, e)
        }));
        self
    }

    /// After a page, popup or modal went up front or was removed, its animation done (C# Navigated).
    pub fn on_navigated<S: Any>(
        mut self,
        mut f: impl FnMut(&mut Mut<'_, SkiaShell>, &mut S, &mut Cx<'_>, &ShellNavigatedArgs) + 'static,
    ) -> Self {
        self.control_mut().on_navigated = Some(Box::new(move |me, state, cx, e| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, e)
        }));
        self
    }

    /// The current route changed, "" = the root (C# RouteChanged). Runs once at mount too, as React.
    pub fn on_route_changed<S: Any>(
        mut self,
        mut f: impl FnMut(&mut Mut<'_, SkiaShell>, &mut S, &mut Cx<'_>, &str) + 'static,
    ) -> Self {
        self.control_mut().on_route_changed = Some(Box::new(move |me, state, cx, route| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, route)
        }));
        self
    }
}

impl Build<SkiaShell> {
    /// After a frame in which the shell changed: a page, popup, modal or toast came or went, an
    /// animation of one ended, the tab changed (React: every `useShell()` consumer renders again).
    /// The place to copy `route`, `navigation_stack` or the counts into the app state.
    pub fn on_changed<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaShell>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.control_mut().on_changed = Some(Box::new(move |me, state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx)
        }));
        self
    }
}

/// A stateless tapped handler for the shell's own buttons: they only ask the shell.
fn tapped<T: Control>(mut build: Build<T>, mut run: impl FnMut(&mut Cx) + 'static) -> Build<T> {
    build.handlers.tapped = Some(Box::new(move |_, _, cx| run(cx)));
    build
}

// ---------------------------------------------------------------- popup wrapper

/// The layer a popup lives in (C# PopupWrapper): it fills the shell, takes every gesture the
/// content leaves, and closes the popup on a tap outside the content.
struct PopupWrapper {
    layout: SkiaLayout,
    content: ControlId,
    /// The last tap landed outside the drawn content.
    outside: bool,
}

impl Control for PopupWrapper {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// Its tapped handler only closes the popup on a tap beside the content: the background is
    /// nothing to interact with (no hand over it, no tab stop).
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(false)
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if gesture.kind == GestureKind::Tapped {
            // Through the content's transform, as C# HitIsInside: a popup scaling in is hit where drawn.
            let tree = &*cx.tree;
            let matrix = tree.render.get(self.content.index as usize).and_then(|r| r.matrix).and_then(|m| m.invert());
            let local = matrix.map_or(cx.point, |m| m.map_point(cx.point));
            let r = tree.base(self.content).map(|b| b.rect).unwrap_or_default();
            self.outside = !(local.x >= r.left && local.x < r.right && local.y >= r.top && local.y < r.bottom);
        }
        Handled::No
    }
}

// ---------------------------------------------------------------- navigation from handlers

impl Cx<'_> {
    /// Pushes a registered route over the current page, sliding in from the right when `animated`
    /// (C# GoToAsync). The route may carry a query: `"detail?id=7"`. Panics for a route that is
    /// not registered.
    pub fn go_to(&mut self, shell: impl Into<ControlId>, route: &str, animated: bool) {
        self.go_to_with(shell, route, animated, Vec::new())
    }

    /// `go_to` with arguments; they override query values of the same name (C# GoToAsync arguments).
    pub fn go_to_with(&mut self, shell: impl Into<ControlId>, route: &str, animated: bool, arguments: ShellArguments) {
        request(self, shell.into(), Request::GoTo { route: route.to_owned(), animated, arguments });
    }

    /// Closes the top popup, else the top modal, else pops the current page (C# GoBack order).
    pub fn go_back(&mut self, shell: impl Into<ControlId>, animated: bool) {
        request(self, shell.into(), Request::GoBack { animated });
    }

    /// Removes every pushed page of every tab at once; the roots show (React PopToRootAsync).
    pub fn pop_to_root(&mut self, shell: impl Into<ControlId>) {
        request(self, shell.into(), Request::PopToRoot);
    }

    /// Removes every pushed page of the current tab (C# PopTabToRoot).
    pub fn pop_tab_to_root(&mut self, shell: impl Into<ControlId>) {
        request(self, shell.into(), Request::PopTabToRoot);
    }

    /// Shows a tab, sliding and fading when `animate_tabs` (C# SkiaViewSwitcher.SelectedIndex).
    pub fn select_tab(&mut self, shell: impl Into<ControlId>, index: usize) {
        request(self, shell.into(), Request::SelectTab(index));
    }

    /// Shows `content` centered above everything, over a dimmed page; it scales in from 0.5 and
    /// fades in (C# OpenPopupAsync).
    pub fn open_popup(&mut self, shell: impl Into<ControlId>, content: impl Into<Detached>, options: PopupOptions) {
        request(self, shell.into(), Request::OpenPopup { content: content.into(), options });
    }

    /// Closes the top popup (C# ClosePopupAsync).
    pub fn close_popup(&mut self, shell: impl Into<ControlId>, animated: bool) {
        request(self, shell.into(), Request::ClosePopup { wrapper: None, animated });
    }

    /// Closes every popup, top first, without animation.
    pub fn close_all_popups(&mut self, shell: impl Into<ControlId>) {
        request(self, shell.into(), Request::CloseAllPopups);
    }

    /// Shows `content` full screen, sliding up from the bottom over a dimmed page (C# PushModalAsync).
    pub fn push_modal(&mut self, shell: impl Into<ControlId>, content: impl Into<Detached>, options: ModalOptions) {
        request(self, shell.into(), Request::PushModal { content: content.into(), options });
    }

    /// Closes the top modal, sliding it down (C# PopModalAsync).
    pub fn pop_modal(&mut self, shell: impl Into<ControlId>, animated: bool) {
        request(self, shell.into(), Request::PopModal { animated });
    }

    /// A banner of markdown text at the bottom for `ms` milliseconds; it slides up and fades in,
    /// then down and out. A toast shown before goes at once (C# ShowToast).
    pub fn show_toast(&mut self, shell: impl Into<ControlId>, text: &str, ms: impl IntoProp<f32>) {
        let label = SkiaRichLabel::new(text)
            .text_color(SkiaShell::TOAST_TEXT_COLOR)
            .font_size(SkiaShell::TOAST_TEXT_SIZE)
            .margin(SkiaShell::TOAST_TEXT_MARGINS)
            .fill_x();
        self.show_toast_content(shell, label, ms);
    }

    /// `show_toast` with any content.
    pub fn show_toast_content(&mut self, shell: impl Into<ControlId>, content: impl Into<Detached>, ms: impl IntoProp<f32>) {
        request(self, shell.into(), Request::ShowToast { content: content.into(), ms: ms.into_prop() });
    }

    /// Removes every toast at once.
    pub fn close_all_toasts(&mut self, shell: impl Into<ControlId>) {
        request(self, shell.into(), Request::CloseAllToasts);
    }
}

thread_local! {
    // The shell whose event handler runs (it is out of the tree meanwhile) and what was asked of
    // it from inside: carried out right after the handler.
    static FIRING: RefCell<(Option<ControlId>, Vec<Request>)> = const { RefCell::new((None, Vec::new())) };
}

fn shell(tree: &mut Tree, id: ControlId) -> Option<&mut SkiaShell> {
    part_mut(tree.node_mut(id)?.kind.as_deref_mut()?)
}

fn request(cx: &mut Cx, id: ControlId, request: Request) {
    let Some(s) = shell(cx.tree, id) else {
        FIRING.with_borrow_mut(|(firing, pending)| {
            if *firing == Some(id) {
                pending.push(request);
            }
        });
        return;
    };
    s.requests.push_back(request);
    wake(cx.tree, id);
}

fn post(cx: &mut Cx, id: ControlId, done: Done) {
    request(cx, id, Request::Done(done));
}

/// Starts the shell's frame, unless it runs.
fn wake(tree: &mut Tree, id: ControlId) {
    if let Some(s) = shell(tree, id)
        && !std::mem::replace(&mut s.ticking, true)
    {
        animators::start_frame(tree, id, tick);
    }
}

/// The shell's frame: carries out what was asked, with the app state at hand.
fn tick(id: ControlId, _time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut ran = false;
    for _ in 0..REQUESTS_PER_FRAME {
        let Some(request) = shell(cx.tree, id).and_then(|s| s.requests.pop_front()) else { break };
        run(cx, id, state, request);
        ran = true;
    }
    if ran {
        fire(cx, id, |s| &mut s.on_changed, |h, me, cx| h(me, state, cx));
    }
    let keep = shell(cx.tree, id).is_some_and(|s| {
        s.ticking = !s.requests.is_empty();
        s.ticking
    });
    FrameTick { keep, state_touched: ran }
}

fn run(cx: &mut Cx, id: ControlId, state: &mut dyn Any, request: Request) {
    match request {
        Request::Start => start(cx, id, state),
        Request::GoTo { route, animated, arguments } => go_to(cx, id, state, &route, animated, arguments),
        Request::GoBack { animated } => go_back(cx, id, state, animated),
        Request::PopToRoot => pop_to_root(cx, id, state, true),
        Request::PopTabToRoot => pop_to_root(cx, id, state, false),
        Request::SelectTab(index) => select_tab(cx, id, state, index),
        Request::OpenPopup { content, options } => open_popup(cx, id, state, content, options),
        Request::ClosePopup { wrapper, animated } => {
            let top = shell(cx.tree, id).and_then(|s| s.popups.last()).map(|p| p.wrapper);
            if let Some(wrapper) = wrapper.or(top) {
                close_popup(cx, id, state, wrapper, animated);
            }
        }
        Request::CloseAllPopups => {
            let count = shell(cx.tree, id).map_or(0, |s| s.popups.len());
            for i in (0..count).rev() {
                if let Some(wrapper) = shell(cx.tree, id).and_then(|s| s.popups.get(i)).map(|p| p.wrapper) {
                    close_popup(cx, id, state, wrapper, false);
                }
            }
        }
        Request::PushModal { content, options } => push_modal(cx, id, state, content, options),
        Request::PopModal { animated } => pop_modal(cx, id, state, animated),
        Request::ShowToast { content, ms } => show_toast(cx, id, content, ms),
        Request::CloseAllToasts => close_all_toasts(cx, id),
        Request::HistoryMoved { depth, hash } => history_moved(cx, id, state, depth, &hash),
        Request::Done(done) => finish(cx, id, state, done),
    }
}

// ---------------------------------------------------------------- events

/// Runs one of the shell's handlers with the shell as `me`: the shell leaves the tree meanwhile,
/// so the handler can reach the rest; what it asks of this shell is queued after it.
fn fire<H: ?Sized>(
    cx: &mut Cx,
    id: ControlId,
    slot: fn(&mut SkiaShell) -> &mut Option<Box<H>>,
    call: impl FnOnce(&mut H, Raw<'_>, &mut Cx<'_>),
) {
    let Some(mut handler) = shell(cx.tree, id).and_then(|s| slot(s).take()) else { return };
    let Some(mut node) = cx.tree.take(id) else { return };
    let outer = FIRING.with_borrow_mut(|(firing, _)| firing.replace(id));
    let mut queue = Vec::new();
    if let Some(control) = node.kind.as_deref_mut() {
        call(&mut *handler, Raw { id, control, base: &mut node.base, queue: &mut queue }, cx);
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    let asked = FIRING.with_borrow_mut(|(firing, pending)| {
        *firing = outer;
        std::mem::take(pending)
    });
    if let Some(s) = shell(cx.tree, id) {
        slot(s).get_or_insert(handler);
        s.requests.extend(asked);
    }
}

/// C# NotifyAndCheckCanNavigate: false when the handler cancelled.
fn navigating(
    cx: &mut Cx,
    id: ControlId,
    state: &mut dyn Any,
    route: &str,
    source: NavigationSource,
    view: Option<ControlId>,
    previous: Option<ControlId>,
) -> bool {
    let mut e = ShellNavigatingArgs { route: route.to_owned(), source, cancel: false, view, previous };
    fire(cx, id, |s| &mut s.on_navigating, |h, me, cx| h(me, state, cx, &mut e));
    !e.cancel
}

fn navigated(cx: &mut Cx, id: ControlId, state: &mut dyn Any, route: &str, source: NavigationSource, view: Option<ControlId>) {
    if shell(cx.tree, id).is_some_and(|s| s.on_navigated.is_some()) {
        let e = ShellNavigatedArgs { route: route.to_owned(), source, view };
        fire(cx, id, |s| &mut s.on_navigated, |h, me, cx| h(me, state, cx, &e));
    }
}

/// Reports the current route when it is not the one reported last.
fn route_changed(cx: &mut Cx, id: ControlId, state: &mut dyn Any) {
    let Some(s) = shell(cx.tree, id) else { return };
    if s.route_seen.as_deref() == Some(s.route()) {
        return;
    }
    let route = s.route().to_owned();
    s.route_seen = Some(route.clone());
    fire(cx, id, |s| &mut s.on_route_changed, |h, me, cx| h(me, state, cx, &route));
}

fn current_route(tree: &mut Tree, id: ControlId) -> String {
    shell(tree, id).map_or_else(String::new, |s| s.route().to_owned())
}

// ---------------------------------------------------------------- pages

fn set_visible(tree: &mut Tree, id: ControlId, visible: bool) {
    if let Some(mut c) = tree.any_mut(id) {
        c.set_is_visible(visible);
    }
}

/// A tab's layer (the only one without tabs), with its root layer holding `content`. `tab`: the
/// room left for the tab bar and the root's background, for a shell with tabs.
fn mount_stack(cx: &mut Cx, pages: ControlId, content: Vec<Detached>, visible: bool, tab: Option<(f32, Color)>, insets: Thickness) -> Stack {
    // The root page keeps out of the safe area; the tab bar takes the bottom one when there is one.
    let bottom_inset = if tab.is_some() { 0.0 } else { insets.bottom };
    let mut root = SkiaLayer::new().fill().margin((insets.left, insets.top, insets.right, bottom_inset)).children(content);
    if let Some((_, background)) = tab {
        root = root.background_color(background);
    }
    let root_id = root.id();
    let bottom = tab.map_or(0.0, |(bottom, _)| bottom);
    let layer = SkiaLayer::new().fill().margin((0.0, 0.0, 0.0, bottom)).is_visible(visible).children(root);
    Stack { layer: cx.add_child(pages, layer), root: root_id, pages: Vec::new() }
}

/// The end value exactly at the end.
fn lerp(from: f32, to: f32, v: f32) -> f32 {
    if v == 1.0 { to } else { from + (to - from) * v }
}

/// Width of the shell in points: how far a page slides.
fn width(tree: &Tree, id: ControlId) -> f32 {
    tree.base(id).map_or(0.0, |b| b.rect.width() / b.scale.max(0.01))
}

/// Only the top page (or the root) shows; while a page slides, everything below it shows too,
/// as React.
fn show_pages(tree: &mut Tree, id: ControlId, tab: usize) {
    let Some(stack) = shell(tree, id).and_then(|s| s.stacks.get(tab)?.as_ref()) else { return };
    let moving = stack.pages.iter().any(|p| p.slide.is_some());
    let (root, count) = (stack.root, stack.pages.len());
    set_visible(tree, root, moving || count == 0);
    for i in 0..count {
        let Some(host) = shell(tree, id).and_then(|s| Some(s.stacks.get(tab)?.as_ref()?.pages.get(i)?.host)) else { break };
        set_visible(tree, host, moving || i + 1 == count);
    }
}

/// A page host slides along x (React TranslateToAsync, linear), then `done` is posted.
fn slide(cx: &mut Cx, id: ControlId, host: ControlId, from: f32, to: f32, ms: f32, done: Done) -> AnimationId {
    cx.animate(host, ms, easing::linear, move |v, cx| {
        if let Some(mut c) = cx.any_mut(host) {
            c.set_translation_x(lerp(from, to, v));
        }
        if v == 1.0 {
            post(cx, id, done);
        }
    })
}

fn start(cx: &mut Cx, id: ControlId, state: &mut dyn Any) {
    let hash = cx.location_hash().to_owned();
    let Some(s) = shell(cx.tree, id) else { return };
    if let (Some((route, _)), Some(root)) = (s.tabs.first(), s.stack().map(|st| st.root))
        && let Some(factory) = s.factory(route)
    {
        let content = factory(state, &Vec::new());
        cx.add_child(root, content);
    }
    // A deep link: the pages the URL hash names, built without animation (React's first effect).
    if let Some(s) = shell(cx.tree, id)
        && s.history_on
    {
        let initial = parse_hash(s, &hash);
        if !initial.is_empty() {
            s.history = vec![HistoryKind::Page; initial.len()];
            let op = HistoryOp::Replace { depth: initial.len() as u32, hash: Some(hash_for(initial.iter().map(String::as_str))) };
            cx.history(op);
            rebuild(cx, id, state, &initial);
        }
    }
    route_changed(cx, id, state);
}

/// Builds a page and puts it on top of the current tab, off screen from its first frame and then
/// sliding in from the right when `animated`. Returns its host and whether it slides.
fn push_page(
    cx: &mut Cx,
    id: ControlId,
    state: &mut dyn Any,
    name: &str,
    route: String,
    arguments: ShellArguments,
    animated: bool,
) -> Option<(ControlId, bool)> {
    let factory = shell(cx.tree, id)?.factory(name)?;
    let page = factory(state, &arguments);
    let distance = if animated { width(cx.tree, id) } else { 0.0 };
    let s = shell(cx.tree, id)?;
    let (tab, speed, insets) = (s.selected, s.p.pages_animation_speed, s.applied_insets.unwrap_or(Thickness::ZERO));
    let layer = s.stack()?.layer;
    let build = s.page_host(&s.title(name), page, insets).translation_x(distance);
    let host = cx.add_child(layer, build);
    let slide = (distance > 0.0).then(|| slide(cx, id, host, distance, 0.0, speed, Done::Pushed(host)));
    shell(cx.tree, id)?.stacks.get_mut(tab)?.as_mut()?.pages.push(Page { route, arguments, host, slide, popping: false });
    show_pages(cx.tree, id, tab);
    Some((host, slide.is_some()))
}

fn go_to(cx: &mut Cx, id: ControlId, state: &mut dyn Any, route: &str, animated: bool, arguments: ShellArguments) {
    let Some(s) = shell(cx.tree, id) else { return };
    let (name, mut merged) = split_route(route);
    if s.factory(name).is_none() {
        panic!("DrawnUi: route '{name}' is not registered in SkiaShell routes");
    }
    for (key, value) in arguments {
        set_argument(&mut merged, key, value);
    }
    let full = build_route(name, &merged);
    let previous = s.stack().and_then(|st| st.pages.last()).map(|p| p.host);
    if !navigating(cx, id, state, &full, NavigationSource::Push, None, previous) {
        return;
    }
    let Some((host, slides)) = push_page(cx, id, state, name, full.clone(), merged, animated) else { return };
    push_history(cx, id, HistoryKind::Page);
    route_changed(cx, id, state);
    if !slides {
        navigated(cx, id, state, &full, NavigationSource::Push, Some(host));
    }
}

fn go_back(cx: &mut Cx, id: ControlId, state: &mut dyn Any, animated: bool) {
    let Some(s) = shell(cx.tree, id) else { return };
    if let Some(top) = s.popups.last() {
        let wrapper = top.wrapper;
        return close_popup(cx, id, state, wrapper, animated);
    }
    if !s.modals.is_empty() {
        return pop_modal(cx, id, state, animated);
    }
    if !s.stack().is_some_and(|st| st.pages.iter().any(|p| !p.popping)) || back_through_history(cx, id, HistoryKind::Page) {
        return;
    }
    pop_page(cx, id, state, animated);
}

/// The page below page `i` of a stack: its route and host; "" and none for the root.
fn below(stack: &Stack, i: usize) -> (String, Option<ControlId>) {
    match i.checked_sub(1).and_then(|b| stack.pages.get(b)) {
        Some(p) => (p.route.clone(), Some(p.host)),
        None => (String::new(), None),
    }
}

/// Pops the current page; false when Navigating cancelled it (React popPage). Nothing to pop
/// counts as done.
fn pop_page(cx: &mut Cx, id: ControlId, state: &mut dyn Any, animated: bool) -> bool {
    let Some(s) = shell(cx.tree, id) else { return true };
    let (tab, speed) = (s.selected, s.p.pages_animation_speed);
    let Some(stack) = s.stack() else { return true };
    // A page already sliding out is on its way: the one below it goes next.
    let Some(i) = stack.pages.iter().rposition(|p| !p.popping) else { return true };
    let (host, (below_route, below_host)) = (stack.pages[i].host, below(stack, i));
    if !navigating(cx, id, state, &below_route, NavigationSource::Pop, Some(host), Some(host)) {
        return false;
    }
    let distance = if animated { width(cx.tree, id) } else { 0.0 };
    let from = cx.base(host).map_or(0.0, |b| b.p.translation_x);
    let Some(page) = shell(cx.tree, id).and_then(|s| s.stacks.get_mut(tab)?.as_mut()?.pages.iter_mut().find(|p| p.host == host)) else {
        return true;
    };
    // A page still sliding in turns back from where it is.
    let running = page.slide.take();
    if let Some(running) = running {
        cx.stop_animation(running);
    }
    if distance > 0.0 {
        let slide = slide(cx, id, host, from, distance, speed, Done::Popped(host));
        if let Some(page) = shell(cx.tree, id).and_then(|s| s.stacks.get_mut(tab)?.as_mut()?.pages.iter_mut().find(|p| p.host == host)) {
            (page.slide, page.popping) = (Some(slide), true);
        }
        show_pages(cx.tree, id, tab);
        return true;
    }
    if let Some(stack) = shell(cx.tree, id).and_then(|s| s.stacks.get_mut(tab)?.as_mut()) {
        stack.pages.retain(|p| p.host != host);
    }
    cx.remove(host);
    show_pages(cx.tree, id, tab);
    route_changed(cx, id, state);
    navigated(cx, id, state, &below_route, NavigationSource::Pop, below_host);
    true
}

fn pop_to_root(cx: &mut Cx, id: ControlId, state: &mut dyn Any, every_tab: bool) {
    let Some(s) = shell(cx.tree, id) else { return };
    if every_tab && s.history_on {
        // React PopToRootAsync: the page entries leave the history, the URL loses its hash.
        let pages = s.history.iter().filter(|k| **k == HistoryKind::Page).count();
        s.history.retain(|k| *k != HistoryKind::Page);
        let depth = s.history.len() as u32;
        if pages > 0 {
            cx.history(HistoryOp::Replace { depth, hash: Some(String::new()) });
        }
    }
    let Some(s) = shell(cx.tree, id) else { return };
    let tabs = if every_tab { 0..s.stacks.len() } else { s.selected..s.selected + 1 };
    for tab in tabs {
        clear_stack(cx, id, tab);
    }
    route_changed(cx, id, state);
}

/// Removes every pushed page of a tab at once.
fn clear_stack(cx: &mut Cx, id: ControlId, tab: usize) {
    let hosts: Vec<ControlId> = shell(cx.tree, id)
        .and_then(|s| s.stacks.get_mut(tab)?.as_mut())
        .map(|st| st.pages.drain(..).map(|p| p.host).collect())
        .unwrap_or_default();
    for host in hosts {
        cx.remove(host);
    }
    show_pages(cx.tree, id, tab);
}

/// The current tab's stack becomes `routes`, built without animation or events (React sets the
/// stack from the URL hash).
fn rebuild(cx: &mut Cx, id: ControlId, state: &mut dyn Any, routes: &[String]) {
    let Some(tab) = shell(cx.tree, id).map(|s| s.selected) else { return };
    clear_stack(cx, id, tab);
    for route in routes {
        let (name, arguments) = split_route(route);
        push_page(cx, id, state, name, route.clone(), arguments, false);
    }
    route_changed(cx, id, state);
}

// ---------------------------------------------------------------- browser history

/// What an entry of the browser's history stands for (React HistoryKind).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HistoryKind {
    Page,
    Popup,
    Modal,
}

/// The URL hash of a page stack (React hashFor): `#/a/b%3Fid%3D7`; "" for none, which leaves
/// the URL without a hash.
fn hash_for<'a>(routes: impl Iterator<Item = &'a str>) -> String {
    let mut hash = String::new();
    for route in routes {
        hash.push_str(if hash.is_empty() { "#/" } else { "/" });
        encode(&mut hash, route, false);
    }
    hash
}

/// The registered routes a URL hash names (React parseHash).
fn parse_hash(s: &SkiaShell, hash: &str) -> Vec<String> {
    let path = hash.strip_prefix('#').map_or(hash, |h| h.strip_prefix('/').unwrap_or(h));
    let routes = path.split('/').filter(|r| !r.is_empty()).map(|r| decode(r, false));
    routes.filter(|r| s.factory(split_route(r).0).is_some()).collect()
}

/// Records an entry in the browser's history (React pushHistory): a page with the hash of the
/// stack, a popup or a modal keeping the URL.
fn push_history(cx: &mut Cx, id: ControlId, kind: HistoryKind) {
    let Some(s) = shell(cx.tree, id).filter(|s| s.history_on) else { return };
    s.history.push(kind);
    let depth = s.history.len() as u32;
    let hash = (kind == HistoryKind::Page).then(|| hash_for(s.navigation_stack()));
    cx.history(HistoryOp::Push { depth, hash });
}

/// Something that has the last entry of the history closes through `history.back()`, so the URL
/// stays in step; the browser's answer (`on_history`) closes it (React backThroughHistory).
fn back_through_history(cx: &mut Cx, id: ControlId, kind: HistoryKind) -> bool {
    if !shell(cx.tree, id).is_some_and(|s| s.history_on && s.history.last() == Some(&kind)) {
        return false;
    }
    cx.history(HistoryOp::Back);
    true
}

/// The browser moved in its history (React onPop). Back: the entries above the one it is at are
/// unwound, top first, as GoBack would; one Navigating cancels gets its entry back. Forward, or
/// another hash at the same depth (a link, a typed hash): the stack is rebuilt from the hash
/// without animation; overlays are not restored.
fn history_moved(cx: &mut Cx, id: ControlId, state: &mut dyn Any, depth: u32, hash: &str) {
    let Some(s) = shell(cx.tree, id) else { return };
    let count = s.history.len() as u32;
    if depth < count {
        while let Some(kind) = shell(cx.tree, id).filter(|s| s.history.len() as u32 > depth).and_then(|s| s.history.pop()) {
            let done = match kind {
                HistoryKind::Popup => match shell(cx.tree, id).and_then(|s| s.popups.last_mut()) {
                    Some(top) => {
                        top.history = false;
                        let wrapper = top.wrapper;
                        let done = close_popup_now(cx, id, state, wrapper, true);
                        set_overlay_history(cx.tree, id, wrapper, !done);
                        done
                    }
                    None => true,
                },
                HistoryKind::Modal => match shell(cx.tree, id).and_then(|s| s.modals.last_mut()) {
                    Some(top) => {
                        top.history = false;
                        let wrapper = top.wrapper;
                        let done = pop_modal_now(cx, id, state, true);
                        set_overlay_history(cx.tree, id, wrapper, !done);
                        done
                    }
                    None => true,
                },
                HistoryKind::Page => pop_page(cx, id, state, true),
            };
            if !done {
                // Navigating cancelled it: the browser gets its entry back.
                if let Some(s) = shell(cx.tree, id) {
                    s.history.push(kind);
                    let depth = s.history.len() as u32;
                    let hash = (kind == HistoryKind::Page).then(|| hash_for(s.navigation_stack()));
                    cx.history(HistoryOp::Push { depth, hash });
                }
                break;
            }
        }
        return;
    }
    let target = parse_hash(s, hash);
    if depth == count && target.iter().map(String::as_str).eq(s.navigation_stack()) {
        return;
    }
    s.history = vec![HistoryKind::Page; target.len()];
    cx.history(HistoryOp::Replace { depth: target.len() as u32, hash: Some(hash_for(target.iter().map(String::as_str))) });
    rebuild(cx, id, state, &target);
}

fn set_overlay_history(tree: &mut Tree, id: ControlId, wrapper: ControlId, history: bool) {
    let Some(s) = shell(tree, id) else { return };
    if let Some(o) = s.popups.iter_mut().chain(s.modals.iter_mut()).find(|o| o.wrapper == wrapper) {
        o.history = history;
    }
}

// ---------------------------------------------------------------- tabs

fn select_tab(cx: &mut Cx, id: ControlId, state: &mut dyn Any, index: usize) {
    let Some(s) = shell(cx.tree, id) else { return };
    let (count, from) = (s.tabs.len(), s.selected);
    if count == 0 {
        return;
    }
    // A switch still running ends here.
    if let Some((animation, left)) = s.tab_switch.take() {
        cx.stop_animation(animation);
        end_tab_switch(cx.tree, id, left);
    }
    let to = index.min(count - 1);
    if to == from {
        return;
    }
    // A tab gets its layer and its root when it is first shown.
    let Some(s) = shell(cx.tree, id) else { return };
    let Some(pages) = s.parts.map(|p| p.pages) else { return };
    if s.stacks[to].is_none() {
        let insets = s.applied_insets.unwrap_or(Thickness::ZERO);
        let look = s.tab_look(insets);
        let stack = mount_stack(cx, pages, Vec::new(), false, look, insets);
        let root = stack.root;
        let Some(s) = shell(cx.tree, id) else { return };
        s.stacks[to] = Some(stack);
        if let Some(factory) = s.factory(&s.tabs[to].0.clone()) {
            let content = factory(state, &Vec::new());
            cx.add_child(root, content);
        }
    }
    let Some(s) = shell(cx.tree, id) else { return };
    s.selected = to;
    let (animate, ms) = (s.p.animate_tabs, s.p.tabs_animation_speed);
    let (Some(old), Some(new)) = (s.stacks[from].as_ref().map(|st| st.layer), s.stacks[to].as_ref().map(|st| st.layer)) else {
        return;
    };
    paint_tab_bar(cx.tree, id);
    set_visible(cx.tree, new, true);
    let distance = width(cx.tree, id);
    if !animate || distance <= 0.0 {
        set_visible(cx.tree, old, false);
    } else {
        // C# SelectRightTab / SelectLeftTab: the new root comes from 0.75 of the width, fading in,
        // over the old one, which goes out the other way.
        bring_to_front(cx.tree, pages, new);
        let dir = if to > from { 1.0 } else { -1.0 };
        let (enter, leave) = (dir * 0.75 * distance, -dir * distance);
        if let Some(mut c) = cx.any_mut(new) {
            c.set_translation_x(enter);
            c.set_opacity(0.001);
        }
        let animation = cx.animate(new, ms, easing::linear, move |p, cx| {
            let v = if p == 1.0 { 1.0 } else { tabs_easing(p) };
            if let Some(mut c) = cx.any_mut(new) {
                c.set_translation_x(lerp(enter, 0.0, v));
                c.set_opacity(lerp(0.001, 1.0, p));
            }
            if let Some(mut c) = cx.any_mut(old) {
                c.set_translation_x(lerp(0.0, leave, v));
            }
            if p == 1.0 {
                post(cx, id, Done::TabShown(from));
            }
        });
        if let Some(s) = shell(cx.tree, id) {
            s.tab_switch = Some((animation, from));
        }
    }
    route_changed(cx, id, state);
}

/// Moves a child to the end of its parent's children: it paints over its siblings and takes
/// gestures first, without a z-index (which would sort a copy of the children every frame).
fn bring_to_front(tree: &mut Tree, parent: ControlId, child: ControlId) {
    let Some(node) = tree.node_mut(parent) else { return };
    if let Some(i) = node.children.iter().position(|c| *c == child) {
        let id = node.children.remove(i);
        node.children.push(id);
        tree.invalidate(parent, crate::types::Dirty::DRAW);
    }
}

/// The tab that left an animated switch: hidden, and ready to come back untransformed.
fn end_tab_switch(tree: &mut Tree, id: ControlId, left: usize) {
    let Some(s) = shell(tree, id) else { return };
    if left == s.selected {
        return;
    }
    let Some(layer) = s.stacks.get(left).and_then(|st| st.as_ref()).map(|st| st.layer) else { return };
    if let Some(mut c) = tree.any_mut(layer) {
        c.set_is_visible(false);
        c.set_translation_x(0.0);
        c.set_opacity(1.0);
    }
}

/// The selected tab's caption bold and accented, its indicator shown.
fn paint_tab_bar(tree: &mut Tree, id: ControlId) {
    let Some(s) = shell(tree, id) else { return };
    let (selected, count) = (s.selected, s.tab_bar.len());
    for i in 0..count {
        let Some((button, indicator)) = shell(tree, id).map(|s| s.tab_bar[i]) else { return };
        let on = i == selected;
        if let Some(mut b) = tree.get_mut(button) {
            b.set_font_family(if on { "FontTextBold" } else { "" });
            b.set_text_color(if on { SkiaShell::TAB_SELECTED_COLOR } else { SkiaShell::TAB_COLOR });
        }
        set_visible(tree, indicator, on);
    }
}

// ---------------------------------------------------------------- popups

fn open_popup(cx: &mut Cx, id: ControlId, state: &mut dyn Any, content: Detached, o: PopupOptions) {
    let route = current_route(cx.tree, id);
    if !navigating(cx, id, state, &route, NavigationSource::Push, None, None) {
        return;
    }
    let Some(layer) = shell(cx.tree, id).and_then(|s| s.parts).map(|p| p.popups) else { return };
    let inner = SkiaLayer::new().center().scale(if o.animated { 0.5 } else { 1.0 }).children(content);
    let content = inner.id();
    let wrapper = PopupWrapper { layout: SkiaLayout::default(), content, outside: false };
    let mut wrapper = Build::new(wrapper).fill().block_gestures_below(true).opacity(if o.animated { 0.1 } else { 1.0 });
    if o.show_overlay {
        let dim = o.background_color.unwrap_or(SkiaShell::POPUP_BACKGROUND_COLOR);
        wrapper.push_child(backdrop(dim));
    }
    wrapper.push_child(inner);
    let close = o.close_when_background_tapped;
    wrapper.handlers.tapped = Some(Box::new(move |me, _, cx| {
        let me = me.typed::<PopupWrapper>();
        if close && me.outside {
            let wrapper = me.id();
            request(cx, id, Request::ClosePopup { wrapper: Some(wrapper), animated: o.animated });
        }
    }));
    let wrapper = cx.add_child(layer, wrapper);
    let animation = o.animated.then(|| {
        cx.animate(wrapper, SkiaShell::POPUPS_ANIMATION_SPEED, easing::linear, move |v, cx| {
            if let Some(mut c) = cx.any_mut(wrapper) {
                c.set_opacity(lerp(0.1, 1.0, v));
            }
            if let Some(mut c) = cx.any_mut(content) {
                c.set_scale_x(lerp(0.5, 1.0, v));
                c.set_scale_y(lerp(0.5, 1.0, v));
            }
            if v == 1.0 {
                post(cx, id, Done::PopupOpened(wrapper));
            }
        })
    });
    if let Some(s) = shell(cx.tree, id) {
        let history = s.history_on;
        s.popups.push(Overlay { wrapper, content, animation, closing: false, animated: o.animated, history });
    }
    push_history(cx, id, HistoryKind::Popup);
    if animation.is_none() {
        navigated(cx, id, state, &route, NavigationSource::Push, Some(content));
    }
}

/// Closes a popup; with an entry in the browser's history, through `history.back()` (React closePopup).
fn close_popup(cx: &mut Cx, id: ControlId, state: &mut dyn Any, wrapper: ControlId, animated: bool) {
    let Some(p) = shell(cx.tree, id).and_then(|s| s.popups.iter().find(|p| p.wrapper == wrapper)) else { return };
    if p.closing {
        return;
    }
    if p.history && back_through_history(cx, id, HistoryKind::Popup) {
        set_overlay_history(cx.tree, id, wrapper, false);
        return;
    }
    close_popup_now(cx, id, state, wrapper, animated);
}

/// Closes a popup at once; false when Navigating cancelled (React closePopupNow).
fn close_popup_now(cx: &mut Cx, id: ControlId, state: &mut dyn Any, wrapper: ControlId, animated: bool) -> bool {
    let Some(p) = shell(cx.tree, id).and_then(|s| s.popups.iter().find(|p| p.wrapper == wrapper)) else { return true };
    if p.closing {
        return true;
    }
    let (content, own) = (p.content, p.animated);
    let route = current_route(cx.tree, id);
    if !navigating(cx, id, state, &route, NavigationSource::Pop, Some(content), Some(content)) {
        return false;
    }
    let Some(p) = shell(cx.tree, id).and_then(|s| s.popups.iter_mut().find(|p| p.wrapper == wrapper)) else { return true };
    p.closing = true;
    let opening = p.animation.take();
    if let Some(opening) = opening {
        cx.stop_animation(opening);
    }
    if animated && own {
        let opacity = cx.base(wrapper).map_or(1.0, |b| b.p.opacity);
        let scale = cx.base(content).map_or(1.0, |b| b.p.scale_x);
        let animation = cx.animate(wrapper, SkiaShell::POPUPS_ANIMATION_SPEED, easing::linear, move |v, cx| {
            if let Some(mut c) = cx.any_mut(wrapper) {
                c.set_opacity(lerp(opacity, 0.0, v));
            }
            if let Some(mut c) = cx.any_mut(content) {
                c.set_scale_x(lerp(scale, 0.0, v));
                c.set_scale_y(lerp(scale, 0.0, v));
            }
            if v == 1.0 {
                post(cx, id, Done::PopupClosed(wrapper));
            }
        });
        if let Some(p) = shell(cx.tree, id).and_then(|s| s.popups.iter_mut().find(|p| p.wrapper == wrapper)) {
            p.animation = Some(animation);
        }
        return true;
    }
    remove_overlay(cx, id, wrapper);
    navigated(cx, id, state, &route, NavigationSource::Pop, None);
    true
}

/// The blurred, dimmed page behind a popup or a modal.
fn backdrop(color: Color) -> Build<SkiaBackdrop> {
    SkiaBackdrop::new().blur(SkiaShell::POPUPS_BACKGROUND_BLUR).background_color(color).input_transparent(true)
}

/// Removes a popup, modal or toast and forgets it.
fn remove_overlay(cx: &mut Cx, id: ControlId, wrapper: ControlId) {
    if let Some(s) = shell(cx.tree, id) {
        for list in [&mut s.popups, &mut s.modals, &mut s.toasts] {
            list.retain(|o| o.wrapper != wrapper);
        }
    }
    cx.remove(wrapper);
}

// ---------------------------------------------------------------- modals

fn push_modal(cx: &mut Cx, id: ControlId, state: &mut dyn Any, content: Detached, o: ModalOptions) {
    let route = current_route(cx.tree, id);
    if !navigating(cx, id, state, &route, NavigationSource::Push, None, None) {
        return;
    }
    let Some(layer) = shell(cx.tree, id).and_then(|s| s.parts).map(|p| p.modals) else { return };
    // React: a full-screen SkiaDrawer from the bottom, closed until it was laid out, then opened.
    let mut drawer = SkiaDrawer::new()
        .direction(DrawerDirection::FromBottom)
        .header_size(0)
        .fill()
        .responds_to_gestures(o.use_gestures)
        .animated(o.animated)
        .bounces(false)
        .block_gestures_below(true)
        .children(content);
    let dim = o.freeze_background.then(|| backdrop(SkiaShell::POPUP_BACKGROUND_COLOR));
    let wrapper = SkiaLayer::new().fill().block_gestures_below(true);
    let (wrapper_id, drawer_id) = (wrapper.id(), drawer.id());
    let d = drawer.control_mut();
    d.on_state_transition_complete = Some(Box::new(move |_, _, cx, open| {
        post(cx, id, if open { Done::ModalOpened(wrapper_id) } else { Done::ModalClosed(wrapper_id) })
    }));
    if !o.animated {
        d.on_is_open_changed = Some(Box::new(move |_, _, cx, open| {
            if !open {
                post(cx, id, Done::ModalClosed(wrapper_id));
            }
        }));
    }
    let wrapper = cx.add_child(layer, wrapper.children((dim, drawer)));
    // Opened on the next frame, once it knows its travel; until Navigated it stands for "opening".
    let animated = o.animated;
    let opener = cx.start_animator(drawer_id, ValueAnimator::new(0.0, 1.0, 0.0, easing::linear), move |_, cx| {
        if let Some(mut drawer) = cx.tree.find_mut::<SkiaDrawer>(drawer_id) {
            drawer.set_is_open(true);
        }
        if !animated {
            post(cx, id, Done::ModalOpened(wrapper));
        }
    });
    if let Some(s) = shell(cx.tree, id) {
        let history = s.history_on;
        s.modals.push(Overlay { wrapper, content: drawer_id, animation: Some(opener), closing: false, animated, history });
    }
    push_history(cx, id, HistoryKind::Modal);
}

/// Closes the top modal; with an entry in the browser's history, through `history.back()` (React
/// PopModalAsync).
fn pop_modal(cx: &mut Cx, id: ControlId, state: &mut dyn Any, animated: bool) {
    let Some(top) = shell(cx.tree, id).and_then(|s| s.modals.last()) else { return };
    if top.closing {
        return;
    }
    let wrapper = top.wrapper;
    if top.history && back_through_history(cx, id, HistoryKind::Modal) {
        set_overlay_history(cx.tree, id, wrapper, false);
        return;
    }
    pop_modal_now(cx, id, state, animated);
}

/// Closes the top modal: its drawer slides down, then it goes; false when Navigating cancelled
/// (React popModalNow).
fn pop_modal_now(cx: &mut Cx, id: ControlId, state: &mut dyn Any, animated: bool) -> bool {
    let Some(top) = shell(cx.tree, id).and_then(|s| s.modals.last()) else { return true };
    if top.closing {
        return true;
    }
    let (wrapper, drawer) = (top.wrapper, top.content);
    let route = current_route(cx.tree, id);
    if !navigating(cx, id, state, &route, NavigationSource::Pop, Some(drawer), Some(drawer)) {
        return false;
    }
    let Some(m) = shell(cx.tree, id).and_then(|s| s.modals.iter_mut().find(|m| m.wrapper == wrapper)) else { return true };
    m.closing = true;
    let opener = m.animation.take();
    if let Some(opener) = opener {
        cx.stop_animation(opener);
    }
    if let Some(mut d) = cx.tree.find_mut::<SkiaDrawer>(drawer)
        && d.is_open()
        && animated
    {
        // It rests closed: `on_state_transition_complete` ends the pop.
        d.set_animated(true);
        d.set_is_open(false);
        return true;
    }
    remove_overlay(cx, id, wrapper);
    navigated(cx, id, state, &route, NavigationSource::Pop, None);
    true
}

/// The drawer of a modal came to rest closed: the end of a pop, or the user dragged it down
/// (React userClosed: through the history when it has an entry, else it just goes, no events).
fn modal_closed(cx: &mut Cx, id: ControlId, state: &mut dyn Any, wrapper: ControlId) {
    let Some(m) = shell(cx.tree, id).and_then(|s| s.modals.iter_mut().find(|m| m.wrapper == wrapper)) else { return };
    if m.closing {
        remove_overlay(cx, id, wrapper);
        let route = current_route(cx.tree, id);
        navigated(cx, id, state, &route, NavigationSource::Pop, None);
        return;
    }
    if m.history && back_through_history(cx, id, HistoryKind::Modal) {
        set_overlay_history(cx.tree, id, wrapper, false);
        return;
    }
    remove_overlay(cx, id, wrapper);
}

// ---------------------------------------------------------------- toasts

fn show_toast(cx: &mut Cx, id: ControlId, content: Detached, ms: f32) {
    close_all_toasts(cx, id);
    let Some(layer) = shell(cx.tree, id).and_then(|s| s.parts).map(|p| p.toasts) else { return };
    let toast = SkiaLayer::new()
        .vertical_options(LayoutOptions::End)
        .background_color(SkiaShell::TOAST_BACKGROUND_COLOR)
        .block_gestures_below(true)
        .use_cache(CacheType::Operations)
        .opacity(0)
        .children(SkiaLayer::new().children(content));
    let toast = cx.add_child(layer, toast);
    // Up from its own height and in, once laid out: the first tick comes after the frame that measured it.
    let animation = cx.animate(toast, TOAST_IN_MS, easing::linear, move |v, cx| {
        let height = cx.base(toast).map_or(0.0, |b| b.rect.height() / b.scale.max(0.01));
        if let Some(mut c) = cx.any_mut(toast) {
            c.set_translation_y(lerp(height, 0.0, v));
            c.set_opacity(v);
        }
    });
    // The timer: no frames while it waits.
    cx.start_animator(toast, ValueAnimator::new(0.0, 1.0, 0.0, easing::linear).delay(ms), move |_, cx| {
        post(cx, id, Done::ToastDue(toast))
    });
    if let Some(s) = shell(cx.tree, id) {
        s.toasts.push(Overlay { wrapper: toast, content: toast, animation: Some(animation), closing: false, animated: true, history: false });
    }
}

fn close_toast(cx: &mut Cx, id: ControlId, toast: ControlId) {
    let Some(t) = shell(cx.tree, id).and_then(|s| s.toasts.iter_mut().find(|t| t.wrapper == toast)) else { return };
    if std::mem::replace(&mut t.closing, true) {
        return;
    }
    let showing = t.animation.take();
    if let Some(showing) = showing {
        cx.stop_animation(showing);
    }
    let (from_y, from_opacity) = cx.base(toast).map_or((0.0, 1.0), |b| (b.p.translation_y, b.p.opacity));
    cx.animate(toast, TOAST_OUT_MS, easing::linear, move |v, cx| {
        let height = cx.base(toast).map_or(0.0, |b| b.rect.height() / b.scale.max(0.01));
        if let Some(mut c) = cx.any_mut(toast) {
            c.set_translation_y(lerp(from_y, height, v));
            c.set_opacity(lerp(from_opacity, 0.0, v));
        }
        if v == 1.0 {
            post(cx, id, Done::ToastClosed(toast));
        }
    });
}

fn close_all_toasts(cx: &mut Cx, id: ControlId) {
    let toasts: Vec<ControlId> = shell(cx.tree, id).map(|s| s.toasts.drain(..).map(|t| t.wrapper).collect()).unwrap_or_default();
    for toast in toasts {
        cx.remove(toast);
    }
}

// ---------------------------------------------------------------- ends of animations

fn finish(cx: &mut Cx, id: ControlId, state: &mut dyn Any, done: Done) {
    match done {
        Done::Pushed(host) => {
            let Some(s) = shell(cx.tree, id) else { return };
            let found = s.stacks.iter_mut().enumerate().find_map(|(tab, st)| {
                let page = st.as_mut()?.pages.iter_mut().find(|p| p.host == host && !p.popping)?;
                page.slide = None;
                Some((tab, page.route.clone()))
            });
            if let Some((tab, route)) = found {
                show_pages(cx.tree, id, tab);
                navigated(cx, id, state, &route, NavigationSource::Push, Some(host));
            }
        }
        Done::Popped(host) => {
            let Some(s) = shell(cx.tree, id) else { return };
            let found = s.stacks.iter_mut().enumerate().find_map(|(tab, st)| {
                let st = st.as_mut()?;
                let i = st.pages.iter().position(|p| p.host == host)?;
                let (route, view) = below(st, i);
                st.pages.remove(i);
                Some((tab, route, view))
            });
            cx.remove(host);
            if let Some((tab, route, view)) = found {
                show_pages(cx.tree, id, tab);
                route_changed(cx, id, state);
                navigated(cx, id, state, &route, NavigationSource::Pop, view);
            }
        }
        Done::TabShown(left) => {
            if let Some(s) = shell(cx.tree, id) {
                s.tab_switch = None;
            }
            end_tab_switch(cx.tree, id, left);
        }
        Done::PopupOpened(wrapper) | Done::ModalOpened(wrapper) => {
            let Some(s) = shell(cx.tree, id) else { return };
            let Some(o) = s.popups.iter_mut().chain(s.modals.iter_mut()).find(|o| o.wrapper == wrapper) else { return };
            // Once: a modal dragged part way and snapping back open rests open again.
            if o.animation.take().is_none() {
                return;
            }
            let content = o.content;
            let route = current_route(cx.tree, id);
            navigated(cx, id, state, &route, NavigationSource::Push, Some(content));
        }
        Done::PopupClosed(wrapper) => {
            remove_overlay(cx, id, wrapper);
            let route = current_route(cx.tree, id);
            navigated(cx, id, state, &route, NavigationSource::Pop, None);
        }
        Done::ModalClosed(wrapper) => modal_closed(cx, id, state, wrapper),
        Done::ToastDue(toast) => close_toast(cx, id, toast),
        Done::ToastClosed(toast) => remove_overlay(cx, id, toast),
    }
}
