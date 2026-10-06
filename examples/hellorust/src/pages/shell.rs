//! SkiaShell: pages with slide transitions, popups, modals, toasts, tabs. Ported from the React
//! demo's ShellPage.tsx; the page drives the app's own shell, and a nested one for the tabs.

use drawnui::prelude::*;

use super::{card, card_title, scrolling};
use crate::{App, hex};

const BODY: Color = hex(0xADB5BD);

/// What the page keeps.
#[derive(Default)]
pub struct State {
    /// The nested tabbed shell.
    tabs: Handle<SkiaShell>,
    /// Where the nested shell is: tab, stack, arguments of the page on top.
    tabs_info: String,
    /// The last four events of the nested shell, newest first.
    pub(super) events: Vec<String>,
    /// The next Navigating of the nested shell is cancelled.
    cancel_next: bool,
    /// Said after the status line once a popup or a modal opened.
    pub(super) log: &'static str,
    /// Said once the popup or modal just asked for is up (React awaits OpenPopupAsync).
    waiting: Option<&'static str>,
}

/// The status of a shell as the React page prints it.
pub fn describe(shell: &SkiaShell) -> String {
    let route = if shell.route().is_empty() { "\"\"" } else { shell.route() };
    let stack = shell.navigation_stack().collect::<Vec<_>>().join(", ");
    format!(
        "Route={route} · NavigationStack=[{stack}] · Popups={} · Modals={} · Toasts={}",
        shell.popups_count(),
        shell.modals_count(),
        shell.toasts_count()
    )
}

/// The app's shell brought something up front: the popup or modal a log line waited for (the
/// next push after the button asked for it).
pub fn navigated(app: &mut App, e: &ShellNavigatedArgs) {
    let page = &mut app.shell_page;
    if e.source == NavigationSource::Push
        && let Some(line) = page.waiting.take()
    {
        page.log = line;
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    let page = &mut app.shell_page;
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        SkiaLabel::new("SkiaShell").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new("")
            .font_size(12)
            .text_color(BODY)
            .fill_x()
            .observe(|me, app: &App| me.set_text(format!("{} {}", app.shell_info, app.shell_page.log))),
        SkiaLabel::new("Pages live in the URL hash (#/shell/shapes); the browser Back button closes the top popup, then the top modal, then pops the page — the C# GoBack order.")
            .font_size(12)
            .text_color(BODY)
            .fill_x(),
        card(
            card_title("Pages — GoToAsync slides the page in from the right (PagesAnimationSpeed 200 ms), GoBackAsync slides it out"),
            SkiaWrap::new().spacing(8).children((
                button("GoToAsync('shapes')", 0x0D6EFD, |app, cx| cx.go_to(app.shell, "shapes", true)),
                button("GoToAsync('shell') again", 0x0D6EFD, |app, cx| cx.go_to(app.shell, "shell", true)),
                button("GoBackAsync()", 0x495057, |app, cx| cx.go_back(app.shell, true)),
                button("PopToRootAsync()", 0x495057, |app, cx| cx.pop_to_root(app.shell)),
            )),
        ),
        card(
            card_title("Tabs — a nested SkiaShell with Tabs: per-tab navigation stacks, AnimateTabs (C# SkiaViewSwitcher slide + fade), PopTabToRootAsync"),
            (
                SkiaLayer::new().height_request(300).fill_x().is_clipped_to_bounds(true).children(tabs(&mut page.tabs)),
                SkiaWrap::new().spacing(8).children(
                    SkiaButton::new("")
                        .font_size(12)
                        .observe(|me, app: &App| {
                            let cancel = app.shell_page.cancel_next;
                            me.set_text(if cancel { "Next Navigating will be CANCELLED" } else { "Cancel next navigation" });
                            me.set_background_color(hex(if cancel { 0xD63384 } else { 0x495057 }));
                        })
                        .on_tapped(|_me, app: &mut App, _cx| app.shell_page.cancel_next = !app.shell_page.cancel_next),
                ),
                SkiaLabel::new("").font_size(12).text_color(BODY).fill_x().observe(|me, app: &App| {
                    let events = &app.shell_page.events;
                    if events.is_empty() {
                        me.set_text("Navigating / Navigated / RouteChanged events of the nested shell appear here");
                    } else {
                        me.set_text(events.join("\n"));
                    }
                }),
            ),
        ),
        card(card_title("SkiaBackdrop — the Sandbox MainPageBackdrop frosted glass, same tree"), backdrop()),
        card(
            card_title("Popups — OpenPopupAsync(content, options)"),
            SkiaWrap::new().spacing(8).children((
                button("Open popup", 0x6610F2, |app, cx| {
                    app.shell_page.waiting = Some("· popup opened");
                    cx.open_popup(app.shell, popup("Hello popup"), PopupOptions::default());
                }),
                button("Not closable outside", 0x6610F2, |app, cx| {
                    let options = PopupOptions { close_when_background_tapped: false, ..PopupOptions::default() };
                    cx.open_popup(app.shell, popup("closeWhenBackgroundTapped=false"), options);
                }),
                button("No overlay, not animated", 0x6610F2, |app, cx| {
                    let options = PopupOptions { show_overlay: false, animated: false, ..PopupOptions::default() };
                    cx.open_popup(app.shell, popup("showOverlay=false"), options);
                }),
                button("Red overlay", 0x6610F2, |app, cx| {
                    let options = PopupOptions { background_color: Some(Color::new(0x66FF0000)), ..PopupOptions::default() };
                    cx.open_popup(app.shell, popup("backgroundColor"), options);
                }),
                button("CloseAllPopups()", 0x495057, |app, cx| cx.close_all_popups(app.shell)),
            )),
        ),
        card(
            card_title("Modals — PushModalAsync(content, { useGestures, animated })"),
            SkiaWrap::new().spacing(8).children((
                green("Push modal", |app, cx| {
                    app.shell_page.waiting = Some("· modal opened");
                    cx.push_modal(app.shell, modal(), ModalOptions::default());
                }),
                green("Draggable (useGestures)", |app, cx| {
                    cx.push_modal(app.shell, modal(), ModalOptions { use_gestures: true, ..ModalOptions::default() });
                }),
                green("Not animated", |app, cx| {
                    cx.push_modal(app.shell, modal(), ModalOptions { animated: false, ..ModalOptions::default() });
                }),
            )),
        ),
        card(
            card_title("Toasts — ShowToast(text | content, msShowTime)"),
            SkiaWrap::new().spacing(8).children((
                orange("ShowToast('Saved!')", |app, cx| cx.show_toast(app.shell, "**Saved!** The toast slides up, stays 4 s, slides down.", 4000)),
                orange("Short (1.5 s)", |app, cx| cx.show_toast(app.shell, "Gone in 1.5 seconds", 1500)),
                orange("Custom content", |app, cx| {
                    let content = SkiaStack::new().spacing(4).padding((24, 16)).children((
                        SkiaLabel::new("Custom toast").font_size(16).font_family("FontTextBold").text_color(Color::WHITE),
                        SkiaLabel::new("Any drawn tree works as toast content.").font_size(13).text_color(BODY),
                    ));
                    cx.show_toast_content(app.shell, content, 3000);
                }),
                button("CloseAllToasts()", 0x495057, |app, cx| cx.close_all_toasts(app.shell)),
            )),
        ),
    )))
}

fn button(caption: &str, color: u32, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption).background_color(hex(color)).on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

fn green(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    button(caption, 0x20C997, action).text_color(hex(0x1A1A2E))
}

fn orange(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    button(caption, 0xFD7E14, action).text_color(hex(0x1A1A2E))
}

/// Content of the demo popup: a card with its own close button.
fn popup(title: &str) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(16)
        .background_color(hex(0xF5F5F5))
        .width_request(300)
        .shadows(SkiaShadow::new(Color::BLACK).x(0).y(6).blur(12).opacity(0.5))
        .children(SkiaStack::new().spacing(12).padding(20).children((
            SkiaLabel::new(title).font_size(20).font_family("FontTextBold").text_color(hex(0x111827)),
            SkiaLabel::new("OpenPopupAsync centers the content over a dimmed backdrop, scales it in from 0.5 and fades the layer (PopupsAnimationSpeed 250 ms). A tap outside closes it when closeWhenBackgroundTapped.")
                .font_size(13)
                .text_color(hex(0x374151))
                .fill_x(),
            SkiaButton::new("Close")
                .control_style(PrebuiltControlStyle::Material)
                .horizontal_options(LayoutOptions::End)
                .on_tapped(|_me, app: &mut App, cx| cx.close_popup(app.shell, true)),
        )))
}

fn modal() -> Build<SkiaShape> {
    SkiaShape::new().background_color(hex(0x212529)).fill().children(
        SkiaStack::new().spacing(16).padding((24, 40)).horizontal_options(LayoutOptions::Center).maximum_width_request(520).children((
            SkiaLabel::new("Modal page").font_size(28).font_family("FontTextBold").text_color(Color::WHITE),
            SkiaLabel::new("PushModalAsync slides the content up from the bottom over a dimmed page; with useGestures it can be dragged down to close once SkiaDrawer is in. PopModalAsync closes it.")
                .font_size(14)
                .text_color(BODY)
                .fill_x(),
            SkiaWrap::new().spacing(8).children((
                button("PopModalAsync()", 0x0D6EFD, |app, cx| cx.pop_modal(app.shell, true)),
                button("Popup over the modal", 0x6610F2, |app, cx| {
                    cx.open_popup(app.shell, popup("Popup over a modal"), PopupOptions::default())
                }),
                button("Toast", 0x495057, |app, cx| cx.show_toast(app.shell, "Toast shown above the modal (ZIndexToasts)", 4000)),
            )),
        )),
    )
}

/// The nested tabbed shell: three tabs, each with its own stack, and a detail page to push.
fn tabs(handle: &mut Handle<SkiaShell>) -> Build<SkiaShell> {
    SkiaShell::new()
        .assign(handle)
        .route("home", |_app: &mut App, _: &ShellArguments| tab_page("Home".to_owned(), 0x0F3460))
        .route("search", |_app: &mut App, _: &ShellArguments| tab_page("Search".to_owned(), 0x533483))
        .route("profile", |_app: &mut App, _: &ShellArguments| tab_page("Profile".to_owned(), 0x1B4332))
        .route("detail", |_app: &mut App, arguments: &ShellArguments| {
            let name = match arguments.iter().find(|(key, _)| key == "id") {
                Some((_, id)) => format!("Detail id={id}"),
                None => "Detail page (pushed inside this tab)".to_owned(),
            };
            tab_page(name, 0x2B3035)
        })
        .titles([("detail", "Detail")])
        .tabs([("home", "Home"), ("search", "Search"), ("profile", "Profile")])
        .use_browser_history(false)
        .nav_bar_height(44)
        .animate_tabs(true)
        .insets(Some(Thickness::ZERO))
        .on_navigating(|_me, app: &mut App, _cx, e| {
            let page = &mut app.shell_page;
            if page.cancel_next {
                e.cancel = true;
                page.cancel_next = false;
            }
            let cancelled = if e.cancel { " CANCELLED" } else { "" };
            log(page, format!("Navigating {:?} '{}'{cancelled}", e.source, e.route));
        })
        .on_navigated(|_me, app: &mut App, _cx, e| {
            let view = if e.view.is_some() { "SkiaLayout" } else { "-" };
            log(&mut app.shell_page, format!("Navigated {:?} '{}' view={view}", e.source, e.route));
        })
        .on_route_changed(|_me, app: &mut App, _cx, route| log(&mut app.shell_page, format!("RouteChanged '{route}'")))
        .on_changed(|me, app: &mut App, _cx| {
            let shell = me.control_mut();
            let stack = shell.navigation_stack().collect::<Vec<_>>().join(", ");
            let arguments = shell.arguments().iter().map(|(key, value)| format!("\"{key}\":\"{value}\"")).collect::<Vec<_>>();
            let arguments = if arguments.is_empty() { String::new() } else { format!(" · Arguments {{{}}}", arguments.join(",")) };
            app.shell_page.tabs_info = format!("Tab {} · stack [{stack}]{arguments}", shell.selected_tab());
        })
}

/// Keeps the four newest events.
fn log(page: &mut State, event: String) {
    page.events.insert(0, event);
    page.events.truncate(4);
}

/// A page inside the nested shell: shows its tab stack and pushes deeper pages.
fn tab_page(name: String, color: u32) -> Build<SkiaLayout> {
    let from = name.clone();
    SkiaLayer::new().fill().background_color(hex(color)).children(
        SkiaStack::new()
            .spacing(10)
            .padding(16)
            .horizontal_options(LayoutOptions::Center)
            .vertical_options(LayoutOptions::Center)
            .children((
                SkiaLabel::new(name).font_size(22).font_family("FontTextBold").text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
                SkiaLabel::new("")
                    .font_size(12)
                    .text_color(hex(0xDEE2E6))
                    .horizontal_options(LayoutOptions::Center)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .observe(|me, app: &App| me.set_text(app.shell_page.tabs_info.as_str())),
                SkiaWrap::new().spacing(8).horizontal_options(LayoutOptions::Center).children((
                    tab_button("Push detail", |app, cx| cx.go_to(app.shell_page.tabs, "detail", true)),
                    SkiaButton::new("GoToAsync('detail', true, { id: 42, from })").background_color(hex(0x212529)).font_size(13).on_tapped(
                        move |_me, app: &mut App, cx| {
                            let arguments = vec![("id".to_owned(), "42".to_owned()), ("from".to_owned(), from.clone())];
                            cx.go_to_with(app.shell_page.tabs, "detail", true, arguments);
                        },
                    ),
                    tab_button("GoToAsync('detail?id=7')", |app, cx| cx.go_to(app.shell_page.tabs, "detail?id=7", true)),
                )),
            )),
    )
}

fn tab_button(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption).background_color(hex(0x212529)).font_size(13).on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

/// The Sandbox MainPageBackdrop: a photo, and over it a frosted glass card with a caption.
fn backdrop() -> Build<SkiaLayout> {
    // The backdrop copies what the Image cache of this layer holds under it.
    SkiaLayer::new()
        .height_request(260)
        .fill_x()
        .is_clipped_to_bounds(true)
        .use_cache(CacheType::Image)
        .background_color(hex(0xF5F5F5))
        .padding(24)
        .children((
            SkiaImage::new("assets/images/baboon.jpg").aspect(TransformAspect::AspectCover).background_color(hex(0x008000)).fill(),
            SkiaLayer::new().width_request(200).height_request(200).center().children((
                // Static shadow and texture, cached.
                SkiaLayer::new().padding(16).fill().use_cache(CacheType::Image).z_index(-1).children(
                    SkiaShape::new()
                        .background_color(Color::new(0x22DDDDDD))
                        .corner_radius(16)
                        .stroke_color(Color::RED)
                        .stroke_width(2)
                        .fill()
                        .stroke_gradient(
                            SkiaGradient::new(GradientType::Linear, [Color::new(0x66FFFFFF), Color::new(0x66999999)])
                                .start_x_ratio(0)
                                .start_y_ratio(0)
                                .end_x_ratio(1)
                                .end_y_ratio(1),
                        )
                        .children(SkiaImage::new("assets/images/glass2.jpg").aspect(TransformAspect::AspectCover).opacity(0.15).fill()),
                ),
                // The frosted glass.
                SkiaShape::new()
                    .margin(16)
                    .background_color(Color::new(0x66FFFFFF))
                    .clip_background_color(true)
                    .corner_radius(19)
                    .fill()
                    .shadows(SkiaShadow::new(Color::new(0x44000000)).x(4).y(4).blur(3).opacity(1))
                    .children(SkiaLayer::new().children((
                        SkiaBackdrop::new().blur(3).z_index(-1),
                        SkiaLayer::new().padding(8).fill().children(
                            SkiaLabel::new("Wonnabe Frosted Glass")
                                .font_size(20)
                                .text_color(hex(0xEFEFEF))
                                .center()
                                .horizontal_text_alignment(TextAlignment::Center),
                        ),
                    ))),
            )),
        ))
}
