//! Browser host on wasm32-unknown-emscripten. `web/drawnui_host.js` owns the canvas, its WebGL2 (or 2D)
//! context, requestAnimationFrame and DOM events, and calls the exports below.

use std::{cell::Cell, ffi::CString, os::raw::c_char};

use std::sync::Arc;

use crate::{
    App, ContextMenuSource, Cursor, Decoded, Frame, Frames, GesturesMode, Gpu, HistoryOp, Host, KeyKind, Modifiers, MouseButton,
    PointerKind, RenderingModeType, Thickness, key_name, skia,
};

thread_local! {
    static MAKE: Cell<Option<fn() -> Box<dyn App>>> = const { Cell::new(None) };
}

pub fn run(_title: &str, make: fn() -> Box<dyn App>) {
    MAKE.set(Some(make));
}

pub struct WebState {
    app: Box<dyn App>,
    gpu: Gpu,
    surface: skia::Surface,
    host: Host,
    width: i32,
    height: i32,
    scale: f32,
    request: Option<CString>,
    /// Pixel buffers JS is filling: `dui_alloc_pixels` to `dui_image`.
    pixels: Vec<skia::Data>,
    /// The message `dui_poll_output` handed out last; valid until the next call.
    output: Option<CString>,
    /// Drawn on the CPU (`RenderingModeType::Default`, or no WebGL2): the frame's pixels for the
    /// page's 2D context, RGBA unpremultiplied (`dui_pixels`).
    cpu: Option<Vec<u8>>,
}

/// The canvas' surface: the WebGL2 framebuffer, or CPU memory.
fn surface(gpu: &mut Gpu, cpu: bool, width: i32, height: i32) -> skia::Surface {
    let surface = if cpu { skia::surfaces::raster_n32_premul((width, height)) } else { gpu.window_surface(width, height, 0, 8) };
    surface.expect("no surface for the canvas")
}

/// Makes the app; nothing is drawn before `dui_attach`. JS asks `dui_accelerated` first and
/// makes the canvas' context to match.
#[unsafe(no_mangle)]
pub extern "C" fn dui_create() -> *mut WebState {
    let make = MAKE.get().expect("drawnui::run was not called from main");
    // The page renders the accessibility overlay (drawnui_host.js), as React's Canvas does.
    let mut host = Host { accessibility_on: true, history_on: true, ..Host::default() };
    let mut app = make();
    app.init(&mut host);
    let mut gpu = Gpu::raster();
    let surface = surface(&mut gpu, true, 1, 1);
    let (request, pixels, output, cpu) = (None, Vec::new(), None, Some(Vec::new()));
    Box::into_raw(Box::new(WebState { app, gpu, surface, host, width: 1, height: 1, scale: 1.0, request, pixels, output, cpu }))
}

/// 1 when the canvas owns every touch (`GesturesMode::Lock`): the page guards it.
#[unsafe(no_mangle)]
pub extern "C" fn dui_gestures_lock(s: *mut WebState) -> i32 {
    (state(s).app.gestures() == GesturesMode::Lock) as i32
}

/// 1 when the app draws on the GPU (`RenderingModeType::Accelerated`, the default).
#[unsafe(no_mangle)]
pub extern "C" fn dui_accelerated(s: *mut WebState) -> i32 {
    (state(s).app.rendering_mode() == RenderingModeType::Accelerated) as i32
}

/// The canvas is ready: `gl` 1 when JS made a WebGL2 context current, 0 for a 2D context (the
/// frames are drawn on the CPU and read with `dui_pixels`). Returns 1 when drawing on the GPU.
#[unsafe(no_mangle)]
pub extern "C" fn dui_attach(s: *mut WebState, width: i32, height: i32, scale: f32, gl: i32) -> i32 {
    let s = state(s);
    let gpu = if gl != 0 { skia::gpu::gl::Interface::new_native().and_then(Gpu::new_gl) } else { None };
    s.cpu = gpu.is_none().then(Vec::new);
    s.gpu = gpu.unwrap_or_else(Gpu::raster);
    (s.width, s.height, s.scale) = (width, height, scale);
    s.surface = surface(&mut s.gpu, s.cpu.is_some(), width, height);
    s.cpu.is_none() as i32
}

/// `dui_create` + `dui_attach` on a WebGL2 context JS made current (pages before the rendering mode).
#[unsafe(no_mangle)]
pub extern "C" fn dui_init(width: i32, height: i32, scale: f32) -> *mut WebState {
    let s = dui_create();
    dui_attach(s, width, height, scale, 1);
    s
}

/// Drawing on the CPU: the last frame's pixels, width x height RGBA unpremultiplied (an
/// ImageData's layout); valid until the next frame or resize. Null on the GPU.
#[unsafe(no_mangle)]
pub extern "C" fn dui_pixels(s: *mut WebState) -> *const u8 {
    state(s).cpu.as_ref().map_or(std::ptr::null(), |p| p.as_ptr())
}

fn state<'a>(state: *mut WebState) -> &'a mut WebState {
    unsafe { state.as_mut() }.expect("invalid state pointer")
}

/// Draws a frame. Returns 1 when another frame is needed.
#[unsafe(no_mangle)]
pub extern "C" fn dui_frame(s: *mut WebState, time_ms: f64) -> i32 {
    let s = state(s);
    let mut frame = Frame {
        surface: &mut s.surface,
        gpu: &mut s.gpu,
        width: s.width as f32,
        height: s.height as f32,
        scale: s.scale,
        time_ms,
        host: &mut s.host,
    };
    let more = s.app.frame(&mut frame);
    s.gpu.end_frame(&mut s.surface);
    // The 2D context takes unpremultiplied RGBA (what CanvasKit's software surface hands it too).
    if let Some(pixels) = &mut s.cpu {
        let (w, h) = (s.width.max(1), s.height.max(1));
        pixels.resize(w as usize * h as usize * 4, 0);
        let info = skia::ImageInfo::new((w, h), skia::ColorType::RGBA8888, skia::AlphaType::Unpremul, None);
        s.surface.read_pixels(&info, pixels, w as usize * 4, (0, 0));
    }
    more as i32
}

/// The page's WebGL context came back after it was lost (`webglcontextrestored`; JS made it
/// current again): a new Skia context on it, a surface follows with `dui_resize`. The next frame
/// drops what lived on the old one (`Ui`, DrawnUI GraphicContextMismatch).
#[unsafe(no_mangle)]
pub extern "C" fn dui_gpu_restored(s: *mut WebState) {
    let s = state(s);
    let interface = skia::gpu::gl::Interface::new_native().expect("no WebGL interface");
    s.gpu = Gpu::new_gl_after(interface, &mut s.gpu).expect("no GPU context");
}

/// After a frame that asked for no other: the frame time the app wants one at (a timer), or -1.
#[unsafe(no_mangle)]
pub extern "C" fn dui_wake(s: *mut WebState) -> f64 {
    state(s).host.wake_ms.take().unwrap_or(-1.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn dui_resize(s: *mut WebState, width: i32, height: i32, scale: f32) {
    let s = state(s);
    (s.width, s.height, s.scale) = (width, height, scale);
    s.surface = surface(&mut s.gpu, s.cpu.is_some(), width, height);
}

/// kind: 0 down, 1 move, 2 up, 3 cancel, 4 hover (no button down), 5 leave. button: the DOM
/// `PointerEvent.button` (0 left, 1 middle, 2 right, 3 back, 4 forward). Coordinates in pixels;
/// `time_ms` is the DOM event time stamp, the clock requestAnimationFrame uses.
#[unsafe(no_mangle)]
pub extern "C" fn dui_pointer(s: *mut WebState, kind: i32, button: i32, x: f32, y: f32, time_ms: f64) {
    let kind = match kind {
        0 => PointerKind::Down,
        1 => PointerKind::Move,
        2 => PointerKind::Up,
        4 => PointerKind::Hover,
        5 => PointerKind::Leave,
        _ => PointerKind::Cancel,
    };
    let button = match button {
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        _ => MouseButton::Left,
    };
    state(s).app.pointer_button(kind, button, x, y, time_ms);
}

/// The DOM `contextmenu` event over the pixel (x, y). source: 0 mouse, 1 touch or pen, 2 keyboard.
/// Returns 1 when a control or the app took it: the browser's own menu is then prevented.
#[unsafe(no_mangle)]
pub extern "C" fn dui_context_menu(s: *mut WebState, x: f32, y: f32, source: i32, time_ms: f64) -> i32 {
    let source = match source {
        1 => ContextMenuSource::Touch,
        2 => ContextMenuSource::Keyboard,
        _ => ContextMenuSource::Mouse,
    };
    state(s).app.context_menu(x, y, source, time_ms) as i32
}

/// A key or typed text. kind: 0 down, 1 up, 2 text. `code` is the DOM `KeyboardEvent.code`,
/// `text` the typed text (kind 2), both UTF-8 in memory JS wrote. modifiers: 1 shift, 2 ctrl,
/// 4 alt, 8 meta. Returns 1 when the app used it: the browser's default action is then prevented.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dui_key(
    s: *mut WebState,
    kind: i32,
    code: *const u8,
    code_len: usize,
    text: *const u8,
    text_len: usize,
    modifiers: u32,
    repeat: i32,
) -> i32 {
    let read = |ptr: *const u8, len: usize| match ptr.is_null() {
        true => "",
        false => std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }).unwrap_or(""),
    };
    let kind = match kind {
        0 => KeyKind::Down,
        1 => KeyKind::Up,
        _ => KeyKind::Char,
    };
    let key = if kind == KeyKind::Char { "" } else { key_name(read(code, code_len)) };
    state(s).app.key(kind, key, read(text, text_len), Modifiers::from_bits(modifiers), repeat != 0) as i32
}

/// The safe area in CSS pixels (`env(safe-area-inset-*)`), at start and when it changes.
#[unsafe(no_mangle)]
pub extern "C" fn dui_safe_insets(s: *mut WebState, left: f32, top: f32, right: f32, bottom: f32) {
    state(s).app.safe_insets(Thickness { left, top, right, bottom });
}

/// The page keeps the browser's history out of the app (`DrawnUi.start({ history: false })`, e.g.
/// a page in an iframe, whose entries would join the tab's Back): the app writes no URL hash and
/// SkiaShell navigates inside the canvas only.
#[unsafe(no_mangle)]
pub extern "C" fn dui_history(s: *mut WebState, on: i32) {
    state(s).host.history_on = on != 0;
}

/// The URL hash, UTF-8 in memory JS wrote: at start (`popped` -1), and after a `popstate` with
/// the depth its entry was pushed with (0 for one the app did not push).
#[unsafe(no_mangle)]
pub extern "C" fn dui_location(s: *mut WebState, hash: *const u8, hash_len: usize, popped: i32) {
    let hash = if hash.is_null() { "" } else { std::str::from_utf8(unsafe { std::slice::from_raw_parts(hash, hash_len) }).unwrap_or("") };
    state(s).app.location(hash, (popped >= 0).then_some(popped as u32));
}

/// The page was hidden (`visible` 0: a background tab) or shown again (DOM `visibilitychange`).
#[unsafe(no_mangle)]
pub extern "C" fn dui_visibility(s: *mut WebState, visible: i32) {
    state(s).app.visibility(visible != 0);
}

/// The window lost the keyboard (DOM `blur`): no key counts as held anymore.
#[unsafe(no_mangle)]
pub extern "C" fn dui_blur(s: *mut WebState) {
    state(s).app.blur();
}

/// The page focus went to an element outside the canvas (a field of the page).
#[unsafe(no_mangle)]
pub extern "C" fn dui_focus_out(s: *mut WebState) {
    state(s).app.focus_out();
}

/// The overlay activated the accessibility node `id` (a click, Enter or Space on it).
#[unsafe(no_mangle)]
pub extern "C" fn dui_a11y_activate(s: *mut WebState, id: u32) {
    state(s).app.accessibility_activate(id);
}

/// The overlay's keyboard focus moved onto (`focused` 1) or off the node `id`.
#[unsafe(no_mangle)]
pub extern "C" fn dui_a11y_focus(s: *mut WebState, id: u32, focused: i32) {
    state(s).app.accessibility_focus(id, focused != 0);
}

/// The URL part of a history entry: nothing keeps the URL, `#` asks for the URL without a hash.
fn url(hash: Option<String>) -> String {
    match hash {
        Some(hash) if hash.is_empty() => "#".to_owned(),
        Some(hash) => hash,
        None => String::new(),
    }
}

/// Next thing the page has to do after a frame, or null. The string is valid until the next call.
///   `c N`            the cursor: 0 default, 1 pointer (the hand), 2 text
///   `t L T W H N`    text input open over that rect (CSS pixels on the canvas) for the node N;
///                    `t` alone: closed
///   `p TEXT`         put TEXT on the clipboard
///   `v`              read the clipboard and send its text as typed text
///   `u URL`          open URL in a new tab
///   `h push D HASH`, `h replace D HASH`, `h back`: the browser history (HASH absent: the URL
///                    stays; `#` alone: the URL without its hash)
///   `a` + nodes      the accessibility snapshot: per node a line break, then tab-separated id,
///                    role, label, hint, left, top, width, height (CSS pixels), interactive 1, 0,
///                    or `-` for a control that takes no input (disabled),
///                    pressed -1 / 0 / 1, live, group `G:I` (the group's id and the item's) or
///                    empty, then the text lines of a selectable text: lines separated by U+001E,
///                    each with U+001F-separated text, left, top, width, height, font family,
///                    weight, size. The overlay is flat: a title that repeats the name of the
///                    control it is in is left out, a group named by its title has no name.
///   `f N`            the engine moved the keyboard to node N (an arrow in a group): focus it
#[unsafe(no_mangle)]
pub extern "C" fn dui_poll_output(s: *mut WebState) -> *const c_char {
    let s = state(s);
    let host = &mut s.host;
    let message = if let Some(cursor) = host.cursor.take() {
        let n = match cursor {
            Cursor::Default => 0,
            Cursor::Pointer => 1,
            Cursor::Text => 2,
        };
        Some(format!("c {n}"))
    } else if let Some(area) = host.text_input.take() {
        let node = host.text_input_node.map_or(String::new(), |n| n.to_string());
        Some(area.map_or_else(|| "t".to_owned(), |r| format!("t {} {} {} {} {node}", r.left, r.top, r.width(), r.height())))
    } else if let Some(text) = host.clipboard.take() {
        Some(format!("p {text}"))
    } else if let Some(url) = (!host.urls.is_empty()).then(|| host.urls.remove(0)) {
        Some(format!("u {url}"))
    } else if let Some(op) = (!host.history.is_empty()).then(|| host.history.remove(0)) {
        Some(match op {
            HistoryOp::Push { depth, hash } => format!("h push {depth} {}", url(hash)),
            HistoryOp::Replace { depth, hash } => format!("h replace {depth} {}", url(hash)),
            HistoryOp::Back => "h back".to_owned(),
        })
    } else if std::mem::take(&mut host.paste) {
        Some("v".to_owned())
    } else if let Some(nodes) = host.accessibility.take() {
        use std::fmt::Write;
        let clean = |t: &str| t.replace(['\t', '\n', '\r', '\u{1e}', '\u{1f}'], " ");
        let mut out = String::from("a");
        for n in &nodes {
            // Flat elements: a control is not named by what is inside it, so its title is left
            // out instead; a group keeps its title and goes unnamed.
            // A scroll's node is for native screen readers; the page scrolls nothing itself.
            if n.role == crate::ui::Aria::SCROLL_VIEW {
                continue;
            }
            let parent = n.parent.and_then(|p| nodes.iter().find(|m| m.id == p));
            if parent.is_some_and(|p| p.can_interact && crate::ui::said_by_child(std::slice::from_ref(n), p)) {
                continue;
            }
            let named = n.can_interact || !crate::ui::said_by_child(&nodes, n);
            let pressed = n.is_pressed.map_or(-1, |p| p as i32);
            let (r, label, hint) = (n.rect, if named { clean(&n.label) } else { String::new() }, clean(&n.hint));
            let interactive = if n.can_interact { "1" } else if crate::ui::is_control_role(n.role) { "-" } else { "0" };
            let (id, role, live) = (n.id, n.role, n.live);
            let (left, top, width, height) = (r.left, r.top, r.width(), r.height());
            let group = n.group.map_or(String::new(), |(g, i)| format!("{g}:{i}"));
            // A range control's value: now, min, max and the text read instead of the number.
            let value = n.value.as_ref().map_or(String::new(), |v| format!("{}\u{1f}{}\u{1f}{}\u{1f}{}", v.now, v.min, v.max, clean(&v.text)));
            let _ = write!(out, "\n{id}\t{role}\t{label}\t{hint}\t{left}\t{top}\t{width}\t{height}\t{interactive}\t{pressed}\t{live}\t{group}\t{value}\t");
            for (i, l) in n.text_lines.iter().enumerate() {
                let (text, family) = (clean(&l.text), clean(&l.font_family));
                let separator = if i > 0 { "\u{1e}" } else { "" };
                let _ = write!(out, "{separator}{text}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{family}\u{1f}{}\u{1f}{}", l.left, l.top, l.width, l.height, l.font_weight, l.font_size);
            }
        }
        Some(out)
    } else if let Some(id) = host.keyboard_moved.take() {
        Some(format!("f {id}"))
    } else {
        None
    };
    // A text with a NUL cannot cross: it is cut there.
    s.output = message.map(|m| {
        CString::new(m).unwrap_or_else(|e| {
            let at = e.nul_position();
            CString::new(&e.into_vec()[..at]).unwrap_or_default()
        })
    });
    s.output.as_ref().map_or(std::ptr::null(), |m| m.as_ptr())
}

/// `delta` in notches, positive = away from the user. Returns 1 when the app used the wheel.
#[unsafe(no_mangle)]
pub extern "C" fn dui_wheel(s: *mut WebState, x: f32, y: f32, delta: f32, time_ms: f64) -> i32 {
    state(s).app.wheel(x, y, delta, false, time_ms) as i32
}

/// `dui_wheel` with its axis: `horizontal` 1 for a horizontal event, `delta` positive = left.
#[unsafe(no_mangle)]
pub extern "C" fn dui_wheel_axis(s: *mut WebState, x: f32, y: f32, delta: f32, horizontal: i32, time_ms: f64) -> i32 {
    state(s).app.wheel(x, y, delta, horizontal != 0, time_ms) as i32
}

/// Next pending asset request as "id url", or null. The string is valid until the next call.
#[unsafe(no_mangle)]
pub extern "C" fn dui_poll_request(s: *mut WebState) -> *const c_char {
    let s = state(s);
    s.request = s.host.requests.pop().and_then(|(id, url)| CString::new(format!("{id} {url}")).ok());
    s.request.as_ref().map_or(std::ptr::null(), |r| r.as_ptr())
}

/// Next pending image request as "id width height frames url", or null: the picture of the url,
/// decoded, no larger than it takes to cover width x height pixels (a 0 side does not count,
/// both 0 = full size); frames 1 = every frame of an animated file, answered with
/// `dui_image_frames`. The string is valid until the next call.
#[unsafe(no_mangle)]
pub extern "C" fn dui_poll_image(s: *mut WebState) -> *const c_char {
    let s = state(s);
    let next = (!s.host.images.is_empty()).then(|| s.host.images.remove(0));
    s.request = next.and_then(|r| {
        CString::new(format!("{} {} {} {} {}", r.id, r.width, r.height, r.frames as u8, r.source)).ok()
    });
    s.request.as_ref().map_or(std::ptr::null(), |r| r.as_ptr())
}

/// Memory for the pixels of a decoded image. It is Skia's own, so `dui_image` makes an image of
/// it without another copy.
#[unsafe(no_mangle)]
pub extern "C" fn dui_alloc_pixels(s: *mut WebState, len: usize) -> *mut u8 {
    // JS writes every byte before `dui_image` reads any.
    let data = unsafe { skia::Data::new_uninitialized(len) };
    let ptr = data.as_bytes().as_ptr() as *mut u8;
    state(s).pixels.push(data);
    ptr
}

/// The answer to an image request: `ptr` from `dui_alloc_pixels`, filled with `width` x `height`
/// RGBA pixels, rows top to bottom, premultiplied. `source_width` x `source_height` is the size
/// of the file. A null `ptr` reports a failed load.
#[unsafe(no_mangle)]
pub extern "C" fn dui_image(
    s: *mut WebState,
    id: u32,
    ptr: *const u8,
    width: i32,
    height: i32,
    source_width: i32,
    source_height: i32,
    opaque: i32,
) {
    let s = state(s);
    let decoded = s.pixels.iter().position(|data| data.as_bytes().as_ptr() == ptr).and_then(|at| {
        let alpha = if opaque != 0 { skia::AlphaType::Opaque } else { skia::AlphaType::Premul };
        let info = skia::ImageInfo::new((width, height), skia::ColorType::RGBA8888, alpha, None);
        let image = skia::images::raster_from_data(&info, s.pixels.swap_remove(at), info.min_row_bytes())?;
        Some(Decoded { image, source_size: skia::ISize::new(source_width, source_height), frames: None })
    });
    s.app.image(id, decoded);
}

/// The answer to a request for frames: `ptr` from `dui_alloc_pixels` holding `count` frames of
/// `width` x `height` premultiplied RGBA pixels one after another, and `durations` from
/// `dui_alloc`, `count` little-endian u32 milliseconds. Every frame shares the one buffer.
#[unsafe(no_mangle)]
pub extern "C" fn dui_image_frames(
    s: *mut WebState,
    id: u32,
    ptr: *const u8,
    width: i32,
    height: i32,
    count: usize,
    durations: *mut u8,
) {
    let s = state(s);
    let ms: Vec<u32> = if durations.is_null() {
        Vec::new()
    } else {
        let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(durations, count * 4)) };
        bytes.chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
    };
    let decoded = s.pixels.iter().position(|data| data.as_bytes().as_ptr() == ptr).and_then(|at| {
        let info = skia::ImageInfo::new((width, height), skia::ColorType::RGBA8888, skia::AlphaType::Premul, None);
        let (row_bytes, all) = (info.min_row_bytes(), s.pixels.swap_remove(at));
        let frame_bytes = info.compute_byte_size(row_bytes);
        let images: Vec<skia::Image> = (0..count)
            .filter_map(|i| skia::Data::new_subset(&all, i * frame_bytes, frame_bytes).into())
            .filter_map(|data| skia::images::raster_from_data(&info, data, row_bytes))
            .collect();
        let image = images.first()?.clone();
        let size = skia::ISize::new(width, height);
        let frames = (images.len() > 1).then(|| Arc::new(Frames { images, durations: ms, size }));
        Some(Decoded { image, source_size: size, frames })
    });
    s.app.image(id, decoded);
}

/// Buffer for JS to copy asset bytes into; ownership returns to Rust in `dui_asset`, or with
/// `dui_free` for a buffer JS used for text.
#[unsafe(no_mangle)]
pub extern "C" fn dui_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Frees a `dui_alloc` buffer of `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn dui_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dui_asset(s: *mut WebState, id: u32, ptr: *mut u8, len: usize) {
    // A null pointer reports a failed load.
    let bytes = if ptr.is_null() { Vec::new() } else { unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) }.into_vec() };
    state(s).app.asset(id, bytes);
}
