//! Asset loads for controls (SVG files, Lottie JSON, shader sources): a module asks for the bytes
//! of a url with the code that takes them, the host fetches them through the font channel
//! (`Host::fetch` / `App::asset`), and the bytes come back to that code on the frame thread.
//! Empty bytes mean the load failed. Each module keeps its own cache by url; this only carries.

use crate::tree::Tree;

/// Ids below it are fonts (the index in the registration order).
pub(crate) const ASSET_BASE: u32 = 1 << 20;

type Deliver = Box<dyn FnOnce(&mut Tree, Vec<u8>)>;

#[derive(Default)]
pub struct Assets {
    next: u32,
    pending: Vec<(u32, String)>,
    waiting: Vec<(u32, Deliver)>,
}

impl Assets {
    /// Asks the host for `url`; `deliver` runs with the bytes when they arrive (empty = failed).
    pub(crate) fn fetch(&mut self, url: &str, deliver: impl FnOnce(&mut Tree, Vec<u8>) + 'static) -> u32 {
        let id = ASSET_BASE + self.next;
        self.next += 1;
        self.pending.push((id, url.to_owned()));
        self.waiting.push((id, Box::new(deliver)));
        id
    }

    /// The requests not yet handed to the host.
    pub(crate) fn take_requests(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending)
    }

    pub(crate) fn take(&mut self, id: u32) -> Option<Deliver> {
        let at = self.waiting.iter().position(|(waiting, _)| *waiting == id)?;
        Some(self.waiting.swap_remove(at).1)
    }
}
