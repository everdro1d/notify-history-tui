use serde::Deserialize;
use std::path::PathBuf;

// ── Sub-configs ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_show_app")]
    pub show_app: bool,
    /// Number of body lines shown per notification (0–4)
    #[serde(default = "default_body_lines")]
    pub body_lines: u8,
    /// Whether to show the keybind hint bar at the bottom (can be toggled per-session with F1)
    #[serde(default = "default_show_hints")]
    pub show_hints: bool,
}

fn default_show_app() -> bool {
    true
}
fn default_body_lines() -> u8 {
    3
}
fn default_show_hints() -> bool {
    true
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_app: default_show_app(),
            body_lines: default_body_lines(),
            show_hints: default_show_hints(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PersistenceConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Maximum number of notifications to keep in history (0 = unlimited)
    #[serde(default)]
    pub max_history: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColorConfig {
    #[serde(default = "default_fg")]
    pub foreground: String,
    #[serde(default = "default_bg")]
    pub background: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    /// Background colour for the selected (cursor) item
    #[serde(default = "default_highlight")]
    pub highlight: String,
    /// Foreground colour for fuzzy-matched characters
    #[serde(default = "default_matching")]
    pub matching: String,
}

fn default_fg() -> String { "#cdd6f4".into() }
fn default_bg() -> String { "#1e1e2e".into() }
fn default_accent() -> String { "#89b4fa".into() }
fn default_highlight() -> String { "#313244".into() }
fn default_matching() -> String { "#a6e3a1".into() }

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            foreground: default_fg(),
            background: default_bg(),
            accent: default_accent(),
            highlight: default_highlight(),
            matching: default_matching(),
        }
    }
}

// ── Root config ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub persistence: PersistenceConfig,
    #[serde(default)]
    pub colors: ColorConfig,
}

impl Config {
    /// Load config from `~/.config/notify-history/config.toml`.
    /// Falls back to defaults if the file is absent or malformed.
    pub fn load() -> Self {
        let path = Self::config_path();
        if let Some(p) = path {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(cfg) = toml::from_str::<Config>(&content) {
                    return cfg;
                }
            }
        }
        Config::default()
    }

    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("notify-history").join("config.toml"))
    }

    pub fn history_file(&self) -> PathBuf {
        if self.persistence.enabled {
            let state = dirs::state_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")));
            if let Some(state_dir) = state {
                return state_dir.join("notify-history").join("notification-history");
            }
        }
        PathBuf::from("/tmp/notification-history")
    }
}
