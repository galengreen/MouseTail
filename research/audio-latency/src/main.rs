//! Spike: how long does sound take from an app on one computer to the other's output?
//!
//! The sending computer (Linux) plays a short rising chirp every second into its default
//! output (MouseTail's virtual speaker while it's sending), over quiet noise so the stream
//! never pauses, and prints the wall-clock time each chirp entered the PipeWire graph.
//!
//!   cargo run -p spike-audio-latency -- send [secs]
//!
//! The listening Mac taps the MouseTail daemon's output (plus its own falling chirps, played
//! locally to calibrate what the tap's timestamps mean) and writes it all to a directory for
//! `analyse.py`, along with the output device's reported latency.
//!
//!   cargo run -p spike-audio-latency -- listen [secs] [dir]
//!
//! `measure.sh` runs both with the clocks lined up.
//!
//! `replay <trace>` runs a recorded packet trace (`trace.sh` pulls one from a development
//! build's log) through the old and new playout buffers offline.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod mac;
mod old_playout;
mod replay;

pub const RATE: u32 = 48_000;
/// One chirp a second.
pub const PERIOD: u64 = RATE as u64;
const CHIRP_S: f64 = 0.02;

/// A 20 ms Hann-windowed linear chirp between 1.5 and 7 kHz, rising or falling.
pub fn chirp(rising: bool, amplitude: f32) -> Vec<f32> {
    let n = (CHIRP_S * RATE as f64) as usize;
    let (f0, f1) = if rising {
        (1500.0, 7000.0)
    } else {
        (7000.0, 1500.0)
    };
    (0..n)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let phase = std::f64::consts::TAU * (f0 * t + (f1 - f0) * t * t / (2.0 * CHIRP_S));
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            (phase.sin() * window) as f32 * amplitude
        })
        .collect()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let secs = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    match args.first().map(String::as_str) {
        #[cfg(target_os = "linux")]
        Some("send") => linux::send(secs),
        #[cfg(target_os = "macos")]
        Some("listen") => mac::listen(
            secs,
            args.get(2)
                .map(String::as_str)
                .unwrap_or("/tmp/audio-latency"),
        ),
        Some("replay") => replay::main(args.get(1).map(String::as_str).unwrap_or("/tmp/trace.txt")),
        _ => anyhow::bail!(
            "usage: send [secs] (Linux) | listen [secs] [dir] (macOS) | replay [trace]"
        ),
    }
}
