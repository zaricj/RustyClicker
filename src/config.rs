//! Settings, named profiles and persistence (JSON in the OS config dir).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum ClickType {
    #[default]
    Single,
    Double,
}

/// One clicker configuration (what a profile stores).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hours: u32,
    pub mins: u32,
    pub secs: u32,
    pub millis: u32,
    pub button: MouseButton,
    pub click_type: ClickType,
    pub repeat_until_stopped: bool,
    pub repeat_count: u32,
    pub use_fixed_pos: bool,
    pub x: i32,
    pub y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hours: 0,
            mins: 0,
            secs: 0,
            millis: 100,
            button: MouseButton::Left,
            click_type: ClickType::Single,
            repeat_until_stopped: true,
            repeat_count: 1,
            use_fixed_pos: false,
            x: 0,
            y: 0,
        }
    }
}

impl Settings {
    /// Interval between clicks, clamped to >= 1 ms so the worker never busy-spins.
    pub fn interval(&self) -> Duration {
        let ms = self.hours as u64 * 3_600_000
            + self.mins as u64 * 60_000
            + self.secs as u64 * 1_000
            + self.millis as u64;
        Duration::from_millis(ms.max(1))
    }
}

/// Global start/stop hotkey: F1..F12 plus optional modifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkey {
    pub fkey: u8,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self { fkey: 6, ctrl: false, shift: false, alt: false }
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        write!(f, "F{}", self.fkey)
    }
}

pub const DEFAULT_PROFILE: &str = "Default";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub last_profile: String,
    pub hotkey: Hotkey,
    pub profiles: BTreeMap<String, Settings>,
    /// Top-left of the window (logical points) when the app was last closed.
    pub window_pos: Option<[f32; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(DEFAULT_PROFILE.to_owned(), Settings::default());
        Self {
            last_profile: DEFAULT_PROFILE.to_owned(),
            hotkey: Hotkey::default(),
            profiles,
            window_pos: None,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rustyclicker").join("config.json"))
    }

    /// Loads the config; falls back to defaults if missing or corrupt.
    pub fn load() -> Self {
        let mut cfg: Config = Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if cfg.profiles.is_empty() {
            cfg.profiles.insert(DEFAULT_PROFILE.to_owned(), Settings::default());
        }
        if !cfg.profiles.contains_key(&cfg.last_profile) {
            cfg.last_profile = cfg.profiles.keys().next().cloned().unwrap_or_default();
        }
        cfg
    }

    /// Atomic-ish write: temp file + rename, so a crash never leaves a half-written config.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory on this system")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }
}
