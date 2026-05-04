use serde::Deserialize;
use std::path::{Path, PathBuf};

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
    /// Automatically refresh the notification list every N seconds (0 = disabled)
    #[serde(default = "default_refresh_time")]
    pub refresh_time: u64,
    /// When true, show literal escape sequences (e.g. `\n`) in the notification body.
    /// When false (default), convert them to real characters for display.
    #[serde(default = "default_escape_body")]
    pub escape_body: bool,
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
fn default_refresh_time() -> u64 {
    5
}
fn default_escape_body() -> bool {
    false
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_app: default_show_app(),
            body_lines: default_body_lines(),
            show_hints: default_show_hints(),
            refresh_time: default_refresh_time(),
            escape_body: default_escape_body(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
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
    /// If the file does not exist, write a commented default config first.
    /// Falls back to defaults if the file is absent or malformed.
    pub fn load() -> Self {
        let path = Self::config_path();
        if let Some(p) = path {
            if !p.exists() {
                Self::write_defaults(&p);
            }
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(cfg) = toml::from_str::<Config>(&content) {
                    return cfg;
                }
            }
        }
        Config::default()
    }

    /// Write a commented default config file to `path`, creating parent directories as needed.
    /// Silently ignores any I/O error.
    fn write_defaults(path: &Path) {
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return;
            }
        }
        let _ = std::fs::write(path, Self::default_toml_content());
    }

    /// Returns a hand-crafted, fully-commented TOML string with all default values.
    fn default_toml_content() -> &'static str {
        r##"[display]
# Show the application name for each notification.
show_app = true

# Number of body lines shown per notification (0-4).
body_lines = 3

# Show the keybind hint bar at the bottom (can be toggled per-session with F1).
show_hints = true

# Automatically refresh the notification list every N seconds (0 = disabled).
refresh_time = 5

# When true, show literal escape sequences (e.g. \n) in the notification body.
# When false (default), convert them to real characters for display.
escape_body = false

[persistence]
# Persist notification history across reboots.
enabled = false

# Maximum number of notifications to keep in history (0 = unlimited).
max_history = 0

[colors]
# Hex color strings for the TUI.
foreground = "#cdd6f4"
background = "#1e1e2e"
accent     = "#89b4fa"
highlight  = "#313244"
matching   = "#a6e3a1"
"##
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
