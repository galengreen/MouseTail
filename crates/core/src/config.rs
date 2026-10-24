//! Persistent settings and paired peers. Everything has a sensible default; the file only
//! exists so people can dig in if they want to.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::layout::Point;
use crate::proto::DisplayInfo;

pub const DEFAULT_PORT: u16 = 24802;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Name shown to other machines. Defaults to the host name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, rename = "peer")]
    pub peers: Vec<PeerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            port: DEFAULT_PORT,
            settings: Settings::default(),
            peers: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Carry the clipboard across when the cursor crosses.
    pub clipboard: bool,
    /// Largest clipboard payload to send, in bytes.
    pub clipboard_max_bytes: usize,
    /// Share sound: the computer you're sitting at plays the other's sound. Off here means
    /// neither way.
    pub audio: bool,
    /// Install new releases automatically (Linux; the Mac app has its own setting).
    pub updates: bool,
    /// A ripple where the cursor crosses to or from another computer.
    pub ripple: bool,
    /// What ⌘ plus a key becomes when a Mac controls this computer, overriding the
    /// automatic choice: key name (as in Hyprland, `F`, `RETURN`…) to `"ctrl"` or `"super"`.
    /// By default it's Super wherever the desktop has a Super shortcut, Ctrl elsewhere.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub command_keys: BTreeMap<String, String>,
}

impl Settings {
    /// Largest clipboard payload to send or accept: the setting, but never more than fits in
    /// one message (with room for its type).
    pub fn clipboard_limit(&self) -> usize {
        self.clipboard_max_bytes
            .min(crate::proto::MAX_FRAME - 64 * 1024)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clipboard: true,
            clipboard_max_bytes: 10 * 1024 * 1024,
            audio: true,
            updates: true,
            ripple: true,
            command_keys: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerConfig {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    /// Offset of the peer's displays in this machine's layout. Unset = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Point>,
    /// When the placement was last chosen by a person (Unix ms; 0 = automatic). The newest
    /// arrangement wins when two computers compare theirs.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub placement_updated: u64,
    /// Last known displays, so the arrangement can show the machine while it's offline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub displays: Vec<DisplayInfo>,
    /// Hardware addresses for waking it (Wake-on-LAN).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wake_macs: Vec<String>,
    /// This machine plays the peer's sound (true) or sends it ours (false), as decided by
    /// whichever was last used to push the cursor onto the other. Unset = the default
    /// (`audio::listens_by_default`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen: Option<bool>,
    /// When `listen` was decided (Unix ms).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub listen_updated: u64,
    /// Paused: still paired, but nothing crosses (cursor, keyboard, clipboard, sound) until
    /// it's resumed, from either computer.
    #[serde(default, skip_serializing_if = "is_false")]
    pub paused: bool,
    /// When it was last paused or resumed (Unix ms; 0 = never). The newest wins when two
    /// computers compare.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub paused_updated: u64,
    /// Where it was last reached, newest first. Tried whenever it can't be found on the
    /// network, which some networks (and firewalls) get in the way of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addrs: Vec<SocketAddr>,
}

/// How many of a peer's addresses to remember.
pub const REMEMBERED_ADDRS: usize = 4;

impl PeerConfig {
    /// Note that `addr` just worked. Returns whether anything changed.
    pub fn reached_at(&mut self, addr: SocketAddr) -> bool {
        if self.addrs.first() == Some(&addr) {
            return false;
        }
        self.addrs.retain(|a| *a != addr);
        self.addrs.insert(0, addr);
        self.addrs.truncate(REMEMBERED_ADDRS);
        true
    }
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Config {
    /// Load the file (defaults if there isn't one). A file that can't be read as a whole
    /// doesn't stop MouseTail: keep whatever parts still make sense (each paired computer
    /// separately), set the original aside as `<name>.bad`, and say what happened.
    pub fn load_or_recover(path: &Path) -> anyhow::Result<(Self, Option<String>)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(e) => return Err(e.into()),
        };
        let error = match toml::from_str(&text) {
            Ok(config) => return Ok((config, None)),
            Err(e) => e,
        };
        let bad = path.with_extension("toml.bad");
        fs::write(&bad, &text)?;
        let config = Self::recover(&text);
        // Save the repaired version, so this only comes up once.
        config.save(path)?;
        let problem = format!(
            "{} couldn't be read ({}); kept what could be, the original is in {}",
            path.display(),
            error.message(),
            bad.display()
        );
        Ok((config, Some(problem)))
    }

    /// Everything in `text` that still parses, the rest left at defaults.
    fn recover(text: &str) -> Self {
        fn get<T: DeserializeOwned>(table: &toml::Table, key: &str) -> Option<T> {
            table.get(key).cloned()?.try_into().ok()
        }
        let Ok(table) = text.parse::<toml::Table>() else {
            return Self::default();
        };
        let peers = match table.get("peer") {
            Some(toml::Value::Array(peers)) => peers
                .iter()
                .filter_map(|p| p.clone().try_into().ok())
                .collect(),
            _ => vec![],
        };
        Self {
            name: get(&table, "name"),
            port: get(&table, "port").unwrap_or(DEFAULT_PORT),
            settings: get(&table, "settings").unwrap_or_default(),
            peers,
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Write, flush to disk, then swap in: a crash or power cut leaves the old file or the
        // new one, never half of one.
        let tmp = path.with_extension("toml.tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn peer(&self, id: &str) -> Option<&PeerConfig> {
        self.peers.iter().find(|p| p.id == id)
    }

    pub fn peer_mut(&mut self, id: &str) -> Option<&mut PeerConfig> {
        self.peers.iter_mut().find(|p| p.id == id)
    }

    pub fn is_paired(&self, fingerprint: &str) -> bool {
        self.peers.iter().any(|p| p.fingerprint == fingerprint)
    }

    pub fn add_peer(&mut self, peer: PeerConfig) {
        self.peers.retain(|p| p.id != peer.id);
        self.peers.push(peer);
    }

    /// Match a peer by id, id prefix or (case-insensitive) name. Nothing if the query is empty
    /// or fits more than one, so the wrong computer is never picked.
    pub fn find_peer(&self, query: &str) -> Option<&PeerConfig> {
        if query.is_empty() {
            return None;
        }
        if let Some(exact) = self.peer(query) {
            return Some(exact);
        }
        let mut matches = self
            .peers
            .iter()
            .filter(|p| p.id.starts_with(query) || p.name.eq_ignore_ascii_case(query));
        let first = matches.next();
        matches.next().is_none().then_some(first).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults() {
        let parsed: Config = toml::from_str("").unwrap();
        assert_eq!(parsed, Config::default());

        let mut c = Config::default();
        c.add_peer(PeerConfig {
            id: "abc".into(),
            name: "omarchy".into(),
            fingerprint: "ff".into(),
            placement: Some(Point::new(-1920.0, -49.0)),
            placement_updated: 0,
            displays: vec![],
            wake_macs: vec![],
            listen: Some(true),
            listen_updated: 7,
            paused: true,
            paused_updated: 9,
            addrs: vec!["192.168.1.20:24802".parse().unwrap()],
        });
        let text = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), c);
        assert!(c.find_peer("OMARCHY").is_some());
    }

    #[test]
    fn remembers_where_peers_were_reached() {
        let mut p: PeerConfig = toml::from_str("id = 'a'\nname = 'a'\nfingerprint = 'a'").unwrap();
        let addr = |s: &str| -> SocketAddr { s.parse().unwrap() };
        assert!(p.reached_at(addr("10.0.0.1:1")));
        assert!(!p.reached_at(addr("10.0.0.1:1")));
        for i in 2..=6 {
            assert!(p.reached_at(addr(&format!("10.0.0.{i}:1"))));
        }
        assert!(p.reached_at(addr("10.0.0.4:1")));
        assert_eq!(
            p.addrs,
            ["10.0.0.4:1", "10.0.0.6:1", "10.0.0.5:1", "10.0.0.3:1"].map(addr)
        );
    }

    #[test]
    fn find_peer_never_guesses() {
        let peer = |id: &str, name: &str| PeerConfig {
            id: id.into(),
            name: name.into(),
            fingerprint: id.into(),
            placement: None,
            placement_updated: 0,
            displays: vec![],
            wake_macs: vec![],
            listen: None,
            listen_updated: 0,
            paused: false,
            paused_updated: 0,
            addrs: vec![],
        };
        let mut c = Config::default();
        c.add_peer(peer("abc1", "iMac"));
        c.add_peer(peer("abc2", "Laptop"));
        assert!(c.find_peer("").is_none());
        assert!(c.find_peer("abc").is_none());
        assert_eq!(c.find_peer("abc2").unwrap().name, "Laptop");
        assert_eq!(c.find_peer("imac").unwrap().id, "abc1");
    }

    #[test]
    fn clipboard_limit_fits_in_a_message() {
        let mut settings = Settings::default();
        assert_eq!(settings.clipboard_limit(), settings.clipboard_max_bytes);
        settings.clipboard_max_bytes = usize::MAX;
        assert!(settings.clipboard_limit() < crate::proto::MAX_FRAME);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mousetail-config-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn broken_file_keeps_what_it_can() {
        let path = temp_path("partial.toml");
        let text = r#"
            name = "Desk"
            port = "not a number"

            [settings]
            audio = false

            [[peer]]
            id = "good"
            name = "iMac"
            fingerprint = "aa"

            [[peer]]
            id = "no-name"
            fingerprint = "bb"
        "#;
        fs::write(&path, text).unwrap();
        let (config, problem) = Config::load_or_recover(&path).unwrap();
        assert!(problem.is_some());
        assert_eq!(config.name.as_deref(), Some("Desk"));
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(!config.settings.audio);
        assert_eq!(config.peers.len(), 1);
        assert_eq!(config.peers[0].id, "good");
        assert_eq!(
            fs::read_to_string(path.with_extension("toml.bad")).unwrap(),
            text
        );
        assert_eq!(Config::load_or_recover(&path).unwrap(), (config, None));
    }

    #[test]
    fn unreadable_file_starts_fresh() {
        let path = temp_path("garbage.toml");
        fs::write(&path, "[[[ not toml").unwrap();
        let (config, problem) = Config::load_or_recover(&path).unwrap();
        assert!(problem.is_some());
        assert_eq!(config, Config::default());
    }

    #[test]
    fn good_and_missing_files_load_quietly() {
        let path = temp_path("good.toml");
        let c = Config {
            name: Some("Desk".into()),
            ..Config::default()
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load_or_recover(&path).unwrap(), (c, None));
        let missing = temp_path("missing.toml");
        assert_eq!(
            Config::load_or_recover(&missing).unwrap(),
            (Config::default(), None)
        );
    }
}
