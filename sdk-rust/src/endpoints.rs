//! Fixed coordinator policy. Never discover or fetch an endpoint while validating it.
use crate::{error::protocol, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use url::Url;
pub(crate) const MAX_SAFE: u64 = 9_007_199_254_740_991;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointProvider {
    pub id: String,
    pub origins: Vec<String>,
    pub paths: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointPolicy {
    pub version: u8,
    pub policy_version: String,
    pub providers: Vec<EndpointProvider>,
}
fn identifier(v: &str, max: usize, dot: bool) -> bool {
    !v.is_empty()
        && v.len() <= max
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || dot && b == b'.')
}
fn path(v: &str) -> bool {
    v.starts_with('/')
        && v.len() <= 256
        && !v.contains("//")
        && !v.split('/').any(|s| s == "." || s == "..")
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_~.!$&'()*+,;=:@-".contains(&b))
}
fn strict_url(raw: &str, scheme: &str) -> Result<Url> {
    let u = Url::parse(raw).map_err(|_| protocol("invalid endpoint URL"))?;
    if raw.len() > 1024
        || raw.bytes().any(|b| !(0x21..=0x7e).contains(&b))
        || raw.contains(['%', '\\', '?', '#'])
        || u.scheme() != scheme
        || u.host().is_none()
        || u[..url::Position::BeforeHost].contains('@')
        || !path(u.path())
        || !(raw == u.as_str()
            || scheme == "https" && u.path() == "/" && raw == u.origin().ascii_serialization())
    {
        return Err(protocol("noncanonical endpoint URL"));
    }
    Ok(u)
}
fn quick(u: &Url) -> bool {
    u.port().is_none()
        && u.host_str()
            .and_then(|h| h.strip_suffix(".trycloudflare.com"))
            .is_some_and(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.split('-').all(|p| {
                        !p.is_empty()
                            && p.bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    })
            })
}
pub(crate) fn quick_tunnel(raw: &str) -> Result<String> {
    let u = strict_url(raw, "https")?;
    if !quick(&u) || u.path() != "/" {
        return Err(protocol("untrusted Quick Tunnel root"));
    }
    Ok(u.origin().ascii_serialization())
}
impl EndpointPolicy {
    pub fn validate(&self) -> Result<()> {
        if serde_json::to_vec(self)
            .map_err(|_| protocol("invalid endpoint policy"))?
            .len()
            > 32 * 1024
            || self.version != 1
            || self.providers.len() > 16
        {
            return Err(protocol("unsupported endpoint policy"));
        }
        let mut ids = BTreeSet::new();
        let mut entries = Vec::new();
        for p in &self.providers {
            if !identifier(&p.id, 64, false)
                || p.id == "quick-tunnel"
                || !ids.insert(&p.id)
                || p.origins.is_empty()
                || p.origins.len() > 16
                || p.paths.is_empty()
                || p.paths.len() > 16
            {
                return Err(protocol("invalid endpoint provider"));
            }
            let mut origins = BTreeSet::new();
            for o in &p.origins {
                let u = strict_url(o, "https")?;
                if *o != u.origin().ascii_serialization() || !origins.insert(o) {
                    return Err(protocol("invalid provider origin"));
                }
            }
            let mut paths = BTreeSet::new();
            for s in &p.paths {
                if !path(s) || !paths.insert(s) {
                    return Err(protocol("invalid provider path"));
                }
            }
            entries.push((
                p.id.clone(),
                origins.into_iter().cloned().collect::<Vec<_>>(),
                paths.into_iter().cloned().collect::<Vec<_>>(),
            ));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let bytes = serde_json::to_vec(&("cybergrid-endpoint-policy-v1", entries))
            .map_err(|_| protocol("invalid policy serialization"))?;
        if hex::encode(Sha256::digest(bytes)) != self.policy_version {
            return Err(protocol("endpoint policy digest mismatch"));
        }
        Ok(())
    }
    pub fn tunnel_url(&self, raw: &str, device_root: bool) -> Result<String> {
        let u = strict_url(raw, "https")?;
        if device_root && u.path() != "/" {
            return Err(protocol("device endpoint must be HTTPS root"));
        }
        if quick(&u) && u.path() == "/" {
            return Ok(u.origin().ascii_serialization());
        }
        if !self.providers.iter().any(|p| {
            p.origins.contains(&u.origin().ascii_serialization())
                && p.paths.iter().any(|s| s == u.path())
        }) {
            return Err(protocol("untrusted tunnel endpoint"));
        }
        Ok(if u.path() == "/" {
            u.origin().ascii_serialization()
        } else {
            u.into()
        })
    }
    pub fn bridge_url(&self, raw: &str, provider: &str) -> Result<String> {
        let u = strict_url(raw, "wss")?;
        if provider == "quick-tunnel" {
            if quick(&u) {
                return Ok(u.into());
            }
        } else {
            let origin = format!(
                "https://{}",
                &u[url::Position::BeforeHost..url::Position::AfterPort]
            );
            if self.providers.iter().any(|p| {
                p.id == provider
                    && p.origins.contains(&origin)
                    && p.paths.iter().any(|s| s == u.path())
            }) {
                return Ok(u.into());
            }
        }
        Err(protocol("untrusted bridge endpoint"))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeLimits {
    pub max_frame_bytes: u32,
    pub max_streams: u32,
    pub max_pending_requests: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserBridge {
    pub id: String,
    pub protocol: String,
    pub provider_id: String,
    pub url: String,
    pub installation_id: String,
    pub public_key: String,
    pub address_revision: u64,
    pub expires_at: String,
    pub limits: BridgeLimits,
}
impl BrowserBridge {
    pub fn validate(&self, policy: Option<&EndpointPolicy>) -> Result<()> {
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&self.public_key)
            .map_err(|_| protocol("invalid bridge public key"))?;
        let timestamp = DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|_| protocol("invalid bridge expiry"))?;
        if !identifier(&self.id, 64, false)
            || !identifier(&self.protocol, 128, true)
            || !identifier(&self.provider_id, 64, false)
            || !identifier(&self.installation_id, 128, false)
            || key.len() != 32
            || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key) != self.public_key
            || self.address_revision == 0
            || self.address_revision > MAX_SAFE
            || timestamp
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                != self.expires_at
            || !(1..=65535).contains(&self.limits.max_frame_bytes)
            || !(1..=32).contains(&self.limits.max_streams)
            || !(1..=128).contains(&self.limits.max_pending_requests)
        {
            return Err(protocol("invalid bridge descriptor"));
        }
        match policy {
            Some(p) => {
                p.bridge_url(&self.url, &self.provider_id)?;
            }
            None => {
                let u = strict_url(&self.url, "wss")?;
                if self.provider_id != "quick-tunnel" || !quick(&u) {
                    return Err(protocol("trusted provider policy unavailable"));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn identity(&self) -> Result<Vec<u8>> {
        let mut v = serde_json::to_value(self).map_err(|_| protocol("invalid bridge"))?;
        let o = v
            .as_object_mut()
            .ok_or_else(|| protocol("invalid bridge"))?;
        o.remove("expiresAt");
        o.remove("addressRevision");
        serde_json::to_vec(&v).map_err(|_| protocol("invalid bridge"))
    }
    pub(crate) fn expiry(&self) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| protocol("invalid bridge expiry"))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySnapshot {
    pub version: u8,
    pub product_id: String,
    pub epoch: String,
    pub endpoint_policy_version: String,
    pub revision: String,
    pub sampled_at: DateTime<Utc>,
    pub reset: bool,
    pub changed: bool,
    pub devices: Vec<crate::Device>,
}
