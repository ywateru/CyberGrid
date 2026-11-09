use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub base_url: String,
    pub app_id: String,
    pub direct_timeout: Duration,
    pub direct_strategy: DirectStrategy,
    /// Enable only direct-IP iroh QUIC; no relay or address lookup service.
    pub iroh_enabled: bool,
    pub iroh_timeout: Duration,
    pub fallback_timeout: Duration,
    pub poll_interval: Duration,
    pub stun_servers: Vec<String>,
    pub interface_name: Option<String>,
    /// None disables a local provider, but dialing a remote tunnel still works.
    pub cloudflared: Option<CloudflaredConfig>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://cybergrid.aixgg.com".into(),
            app_id: String::new(),
            direct_timeout: Duration::from_secs(12),
            direct_strategy: DirectStrategy::Sequential,
            iroh_enabled: true,
            iroh_timeout: Duration::from_secs(8),
            fallback_timeout: Duration::from_secs(35),
            poll_interval: Duration::from_secs(1),
            stun_servers: vec![
                "stun:stun.l.google.com:19302".into(),
                "stun:stun.cloudflare.com:3478".into(),
            ],
            interface_name: None,
            cloudflared: None,
        }
    }
}
pub type ProviderEnvironment =
    std::sync::Arc<dyn Fn() -> Result<HashMap<String, String>> + Send + Sync>;
#[derive(Clone)]
pub struct CloudflaredConfig {
    pub(crate) source: crate::provider::ProviderSource,
    /// Re-read non-secret platform environment before each provider start.
    pub env_provider: Option<ProviderEnvironment>,
    pub binary: PathBuf,
    pub temp_dir: Option<PathBuf>,
    /// Explicit non-secret provider settings, e.g. current Android DNS.
    pub env: HashMap<String, String>,
}
impl CloudflaredConfig {
    /// Resolve the pinned macOS arm64 provider into a private cache; no PATH or network lookup.
    pub fn bundled(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache = cache_dir.into();
        let binary = crate::provider::mac_path(&cache)?;
        let mut value = Self::new(binary);
        value.source = crate::provider::ProviderSource::Mac(cache.clone());
        value.temp_dir = Some(cache);
        Ok(value)
    }
    /// Use the single APK-installed provider. Never extract executable code to Android app data.
    pub fn android_native(
        native_library_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        if !cfg!(all(target_os = "android", target_arch = "aarch64")) {
            return Err(Error::Config(
                "bundled Android provider supports only android/arm64".into(),
            ));
        }
        let directory = native_library_dir.into();
        let binary = crate::provider::android_path(&directory)?;
        let mut value = Self::new(binary);
        value.source = crate::provider::ProviderSource::Android(directory);
        value.temp_dir = Some(cache_dir.into());
        Ok(value)
    }

    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            source: crate::provider::ProviderSource::External,
            binary: binary.into(),
            env_provider: None,
            temp_dir: None,
            env: HashMap::new(),
        }
    }
}
pub(crate) fn validate_app_id(id: &str) -> Result<()> {
    if !(1..=21).contains(&id.chars().count()) {
        return Err(Error::Config(
            "explicit Logto Native application ID (1..21 characters) required".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_config(c: &mut Config) -> Result<()> {
    validate_app_id(&c.app_id)?;
    let u = Url::parse(&c.base_url).map_err(|_| Error::Config("invalid server URL".into()))?;
    let loopback = u.host_str().is_some_and(|h| {
        h == "localhost" || h.parse::<std::net::IpAddr>().is_ok_and(|a| a.is_loopback())
    });
    if u.host().is_none()
        || u[..url::Position::BeforeHost].contains('@')
        || u.query().is_some()
        || u.fragment().is_some()
        || !(u.scheme() == "https" || u.scheme() == "http" && loopback)
    {
        return Err(Error::Config(
            "server URL must be HTTPS without credentials/query/fragment (HTTP only loopback)"
                .into(),
        ));
    }
    if c.direct_timeout.is_zero()
        || c.iroh_timeout.is_zero()
        || c.fallback_timeout.is_zero()
        || c.poll_interval.is_zero()
    {
        return Err(Error::Config("timeouts must be positive".into()));
    }
    if c.stun_servers.iter().any(|s| !s.starts_with("stun:")) {
        return Err(Error::Config("only STUN permitted; TURN disabled".into()));
    }
    c.base_url = c.base_url.trim_end_matches('/').into();
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_url: Option<String>,
    pub id: String,
    pub port: u16,
    #[serde(rename = "type")]
    pub type_: String,
    pub transport: String,
}
impl Port {
    pub fn tcp(id: impl Into<String>, port: u16, type_: impl Into<String>) -> Self {
        Self {
            tunnel_url: None,
            id: id.into(),
            port,
            type_: type_.into(),
            transport: "tcp".into(),
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(url) = &self.tunnel_url {
            crate::validate_tunnel_url(url)?;
        }
        if self.id.is_empty()
            || self.id.len() > 64
            || self.port == 0
            || self.type_.trim().is_empty()
            || self.type_.len() > 128
            || self.transport != "tcp"
        {
            return Err(Error::Config("invalid explicit TCP port".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[serde(default)]
    pub iroh: Option<IrohInfo>,
    #[serde(default)]
    pub incarnation: String,
    #[serde(default)]
    pub publish_revision: u64,
    pub id: String,
    pub name: String,
    pub online: bool,
    pub last_seen: DateTime<Utc>,
    pub ports: Vec<Port>,
    pub tunnel_url: Option<String>,
    pub certificate_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IrohInfo {
    pub endpoint_id: String,
    #[serde(deserialize_with = "deserialize_iroh_addresses")]
    pub addresses: Vec<std::net::SocketAddr>,
}
fn deserialize_iroh_addresses<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<std::net::SocketAddr>, D::Error> {
    use serde::de::Error as _;
    let addresses = Vec::<String>::deserialize(deserializer)?;
    // SocketAddr discards textual details such as a zero IPv6 scope. Reject
    // zone syntax before parsing and enforce the wire bound before normalizing.
    if addresses.len() > 8
        || serde_json::to_vec(&addresses).map_or(true, |json| json.len() + 94 > 1024)
    {
        return Err(D::Error::custom("direct iroh addresses exceed wire limit"));
    }
    addresses
        .into_iter()
        .map(|address| {
            if address.contains('%') {
                return Err(D::Error::custom("iroh IPv6 zones are not permitted"));
            }
            address
                .parse()
                .map_err(|_| D::Error::custom("iroh requires a numeric socket address"))
        })
        .collect()
}
impl IrohInfo {
    pub(crate) fn validate(&self) -> Result<()> {
        use std::collections::HashSet;
        if self.endpoint_id.len() != 64
            || !self
                .endpoint_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.addresses.len() > 8
            || serde_json::to_vec(self).map_or(true, |bytes| bytes.len() > 1024)
        {
            return Err(crate::error::protocol("invalid direct iroh metadata"));
        }
        let mut seen = HashSet::new();
        for addr in &self.addresses {
            if !canonical_iroh_address(*addr).is_some_and(|addr| seen.insert(addr)) {
                return Err(crate::error::protocol("invalid direct iroh address"));
            }
        }
        Ok(())
    }
}

impl Config {
    pub(crate) fn connection_budget(&self) -> Duration {
        let iroh = if self.iroh_enabled && self.interface_name.is_none() {
            self.iroh_timeout
        } else {
            Duration::ZERO
        };
        let direct = match self.direct_strategy {
            DirectStrategy::Sequential => self.direct_timeout.saturating_add(iroh),
            DirectStrategy::Parallel => self.direct_timeout.max(iroh),
        };
        direct
            .saturating_add(self.fallback_timeout)
            .saturating_add(Duration::from_secs(2))
            .min(Duration::from_secs(90))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectStrategy {
    #[default]
    Sequential,
    Parallel,
}

pub(crate) fn canonical_iroh_address(
    mut addr: std::net::SocketAddr,
) -> Option<std::net::SocketAddr> {
    if let std::net::SocketAddr::V6(v6) = addr {
        if v6.scope_id() != 0 || v6.flowinfo() != 0 {
            return None;
        }
        if let Some(v4) = v6.ip().to_ipv4_mapped() {
            addr = std::net::SocketAddr::new(v4.into(), addr.port());
        }
    }
    (addr.port() != 0 && !addr.ip().is_unspecified() && !addr.ip().is_multicast()).then_some(addr)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub coordinator_epoch: String,
    #[serde(default)]
    pub incarnation: String,
    pub purpose: String,
    pub app_id: String,
    #[serde(skip_serializing)]
    pub(crate) session_key: String,
    pub expires_at: DateTime<Utc>,
    pub device_id: String,
    pub user: User,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("purpose", &self.purpose)
            .field("app_id", &self.app_id)
            .field("device_id", &self.device_id)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub is_device_flow: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub generation: u64,
    pub epoch: Option<String>,
    pub phase: String,
    pub transport: Option<String>,
    pub peer_id: Option<String>,
    pub message: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Signal {
    pub id: u64,
    #[serde(default)]
    pub coordinator_epoch: String,
    #[serde(default)]
    pub from_incarnation: String,
    #[serde(default)]
    pub to_incarnation: String,
    pub from_device_id: String,
    pub connection_id: String,
    pub kind: String,
    pub payload: String,
}
