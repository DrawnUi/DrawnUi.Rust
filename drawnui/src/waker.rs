//! Waker: marks controls dirty from any thread and wakes the frame loop (camera frames, sensors,
//! network). The marks are applied at the start of the next frame, on the UI thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::tree::ControlId;
use crate::types::Dirty;

/// A handle other threads use to ask for a frame (`Cx::waker`, `Ui::waker`). Cheap to clone.
/// Desktop and mobile: the first call after a frame wakes the event loop. Web: the marks wait for
/// the next frame; the page script that delivered the data calls the host's `requestFrame`.
#[derive(Clone, Default)]
pub struct Waker(Arc<Shared>);

#[derive(Default)]
struct Shared {
    /// Marks are queued and no frame took them yet.
    pending: AtomicBool,
    marks: Mutex<Vec<(ControlId, Dirty)>>,
    /// Wakes the host's event loop; set once by the host.
    signal: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl Waker {
    /// Marks `id` with `dirty` at the start of the next frame and asks for that frame. A removed
    /// control is skipped. Call it once per new piece of data, never in a loop: every call is a
    /// frame.
    pub fn invalidate(&self, id: impl Into<ControlId>, dirty: Dirty) {
        let first = {
            let mut marks = self.0.marks.lock().unwrap_or_else(|e| e.into_inner());
            marks.push((id.into(), dirty));
            !self.0.pending.swap(true, Ordering::Relaxed)
        };
        if first && let Some(signal) = self.0.signal.get() {
            signal();
        }
    }

    /// Marks are waiting for a frame.
    pub(crate) fn pending(&self) -> bool {
        self.0.pending.load(Ordering::Relaxed)
    }

    /// Hands the waiting marks over in `into` (empty), keeping its allocation for the next ones.
    pub(crate) fn take(&self, into: &mut Vec<(ControlId, Dirty)>) {
        if !self.pending() {
            return;
        }
        let mut marks = self.0.marks.lock().unwrap_or_else(|e| e.into_inner());
        self.0.pending.store(false, Ordering::Relaxed);
        std::mem::swap(&mut *marks, into);
    }

    /// The host's way to wake its event loop; only the first one set counts.
    pub(crate) fn set_signal(&self, signal: Box<dyn Fn() + Send + Sync>) {
        let _ = self.0.signal.set(signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn a_burst_of_marks_wakes_the_loop_once_until_a_frame_takes_them() {
        let (waker, signals) = (Waker::default(), Arc::new(AtomicU32::new(0)));
        let counted = signals.clone();
        waker.set_signal(Box::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        }));
        let id = ControlId { index: 1, generation: 0 };
        for _ in 0..3 {
            waker.invalidate(id, Dirty::DRAW);
        }
        assert_eq!(signals.load(Ordering::Relaxed), 1);

        let mut taken = Vec::new();
        waker.take(&mut taken);
        assert_eq!(taken.len(), 3);
        assert!(!waker.pending());

        waker.invalidate(id, Dirty::DRAW);
        assert_eq!(signals.load(Ordering::Relaxed), 2, "a frame took the marks: the next one wakes again");
    }
}
