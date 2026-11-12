use crate::{
    error::protocol, rtc::Attempt, secure::Identity, types::Signal, Application, AuthConfig,
    BusinessAuthorization, Config, CoordinatorRevocation, CredentialStore, Device, Error, Event,
    LayerState, OAuthTokens, Peer, Port, RemoteDisableOutcome, Result, Session, StatePhase,
    StateReason, StateSnapshot,
};
use chrono::Utc;
use reqwest::{Method, Response};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{broadcast, watch, Mutex, Notify, RwLock},
};
use tokio_util::sync::CancellationToken;
use url::Url;

pub type AuthFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthRequestReason {
    Connect,
    CoordinationRecovery,
    AuthorizationRejected,
}
#[derive(Clone, Debug)]
pub struct AuthTokenRequest {
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub min_validity: u64,
    pub force_refresh: bool,
    pub reason: AuthRequestReason,
    pub generation: u64,
    pub cancel: CancellationToken,
}
/// Explicit recovery credential source. Implementations must retain every newly
/// rotated grant privately on checkpoint failure; never prompt for interactive login.
pub trait AuthProvider: Send + Sync {
    fn access_token<'a>(&'a self, _client: &'a Client) -> AuthFuture<'a> {
        Box::pin(async { Err(Error::AuthorizationUnavailable) })
    }
    /// Legacy adapters cannot guarantee freshness. A host implementing cached
    /// access tokens must override this method and honor the request context.
    fn access_token_for<'a>(
        &'a self,
        client: &'a Client,
        _request: &'a AuthTokenRequest,
    ) -> AuthFuture<'a> {
        self.access_token(client)
    }
}

// Never Debug/Serialize: an uncertain operation privately retains its exact token.
struct AdmissionOperation {
    challenge: Option<String>,
    request: String,
    ticket: Option<String>,
    expires: Option<chrono::DateTime<Utc>>,
    mint_unknown: bool,
}
struct RegistrationAttempt {
    token: String,
    request: String,
    revision: u64,
    config: AuthConfig,
    unknown: bool,
    force_refreshed: bool,
    admission: Option<AdmissionOperation>,
    expired: bool,
}

pub(crate) struct Runtime {
    pub iroh: Option<Arc<crate::iroh_direct::DirectEndpoint>>,
    pub identity: Identity,
    pub cancel: CancellationToken,
    pub ports: RwLock<HashMap<String, Port>>,
    pub tunnel_url: RwLock<Option<String>>,
    pub origin_addr: std::net::SocketAddr,
    pub last_heartbeat: Mutex<Instant>,
    pub peers: Mutex<Vec<Arc<Peer>>>,
    pub attempts: Mutex<HashMap<String, Arc<Attempt>>>,
    pub generation: u64,
    pub ready: AtomicBool,
    pub provider_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
/// Thread-safe client with explicit credentials and one recovery supervisor.
pub struct Client {
    pub(crate) cfg: Config,
    pub(crate) http: reqwest::Client,
    #[cfg(test)]
    pub(crate) ws_connector: std::sync::RwLock<Option<Arc<rustls::ClientConfig>>>,
    pub(crate) auth: Mutex<Option<AuthConfig>>,
    pub(crate) nonce: String,
    pub(crate) session: RwLock<Option<Session>>,
    pub(crate) runtime: RwLock<Option<Arc<Runtime>>>,
    events: broadcast::Sender<Event>,
    state: watch::Sender<StateSnapshot>,
    mutation: Mutex<()>,
    publication: Mutex<u64>,
    pub(crate) ports: RwLock<HashMap<String, Port>>,
    closed: AtomicBool,
    remote_enabled: AtomicBool,
    intent_revision: AtomicU64,
    lifetime: CancellationToken,
    generation: AtomicU64,
    registration_revision: AtomicU64,
    registration_key: String,
    recovery: Notify,
    recovery_started: AtomicBool,
    initial_login_recovery: AtomicBool,
    hard_failure: AtomicBool,
    recovery_pending: AtomicBool,
    retry_after: AtomicU64,
    provider: RwLock<Option<Arc<dyn AuthProvider>>>,
    name: RwLock<String>,
    original_user: RwLock<Option<String>>,
    pending_registration: Mutex<Option<RegistrationAttempt>>,
    retired_session: Mutex<Option<Session>>,
    retired_unknown: AtomicBool,
    pub(crate) bridge_intent: RwLock<HashMap<String, crate::BrowserBridge>>,
    pub(crate) bridge_history: Mutex<HashMap<String, crate::BrowserBridge>>,
    bridge_incarnation: Mutex<Option<String>>,
    pub(crate) device_tunnel_intent: RwLock<Option<String>>,
    pub(crate) grant_operation: Mutex<()>,
    pub(crate) grant_store: RwLock<Option<CredentialStore>>,
    pending_grant: Mutex<Option<OAuthTokens>>,
    pub(crate) latest_grant: Mutex<Option<String>>,
    pub(crate) browser_imported: Mutex<Option<String>>,
}
impl Client {
    pub fn new(mut cfg: Config) -> Result<Arc<Self>> {
        crate::types::validate_config(&mut cfg)?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(35))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let (events, _) = broadcast::channel(128);
        let (state, _) = watch::channel(StateSnapshot::default());
        // Two UUIDv4 supply 244 random bits. registrationKey requires full 256;
        // use the OS RNG directly through rustls' crypto provider.
        let mut key = [0u8; 32];
        rustls::crypto::ring::default_provider()
            .secure_random
            .fill(&mut key)
            .map_err(|_| protocol("OS random unavailable"))?;
        use base64::Engine;
        Ok(Arc::new(Self {
            cfg,
            http,
            #[cfg(test)]
            ws_connector: std::sync::RwLock::new(None),
            auth: Mutex::new(None),
            nonce: uuid::Uuid::new_v4().to_string(),
            session: RwLock::new(None),
            runtime: RwLock::new(None),
            events,
            state,
            mutation: Mutex::new(()),
            publication: Mutex::new(0),
            ports: RwLock::new(HashMap::new()),
            closed: AtomicBool::new(false),
            remote_enabled: AtomicBool::new(false),
            intent_revision: AtomicU64::new(0),
            lifetime: CancellationToken::new(),
            generation: AtomicU64::new(0),
            registration_revision: AtomicU64::new(0),
            registration_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key),
            recovery: Notify::new(),
            recovery_started: AtomicBool::new(false),
            initial_login_recovery: AtomicBool::new(false),
            hard_failure: AtomicBool::new(false),
            recovery_pending: AtomicBool::new(false),
            retry_after: AtomicU64::new(0),
            provider: RwLock::new(None),
            name: RwLock::new(String::new()),
            original_user: RwLock::new(None),
            pending_registration: Mutex::new(None),
            retired_session: Mutex::new(None),
            retired_unknown: AtomicBool::new(false),
            bridge_intent: RwLock::new(HashMap::new()),
            bridge_history: Mutex::new(HashMap::new()),
            bridge_incarnation: Mutex::new(None),
            device_tunnel_intent: RwLock::new(None),
            grant_operation: Mutex::new(()),
            grant_store: RwLock::new(None),
            pending_grant: Mutex::new(None),
            latest_grant: Mutex::new(None),
            browser_imported: Mutex::new(None),
        }))
    }
    pub(crate) fn closed_token(&self) -> &CancellationToken {
        &self.lifetime
    }
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub(crate) fn mapping_cancel(&self) -> CancellationToken {
        self.lifetime.child_token()
    }
    pub fn config(&self) -> &Config {
        &self.cfg
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    /// A receiver immediately contains the current snapshot, including before login.
    pub fn watch_state(&self) -> watch::Receiver<StateSnapshot> {
        self.state.subscribe()
    }
    pub fn snapshot(&self) -> StateSnapshot {
        self.state.borrow().clone()
    }
    pub fn set_business_authorization(&self, authorization: BusinessAuthorization) {
        self.update_state(|state| state.business_authorization = authorization);
    }
    fn update_state(&self, update: impl FnOnce(&mut StateSnapshot)) {
        self.state.send_modify(|state| {
            update(state);
            state.revision = state
                .revision
                .checked_add(1)
                .expect("state revision exhausted");
            state.generation = self.generation();
            state.remote_enabled = self.remote_access_enabled();
            state.intent_revision = self.intent_revision.load(Ordering::SeqCst);
            if self.is_closed() {
                state.coordination =
                    LayerState::new(StatePhase::Closed, StateReason::Closed, false);
                state.publication =
                    LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false);
                state.reachability =
                    LayerState::new(StatePhase::Closed, StateReason::Closed, false);
                state.confirmed_intent_revision = None;
                state.server_publish_revision = None;
            }
        });
    }
    pub fn remote_access_enabled(&self) -> bool {
        self.remote_enabled.load(Ordering::SeqCst) && !self.is_closed()
    }
    pub(crate) fn require_remote_access(&self) -> Result<()> {
        if self.remote_access_enabled() {
            Ok(())
        } else {
            Err(Error::NotLoggedIn)
        }
    }
    pub async fn port_intents(&self) -> Vec<Port> {
        let mut ports: Vec<_> = self.ports.read().await.values().cloned().collect();
        ports.sort_by(|a, b| a.id.cmp(&b.id));
        ports
    }
    pub async fn set_auth_provider(&self, provider: Arc<dyn AuthProvider>) {
        *self.provider.write().await = Some(provider);
    }
    /// Explicit store binding also checkpoints grants before resource exchange.
    pub async fn use_credential_store(&self, store: CredentialStore) -> Result<()> {
        let _grant = self.grant_operation.lock().await;
        let mut current = self.grant_store.write().await;
        if let Some(existing) = current.as_ref() {
            if existing.path() != store.path() {
                return Err(Error::Credentials(
                    "credential store binding cannot change within Client".into(),
                ));
            }
            return Ok(());
        }
        *current = Some(store.clone());
        drop(current);
        let pending = self.pending_grant.lock().await.clone();
        if let Some(tokens) = pending {
            let _auth = store.auth_gate.lock().await;
            self.checkpoint_owned(tokens, None).await?;
        }
        Ok(())
    }
    pub(crate) async fn pending_grant_value(&self) -> Option<OAuthTokens> {
        self.pending_grant.lock().await.clone()
    }
    pub async fn has_pending_grant(&self) -> bool {
        if let Some(store) = self.grant_store.read().await.clone() {
            return store.has_pending().await;
        }
        self.pending_grant.lock().await.is_some()
    }
    pub(crate) async fn checkpoint_grant(&self, tokens: OAuthTokens) -> Result<()> {
        self.checkpoint_owned(tokens, None).await
    }
    pub(crate) async fn checkpoint_owned(
        &self,
        tokens: OAuthTokens,
        file_locked: Option<&std::fs::File>,
    ) -> Result<()> {
        *self.latest_grant.lock().await = Some(tokens.refresh_token.clone());
        *self.pending_grant.lock().await = Some(tokens.clone());
        if let Some(store) = self.grant_store.read().await.clone() {
            if let Some(file) = file_locked {
                store.save_under_file_lock(self, &tokens, file).await?;
            } else {
                store.save_checkpoint(self, &tokens).await?;
            }
            self.pending_grant.lock().await.take();
        }
        Ok(())
    }
    pub(crate) async fn clear_revoked_grant(&self, refresh: &str) {
        let mut pending = self.pending_grant.lock().await;
        if pending.as_ref().is_some_and(|t| t.refresh_token == refresh) {
            pending.take();
        }
        let mut latest = self.latest_grant.lock().await;
        if latest.as_deref() == Some(refresh) {
            latest.take();
        }
    }
    pub async fn revoke_pending_grant(&self) -> Result<()> {
        let _grant = self.grant_operation.lock().await;
        let store = self.grant_store.read().await.clone();
        if let Some(store) = store {
            return store.logout(self).await;
        }
        let pending = self.pending_grant.lock().await.clone();
        if let Some(tokens) = pending {
            self.revoke_refresh_token(&tokens.refresh_token).await?;
            self.clear_revoked_grant(&tokens.refresh_token).await;
        }
        Ok(())
    }
    pub async fn retry_pending_grant(&self) -> Result<()> {
        let _grant = self.grant_operation.lock().await;
        let store = self.grant_store.read().await.clone();
        let _auth = match store.as_ref() {
            Some(s) => Some(s.auth_gate.lock().await),
            None => None,
        };
        let pending = match store.as_ref() {
            Some(store) => store.pending_tokens().await,
            None => self.pending_grant.lock().await.clone(),
        };
        if pending.is_none() {
            self.pending_grant.lock().await.take();
        }
        if let Some(t) = pending {
            self.checkpoint_grant(t).await?;
        }
        Ok(())
    }
    pub(crate) fn event(
        &self,
        phase: &str,
        transport: Option<&str>,
        peer: Option<&str>,
        message: Option<String>,
    ) {
        let epoch = self
            .session
            .try_read()
            .ok()
            .and_then(|s| s.as_ref().map(|s| s.coordinator_epoch.clone()));
        self.update_state(|s| {
            s.epoch = epoch.clone();
            match phase {
                "coordination-ready" => {
                    s.identity = LayerState::new(
                        StatePhase::Authenticated,
                        StateReason::CoordinationReady,
                        false,
                    );
                    s.coordination =
                        LayerState::new(StatePhase::Ready, StateReason::CoordinationReady, false);
                }
                "recovering" => {
                    s.coordination = LayerState::new(
                        StatePhase::Recovering,
                        StateReason::CoordinationLost,
                        true,
                    );
                    s.publication = if self.remote_access_enabled() {
                        LayerState::new(StatePhase::Pending, StateReason::CoordinationLost, true)
                    } else {
                        LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false)
                    };
                    s.confirmed_intent_revision = None;
                    s.server_publish_revision = None;
                    s.reachability =
                        LayerState::new(StatePhase::Unknown, StateReason::CoordinationLost, true);
                }
                "requires-login" => {
                    s.identity = LayerState::new(
                        StatePhase::RequiresLogin,
                        StateReason::AuthorizationUnavailable,
                        false,
                    );
                    // Preserve the authoritative expiry reason so hosts can
                    // distinguish explicit fresh login from unknown 401/expiry.
                    if s.coordination.reason != StateReason::AdmissionExpired {
                        s.coordination = LayerState::new(
                            StatePhase::Unavailable,
                            StateReason::CoordinationLost,
                            false,
                        );
                    }
                }
                "connecting" | "direct-attempt" | "iroh-attempt" | "fallback-attempt" => {
                    s.reachability = LayerState::new(
                        StatePhase::Connecting,
                        StateReason::TransportConnecting,
                        true,
                    )
                }
                "multiplexer-ready" => {
                    s.reachability =
                        LayerState::new(StatePhase::Ready, StateReason::TransportReady, false)
                }
                "mapping-disconnected" => {
                    s.reachability = LayerState::new(
                        StatePhase::Unknown,
                        StateReason::TransportDisconnected,
                        true,
                    )
                }
                "closed" => {
                    s.coordination =
                        LayerState::new(StatePhase::Closed, StateReason::Closed, false);
                    s.publication =
                        LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false);
                    s.reachability =
                        LayerState::new(StatePhase::Closed, StateReason::Closed, false);
                    s.confirmed_intent_revision = None;
                    s.server_publish_revision = None;
                }
                _ => {}
            }
        });
        let _ = self.events.send(Event {
            generation: self.generation.load(Ordering::SeqCst),
            epoch,
            phase: phase.into(),
            transport: transport.map(str::to_owned),
            peer_id: peer.map(str::to_owned),
            message,
        });
    }
    pub(crate) fn report(&self, e: &Error) {
        self.event("error", None, None, Some(e.to_string()));
    }
    pub(crate) async fn bounded_body(mut response: Response, max: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(b) = response.chunk().await? {
            if out.len() + b.len() > max {
                return Err(protocol("HTTP response too large"));
            }
            out.extend_from_slice(&b);
        }
        Ok(out)
    }
    pub(crate) fn directory_unknown(&self) {
        self.update_state(|s| {
            s.reachability =
                LayerState::new(StatePhase::Unknown, StateReason::DirectoryUnavailable, true)
        });
    }
    pub(crate) async fn reject_binding(&self) {
        self.remote_enabled.store(false, Ordering::SeqCst);
        self.hard_failure.store(true, Ordering::SeqCst);
        self.closed.store(true, Ordering::SeqCst);
        self.lifetime.cancel();
        if let Some(rt) = self.runtime.read().await.clone() {
            rt.cancel.cancel();
        }
        self.update_state(|s| {
            s.remote_enabled = false;
            s.identity = LayerState::new(
                StatePhase::RequiresLogin,
                StateReason::AuthorizationUnavailable,
                false,
            );
            s.publication =
                LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false);
            s.confirmed_intent_revision = None;
        });
    }
    async fn invalidate(&self, hard: bool) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if hard {
            self.hard_failure.store(true, Ordering::SeqCst);
        }
        if self.recovery_pending.swap(true, Ordering::SeqCst) {
            return;
        }
        self.update_state(|s| {
            s.coordination = LayerState::new(
                StatePhase::Unavailable,
                StateReason::CoordinationLost,
                !hard,
            );
            s.publication = if self.remote_access_enabled() {
                LayerState::new(StatePhase::Pending, StateReason::CoordinationLost, !hard)
            } else {
                LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false)
            };
            s.confirmed_intent_revision = None;
            s.server_publish_revision = None;
            s.reachability =
                LayerState::new(StatePhase::Unknown, StateReason::CoordinationLost, !hard);
        });
        if let Some(rt) = self.runtime.read().await.clone() {
            rt.cancel.cancel();
        }
        self.recovery.notify_one();
    }
    pub(crate) async fn http_json<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
        auth: bool,
        max: usize,
    ) -> Result<T> {
        let mut req = self.http.request(method.clone(), url);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let snapshot = if auth {
            self.session.read().await.clone()
        } else {
            None
        };
        if auth {
            let s = snapshot.as_ref().ok_or(Error::NotLoggedIn)?;
            if s.expires_at <= Utc::now() && method != Method::DELETE {
                self.invalidate(false).await;
                return Err(Error::Api {
                    status: 401,
                    code: "SESSION_EXPIRED".into(),
                });
            }
            req = req
                .bearer_auth(&s.session_key)
                .header("X-CyberGrid-Epoch", &s.coordinator_epoch);
        }
        let response = tokio::select! { _=self.lifetime.cancelled(),if auth&&method!=Method::DELETE=>return Err(Error::Cancelled), r=req.send()=>r? };
        let status = response.status().as_u16();
        if status == 429 || status == 503 {
            let retry = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1)
                .clamp(1, 60);
            self.retry_after.store(retry, Ordering::SeqCst);
        }
        let epoch = response
            .headers()
            .get("X-CyberGrid-Epoch")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let data = Self::bounded_body(response, max).await?;
        if let Some(s) = snapshot.as_ref() {
            // An old response must never invalidate a newer generation.
            if self
                .session
                .read()
                .await
                .as_ref()
                .is_none_or(|now| now.incarnation != s.incarnation)
            {
                return Err(Error::Cancelled);
            }
            if epoch.as_deref().is_some_and(|e| e != s.coordinator_epoch)
                && method != Method::DELETE
            {
                self.invalidate(false).await;
                return Err(Error::Api {
                    status: 409,
                    code: "COORDINATOR_RESET".into(),
                });
            }
        }
        if !(200..300).contains(&status) {
            let v: Option<Value> = serde_json::from_slice(&data).ok();
            let raw = v
                .as_ref()
                .and_then(|v| v.pointer("/error/code"))
                .and_then(Value::as_str)
                .unwrap_or("HTTP_ERROR");
            let code = if raw.len() <= 80
                && raw
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            {
                raw.to_owned()
            } else {
                "HTTP_ERROR".into()
            };
            if auth
                && method != Method::DELETE
                && (status == 401 || status == 403 || code == "COORDINATOR_RESET")
            {
                self.invalidate(status == 403 || code == "SESSION_SUPERSEDED")
                    .await;
            }
            return Err(Error::Api { status, code });
        }
        if auth && method != Method::DELETE && epoch.is_none() {
            return Err(protocol("missing coordinator epoch header"));
        }
        serde_json::from_slice(&data).map_err(|_| protocol("invalid API JSON response"))
    }
    pub(crate) async fn api<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        auth: bool,
    ) -> Result<T> {
        let u = Url::parse(&format!("{}/api/v1{path}", self.cfg.base_url))
            .map_err(|_| protocol("invalid API path"))?;
        self.http_json(method, u, body, auth, 2 << 20).await
    }
    pub async fn device_session(&self) -> Option<Session> {
        self.session.read().await.clone()
    }
    pub(crate) async fn active(&self) -> Result<Arc<Runtime>> {
        let r = self
            .runtime
            .read()
            .await
            .clone()
            .ok_or(Error::NotLoggedIn)?;
        if r.cancel.is_cancelled()
            || !r.ready.load(Ordering::SeqCst)
            || self.closed.load(Ordering::SeqCst)
        {
            return Err(Error::NotLoggedIn);
        }
        Ok(r)
    }
    /// First login using the host's in-memory grant owner; no CredentialStore required.
    pub async fn login_with_auth_provider(self: &Arc<Self>, name: &str) -> Result<Session> {
        self.login(None, name).await
    }
    pub async fn login_with_access_token(
        self: &Arc<Self>,
        token: &str,
        name: &str,
    ) -> Result<Session> {
        self.login(Some(token), name).await
    }
    async fn login(self: &Arc<Self>, token: Option<&str>, name: &str) -> Result<Session> {
        let _mutation = self.mutation.lock().await;
        if self.is_closed() {
            return Err(protocol("client already logged in or closed"));
        }
        if let Some(session) = self.session.read().await.clone() {
            // A prior verified owner may explicitly replace an authoritatively
            // expired unknown operation. This is distinct from acknowledging a
            // recovered initial publication: never return the old session here.
            let explicit_expired = self
                .pending_registration
                .lock()
                .await
                .as_ref()
                .is_some_and(|attempt| attempt.unknown && attempt.expired)
                && self.original_user.read().await.is_some();
            if !self.initial_login_recovery.load(Ordering::SeqCst) && !explicit_expired {
                return Err(protocol("client already logged in or closed"));
            }
            if !explicit_expired && self.active().await.is_ok() {
                // Background recovery completed the known initial registration.
                self.initial_login_recovery.store(false, Ordering::SeqCst);
                return Ok(session);
            }
            // This is an already committed registration, not a fresh login.
            // Preserve its subject/key and retire its uncertain publication with
            // a higher registration revision before generating another cert.
            self.stop_runtime().await;
        }
        // An uncertain attempt retains its original name as well as token/id.
        if self.original_user.read().await.is_none()
            && self.pending_registration.lock().await.is_none()
        {
            *self.name.write().await = name.to_owned();
        }
        self.update_state(|s| {
            s.identity = LayerState::new(
                StatePhase::Authorizing,
                StateReason::AuthorizationRequested,
                false,
            )
        });
        let s = match self
            .exchange_pending(token, AuthRequestReason::Connect)
            .await
        {
            Ok(session) => session,
            Err(error) => {
                if matches!(error, Error::AuthorizationUnavailable) {
                    self.update_state(|s| {
                        s.identity = LayerState::new(
                            StatePhase::RequiresLogin,
                            StateReason::AuthorizationUnavailable,
                            false,
                        )
                    });
                }
                return Err(error);
            }
        };
        *self.original_user.write().await = Some(s.user.id.clone());
        self.spawn_recovery();
        match self.install(s.clone()).await {
            Ok(()) => {
                self.initial_login_recovery.store(false, Ordering::SeqCst);
                Ok(s)
            }
            Err(error) => {
                self.initial_login_recovery.store(true, Ordering::SeqCst);
                self.invalidate(error.fatal()).await;
                Err(error)
            }
        }
    }
    async fn provider_token(
        &self,
        config: &AuthConfig,
        reason: AuthRequestReason,
        force_refresh: bool,
    ) -> Result<String> {
        let provider = self
            .provider
            .read()
            .await
            .clone()
            .ok_or(Error::AuthorizationUnavailable)?;
        let request = AuthTokenRequest {
            issuer: config.issuer.clone(),
            client_id: config.client_id.clone(),
            resource: config.resource.clone(),
            scopes: config.scope.split_whitespace().map(str::to_owned).collect(),
            min_validity: self.cfg.auth_min_validity,
            force_refresh,
            reason,
            generation: self.generation(),
            cancel: self.lifetime.child_token(),
        };
        // The host owns rotation/checkpoint: never select cancellation against
        // its future after dispatch and thereby discard a minted grant.
        let result = provider
            .access_token_for(self, &request)
            .await
            .map_err(|_| Error::AuthorizationUnavailable);
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        result.and_then(|token| {
            if token.trim().is_empty() {
                Err(Error::AuthorizationUnavailable)
            } else {
                Ok(token)
            }
        })
    }
    async fn admission(&self, attempt: &mut RegistrationAttempt) -> Result<()> {
        let Some(op) = attempt.admission.as_mut() else {
            return Ok(());
        };
        let epoch = &Self::coordination_config(&attempt.config)?.epoch;
        // Responses are private: never Debug/Serialize admission material.
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Challenge {
            challenge: String,
            epoch: String,
            expires_at: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Ticket {
            admission_ticket: String,
            epoch: String,
            expires_at: String,
        }
        fn opaque(v: &str) -> bool {
            use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
            let parts = v.split('.').collect::<Vec<_>>();
            v.len() <= 4096
                && parts.len() == 2
                && parts.iter().enumerate().all(|(i, raw)| {
                    !raw.is_empty()
                        && URL_SAFE_NO_PAD.decode(raw).is_ok_and(|bytes| {
                            (i == 0 || bytes.len() == 32) && URL_SAFE_NO_PAD.encode(bytes) == *raw
                        })
                })
        }
        if op.ticket.is_some() {
            return Ok(());
        }
        if op.challenge.is_none() {
            let c:Challenge=self.api(Method::POST,"/auth/registration/challenge",Some(json!({"appId":self.cfg.app_id,"registrationKey":self.registration_key,"accessToken":attempt.token})),false).await?;
            if c.epoch != *epoch
                || !opaque(&c.challenge)
                || chrono::DateTime::parse_from_rfc3339(&c.expires_at).is_err()
            {
                return Err(protocol("invalid admission challenge"));
            }
            op.challenge = Some(c.challenge);
        }
        // Mark before dispatch: dropping a caller future must retain uncertainty.
        let was_unknown = op.mint_unknown || attempt.unknown;
        op.mint_unknown = true;
        attempt.unknown = true;
        let result=self.api::<Ticket>(Method::POST,"/auth/registration/admit",Some(json!({"appId":self.cfg.app_id,"registrationKey":self.registration_key,"accessToken":attempt.token,"challenge":op.challenge,"requestId":op.request})),false).await;
        let t = match result {
            Ok(t) => t,
            Err(e) => {
                if !was_unknown && !Self::uncertain_error(&e) {
                    op.mint_unknown = false;
                    attempt.unknown = false;
                }
                return Err(e);
            }
        };
        let expiry = chrono::DateTime::parse_from_rfc3339(&t.expires_at)
            .map_err(|_| protocol("invalid admission ticket expiry"))?
            .with_timezone(&Utc);
        if t.epoch != *epoch || !opaque(&t.admission_ticket) {
            return Err(protocol("invalid admission ticket"));
        }
        op.ticket = Some(t.admission_ticket);
        op.expires = Some(expiry);
        op.mint_unknown = false;
        attempt.unknown = false;
        Ok(())
    }
    async fn exchange_pending(
        &self,
        token: Option<&str>,
        reason: AuthRequestReason,
    ) -> Result<Session> {
        let mut pending = self.pending_registration.lock().await;
        let mut replacement_config = None;
        let mut fresh = false;
        if let Some(attempt) = pending.as_ref().filter(|a| a.unknown) {
            if let Ok(current) = self.auth_config().await {
                if Self::coordination_config(&current)?.epoch
                    != Self::coordination_config(&attempt.config)?.epoch
                {
                    replacement_config = Some(current);
                    *pending = None;
                    self.retired_unknown.store(false, Ordering::SeqCst);
                }
            }
        }
        if pending.as_ref().is_some_and(|a| a.expired) && reason == AuthRequestReason::Connect {
            // The initial unknown operation has never established a trusted
            // subject. Opaque tickets/JWT payloads cannot fill that gap. A host
            // must retain this pending close fact and explicitly start a new
            // Client after checking its official token owner.
            if self.original_user.read().await.is_none() {
                return Err(Error::ExchangeOutcomeUnknown);
            }
            fresh = true;
        }
        if !fresh && pending.as_ref().is_some_and(|a| a.expired) {
            return Err(Error::ExchangeOutcomeUnknown);
        }
        if pending.is_none() || fresh {
            let revision = self.registration_revision.fetch_add(1, Ordering::SeqCst) + 1;
            if revision > crate::endpoints::MAX_SAFE {
                return Err(protocol("registration revision exhausted"));
            }
            let config = match replacement_config {
                Some(c) => c,
                None => self.auth_config().await?,
            };
            Self::coordination_config(&config)?;
            let natural_expiry = self
                .session
                .read()
                .await
                .as_ref()
                .is_some_and(|s| s.expires_at <= Utc::now())
                && config.registration_admission.is_some();
            let value = match token {
                Some(t) => t.to_owned(),
                None => {
                    self.provider_token(&config, reason, fresh || natural_expiry)
                        .await?
                }
            };
            if value.trim().is_empty() {
                return Err(Error::AuthorizationUnavailable);
            }
            if fresh {
                self.retired_unknown.store(true, Ordering::SeqCst);
            }
            let admission = config
                .registration_admission
                .as_ref()
                .map(|_| AdmissionOperation {
                    challenge: None,
                    request: uuid::Uuid::new_v4().to_string(),
                    ticket: None,
                    expires: None,
                    mint_unknown: false,
                });
            *pending = Some(RegistrationAttempt {
                token: value,
                config,
                revision,
                request: uuid::Uuid::new_v4().to_string(),
                unknown: false,
                force_refreshed: fresh || natural_expiry,
                admission,
                expired: false,
            });
        }
        loop {
            let attempt = pending.as_mut().expect("registration attempt exists");
            let previous_unknown = attempt.unknown;
            let admission_result = self.admission(attempt).await;
            let result = match admission_result {
                Err(e) => Err(e),
                Ok(()) => {
                    let prior = attempt.unknown;
                    attempt.unknown = true;
                    let r = self
                        .register_config(
                            &attempt.token,
                            &attempt.request,
                            attempt.revision,
                            &attempt.config,
                            attempt.admission.as_ref().and_then(|a| a.ticket.as_deref()),
                        )
                        .await;
                    if r.is_err()
                        && !prior
                        && !previous_unknown
                        && !Self::uncertain_error(r.as_ref().expect_err("error"))
                    {
                        attempt.unknown = false;
                    }
                    r
                }
            };
            match result {
                Ok(session) => {
                    *pending = None;
                    return Ok(session);
                }
                Err(Error::Api {
                    status: 401,
                    ref code,
                }) if code == "INVALID_IDENTITY"
                    && !previous_unknown
                    && !attempt.admission.as_ref().is_some_and(|a| a.mint_unknown)
                    && !attempt.force_refreshed
                    && self.provider.read().await.is_some() =>
                {
                    attempt.unknown = false;
                    attempt.force_refreshed = true;
                    match self
                        .provider_token(
                            &attempt.config,
                            AuthRequestReason::AuthorizationRejected,
                            true,
                        )
                        .await
                    {
                        Ok(t) => attempt.token = t,
                        Err(e) => {
                            *pending = None;
                            return Err(e);
                        }
                    }
                }
                Err(error) => {
                    if matches!(&error,Error::Api{code,..} if code=="REGISTRATION_ADMISSION_EXPIRED")
                        && attempt.unknown
                    {
                        attempt.expired = true;
                    }
                    if Self::uncertain_error(&error) {
                        attempt.unknown = true;
                    }
                    if attempt.unknown {
                        self.update_state(|s| {
                            s.coordination = LayerState::new(
                                StatePhase::Unavailable,
                                if attempt.expired {
                                    StateReason::AdmissionExpired
                                } else if attempt.admission.as_ref().is_some_and(|a| a.mint_unknown)
                                {
                                    StateReason::AdmissionOutcomeUnknown
                                } else {
                                    StateReason::ExchangeOutcomeUnknown
                                },
                                !attempt.expired,
                            )
                        });
                        return Err(Error::ExchangeOutcomeUnknown);
                    }
                    *pending = None;
                    self.update_state(|s| {
                        s.identity = LayerState::new(
                            StatePhase::RequiresLogin,
                            StateReason::AuthorizationUnavailable,
                            false,
                        )
                    });
                    return Err(error);
                }
            }
        }
    }
    fn uncertain_error(e: &Error) -> bool {
        matches!(
            e,
            Error::Http(_) | Error::Io(_) | Error::Timeout(_) | Error::Protocol(_)
        ) || matches!(
            e,
            Error::Api {
                status: 500..=599,
                ..
            }
        )
    }
    #[cfg(test)]
    async fn register(&self, token: &str, request: &str, revision: u64) -> Result<Session> {
        if revision == 0 || revision > 9_007_199_254_740_991 {
            return Err(protocol("registration revision exhausted"));
        }
        let config = self.auth_config().await?;
        self.register_config(token, request, revision, &config, None)
            .await
    }
    fn coordination_config(config: &AuthConfig) -> Result<&crate::Coordination> {
        let coordination = config
            .coordination
            .as_ref()
            .ok_or_else(|| protocol("server does not support memory-v1"))?;
        if coordination.protocol != "memory-v1"
            || coordination.epoch.is_empty()
            || coordination.max_signal_wait_seconds != 15
            || coordination.lease_seconds != 65
            || coordination.heartbeat_seconds != 20
        {
            return Err(protocol("unsupported coordination protocol/budgets"));
        }
        Ok(coordination)
    }
    async fn register_config(
        &self,
        token: &str,
        request: &str,
        revision: u64,
        config: &AuthConfig,
        admission_ticket: Option<&str>,
    ) -> Result<Session> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        if revision == 0 || revision > 9_007_199_254_740_991 {
            return Err(protocol("registration revision exhausted"));
        }
        if token.trim().is_empty() {
            return Err(protocol("resource access token required"));
        }
        let coordination = Self::coordination_config(config)?;
        let mut body = json!({"appId":self.cfg.app_id,"deviceName":*self.name.read().await,"accessToken":token,"coordinationProtocol":"memory-v1","coordinatorEpoch":coordination.epoch,"registrationKey":self.registration_key,"requestId":request,"registrationRevision":revision});
        if let Some(ticket) = admission_ticket {
            body["admissionTicket"] = json!(ticket);
        }
        let s: Session = self
            .api(Method::POST, "/auth/exchange", Some(body), false)
            .await?;
        if s.purpose != "device"
            || s.app_id != self.cfg.app_id
            || s.product_id.as_deref().unwrap_or(&s.app_id)
                != config.product_id.as_deref().unwrap_or(&self.cfg.app_id)
            || s.session_key.is_empty()
            || s.device_id.is_empty()
            || s.user.id.is_empty()
            || s.expires_at <= Utc::now()
            || s.coordinator_epoch != coordination.epoch
            || uuid::Uuid::parse_str(&s.incarnation).is_err()
        {
            return Err(protocol(
                "device exchange purpose/appId/generation/identity mismatch",
            ));
        }
        let wrong_user = self
            .original_user
            .read()
            .await
            .as_ref()
            .is_some_and(|id| id != &s.user.id);
        if wrong_user || self.closed.load(Ordering::SeqCst) {
            // Exchange may already have committed when Close won. Dispose only
            // this returned incarnation, never the current logical generation.
            if !self.revoke_exact_session(&s).await {
                // Keep the exact private incarnation so close can retry instead
                // of claiming an unacknowledged best-effort DELETE succeeded.
                *self.retired_session.lock().await = Some(s.clone());
            }
            if wrong_user {
                return Err(Error::IdentityChanged);
            }
            return Err(Error::Cancelled);
        }
        Ok(s)
    }
    async fn install(self: &Arc<Self>, s: Session) -> Result<()> {
        // Exchange has committed. Keep its revocation owner even when local
        // listener/certificate setup or the first publication fails.
        *self.session.write().await = Some(s.clone());
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        {
            let mut incarnation = self.bridge_incarnation.lock().await;
            if incarnation.as_deref() != Some(&s.incarnation) {
                let mut history = self.bridge_history.lock().await;
                *history = self.bridge_intent.read().await.clone();
                *incarnation = Some(s.incarnation.clone());
            }
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        // iroh is an optional direct path. Unsupported interface constraints or
        // local UDP failures must not prevent the existing paths from working.
        let identity = Identity::new(&s.device_id)?;
        let iroh = if self.cfg.iroh_enabled && self.cfg.interface_name.is_none() {
            match tokio::time::timeout(
                self.cfg.iroh_timeout,
                crate::iroh_direct::DirectEndpoint::bind(),
            )
            .await
            {
                Ok(Ok(endpoint)) => Some(endpoint),
                _ => {
                    self.event("iroh-skipped", Some("iroh-direct"), None, None);
                    None
                }
            }
        } else {
            None
        };
        let runtime = Arc::new(Runtime {
            iroh,
            identity,
            cancel: self.lifetime.child_token(),
            ports: RwLock::new(self.ports.read().await.clone()),
            tunnel_url: RwLock::new(None),
            origin_addr: listener.local_addr()?,
            last_heartbeat: Mutex::new(Instant::now()),
            peers: Mutex::new(Vec::new()),
            attempts: Mutex::new(HashMap::new()),
            generation,
            ready: AtomicBool::new(false),
            provider_task: Mutex::new(None),
        });
        {
            let mut publication = self.publication.lock().await;
            *self.session.write().await = Some(s.clone());
            *self.runtime.write().await = Some(runtime.clone());
            *publication = 0;
        }
        if let Err(e) = self.heartbeat().await {
            runtime.cancel.cancel();
            if let Some(endpoint) = &runtime.iroh {
                endpoint.close().await;
            }
            return Err(e);
        }
        if self.closed.load(Ordering::SeqCst) {
            runtime.cancel.cancel();
            if let Some(endpoint) = &runtime.iroh {
                endpoint.close().await;
            }
            return Err(Error::Cancelled);
        }
        runtime.ready.store(true, Ordering::SeqCst);
        self.spawn_iroh(runtime.clone()).await;
        self.spawn_origin(listener, runtime.clone());
        self.spawn_background(runtime.clone(), s.expires_at);
        if let Some(p) = self.cfg.cloudflared.clone() {
            let task = self.spawn_provider(p, runtime.clone());
            *runtime.provider_task.lock().await = Some(task);
        }
        self.recovery_pending.store(false, Ordering::SeqCst);
        self.event("coordination-ready", None, None, None);
        Ok(())
    }
    fn spawn_recovery(self: &Arc<Self>) {
        if self.recovery_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(c) = weak.upgrade() else { break };
                tokio::select! {_=c.lifetime.cancelled()=>break,_=c.recovery.notified()=>{}}
                if c.closed.load(Ordering::SeqCst) {
                    break;
                }
                let _mutation = c.mutation.lock().await;
                if c.is_closed() {
                    break;
                }
                // An explicit initial-login retry can finish while a queued
                // wakeup waits for mutation. It must not cancel that new runtime.
                if !c.recovery_pending.load(Ordering::SeqCst) {
                    continue;
                }
                c.stop_runtime().await;
                if c.hard_failure.swap(false, Ordering::SeqCst) {
                    c.event(
                        "requires-login",
                        None,
                        None,
                        Some("identity/application denied or session superseded".into()),
                    );
                    c.recovery_pending.store(false, Ordering::SeqCst);
                    continue;
                }
                if c.provider.read().await.is_none() {
                    c.event("requires-login", None, None, None);
                    c.recovery_pending.store(false, Ordering::SeqCst);
                    continue;
                }
                c.event(
                    "recovering",
                    None,
                    None,
                    Some("coordination generation unavailable; old TCP streams closed".into()),
                );
                let mut failures = 0u32;
                loop {
                    if c.is_closed() {
                        break;
                    }
                    let result = async {
                        let s = c
                            .exchange_pending(None, AuthRequestReason::CoordinationRecovery)
                            .await?;
                        c.install(s).await
                    }
                    .await;
                    if matches!(&result, Err(Error::ExchangeOutcomeUnknown))
                        && c.pending_registration
                            .lock()
                            .await
                            .as_ref()
                            .is_some_and(|attempt| attempt.expired)
                    {
                        // No background fresh domain after an unknown result.
                        // Release mutation so an explicit host login can request
                        // fresh authorization for a previously verified subject.
                        // The private attempt and close-pending fact stay owned.
                        c.event("requires-login", None, None, None);
                        break;
                    }
                    match result {
                        Ok(()) => break,
                        Err(Error::Cancelled) => break,
                        Err(e)
                            if e.fatal()
                                || matches!(
                                    e,
                                    Error::AuthorizationUnavailable | Error::IdentityChanged
                                )
                                || matches!(&e,Error::Api{code,..} if code=="SESSION_SUPERSEDED")
                                || matches!(
                                    e,
                                    Error::OAuth {
                                        status: 400 | 401 | 403,
                                        ..
                                    } | Error::Protocol(_)
                                ) =>
                        {
                            c.event("requires-login", None, None, Some(e.to_string()));
                            break;
                        }
                        Err(e) => {
                            c.report(&e);
                            failures = failures.saturating_add(1);
                            let base = 1u64 << failures.min(5);
                            let jitter = (uuid::Uuid::new_v4().as_u128() % 500) as u64;
                            tokio::select! {_=c.lifetime.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(base.max(c.retry_after.swap(0,Ordering::SeqCst))*1000+jitter))=>{}}
                        }
                    }
                }
                c.recovery_pending.store(false, Ordering::SeqCst);
            }
        });
    }
    pub async fn apps(&self) -> Result<Vec<Application>> {
        self.active().await?;
        #[derive(Deserialize)]
        struct Directory {
            source: String,
            apps: Vec<Application>,
        }
        let out: Directory = self.api(Method::GET, "/apps", None, true).await?;
        let mut seen = std::collections::HashSet::new();
        if !matches!(out.source.as_str(), "logto" | "static") {
            return Err(protocol("invalid application directory source"));
        }
        for a in &out.apps {
            crate::types::validate_app_id(&a.id)?;
            if a.type_ != "Native" || a.is_device_flow || !seen.insert(&a.id) {
                return Err(protocol("invalid or duplicate Native application"));
            }
        }
        Ok(out.apps)
    }
    pub(crate) async fn device(&self, id: &str) -> Result<Device> {
        self.devices()
            .await?
            .into_iter()
            .find(|d| d.id == id && d.online && d.certificate_fingerprint.is_some())
            .ok_or_else(|| protocol("remote device not online in this account/application"))
    }

    pub(crate) fn intent_changed(&self) {
        self.intent_revision.fetch_add(1, Ordering::SeqCst);
        self.update_state(|s| {
            s.publication = if self.remote_access_enabled() {
                LayerState::new(StatePhase::Pending, StateReason::LocalIntentAccepted, true)
            } else {
                LayerState::new(
                    StatePhase::Disabled,
                    StateReason::LocalIntentAccepted,
                    false,
                )
            };
            s.confirmed_intent_revision = None;
        });
    }
    pub async fn expose(&self, port: Port) -> Result<()> {
        port.validate()?;
        if let Some(url) = &port.tunnel_url {
            self.validate_endpoint(url, false).await?;
        }
        {
            let mut ports = self.ports.write().await;
            if !ports.contains_key(&port.id) && ports.len() >= 32 {
                return Err(protocol("maximum 32 exposed ports"));
            }
            ports.insert(port.id.clone(), port);
            self.intent_changed();
        }
        if let Ok(rt) = self.active().await {
            *rt.ports.write().await = self.ports.read().await.clone();
            if self.remote_access_enabled() {
                self.heartbeat().await?;
            }
        }
        Ok(())
    }
    pub async fn unexpose(&self, id: &str) -> Result<()> {
        {
            let mut ports = self.ports.write().await;
            ports
                .remove(id)
                .ok_or_else(|| protocol("port is not exposed"))?;
            self.intent_changed();
        }
        if let Ok(rt) = self.active().await {
            *rt.ports.write().await = self.ports.read().await.clone();
            if self.remote_access_enabled() {
                self.heartbeat().await?;
            }
        }
        Ok(())
    }
    /// Advertise or withdraw a host-managed HTTP tunnel for this business port.
    /// This never starts a provider or substitutes the SDK's encrypted device tunnel.
    pub async fn update_port_tunnel_url(&self, id: &str, url: Option<&str>) -> Result<()> {
        if let Some(url) = url {
            self.validate_endpoint(url, false).await?;
        }
        {
            let mut ports = self.ports.write().await;
            let port = ports
                .get_mut(id)
                .ok_or_else(|| protocol("port is not exposed"))?;
            port.tunnel_url = url.map(str::to_owned);
            self.intent_changed();
        }
        if let Ok(rt) = self.active().await {
            *rt.ports.write().await = self.ports.read().await.clone();
            if self.remote_access_enabled() {
                self.heartbeat().await?;
            }
        }
        Ok(())
    }
    pub(crate) async fn heartbeat(&self) -> Result<()> {
        let mut revision = self.publication.lock().await;
        let rt = self
            .runtime
            .read()
            .await
            .clone()
            .ok_or(Error::NotLoggedIn)?;
        if rt.cancel.is_cancelled() || self.closed.load(Ordering::SeqCst) {
            return Err(Error::NotLoggedIn);
        }
        let s = self
            .session
            .read()
            .await
            .clone()
            .ok_or(Error::NotLoggedIn)?;
        let (mut ports, mut bridges, intent_revision) = {
            let ports = self.ports.read().await;
            let bridges = self.bridge_intent.read().await;
            (
                ports.values().cloned().collect::<Vec<_>>(),
                bridges
                    .values()
                    .filter(|b| b.expiry().is_ok_and(|e| e > Utc::now()))
                    .cloned()
                    .collect::<Vec<_>>(),
                self.intent_revision.load(Ordering::SeqCst),
            )
        };
        let enabled = self.remote_access_enabled();
        if !enabled {
            ports.clear();
            bridges.clear();
        }
        ports.sort_by(|a, b| a.id.cmp(&b.id));
        bridges.sort_by(|a, b| a.id.cmp(&b.id));
        for p in &ports {
            if let Some(url) = &p.tunnel_url {
                self.validate_endpoint(url, false).await?;
            }
        }
        if !bridges.is_empty()
            && self
                .auth
                .lock()
                .await
                .as_ref()
                .is_none_or(|a| a.endpoint_policy.is_none())
        {
            return Err(protocol(
                "bridge publication requires advertised endpoint policy",
            ));
        }
        for b in &bridges {
            self.validate_bridge(b).await?;
        }
        *revision += 1;
        if *revision > 9_007_199_254_740_991 {
            return Err(protocol("publication revision exhausted"));
        }
        let iroh = if enabled {
            rt.iroh.as_ref().map(|endpoint| endpoint.info())
        } else {
            None
        };
        let tunnel = if enabled {
            self.device_tunnel_intent
                .read()
                .await
                .clone()
                .or(rt.tunnel_url.read().await.clone())
        } else {
            None
        };
        if let Some(url) = &tunnel {
            self.validate_endpoint(url, true).await?;
        }
        let body = json!({"bridges":bridges,"ports":ports,"certificateFingerprint":rt.identity.fingerprint,"tunnelUrl":tunnel,"iroh":iroh,"incarnation":s.incarnation,"publishRevision":*revision});
        let metadata = json!({"ports":body["ports"],"bridges":body["bridges"],"tunnelUrl":body["tunnelUrl"],"certificateFingerprint":body["certificateFingerprint"],"iroh":body["iroh"]});
        if serde_json::to_vec(&metadata)
            .map_err(|_| protocol("invalid publication"))?
            .len()
            > 24 * 1024
        {
            return Err(protocol("publication exceeds 24KiB"));
        }
        let _: Value = self
            .api(Method::PUT, "/devices/self", Some(body), true)
            .await?;
        if self.generation.load(Ordering::SeqCst) != rt.generation || rt.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        *rt.last_heartbeat.lock().await = Instant::now();
        self.update_state(|state| {
            state.epoch = Some(s.coordinator_epoch.clone());
            state.server_publish_revision = Some(*revision);
            if enabled
                && self.remote_access_enabled()
                && self.intent_revision.load(Ordering::SeqCst) == intent_revision
            {
                state.confirmed_intent_revision = Some(intent_revision);
                state.publication =
                    LayerState::new(StatePhase::Ready, StateReason::PublicationConfirmed, false);
            }
        });
        Ok(())
    }
    pub(crate) async fn send_signal(
        &self,
        to: &str,
        id: &str,
        kind: &str,
        payload: Value,
    ) -> Result<()> {
        let target = self.device(to).await?;
        let rt = self.active().await?;
        let text = serde_json::to_string(&payload).map_err(|_| protocol("cannot encode signal"))?;
        let body = json!({"toDeviceId":to,"toIncarnation":target.incarnation,"clientMessageId":uuid::Uuid::new_v4().to_string(),"connectionId":id,"kind":kind,"payload":text});
        for attempt in 0..3 {
            let result = tokio::select! {_=rt.cancel.cancelled()=>return Err(Error::Cancelled),r=self.api::<Value>(Method::POST,"/signals",Some(body.clone()),true)=>r};
            match result {
                Ok(_) => return Ok(()),
                Err(e) if attempt < 2 && !e.fatal() => {
                    tokio::select! {_=rt.cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(250*(attempt+1)))=>{}}
                }
                Err(e) => return Err(e),
            }
        }
        Err(Error::Cancelled)
    }
    fn spawn_background(self: &Arc<Self>, rt: Arc<Runtime>, expiry: chrono::DateTime<Utc>) {
        let c = Arc::downgrade(self);
        let r = rt.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(20));
            tick.tick().await;
            loop {
                tokio::select! {_=r.cancel.cancelled()=>break,_=tick.tick()=>{let Some(c)=c.upgrade()else{break};if c.generation()!=r.generation||r.cancel.is_cancelled(){break;}
                if let Err(e)=c.heartbeat().await{c.report(&e);}}}
            }
        });
        let c = Arc::downgrade(self);
        let r = rt.clone();
        tokio::spawn(async move {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Signals {
                signals: Vec<Signal>,
                cursor: u64,
                coordinator_epoch: String,
            }
            let mut cursor = 0;
            let mut failures = 0u32;
            loop {
                let Some(c) = c.upgrade() else { break };
                let path = format!("/signals?after={cursor}&wait=15");
                let response = tokio::select! {_=r.cancel.cancelled()=>break,r=c.api::<Signals>(Method::GET,&path,None,true)=>r};
                match response {
                    Ok(out) => {
                        failures = 0;
                        let Some(session) = c.device_session().await else {
                            break;
                        };
                        if out.coordinator_epoch != session.coordinator_epoch {
                            c.invalidate(false).await;
                            break;
                        }
                        if out.cursor < cursor {
                            c.report(&protocol("signaling cursor moved backwards"));
                            continue;
                        }
                        let mut blocked = false;
                        for signal in out.signals {
                            let id = signal.id;
                            if signal.coordinator_epoch != session.coordinator_epoch
                                || signal.to_incarnation != session.incarnation
                            {
                                cursor = cursor.max(id);
                                continue;
                            }
                            let result = async {
                                let from = c.device(&signal.from_device_id).await?;
                                if from.incarnation != signal.from_incarnation {
                                    return Ok(());
                                }
                                c.handle_signal(signal).await
                            }
                            .await;
                            match result {
                                Ok(()) => cursor = cursor.max(id),
                                Err(e) if matches!(e, Error::Protocol(_) | Error::Transport(_)) => {
                                    c.report(&e);
                                    cursor = cursor.max(id);
                                }
                                Err(e) => {
                                    c.report(&e);
                                    blocked = true;
                                    break;
                                }
                            }
                        }
                        if !blocked {
                            cursor = out.cursor;
                        }
                        tokio::select! {_=r.cancel.cancelled()=>break,_=tokio::time::sleep(c.cfg.poll_interval)=>{}}
                    }
                    Err(e) => {
                        c.report(&e);
                        failures = failures.saturating_add(1);
                        let wait = Duration::from_millis(
                            (250u64 << failures.min(6))
                                .min(15000)
                                .max(c.retry_after.swap(0, Ordering::SeqCst) * 1000),
                        );
                        tokio::select! {_=r.cancel.cancelled()=>break,_=tokio::time::sleep(wait)=>{}}
                    }
                }
            }
        });
        let c = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {_=rt.cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(250))=>{if Utc::now()>=expiry||rt.last_heartbeat.lock().await.elapsed()>Duration::from_secs(65){if let Some(c)=c.upgrade(){if c.generation.load(Ordering::SeqCst)==rt.generation{c.invalidate(false).await;}}break;}}}
            }
            for p in rt.peers.lock().await.clone() {
                p.close().await;
                if let Some(c) = c.upgrade() {
                    if c.generation() == rt.generation {
                        c.event("mapping-disconnected", None, Some(&p.device_id), None);
                    }
                }
            }
            let attempts: Vec<_> = rt.attempts.lock().await.values().cloned().collect();
            for a in attempts {
                a.close().await;
            }
        });
    }
    async fn stop_runtime(&self) {
        if let Some(rt) = self.runtime.read().await.clone() {
            rt.cancel.cancel();
            if let Some(endpoint) = &rt.iroh {
                endpoint.close().await;
            }
            if let Some(task) = rt.provider_task.lock().await.take() {
                let _ = task.await;
            }
            for p in rt.peers.lock().await.clone() {
                p.close().await;
                self.event("mapping-disconnected", None, Some(&p.device_id), None);
            }
            let attempts: Vec<_> = rt.attempts.lock().await.values().cloned().collect();
            for a in attempts {
                a.close().await;
            }
        }
    }
    async fn revoke_exact_session(&self, session: &Session) -> bool {
        self.http
            .delete(format!("{}/api/v1/session", self.cfg.base_url))
            .bearer_auth(&session.session_key)
            .header("X-CyberGrid-Epoch", &session.coordinator_epoch)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }
    async fn delete_session(&self) -> Result<()> {
        if self.session.read().await.is_some() {
            let _: Value = self.api(Method::DELETE, "/session", None, true).await?;
        }
        Ok(())
    }
    /// Explicitly publish saved port intent and permit inbound business streams.
    pub async fn enable_remote(&self) -> Result<()> {
        self.active().await?;
        if self.is_closed() {
            return Err(Error::Cancelled);
        }
        self.remote_enabled.store(true, Ordering::SeqCst);
        self.intent_changed();
        self.heartbeat().await
    }
    /// This retires this Client, while the host's OAuth owner and local service survive.
    /// The local gate closes before the first await; cloud failure cannot reopen it.
    pub async fn disable_remote(&self) -> RemoteDisableOutcome {
        let result = self.close().await;
        RemoteDisableOutcome {
            local_closed: true,
            coordinator_revocation: if result.is_ok() {
                CoordinatorRevocation::Confirmed
            } else {
                CoordinatorRevocation::Pending
            },
        }
    }
    pub async fn close(&self) -> Result<()> {
        self.remote_enabled.store(false, Ordering::SeqCst);
        self.closed.store(true, Ordering::SeqCst);
        self.lifetime.cancel();
        self.update_state(|s| {
            s.publication =
                LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false);
            s.confirmed_intent_revision = None;
        });
        self.stop_runtime().await;
        // Do not cancel a refresh response after dispatch. Wait for its owner to
        // checkpoint, then prevent the retired generation from entering Ready.
        let _mutation = self.mutation.lock().await;
        let retired_revoked = {
            let mut retired = self.retired_session.lock().await;
            if let Some(session) = retired.as_ref() {
                if self.revoke_exact_session(session).await {
                    *retired = None;
                    true
                } else {
                    false
                }
            } else {
                true
            }
        };
        let result = self.delete_session().await;
        if result.is_ok() {
            *self.session.write().await = None;
        }
        self.event("closed", None, None, None);
        // A session from a lost exchange response may still exist. Deleting a
        // previous known session does not confirm that uncertain incarnation.
        if self.retired_unknown.load(Ordering::SeqCst)
            || self
                .pending_registration
                .lock()
                .await
                .as_ref()
                .is_some_and(|a| a.unknown)
        {
            return Err(Error::ExchangeOutcomeUnknown);
        }
        if !retired_revoked {
            return Err(Error::RevocationPending);
        }
        result
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;

    #[tokio::test]
    async fn registration_exhaustion_stops_before_network() {
        let client = Client::new(Config {
            base_url: "http://127.0.0.1:1".into(),
            app_id: "native-boundary".into(),
            ..Config::default()
        })
        .unwrap();
        client
            .registration_revision
            .store(9_007_199_254_740_991, Ordering::SeqCst);
        let error = client
            .login_with_access_token("local-fixture-token", "boundary")
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Protocol(message) if message == "registration revision exhausted")
        );
        assert!(client.session.read().await.is_none());
        assert!(matches!(
            client.register("local-fixture-token", "unused", 0).await,
            Err(Error::Protocol(_))
        ));
    }
}

#[cfg(test)]
#[path = "capability_tests.rs"]
mod capability_tests;
