// config.rs
// Handles all persistent user configuration stored in config.toml.
// The AI never sees this file. It is only read by antflow itself at startup
// and written when the user changes a setting.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Storage root
// ---------------------------------------------------------------------------

// StorageLocation decides where antflow keeps ALL its files:
// config.toml, antflow.db (SQLite), screenshots, etc.
// "Portable" means everything lives next to the binary (USB-friendly).
// "Global" means the OS standard user-config directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StorageLocation {
    Portable,  // next to the binary
    Global,    // ~/.config/antflow  (or %APPDATA%\antflow on Windows)
}

impl Default for StorageLocation {
    fn default() -> Self {
        // Default to portable so a fresh run on a USB works immediately.
        StorageLocation::Portable
    }
}

// ---------------------------------------------------------------------------
// AI provider config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    // Which provider is currently active: "ollama", "openai", "anthropic", etc.
    pub provider: String,

    // Base URL of the provider's API endpoint.
    // For Ollama this is typically "http://localhost:11434".
    pub base_url: String,

    // API key. Empty string for local Ollama.
    // Stored in plain TOML; sensitive but kept local to the device.
    pub api_key: String,

    // The model name the user selected, e.g. "llava", "gpt-4o", "gemini-1.5-pro".
    pub model: String,

    // Whether the selected model supports vision (image input).
    // antflow checks this before sending screenshots.
    pub vision_capable: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        AiProviderConfig {
            provider: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            vision_capable: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Session memory config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    // Whether to persist conversation history between launches.
    pub remember_sessions: bool,

    // Maximum number of past messages to include in each AI request.
    // Older messages are dropped to stay within context limits.
    pub max_history_messages: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            remember_sessions: true,
            max_history_messages: 40,
        }
    }
}

// ---------------------------------------------------------------------------
// Scheduler config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    // Whether the background scheduler daemon is enabled.
    pub enabled: bool,

    // How often (seconds) the scheduler wakes to check for due tasks.
    pub poll_interval_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            enabled: false,
            poll_interval_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level config struct
// ---------------------------------------------------------------------------

// AppConfig is the single source of truth for all user settings.
// It maps 1-to-1 with the TOML file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Where to store all antflow data files.
    pub storage: StorageLocation,

    // Current AI provider settings.
    pub ai: AiProviderConfig,

    // Session / memory settings.
    pub session: SessionConfig,

    // Background scheduler settings.
    pub scheduler: SchedulerConfig,

    // Whether first-run setup has been completed.
    // When false the wizard runs automatically on next launch.
    pub setup_complete: bool,

    // Whether to keep running in the background when the main window is closed.
    #[serde(default)]
    pub run_in_background: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            storage: StorageLocation::default(),
            ai: AiProviderConfig::default(),
            session: SessionConfig::default(),
            scheduler: SchedulerConfig::default(),
            setup_complete: false,
            run_in_background: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

// Returns the directory where antflow stores its data files,
// based on the StorageLocation setting.
//
// `exe_dir` is the folder that contains the antflow binary.
// This is passed in rather than looked up here to keep this function pure.
pub fn resolve_storage_dir(location: &StorageLocation, exe_dir: &Path) -> PathBuf {
    match location {
        StorageLocation::Portable => {
            // Portable mode: store everything next to the binary.
            exe_dir.to_path_buf()
        }
        StorageLocation::Global => {
            // Global mode: use the OS config directory.
            // dirs::config_dir() returns:
            //   Linux/macOS : ~/.config
            //   Windows     : C:\Users\<user>\AppData\Roaming
            dirs::config_dir()
                .unwrap_or_else(|| exe_dir.to_path_buf())
                .join("antflow")
        }
    }
}

// Returns the path of the config file given the storage directory.
pub fn config_path(storage_dir: &Path) -> PathBuf {
    storage_dir.join("config.toml")
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

// Reads the config file from disk.
// If the file does not exist, returns the default config (triggers wizard).
pub fn load(storage_dir: &Path) -> Result<AppConfig> {
    let path = config_path(storage_dir);
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    // std::fs::read_to_string reads the entire file into a String.
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    // toml::from_str parses the TOML text into our AppConfig struct.
    let mut config: AppConfig = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    // Normalize localhost to 127.0.0.1 to prevent Windows IPv6 connection issues
    if config.ai.base_url == "http://localhost:11434" {
        config.ai.base_url = "http://127.0.0.1:11434".to_string();
        let _ = save(storage_dir, &config);
    }

    Ok(config)
}

// Writes the config struct to disk as TOML.
// Creates the directory if it does not exist yet.
pub fn save(storage_dir: &Path, config: &AppConfig) -> Result<()> {
    // std::fs::create_dir_all creates the directory and all parents silently
    // if they already exist.
    std::fs::create_dir_all(storage_dir)
        .with_context(|| format!("Failed to create storage directory: {}", storage_dir.display()))?;

    let path = config_path(storage_dir);

    // toml::to_string_pretty serializes the struct to a human-readable TOML string.
    let content = toml::to_string_pretty(config)
        .context("Failed to serialize config to TOML")?;

    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    Ok(())
}
