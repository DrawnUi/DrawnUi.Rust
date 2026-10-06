//! ImageDoubleBuffered off the frame thread (DrawnUI offscreen bakes). The frame records what the
//! control paints into a picture; a worker of the host draws that picture into a CPU bitmap; the
//! frame that gets the bitmap back swaps it in. Meanwhile the control shows its last bitmap as it
//! was made (its size, not stretched), or, the very first time, a placeholder for one frame.
//!
//! A host without workers (the browser) never enables them: there ImageDoubleBuffered records in
//! the frame, as an Image cache (DrawnUI CanUseCacheDoubleBuffering is false on the web).

use std::collections::HashMap;

use skia_safe::{Color, ISize, Image, Picture, Point, Rect, surfaces};

use crate::tree::ControlId;
use crate::types::Thickness;

/// A picture for a worker to draw into a CPU bitmap.
pub struct BakeRequest {
    pub id: u32,
    picture: Picture,
    size: ISize,
    /// Canvas pixel the bitmap's top-left corner shows.
    origin: Point,
}

impl BakeRequest {
    /// What a host worker does with it: the picture drawn into a CPU bitmap of its size. The
    /// picture holds no GPU texture (`PaintCx::offthread` keeps them out).
    pub fn bake(&self) -> Option<Image> {
        draw(&self.picture, self.size, self.origin)
    }
}

/// `picture` drawn into a CPU bitmap of `bounds` (canvas pixels).
pub(crate) fn rasterize(picture: &Picture, bounds: Rect) -> Option<Image> {
    draw(picture, size_of(bounds), Point::new(bounds.left, bounds.top))
}

fn size_of(bounds: Rect) -> ISize {
    ISize::new((bounds.width().round() as i32).max(1), (bounds.height().round() as i32).max(1))
}

fn draw(picture: &Picture, size: ISize, origin: Point) -> Option<Image> {
    let mut surface = surfaces::raster_n32_premul(size)?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-origin.x, -origin.y));
    canvas.draw_picture(picture, None, None);
    Some(surface.image_snapshot())
}

/// A bake out with a worker: the cache it will become.
pub(crate) struct Flying {
    pub id: u32,
    pub control: ControlId,
    /// The control's `content_epoch` the picture was recorded at.
    pub epoch: u32,
    pub bounds: Rect,
    pub margin: Thickness,
    pub scale: f32,
}

#[derive(Default)]
pub(crate) struct Bakes {
    /// The host draws the pictures on workers (desktop, headless tests). Off: ImageDoubleBuffered
    /// records in the frame as Image.
    pub enabled: bool,
    next: u32,
    pub requests: Vec<BakeRequest>,
    /// At most one bake per control, by render slot: a change while one is out is recorded after
    /// it is back (DrawnUI keeps one running and the latest pending).
    pub flying: HashMap<usize, Flying>,
    /// The `content_epoch` a control's last bake failed at (no bitmap that big): no new try until
    /// its content changes; meanwhile it shows its last bitmap, or draws live.
    failed: HashMap<usize, (ControlId, u32)>,
}

impl Bakes {
    /// The bake of control `id` out with a worker, if any (a slot's old control does not count).
    pub fn flying(&self, id: ControlId) -> bool {
        self.flying.get(&(id.index as usize)).is_some_and(|f| f.control == id)
    }

    /// Sends the picture of `id`'s content to a worker.
    pub fn send(&mut self, id: ControlId, picture: Picture, epoch: u32, bounds: Rect, margin: Thickness, scale: f32) {
        self.next = self.next.wrapping_add(1);
        let (size, origin) = (size_of(bounds), Point::new(bounds.left, bounds.top));
        self.requests.push(BakeRequest { id: self.next, picture, size, origin });
        let flying = Flying { id: self.next, control: id, epoch, bounds, margin, scale };
        self.flying.insert(id.index as usize, flying);
    }

    /// The content epoch `id`'s last bake failed at.
    pub fn failed(&self, id: ControlId) -> Option<u32> {
        self.failed.get(&(id.index as usize)).filter(|f| f.0 == id).map(|f| f.1)
    }

    pub fn set_failed(&mut self, id: ControlId, epoch: u32) {
        self.failed.insert(id.index as usize, (id, epoch));
    }

    /// The bake `id` is back: what it was for.
    pub fn arrived(&mut self, id: u32) -> Option<(usize, Flying)> {
        let slot = *self.flying.iter().find(|(_, f)| f.id == id)?.0;
        self.flying.remove(&slot).map(|f| (slot, f))
    }
}
