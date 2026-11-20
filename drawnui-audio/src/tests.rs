use super::*;

fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

/// A mono ramp 0.1, 0.2, ... at the output's rate.
fn ramp(n: usize, rate: u32) -> Sample {
    Sample::from_frames((1..=n).map(|i| i as f32 / 10.0).collect(), Layout::Mono, rate)
}

#[test]
fn mixes_streams_and_generators_with_their_volumes() {
    let audio = Audio::null(48_000, 2);
    let chip = audio.stream(Layout::Mono, 100);
    chip.push_slice(&[0.5; 4]);
    let stereo = audio.stream(Layout::Stereo, 100);
    stereo.push_slice(&[0.1, -0.1, 0.1, -0.1]);
    stereo.set_volume(2.0);
    let tone = audio.generator(Layout::Mono, |b| b.fill(0.25));
    tone.set_volume(0.5);
    let out = audio.render(3);
    assert!(close(&out, &[0.825, 0.425, 0.825, 0.425, 0.625, 0.625]), "{out:?}");
    assert_eq!(chip.queued(), 1);
}

#[test]
fn a_full_stream_drops_and_a_dry_one_plays_silence() {
    let audio = Audio::null(1000, 1);
    let s = audio.stream(Layout::Mono, 10);
    assert_eq!(s.room(), 10);
    assert_eq!(s.push_slice(&[1.0; 15]), 10);
    assert!(!s.push(1.0));
    assert!((s.queued_ms() - 10.0).abs() < 1e-3);
    let out = audio.render(12);
    assert_eq!(&out[..10], &[1.0; 10]);
    assert_eq!(&out[10..], &[0.0; 2]);
}

#[test]
fn dropped_sources_leave_the_mix_and_pause_holds_everything() {
    let audio = Audio::null(1000, 1);
    let g = audio.generator(Layout::Mono, |b| b.fill(0.5));
    assert_eq!(audio.render(1), [0.5]);
    drop(g);
    assert_eq!(audio.render(1), [0.0]);
    let s = audio.stream(Layout::Mono, 10);
    s.push_slice(&[0.3; 4]);
    audio.pause();
    assert!(!audio.is_running());
    assert_eq!(audio.render(2), [0.0, 0.0]);
    assert_eq!(s.queued(), 4);
    audio.resume();
    s.clear();
    assert_eq!(audio.render(2), [0.0, 0.0]);
}

#[test]
fn master_volume_scales_and_the_sum_is_clamped() {
    let audio = Audio::null(1000, 1);
    let _a = audio.generator(Layout::Mono, |b| b.fill(0.8));
    let _b = audio.generator(Layout::Mono, |b| b.fill(0.8));
    assert_eq!(audio.render(1), [1.0]);
    audio.set_volume(0.25);
    assert!((audio.render(1)[0] - 0.4).abs() < 1e-6);
}

#[test]
fn a_channel_plays_a_sample_once_then_is_free() {
    let audio = Audio::null(1000, 1);
    let ch = audio.channel(2);
    ch.play(&ramp(4, 1000), Note::default());
    assert!(ch.is_playing());
    assert!(close(&audio.render(6), &[0.1, 0.2, 0.3, 0.4, 0.0, 0.0]));
    assert!(!ch.is_playing());
}

#[test]
fn a_sample_plays_at_its_own_pitch_on_any_output() {
    // A 500 Hz sample on a 1000 Hz output: each sample frame lasts two output frames.
    let audio = Audio::null(1000, 1);
    audio.channel(0).play(&ramp(3, 500), Note::default());
    let out = audio.render(6);
    assert!(close(&[out[0], out[2], out[4]], &[0.1, 0.2, 0.3]), "{out:?}");
    assert!(out[1] > 0.1 && out[1] < 0.2, "interpolated between: {out:?}");
}

#[test]
fn speed_raises_the_pitch_live() {
    let audio = Audio::null(1000, 1);
    let ch = audio.channel(0);
    ch.play(&ramp(8, 1000), Note { speed: 2.0, ..Note::default() });
    assert!(close(&audio.render(2), &[0.1, 0.3]));
    ch.set_speed(1.0);
    assert!(close(&audio.render(2), &[0.5, 0.6]));
}

#[test]
fn volumes_multiply_and_mono_pans_with_equal_power() {
    let audio = Audio::null(1000, 2);
    let ch = audio.channel(0);
    ch.set_volume(0.5);
    ch.play(&Sample::from_frames(vec![1.0; 8], Layout::Mono, 1000), Note { volume: 0.5, ..Note::default() });
    assert!(close(&audio.render(1), &[0.25, 0.25]), "center: full in both");
    ch.set_pan(-1.0);
    let hard_left = audio.render(1);
    assert!((hard_left[0] - 0.25 * std::f32::consts::SQRT_2).abs() < 1e-4 && hard_left[1].abs() < 1e-6, "{hard_left:?}");
    ch.set_note_volume(0.0);
    assert!(close(&audio.render(1), &[0.0, 0.0]));
    // A note can place the channel itself (Arkanoid's balance).
    let picked = audio.play(&Sample::from_frames(vec![1.0; 8], Layout::Mono, 1000), Note { volume: 0.5, pan: Some(1.0), ..Note::default() });
    let right = audio.render(1);
    assert!((picked.pan() - 1.0).abs() < 1e-6 && right[0].abs() < 1e-6 && (right[1] - 0.5 * std::f32::consts::SQRT_2).abs() < 1e-4, "{right:?}");
}

#[test]
fn a_new_note_cuts_the_old_one_unless_it_continues() {
    let audio = Audio::null(1000, 1);
    let ch = audio.channel(0);
    let long = Sample::from_frames(vec![0.25; 100], Layout::Mono, 1000);
    ch.play(&long, Note::default());
    audio.render(1);
    ch.play(&long, Note::default());
    assert!(close(&audio.render(1), &[0.25]), "cut: one note");
    ch.play(&long, Note { keep_previous: true, ..Note::default() });
    assert!(close(&audio.render(1), &[0.5]), "continue: both");
    ch.stop();
    assert!(close(&audio.render(1), &[0.25]), "a note cut stops only the channel's own note");
}

#[test]
fn forward_and_ping_pong_loops() {
    let audio = Audio::null(1000, 1);
    let s = ramp(4, 1000).with_loop(Loop::Forward { start: 1, end: 3 });
    audio.channel(0).play(&s, Note::default());
    assert!(close(&audio.render(7), &[0.1, 0.2, 0.3, 0.2, 0.3, 0.2, 0.3]));
    let p = ramp(4, 1000).with_loop(Loop::PingPong { start: 0, end: 4 });
    audio.channel(0).play(&p, Note::default());
    assert!(close(&audio.render(8), &[0.1, 0.2, 0.3, 0.4, 0.3, 0.2, 0.1, 0.2]));
    let whole = ramp(2, 1000);
    audio.channel(0).play(&whole, Note { looped: true, ..Note::default() });
    assert!(close(&audio.render(5), &[0.1, 0.2, 0.1, 0.2, 0.1]));
}

#[test]
fn play_takes_a_free_channel_skips_reserved_ones_and_cuts_the_oldest() {
    let audio = Audio::null(1000, 1);
    audio.set_channels(3);
    audio.channel(0).set_reserved(true);
    let s = Sample::from_frames(vec![0.1; 100], Layout::Mono, 1000);
    assert_eq!(audio.play(&s, Note::default()).index(), 1);
    assert_eq!(audio.play(&s, Note::default()).index(), 2);
    // All free ones busy: the oldest note goes.
    assert_eq!(audio.play(&s, Note::default()).index(), 1);
    assert_eq!(audio.play(&s, Note::default()).index(), 2);
    assert!(!audio.channel(0).is_playing());
}

#[test]
fn mute_and_solo() {
    let audio = Audio::null(1000, 1);
    let s = Sample::from_frames(vec![0.25; 10], Layout::Mono, 1000);
    audio.channel(0).play(&s, Note::default());
    audio.channel(1).play(&s, Note::default());
    assert!(close(&audio.render(1), &[0.5]));
    audio.channel(0).set_muted(true);
    assert!(close(&audio.render(1), &[0.25]));
    audio.channel(0).set_muted(false);
    audio.channel(1).set_solo(true);
    assert!(close(&audio.render(1), &[0.25]));
}

/// A 16-bit PCM WAV file in memory.
#[cfg(feature = "files")]
fn wav(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

#[cfg(feature = "files")]
#[test]
fn a_wav_file_decodes_and_plays() {
    let bytes = wav(&[16384, -16384, 8192, -8192], 2, 22_050);
    let s = Sample::decode(&bytes).unwrap();
    assert_eq!((s.layout(), s.rate(), s.frames()), (Layout::Stereo, 22_050, 2));
    let audio = Audio::null(22_050, 2);
    audio.channel(0).play(&s, Note::default());
    assert!(close(&audio.render(3), &[0.5, -0.5, 0.25, -0.25, 0.0, 0.0]));
}

#[cfg(feature = "files")]
#[test]
fn music_streams_from_its_file_and_loops() {
    // Two seconds of a mono ramp: longer than what is decoded ahead.
    let n = 2 * 8000;
    let samples: Vec<i16> = (0..n).map(|i| ((i % 100) * 300) as i16).collect();
    let music = Sample::streamed(wav(&samples, 1, 8000)).unwrap();
    assert_eq!((music.layout(), music.rate(), music.frames()), (Layout::Mono, 8000, 0));
    let audio = Audio::null(8000, 1);
    let ch = audio.channel(5);
    ch.play(&music, Note { looped: true, ..Note::default() });
    let mut out = Vec::new();
    for _ in 0..50 {
        out.extend(audio.render(800)); // 5 s in 100 ms steps, as an output would ask
    }
    for (i, v) in out.iter().enumerate().step_by(997) {
        let expect = ((i % n) % 100) as f32 * 300.0 / 32768.0;
        assert!((v - expect).abs() < 1e-3, "frame {i}: {v} vs {expect}");
    }
    assert!(ch.is_playing(), "loops");
    ch.stop();
    audio.render(1);
    assert!(!ch.is_playing());
    audio.render(1); // the next feed lets go of the decoder
    assert!(audio.output.feeds.lock().unwrap().is_empty(), "the decoder stops with its note");
}

#[cfg(feature = "files")]
#[test]
fn music_that_does_not_loop_ends() {
    let music = Sample::streamed(wav(&[8192; 4000], 1, 8000)).unwrap();
    let audio = Audio::null(8000, 1);
    let ch = audio.channel(0);
    ch.play(&music, Note::default());
    audio.render(3999);
    assert!(ch.is_playing());
    let tail = audio.render(100);
    assert!((tail[0] - 0.25).abs() < 1e-3 && tail[99] == 0.0, "{:?}", &tail[..4]);
    assert!(!ch.is_playing());
}
