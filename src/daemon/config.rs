// examples/daemon/config.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub reticulum: ReticulumConfig,
    #[serde(default)]
    pub iroh: IrohConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub interfaces: HashMap<String, InterfaceConfig>,
}

// ---------------------------------------------------------------------------
// Reticulum config 
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReticulumConfig {
    #[serde(default)]
    pub enable_transport: bool,
    #[serde(default = "default_true")]
    pub share_instance: bool,
    #[serde(default = "default_shared_port")]
    pub shared_instance_port: u16,
    #[serde(default = "default_control_port")]
    pub instance_control_port: u16,
    #[serde(default)]
    pub panic_on_interface_error: bool,
}

// ---------------------------------------------------------------------------
// Iroh config
// ---------------------------------------------------------------------------
//
// Iroh is a QUIC-based peer-to-peer transport. It provides direct encrypted
// connections with NAT traversal, complementing Reticulum's mesh routing.
//
// Fields:
//   enabled    — set false to run Reticulum-only without Iroh overhead
//   bind_port  — UDP port Iroh listens on (0 = OS assigns a free port)
//   relay_url  — DERP relay URL for NAT traversal. Empty = Iroh's public
//                defaults. Set a self-hosted URL for air-gapped networks.

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IrohConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 0 lets the OS pick — avoids hardcoded port conflicts
    #[serde(default)]
    pub bind_port: u16,
    /// Empty string means use Iroh's built-in default relay nodes
    #[serde(default)]
    pub relay_url: String,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_port: 0,
            relay_url: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Logging config — unchanged from original
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_loglevel")]
    pub loglevel: u8,
}

// ---------------------------------------------------------------------------
// Interface config — unchanged from original
// ---------------------------------------------------------------------------

#[derive(Debug, Clone,  Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum InterfaceConfig {
    TCPServerInterface {
        #[serde(default = "default_true")]
        interface_enabled: bool,
        bind_host: String,
        bind_port: u16,
    },
    TCPClientInterface {
        #[serde(default = "default_true")]
        interface_enabled: bool,
        target_host: String,
        target_port: u16,
    },
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_true() -> bool { true }
fn default_shared_port() -> u16 { 37428 }
fn default_control_port() -> u16 { 37429 }
fn default_loglevel() -> u8 { 4 }

impl Default for ReticulumConfig {
    fn default() -> Self {
        Self {
            enable_transport: false,
            share_instance: true,
            shared_instance_port: 37428,
            instance_control_port: 37429,
            panic_on_interface_error: false,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { loglevel: 4 }
    }
}

// ---------------------------------------------------------------------------
// Config loading — unchanged from original
// ---------------------------------------------------------------------------

impl Config {
    pub fn search_paths() -> Vec<PathBuf> {
        let mut paths = vec![PathBuf::from("/etc/aerowan")];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/aerowan"));
            paths.push(home.join(".aerowan"));
        }
        paths
    }

    pub fn find_existing() -> Option<PathBuf> {
        Self::search_paths()
            .into_iter()
            .find(|p| p.join("config.toml").exists())
    }

    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("home directory")
            .join(".config/aerowan")
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let config_file = path.join("config.toml");
        let content = std::fs::read_to_string(config_file)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn load() -> Result<(Self, PathBuf), Box<dyn std::error::Error>> {
        if let Some(existing) = Self::find_existing() {
            let config = Self::from_file(&existing)?;
            Ok((config, existing))
        } else {
            let default_dir = Self::default_path();
            std::fs::create_dir_all(&default_dir)?;
            let config = Self::default_config();
            let config_file = default_dir.join("config.toml");
            std::fs::write(&config_file, toml::to_string_pretty(&config)?)?;
            Ok((config, default_dir))
        }
    }

    fn default_config() -> Self {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "Default TCP Server".to_string(),
            InterfaceConfig::TCPServerInterface {
                interface_enabled: false,
                bind_host: "[::]".to_string(),
                bind_port: 4242,
            },
        );
        Self {
            reticulum: ReticulumConfig::default(),
            iroh: IrohConfig::default(),
            logging: LoggingConfig::default(),
            interfaces,
        }
    }

    pub fn log_filter(&self) -> &'static str {
        match self.logging.loglevel {
            0 | 1 => "error",
            2     => "warn",
            3 | 4 => "info",
            5 | 6 => "debug",
            _     => "trace",
        }
    }
}
