use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub hosting: HostingConfig,
    pub analytics: AnalyticsConfig,
    pub admin: AdminConfig,
    pub auth: AuthConfig,
    pub comments: CommentsConfig,
    pub zone: ZoneConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            hosting: HostingConfig::default(),
            analytics: AnalyticsConfig::default(),
            admin: AdminConfig::default(),
            auth: AuthConfig::default(),
            comments: CommentsConfig::default(),
            zone: ZoneConfig::default(),
        }
    }
}

/// Zone (network-reach) configuration. Only consumed when the `zone` feature
/// is compiled into `pipa-server`; the struct is always parsed so a single
/// `pages.toml` works against both feature builds. `default` is the zone new
/// deploys land in when the caller omits `--zone` ("public" | "private",
/// fallback "private" = secure by default). A request counts as the internal
/// (LAN) zone when its proxy peer IP is in `internal_proxy_ips` AND its `Host`
/// matches `internal_hosts` (supports a leading `*.` wildcard); otherwise it
/// is treated as the external (internet) zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoneConfig {
    pub default: String,
    pub internal_proxy_ips: Vec<String>,
    pub internal_hosts: Vec<String>,
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self {
            default: "private".to_string(),
            internal_proxy_ips: Vec::new(),
            internal_hosts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub addr: String,
    pub public_url: String,
    pub data_dir: PathBuf,
    pub pages_dir: PathBuf,
    pub trusted_proxy: String,
    pub dev: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8080".to_string(),
            public_url: "http://127.0.0.1:8080".to_string(),
            data_dir: PathBuf::from("./data"),
            pages_dir: PathBuf::from("./pages"),
            trusted_proxy: "127.0.0.1".to_string(),
            dev: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostingConfig {
    pub max_upload_bytes: u64,
    pub default_mode: String,
}

impl Default for HostingConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 100 * 1024 * 1024,
            default_mode: "spa".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticsConfig {
    pub ip_salt_rotation: String,
    pub retention_days: u32,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            ip_salt_rotation: "daily".to_string(),
            retention_days: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdminConfig {
    pub ui_enabled: bool,
    pub ui_path: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            ui_enabled: true,
            ui_path: "/admin".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub refresh_ttl_days: i64,
    pub access_ttl_seconds: i64,
    pub step_up_ttl_seconds: i64,
    pub setup_code_ttl_minutes: i64,
    pub notifications: AuthNotificationsConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            refresh_ttl_days: 90,
            access_ttl_seconds: 300,
            step_up_ttl_seconds: 300,
            setup_code_ttl_minutes: 15,
            notifications: AuthNotificationsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthNotificationsConfig {
    pub webhook_on_stepup: Option<String>,
    pub webhook_on_device_register: Option<String>,
    pub email_on_device_register: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentsConfig {
    pub enabled: bool,
    pub max_author_length: usize,
    pub max_body_length: usize,
    pub default_require_approval: bool,
    pub allowed_origins: String,
}

impl Default for CommentsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_author_length: 64,
            max_body_length: 2000,
            default_require_approval: false,
            allowed_origins: "same-origin".to_string(),
        }
    }
}

impl Config {
    /// Apply dev-mode overrides and reject non-loopback bind addresses. We
    /// refuse to bind to a public interface in `--dev` because dev mode
    /// loosens cookie flags + skips HSTS expectations.
    pub fn dev_mode_overlay(&mut self, dev: bool) -> Result<()> {
        if !dev {
            return Ok(());
        }
        self.server.dev = true;

        let parsed: SocketAddr = self
            .server
            .addr
            .parse()
            .with_context(|| format!("parsing server.addr {:?}", self.server.addr))?;
        if !parsed.ip().is_loopback() {
            return Err(anyhow!(
                "--dev requires a loopback bind address; got {}",
                parsed.ip()
            ));
        }
        Ok(())
    }
}

pub fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        tracing::warn!(
            path = %path.display(),
            "config file not found - using defaults"
        );
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let cfg: Config = toml::from_str(&raw)
        .with_context(|| format!("parsing config from {}", path.display()))?;
    Ok(cfg)
}
