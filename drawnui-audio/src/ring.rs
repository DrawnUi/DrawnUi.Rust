//! A ring of samples between one writer (the app) and one reader (the mixer): fixed size, no lock,
//! no allocation after it is made. Samples are stored as bits in atomics, so the code is safe Rust.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub(crate) struct Ring {
    slots: Box<[AtomicU32]>,
    /// Samples ever written / read; the difference is what is queued.
    written: AtomicUsize,
    read: AtomicUsize,
}

impl Ring {
    pub(crate) fn new(capacity: usize) -> Ring {
        Ring { slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(), written: AtomicUsize::new(0), read: AtomicUsize::new(0) }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Samples waiting to be read.
    pub(crate) fn len(&self) -> usize {
        self.written.load(Ordering::Acquire).wrapping_sub(self.read.load(Ordering::Acquire))
    }

    /// Writer side: as many of `samples` as fit; the rest is dropped. Returns how many went in.
    pub(crate) fn push(&self, samples: &[f32]) -> usize {
        let w = self.written.load(Ordering::Relaxed);
        let free = self.capacity() - w.wrapping_sub(self.read.load(Ordering::Acquire));
        let n = samples.len().min(free);
        for (i, s) in samples[..n].iter().enumerate() {
            self.slots[(w + i) % self.capacity()].store(s.to_bits(), Ordering::Relaxed);
        }
        self.written.store(w.wrapping_add(n), Ordering::Release);
        n
    }

    /// Reader side: fills `out` with what is queued, from the oldest; returns how many it read.
    pub(crate) fn pop(&self, out: &mut [f32]) -> usize {
        let r = self.read.load(Ordering::Relaxed);
        let n = out.len().min(self.written.load(Ordering::Acquire).wrapping_sub(r));
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = f32::from_bits(self.slots[(r + i) % self.capacity()].load(Ordering::Relaxed));
        }
        self.read.store(r.wrapping_add(n), Ordering::Release);
        n
    }

    /// Reader side: forgets everything queued.
    pub(crate) fn skip_all(&self) {
        self.read.store(self.written.load(Ordering::Acquire), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::Ring;

    #[test]
    fn keeps_order_drops_what_does_not_fit_and_wraps() {
        let ring = Ring::new(4);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0, 4.0, 5.0]), 4);
        assert_eq!(ring.len(), 4);
        let mut out = [0.0; 3];
        assert_eq!(ring.pop(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert_eq!(ring.push(&[6.0, 7.0, 8.0]), 3);
        let mut out = [0.0; 8];
        assert_eq!(ring.pop(&mut out), 4);
        assert_eq!(&out[..4], &[4.0, 6.0, 7.0, 8.0]);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn one_writer_one_reader_on_two_threads() {
        let ring = std::sync::Arc::new(Ring::new(64));
        let reader = ring.clone();
        let total = 200_000;
        let handle = std::thread::spawn(move || {
            let (mut expect, mut buf) = (0u32, [0.0f32; 16]);
            while expect < total {
                let n = reader.pop(&mut buf);
                for &s in &buf[..n] {
                    assert_eq!(s, expect as f32);
                    expect += 1;
                }
            }
        });
        let mut next = 0u32;
        while next < total {
            let chunk: Vec<f32> = (next..(next + 7).min(total)).map(|v| v as f32).collect();
            next += ring.push(&chunk) as u32;
        }
        handle.join().unwrap();
    }
}
