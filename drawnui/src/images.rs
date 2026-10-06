//! The image manager (DrawnUI SkiaImageManager): one load per source however many controls show
//! it, a few loads with the host at a time, decoded bitmaps kept by source inside a byte budget.
//! The host decodes off the frame thread: `Host::fetch_image` out, `App::image` back, so nothing
//! here or in a paint ever decodes. A bitmap is asked for at the size it is shown at.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use skia_safe::{
    AlphaType, Codec, ColorType, ConditionallySend, Data, FilterMode, ISize, Image, ImageInfo, MipmapMode, Paint,
    SamplingOptions, codec, codec::Options, images, surfaces,
};

use crate::controls::gif::SkiaGif;
use crate::controls::image::{Loaded as Handler, SkiaImage, auto_sized};
use crate::Decoded;
use crate::tree::{ControlId, Cx, Tree, wrong_state};
use crate::types::Dirty;

/// The box a bitmap must cover without being enlarged, pixels. A 0 side does not count; (0, 0)
/// is the full size of the file.
pub(crate) type Want = (u32, u32);

const FULL: Want = (0, 0);

/// A bitmap decoded for `have` is good for `want`.
pub(crate) fn within(want: Want, have: Want) -> bool {
    have == FULL || (want != FULL && want.0 <= have.0 && want.1 <= have.1)
}

/// The smallest box good for both.
pub(crate) fn grow(a: Want, b: Want) -> Want {
    if a == FULL || b == FULL { FULL } else { (a.0.max(b.0), a.1.max(b.1)) }
}

/// What the manager asks a host for: the picture of `source`, decoded, no larger than it takes
/// to cover `width` x `height` pixels with its aspect kept (a 0 side does not count, both 0 =
/// the full size), never enlarged. With `frames`, every frame of an animated file at full size.
/// The answer is `App::image(id, ..)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRequest {
    pub id: u32,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub frames: bool,
}

/// The frames of an animated picture (a GIF), decoded once and shared by the controls that play it.
pub struct Frames {
    pub images: Vec<Image>,
    /// Milliseconds each frame shows.
    pub durations: Vec<u32>,
    pub size: ISize,
}

impl Frames {
    /// Sum of the durations.
    pub fn duration_ms(&self) -> u32 {
        self.durations.iter().sum()
    }

    fn bytes(&self) -> usize {
        self.images.iter().map(|image| image.image_info().compute_min_byte_size()).sum()
    }
}

struct Bitmap {
    image: Image,
    /// Pixel size of the file.
    source_size: ISize,
    /// What it was decoded for; `FULL` when it has every pixel of the file.
    good_for: Want,
    bytes: usize,
    /// `Images::tick` when a control last got it.
    shown: u64,
}

struct Loading {
    /// The controls waiting, each with the box it shows the picture in.
    waiting: Vec<(ControlId, Want)>,
    /// What the host is asked for.
    want: Want,
    /// Every frame is asked for: a control plays the file.
    frames: bool,
    /// Kept even when no control waits for it.
    preload: bool,
    /// The host has it; until then it waits in the queue.
    flying: bool,
}

#[derive(Default)]
struct Entry {
    bitmap: Option<Bitmap>,
    /// The frames of an animated file, once a control asked for them.
    frames: Option<Arc<Frames>>,
    loading: Option<Loading>,
}

/// `handler(app state, cx)` of a preload that is over.
type Done = Box<dyn FnOnce(&mut dyn Any, &mut Cx<'_>)>;

/// The image manager of a tree: `Tree::images`.
pub struct Images {
    entries: HashMap<String, Entry>,
    /// Sources waiting for a free slot. The first `normal` ones are wanted by controls, in the
    /// order they asked; preloads follow (upstream LoadPriority Normal and Low).
    queue: VecDeque<String>,
    normal: usize,
    /// Request id to source, for the loads the host has.
    flying: HashMap<u32, String>,
    next_id: u32,
    /// Requests the host did not get yet.
    outbox: Vec<ImageRequest>,
    /// (control, source, loaded) whose handler is due.
    pub(crate) events: Vec<(ControlId, String, bool)>,
    /// Preloads with a handler: the sources still loading, and the handler.
    preloads: Vec<(Vec<String>, Done)>,
    /// Handlers of preloads that are over, due in the next frame.
    done: Vec<Done>,
    budget: usize,
    used: usize,
    tick: u64,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            queue: VecDeque::new(),
            normal: 0,
            flying: HashMap::new(),
            next_id: 0,
            outbox: Vec::new(),
            events: Vec::new(),
            preloads: Vec::new(),
            done: Vec::new(),
            budget: Self::DEFAULT_BUDGET,
            used: 0,
            tick: 0,
        }
    }
}

impl Images {
    /// Loads with the host at once (upstream: 5 on Android and in the TypeScript port).
    pub const MAX_IN_FLIGHT: usize = 5;

    /// 128 MB of decoded pixels. A feed of 100 photos in 400 x 500 pixel cells is 80 MB, so it
    /// scrolls back without a reload; 22 photos of 1080 x 1350 (a phone's full width, about ten
    /// screens) fit too. With the same bytes again as textures it stays within a mobile browser tab.
    pub const DEFAULT_BUDGET: usize = 128 << 20;

    /// Loads sources ahead of the controls that will show them, at full size (DrawnUI
    /// PreloadImages). They go to the host after everything a control is waiting for.
    pub fn preload<S: AsRef<str>>(&mut self, sources: impl IntoIterator<Item = S>) {
        for source in sources {
            let source = source.as_ref();
            if source.is_empty() {
                continue;
            }
            let entry = self.entries.entry(source.to_owned()).or_default();
            match &mut entry.loading {
                Some(loading) => loading.preload = true,
                None if entry.bitmap.as_ref().is_some_and(|b| b.good_for == FULL) => {}
                None => {
                    let loading = Loading { waiting: Vec::new(), want: FULL, frames: false, preload: true, flying: false };
                    entry.loading = Some(loading);
                    self.queue.push_back(source.to_owned());
                }
            }
        }
    }

    /// The requests for the host. `Ui` sends them after each frame; a test takes them and answers
    /// with `App::image`. A source leaves the queue only here, so every control that asked for it
    /// during the frame is in its one request: the box is the largest any of them needs.
    pub fn take_requests(&mut self) -> Vec<ImageRequest> {
        while self.flying.len() < Self::MAX_IN_FLIGHT
            && let Some(source) = self.queue.pop_front()
        {
            self.normal = self.normal.saturating_sub(1);
            let loading = self.entries.get_mut(&source).and_then(|entry| entry.loading.as_mut());
            let Some(loading) = loading else { continue };
            loading.flying = true;
            let (id, (width, height), frames) = (self.next_id, loading.want, loading.frames);
            self.next_id = self.next_id.wrapping_add(1);
            self.outbox.push(ImageRequest { id, source: source.clone(), width, height, frames });
            self.flying.insert(id, source);
        }
        std::mem::take(&mut self.outbox)
    }

    /// The decoded bitmap of a source, if it is loaded (DrawnUI GetFromCache).
    pub fn get(&self, source: &str) -> Option<&Image> {
        Some(&self.entries.get(source)?.bitmap.as_ref()?.image)
    }

    /// Bytes of decoded pixels the cache holds.
    pub fn memory_bytes(&self) -> usize {
        self.used
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// The most bytes of decoded pixels to keep. Bitmaps on screen are never dropped for it.
    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes;
        self.trim();
    }

    /// Forgets every loaded bitmap. Controls showing one keep it; the next control asking loads again.
    pub fn clear(&mut self) {
        self.entries.retain(|_, entry| {
            (entry.bitmap, entry.frames) = (None, None);
            entry.loading.is_some()
        });
        self.used = 0;
    }

    /// Forgets the bitmap of one source (React `Clear(source)`).
    pub fn clear_source(&mut self, source: &str) {
        let Some(entry) = self.entries.get_mut(source) else { return };
        self.used -= entry.bitmap.take().map_or(0, |bitmap| bitmap.bytes);
        self.used -= entry.frames.take().map_or(0, |frames| frames.bytes());
        if entry.loading.is_none() {
            self.entries.remove(source);
        }
    }

    /// Loads with the host right now (React `RunningCount`).
    pub fn in_flight(&self) -> usize {
        self.flying.len()
    }

    /// Sources waiting for a free slot (React `QueuedCount`).
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// A control plays a source: its frames when they are loaded, else the control waits for them.
    pub(crate) fn request_frames(&mut self, source: &str, control: ControlId) -> Option<Arc<Frames>> {
        self.tick += 1;
        let entry = self.entries.entry(source.to_owned()).or_default();
        if let Some(frames) = &entry.frames {
            if let Some(bitmap) = &mut entry.bitmap {
                bitmap.shown = self.tick;
            }
            return Some(frames.clone());
        }
        match &mut entry.loading {
            Some(loading) => {
                loading.waiting.retain(|waiting| waiting.0 != control);
                loading.waiting.push((control, FULL));
                if !loading.flying {
                    (loading.want, loading.frames) = (FULL, true);
                }
            }
            None => {
                let loading = Loading { waiting: vec![(control, FULL)], want: FULL, frames: true, preload: false, flying: false };
                entry.loading = Some(loading);
                self.queue.insert(self.normal, source.to_owned());
                self.normal += 1;
            }
        }
        None
    }

    /// A control wants a source in a box: the bitmap when one is loaded, with the pixel size of
    /// its file. When none is loaded, or only one too small for the box, the control waits for
    /// the load (showing the small one meanwhile).
    pub(crate) fn request(&mut self, source: &str, control: ControlId, want: Want) -> Option<(Image, ISize)> {
        self.tick += 1;
        let entry = self.entries.entry(source.to_owned()).or_default();
        let bitmap = entry.bitmap.as_mut();
        let enough = bitmap.as_ref().is_some_and(|bitmap| within(want, bitmap.good_for));
        let hit = bitmap.map(|bitmap| {
            bitmap.shown = self.tick;
            (bitmap.image.clone(), bitmap.source_size)
        });
        if enough {
            return hit;
        }
        match &mut entry.loading {
            Some(loading) => {
                // Its box grew while it waited: only the last one counts.
                loading.waiting.retain(|waiting| waiting.0 != control);
                loading.waiting.push((control, want));
                if !loading.flying {
                    loading.want = grow(loading.want, want);
                    // A file another control plays is loaded whole anyway.
                    if loading.frames {
                        loading.want = FULL;
                    }
                    // A preload somebody waits for goes before the other preloads.
                    if let Some(at) = self.queue.iter().skip(self.normal).position(|queued| queued == source) {
                        let source = self.queue.remove(self.normal + at).expect("just found");
                        self.queue.insert(self.normal, source);
                        self.normal += 1;
                    }
                }
            }
            None => {
                let loading = Loading { waiting: vec![(control, want)], want, frames: false, preload: false, flying: false };
                entry.loading = Some(loading);
                self.queue.insert(self.normal, source.to_owned());
                self.normal += 1;
            }
        }
        hit
    }

    /// The control shows something else now (DrawnUI CancelLoading): a load nobody waits for
    /// leaves the queue, unless it is a preload or the host has it already.
    pub(crate) fn release(&mut self, source: &str, control: ControlId) {
        let Some(entry) = self.entries.get_mut(source) else { return };
        let Some(loading) = &mut entry.loading else { return };
        loading.waiting.retain(|waiting| waiting.0 != control);
        if loading.waiting.is_empty() && !loading.preload && !loading.flying {
            entry.loading = None;
            if entry.bitmap.is_none() {
                self.entries.remove(source);
            }
            if let Some(at) = self.queue.iter().position(|queued| queued == source) {
                self.queue.remove(at);
                self.normal -= (at < self.normal) as usize;
            }
        }
    }

    /// Over the budget: the bitmaps no control shows go, the one shown longest ago first.
    // ponytail: collects and sorts the idle bitmaps on every arrival over the budget (some 200
    // tiles at the default). A list in shown order when a cache holds thousands.
    fn trim(&mut self) {
        if self.used <= self.budget {
            return;
        }
        // Only the cache holds it (a reference count of one): no control shows it, and no
        // recorded cache of a control drew it.
        let idle = |entry: &Entry| {
            let frames_idle = entry.frames.as_ref().is_none_or(|frames| Arc::strong_count(frames) == 1);
            entry.bitmap.as_ref().filter(|b| b.image.can_send() && frames_idle).map(|b| b.shown)
        };
        let mut idle: Vec<(u64, String)> =
            self.entries.iter().filter_map(|(source, entry)| Some((idle(entry)?, source.clone()))).collect();
        idle.sort_unstable();
        for (_, source) in idle {
            if self.used <= self.budget {
                break;
            }
            let Some(entry) = self.entries.get_mut(&source) else { continue };
            self.used -= entry.bitmap.take().map_or(0, |bitmap| bitmap.bytes);
            self.used -= entry.frames.take().map_or(0, |frames| frames.bytes());
            if entry.loading.is_none() {
                self.entries.remove(&source);
            }
        }
    }

    /// What a host does with a request, as one call: decodes the bytes of an image file to a
    /// bitmap no larger than it takes to cover `width` x `height` pixels (a 0 side does not
    /// count, both 0 = full size), turned as its EXIF orientation says. It is slow (about 10 ms
    /// per megapixel of JPEG): hosts call it on a worker thread, never on the frame thread.
    pub fn decode(bytes: &[u8], width: u32, height: u32) -> Option<Decoded> {
        let mut codec = Codec::from_data(Data::new_copy(bytes))?;
        let swaps = codec.origin().swaps_width_height();
        let turned = |size: ISize| if swaps { ISize::new(size.height, size.width) } else { size };
        let source_size = turned(codec.dimensions());
        if source_size.is_empty() {
            return None;
        }
        let (cover_x, cover_y) = (width as f64 / source_size.width as f64, height as f64 / source_size.height as f64);
        let scale = if (width, height) == FULL { 1.0 } else { cover_x.max(cover_y).min(1.0) };
        let side = |pixels: i32| ((pixels as f64 * scale - 1e-6).ceil() as i32).max(1);
        let target = ISize::new(side(source_size.width), side(source_size.height));

        // A JPEG decodes at eighths of its size for a fraction of the work; other formats come
        // whole. `get_image` turns the pixels upright.
        let decoded = turned(codec.get_scaled_dimensions(((scale * 8.0).ceil() / 8.0) as f32));
        let alpha = if codec.info().is_opaque() { AlphaType::Opaque } else { AlphaType::Premul };
        let info = ImageInfo::new(decoded, ColorType::RGBA8888, alpha, codec.info().color_space());
        let image = codec.get_image(info.clone(), None).ok()?;
        if decoded.width <= target.width + 1 {
            return Some(Decoded { image, source_size, frames: None });
        }
        // Larger than asked for (between two eighths, or a format that cannot decode smaller):
        // drawn once into a bitmap of the wanted size, so the cache holds no more than that.
        let mut surface = surfaces::raster(&info.with_dimensions(target), None, None)?;
        let canvas = surface.canvas();
        canvas.scale((target.width as f32 / decoded.width as f32, target.height as f32 / decoded.height as f32));
        // Mip levels only where a plain linear filter would skip pixels.
        let mips = if decoded.width >= 2 * target.width { MipmapMode::Linear } else { MipmapMode::None };
        let sampling = SamplingOptions::new(FilterMode::Linear, mips);
        canvas.draw_image_with_sampling_options(&image, (0, 0), sampling, Some(&Paint::default()));
        Some(Decoded { image: surface.image_snapshot(), source_size, frames: None })
    }

    /// What a host does with a request for frames: decodes every frame of an animated file at
    /// full size (a still file gives one frame). Slow, like `decode`: for a worker thread.
    pub fn decode_frames(bytes: &[u8]) -> Option<Decoded> {
        let mut codec = Codec::from_data(Data::new_copy(bytes))?;
        let count = codec.get_frame_count().max(1);
        let info = ImageInfo::new(codec.dimensions(), ColorType::RGBA8888, AlphaType::Premul, None);
        let row_bytes = info.min_row_bytes();
        // A frame may be painted over the one it says it needs: those pixels are kept.
        let mut pixels: Vec<Vec<u8>> = Vec::with_capacity(count);
        let (mut images, mut durations) = (Vec::with_capacity(count), Vec::with_capacity(count));
        for index in 0..count {
            let frame = codec.get_frame_info(index);
            let required = frame.as_ref().map(|f| f.required_frame).filter(|r| *r >= 0).map(|r| r as usize);
            let mut buffer = match required {
                Some(prior) => pixels[prior].clone(),
                None => vec![0u8; info.compute_byte_size(row_bytes)],
            };
            let options = Options { frame_index: index, prior_frame: required, ..Options::default() };
            match codec.get_pixels_with_options(&info, &mut buffer, row_bytes, Some(&options)) {
                codec::Result::Success | codec::Result::IncompleteInput => {}
                _ => break,
            }
            images.push(images::raster_from_data(&info, Data::new_copy(&buffer), row_bytes)?);
            // At least 1 ms, as React's GifAnimation (upstream takes 0 as it is; browsers show 100).
            durations.push(frame.map_or(1, |f| f.duration.max(1) as u32));
            pixels.push(buffer);
        }
        let image = images.first()?.clone();
        let source_size = codec.dimensions();
        let frames = (images.len() > 1).then(|| Arc::new(Frames { images, durations, size: source_size }));
        Some(Decoded { image, source_size, frames })
    }
}

/// The host answered a request (`None` = failed). Every control waiting for the bitmap gets it
/// and is invalidated: measured again when it takes its size from the bitmap, else only drawn
/// again. A control that asked for more than the host was asked for shows this one and waits for
/// a bigger one. A failure is not remembered: the waiting controls keep `has_error`, the next
/// control asking starts a new load (as upstream).
pub(crate) fn deliver(tree: &mut Tree, id: u32, decoded: Option<Decoded>) {
    let images = &mut tree.images;
    let Some(source) = images.flying.remove(&id) else { return };
    let Some(entry) = images.entries.get_mut(&source) else { return };
    let Some(loading) = entry.loading.take() else { return };
    images.tick += 1;
    match &decoded {
        Some(Decoded { image, source_size, frames }) => {
            let full = image.width() >= source_size.width && image.height() >= source_size.height;
            let bytes = image.image_info().compute_min_byte_size();
            let good_for = if full { FULL } else { loading.want };
            let (image, source_size, shown) = (image.clone(), *source_size, images.tick);
            let old = entry.bitmap.replace(Bitmap { image, source_size, good_for, bytes, shown });
            images.used = images.used + bytes - old.map_or(0, |old| old.bytes);
            if let Some(frames) = frames {
                let old = entry.frames.replace(frames.clone());
                images.used = images.used + frames.bytes() - old.map_or(0, |old| old.bytes());
            }
        }
        None => {
            eprintln!("drawnui: image {source} did not load");
            if entry.bitmap.is_none() {
                images.entries.remove(&source);
            }
        }
    }
    // A control that plays the file gets every frame; a still file plays as its one frame.
    let played = decoded.as_ref().map(|d| {
        d.frames.clone().unwrap_or_else(|| {
            Arc::new(Frames { images: vec![d.image.clone()], durations: vec![0], size: d.source_size })
        })
    });
    let mut again: Vec<(ControlId, Want)> = Vec::new();
    let mut events = Vec::new();
    for (control, want) in loading.waiting {
        if tree.find::<SkiaGif>(control).is_some_and(|gif| gif.resolved == source) {
            events.extend(SkiaGif::arrived(tree, control, &source, played.clone()));
            continue;
        }
        // A control removed meanwhile left a stale id behind.
        let waiting = tree.find_mut::<SkiaImage>(control).filter(|me| me.resolved == source);
        let Some(mut me) = waiting else { continue };
        let auto = auto_sized(&me.base().p);
        let it = me.control_mut();
        let first = it.image.is_none();
        match &decoded {
            Some(Decoded { image, source_size, .. }) => {
                (it.image, it.source_size, it.loading, it.error) = (Some(image.clone()), *source_size, false, false);
                let handled = first && it.on_success.is_some();
                // A bigger bitmap of the same picture changes no size.
                let measure = first && (auto || it.remeasure_on_arrival);
                me.mark(if measure { Dirty::MEASURE } else { Dirty::DRAW });
                if handled {
                    events.push((control, source.clone(), true));
                }
                if !within(want, loading.want) {
                    again.push((control, want));
                }
            }
            None if first => {
                (it.loading, it.error) = (false, true);
                if it.on_error.is_some() {
                    events.push((control, source.clone(), false));
                }
            }
            None => {}
        }
    }
    let images = &mut tree.images;
    images.events.append(&mut events);
    // Preloads waiting for this source: over when nothing else of theirs loads (a failure too,
    // as React's PreloadImages ignores failures).
    for (sources, _) in &mut images.preloads {
        sources.retain(|waiting| *waiting != source);
    }
    while let Some(at) = images.preloads.iter().position(|(sources, _)| sources.is_empty()) {
        let (_, handler) = images.preloads.swap_remove(at);
        images.done.push(handler);
    }
    if !again.is_empty() {
        let want = again.iter().fold(again[0].1, |want, waiting| grow(want, waiting.1));
        let entry = images.entries.entry(source.clone()).or_default();
        entry.loading = Some(Loading { waiting: again, want, frames: false, preload: false, flying: false });
        images.queue.insert(images.normal, source);
        images.normal += 1;
    }
    images.trim();
    // A frame hands the next requests to the host and runs the handlers.
    if !images.queue.is_empty() || !images.events.is_empty() || !images.done.is_empty() {
        tree.needs_frame = true;
    }
}

/// Takes the success or error handler of an image or a gif out, or puts one back.
fn swap_handler(tree: &mut Tree, id: ControlId, loaded: bool, handler: Option<Handler>) -> Option<Handler> {
    if let Some(mut gif) = tree.find_mut::<SkiaGif>(id) {
        let gif = gif.control_mut();
        let slot = if loaded { &mut gif.on_success } else { &mut gif.on_error };
        return std::mem::replace(slot, handler);
    }
    let mut image = tree.find_mut::<SkiaImage>(id)?;
    let image = image.control_mut();
    let slot = if loaded { &mut image.on_success } else { &mut image.on_error };
    std::mem::replace(slot, handler)
}

/// Runs the success and error handlers that are due, then those of preloads that are over.
/// True when one ran (it may have changed the app state).
pub(crate) fn dispatch(tree: &mut Tree, state: &mut dyn Any) -> bool {
    let mut ran = false;
    for (id, source, loaded) in std::mem::take(&mut tree.images.events) {
        // The handler leaves the control while it runs: it reaches the control through the tree.
        let Some(mut handler) = swap_handler(tree, id, loaded, None) else { continue };
        handler(state, &mut Cx { tree }, &source);
        swap_handler(tree, id, loaded, Some(handler));
        ran = true;
    }
    for handler in std::mem::take(&mut tree.images.done) {
        handler(state, &mut Cx { tree });
        ran = true;
    }
    ran
}

impl Cx<'_> {
    /// Loads image sources ahead of the controls that will show them (DrawnUI PreloadImages).
    pub fn preload_images<S: AsRef<str>>(&mut self, sources: impl IntoIterator<Item = S>) {
        self.tree.images.preload(sources);
        self.tree.needs_frame = true;
    }

    /// Loads image sources ahead, as `preload_images`, and runs `done` once every one of them
    /// arrived or failed (React `await PreloadImages`): in the next frame, sources already loaded
    /// count as arrived.
    pub fn preload_images_then<S: Any, I: AsRef<str>>(
        &mut self,
        sources: impl IntoIterator<Item = I>,
        done: impl FnOnce(&mut S, &mut Cx<'_>) + 'static,
    ) {
        let sources: Vec<String> = sources.into_iter().map(|s| s.as_ref().to_owned()).collect();
        let images = &mut self.tree.images;
        images.preload(&sources);
        let mut loading: Vec<String> =
            sources.into_iter().filter(|s| images.entries.get(s).is_some_and(|entry| entry.loading.is_some())).collect();
        loading.sort_unstable();
        loading.dedup();
        let done: Done = Box::new(move |state, cx| done(state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>()), cx));
        match loading.is_empty() {
            true => images.done.push(done),
            false => images.preloads.push((loading, done)),
        }
        self.tree.needs_frame = true;
    }

    /// The image manager (React `SkiaImageManager.Instance`): `in_flight`, `queued`,
    /// `clear_source`, `memory_bytes`, `get`. To preload, use `preload_images`: it also asks for
    /// the frame that hands the loads to the host.
    pub fn images(&mut self) -> &mut Images {
        &mut self.tree.images
    }
}
