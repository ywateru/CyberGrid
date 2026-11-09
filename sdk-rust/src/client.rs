use crate::{
    error::protocol, rtc::Attempt, secure::Identity, types::Signal, Application, AuthConfig,
    Config, CredentialStore, Device, Error, Event, OAuthTokens, Peer, Port, Result, Session,
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
    sync::{broadcast, Mutex, Notify, RwLock},
};
use tokio_util::sync::CancellationToken;
use url::Url;

pub type AuthFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
/// Explicit recovery credential source. Implementations must retain every newly
/// rotated grant privately on checkpoint failure; never prompt for interactive login.
pub trait AuthProvider: Send + Sync {
    fn access_token<'a>(&'a self, client: &'a Client) -> AuthFuture<'a>;
}

pub(crate) struct Runtime {
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
    pub(crate) auth: Mutex<Option<AuthConfig>>,
    pub(crate) nonce: String,
    pub(crate) session: RwLock<Option<Session>>,
    pub(crate) runtime: RwLock<Option<Arc<Runtime>>>,
    events: broadcast::Sender<Event>,
    mutation: Mutex<()>,
    publication: Mutex<u64>,
    pub(crate) ports: RwLock<HashMap<String, Port>>,
    closed: AtomicBool,
    lifetime: CancellationToken,
    generation: AtomicU64,
    registration_revision: AtomicU64,
    registration_key: String,
    recovery: Notify,
    hard_failure: AtomicBool,
    recovery_pending: AtomicBool,
    retry_after: AtomicU64,
    provider: RwLock<Option<Arc<dyn AuthProvider>>>,
    name: RwLock<String>,
    original_user: RwLock<Option<String>>,
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
            auth: Mutex::new(None),
            nonce: uuid::Uuid::new_v4().to_string(),
            session: RwLock::new(None),
            runtime: RwLock::new(None),
            events,
            mutation: Mutex::new(()),
            publication: Mutex::new(0),
            ports: RwLock::new(HashMap::new()),
            closed: AtomicBool::new(false),
            lifetime: CancellationToken::new(),
            generation: AtomicU64::new(0),
            registration_revision: AtomicU64::new(0),
            registration_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key),
            recovery: Notify::new(),
            hard_failure: AtomicBool::new(false),
            recovery_pending: AtomicBool::new(false),
            retry_after: AtomicU64::new(0),
            provider: RwLock::new(None),
            name: RwLock::new(String::new()),
            original_user: RwLock::new(None),
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
    pub async fn login_with_access_token(
        self: &Arc<Self>,
        token: &str,
        name: &str,
    ) -> Result<Session> {
        let _mutation = self.mutation.lock().await;
        if self.closed.load(Ordering::SeqCst) || self.session.read().await.is_some() {
            return Err(protocol("client already logged in or closed"));
        }
        *self.name.write().await = name.to_owned();
        let request = uuid::Uuid::new_v4().to_string();
        let revision = self.registration_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let s = self.register(token, &request, revision).await?;
        *self.original_user.write().await = Some(s.user.id.clone());
        self.install(s.clone()).await?;
        self.spawn_recovery();
        Ok(s)
    }
    async fn register(&self, token: &str, request: &str, revision: u64) -> Result<Session> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        if revision == 0 || revision > 9_007_199_254_740_991 {
            return Err(protocol("registration revision exhausted"));
        }
        if token.trim().is_empty() {
            return Err(protocol("resource access token required"));
        }
        let config = self.auth_config().await?;
        let coordination = config
            .coordination
            .ok_or_else(|| protocol("server does not support memory-v1"))?;
        if coordination.protocol != "memory-v1"
            || coordination.epoch.is_empty()
            || coordination.max_signal_wait_seconds != 15
            || coordination.lease_seconds != 65
            || coordination.heartbeat_seconds != 20
        {
            return Err(protocol("unsupported coordination protocol/budgets"));
        }
        let body = json!({"appId":self.cfg.app_id,"deviceName":*self.name.read().await,"accessToken":token,"coordinationProtocol":"memory-v1","coordinatorEpoch":coordination.epoch,"registrationKey":self.registration_key,"requestId":request,"registrationRevision":revision});
        let mut tries = 0;
        let s: Session = loop {
            match self
                .api(Method::POST, "/auth/exchange", Some(body.clone()), false)
                .await
            {
                Ok(s) => break s,
                Err(e) if !e.fatal() && tries < 2 => {
                    tries += 1;
                    tokio::select! {_=self.lifetime.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(250*tries))=>{}}
                }
                Err(e) => return Err(e),
            }
        };
        if s.purpose != "device"
            || s.app_id != self.cfg.app_id
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
            let _ = self
                .http
                .delete(format!("{}/api/v1/session", self.cfg.base_url))
                .bearer_auth(&s.session_key)
                .header("X-CyberGrid-Epoch", &s.coordinator_epoch)
                .send()
                .await;
            if wrong_user {
                return Err(protocol("recovery identity changed"));
            }
            return Err(Error::Cancelled);
        }
        Ok(s)
    }
    async fn install(self: &Arc<Self>, s: Session) -> Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let runtime = Arc::new(Runtime {
            identity: Identity::new(&s.device_id)?,
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
            return Err(e);
        }
        if self.closed.load(Ordering::SeqCst) {
            runtime.cancel.cancel();
            return Err(Error::Cancelled);
        }
        runtime.ready.store(true, Ordering::SeqCst);
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
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(c) = weak.upgrade() else { break };
                tokio::select! {_=c.lifetime.cancelled()=>break,_=c.recovery.notified()=>{}}
                if c.closed.load(Ordering::SeqCst) {
                    break;
                }
                c.stop_runtime().await;
                if c.hard_failure.swap(false, Ordering::SeqCst) {
                    c.event(
                        "requires-login",
                        None,
                        None,
                        Some("identity/application denied or session superseded".into()),
                    );
                    break;
                }
                let Some(provider) = c.provider.read().await.clone() else {
                    c.event(
                        "requires-login",
                        None,
                        None,
                        Some("explicit recovery credentials required".into()),
                    );
                    break;
                };
                c.event(
                    "recovering",
                    None,
                    None,
                    Some("coordination generation unavailable; old TCP streams closed".into()),
                );
                let _mutation = c.mutation.lock().await;
                let mut token = None;
                let mut request = uuid::Uuid::new_v4().to_string();
                let mut revision = c.registration_revision.fetch_add(1, Ordering::SeqCst) + 1;
                let mut failures = 0u32;
                loop {
                    if c.closed.load(Ordering::SeqCst) {
                        break;
                    }
                    let result = async {
                        if token.is_none() {
                            token = Some(provider.access_token(&c).await?);
                        }
                        if c.closed.load(Ordering::SeqCst) {
                            return Err(Error::Cancelled);
                        }
                        let s = c
                            .register(token.as_deref().unwrap_or(""), &request, revision)
                            .await?;
                        match c.install(s).await {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                // A first snapshot may have committed although its
                                // response was lost. Retire that attempt explicitly:
                                // a new certificate must never reuse revision 1 in
                                // the same incarnation.
                                request = uuid::Uuid::new_v4().to_string();
                                revision =
                                    c.registration_revision.fetch_add(1, Ordering::SeqCst) + 1;
                                Err(error)
                            }
                        }
                    }
                    .await;
                    match result {
                        Ok(()) => break,
                        Err(Error::Cancelled) => break,
                        Err(Error::Api { ref code, .. })
                            if code == "REGISTRATION_ATTEMPT_EXPIRED" =>
                        {
                            request = uuid::Uuid::new_v4().to_string();
                            revision = c.registration_revision.fetch_add(1, Ordering::SeqCst) + 1;
                        }
                        Err(e)
                            if e.fatal()
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
        if out.source != "logto" {
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
    pub async fn devices(&self) -> Result<Vec<Device>> {
        self.active().await?;
        #[derive(Deserialize)]
        struct Devices {
            devices: Vec<Device>,
        }
        Ok(self
            .api::<Devices>(Method::GET, "/devices", None, true)
            .await?
            .devices)
    }
    pub(crate) async fn device(&self, id: &str) -> Result<Device> {
        self.devices()
            .await?
            .into_iter()
            .find(|d| d.id == id && d.online && d.certificate_fingerprint.is_some())
            .ok_or_else(|| protocol("remote device not online in this account/application"))
    }

    pub async fn expose(&self, port: Port) -> Result<()> {
        port.validate()?;
        {
            let mut ports = self.ports.write().await;
            if !ports.contains_key(&port.id) && ports.len() >= 32 {
                return Err(protocol("maximum 32 exposed ports"));
            }
            ports.insert(port.id.clone(), port);
        }
        if let Ok(rt) = self.active().await {
            *rt.ports.write().await = self.ports.read().await.clone();
            self.heartbeat().await?;
        }
        Ok(())
    }
    pub async fn unexpose(&self, id: &str) -> Result<()> {
        self.ports
            .write()
            .await
            .remove(id)
            .ok_or_else(|| protocol("port is not exposed"))?;
        if let Ok(rt) = self.active().await {
            *rt.ports.write().await = self.ports.read().await.clone();
            self.heartbeat().await?;
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
        let mut ports: Vec<_> = self.ports.read().await.values().cloned().collect();
        ports.sort_by(|a, b| a.id.cmp(&b.id));
        *revision += 1;
        if *revision > 9_007_199_254_740_991 {
            return Err(protocol("publication revision exhausted"));
        }
        let body = json!({"ports":ports,"certificateFingerprint":rt.identity.fingerprint,"tunnelUrl":*rt.tunnel_url.read().await,"incarnation":s.incarnation,"publishRevision":*revision});
        let _: Value = self
            .api(Method::PUT, "/devices/self", Some(body), true)
            .await?;
        if self.generation.load(Ordering::SeqCst) != rt.generation || rt.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        *rt.last_heartbeat.lock().await = Instant::now();
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
            for a in rt
                .attempts
                .lock()
                .await
                .values()
                .cloned()
                .collect::<Vec<_>>()
            {
                a.close().await;
            }
        });
    }
    async fn stop_runtime(&self) {
        if let Some(rt) = self.runtime.read().await.clone() {
            rt.cancel.cancel();
            if let Some(task) = rt.provider_task.lock().await.take() {
                let _ = task.await;
            }
            for p in rt.peers.lock().await.clone() {
                p.close().await;
                self.event("mapping-disconnected", None, Some(&p.device_id), None);
            }
            for a in rt
                .attempts
                .lock()
                .await
                .values()
                .cloned()
                .collect::<Vec<_>>()
            {
                a.close().await;
            }
        }
    }
    async fn delete_session(&self) -> Result<()> {
        if self.session.read().await.is_some() {
            let _: Value = self.api(Method::DELETE, "/session", None, true).await?;
        }
        Ok(())
    }
    pub async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.lifetime.cancel();
        self.stop_runtime().await;
        // Do not cancel a refresh response after dispatch. Wait for its owner to
        // checkpoint, then prevent the retired generation from entering Ready.
        let _mutation = self.mutation.lock().await;
        let result = self.delete_session().await;
        if result.is_ok() {
            *self.session.write().await = None;
        }
        self.event("closed", None, None, None);
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
