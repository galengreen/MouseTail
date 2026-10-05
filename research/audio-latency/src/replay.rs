//! Replay a recorded packet trace (`t seq stream` per line, seconds on the listener's clock)
//! through the old and new playout buffers, with a sound card pulling 512 frames at a time (`REPLAY_PULL`),
//! and compare how much cushion each kept and how often each ran dry.

use mousetail_core::audio::{CHANNELS, FRAME, Playout};

use crate::old_playout::OldPlayout;

/// What the sim needs from either buffer.
pub trait Buffer {
    fn reset(&mut self);
    fn push(&mut self, pcm: &[i16]);
    fn pull(&mut self, out: &mut [f32]);
    fn level_ms(&self) -> usize;
    fn target_ms(&self) -> usize;
    fn dropouts(&mut self) -> u32;
    fn skipped_ms(&mut self) -> usize;
}

macro_rules! buffer {
    ($t:ty) => {
        impl Buffer for $t {
            fn reset(&mut self) {
                <$t>::reset(self)
            }
            fn push(&mut self, pcm: &[i16]) {
                <$t>::push(self, pcm)
            }
            fn pull(&mut self, out: &mut [f32]) {
                <$t>::pull(self, out, 2, 48_000)
            }
            fn level_ms(&self) -> usize {
                <$t>::level_ms(self)
            }
            fn target_ms(&self) -> usize {
                <$t>::target_ms(self)
            }
            fn dropouts(&mut self) -> u32 {
                self.stats().dropouts
            }
            fn skipped_ms(&mut self) -> usize {
                self.stats().skipped_ms
            }
        }
    };
}
buffer!(Playout);
buffer!(OldPlayout);

const IDLE_S: f64 = 3.0;

pub struct Result {
    pub dropouts: u32,
    pub mean_level_ms: f64,
    pub p90_level_ms: usize,
    pub mean_target_ms: f64,
    pub skipped_ms: usize,
    pub minutes: f64,
}

pub fn run(trace: &[(f64, u32, u32)], buffer: &mut dyn Buffer) -> Result {
    // The sender sends ~0.5 s of silence before going quiet: those packets carry silence.
    let mut silent = vec![false; trace.len()];
    for i in 0..trace.len() {
        let gap = trace
            .get(i + 1)
            .is_none_or(|n| n.2 != trace[i].2 || n.1.wrapping_sub(trace[i].1) as i32 > 5);
        if gap {
            for s in silent.iter_mut().take(i + 1).skip(i.saturating_sub(49)) {
                *s = true;
            }
        }
    }
    let tone: Vec<i16> = (0..FRAME)
        .flat_map(|i| {
            let v = ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 12_000.0) as i16;
            [v, v]
        })
        .collect();
    let quiet = vec![0i16; FRAME * CHANNELS];

    // The sound card's buffer, in frames.
    let pull: usize = std::env::var("REPLAY_PULL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let start = trace[0].0;
    let mut next_pull = start;
    let mut out = vec![0f32; pull * 2];
    let (mut stream, mut next_seq) = (None, 0u32);
    let mut last_packet = start;
    let mut levels = vec![];
    let mut targets = 0.0;
    let (mut dropouts, mut skipped) = (0, 0);
    let mut i = 0;
    let log = std::env::var_os("REPLAY_LOG").is_some();
    let end = trace.last().unwrap().0 + 1.0;
    while next_pull < end {
        // Everything that arrived before this pull.
        while i < trace.len() && trace[i].0 <= next_pull {
            let (t, seq, st) = trace[i];
            last_packet = t;
            let ahead = seq.wrapping_sub(next_seq) as i32;
            let fresh = stream != Some(st) || ahead > 5;
            if fresh {
                buffer.reset();
                stream = Some(st);
            } else if ahead < 0 {
                i += 1;
                continue;
            } else {
                for _ in 0..ahead {
                    buffer.push(&tone); // concealed
                }
            }
            buffer.push(if silent[i] { &quiet } else { &tone });
            next_seq = seq.wrapping_add(1);
            i += 1;
        }
        if next_pull - last_packet > IDLE_S {
            buffer.reset();
        }
        let level = buffer.level_ms();
        buffer.pull(&mut out);
        if out.iter().any(|s| s.abs() > 0.01) {
            levels.push(level);
            targets += buffer.target_ms() as f64;
        }
        if log && ((next_pull - start) % 10.0) < pull as f64 / 48_000.0 {
            println!(
                "  {:>5.0} s  level {:>3} ms  target {:>3} ms",
                next_pull - start,
                level,
                buffer.target_ms()
            );
        }
        dropouts += buffer.dropouts();
        skipped += buffer.skipped_ms();
        next_pull += pull as f64 / 48_000.0;
    }
    let n = levels.len().max(1);
    let mean = levels.iter().sum::<usize>() as f64 / n as f64;
    levels.sort();
    Result {
        dropouts,
        mean_level_ms: mean,
        p90_level_ms: levels.get(n * 9 / 10).copied().unwrap_or(0),
        mean_target_ms: targets / n as f64,
        skipped_ms: skipped,
        minutes: (end - start) / 60.0,
    }
}

pub fn main(path: &str) -> anyhow::Result<()> {
    let trace: Vec<(f64, u32, u32)> = std::fs::read_to_string(path)?
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            Some((
                w.next()?.parse().ok()?,
                w.next()?.parse().ok()?,
                w.next()?.parse().ok()?,
            ))
        })
        .collect();
    anyhow::ensure!(!trace.is_empty(), "empty trace");
    let show = |name: &str, r: Result| {
        println!(
            "{name:<4} dropouts {:>4} ({:.2}/min)  cushion mean {:>5.1} ms, p90 {:>3} ms, target mean {:>5.1} ms, skipped {} ms",
            r.dropouts,
            r.dropouts as f64 / r.minutes,
            r.mean_level_ms,
            r.p90_level_ms,
            r.mean_target_ms,
            r.skipped_ms
        )
    };
    println!(
        "{} packets over {:.1} min",
        trace.len(),
        (trace.last().unwrap().0 - trace[0].0) / 60.0
    );
    show("old", run(&trace, &mut OldPlayout::default()));
    show("new", run(&trace, &mut Playout::default()));
    if std::env::var_os("REPLAY_LOG").is_some() {
        run(&trace, &mut Playout::default());
    }
    Ok(())
}
