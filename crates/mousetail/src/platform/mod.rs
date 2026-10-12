//! Platform backends behind one small surface so the node stays platform-free.
//!
//! - `Capture`: owns the local keyboard and mouse and feeds the `Controller`.
//! - `Emulator`: injects input received from a controller.
//! - `AudioSource` / `AudioPlayer`: this machine's sound going out / another's playing here.
//! - `NowPlaying`: this machine's media controls, while another machine's sound plays here.
//! - `Ripples`: the ripple drawn where the cursor crosses.
//!
//! Platforms without a backend get stubs whose `start` fails, which simply means that machine
//! can't take that role yet.

#[cfg(not(target_os = "linux"))]
use std::collections::HashSet;

/// An edge of one of this machine's displays that leads to another computer (capture
/// backends that can't watch the whole screen use these).
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub display: String,
    pub side: mousetail_core::layout::Side,
    /// Where that display is, in this machine's logical coordinates.
    pub rect: mousetail_core::layout::Rect,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub mod macos_audio;
#[cfg(target_os = "macos")]
mod macos_emulate;
#[cfg(target_os = "macos")]
mod macos_media;
#[cfg(target_os = "macos")]
mod macos_ripple;
#[cfg(target_os = "macos")]
mod macos_tap;
#[cfg(target_os = "macos")]
pub use macos::{
    Capture, caps_lock_on, clipboard_get, clipboard_set, displays, idle_time, keyboard_blocked,
    notify,
};
#[cfg(target_os = "macos")]
pub use macos_emulate::{Emulator, on_enter};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{
    Emulator, clipboard_get, clipboard_set, command_super_keys, displays, notify, on_enter,
    wake_macs,
};

#[cfg(target_os = "linux")]
pub use linux::capture::Capture;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use stubs::Capture;

#[cfg(target_os = "linux")]
pub use linux::mpris::NowPlaying;
#[cfg(target_os = "macos")]
pub use macos_media::{NowPlaying, run_main_loop};
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use stubs::NowPlaying;

/// A media control pressed here (AirPods, media keys, Control Centre).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
}

#[cfg(target_os = "linux")]
pub use linux::player::Player as AudioPlayer;
#[cfg(target_os = "macos")]
pub use macos_audio::Player as AudioPlayer;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use stubs::AudioPlayer;

#[cfg(target_os = "linux")]
pub use linux::audio::VirtualSpeaker as AudioSource;
#[cfg(target_os = "macos")]
pub use macos_tap::SystemSound as AudioSource;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use stubs::AudioSource;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use stubs::Emulator;

#[cfg(target_os = "linux")]
pub use linux::ripple::Ripples;
#[cfg(target_os = "macos")]
pub use macos_ripple::Ripples;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use stubs::Ripples;

/// Keys that Command should turn into Super on this machine (see `keys::CommandRemap`).
#[cfg(not(target_os = "linux"))]
pub fn command_super_keys() -> HashSet<u16> {
    mousetail_core::keys::CommandRemap::default_super_keys()
}

/// Is the OS withholding keystrokes from us (e.g. macOS Secure Input)?
#[cfg(not(target_os = "macos"))]
pub fn keyboard_blocked() -> bool {
    false
}

/// How long since this machine's own keyboard or mouse was used. (Only the Mac says yet.)
#[cfg(not(target_os = "macos"))]
pub fn idle_time() -> Option<std::time::Duration> {
    None
}

/// Is Caps Lock on here? (Only the Mac says yet.)
#[cfg(not(target_os = "macos"))]
pub fn caps_lock_on() -> bool {
    false
}

/// Hardware addresses others can wake this machine through (none reported yet).
#[cfg(not(target_os = "linux"))]
pub fn wake_macs() -> Vec<String> {
    vec![]
}

/// Called when a controller's cursor arrives here.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn on_enter() {}

#[allow(dead_code)]
mod stubs {
    use std::sync::{Arc, Mutex};

    use mousetail_core::controller::{Action, Controller};
    use mousetail_core::layout::Rect;
    use mousetail_core::proto::Scroll;
    use tokio::sync::mpsc::UnboundedSender;

    pub struct Capture;

    impl Capture {
        pub fn start(
            _controller: Arc<Mutex<Controller>>,
            _actions: UnboundedSender<Action>,
            _prompt: bool,
        ) -> anyhow::Result<Self> {
            anyhow::bail!("capturing input isn't supported on this platform yet")
        }

        pub fn supported() -> bool {
            false
        }

        pub fn apply(&self, _action: &Action) {}

        pub fn set_edges(&self, _edges: Vec<super::Edge>) {}

        pub fn hide_cursor(&self) {}

        pub fn stopped(&self) -> Option<String> {
            None
        }
    }

    /// Plays another machine's sound here.
    pub struct AudioPlayer;

    impl AudioPlayer {
        pub fn start() -> anyhow::Result<Self> {
            anyhow::bail!("playing shared sound isn't supported on this platform yet")
        }

        pub fn play(&self, _packet: mousetail_core::audio::AudioPacket) {}
    }

    /// Receives this machine's media controls while another machine's sound plays here.
    pub struct NowPlaying;

    impl NowPlaying {
        pub fn start(_commands: UnboundedSender<super::MediaCommand>) -> anyhow::Result<Self> {
            anyhow::bail!("media controls aren't supported on this platform yet")
        }

        pub fn show(&self, _source: &str, _playing: bool) {}
        pub fn clear(&self) {}
    }

    /// Draws the ripple where the cursor crosses.
    pub struct Ripples;

    impl Ripples {
        pub fn start() -> anyhow::Result<Self> {
            anyhow::bail!("the crossing ripple isn't supported on this platform yet")
        }

        pub fn show(&self, _at: mousetail_core::layout::Point, _strength: f32) {}
    }

    /// A virtual speaker whose sound is sent to another machine.
    pub struct AudioSource;

    impl AudioSource {
        pub fn supported() -> bool {
            false
        }

        pub fn start(
            _id: &str,
            _description: &str,
        ) -> anyhow::Result<(Self, std::sync::mpsc::Receiver<Vec<i16>>)> {
            anyhow::bail!("sharing sound isn't supported on this platform yet")
        }
    }

    pub struct Emulator;

    impl Emulator {
        pub fn start() -> anyhow::Result<Self> {
            anyhow::bail!("injecting input isn't supported on this platform yet")
        }

        pub fn supported() -> bool {
            false
        }

        pub fn set_bounds(&self, _bounds: Rect) {}
        pub fn motion(&self, _x: f64, _y: f64) {}
        pub fn button(&self, _code: u16, _down: bool) {}
        pub fn key(&self, _code: u16, _down: bool) {}
        pub fn scroll(&self, _scroll: Scroll) {}
        pub fn trackpad_scroll(&self, _dx: f64, _dy: f64) {}
        pub fn trackpad_scroll_end(&self) {}
        pub fn release_all(&self) {}
        pub fn media(&self, _key: mousetail_core::proto::MediaKey) {}
    }
}

/// A readable machine name: the one people see in Finder or their shell prompt.
pub fn machine_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .map(|h| h.trim_end_matches(".local").to_string())
        .unwrap_or_else(|| "MouseTail".into())
}
