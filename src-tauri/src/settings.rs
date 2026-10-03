// Preferences in %APPDATA%\Dos Live\settings.json. Every field has a default, so
// a hand-edited or older file always loads. No secret ever lands here — keys
// live in the Windows Credential Manager (secrets.rs).

use std::path::PathBuf;

use dos_core::{default_worlds, BoardLayout, WorldDef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Boards {
    /// Trello short links (the part after /b/ in the board URL).
    pub dos: String,
    pub don: String,
    pub boom: String,
}

impl Default for Boards {
    fn default() -> Self {
        Boards { dos: "vKKgCDGD".into(), don: "5jR6zssk".into(), boom: "vbw8udrd".into() }
    }
}

impl Boards {
    pub fn keyed(&self) -> Vec<(String, String)> {
        [("dos", &self.dos), ("don", &self.don), ("boom", &self.boom)]
            .into_iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (k.to_string(), v.trim().to_string()))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceSettings {
    /// "wake": always listening for "Dos …" · "hotkey": only after the hotkey · "off".
    pub mode: String,
    pub wake_word: String,
    /// Part of a Windows voice name; empty = best male English voice.
    pub voice_name: String,
    pub rate: f64,
    pub speak_pings: bool,
    pub speak_asks: bool,
    /// Push-to-talk, in Tauri's shortcut syntax.
    pub hotkey: String,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings {
            mode: "wake".into(),
            wake_word: "dos".into(),
            voice_name: String::new(),
            rate: 1.0,
            speak_pings: true,
            speak_asks: true,
            hotkey: "Ctrl+Alt+D".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HermesSettings {
    pub enabled: bool,
    /// Hermes' OpenAI-compatible API server (API_SERVER_ENABLED=true).
    pub url: String,
    pub model: String,
}

impl Default for HermesSettings {
    fn default() -> Self {
        HermesSettings { enabled: false, url: "http://127.0.0.1:8642/v1".into(), model: "hermes-agent".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    /// "primary" or "cursor" (whichever display the mouse is on).
    pub screen: String,
    pub sound: bool,
    pub volume: f64,
    /// Invisible to screen sharing and recording (still visible to you).
    pub stealth: bool,
    /// Work pill shows counts only, never ticket titles.
    pub discreet_work: bool,
    pub autostart: bool,
    /// Drop the panel open when a ping or ask arrives.
    pub expand_on_alert: bool,
    pub collapse_after_secs: f64,
    /// Slide the idle bar up to a thin line (or fade it, when floating).
    pub auto_hide: bool,
    pub hide_after_secs: f64,
    /// Global hotkey that hides the bar until something needs you.
    pub hide_hotkey: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            screen: "primary".into(),
            sound: true,
            volume: 0.5,
            stealth: true,
            discreet_work: true,
            autostart: true,
            expand_on_alert: true,
            collapse_after_secs: 14.0,
            auto_hide: true,
            hide_after_secs: 8.0,
            hide_hotkey: "Ctrl+Alt+H".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub boards: Boards,
    pub poll_secs: u64,
    pub layout: BoardLayout,
    pub worlds: Vec<WorldDef>,
    pub stale_days: i64,
    pub due_soon_days: i64,
    pub voice: VoiceSettings,
    /// Go quiet (and deaf) while another app holds the microphone.
    pub call_guard: bool,
    /// Microphone users that never count as "on a call".
    pub call_guard_ignore: Vec<String>,
    pub hermes: HermesSettings,
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            boards: Boards::default(),
            poll_secs: 60,
            layout: BoardLayout::default(),
            worlds: default_worlds(),
            stale_days: 21,
            due_soon_days: 3,
            voice: VoiceSettings::default(),
            call_guard: true,
            call_guard_ignore: vec!["SpeechRuntime".into(), "dos-live".into(), "Dos Live".into()],
            hermes: HermesSettings::default(),
            ui: UiSettings::default(),
        }
    }
}

impl Settings {
    pub fn rules(&self) -> dos_core::Rules {
        dos_core::Rules { stale_days: self.stale_days.max(1), due_soon_days: self.due_soon_days.max(0) }
    }
}

fn base(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// %APPDATA%\Dos Live
pub fn config_dir() -> PathBuf {
    base("APPDATA", ".config").join("Dos Live")
}

/// %LOCALAPPDATA%\Dos Live — the log.
pub fn local_dir() -> PathBuf {
    base("LOCALAPPDATA", ".local/share").join("Dos Live")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Where the bar was last dragged to. Kept apart from settings.json so the
/// settings window, which holds its own copy of Settings, can never overwrite it.
pub fn position_path() -> PathBuf {
    config_dir().join("position.json")
}

pub fn state_path() -> PathBuf {
    config_dir().join("state.json")
}

pub fn load() -> Settings {
    match std::fs::read(settings_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            crate::log::line(format!("settings.json unreadable ({e}) — using defaults"));
            Settings::default()
        }),
        Err(_) => {
            let s = Settings::default();
            // Write the defaults out so worlds and layout are there to edit.
            let _ = save(&s);
            s
        }
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let json = serde_json::to_vec_pretty(settings).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(settings_path(), json)
}
