use crate::{
    client::Client, error::protocol, BrowserBridge, Device, DirectorySnapshot, Error, Result,
};
use chrono::Utc;
use reqwest::Method;
use serde::Deserialize;
use std::collections::HashSet;
impl Client {
    pub(crate) async fn validate_endpoint(&self, url: &str, root: bool) -> Result<String> {
        match self
            .auth
            .lock()
            .await
            .as_ref()
            .and_then(|a| a.endpoint_policy.as_ref())
        {
            Some(p) => p.tunnel_url(url, root),
            None => crate::endpoints::quick_tunnel(url),
        }
    }
    pub(crate) async fn validate_bridge(&self, b: &BrowserBridge) -> Result<()> {
        b.validate(
            self.auth
                .lock()
                .await
                .as_ref()
                .and_then(|a| a.endpoint_policy.as_ref()),
        )
    }
    /// Complete finite snapshot. The cursor is opaque and cannot grant access.
    pub async fn directory(&self, cursor: Option<&str>) -> Result<DirectorySnapshot> {
        let result = self.directory_inner(cursor).await;
        if result.is_err() {
            self.directory_unknown();
        }
        result
    }
    async fn directory_inner(&self, cursor: Option<&str>) -> Result<DirectorySnapshot> {
        let rt = self.active().await?;
        if cursor.is_some_and(|c| c.len() > 512) {
            return Err(protocol("directory cursor exceeds 512 bytes"));
        }
        let mut config = match self.auth.lock().await.clone() {
            Some(a) => a,
            None => return Err(protocol("trusted directory configuration unavailable")),
        };
        if config.endpoint_policy.is_none() {
            return Err(protocol("server does not advertise versioned directory"));
        }
        let mut url = url::Url::parse(&format!("{}/api/v1/directory", self.cfg.base_url))
            .map_err(|_| protocol("invalid directory URL"))?;
        if let Some(c) = cursor {
            url.query_pairs_mut().append_pair("cursor", c);
        }
        let snapshot: DirectorySnapshot = self
            .http_json(Method::GET, url, None, true, 2 << 20)
            .await?;
        if config
            .endpoint_policy
            .as_ref()
            .is_none_or(|p| p.policy_version != snapshot.endpoint_policy_version)
        {
            config = self.auth_config().await?;
        }
        let p = config
            .endpoint_policy
            .as_ref()
            .filter(|p| p.policy_version == snapshot.endpoint_policy_version)
            .ok_or_else(|| protocol("directory endpoint policy unavailable"))?;
        let session = self
            .session
            .read()
            .await
            .clone()
            .ok_or(Error::NotLoggedIn)?;
        let prefix = format!("v1.{}.", snapshot.epoch);
        let parts = snapshot
            .revision
            .strip_prefix(&prefix)
            .map(|v| v.split('.').collect::<Vec<_>>());
        if snapshot.version != 1
            || snapshot.product_id != config.product_id.as_deref().unwrap_or(&self.cfg.app_id)
            || snapshot.epoch != session.coordinator_epoch
            || snapshot.revision.len() > 512
            || parts.as_ref().is_none_or(|ps| {
                ps.len() != 2
                    || ps.iter().any(|p| {
                        p.len() != 64
                            || !p
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
            })
        {
            return Err(protocol("invalid directory scope or version"));
        }
        let mut previous = None;
        for d in &snapshot.devices {
            if previous.is_some_and(|id: &str| id >= d.id.as_str()) {
                return Err(protocol("unordered or duplicate directory device"));
            }
            previous = Some(&d.id);
            if d.id.is_empty()
                || uuid::Uuid::parse_str(&d.incarnation).is_err()
                || d.publish_revision > crate::endpoints::MAX_SAFE
                || d.ports.len() > 32
                || d.bridges.len() > 4
            {
                return Err(protocol("invalid device metadata"));
            }
            let mut ports = HashSet::new();
            for port in &d.ports {
                port.validate()?;
                if !ports.insert(&port.id) {
                    return Err(protocol("duplicate device port"));
                }
                if let Some(u) = &port.tunnel_url {
                    p.tunnel_url(u, false)?;
                }
            }
            if let Some(u) = &d.tunnel_url {
                p.tunnel_url(u, true)?;
            }
            if let Some(i) = &d.iroh {
                i.validate()?;
            }
            let mut old = None;
            for b in &d.bridges {
                b.validate(Some(p))?;
                if old.is_some_and(|id: &str| id >= b.id.as_str())
                    || b.expiry()? <= snapshot.sampled_at
                    || b.expiry()? > snapshot.sampled_at + chrono::Duration::minutes(5)
                {
                    return Err(protocol("invalid or expired directory bridge"));
                }
                old = Some(&b.id);
            }
        }
        if rt.cancel.is_cancelled() || self.generation() != rt.generation {
            return Err(Error::Cancelled);
        }
        Ok(snapshot)
    }
    pub async fn devices(&self) -> Result<Vec<Device>> {
        if self
            .auth
            .lock()
            .await
            .as_ref()
            .is_some_and(|a| a.endpoint_policy.is_some())
        {
            return Ok(self.directory(None).await?.devices);
        }
        self.active().await?;
        #[derive(Deserialize)]
        struct Devices {
            devices: Vec<Device>,
        }
        let devices = self
            .api::<Devices>(Method::GET, "/devices", None, true)
            .await?
            .devices;
        for d in &devices {
            if let Some(i) = &d.iroh {
                i.validate()?;
            }
            if let Some(u) = &d.tunnel_url {
                crate::endpoints::quick_tunnel(u)?;
            }
            for p in &d.ports {
                p.validate()?;
                if let Some(u) = &p.tunnel_url {
                    crate::endpoints::quick_tunnel(u)?;
                }
            }
            for b in &d.bridges {
                b.validate(None)?;
            }
        }
        Ok(devices)
    }
    /// Declare the SDK listener's root HTTPS ingress. It must forward /connect unchanged.
    /// This never aliases a business port or a browser bridge.
    pub async fn update_device_tunnel_url(&self, url: Option<&str>) -> Result<()> {
        let value = match url {
            Some(v) => Some(self.validate_endpoint(v, true).await?),
            None => None,
        };
        *self.device_tunnel_intent.write().await = value;
        self.intent_changed();
        if self.active().await.is_ok() && self.remote_access_enabled() {
            self.heartbeat().await?;
        }
        Ok(())
    }
    /// Save bridge intent. Publishing requires explicit enable_remote and live configuration.
    pub async fn publish_bridge(&self, bridge: BrowserBridge) -> Result<()> {
        self.validate_bridge(&bridge).await?;
        let expiry = bridge.expiry()?;
        if expiry <= Utc::now() || expiry > Utc::now() + chrono::Duration::minutes(5) {
            return Err(protocol("bridge expiry must be within five minutes"));
        }
        let mut history = self.bridge_history.lock().await;
        if let Some(old) = history.get(&bridge.id) {
            if bridge.address_revision < old.address_revision
                || bridge.address_revision == old.address_revision
                    && (bridge.identity()? != old.identity()? || expiry < old.expiry()?)
            {
                return Err(protocol("bridge address revision conflict"));
            }
        } else if history.len() >= 4 {
            return Err(protocol("maximum four historical bridge ids"));
        }
        let mut intent = self.bridge_intent.write().await;
        history.insert(bridge.id.clone(), bridge.clone());
        intent.insert(bridge.id.clone(), bridge);
        self.intent_changed();
        drop(intent);
        drop(history);
        if self.active().await.is_ok() && self.remote_access_enabled() {
            self.heartbeat().await?;
        }
        Ok(())
    }
    pub async fn renew_bridge(&self, id: &str, expires_at: &str) -> Result<()> {
        let mut b = self
            .bridge_intent
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| protocol("bridge not declared"))?;
        b.expires_at = expires_at.into();
        self.publish_bridge(b).await
    }
    pub async fn withdraw_bridge(&self, id: &str) -> Result<()> {
        let mut intent = self.bridge_intent.write().await;
        if intent.remove(id).is_none() {
            return Err(protocol("bridge not declared"));
        }
        self.intent_changed();
        drop(intent);
        if self.active().await.is_ok() && self.remote_access_enabled() {
            self.heartbeat().await?;
        }
        Ok(())
    }
    pub async fn bridge_intents(&self) -> Vec<BrowserBridge> {
        let mut bs = self
            .bridge_intent
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        bs.sort_by(|a, b| a.id.cmp(&b.id));
        bs
    }
}
impl DirectorySnapshot {
    /// Display association only, scoped to this already authorized snapshot. No key merging.
    pub fn devices_for_installation<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Device> {
        self.devices
            .iter()
            .filter(move |d| d.bridges.iter().any(|b| b.installation_id == id))
    }
}
