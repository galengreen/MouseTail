//! Key codes and translation.
//!
//! The wire format is Linux evdev codes. Capture backends translate their native codes into
//! evdev; the receiving side decides what modifiers mean for its platform.

use std::collections::HashSet;

pub mod ev {
    pub const ESC: u16 = 1;
    pub const BACKSPACE: u16 = 14;
    pub const TAB: u16 = 15;
    pub const ENTER: u16 = 28;
    pub const LEFTCTRL: u16 = 29;
    pub const LEFTSHIFT: u16 = 42;
    pub const RIGHTSHIFT: u16 = 54;
    pub const LEFTALT: u16 = 56;
    pub const SPACE: u16 = 57;
    pub const CAPSLOCK: u16 = 58;
    pub const RIGHTCTRL: u16 = 97;
    pub const RIGHTALT: u16 = 100;
    pub const UP: u16 = 103;
    pub const LEFT: u16 = 105;
    pub const RIGHT: u16 = 106;
    pub const DOWN: u16 = 108;
    pub const LEFTMETA: u16 = 125;
    pub const RIGHTMETA: u16 = 126;
    pub const X: u16 = 45;
    pub const C: u16 = 46;
    pub const V: u16 = 47;
    pub const NEXTSONG: u16 = 163;
    pub const PLAYPAUSE: u16 = 164;
    pub const PREVIOUSSONG: u16 = 165;

    pub const BTN_LEFT: u16 = 0x110;
    pub const BTN_RIGHT: u16 = 0x111;
    pub const BTN_MIDDLE: u16 = 0x112;
    pub const BTN_SIDE: u16 = 0x113;
    pub const BTN_EXTRA: u16 = 0x114;

    pub fn is_meta(code: u16) -> bool {
        code == LEFTMETA || code == RIGHTMETA
    }

    pub fn is_ctrl(code: u16) -> bool {
        code == LEFTCTRL || code == RIGHTCTRL
    }

    pub fn is_alt(code: u16) -> bool {
        code == LEFTALT || code == RIGHTALT
    }

    pub fn is_button(code: u16) -> bool {
        (0x110..0x120).contains(&code)
    }
}

/// macOS virtual key code (`kVK_*`) to evdev. `None` for keys with no equivalent (Fn).
pub fn mac_to_evdev(vk: u16) -> Option<u16> {
    Some(match vk {
        0x00 => 30,  // A
        0x01 => 31,  // S
        0x02 => 32,  // D
        0x03 => 33,  // F
        0x04 => 35,  // H
        0x05 => 34,  // G
        0x06 => 44,  // Z
        0x07 => 45,  // X
        0x08 => 46,  // C
        0x09 => 47,  // V
        0x0A => 86,  // ISO section (§) -> 102ND
        0x0B => 48,  // B
        0x0C => 16,  // Q
        0x0D => 17,  // W
        0x0E => 18,  // E
        0x0F => 19,  // R
        0x10 => 21,  // Y
        0x11 => 20,  // T
        0x12 => 2,   // 1
        0x13 => 3,   // 2
        0x14 => 4,   // 3
        0x15 => 5,   // 4
        0x16 => 7,   // 6
        0x17 => 6,   // 5
        0x18 => 13,  // =
        0x19 => 10,  // 9
        0x1A => 8,   // 7
        0x1B => 12,  // -
        0x1C => 9,   // 8
        0x1D => 11,  // 0
        0x1E => 27,  // ]
        0x1F => 24,  // O
        0x20 => 22,  // U
        0x21 => 26,  // [
        0x22 => 23,  // I
        0x23 => 25,  // P
        0x24 => 28,  // Return
        0x25 => 38,  // L
        0x26 => 36,  // J
        0x27 => 40,  // '
        0x28 => 37,  // K
        0x29 => 39,  // ;
        0x2A => 43,  // \
        0x2B => 51,  // ,
        0x2C => 53,  // /
        0x2D => 49,  // N
        0x2E => 50,  // M
        0x2F => 52,  // .
        0x30 => 15,  // Tab
        0x31 => 57,  // Space
        0x32 => 41,  // `
        0x33 => 14,  // Delete (backspace)
        0x34 => 96,  // Enter on some keyboards -> KPENTER
        0x35 => 1,   // Escape
        0x36 => 126, // Right Command
        0x37 => 125, // Command
        0x38 => 42,  // Shift
        0x39 => 58,  // Caps Lock
        0x3A => 56,  // Option
        0x3B => 29,  // Control
        0x3C => 54,  // Right Shift
        0x3D => 100, // Right Option
        0x3E => 97,  // Right Control
        0x40 => 187, // F17
        0x41 => 83,  // Keypad .
        0x43 => 55,  // Keypad *
        0x45 => 78,  // Keypad +
        0x47 => 69,  // Keypad Clear -> NumLock
        0x48 => 115, // Volume Up
        0x49 => 114, // Volume Down
        0x4A => 113, // Mute
        0x4B => 98,  // Keypad /
        0x4C => 96,  // Keypad Enter
        0x4E => 74,  // Keypad -
        0x4F => 188, // F18
        0x50 => 189, // F19
        0x51 => 117, // Keypad =
        0x52 => 82,  // Keypad 0
        0x53 => 79,  // Keypad 1
        0x54 => 80,  // Keypad 2
        0x55 => 81,  // Keypad 3
        0x56 => 75,  // Keypad 4
        0x57 => 76,  // Keypad 5
        0x58 => 77,  // Keypad 6
        0x59 => 71,  // Keypad 7
        0x5A => 190, // F20
        0x5B => 72,  // Keypad 8
        0x5C => 73,  // Keypad 9
        0x5D => 124, // JIS Yen
        0x5E => 89,  // JIS Underscore (RO)
        0x5F => 95,  // JIS Keypad comma
        0x60 => 63,  // F5
        0x61 => 64,  // F6
        0x62 => 65,  // F7
        0x63 => 61,  // F3
        0x64 => 66,  // F8
        0x65 => 67,  // F9
        0x66 => 94,  // JIS Eisu -> MUHENKAN
        0x67 => 87,  // F11
        0x68 => 93,  // JIS Kana -> KATAKANAHIRAGANA
        0x69 => 183, // F13
        0x6A => 186, // F16
        0x6B => 184, // F14
        0x6D => 68,  // F10
        0x6E => 127, // Context menu -> COMPOSE
        0x6F => 88,  // F12
        0x71 => 185, // F15
        0x72 => 110, // Help -> INSERT (where PC keyboards have it)
        0x73 => 102, // Home
        0x74 => 104, // Page Up
        0x75 => 111, // Forward Delete
        0x76 => 62,  // F4
        0x77 => 107, // End
        0x78 => 60,  // F2
        0x79 => 109, // Page Down
        0x7A => 59,  // F1
        0x7B => 105, // Left
        0x7C => 106, // Right
        0x7D => 108, // Down
        0x7E => 103, // Up
        _ => return None,
    })
}

/// evdev to macOS virtual key code: the inverse of `mac_to_evdev`, for injecting on a Mac.
/// Where two Mac keys share an evdev code (Return / keypad Enter), the main one wins.
pub fn evdev_to_mac(code: u16) -> Option<u16> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<std::collections::HashMap<u16, u16>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = std::collections::HashMap::new();
        for vk in (0..0x80u16).rev() {
            if let Some(ev) = mac_to_evdev(vk) {
                t.insert(ev, vk);
            }
        }
        t.insert(96, 0x4C); // keypad Enter
        t.insert(110, 0x72); // Insert → Help, where Mac keyboards have it
        t
    });
    table.get(&code).copied()
}

/// evdev code for a key as Hyprland names it in a binding: an xkb key name (`W`, `RETURN`,
/// `comma`, `BracketLeft`; any case) or `code:N` (an xkb keycode). Positions on a US layout,
/// as everywhere else here. `None` for names with no fixed key (mouse buttons, `XF86…`).
pub fn evdev_from_name(name: &str) -> Option<u16> {
    let lower = name.trim().to_ascii_lowercase();
    if let Some(code) = lower.strip_prefix("code:") {
        return code.parse::<u16>().ok()?.checked_sub(8).filter(|&c| c > 0);
    }
    if let [c] = lower.as_bytes() {
        const LETTERS: [u16; 26] = [
            30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47,
            17, 45, 21, 44,
        ];
        return match c {
            b'a'..=b'z' => Some(LETTERS[(c - b'a') as usize]),
            b'1'..=b'9' => Some((c - b'1') as u16 + 2),
            b'0' => Some(11),
            _ => None,
        };
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return match n {
            1..=10 => Some(58 + n),
            11 | 12 => Some(76 + n),
            13..=24 => Some(170 + n),
            _ => None,
        };
    }
    Some(match lower.as_str() {
        "escape" | "esc" => ev::ESC,
        "minus" => 12,
        "equal" => 13,
        "backspace" => ev::BACKSPACE,
        "tab" => ev::TAB,
        "bracketleft" => 26,
        "bracketright" => 27,
        "return" | "enter" => ev::ENTER,
        "semicolon" => 39,
        "apostrophe" => 40,
        "grave" => 41,
        "backslash" => 43,
        "comma" => 51,
        "period" => 52,
        "slash" => 53,
        "space" => ev::SPACE,
        "print" => 99,
        "home" => 102,
        "up" => ev::UP,
        "prior" | "page_up" => 104,
        "left" => ev::LEFT,
        "right" => ev::RIGHT,
        "end" => 107,
        "down" => ev::DOWN,
        "next" | "page_down" => 109,
        "insert" => 110,
        "delete" => 111,
        _ => return None,
    })
}

/// Modifiers other than Command, as bits in the same places as Hyprland's `modmask`.
pub mod mods {
    pub const SHIFT: u8 = 1;
    pub const CTRL: u8 = 4;
    pub const ALT: u8 = 8;
}

/// The modifier bit for a Shift, Ctrl or Alt key; 0 for anything else.
fn mod_bit(code: u16) -> u8 {
    match code {
        ev::LEFTSHIFT | ev::RIGHTSHIFT => mods::SHIFT,
        ev::LEFTCTRL | ev::RIGHTCTRL => mods::CTRL,
        ev::LEFTALT | ev::RIGHTALT => mods::ALT,
        _ => 0,
    }
}

/// What Command (the Mac's ⌘, sent as evdev META) becomes on a non-Mac target.
///
/// Command is held back until the next key or click decides it. It becomes Super where the
/// desktop has a Super shortcut for that key with the same other modifiers held (so ⌘W is
/// Omarchy's Super+W, close window, just as with a Mac keyboard plugged in), or where a key
/// is in `super_keys`; everything else gets Ctrl (app shortcuts: ⌘A, ⌘Z, ⌘N…).
#[derive(Clone, Debug, Default)]
pub struct CommandKeys {
    /// Super with any other modifiers.
    pub super_keys: HashSet<u16>,
    /// Super with exactly these other modifiers (`mods` bits): the desktop's own shortcuts.
    pub super_combos: HashSet<(u16, u8)>,
    /// Always Ctrl, whatever the desktop binds: for people who'd rather ⌘F found than went
    /// full screen.
    pub ctrl_keys: HashSet<u16>,
}

impl CommandKeys {
    /// Apply the `command_keys` setting (key name → `"ctrl"` or `"super"`). Returns the
    /// entries it couldn't make sense of.
    pub fn apply_overrides<'a>(
        &mut self,
        overrides: impl IntoIterator<Item = (&'a String, &'a String)>,
    ) -> Vec<String> {
        let mut bad = vec![];
        for (name, to) in overrides {
            match (evdev_from_name(name), to.to_ascii_lowercase().as_str()) {
                (Some(code), "ctrl") => {
                    self.super_keys.remove(&code);
                    self.ctrl_keys.insert(code);
                }
                (Some(code), "super") => {
                    self.ctrl_keys.remove(&code);
                    self.super_keys.insert(code);
                }
                _ => bad.push(format!("{name} = {to:?}")),
            }
        }
        bad
    }

    fn wants_super(&self, code: u16, mods: u8) -> bool {
        !self.ctrl_keys.contains(&code)
            && (self.super_keys.contains(&code) || self.super_combos.contains(&(code, mods)))
    }
}

#[derive(Clone, Debug)]
pub struct CommandRemap {
    pub keys: CommandKeys,
    /// Command + click: Super (window move/resize in tiling WMs) when true, else Ctrl.
    pub super_for_buttons: bool,
    held: HashSet<u16>,
    /// Shift, Ctrl and Alt keys held, whose `mods` bits pick the desktop shortcut.
    mods_held: HashSet<u16>,
    emitted: Option<u16>,
}

impl CommandRemap {
    pub fn new(keys: CommandKeys) -> Self {
        Self {
            keys,
            super_for_buttons: true,
            held: HashSet::new(),
            mods_held: HashSet::new(),
            emitted: None,
        }
    }

    /// Desktop-level shortcuts that stay on Super by default, wherever MouseTail runs.
    pub fn default_super_keys() -> HashSet<u16> {
        let mut keys: HashSet<u16> = [ev::TAB, ev::SPACE, ev::ENTER, ev::ESC]
            .into_iter()
            .chain(2..=11) // 1..0
            .chain([ev::UP, ev::DOWN, ev::LEFT, ev::RIGHT])
            .collect();
        keys.shrink_to_fit();
        keys
    }

    /// Translate one key or button event into zero or more events to inject.
    pub fn map(&mut self, code: u16, down: bool) -> Vec<(u16, bool)> {
        if ev::is_meta(code) {
            if down {
                self.held.insert(code);
                return vec![];
            }
            self.held.remove(&code);
            if self.held.is_empty() {
                return self
                    .emitted
                    .take()
                    .map(|m| vec![(m, false)])
                    .unwrap_or_default();
            }
            return vec![];
        }
        let bit = mod_bit(code);
        if bit != 0 {
            if down {
                self.mods_held.insert(code);
            } else {
                self.mods_held.remove(&code);
            }
        }
        // Shift, Ctrl and Alt don't decide Command: the key pressed with them does.
        if self.held.is_empty() || !down || bit != 0 {
            return vec![(code, down)];
        }
        let wants_super = if ev::is_button(code) {
            self.super_for_buttons
        } else {
            let mods = self.mods_held.iter().fold(0, |m, &c| m | mod_bit(c));
            self.keys.wants_super(code, mods)
        };
        let want = if wants_super {
            ev::LEFTMETA
        } else {
            ev::LEFTCTRL
        };
        let mut out = Vec::with_capacity(3);
        if self.emitted != Some(want) {
            if let Some(prev) = self.emitted.replace(want) {
                out.push((prev, false));
            }
            out.push((want, true));
        }
        out.push((code, true));
        out
    }

    /// Forget held state (on leave/disconnect). Returns releases still owed.
    pub fn reset(&mut self) -> Vec<(u16, bool)> {
        self.held.clear();
        self.mods_held.clear();
        self.emitted
            .take()
            .map(|m| vec![(m, false)])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::ev::*;
    use super::*;

    const W: u16 = 17;
    const T: u16 = 20;
    const F: u16 = 33;

    fn remap() -> CommandRemap {
        let mut super_keys = CommandRemap::default_super_keys();
        super_keys.extend([C, V, X]);
        CommandRemap::new(CommandKeys {
            super_keys,
            ..Default::default()
        })
    }

    /// Like Omarchy: Super+W closes, Super+Shift+F opens files, Super+F is full screen.
    fn omarchy() -> CommandRemap {
        let mut r = remap();
        r.keys.super_combos = [(W, 0), (F, 0), (F, mods::SHIFT)].into();
        r
    }

    #[test]
    fn command_follows_desktop_shortcuts() {
        let mut r = omarchy();
        r.map(LEFTMETA, true);
        assert_eq!(r.map(W, true), vec![(LEFTMETA, true), (W, true)]);
        r.map(W, false);
        assert_eq!(r.map(LEFTMETA, false), vec![(LEFTMETA, false)]);

        // Super+T isn't bound here, so ⌘T is still a new tab.
        r.map(LEFTMETA, true);
        assert_eq!(r.map(T, true), vec![(LEFTCTRL, true), (T, true)]);
    }

    #[test]
    fn desktop_shortcuts_match_other_modifiers() {
        let mut r = omarchy();
        r.map(LEFTMETA, true);
        assert_eq!(r.map(LEFTSHIFT, true), vec![(LEFTSHIFT, true)]);
        assert_eq!(r.map(F, true), vec![(LEFTMETA, true), (F, true)]);
        r.map(F, false);
        r.map(LEFTSHIFT, false);
        r.map(LEFTMETA, false);

        // Nothing is bound to Super+Shift+W, so ⌘⇧W goes to the app.
        r.map(LEFTMETA, true);
        r.map(LEFTSHIFT, true);
        assert_eq!(r.map(W, true), vec![(LEFTCTRL, true), (W, true)]);
    }

    #[test]
    fn ctrl_override_beats_desktop() {
        let mut r = omarchy();
        r.keys.ctrl_keys.insert(F);
        r.map(LEFTMETA, true);
        assert_eq!(r.map(F, true), vec![(LEFTCTRL, true), (F, true)]);
    }

    #[test]
    fn overrides_from_settings() {
        let mut keys = CommandKeys::default();
        keys.super_combos.insert((F, 0));
        let set = |k: &str, v: &str| (k.to_string(), v.to_string());
        let overrides = [
            set("f", "ctrl"),
            set("T", "Super"),
            set("nope", "ctrl"),
            set("W", "alt"),
        ];
        let bad = keys.apply_overrides(overrides.iter().map(|(k, v)| (k, v)));
        assert_eq!(bad, ["nope = \"ctrl\"", "W = \"alt\""]);
        assert!(!keys.wants_super(F, 0));
        assert!(keys.wants_super(T, mods::SHIFT));
    }

    #[test]
    fn names_hyprland_keys() {
        assert_eq!(evdev_from_name("W"), Some(W));
        assert_eq!(evdev_from_name("q"), Some(16));
        assert_eq!(evdev_from_name("RETURN"), Some(ENTER));
        assert_eq!(evdev_from_name("comma"), Some(51));
        assert_eq!(evdev_from_name("code:10"), Some(2)); // 1
        assert_eq!(evdev_from_name("0"), Some(11));
        assert_eq!(evdev_from_name("F12"), Some(88));
        assert_eq!(evdev_from_name("Delete"), Some(111));
        assert_eq!(evdev_from_name("mouse:272"), None);
        assert_eq!(evdev_from_name("XF86AudioMute"), None);
    }

    #[test]
    fn command_t_becomes_ctrl_t() {
        let mut r = remap();
        assert_eq!(r.map(LEFTMETA, true), vec![]);
        assert_eq!(r.map(T, true), vec![(LEFTCTRL, true), (T, true)]);
        assert_eq!(r.map(T, false), vec![(T, false)]);
        assert_eq!(r.map(LEFTMETA, false), vec![(LEFTCTRL, false)]);
    }

    #[test]
    fn command_c_uses_super_when_listed() {
        let mut r = remap();
        r.map(LEFTMETA, true);
        assert_eq!(r.map(C, true), vec![(LEFTMETA, true), (C, true)]);
    }

    #[test]
    fn switches_modifier_mid_hold() {
        let mut r = remap();
        r.map(LEFTMETA, true);
        r.map(T, true);
        r.map(T, false);
        assert_eq!(
            r.map(TAB, true),
            vec![(LEFTCTRL, false), (LEFTMETA, true), (TAB, true)]
        );
        assert_eq!(r.map(LEFTMETA, false), vec![(LEFTMETA, false)]);
    }

    #[test]
    fn lone_command_emits_nothing() {
        let mut r = remap();
        assert!(r.map(LEFTMETA, true).is_empty());
        assert!(r.map(LEFTMETA, false).is_empty());
    }

    #[test]
    fn command_click_is_super() {
        let mut r = remap();
        r.map(RIGHTMETA, true);
        assert_eq!(
            r.map(BTN_LEFT, true),
            vec![(LEFTMETA, true), (BTN_LEFT, true)]
        );
    }

    #[test]
    fn plain_keys_pass_through() {
        let mut r = remap();
        assert_eq!(r.map(30, true), vec![(30, true)]);
        assert_eq!(r.map(30, false), vec![(30, false)]);
    }

    #[test]
    fn evdev_maps_back_to_mac() {
        for vk in 0..0x80u16 {
            if let Some(ev) = mac_to_evdev(vk)
                && vk != 0x34
            {
                assert_eq!(evdev_to_mac(ev), Some(vk), "vk {vk:#x} (evdev {ev})");
            }
        }
        assert_eq!(evdev_to_mac(LEFTMETA), Some(0x37)); // Super → Command
        assert_eq!(evdev_to_mac(LEFTALT), Some(0x3A)); // Alt → Option
    }

    #[test]
    fn mac_letters_map() {
        assert_eq!(mac_to_evdev(0x00), Some(30)); // A
        assert_eq!(mac_to_evdev(0x37), Some(LEFTMETA));
        assert_eq!(mac_to_evdev(0x3F), None); // Fn
    }
}
