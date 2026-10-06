//! Headless host: renders a `Ui` into CPU memory, drives frames and pointer input, reads pixels.
//! No window, no GPU. The same role as DrawnUi.Net HeadlessCanvasHost + GestureRobot.
//! Frame time is a synthetic clock that only the test moves, so animations are deterministic.

use skia_safe::{Color, EncodedImageFormat, ImageInfo, Rect, Surface, surfaces};

use crate::gpu::Gpu;
use crate::tree::ControlId;
use crate::ui::Ui;
use crate::{
    App as _, ContextMenuSource, HistoryOp, ImageRequest, Images, KeyKind, LONG_PRESS_MS, Modifiers, MouseButton, PointerKind,
    key_name,
};

pub struct Headless<S: 'static> {
    pub ui: Ui<S>,
    surface: Surface,
    gpu: Gpu,
    width: i32,
    height: i32,
    scale: f32,
    /// The frame time the next frame gets, milliseconds.
    time_ms: f64,
    /// ImageDoubleBuffered bitmaps wait for `deliver_bakes` instead of being made after each frame.
    hold_bakes: bool,
    /// The requests being made, kept with its capacity: a frame allocates nothing for them.
    baking: Vec<crate::BakeRequest>,
    /// The robot presses with a finger.
    touch: bool,
}

impl<S: 'static> Headless<S> {
    /// A canvas of `width` x `height` pixels at `scale` pixels per point.
    pub fn new(ui: Ui<S>, width: i32, height: i32, scale: f32) -> Self {
        let surface = surfaces::raster(&ImageInfo::new_n32_premul((width, height), None), None, None).expect("raster surface");
        let mut ui = ui;
        ui.tree.synthetic_clock = true;
        // ImageDoubleBuffered as on the desktop: bitmaps made apart from the frame.
        ui.tree.bakes.enabled = true;
        Self { ui, surface, gpu: Gpu::raster(), width, height, scale, time_ms: 0.0, hold_bakes: false, baking: Vec::new(), touch: false }
    }

    /// The canvas changes size, as a window does (a minimized desktop window is 1 x 1 pixels).
    pub fn resize(&mut self, width: i32, height: i32) {
        (self.width, self.height) = (width.max(1), height.max(1));
        let info = ImageInfo::new_n32_premul((self.width, self.height), None);
        self.surface = surfaces::raster(&info, None, None).expect("raster surface");
    }

    /// Renders one frame at the current clock time. The ImageDoubleBuffered bitmaps it sent to be
    /// made are made right after, as by a worker that is done before the next frame: they show in it.
    pub fn frame(&mut self) {
        let (width, height) = (self.width as f32, self.height as f32);
        self.ui.draw(self.surface.canvas(), &mut self.gpu, width, height, self.scale, self.time_ms);
        if !self.hold_bakes {
            self.deliver_bakes();
        }
    }

    /// Workers that take their time: ImageDoubleBuffered bitmaps are made only by `deliver_bakes`.
    pub fn hold_bakes(&mut self, hold: bool) {
        self.hold_bakes = hold;
    }

    /// Makes the ImageDoubleBuffered bitmaps sent so far and hands them back, as the desktop's
    /// workers do. Returns how many there were.
    pub fn deliver_bakes(&mut self) -> usize {
        std::mem::swap(&mut self.baking, &mut self.ui.tree.bakes.requests);
        let count = self.baking.len();
        for request in self.baking.drain(..) {
            self.ui.baked(request.id, request.bake());
        }
        count
    }

    /// The GPU context is made again, as a host does after a lost one: the next frame drops what
    /// lived on the old one.
    pub fn gpu_recreated(&mut self) {
        self.gpu = Gpu::raster_after(&self.gpu);
    }

    /// The ImageDoubleBuffered pictures sent so far, for a test's own workers; their bitmaps go
    /// back through `App::baked`.
    pub fn take_bakes(&mut self) -> Vec<crate::BakeRequest> {
        std::mem::take(&mut self.ui.tree.bakes.requests)
    }

    /// No bake workers, as the browser: ImageDoubleBuffered records in the frame, as Image.
    pub fn without_bake_workers(&mut self) {
        self.ui.tree.bakes.enabled = false;
    }

    /// The synthetic clock: the time the next frame gets, and the stamp for input sent by hand.
    pub fn time_ms(&self) -> f64 {
        self.time_ms
    }

    /// Moves the clock forward by `ms`, then renders one frame.
    pub fn frame_after(&mut self, ms: f64) {
        self.time_ms += ms;
        self.frame();
    }

    /// Renders frames, 16 ms apart, until nothing is pending: running animations play to their
    /// end, and the clock jumps to every timer that waits. Panics when the tree never settles (a
    /// repeat-forever animator, one longer than 9.6 s).
    pub fn settle(&mut self) {
        self.frame();
        for _ in 0..600 {
            match (self.ui.needs_frame(), self.ui.wake_at()) {
                (true, _) => self.frame_after(16.0),
                (false, Some(wake)) => self.frame_after(wake - self.time_ms),
                (false, None) => return,
            }
        }
        panic!("the tree did not settle in 600 frames");
    }

    /// How many times the cache of a control was recorded.
    pub fn cache_records(&self, id: impl Into<ControlId>) -> u32 {
        self.ui.tree.render.get(id.into().index as usize).map_or(0, |slot| slot.records)
    }

    // The robot (DrawnUi.Net GestureRobot). Input is stamped with the synthetic clock, so
    // velocities, and with them flings, are the same on every run.

    /// The robot's presses are a finger (`Gesture::touch`) from now on, or a mouse again.
    pub fn use_touch(&mut self, touch: bool) {
        self.touch = touch;
    }

    fn press(&mut self, kind: PointerKind, x: f32, y: f32) {
        match self.touch {
            true => self.ui.pointer_touch(kind, x, y, self.time_ms),
            false => self.ui.pointer(kind, x, y, self.time_ms),
        }
    }

    /// The text the controls put on the clipboard since the last call (`Cx::set_clipboard`).
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.ui.tree.clipboard.take()
    }

    /// Pointer down and up at a pixel position, one frame after each.
    pub fn tap(&mut self, x: f32, y: f32) {
        self.press(PointerKind::Down, x, y);
        self.frame();
        self.press(PointerKind::Up, x, y);
        self.settle();
    }

    /// Pointer down, `steps` moves along the way over `duration_ms`, up one step later. A frame
    /// follows every event. Nothing is settled: a fling the release started is still running.
    pub fn pan(&mut self, from: (f32, f32), to: (f32, f32), duration_ms: f64, steps: u32) {
        let step_ms = duration_ms / steps as f64;
        self.press(PointerKind::Down, from.0, from.1);
        self.frame_after(step_ms);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.press(PointerKind::Move, x, y);
            self.frame_after(step_ms);
        }
        self.press(PointerKind::Up, to.0, to.1);
        self.frame_after(step_ms);
    }

    /// A pan, then frames until everything came to rest. A short `duration_ms` makes it a flick.
    pub fn fling(&mut self, from: (f32, f32), to: (f32, f32), duration_ms: f64, steps: u32) {
        self.pan(from, to, duration_ms, steps);
        self.settle();
    }

    /// One wheel event over a pixel position, `delta` in notches, then a frame 16 ms later.
    /// Returns whether a control used the wheel.
    pub fn wheel(&mut self, x: f32, y: f32, delta: f32) -> bool {
        let used = self.ui.wheel(x, y, delta, self.time_ms);
        self.frame_after(16.0);
        used
    }

    /// `wheel` for a horizontal event, `delta` positive = left.
    pub fn wheel_horizontal(&mut self, x: f32, y: f32, delta: f32) -> bool {
        let used = self.ui.wheel_horizontal(x, y, delta, self.time_ms);
        self.frame_after(16.0);
        used
    }

    /// A key goes down (`key` a DOM `code` name: "KeyA", "ArrowLeft", "Space") with `modifiers`
    /// held, then a frame. Returns whether something used it.
    pub fn key_down(&mut self, key: &str, modifiers: Modifiers) -> bool {
        let used = self.ui.key(KeyKind::Down, key_name(key), "", modifiers, false);
        self.frame_after(16.0);
        used
    }

    /// A key goes up, then a frame. Returns whether something used it.
    pub fn key_up(&mut self, key: &str, modifiers: Modifiers) -> bool {
        let used = self.ui.key(KeyKind::Up, key_name(key), "", modifiers, false);
        self.frame_after(16.0);
        used
    }

    /// A key down and up without modifiers. Returns whether something used the down.
    pub fn press_key(&mut self, key: &str) -> bool {
        let used = self.key_down(key, Modifiers::default());
        self.key_up(key, Modifiers::default());
        used
    }

    /// Typed text, one `KeyKind::Char` per character as a keyboard sends them, then a frame.
    pub fn type_text(&mut self, text: &str) {
        let mut buffer = [0u8; 4];
        for c in text.chars() {
            self.ui.key(KeyKind::Char, "", c.encode_utf8(&mut buffer), Modifiers::default(), false);
        }
        self.frame_after(16.0);
    }

    /// The mouse moves to a pixel with no button down, then a frame.
    pub fn hover(&mut self, x: f32, y: f32) {
        self.ui.pointer(PointerKind::Hover, x, y, self.time_ms);
        self.frame_after(16.0);
    }

    /// The mouse leaves the canvas, then a frame.
    pub fn leave(&mut self) {
        self.ui.pointer(PointerKind::Leave, 0.0, 0.0, self.time_ms);
        self.frame_after(16.0);
    }

    /// Pointer down, held still for `LONG_PRESS_MS` (the LongPressing fires in the frame at that
    /// time), then up, then everything settles.
    pub fn long_press(&mut self, x: f32, y: f32) {
        self.press(PointerKind::Down, x, y);
        self.frame();
        self.frame_after(LONG_PRESS_MS);
        self.press(PointerKind::Up, x, y);
        self.settle();
    }

    /// A right click as a browser on Windows sends it: the right button down and up (a Tapped
    /// with `MouseButton::Right`), then the context menu request. Returns whether a control or
    /// the `Ui::on_context_menu` fallback took the menu.
    pub fn right_click(&mut self, x: f32, y: f32) -> bool {
        self.ui.pointer_button(PointerKind::Down, MouseButton::Right, x, y, self.time_ms);
        self.frame();
        self.ui.pointer_button(PointerKind::Up, MouseButton::Right, x, y, self.time_ms);
        let taken = self.ui.context_menu(x, y, ContextMenuSource::Mouse, self.time_ms);
        self.settle();
        taken
    }

    /// Plays the browser for a shell with history: the history changes the frames asked for, in
    /// order (answer a `Back` with `ui.location(hash, Some(depth))`). Turn the history on first
    /// with `ui.set_history_enabled(true)`.
    pub fn take_history(&mut self) -> Vec<HistoryOp> {
        std::mem::take(&mut self.ui.tree.history_ops)
    }

    /// The drawing rect of a control, pixels.
    pub fn rect(&self, id: impl Into<ControlId>) -> Rect {
        self.ui.tree.base(id).map(|b| b.rect).unwrap_or_default()
    }

    /// Color of one pixel of the last frame.
    pub fn pixel(&mut self, x: i32, y: i32) -> Color {
        self.surface.peek_pixels().expect("raster pixels").get_color((x, y))
    }

    /// Plays the host for the asset channel (SVG files, Lottie JSON, shader sources): every
    /// request is answered through `App::asset` with `bytes_of(url)` (`None` = the load fails:
    /// empty bytes), until none is left. Returns the urls answered, in order.
    pub fn deliver_assets(&mut self, bytes_of: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<String> {
        let mut answered = Vec::new();
        loop {
            let requests = self.ui.tree.assets.take_requests();
            if requests.is_empty() {
                return answered;
            }
            for (id, url) in requests {
                self.ui.asset(id, bytes_of(&url).unwrap_or_default());
                answered.push(url);
            }
        }
    }

    /// Plays the host for images: every request the manager has is answered through
    /// `App::image` with the file bytes `bytes_of(source)` decoded at the size it asked for
    /// (`None` = the load fails), until it has none left: an answer frees a slot for a queued
    /// source, and a control that needs more than arrived asks again. Returns the requests
    /// answered, in order. What controls ask for in a later frame is for the next call.
    pub fn deliver_images(&mut self, bytes_of: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<ImageRequest> {
        let mut answered = Vec::new();
        loop {
            let requests = self.ui.tree.images.take_requests();
            if requests.is_empty() {
                return answered;
            }
            for request in &requests {
                let bytes = bytes_of(&request.source);
                let decoded = bytes.and_then(|b| match request.frames {
                    true => Images::decode_frames(&b),
                    false => Images::decode(&b, request.width, request.height),
                });
                self.ui.image(request.id, decoded);
            }
            answered.extend(requests);
        }
    }

    pub fn save_png(&mut self, path: &str) {
        let image = self.surface.image_snapshot();
        let data = image.encode(None, EncodedImageFormat::PNG, None).expect("png");
        std::fs::write(path, data.as_bytes()).expect("write png");
    }
}
