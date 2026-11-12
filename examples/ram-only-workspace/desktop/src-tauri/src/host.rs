use crate::native_close::NativeClose;
use crate::{
    auth_provider::{IpcProvider, TokenBroker},
    gateway_host::{GatewayFactory, GatewayRuntime, LaunchContext},
    oauth_callback::{Callback, REDIRECT},
    oauth_transport::{Binding, OAuthTransport},
};
use cybergrid_sdk::{Client, Config, CoordinatorRevocation, StateSnapshot};
use local_workspace::LocalWorkspace;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex as SyncMutex,
};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
use workspace_core::Workspace;
#[derive(Clone)]
pub struct BuildConfig {
    pub server: String,
    pub issuer: String,
    pub client: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub spa_client: String,
    pub browser_origins: Vec<String>,
    pub product: String,
}
impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            server: option_env!("CYBERGRID_SAMPLE_SERVER")
                .unwrap_or("https://cybergrid.aixgg.com")
                .into(),
            issuer: option_env!("CYBERGRID_SAMPLE_ISSUER")
                .unwrap_or("https://kn3wnj.logto.app/oidc")
                .into(),
            client: option_env!("CYBERGRID_SAMPLE_NATIVE_CLIENT")
                .unwrap_or("24wdctfohmxkk2nb6n0mp")
                .into(),
            resource: option_env!("CYBERGRID_SAMPLE_RESOURCE")
                .unwrap_or("https://cybergrid.aixgg.com/api")
                .into(),
            product: option_env!("CYBERGRID_SAMPLE_PRODUCT").unwrap_or("").into(),
            scopes: "openid offline_access profile email connect"
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            spa_client: option_env!("CYBERGRID_SAMPLE_SPA_CLIENT")
                .unwrap_or("")
                .into(),
            browser_origins: option_env!("CYBERGRID_SAMPLE_BROWSER_ORIGIN")
                .map(|v| vec![v.to_owned()])
                .unwrap_or_default(),
        }
    }
}
struct Cleanup {
    done: Mutex<bool>,
    notify: Notify,
}
impl Cleanup {
    async fn wait(&self) {
        loop {
            let n = self.notify.notified();
            if *self.done.lock().await {
                return;
            };
            n.await
        }
    }
}
struct Startup {
    done: Mutex<bool>,
    notify: Notify,
}
impl Startup {
    async fn wait(&self) {
        loop {
            let n = self.notify.notified();
            if *self.done.lock().await {
                return;
            };
            n.await
        }
    }
}
struct State {
    client: Option<Arc<Client>>,
    oauth: Option<Arc<OAuthTransport>>,
    callback: Option<Arc<Callback>>,
    owner: u64,
    sdk: u64,
    remote: &'static str,
    revocation: Option<CoordinatorRevocation>,
    error: Option<&'static str>,
    snapshot: Option<StateSnapshot>,
    gateway: Option<GatewayRuntime>,
    account: Option<String>,
    cleanup: Option<Arc<Cleanup>>,
    startup: Option<Arc<Startup>>,
    closing: bool,
}
// All enable admission and close fencing share this short synchronous lock.
// No await occurs while it is held; cancellation cannot be replaced after a fence.
struct RemoteControl {
    generation: u64,
    cancel: CancellationToken,
    reserved: bool,
    closing: usize,
    close_reason: Option<AutomaticCloseReason>,
}
// Only fixed diagnostics can cross the host view boundary. Keep the first cause
// under the same synchronous fence as the gate; stale owners cannot write it.
#[derive(Clone, Copy)]
pub(crate) enum AutomaticCloseReason {
    StartupTimeout,
    EndpointChanged,
    EndpointUnavailable,
    EndpointClosed,
    EndpointWatchClosed,
    PublicationExpired,
    PublicationOwnerEnded,
    PublicationFailure(&'static str),
}
impl AutomaticCloseReason {
    fn code(self) -> &'static str {
        match self {
            Self::StartupTimeout => "REMOTE_START_TIMEOUT",
            Self::EndpointChanged => "GATEWAY_ENDPOINT_CHANGED",
            Self::EndpointUnavailable => "GATEWAY_ENDPOINT_UNAVAILABLE",
            Self::EndpointClosed => "GATEWAY_ENDPOINT_CLOSED",
            Self::EndpointWatchClosed => "GATEWAY_ENDPOINT_WATCH_CLOSED",
            Self::PublicationExpired => "PUBLICATION_EXPIRED",
            Self::PublicationOwnerEnded => "PUBLICATION_OWNER_ENDED",
            Self::PublicationFailure(code) => match code {
                "NO_SESSION"
                | "PRODUCT_MISMATCH"
                | "POLICY_UNAVAILABLE"
                | "POLICY_REQUIRED"
                | "DIRECTORY_UNAVAILABLE"
                | "SCOPE_CHANGED"
                | "ADDRESS_REVISION_EXHAUSTED"
                | "INVALID_GATEWAY_DESCRIPTOR"
                | "PUBLICATION_UNAVAILABLE"
                | "NATIVE_ENABLE_FAILED"
                | "PUBLICATION_UNCONFIRMED"
                | "INVALID_EXPIRY"
                | "PUBLICATION_EXPIRED"
                | "STALE_PUBLICATION"
                | "GATEWAY_ENDPOINT_UNAVAILABLE"
                | "GATEWAY_CAPABILITY_MISMATCH" => code,
                _ => "REMOTE_START_OR_PUBLICATION_FAILED",
            },
        }
    }
    pub(crate) fn endpoint(state: Option<&crate::gateway_host::GatewayEndpointState>) -> Self {
        use crate::gateway_host::GatewayEndpointState;
        match state {
            None => Self::EndpointWatchClosed,
            Some(GatewayEndpointState::Unavailable { .. } | GatewayEndpointState::Pending) => {
                Self::EndpointUnavailable
            }
            Some(GatewayEndpointState::Closed { .. }) => Self::EndpointClosed,
            Some(GatewayEndpointState::Available(_)) => Self::EndpointChanged,
        }
    }
}
struct EnableReservation {
    host: Arc<Host>,
    generation: u64,
    cancel: CancellationToken,
    handed_off: bool,
}
impl Drop for EnableReservation {
    fn drop(&mut self) {
        if !self.handed_off {
            self.host.finish_enable(self.generation, false);
        }
    }
}
struct CloseAdmission(Arc<Host>);
impl Drop for CloseAdmission {
    fn drop(&mut self) {
        self.0.remote_control.lock().unwrap().closing -= 1;
    }
}
pub struct Host {
    pub broker: Arc<TokenBroker>,
    pub workspace: Workspace,
    pub local: LocalWorkspace,
    config: BuildConfig,
    state: Mutex<State>,
    pub exit_allowed: AtomicBool,
    pub gate: Arc<AtomicBool>,
    generation: AtomicU64,
    operation: Mutex<()>,
    factory: Option<Arc<dyn GatewayFactory>>,
    pub(crate) native_close: SyncMutex<Option<Arc<NativeClose>>>,
    runtime: tokio::runtime::Handle,
    pub(crate) bridge_history: SyncMutex<Option<bridge_publisher::AddressRecord>>,
    remote_control: SyncMutex<RemoteControl>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub remote: &'static str,
    pub local_closed: bool,
    pub coordinator_revocation: Option<CoordinatorRevocation>,
    pub gateway: &'static str,
    pub browser_bridge: &'static str,
    pub error: Option<&'static str>,
    pub sdk_generation: u64,
    pub owner_generation: u64,
    pub sdk: Option<StateSnapshot>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Begin {
    pub owner_generation: u64,
    pub binding: Binding,
}
impl Host {
    pub async fn new(
        config: BuildConfig,
        factory: Option<Arc<dyn GatewayFactory>>,
    ) -> Result<Arc<Self>, &'static str> {
        let gate = Arc::new(AtomicBool::new(false));
        let workspace = Workspace::with_remote_gate(gate.clone());
        let local = LocalWorkspace::start(workspace.clone()).await?;
        Ok(Arc::new(Self {
            broker: Arc::new(TokenBroker::default()),
            workspace,
            local,
            config,
            state: Mutex::new(State {
                client: None,
                oauth: None,
                callback: None,
                owner: 0,
                sdk: 0,
                remote: "off",
                revocation: None,
                error: None,
                snapshot: None,
                gateway: None,
                account: None,
                cleanup: None,
                startup: None,
                closing: false,
            }),
            exit_allowed: AtomicBool::new(false),
            gate,
            generation: AtomicU64::new(0),
            operation: Mutex::new(()),
            factory,
            native_close: SyncMutex::new(None),
            runtime: tokio::runtime::Handle::try_current().map_err(|_| "RUNTIME_REQUIRED")?,
            bridge_history: SyncMutex::new(None),
            remote_control: SyncMutex::new(RemoteControl {
                generation: 0,
                cancel: CancellationToken::new(),
                reserved: false,
                closing: 0,
                close_reason: None,
            }),
        }))
    }
    pub async fn view(&self) -> View {
        let s = self.state.lock().await;
        View {
            remote: s.remote,
            local_closed: !self.gate.load(Ordering::SeqCst),
            coordinator_revocation: s.revocation,
            gateway: if self.factory.is_some() {
                "installed"
            } else {
                "unavailable"
            },
            browser_bridge: if self.gate.load(Ordering::SeqCst) && s.remote == "connectable" {
                "connectable"
            } else {
                "unpublished"
            },
            error: self
                .remote_control
                .lock()
                .unwrap()
                .close_reason
                .map(AutomaticCloseReason::code)
                .or(s.error),
            sdk_generation: s.sdk,
            owner_generation: s.owner,
            sdk: s.snapshot.clone(),
        }
    }
    fn client(&self) -> Result<Arc<Client>, &'static str> {
        Client::new(Config {
            base_url: self.config.server.clone(),
            app_id: self.config.client.clone(),
            ..Config::default()
        })
        .map_err(|_| "SDK_CONFIG")
    }
    pub async fn begin(self: &Arc<Self>) -> Result<Begin, &'static str> {
        let _guard = self.operation.lock().await;
        if self.state.lock().await.oauth.is_some() {
            return Err("OWNER_EXISTS");
        };
        let client = self.client()?;
        let expected = client
            .auth_config()
            .await
            .map_err(|_| "COORDINATION_UNAVAILABLE")?;
        let binding = Binding {
            issuer: self.config.issuer.clone(),
            endpoint: url::Url::parse(&self.config.issuer)
                .map_err(|_| "BINDING_MISMATCH")?
                .origin()
                .ascii_serialization(),
            client_id: self.config.client.clone(),
            resource: self.config.resource.clone(),
            scopes: self.config.scopes.clone(),
            redirect_uri: REDIRECT.into(),
        };
        binding.validate()?;
        if !binding.matches(&expected) {
            return Err("BINDING_MISMATCH");
        };
        let oauth = OAuthTransport::new(binding.clone())?;
        let callback = Callback::start().await?;
        let mut s = self.state.lock().await;
        s.owner = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        s.oauth = Some(oauth);
        s.callback = Some(callback);
        s.error = None;
        Ok(Begin {
            owner_generation: s.owner,
            binding,
        })
    }
    pub async fn oauth(&self, owner: u64) -> Result<Arc<OAuthTransport>, &'static str> {
        let s = self.state.lock().await;
        if s.owner != owner {
            return Err("STALE_OWNER");
        };
        s.oauth.clone().ok_or("NO_OWNER")
    }
    pub async fn callback(&self, owner: u64) -> Result<String, &'static str> {
        let c = {
            let s = self.state.lock().await;
            if s.owner != owner {
                return Err("STALE_OWNER");
            };
            s.callback.clone().ok_or("NO_OWNER")?
        };
        c.wait().await
    }
    pub async fn prepare_sdk(&self, owner: u64) -> Result<u64, &'static str> {
        self.broker.barrier().await;
        let _guard = self.operation.lock().await;
        let mut s = self.state.lock().await;
        if s.owner != owner || s.oauth.is_none() || s.closing || s.startup.is_some() {
            return Err("SDK_BUSY");
        };
        if s.client.is_none() {
            s.sdk = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            s.cleanup = None;
            s.snapshot = None;
        }
        let sdk = s.sdk;
        drop(s);
        self.broker.bind(owner, sdk).await?;
        Ok(sdk)
    }
    pub async fn login(self: &Arc<Self>, owner: u64, sdk: u64) -> Result<(), &'static str> {
        let _guard = self.operation.lock().await;
        let client;
        {
            let mut s = self.state.lock().await;
            if s.owner != owner || s.sdk != sdk || s.closing {
                return Err("STALE_SDK");
            };
            if s.snapshot
                .as_ref()
                .is_some_and(|v| v.coordination.phase == cybergrid_sdk::StatePhase::Ready)
            {
                return Ok(());
            };
            if let Some(existing) = s.client.clone() {
                client = existing;
            } else {
                client = self.client()?;
                client
                    .set_auth_provider(Arc::new(IpcProvider {
                        broker: self.broker.clone(),
                        sdk_generation: sdk,
                    }))
                    .await;
                s.client = Some(client.clone());
                s.remote = "off";
                self.watch(client.clone(), sdk);
            }
        }
        match client.login_with_auth_provider("RAM Workspace").await {
            Ok(session) => {
                let product = session.product_id.as_deref().unwrap_or(&session.app_id);
                if !self.config.product.is_empty() && product != self.config.product {
                    client.disable_remote().await;
                    return Err("PRODUCT_MISMATCH");
                }
                let mut s = self.state.lock().await;
                if s.sdk != sdk || s.closing || s.client.is_none() {
                    return Err("CANCELLED");
                };
                s.account = Some(session.user.id.clone());
                drop(s);
                self.workspace.set_account(Some(session.user.id)).await;
                Ok(())
            }
            Err(_) => {
                self.state.lock().await.error = Some("COORDINATION_UNAVAILABLE");
                Err("COORDINATION_UNAVAILABLE")
            }
        }
    }
    fn watch(self: &Arc<Self>, client: Arc<Client>, sdk: u64) {
        let host = Arc::downgrade(self);
        let mut rx = client.watch_state();
        tokio::spawn(async move {
            loop {
                let value = rx.borrow_and_update().clone();
                let Some(host) = host.upgrade() else { break };
                {
                    let mut s = host.state.lock().await;
                    if s.sdk != sdk {
                        break;
                    };
                    s.snapshot = Some(value);
                }
                drop(host);
                if rx.changed().await.is_err() {
                    break;
                }
            }
        });
    }
    pub async fn enable(self: &Arc<Self>) -> Result<(), &'static str> {
        let Some(factory) = self.factory.clone() else {
            self.state.lock().await.error = Some("GATEWAY_UNAVAILABLE");
            return Err("GATEWAY_UNAVAILABLE");
        };
        if self.config.spa_client.is_empty()
            || self.config.browser_origins.is_empty()
            || self.config.product.is_empty()
        {
            return Err("GATEWAY_POLICY_REQUIRED");
        };
        let mut reservation = self.reserve_enable()?;
        let action = reservation.generation;
        let cancel = reservation.cancel.clone();
        let (client, account, sdk, start) = {
            let mut s = self.state.lock().await;
            let control = self.remote_control.lock().unwrap();
            if control.generation != action || cancel.is_cancelled() || control.closing != 0 {
                return Err("CANCELLED");
            }
            if s.closing || s.startup.is_some() || s.gateway.is_some() {
                return Err("REMOTE_BUSY");
            };
            let client = s.client.clone().ok_or("NOT_LOGGED_IN")?;
            let account = s.account.clone().ok_or("NOT_LOGGED_IN")?;
            let start = Arc::new(Startup {
                done: Mutex::new(false),
                notify: Notify::new(),
            });
            s.startup = Some(start.clone());
            s.remote = "preparing";
            s.error = None;
            (client, account, s.sdk, start)
        };
        *self.native_close.lock().unwrap() =
            Some(NativeClose::new(client.clone(), self.runtime.clone()));
        let host = self.clone();
        let deadline_host = self.clone();
        let deadline_cancel = cancel.clone();
        self.runtime.spawn(async move {
            tokio::select! { biased; _ = deadline_cancel.cancelled() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                    if deadline_host.is_generation(action) && !deadline_host.gate.load(Ordering::SeqCst) {
                        deadline_host.seal_generation_with_reason(action, AutomaticCloseReason::StartupTimeout);
                        let _ = deadline_host.disable().await;
                    }
                }
            }
        });
        // No suspension between reservation handoff and the owned startup task.
        reservation.handed_off = true;
        tokio::spawn(async move {
            let result = host
                .start_gateway(
                    factory,
                    client.clone(),
                    account,
                    sdk,
                    action,
                    cancel.clone(),
                )
                .await;
            if let Err(error) = result {
                host.seal_generation_with_reason(
                    action,
                    AutomaticCloseReason::PublicationFailure(error),
                );
                let receipt = host.native_close.lock().unwrap().clone();
                let outcome = match receipt {
                    Some(r) => r.wait().await,
                    None => client.disable_remote().await,
                };
                let mut s = host.state.lock().await;
                if s.sdk == sdk && !s.closing {
                    s.client = None;
                    s.revocation = Some(outcome.coordinator_revocation);
                }
            }
            {
                let mut s = host.state.lock().await;
                if s.sdk == sdk {
                    s.startup = None;
                    if result.is_err() {
                        s.error = Some("REMOTE_START_FAILED");
                        if !s.closing {
                            s.remote = "off"
                        }
                    }
                }
            }
            host.finish_enable(action, result.is_ok());
            *start.done.lock().await = true;
            start.notify.notify_waiters();
            result
        })
        .await
        .map_err(|_| "REMOTE_START_FAILED")?
        .map_err(|_| "REMOTE_START_FAILED")
    }
    async fn start_gateway(
        self: &Arc<Self>,
        factory: Arc<dyn GatewayFactory>,
        client: Arc<Client>,
        account: String,
        sdk: u64,
        action: u64,
        cancel: CancellationToken,
    ) -> Result<(), &'static str> {
        if cancel.is_cancelled() || !self.is_generation(action) {
            return Err("CANCELLED");
        }
        let mut runtime = GatewayRuntime::create(
            factory,
            self.workspace.clone(),
            LaunchContext {
                generation: sdk,
                local_port: 0,
                local_token: String::new(),
                issuer: self.config.issuer.clone(),
                resource: self.config.resource.clone(),
                allowed_spa_client: self.config.spa_client.clone(),
                owner_subject: account,
                allowed_origins: self.config.browser_origins.clone(),
                cancel: cancel.clone(),
            },
        )
        .await?;
        let mut updates = runtime.endpoints.clone();
        let endpoint_host = self.clone();
        let endpoint_cancel = cancel.clone();
        let initial = updates.borrow_and_update().clone();
        self.runtime.spawn(async move {
            loop {
                tokio::select! { biased; _ = endpoint_cancel.cancelled() => break,
                    r = updates.changed() => {
                        if r.is_ok() && *updates.borrow_and_update() == initial { continue; }
                        let reason = if r.is_err() { AutomaticCloseReason::endpoint(None) } else { AutomaticCloseReason::endpoint(Some(&updates.borrow_and_update())) };
                        endpoint_host.seal_generation_with_reason(action, reason);
                        if endpoint_host.is_generation(action) { let _ = endpoint_host.disable().await; }
                        break;
                    }
                }
            }
        });
        let ready = self
            .publish_initial(&client, &mut runtime, sdk, action, &cancel)
            .await;
        if let Err(e) = ready {
            self.seal_generation_with_reason(action, AutomaticCloseReason::PublicationFailure(e));
            let _ = runtime.close().await;
            return Err(e);
        }
        let mut s = self.state.lock().await;
        {
            let control = self.remote_control.lock().unwrap();
            if s.sdk == sdk
                && !s.closing
                && !cancel.is_cancelled()
                && control.generation == action
                && control.closing == 0
            {
                let endpoints = runtime.endpoints.clone();
                s.gateway = Some(runtime);
                s.remote = "connectable";
                self.gate.store(true, Ordering::SeqCst);
                drop(control);
                drop(s);
                self.start_publisher(client, endpoints, sdk, action, cancel);
                return Ok(());
            }
        }
        drop(s);
        let _ = runtime.close().await;
        Err("CANCELLED")
    }
    fn reserve_enable(self: &Arc<Self>) -> Result<EnableReservation, &'static str> {
        let mut control = self.remote_control.lock().unwrap();
        if control.reserved || control.closing != 0 {
            return Err("REMOTE_BUSY");
        }
        control.generation += 1;
        control.cancel.cancel();
        control.cancel = CancellationToken::new();
        control.reserved = true;
        control.close_reason = None;
        Ok(EnableReservation {
            host: self.clone(),
            generation: control.generation,
            cancel: control.cancel.clone(),
            handed_off: false,
        })
    }
    pub(crate) fn is_generation(&self, generation: u64) -> bool {
        let control = self.remote_control.lock().unwrap();
        control.generation == generation && control.closing == 0
    }
    pub(crate) fn seal_generation_with_reason(
        &self,
        generation: u64,
        reason: AutomaticCloseReason,
    ) {
        {
            let mut control = self.remote_control.lock().unwrap();
            if control.generation != generation {
                return;
            }
            if control.close_reason.is_none() {
                control.close_reason = Some(reason);
            }
            self.gate.store(false, Ordering::SeqCst);
            control.cancel.cancel();
        }
        self.seal_native();
    }
    fn finish_enable(&self, generation: u64, success: bool) {
        let mut control = self.remote_control.lock().unwrap();
        if control.generation == generation && !success {
            self.gate.store(false, Ordering::SeqCst);
            control.cancel.cancel();
            control.reserved = false;
        }
    }
    fn fence(&self, control: &mut RemoteControl) {
        self.gate.store(false, Ordering::SeqCst);
        control.generation += 1;
        control.cancel.cancel();
        control.reserved = false;
    }
    pub fn seal(&self) {
        {
            let mut control = self.remote_control.lock().unwrap();
            self.fence(&mut control);
        }
        self.seal_native();
    }
    fn seal_native(&self) {
        let closer = self.native_close.lock().unwrap().clone();
        if let Some(c) = closer {
            c.seal();
        }
    }
    pub async fn disable(self: &Arc<Self>) -> Result<(), &'static str> {
        // Admission and fencing precede any asynchronous lock. The complete close
        // remains owned even if this public caller drops its wait immediately.
        {
            let mut control = self.remote_control.lock().unwrap();
            self.fence(&mut control);
            control.closing += 1;
        }
        self.seal_native();
        let admission = CloseAdmission(self.clone());
        let host = self.clone();
        tokio::spawn(async move {
            let _admission = admission;
            host.disable_owned().await
        })
        .await
        .map_err(|_| "CLEANUP_PENDING")?
    }
    async fn disable_owned(self: &Arc<Self>) -> Result<(), &'static str> {
        let cleanup;
        {
            let mut s = self.state.lock().await;
            if let Some(existing) = s.cleanup.clone() {
                drop(s);
                existing.wait().await;
                return Ok(());
            };
            cleanup = Arc::new(Cleanup {
                done: Mutex::new(false),
                notify: Notify::new(),
            });
            s.cleanup = Some(cleanup.clone());
            s.closing = true;
            s.remote = "disabling";
            if s.client.is_some() {
                s.revocation = Some(CoordinatorRevocation::Pending)
            };
            if let Some(g) = &s.gateway {
                g.seal()
            };
            let client = s.client.take();
            let gateway = s.gateway.take();
            let start = s.startup.clone();
            let sdk = s.sdk;
            let host = self.clone();
            let completion = cleanup.clone();
            tokio::spawn(async move {
                let sdk_close = async {
                    match client {
                        Some(c) => {
                            let _ = c.withdraw_bridge("workspace").await;
                            let owned = host.native_close.lock().unwrap().clone();
                            Some(match owned {
                                Some(receipt) => receipt.wait().await.coordinator_revocation,
                                None => c.disable_remote().await.coordinator_revocation,
                            })
                        }
                        None => None,
                    }
                };
                let gateway_close = async {
                    if let Some(g) = gateway {
                        g.close().await
                    } else {
                        Ok(())
                    }
                };
                let (revocation, closed) = tokio::join!(sdk_close, gateway_close);
                if let Some(start) = start {
                    start.wait().await
                };
                let mut s = host.state.lock().await;
                if s.sdk == sdk {
                    s.remote = "off";
                    s.closing = false;
                    if revocation.is_some() {
                        s.revocation = revocation
                    };
                    if closed.is_err() {
                        s.error = Some("GATEWAY_CLEANUP_PENDING")
                    }
                }
                drop(s);
                *completion.done.lock().await = true;
                completion.notify.notify_waiters();
            });
        }
        cleanup.wait().await;
        Ok(())
    }
    pub async fn allow_exit(&self) -> Result<(), &'static str> {
        self.broker.barrier().await;
        let s = self.state.lock().await;
        if s.oauth.is_some() || s.client.is_some() || s.closing || s.startup.is_some() {
            return Err("OWNER_PENDING");
        };
        self.exit_allowed.store(true, Ordering::SeqCst);
        Ok(())
    }
    pub async fn end_owner(self: &Arc<Self>, owner: u64) -> Result<(), &'static str> {
        self.disable().await?;
        self.broker.barrier().await;
        let oauth = self.oauth(owner).await?;
        oauth.barrier().await;
        self.broker.detach(owner).await?;
        let mut s = self.state.lock().await;
        if let Some(c) = s.callback.take() {
            c.cancel()
        };
        s.oauth = None;
        s.account = None;
        drop(s);
        self.workspace.set_account(None).await;
        Ok(())
    }
}
#[cfg(test)]
mod tests;

#[path = "bridge_publisher.rs"]
mod bridge_publisher;
