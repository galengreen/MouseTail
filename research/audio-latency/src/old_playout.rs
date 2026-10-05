//! The playout buffer as it was in 0.2.8, to compare against (see `replay`).

use std::collections::VecDeque;

use mousetail_core::audio::{CHANNELS, SAMPLE_RATE};

/// The receiving side's cushion between the network and the sound card.
///
/// Fills with 48 kHz stereo; the device pulls at its own rate and channel count. Starts
/// playing once `target` is banked and goes quiet and re-buffers on underrun.
///
/// The two computers' sound clocks never quite agree, so left alone the cushion would slowly
/// drain or overfill and click every few minutes. Instead playback runs a hair faster or
/// slower (at most 0.2%, too little to hear) to hold the cushion at its target. A dropout
/// mid-sound means the network is choppier than the cushion allows, so it grows, and shrinks
/// back after a calm minute. Bursts that overfill it by more than `headroom` are skipped.
pub struct OldPlayout {
    samples: VecDeque<i16>,
    playing: bool,
    /// The cushion to aim for, between `base` and `MAX_TARGET_MS`, in samples.
    target: usize,
    base: usize,
    headroom: usize,
    /// Fractional read position for rate conversion, in 48 kHz frames.
    phase: f64,
    /// The cushion averaged over a couple of seconds, in samples, to steer the rate by.
    level: f64,
    /// Seconds played since the cushion last had to grow.
    calm: f64,
    /// Fades in after a re-buffer and out from the last frame on underrun, so neither clicks.
    gain: f32,
    last: [f32; 2],
    /// The current rate adjustment, and what went wrong since the last `stats`.
    nudge: f64,
    dropouts: u32,
    skipped: usize,
}

/// How playback has been going, for the log.
#[derive(Debug)]
#[allow(dead_code)] // Kept as it was.
pub struct OldStats {
    pub level_ms: usize,
    pub target_ms: usize,
    /// How much faster (or slower, negative) than the device sound is being played, which
    /// settles at the difference between the two computers' clocks.
    pub nudge_ppm: i32,
    pub dropouts: u32,
    pub skipped_ms: usize,
}

const MAX_TARGET_MS: usize = 160;
const GROW_MS: usize = 20;
const SHRINK_MS: usize = 10;
const SHRINK_AFTER_S: f64 = 60.0;
/// Rate adjustment per millisecond the cushion is off target, and its limit.
const NUDGE_PER_MS: f64 = 0.000_05;
const MAX_NUDGE: f64 = 0.002;
const SMOOTHING_S: f64 = 2.0;
const FADE_S: f32 = 0.005;
/// Quieter than this, a dropout is the end of the sound rather than a glitch.
const SILENT: f32 = 0.001;

impl Default for OldPlayout {
    fn default() -> Self {
        Self::new(40, 120)
    }
}

impl OldPlayout {
    pub fn new(target_ms: usize, max_ms: usize) -> Self {
        let target = target_ms * PER_MS;
        Self {
            samples: VecDeque::new(),
            playing: false,
            target,
            base: target,
            headroom: max_ms.saturating_sub(target_ms) * PER_MS,
            phase: 0.0,
            level: 0.0,
            calm: 0.0,
            gain: 0.0,
            last: [0.0; 2],
            nudge: 0.0,
            dropouts: 0,
            skipped: 0,
        }
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.playing = false;
        self.phase = 0.0;
    }

    pub fn push(&mut self, pcm: &[i16]) {
        self.samples.extend(pcm);
        if self.samples.len() > self.target + self.headroom {
            let excess = self.samples.len() - self.target;
            self.samples.drain(..excess - excess % CHANNELS);
            self.skipped += excess;
        }
    }

    /// Buffered audio, in milliseconds.
    pub fn level_ms(&self) -> usize {
        self.samples.len() / PER_MS
    }

    /// The cushion currently aimed for, in milliseconds.
    pub fn target_ms(&self) -> usize {
        self.target / PER_MS
    }

    /// Fill `out` (interleaved, `channels` per frame, at `rate` Hz) with audio or silence.
    pub fn pull(&mut self, out: &mut [f32], channels: usize, rate: u32) {
        let channels = channels.max(1);
        let rate = rate.max(1) as f64;
        if !self.playing && self.samples.len() >= self.target {
            self.playing = true;
            self.level = self.samples.len() as f64;
            self.gain = 0.0;
        }
        let frames = (out.len() / channels) as f64;
        let mut step = SAMPLE_RATE as f64 / rate;
        if self.playing {
            let a = (frames / (rate * SMOOTHING_S)).min(1.0);
            self.level += (self.samples.len() as f64 - self.level) * a;
            let off_ms = (self.level - self.target as f64) / PER_MS as f64;
            self.nudge = (off_ms * NUDGE_PER_MS).clamp(-MAX_NUDGE, MAX_NUDGE);
            step *= 1.0 + self.nudge;
            self.calm += frames / rate;
            if self.calm > SHRINK_AFTER_S && self.target > self.base {
                self.target = (self.target - SHRINK_MS * PER_MS).max(self.base);
                self.calm = 0.0;
            }
        }
        let fade = 1.0 / (FADE_S * rate as f32);
        for frame in out.chunks_mut(channels) {
            let available = self.samples.len() / CHANNELS;
            let [l, r] = if self.playing && available >= 2 {
                // Linear interpolation between the two nearest 48 kHz frames.
                let i = self.phase as usize;
                let t = (self.phase - i as f64) as f32;
                let at = |f: usize, c: usize| self.samples[f * CHANNELS + c] as f32 / 32768.0;
                self.gain = (self.gain + fade).min(1.0);
                self.last = [
                    (at(i, 0) * (1.0 - t) + at(i + 1, 0) * t) * self.gain,
                    (at(i, 1) * (1.0 - t) + at(i + 1, 1) * t) * self.gain,
                ];
                self.phase += step;
                let consumed = self.phase as usize;
                if consumed > 0 {
                    let n = (consumed * CHANNELS).min(self.samples.len());
                    self.samples.drain(..n);
                    self.phase -= consumed as f64;
                }
                self.last
            } else {
                if self.playing {
                    self.underrun();
                }
                let g = (1.0 - fade).max(0.0);
                self.last = self.last.map(|s| if s.abs() < 1e-6 { 0.0 } else { s * g });
                self.last
            };
            match frame.len() {
                1 => frame[0] = (l + r) / 2.0,
                _ => {
                    frame[0] = l;
                    frame[1] = r;
                    frame[2..].fill(0.0);
                }
            }
        }
    }

    /// How it's going, counting dropouts and skips since the last call.
    pub fn stats(&mut self) -> OldStats {
        OldStats {
            level_ms: self.level_ms(),
            target_ms: self.target_ms(),
            nudge_ppm: (self.nudge * 1e6) as i32,
            dropouts: std::mem::take(&mut self.dropouts),
            skipped_ms: std::mem::take(&mut self.skipped) / PER_MS,
        }
    }

    fn underrun(&mut self) {
        self.playing = false;
        let mid_sound = self.last.iter().any(|s| s.abs() > SILENT);
        if mid_sound {
            self.dropouts += 1;
        }
        if mid_sound && self.target < MAX_TARGET_MS * PER_MS {
            self.target = (self.target + GROW_MS * PER_MS).min(MAX_TARGET_MS * PER_MS);
            self.calm = 0.0;
        }
    }
}

/// Samples (interleaved) per millisecond.
const PER_MS: usize = SAMPLE_RATE as usize / 1000 * CHANNELS;
