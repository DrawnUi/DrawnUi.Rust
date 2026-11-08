//! Plays a scene's animations: one main animation (looped, once and back, or held on its last
//! frame), a cross-fade from the one before, and animations layered on top for other objects.

use glam::{Quat, Vec3};

use super::scene::{Scene, Track};

pub(super) struct Player {
    /// The main animation.
    pub(super) main: Option<Track>,
    /// The animation the main one fades from, and the seconds the fade has run.
    fading: Option<(Track, f32)>,
    /// Cross-fade length, seconds.
    pub(super) fade: f32,
    /// A once-only animation goes back to this looping one when it ends.
    back_to: Option<usize>,
    /// Animations playing at the same time as the main one.
    layers: Vec<Track>,
    pub(super) playing: bool,
    pub(super) speed: f32,
}

impl Player {
    /// Loops the scene's first animation (none when it has none).
    pub(super) fn new(scene: Option<&Scene>) -> Player {
        let main = scene.filter(|s| !s.animations.is_empty()).map(|_| Track { animation: 0, time: 0.0, looped: true });
        Player { main, fading: None, fade: 0.25, back_to: None, layers: Vec::new(), playing: true, speed: 1.0 }
    }

    fn start(&mut self, animation: usize, looped: bool) {
        if let Some(old) = self.main.filter(|_| self.fade > 0.0) {
            self.fading = Some((old, 0.0));
        }
        self.main = Some(Track { animation, time: 0.0, looped });
        self.playing = true;
    }

    /// Ends a fade at once (the first animation shows without one).
    pub(super) fn fade_now(&mut self) {
        self.fading = None;
    }

    /// Loops `animation`, blending into it.
    pub(super) fn play(&mut self, animation: usize) {
        self.back_to = None;
        self.start(animation, true);
    }

    /// Plays `animation` once, then blends back into the looping one that played before it.
    pub(super) fn play_once(&mut self, animation: usize) {
        if let Some(main) = self.main.filter(|m| m.looped) {
            self.back_to = Some(main.animation);
        }
        self.start(animation, false);
    }

    /// Plays `animation` once and stays on its last frame.
    pub(super) fn hold(&mut self, animation: usize) {
        self.back_to = None;
        self.start(animation, false);
    }

    /// Loops another animation together with the main one.
    pub(super) fn add_layer(&mut self, animation: usize) {
        if !self.layers.iter().any(|l| l.animation == animation) {
            self.layers.push(Track { animation, time: 0.0, looped: true });
        }
        self.playing = true;
    }

    pub(super) fn remove_layer(&mut self, animation: usize) {
        self.layers.retain(|l| l.animation != animation);
    }

    /// Something moves: the frames go on.
    pub(super) fn busy(&self) -> bool {
        self.fading.is_some() || (self.playing && (self.main.is_some() || !self.layers.is_empty()))
    }

    /// One frame of time. Returns the once-only animation that just ended, if one did.
    pub(super) fn advance(&mut self, scene: &Scene, delta: f32) -> Option<usize> {
        if !self.playing {
            return None;
        }
        let step = delta * self.speed;
        if let Some((track, elapsed)) = &mut self.fading {
            track.time += step;
            *elapsed += delta;
            if *elapsed >= self.fade {
                self.fading = None;
            }
        }
        for layer in &mut self.layers {
            layer.time += step;
        }
        let main = self.main.as_mut()?;
        main.time += step;
        let duration = scene.animations.get(main.animation).map_or(0.0, |a| a.duration);
        if main.looped || main.time < duration {
            return None;
        }
        main.time = duration;
        let ended = main.animation;
        match self.back_to.take() {
            Some(back) => self.start(back, true),
            // Held on its last frame (a death).
            None => self.playing = !self.layers.is_empty(),
        }
        Some(ended)
    }

    /// The local transform of every node: the file's, then the animations' (blended while fading).
    pub(super) fn locals(&self, scene: &Scene) -> Vec<(Vec3, Quat, Vec3)> {
        let mut local = scene.rest();
        if let Some(main) = self.main {
            match self.fading {
                Some((from, elapsed)) => {
                    let mut old = local.clone();
                    scene.apply(from, &mut old);
                    scene.apply(main, &mut local);
                    let w = (elapsed / self.fade).clamp(0.0, 1.0);
                    for (l, o) in local.iter_mut().zip(&old) {
                        *l = (o.0.lerp(l.0, w), o.1.slerp(l.1, w), o.2.lerp(l.2, w));
                    }
                }
                None => scene.apply(main, &mut local),
            }
        }
        for layer in &self.layers {
            scene.apply(*layer, &mut local);
        }
        local
    }
}
