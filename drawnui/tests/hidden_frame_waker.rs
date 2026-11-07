//! Feature `designer`: a hidden frame (drawn while the window shows none) takes the waker's
//! marks, so the next request for a frame reaches the host again.
#![cfg(feature = "designer")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{App, Host};

#[test]
fn every_request_after_a_hidden_frame_reaches_the_host() {
    let requests = Arc::new(AtomicU32::new(0));
    let mut host = Headless::new(Ui::new((), |_| SkiaLayout::new().fill()), 100, 100, 1.0);
    let mut h = Host::default();
    let counted = requests.clone();
    h.on_frame_requested(move || {
        counted.fetch_add(1, Ordering::Relaxed);
    });
    App::init(&mut host.ui, &mut h);
    host.settle();
    let waker = host.ui.waker();
    for round in 1..=3 {
        waker.request_hidden_frame();
        assert_eq!(requests.load(Ordering::Relaxed), round, "round {round}: the request reached the host");
        // The host's hidden frame: no prepare before it, the frame alone.
        assert!(App::take_hidden_frame_request(&mut host.ui));
        host.frame_after(16.0);
        assert!(!host.ui.needs_frame(), "round {round}: nothing left pending");
    }
}
