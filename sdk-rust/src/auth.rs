use crate::{client::Client, error::protocol, Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coordination {
    pub protocol: String,
    pub epoch: String,
    pub heartbeat_seconds: u64,
    pub lease_seconds: u64,
    pub signal_ttl_seconds: u64,
    pub max_signal_wait_seconds: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default)]
    pub login_flow: Option<String>,
    #[serde(default)]
    pub coordination: Option<Coordination>,
    pub provider: String,
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub scope: String,
    pub device_authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
}
fn https(raw: &str) -> Result<Url> {
    let u = Url::parse(raw).map_err(|_| protocol("invalid OAuth URL"))?;
    if u.scheme() != "https"
        || u.host().is_none()
        || u[..url::Position::BeforeHost].contains('@')
        || u.fragment().is_some()
    {
        return Err(protocol(
            "OAuth URL must be HTTPS without credentials/fragment",
        ));
    }
    Ok(u)
}
impl AuthConfig {
    pub(crate) fn validate(&self, app_id: &str) -> Result<()> {
        if self
            .login_flow
            .as_deref()
            .is_some_and(|flow| !["authorization_code", "device_code"].contains(&flow))
        {
            return Err(protocol("unsupported loginFlow"));
        }
        let issuer = https(&self.issuer)?;
        https(&self.resource)?;
        if issuer.query().is_some()
            || self.provider != "logto"
            || self.client_id != app_id
            || self.resource.is_empty()
        {
            return Err(protocol("Logto auth clientId must match explicit AppID"));
        }
        for ep in [&self.device_authorization_endpoint, &self.token_endpoint]
            .into_iter()
            .chain(self.revocation_endpoint.iter())
        {
            let u = https(ep)?;
            if u.origin() != issuer.origin() || u.query().is_some() {
                return Err(protocol("OAuth endpoint must be HTTPS on issuer origin"));
            }
        }
        for required in ["openid", "offline_access", "connect"] {
            if !self.scope.split_whitespace().any(|v| v == required) {
                return Err(protocol(format!("missing {required} scope")));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorization {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(skip)]
    pub(crate) device_code: String,
    #[serde(skip)]
    pub(crate) deadline: Instant,
    #[serde(skip)]
    pub(crate) config: AuthConfig,
    #[serde(skip)]
    pub(crate) client_nonce: String,
}
impl fmt::Debug for DeviceAuthorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceAuthorization{verification required}")
    }
}
#[derive(Clone)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub expires_at: DateTime<Utc>,
    pub token_type: String,
    pub scope: String,
}
/// Private browser-journal crash reconciliation. A different existing grant is
/// ambiguous: callers must retain the journal and must not revoke its token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserGrantCheckpoint {
    Matching,
    Empty,
    OtherGrant,
}
/// Both acknowledgements transfer refresh ownership to Rust. The producing
/// browser SDK must stop all token operations and clear its temporary memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantImportOutcome {
    OwnedPersisted,
    /// Retain this Client and its CredentialStore; retry the checkpoint or revoke.
    /// Do not exchange a CyberGrid session until the checkpoint succeeds.
    OwnedPending,
}
impl fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OAuthTokens{redacted}")
    }
}
fn safe_code(code: &str) -> String {
    match code {
        "invalid_request"
        | "unauthorized_client"
        | "access_denied"
        | "unsupported_response_type"
        | "invalid_scope"
        | "server_error"
        | "temporarily_unavailable"
        | "invalid_client"
        | "invalid_grant"
        | "unsupported_grant_type"
        | "authorization_pending"
        | "slow_down"
        | "expired_token"
        | "invalid_target" => code.into(),
        _ => "invalid_response".into(),
    }
}
impl Client {
    /// Checks an explicit private store under the normal grant/file gates,
    /// without token calls or a network dependency. This does not authenticate
    /// a session or authorize overwriting any stored credential.
    pub async fn browser_grant_checkpoint(
        &self,
        expected: &AuthConfig,
        refresh: &str,
    ) -> Result<BrowserGrantCheckpoint> {
        expected.validate(&self.cfg.app_id)?;
        if refresh.is_empty() {
            return Err(protocol("refresh token required"));
        }
        let _grant = self.grant_operation.lock().await;
        let store = self.grant_store.read().await.clone().ok_or_else(|| {
            Error::Credentials("browser journal requires an explicit credential store".into())
        })?;
        let _auth = store.auth_gate.lock().await;
        let _file = store.lock().await?;
        store.browser_checkpoint(self, expected, refresh).await
    }
    /// One-time browser SDK handoff, without refreshing or authenticating a JWT.
    /// `expected` is the trusted configuration captured before authorization.
    /// An error means ownership was not accepted: the source must retain and
    /// clean up its grant. Both successful outcomes are ownership ACKs, even if
    /// disk persistence failed. Never race/drop this future after submission;
    /// retain the Client/store until its outcome and cleanup are resolved.
    pub async fn import_browser_grant(
        &self,
        expected: &AuthConfig,
        tokens: OAuthTokens,
    ) -> Result<GrantImportOutcome> {
        let _grant = self.grant_operation.lock().await;
        let store = self.grant_store.read().await.clone().ok_or_else(|| {
            Error::Credentials("browser handoff requires an explicit credential store".into())
        })?;
        let _auth = store.auth_gate.lock().await;
        expected.validate(&self.cfg.app_id)?;
        let mut expected = expected.clone();
        expected.coordination = None;
        // A lost ACK is confirmed from private ownership state, without a
        // network dependency, refresh, or rewriting the credential file.
        if let Some(imported) = self.browser_imported.lock().await.as_ref() {
            let mut cached =
                self.auth.lock().await.clone().ok_or_else(|| {
                    Error::Credentials("browser handoff binding unavailable".into())
                })?;
            cached.coordination = None;
            if imported != &tokens.refresh_token || cached != expected {
                return Err(Error::Credentials(
                    "browser handoff cannot replace an owned grant".into(),
                ));
            }
            return store.browser_import_ack(self, imported).await;
        }
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        let mut current = self.auth_config().await?;
        current.coordination = None;
        if current != expected {
            return Err(Error::Credentials(
                "browser handoff OAuth binding differs".into(),
            ));
        }
        let file = store.lock().await?;
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        store.require_empty_browser_destination(self).await?;
        if tokens.access_token.is_empty()
            || tokens.access_token.len() > 65536
            || tokens.refresh_token.is_empty()
            || tokens.refresh_token.len() > 32768
            || !tokens.token_type.eq_ignore_ascii_case("bearer")
            || tokens.expires_in == 0
            || tokens.expires_in > 86400
            || tokens.expires_at <= Utc::now()
            || tokens.expires_at > Utc::now() + chrono::Duration::seconds(86460)
            || !tokens.scope.split_whitespace().any(|s| s == "connect")
        {
            return Err(protocol("invalid browser resource grant"));
        }
        // checkpoint_owned installs pending ownership before fallible storage.
        // A storage failure is an ACK with pending ownership, not a rejection.
        let refresh = tokens.refresh_token.clone();
        let outcome = match self.checkpoint_owned(tokens, Some(&file)).await {
            Ok(()) => GrantImportOutcome::OwnedPersisted,
            Err(_) => GrantImportOutcome::OwnedPending,
        };
        *self.browser_imported.lock().await = Some(refresh);
        Ok(outcome)
    }
    pub async fn auth_config(&self) -> Result<AuthConfig> {
        let mut url = Url::parse(&format!("{}/api/v1/auth/config", self.cfg.base_url))
            .map_err(|_| protocol("invalid API URL"))?;
        url.query_pairs_mut().append_pair("appId", &self.cfg.app_id);
        let a: AuthConfig = self
            .http_json(reqwest::Method::GET, url, None, false, 2 << 20)
            .await?;
        a.validate(&self.cfg.app_id)?;
        let mut cached = self.auth.lock().await;
        if cached.as_ref().is_some_and(|old| {
            let mut old = old.clone();
            let mut current = a.clone();
            old.coordination = None;
            current.coordination = None;
            old != current
        }) {
            return Err(protocol("OAuth configuration changed; create a new client"));
        }
        *cached = Some(a.clone());
        Ok(a)
    }
    async fn oauth(&self, endpoint: &str, form: &[(&str, &str)]) -> Result<serde_json::Value> {
        let r = self.http.post(endpoint).form(form).send().await?;
        let status = r.status().as_u16();
        let bytes = Self::bounded_body(r, 64 << 10).await?;
        if !(200..300).contains(&status) {
            let code = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v.get("error").and_then(|v| v.as_str()).map(safe_code))
                .unwrap_or_else(|| "invalid_response".into());
            return Err(Error::OAuth { status, code });
        }
        serde_json::from_slice(&bytes).map_err(|_| protocol("invalid OAuth JSON"))
    }
    async fn token(
        &self,
        a: &AuthConfig,
        form: &[(&str, &str)],
        file_locked: Option<&std::fs::File>,
    ) -> Result<OAuthTokens> {
        let v = self.oauth(&a.token_endpoint, form).await?;
        // Even an invalid resource response may carry a real rotated grant.
        // Take ownership before validating optional resource fields.
        if let Some(refresh) = v
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            self.checkpoint_owned(
                OAuthTokens {
                    access_token: v
                        .get("access_token")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    refresh_token: refresh.into(),
                    expires_in: 0,
                    expires_at: Utc::now(),
                    token_type: "Bearer".into(),
                    scope: String::new(),
                },
                file_locked,
            )
            .await?;
        }
        let access = v
            .get("access_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| protocol("missing OAuth access token"))?;
        let expires = v
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .filter(|&v| v > 0 && v <= 86400)
            .ok_or_else(|| protocol("invalid OAuth expiry"))?;
        let kind = v
            .get("token_type")
            .and_then(|v| v.as_str())
            .filter(|s| s.eq_ignore_ascii_case("bearer"))
            .ok_or_else(|| protocol("OAuth token_type must be Bearer"))?;
        Ok(OAuthTokens {
            access_token: access.into(),
            refresh_token: v
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            expires_in: expires,
            expires_at: Utc::now() + chrono::Duration::seconds(expires as i64),
            token_type: kind.into(),
            scope: v.get("scope").and_then(|v| v.as_str()).unwrap_or("").into(),
        })
    }
    pub async fn begin_device_authorization(&self) -> Result<DeviceAuthorization> {
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        let a = self.auth_config().await?;
        if a.login_flow.as_deref() == Some("authorization_code") {
            return Err(protocol(
                "Device Flow is not supported; use official browser authorization with PKCE",
            ));
        }
        let v = self
            .oauth(
                &a.device_authorization_endpoint,
                &[
                    ("client_id", &a.client_id),
                    ("scope", &a.scope),
                    ("resource", &a.resource),
                ],
            )
            .await?;
        let device_code = v
            .get("device_code")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| protocol("missing device code"))?
            .to_owned();
        let user_code = v
            .get("user_code")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| protocol("missing user code"))?
            .to_owned();
        let verification_uri = v
            .get("verification_uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| protocol("missing verification URI"))?
            .to_owned();
        let verification_uri_complete = v
            .get("verification_uri_complete")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let issuer = https(&a.issuer)?;
        if https(&verification_uri)?.origin() != issuer.origin() {
            return Err(protocol("verification URI must share issuer origin"));
        }
        if let Some(ref u) = verification_uri_complete {
            if https(u)?.origin() != issuer.origin() {
                return Err(protocol(
                    "complete verification URI must share issuer origin",
                ));
            }
        }
        let expires = v
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .filter(|&v| v > 0 && v <= 86400)
            .ok_or_else(|| protocol("invalid device authorization expiry"))?;
        let interval = v.get("interval").and_then(|v| v.as_u64()).unwrap_or(5);
        if interval > expires {
            return Err(protocol("invalid device authorization interval"));
        }
        Ok(DeviceAuthorization {
            user_code,
            verification_uri,
            verification_uri_complete,
            expires_in: expires,
            interval: interval.max(5),
            device_code,
            deadline: Instant::now() + Duration::from_secs(expires),
            config: a,
            client_nonce: self.nonce.clone(),
        })
    }
    pub async fn complete_device_authorization(
        &self,
        a: &DeviceAuthorization,
        cancel: &CancellationToken,
    ) -> Result<OAuthTokens> {
        if a.client_nonce != self.nonce || a.device_code.is_empty() {
            return Err(protocol("authorization belongs to another client"));
        }
        let mut interval = Duration::from_secs(a.interval.max(5));
        loop {
            let remaining = a
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Timeout("device authorization"))?;
            tokio::select! { _=self.closed_token().cancelled()=>return Err(Error::Cancelled), _=cancel.cancelled()=>return Err(Error::Cancelled), _=tokio::time::sleep(remaining)=>return Err(Error::Timeout("device authorization")), _=tokio::time::sleep(interval)=>{} }
            let form = [
                ("client_id", a.config.client_id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", a.device_code.as_str()),
            ];
            // Once a token poll is sent, finish handling its result: dropping a successful
            // response on cancellation could lose a newly minted refresh grant.
            let grant_guard = self.grant_operation.lock().await;
            let store = self.grant_store.read().await.clone();
            let guard = match store.as_ref() {
                Some(s) => Some(s.auth_gate.lock().await),
                None => None,
            };
            let file_guard = match store.as_ref() {
                Some(s) => Some(s.lock().await?),
                None => None,
            };
            let result = self.token(&a.config, &form, file_guard.as_ref()).await;
            drop(file_guard);
            drop(guard);
            drop(grant_guard);
            match result {
                Ok(t) => {
                    if t.refresh_token.is_empty() {
                        return Err(protocol(
                            "offline refresh token required for resource grant",
                        ));
                    }
                    // Take private ownership before the next fallible network operation.

                    let mut resource = self.refresh_access_token(&t.refresh_token).await?;
                    if resource.refresh_token.is_empty() {
                        resource.refresh_token = t.refresh_token;
                    }
                    return Ok(resource);
                }
                Err(Error::OAuth {
                    status: 400,
                    ref code,
                }) if code == "authorization_pending" => {}
                Err(Error::OAuth {
                    status: 400,
                    ref code,
                }) if code == "slow_down" => interval += Duration::from_secs(5),
                Err(e) => return Err(e),
            }
        }
    }
    pub async fn refresh_access_token(&self, refresh: &str) -> Result<OAuthTokens> {
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        let _grant = self.grant_operation.lock().await;
        let store = self.grant_store.read().await.clone();
        let _auth = match store.as_ref() {
            Some(s) => Some(s.auth_gate.lock().await),
            None => None,
        };
        let _file = match store.as_ref() {
            Some(s) => Some(s.lock().await?),
            None => None,
        };
        let owned = match store.as_ref() {
            Some(s) => {
                s.latest_refresh(self, _file.as_ref().expect("store file guard"))
                    .await?
            }
            None => {
                let p = self.pending_grant_value().await;
                p.map(|t| t.refresh_token)
                    .unwrap_or_else(|| refresh.to_owned())
            }
        };
        let refresh = owned.as_str();
        if refresh.is_empty() {
            return Err(protocol("refresh token required"));
        }
        let a = self.auth_config().await?;
        let mut tokens = self
            .token(
                &a,
                &[
                    ("client_id", &a.client_id),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("resource", &a.resource),
                ],
                _file.as_ref(),
            )
            .await?;
        if tokens.refresh_token.is_empty() {
            tokens.refresh_token = refresh.to_owned();
        }
        self.checkpoint_owned(tokens.clone(), _file.as_ref())
            .await?;
        Ok(tokens)
    }
    pub async fn revoke_refresh_token(&self, refresh: &str) -> Result<()> {
        if refresh.is_empty() {
            return Err(protocol("refresh token required"));
        }
        let mut a = self.auth_config().await?;
        if a.revocation_endpoint.is_none() {
            let response = self
                .http
                .get(format!(
                    "{}/.well-known/openid-configuration",
                    a.issuer.trim_end_matches('/')
                ))
                .send()
                .await?;
            let status = response.status();
            let b = Self::bounded_body(response, 64 << 10).await?;
            if !status.is_success() {
                return Err(protocol("OIDC discovery failed"));
            }
            let v: serde_json::Value =
                serde_json::from_slice(&b).map_err(|_| protocol("invalid discovery JSON"))?;
            if v.get("issuer").and_then(|v| v.as_str()) != Some(a.issuer.as_str()) {
                return Err(protocol("discovery issuer mismatch"));
            }
            a.revocation_endpoint = v
                .get("revocation_endpoint")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            a.validate(&self.cfg.app_id)?;
        }
        let ep = a
            .revocation_endpoint
            .ok_or_else(|| protocol("provider does not advertise token revocation"))?;
        let r = self
            .http
            .post(ep)
            .form(&[
                ("client_id", a.client_id.as_str()),
                ("token", refresh),
                ("token_type_hint", "refresh_token"),
            ])
            .send()
            .await?;
        let status = r.status().as_u16();
        let _ = Self::bounded_body(r, 64 << 10).await?;
        if status != 200 {
            return Err(Error::OAuth {
                status,
                code: "revocation_failed".into(),
            });
        }
        Ok(())
    }
}
