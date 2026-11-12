use crate::{
    browser, platform, protocol,
    services::{Incoming, LocalServices},
};
use cybergrid_sdk::{
    AuthProvider, Client, Config, CoordinatorRevocation, CredentialStore, Device, DirectStrategy,
    Mapping, Port, RemoteDisableOutcome, StatePhase, StateSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabConfig {
    pub base_url: String,
    pub app_id: String,
    pub device_name: String,
    pub force_fallback: bool,
    #[serde(default)]
    pub direct_strategy: DirectStrategy,
    #[serde(default)]
    pub direct_transport: DirectTransport,
    pub cloudflared_path: String,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirectTransport {
    #[default]
    Auto,
    WebrtcOnly,
    IrohCandidate,
}

impl LabConfig {
    fn direct_options(&self) -> (Duration, bool) {
        // The existing near-zero WebRTC budget is a test preference, not a
        // protocol disable switch. Actual transport remains the result.
        (
            if self.force_fallback || self.direct_transport == DirectTransport::IrohCandidate {
                Duration::from_nanos(1)
            } else {
                Duration::from_secs(12)
            },
            !self.force_fallback && self.direct_transport != DirectTransport::WebrtcOnly,
        )
    }
}

#[cfg(test)]
mod direct_transport_tests {
    use super::*;

    #[test]
    fn missing_test_preference_preserves_existing_settings_and_rejects_unknown_values() {
        let mut old = serde_json::to_value(LabConfig::default()).unwrap();
        old.as_object_mut().unwrap().remove("directTransport");
        let config: LabConfig = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(config.direct_transport, DirectTransport::Auto);
        assert_eq!(config.direct_options(), (Duration::from_secs(12), true));
        for value in [
            serde_json::json!("iroh-only"),
            serde_json::json!(true),
            serde_json::json!(null),
        ] {
            old["directTransport"] = value;
            assert!(serde_json::from_value::<LabConfig>(old.clone()).is_err());
        }
    }

    #[test]
    fn test_preferences_keep_fallback_priority_and_direct_strategy() {
        let mut config = LabConfig {
            direct_strategy: DirectStrategy::Parallel,
            ..Default::default()
        };
        for (mode, expected) in [
            (DirectTransport::Auto, (Duration::from_secs(12), true)),
            (
                DirectTransport::WebrtcOnly,
                (Duration::from_secs(12), false),
            ),
            (
                DirectTransport::IrohCandidate,
                (Duration::from_nanos(1), true),
            ),
        ] {
            config.direct_transport = mode;
            config.force_fallback = false;
            assert_eq!(config.direct_options(), expected);
            assert!(matches!(config.direct_strategy, DirectStrategy::Parallel));
            config.force_fallback = true;
            assert_eq!(config.direct_options(), (Duration::from_nanos(1), false));
        }
    }
}

// Only these fixed codes cross the browser-start IPC boundary. Never serialize
// an underlying SDK, storage, or OAuth error into the frontend response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrowserStartError {
    ExistingAuthorization,
    Failed,
}
impl BrowserStartError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::ExistingAuthorization => {
                "此应用已有本机授权，请选择“恢复本机授权”；如需重新网页登录，请先“退出并撤销本机授权”。"
            }
            Self::Failed => "无法启动网页登录，请核对配置或处理已有授权",
        }
    }
}
impl From<String> for BrowserStartError {
    fn from(_: String) -> Self {
        Self::Failed
    }
}
impl From<&str> for BrowserStartError {
    fn from(_: &str) -> Self {
        Self::Failed
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_rotation_survives_shutdown_timeout_and_blocks_new_login_and_revoke() {
        let root = std::env::temp_dir().join(format!(
            "cybergrid-lab-lifecycle-{}",
            hex::encode(rand::random::<[u8; 16]>())
        ));
        let state = LabState::new(root.join("data"), root.join("cache")).unwrap();
        let config = LabConfig::default();
        let path = credential_path(&state.data_dir, &config);
        std::fs::write(&path, b"old private grant fixture").unwrap();
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let (release, released) = tokio::sync::oneshot::channel();
        let (saved, saved_rx) = tokio::sync::oneshot::channel();
        let late_path = path.clone();
        let task = tokio::spawn(async move {
            // Model the SDK's already-sent token response: UI cancellation must
            // not drop the worker before its newest rotation reaches disk.
            worker_cancel.cancelled().await;
            released.await.unwrap();
            std::fs::write(late_path, b"newly rotated private grant fixture").unwrap();
            saved.send(()).unwrap();
        });
        {
            let mut inner = state.inner.lock().await;
            inner.generation = 1;
            inner.phase = "authorizing".into();
            inner.pending = Some(Pending {
                cancel,
                generation: 1,
                task: Some(task),
            });
        }
        state.cancel_login().await.unwrap();
        assert_eq!(state.view().await.phase, "signed_out");
        let error = state
            .shutdown_with_wait(true, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(error.contains("禁止重新登录或撤销"));
        assert!(!error.contains("授权撤销失败"));
        assert_eq!(state.retired.lock().await.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), b"old private grant fixture");
        let error = state.start_login(config.clone(), false).await.unwrap_err();
        assert_eq!(error, BrowserStartError::Failed.message());
        // New browser login exports only fixed codes; resume preserves the
        // existing private-owner exclusion and its actionable fixed message.
        let error = state.start_login(config, true).await.unwrap_err();
        assert!(error.contains("上一次授权正在安全结束"));
        release.send(()).unwrap();
        saved_rx.await.unwrap();
        state.shutdown(false).await.unwrap();
        assert!(state.retired.lock().await.is_empty());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"newly rotated private grant fixture"
        );
        assert!(state.view().await.can_resume);
        std::fs::remove_dir_all(root).unwrap();
    }
}
impl Default for LabConfig {
    fn default() -> Self {
        Self {
            base_url: "https://cybergrid.aixgg.com".into(),
            app_id: "24wdctfohmxkk2nb6n0mp".into(),
            device_name: if cfg!(target_os = "android") {
                "Android".into()
            } else {
                "Mac".into()
            },
            force_fallback: false,
            direct_strategy: DirectStrategy::Sequential,
            direct_transport: DirectTransport::Auto,
            cloudflared_path: String::new(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDevice {
    device_id: String,
    device_name: String,
    user_id: String,
    ports: Vec<Port>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    peer_id: String,
    peer_name: String,
    local_address: String,
    transport: String,
    connected: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicDevice {
    id: String,
    name: String,
    online: bool,
    last_seen: String,
    ports: Vec<Port>,
}
impl From<&Device> for PublicDevice {
    fn from(device: &Device) -> Self {
        Self {
            id: device.id.clone(),
            name: device.name.clone(),
            online: device.online,
            last_seen: device.last_seen.to_rfc3339(),
            ports: device.ports.clone(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    id: u64,
    time: String,
    direction: String,
    peer: String,
    kind: String,
    message: Option<String>,
    bytes: Option<usize>,
    sha256: Option<String>,
    transport: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    phase: String,
    preset_config: LabConfig,
    browser_diagnostic: Option<BrowserDiagnostic>,
    platform: String,
    auth_mode: String,
    config: LabConfig,
    can_resume: bool,
    error: Option<String>,
    local: Option<LocalDevice>,
    devices: Vec<PublicDevice>,
    records: Vec<Record>,
    connection: Option<Connection>,
    status: String,
    sdk_snapshot: Option<StateSnapshot>,
    remote_action: String,
    local_closed: bool,
    coordinator_revocation: Option<CoordinatorRevocation>,
    directory_known: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserFeatures {
    pub abort_timeout: bool,
    pub abort_any: bool,
    pub web_crypto: bool,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDiagnostic {
    phases: Vec<String>,
    features: Option<BrowserFeatures>,
}
struct Pending {
    cancel: CancellationToken,
    generation: u64,
    task: Option<tokio::task::JoinHandle<()>>,
}
struct Running {
    client: Arc<Client>,
    owner: Arc<dyn AuthProvider>,
    services: Arc<LocalServices>,
    cancel: CancellationToken,
    mapping: Mutex<Option<Arc<Mapping>>>,
    operation: Mutex<()>,
    device_id: String,
}
struct Inner {
    phase: String,
    auth_mode: String,
    config: LabConfig,
    error: Option<String>,
    local: Option<LocalDevice>,
    devices: Vec<Device>,
    records: Vec<Record>,
    connection: Option<Connection>,
    status: String,
    generation: u64,
    pending: Option<Pending>,
    running: Option<Arc<Running>>,
    record_id: u64,
    sdk_generation: u64,
    sdk_snapshot: Option<StateSnapshot>,
    remote_action: String,
    local_closed: bool,
    coordinator_revocation: Option<CoordinatorRevocation>,
    directory_known: bool,
    remote_generation: u64,
    opening: Option<Arc<Client>>,
}
#[cfg(test)]
type ClientFactory = Arc<dyn Fn(&LabConfig) -> Result<Arc<Client>, String> + Send + Sync>;
pub struct LabState {
    inner: Mutex<Inner>,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    retired: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    grant_owners: Mutex<Vec<Arc<Client>>>,
    pub exiting: AtomicBool,
    auth_setup: Mutex<()>,
    browser_diagnostic: Mutex<BrowserDiagnostic>,
    browser: Mutex<Option<Arc<browser::Transaction>>>,
    remote_operation: Mutex<()>,
    disabled_clients: Mutex<Vec<Arc<Client>>>,
    #[cfg(test)]
    client_factory: Option<ClientFactory>,
}
fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "时间不可用".into())
}
pub(crate) fn credential_path(data: &Path, config: &LabConfig) -> PathBuf {
    let hash = protocol::text_hash(&format!(
        "{}\0{}",
        config.base_url.trim_end_matches('/'),
        config.app_id
    ));
    data.join(format!("native-v4-{}.json", &hash[..24]))
}
fn protect_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl LabState {
    pub fn new(data_dir: PathBuf, cache_dir: PathBuf) -> Result<Arc<Self>, String> {
        protect_directory(&data_dir)?;
        protect_directory(&cache_dir)?;
        let path = data_dir.join("settings.json");
        let config = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|_| "本机公开配置文件无效".to_string())?
        } else {
            LabConfig::default()
        };
        Ok(Arc::new(Self {
            inner: Mutex::new(Inner {
                phase: "signed_out".into(),
                auth_mode: "browser".into(),
                config,
                error: None,
                local: None,
                devices: vec![],
                records: vec![],
                connection: None,
                status: if browser::journal_path(&data_dir).exists() {
                    "检测到待清理浏览器授权；下次登录会先安全处理，未完成前禁止新授权".into()
                } else {
                    "使用同一个账号与应用，在两端分别登录".into()
                },
                generation: 0,
                pending: None,
                running: None,
                record_id: 0,
                sdk_generation: 0,
                sdk_snapshot: None,
                remote_action: "off".into(),
                local_closed: true,
                coordinator_revocation: None,
                directory_known: false,
                remote_generation: 0,
                opening: None,
            }),
            data_dir,
            cache_dir,
            retired: Mutex::new(Vec::new()),
            grant_owners: Mutex::new(Vec::new()),
            exiting: AtomicBool::new(false),
            auth_setup: Mutex::new(()),
            browser_diagnostic: Mutex::new(BrowserDiagnostic::default()),
            browser: Mutex::new(None),
            remote_operation: Mutex::new(()),
            disabled_clients: Mutex::new(Vec::new()),
            #[cfg(test)]
            client_factory: None,
        }))
    }
    pub async fn browser_trace(
        &self,
        phase: &str,
        features: Option<BrowserFeatures>,
    ) -> Result<(), String> {
        if !cfg!(debug_assertions) {
            return Ok(());
        }
        if ![
            "begin-await",
            "native-reserved",
            "native-config-await",
            "native-config-ready",
            "begin-ready",
            "client-created",
            "sdk-signin",
            "native-fetch-await",
            "native-fetch-ready",
            "native-fetch-error",
            "pkce-state",
            "pkce-verifier",
            "pkce-challenge",
            "open-await",
            "open-ready",
            "native-open-validated",
            "native-open-ready",
            "callback-await",
            "callback-ready",
            "callback-has-code",
            "callback-no-code",
            "callback-has-state",
            "callback-no-state",
            "callback-error-invalid_request",
            "callback-error-invalid_scope",
            "callback-error-invalid_target",
            "callback-error-unauthorized_client",
            "callback-error-access_denied",
            "callback-error-unsupported_response_type",
            "callback-error-server_error",
            "callback-error-temporarily_unavailable",
            "callback-error-interaction_required",
            "callback-error-login_required",
            "callback-error-consent_required",
            "callback-error-other",
            "sdk-session-missing",
            "sdk-session-invalid",
            "sdk-redirect-mismatch",
            "sdk-error-callback",
            "sdk-state-missing",
            "sdk-state-mismatch",
            "sdk-code-missing",
            "sdk-callback-other",
            "sdk-callback",
            "resource-await",
            "commit-await",
            "commit-ready",
            "failed",
        ]
        .contains(&phase)
        {
            return Err("无效的诊断阶段".into());
        }
        let mut d = self.browser_diagnostic.lock().await;
        if phase == "begin-await" {
            d.phases.clear();
            d.features = None;
        }
        d.phases.push(phase.into());
        if d.phases.len() > 32 {
            d.phases.remove(0);
        }
        if features.is_some() {
            d.features = features;
        }
        let _ = browser::write_diagnostic(&self.data_dir, &*d);
        Ok(())
    }
    pub async fn view(&self) -> View {
        let s = self.inner.lock().await;
        View {
            phase: s.phase.clone(),
            preset_config: LabConfig::default(),
            browser_diagnostic: if cfg!(debug_assertions) {
                Some(self.browser_diagnostic.lock().await.clone())
            } else {
                None
            },
            platform: browser::platform().into(),
            auth_mode: s.auth_mode.clone(),
            config: s.config.clone(),
            can_resume: credential_path(&self.data_dir, &s.config).exists(),
            error: s.error.clone(),
            local: s.local.clone(),
            devices: s.devices.iter().map(PublicDevice::from).collect(),
            records: s.records.clone(),
            connection: s.connection.clone(),
            status: s.status.clone(),
            sdk_snapshot: s.sdk_snapshot.clone(),
            remote_action: s.remote_action.clone(),
            local_closed: s.local_closed,
            coordinator_revocation: s.coordinator_revocation,
            directory_known: s.directory_known,
        }
    }
    fn create_client(&self, config: &LabConfig) -> Result<Arc<Client>, String> {
        #[cfg(test)]
        if let Some(factory) = &self.client_factory {
            return factory(config);
        }
        if config.device_name.trim().is_empty() || config.device_name.len() > 80 {
            return Err("本机名称必须为1至80字节".into());
        }
        let mut provider =
            platform::provider_runtime(config.cloudflared_path.trim(), &self.cache_dir)?;
        provider.env_provider = {
            let configured = config.cloudflared_path.clone();
            let cache = self.cache_dir.clone();
            Some(Arc::new(move || {
                platform::provider_runtime(configured.trim(), &cache)
                    .map(|runtime| runtime.env)
                    .map_err(|_| {
                        cybergrid_sdk::Error::Transport("无法读取当前网络的 provider 环境".into())
                    })
            }))
        };
        let (direct_timeout, iroh_enabled) = config.direct_options();
        let c = Config {
            base_url: config.base_url.clone(),
            app_id: config.app_id.clone(),
            direct_timeout,
            iroh_enabled,
            direct_strategy: config.direct_strategy,
            fallback_timeout: Duration::from_secs(45),
            cloudflared: Some(provider),
            ..Config::default()
        };
        Client::new(c).map_err(|e| e.to_string())
    }
    fn save_settings(&self, config: &LabConfig) -> Result<(), String> {
        use std::io::Write;
        let path = self.data_dir.join("settings.json");
        let temp = self.data_dir.join(".settings.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temp).map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec(config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn start_login(
        self: &Arc<Self>,
        mut config: LabConfig,
        resume: bool,
    ) -> Result<(), String> {
        if !resume {
            return self
                .begin_web_login(config)
                .await
                .map(|_| ())
                .map_err(|error| error.message().to_string());
        }
        let _setup = self.auth_setup.lock().await;
        self.prepare_browser_cleanup().await?;
        {
            let mut retired = self.retired.lock().await;
            retired.retain(|task| !task.is_finished());
            if !retired.is_empty() {
                return Err("上一次授权正在安全结束，请稍后恢复或重新登录".into());
            }
        }
        {
            let mut owners = self.grant_owners.lock().await;
            for owner in owners.iter() {
                owner.retry_pending_grant().await.map_err(|_| {
                    "最新授权尚未安全保存；请恢复可写存储后重试，禁止重新授权".to_string()
                })?;
            }
            owners.clear();
        }
        config.base_url = config.base_url.trim().trim_end_matches('/').into();
        config.app_id = config.app_id.trim().into();
        config.device_name = config.device_name.trim().into();
        let client = self.create_client(&config)?;
        let credentials = CredentialStore::new(credential_path(&self.data_dir, &config));
        if !resume && credentials.exists() {
            return Err("此应用已有本机授权，请选择恢复登录，或先撤销本机授权".into());
        }
        if resume && !credentials.exists() {
            return Err("没有此账号应用的本机授权，请先登录".into());
        }
        let cancel = CancellationToken::new();
        let generation = {
            let mut s = self.inner.lock().await;
            if s.running.is_some() || s.pending.is_some() || s.phase == "closing" {
                return Err("已有登录或会话正在运行".into());
            }
            self.save_settings(&config)?;
            s.generation += 1;
            let generation = s.generation;
            s.pending = Some(Pending {
                cancel: cancel.clone(),
                generation,
                task: None,
            });
            s.config = config.clone();
            s.auth_mode = "browser".into();
            s.phase = "connecting".into();
            s.error = None;
            s.records.clear();
            s.sdk_generation = 0;
            s.status = "正在恢复本机授权".into();
            generation
        };
        let state = self.clone();
        let task = tokio::spawn(async move {
            state
                .login_worker(config, client, credentials, cancel, generation)
                .await;
        });
        let mut s = self.inner.lock().await;
        if let Some(pending) = s.pending.as_mut().filter(|p| p.generation == generation) {
            pending.task = Some(task);
        } else {
            // Cancellation can win between spawning and publishing the handle.
            // Keep ownership until any in-flight grant has been persisted.
            drop(s);
            self.retired.lock().await.push(task);
        }
        Ok(())
    }
    async fn login_worker(
        self: Arc<Self>,
        config: LabConfig,
        client: Arc<Client>,
        credentials: CredentialStore,
        cancel: CancellationToken,
        generation: u64,
    ) {
        let binding = client
            .use_credential_store(credentials.clone())
            .await
            .map_err(|error| error.to_string());
        let owner: Arc<dyn AuthProvider> = Arc::new(credentials.clone());
        client.set_auth_provider(owner.clone()).await;
        let flow = async {
            binding?;
            let tokens = credentials
                .refresh(&client)
                .await
                .map_err(|e| e.to_string())?;
            self.install_session(
                &config,
                &client,
                owner,
                &cancel,
                generation,
                &tokens.access_token,
            )
            .await
        };
        // Never drop an in-flight
        // rotating refresh/exchange: finish persistence, then close by generation.
        let result = flow.await;
        if let Err(error) = result {
            let _ = client.close().await;
            if client.has_pending_grant().await {
                self.grant_owners.lock().await.push(client.clone());
            }
            let mut s = self.inner.lock().await;
            if s.generation == generation {
                s.pending = None;
                s.phase = "signed_out".into();
                s.status = "登录未完成；已有授权文件会保留，避免丢失撤销材料".into();
                s.error = if cancel.is_cancelled() {
                    None
                } else {
                    Some(error)
                };
            }
        }
    }
    async fn install_session(
        self: &Arc<Self>,
        config: &LabConfig,
        client: &Arc<Client>,
        owner: Arc<dyn AuthProvider>,
        cancel: &CancellationToken,
        generation: u64,
        access_token: &str,
    ) -> Result<(), String> {
        {
            let mut s = self.inner.lock().await;
            if s.generation != generation || cancel.is_cancelled() {
                return Err("登录已取消，本机授权可在下次恢复或撤销".into());
            }
            s.phase = "connecting".into();
            s.status = "授权成功，正在建立本机设备会话".into();
        }
        let session = client
            .login_with_access_token(access_token, &config.device_name)
            .await
            .map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel(64);
        let services = LocalServices::start(session.device_id.clone(), tx).await?;
        let ports = vec![
            Port::tcp(
                protocol::MESSAGE_PORT,
                services.message_port,
                "cybergrid-lab-message-v1",
            ),
            Port::tcp(
                protocol::ECHO_PORT,
                services.echo_port,
                "cybergrid-lab-echo-v1",
            ),
        ];
        for port in &ports {
            if let Err(e) = client.expose(port.clone()).await {
                services.close().await;
                return Err(e.to_string());
            }
        }
        let devices = match client.devices().await {
            Ok(d) => d,
            Err(e) => {
                services.close().await;
                return Err(e.to_string());
            }
        };
        let runtime = Arc::new(Running {
            client: client.clone(),
            owner,
            services: services.clone(),
            cancel: CancellationToken::new(),
            mapping: Mutex::new(None),
            operation: Mutex::new(()),
            device_id: session.device_id.clone(),
        });
        {
            let mut s = self.inner.lock().await;
            if s.generation != generation || cancel.is_cancelled() {
                drop(s);
                runtime.cancel.cancel();
                runtime.services.close().await;
                return Err("登录已取消".into());
            }
            s.local = Some(LocalDevice {
                device_id: session.device_id,
                device_name: config.device_name.clone(),
                user_id: session.user.id,
                ports,
            });
            s.devices = devices;
            s.running = Some(runtime.clone());
            s.pending = None;
            s.phase = "ready".into();
            s.status = "已授权，远程访问关闭；本地服务可用，端口仅保存为本地意图".into();
            s.sdk_snapshot = Some(client.snapshot());
            s.remote_action = "off".into();
            s.local_closed = true;
            s.directory_known = true;
            s.coordinator_revocation = None;
            s.error = None;
        }
        self.spawn_incoming(services, rx);
        self.spawn_runtime(runtime);
        Ok(())
    }
    async fn prepare_browser_cleanup(&self) -> Result<(), String> {
        if let Some(tx) = self.browser.lock().await.clone() {
            if !tx.cancel.is_cancelled() && tx.owned.lock().await.is_none() {
                return Err("已有网页登录正在执行，请先取消".into());
            }
            tx.cleanup().await?;
        }
        browser::recover(&self.data_dir).await?;
        Ok(())
    }
    pub async fn begin_web_login(
        self: &Arc<Self>,
        mut config: LabConfig,
    ) -> Result<browser::Begin, BrowserStartError> {
        // Reserve before the first await: native/ADB cancellation does not rely
        // on a JS caller remaining alive to cancel a late setup result.
        let cancel = CancellationToken::new();
        let generation = {
            let mut s = self.inner.lock().await;
            if self.exiting.load(std::sync::atomic::Ordering::SeqCst)
                || s.running.is_some()
                || s.pending.is_some()
                || s.phase == "closing"
            {
                return Err("已有登录或会话正在运行".into());
            }
            s.generation += 1;
            let g = s.generation;
            s.pending = Some(Pending {
                cancel: cancel.clone(),
                generation: g,
                task: None,
            });
            s.auth_mode = "browser".into();
            s.phase = "authorizing".into();
            s.error = None;
            s.status = "正在核对网页登录配置".into();
            g
        };
        self.browser_trace("native-reserved", None).await?;
        let setup: Result<browser::Begin, BrowserStartError> = async {
            let _setup = self.auth_setup.lock().await;
            self.prepare_browser_cleanup().await?;
            {
                let mut retired = self.retired.lock().await;
                retired.retain(|t| !t.is_finished());
                if !retired.is_empty() {
                    return Err("上一次授权正在安全结束".into());
                }
            }
            if !self.grant_owners.lock().await.is_empty() {
                return Err("待处理授权必须先保存或撤销".into());
            }
            config.base_url = config.base_url.trim().trim_end_matches('/').into();
            config.app_id = config.app_id.trim().into();
            config.device_name = config.device_name.trim().into();
            let client = self.create_client(&config)?;
            let store = CredentialStore::new(credential_path(&self.data_dir, &config));
            if store.exists() {
                return Err(BrowserStartError::ExistingAuthorization);
            }
            self.browser_trace("native-config-await", None).await?;
            let expected = client
                .auth_config()
                .await
                .map_err(|_| "无法核对服务公开认证配置".to_string())?;
            self.browser_trace("native-config-ready", None).await?;
            if expected
                .login_flow
                .as_deref()
                .is_some_and(|flow| flow != "authorization_code")
            {
                return Err("此应用未配置网页登录，请选择普通 Logto Native 应用".into());
            }
            client
                .use_credential_store(store.clone())
                .await
                .map_err(|_| "无法绑定私有凭据存储".to_string())?;
            let owner: Arc<dyn AuthProvider> = Arc::new(store);
            client.set_auth_provider(owner.clone()).await;
            if cancel.is_cancelled() {
                return Err("网页登录已取消".into());
            }
            let (send, recv) = tokio::sync::oneshot::channel();
            let (tx, begin) = browser::Transaction::new(
                config.clone(),
                expected,
                client.clone(),
                &self.data_dir,
                generation,
                cancel.clone(),
                send,
            )
            .await?;
            {
                let mut s = self.inner.lock().await;
                if s.generation != generation
                    || cancel.is_cancelled()
                    || self.exiting.load(std::sync::atomic::Ordering::SeqCst)
                {
                    return Err("网页登录已取消".into());
                }
                self.save_settings(&config)?;
                s.config = config.clone();
                s.status = "请在系统浏览器完成 Logto 登录".into();
            }
            *self.browser.lock().await = Some(tx.clone());
            let state = self.clone();
            let worker_config = config.clone();
            let worker_cancel = cancel.clone();
            let task = tokio::spawn(async move {
                let result = match recv.await {
                    Ok(Some(access)) => {
                        state
                            .install_session(
                                &worker_config,
                                &client,
                                owner,
                                &worker_cancel,
                                generation,
                                &access,
                            )
                            .await
                    }
                    _ => Err("网页登录已结束；待处理授权可恢复或撤销".into()),
                };
                if let Err(error) = result {
                    let _ = client.close().await;
                    if client.has_pending_grant().await {
                        state.grant_owners.lock().await.push(client.clone());
                    }
                    let mut s = state.inner.lock().await;
                    if s.generation == generation {
                        s.pending = None;
                        s.phase = "signed_out".into();
                        s.error = if worker_cancel.is_cancelled() {
                            None
                        } else {
                            Some(error)
                        };
                        s.status = "网页登录未就绪；本机授权材料保留供安全处理".into();
                    }
                }
            });
            {
                let mut s = self.inner.lock().await;
                if let Some(p) = s
                    .pending
                    .as_mut()
                    .filter(|p| p.generation == generation && !cancel.is_cancelled())
                {
                    p.task = Some(task);
                    #[cfg(target_os = "android")]
                    if {
                        tx.source_active
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        crate::android_browser::start(&begin, cancel.clone()).await
                    }
                    .is_err()
                    {
                        tx.source_active
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        tx.source_settled.notify_waiters();
                        cancel.cancel();
                        return Err("无法启动 Android 系统浏览器登录".into());
                    }
                } else {
                    drop(s);
                    tx.source_active
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    tx.source_settled.notify_waiters();
                    tx.cleanup().await?;
                    self.retired.lock().await.push(task);
                    return Err("网页登录已取消".into());
                }
            }
            Ok(begin)
        }
        .await;
        if let Err(error) = &setup {
            cancel.cancel();
            let mut s = self.inner.lock().await;
            if s.generation == generation {
                s.pending = None;
                s.phase = "signed_out".into();
                s.error = Some(error.message().into());
                s.status = error.message().into();
            }
        }
        setup
    }
    pub async fn browser_transaction(&self, id: &str) -> Result<Arc<browser::Transaction>, String> {
        self.browser
            .lock()
            .await
            .as_ref()
            .filter(|t| t.id == id)
            .cloned()
            .ok_or_else(|| "网页登录事务不存在或已失效".into())
    }
    pub async fn browser_cancel(&self, id: &str) -> Result<(), String> {
        let tx = self.browser_transaction(id).await?;
        // Lost final ACK is safe: Rust already owns it. Never revoke it through
        // the producing SDK or invalidate a newly installed session.
        if tx.owned.lock().await.is_some() {
            return Ok(());
        }
        self.cancel_login().await
    }
    #[cfg(target_os = "android")]
    pub async fn android_browser_stage(
        &self,
        id: &str,
        tokens: browser::Tokens,
        final_grant: bool,
    ) -> Result<String, String> {
        let tx = self.browser_transaction(id).await?;
        {
            let _guard = tx.operation.lock().await;
            if let Some(outcome) = *tx.owned.lock().await {
                return Ok(format!("{outcome:?}"));
            }
            tx.stage(tokens.clone()).await?;
        }
        // Every SDK completion stages before releasing the source-call fence.
        // Initial success stays fenced through resource getAccessToken.
        if final_grant || tx.cancel.is_cancelled() {
            tx.source_active
                .store(false, std::sync::atomic::Ordering::SeqCst);
            tx.source_settled.notify_waiters();
        }
        if tx.cancel.is_cancelled() {
            tx.cleanup().await?;
            return Ok("Cancelled".into());
        }
        if final_grant {
            let result = tx.commit(tokens).await?;
            Ok(format!("{result:?}"))
        } else {
            Ok("Staged".into())
        }
    }
    fn spawn_incoming(
        self: &Arc<Self>,
        services: Arc<LocalServices>,
        mut incoming: mpsc::Receiver<Incoming>,
    ) {
        let state = self.clone();
        tokio::spawn(async move {
            let cancel = services.closed_token();
            loop {
                let message = tokio::select! { _=cancel.cancelled()=>break, message=incoming.recv()=>match message {Some(message)=>message,None=>break} };
                let mut s = state.inner.lock().await;
                if s.running
                    .as_ref()
                    .is_none_or(|r| !Arc::ptr_eq(&r.services, &services))
                {
                    break;
                }
                let peer = message
                    .peer_id
                    .as_ref()
                    .and_then(|id| s.devices.iter().find(|d| &d.id == id))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| {
                        message
                            .peer_id
                            .clone()
                            .unwrap_or_else(|| "本地业务端口".into())
                    });
                s.record_id += 1;
                let id = s.record_id;
                s.records.push(Record {
                    id,
                    time: timestamp(),
                    direction: "received".into(),
                    peer,
                    kind: message.kind.into(),
                    message: message.text,
                    bytes: message.bytes,
                    sha256: message.sha256,
                    transport: Some("本地业务服务".into()),
                });
                if s.records.len() > 100 {
                    s.records.remove(0);
                }
            }
        });
    }
    async fn accept_snapshot(&self, runtime: &Arc<Running>, snapshot: StateSnapshot) {
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, runtime))
            || runtime.cancel.is_cancelled()
        {
            return;
        }
        if s.sdk_snapshot
            .as_ref()
            .is_some_and(|old| snapshot.revision < old.revision)
        {
            return;
        }
        s.sdk_generation = snapshot.generation;
        if snapshot.coordination.phase != StatePhase::Ready {
            s.directory_known = false;
            s.devices.clear();
            if let Some(connection) = s.connection.as_mut() {
                connection.connected = false;
            }
        }
        s.phase = if snapshot.coordination.phase == StatePhase::Recovering {
            "recovering"
        } else {
            "ready"
        }
        .into();
        if s.remote_action != "enabling" && s.remote_action != "disabling" {
            s.remote_action = if snapshot.remote_enabled { "on" } else { "off" }.into();
        }
        s.local_closed = !snapshot.remote_enabled;
        s.sdk_snapshot = Some(snapshot);
    }
    fn spawn_runtime(self: &Arc<Self>, runtime: Arc<Running>) {
        let state = self.clone();
        let run = runtime.clone();
        let mut watch = runtime.client.watch_state();
        let mut events = runtime.client.subscribe();
        tokio::spawn(async move {
            let initial = watch.borrow_and_update().clone();
            state.accept_snapshot(&run, initial).await;
            loop {
                tokio::select! {
                    _=run.cancel.cancelled()=>break,
                    changed=watch.changed()=>{
                        if changed.is_err(){break;}
                        let snapshot=watch.borrow_and_update().clone();
                        state.accept_snapshot(&run,snapshot).await;
                    },
                    event=events.recv()=>match event {
                        Ok(event)=>{
                            let mut s=state.inner.lock().await;
                            if s.running.as_ref().is_none_or(|r|!Arc::ptr_eq(r,&run))||run.cancel.is_cancelled(){break;}
                            if event.generation<s.sdk_generation {continue;}
                            if event.phase=="mapping-disconnected" {if let Some(c)=s.connection.as_mut(){c.connected=false;}}
                            if event.phase=="mapping-ready" {if let Some(c)=s.connection.as_mut(){if event.peer_id.as_ref()==Some(&c.peer_id){c.connected=true;c.transport=event.transport.unwrap_or_default();}}}
                            if event.phase=="requires-login" {s.error=Some("协调授权不可用，请关闭远程后恢复授权或退出".into());}
                            if event.phase=="error" {s.error=Some("连接未完成，请检查协调与发布状态后重试".into());}
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>state.accept_snapshot(&run,run.client.snapshot()).await,
                        Err(_)=>break,
                    }
                }
            }
        });
        let state = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            tick.tick().await;
            loop {
                tokio::select! { _=runtime.cancel.cancelled()=>break, _=tick.tick()=>{} }
                let result = tokio::select! { _=runtime.cancel.cancelled()=>break, result=runtime.client.devices()=>result };
                let mapping = runtime.mapping.lock().await.clone();
                let path = match mapping {
                    Some(m) => m.current_transport().await,
                    None => None,
                };
                let mut s = state.inner.lock().await;
                if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime))
                    || runtime.cancel.is_cancelled()
                {
                    break;
                }
                if let Some(c) = s.connection.as_mut() {
                    c.connected = path.is_some();
                    if let Some(path) = path {
                        c.transport = path;
                    }
                }
                match result {
                    Ok(devices) => {
                        s.devices = devices;
                        s.directory_known = true;
                    }
                    Err(_) => {
                        s.devices.clear();
                        s.directory_known = false;
                    }
                }
            }
        });
    }
    pub async fn cancel_login(&self) -> Result<(), String> {
        if let Some(tx) = self.browser.lock().await.clone() {
            #[cfg(target_os = "android")]
            crate::android_browser::cancel(&tx.id).await?;
            tx.cleanup().await?;
        }
        let pending = {
            let mut s = self.inner.lock().await;
            let pending = s.pending.take();
            if let Some(p) = &pending {
                p.cancel.cancel();
                s.generation = p.generation + 1;
                s.phase = "signed_out".into();
                s.status = "已取消网页登录；已有本机授权仍可恢复或撤销".into();
            }
            pending
        };
        if let Some(mut pending) = pending {
            if let Some(task) = pending.task.take() {
                let mut retired = self.retired.lock().await;
                retired.retain(|t| !t.is_finished());
                retired.push(task);
            }
        }
        Ok(())
    }
    async fn runtime(&self) -> Result<Arc<Running>, String> {
        self.inner
            .lock()
            .await
            .running
            .clone()
            .ok_or_else(|| "请先登录".into())
    }
    pub async fn refresh_devices(&self) -> Result<(), String> {
        let runtime = self.runtime().await?;
        let result = tokio::select! {_=runtime.cancel.cancelled()=>return Err("远程已关闭，设备目录状态未知".into()),result=runtime.client.devices()=>result};
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime))
            || runtime.cancel.is_cancelled()
        {
            return Err("设备目录结果已失效".into());
        }
        match result {
            Ok(devices) => {
                s.devices = devices;
                s.directory_known = true;
                s.error = None;
                Ok(())
            }
            Err(_) => {
                s.devices.clear();
                s.directory_known = false;
                s.error = Some("设备目录暂不可用".into());
                Err("设备目录暂不可用".into())
            }
        }
    }
    fn require_remote(runtime: &Running) -> Result<(), String> {
        if runtime.cancel.is_cancelled() || !runtime.client.remote_access_enabled() {
            Err("远程访问已关闭，请明确开启后再连接或传输".into())
        } else {
            Ok(())
        }
    }
    pub async fn connect_device(&self, id: String) -> Result<(), String> {
        let runtime = self.runtime().await?;
        Self::require_remote(&runtime)?;
        let _op = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),lock=runtime.operation.lock()=>lock};
        let peer = {
            let s = self.inner.lock().await;
            s.devices
                .iter()
                .find(|d| {
                    d.id == id
                        && d.online
                        && d.id != runtime.device_id
                        && d.ports.iter().any(|p| p.id == protocol::MESSAGE_PORT)
                })
                .cloned()
                .ok_or_else(|| "请选择已发布消息端口的在线远端设备".to_string())?
        };
        if let Some(old) = runtime.mapping.lock().await.take() {
            old.close().await;
        }
        {
            let mut s = self.inner.lock().await;
            s.connection = None;
            s.error = None;
            s.status = "正在建立设备连接".into();
        }
        let mapping = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),m=runtime.client.map(&id,protocol::MESSAGE_PORT,"127.0.0.1:0".parse().unwrap())=>m.map_err(|e|e.to_string())?};
        let connection = Connection {
            peer_id: id,
            peer_name: peer.name,
            local_address: mapping.addr.to_string(),
            transport: mapping.current_transport().await.unwrap_or_default(),
            connected: true,
        };
        *runtime.mapping.lock().await = Some(Arc::new(mapping));
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_some_and(|r| Arc::ptr_eq(r, &runtime))
            && !runtime.cancel.is_cancelled()
        {
            s.connection = Some(connection);
            s.status = "已连接，发送消息后等待对端确认".into();
        }
        Ok(())
    }
    pub async fn disconnect_device(&self) -> Result<(), String> {
        let runtime = self.runtime().await?;
        let mapping = runtime.mapping.lock().await.take();
        if let Some(mapping) = mapping {
            mapping.close().await;
        }
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_some_and(|r| Arc::ptr_eq(r, &runtime)) {
            s.connection = None;
            s.status = "已断开当前设备连接".into();
        }
        Ok(())
    }
    pub async fn send_message(&self, text: String) -> Result<(), String> {
        let runtime = self.runtime().await?;
        Self::require_remote(&runtime)?;
        let _op = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),lock=runtime.operation.lock()=>lock};
        let mapping = runtime
            .mapping
            .lock()
            .await
            .clone()
            .ok_or_else(|| "请先连接远端设备".to_string())?;
        let peer = mapping.peer.device_id.clone();
        tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),result=protocol::send_message(mapping.addr,&runtime.device_id,&peer,&text)=>result?};
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime))
            || runtime.cancel.is_cancelled()
        {
            return Err("消息确认返回时会话已退出".into());
        }
        let peer = s
            .devices
            .iter()
            .find(|d| d.id == peer)
            .map(|d| d.name.clone())
            .unwrap_or(peer);
        s.record_id += 1;
        let id = s.record_id;
        s.records.push(Record {
            id,
            time: timestamp(),
            direction: "sent".into(),
            peer,
            kind: "message".into(),
            message: Some(text),
            bytes: None,
            sha256: None,
            transport: mapping.current_transport().await,
        });
        if s.records.len() > 100 {
            s.records.remove(0);
        }
        s.status = "对端已确认收到消息".into();
        Ok(())
    }
    pub async fn probe_device(&self, id: String) -> Result<(), String> {
        let runtime = self.runtime().await?;
        Self::require_remote(&runtime)?;
        let _op = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),lock=runtime.operation.lock()=>lock};
        let mapping = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),m=runtime.client.map(&id,protocol::ECHO_PORT,"127.0.0.1:0".parse().unwrap())=>m.map_err(|e|e.to_string())?};
        let result = tokio::select! {_=runtime.cancel.cancelled()=>Err("会话已关闭".into()),result=protocol::probe(mapping.addr)=>result};
        let transport = mapping
            .current_transport()
            .await
            .unwrap_or_else(|| "已断开".into());
        mapping.close().await;
        let (bytes, sha256) = result?;
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime))
            || runtime.cancel.is_cancelled()
        {
            return Err("校验完成时会话已退出".into());
        }
        let peer = s
            .devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or(id);
        s.record_id += 1;
        let id = s.record_id;
        s.records.push(Record {
            id,
            time: timestamp(),
            direction: "probe".into(),
            peer,
            kind: "probe".into(),
            message: None,
            bytes: Some(bytes),
            sha256: Some(sha256),
            transport: Some(transport),
        });
        if s.records.len() > 100 {
            s.records.remove(0);
        }
        s.status = "64 KiB 传输完成，字节数及 SHA256 一致".into();
        Ok(())
    }
    /// Start one owned operation so IPC/ADB remains available for priority close.
    pub async fn start_enable_remote(self: &Arc<Self>) -> Result<(), String> {
        use std::{future::Future, task::Poll};
        let expected = self.remote_enable_generation().await?;
        let state = self.clone();
        let mut operation = Box::pin(async move { state.enable_remote_generation(expected).await });
        let first = std::future::poll_fn(|cx| Poll::Ready(operation.as_mut().poll(cx))).await;
        match first {
            Poll::Ready(result) => result,
            Poll::Pending => {
                let task = tokio::spawn(async move {
                    let _ = operation.await;
                });
                let mut retired = self.retired.lock().await;
                retired.retain(|task| !task.is_finished());
                retired.push(task);
                Ok(())
            }
        }
    }
    #[cfg(test)]
    async fn enable_remote(self: &Arc<Self>) -> Result<(), String> {
        let expected = self.remote_enable_generation().await?;
        self.enable_remote_generation(expected).await
    }
    async fn remote_enable_generation(&self) -> Result<u64, String> {
        let s = self.inner.lock().await;
        if s.phase == "closing" || s.remote_action == "disabling" {
            return Err("正在安全关闭，请等待云端撤回结果后再开启".into());
        }
        Ok(s.remote_generation)
    }
    async fn enable_remote_generation(self: &Arc<Self>, expected: u64) -> Result<(), String> {
        let _single = match self.remote_operation.try_lock() {
            Ok(lock) => lock,
            Err(_) => return Ok(()), // Duplicate enable observes the same transition.
        };
        let (old, config, generation, user) = {
            let mut s = self.inner.lock().await;
            if s.remote_generation != expected {
                return Err("开启动作已被关闭或新的操作取代".into());
            }
            if s.phase == "closing" || s.remote_action == "disabling" {
                return Err("正在安全关闭，请等待云端撤回结果后再开启".into());
            }
            let run = s.running.clone().ok_or("请先登录")?;
            if run.client.remote_access_enabled() && !run.cancel.is_cancelled() {
                return Ok(());
            }
            s.remote_generation += 1;
            s.remote_action = "enabling".into();
            s.error = None;
            let user = s.local.as_ref().ok_or("本机身份不可用")?.user_id.clone();
            (run, s.config.clone(), s.remote_generation, user)
        };
        let reopening = old.cancel.is_cancelled();
        if reopening {
            // A completed close joins dispatched SDK authorization work. Before
            // creating another Client, also settle any retained checkpoint owner;
            // sharing an Arc alone is not permission to consume an older grant.
            let owners = self.grant_owners.lock().await.clone();
            for owner in &owners {
                if owner.retry_pending_grant().await.is_err() {
                    let message = "最新授权尚未安全保存，请恢复存储后重试开启";
                    let mut s = self.inner.lock().await;
                    if s.remote_generation == generation {
                        s.remote_action = "off".into();
                        s.error = Some(message.into());
                    }
                    return Err(message.into());
                }
            }
            self.grant_owners
                .lock()
                .await
                .retain(|owner| !owners.iter().any(|done| Arc::ptr_eq(owner, done)));
        }
        let client = if reopening {
            match self.create_client(&config) {
                Ok(client) => client,
                Err(_) => {
                    let mut s = self.inner.lock().await;
                    if s.remote_generation == generation {
                        s.remote_action = "off".into();
                        s.error = Some("无法准备新的设备会话，请检查配置".into());
                    }
                    return Err("无法准备新的设备会话，请检查配置".into());
                }
            }
        } else {
            old.client.clone()
        };
        {
            let mut s = self.inner.lock().await;
            if s.remote_generation != generation {
                return Err("开启动作已被关闭取代".into());
            }
            s.opening = Some(client.clone());
        }
        let result: Result<Arc<Running>, String> = async {
            let run = if reopening {
                // Reuse the actual provider owner (CredentialStore clones share
                // auth_gate, pending rotation and file lease), never a second store.
                client.set_auth_provider(old.owner.clone()).await;
                let session = client
                    .login_with_auth_provider(&config.device_name)
                    .await
                    .map_err(|_| "本机授权恢复未完成，请检查授权状态后重试".to_string())?;
                if session.user.id != user {
                    return Err("恢复后的账号与原授权不一致".into());
                }
                for port in old.client.port_intents().await {
                    client
                        .expose(port)
                        .await
                        .map_err(|_| "无法恢复本地端口意图".to_string())?;
                }
                let run = Arc::new(Running {
                    client: client.clone(),
                    owner: old.owner.clone(),
                    services: old.services.clone(),
                    cancel: CancellationToken::new(),
                    mapping: Mutex::new(None),
                    operation: Mutex::new(()),
                    device_id: session.device_id.clone(),
                });
                {
                    let mut s = self.inner.lock().await;
                    if s.remote_generation != generation || s.remote_action != "enabling" {
                        return Err("开启动作已被关闭取代".into());
                    }
                    // Update ACK identity without replacing listeners or active local TCP.
                    run.services
                        .update_device_id(session.device_id.clone())
                        .await;
                    if let Some(local) = s.local.as_mut() {
                        local.device_id = session.device_id;
                    }
                    s.sdk_snapshot = Some(client.snapshot());
                    s.sdk_generation = 0;
                    s.running = Some(run.clone());
                    s.devices.clear();
                    s.directory_known = false;
                }
                self.spawn_runtime(run.clone());
                run
            } else {
                old.clone()
            };
            {
                let s = self.inner.lock().await;
                if s.remote_generation != generation || s.remote_action != "enabling" {
                    return Err("开启动作已被关闭取代".into());
                }
            }
            client
                .enable_remote()
                .await
                .map_err(|_| "远程开启尚未获服务确认，请查看发布状态".to_string())?;
            Ok(run)
        }
        .await;
        let mut s = self.inner.lock().await;
        if s.remote_generation != generation || s.remote_action != "enabling" {
            drop(s);
            let outcome = client.disable_remote().await;
            if outcome.coordinator_revocation == CoordinatorRevocation::Pending {
                let mut clients = self.disabled_clients.lock().await;
                if !clients.iter().any(|old| Arc::ptr_eq(old, &client)) {
                    clients.push(client.clone());
                }
            }
            if client.has_pending_grant().await {
                self.grant_owners.lock().await.push(client);
            }
            return Err("开启动作已被关闭取代".into());
        }
        s.opening = None;
        s.sdk_snapshot = Some(client.snapshot());
        s.local_closed = !client.remote_access_enabled();
        s.remote_action = if client.remote_access_enabled() {
            "on"
        } else {
            "off"
        }
        .into();
        s.coordinator_revocation = None;
        match result {
            Ok(_) => {
                s.status = "远程访问已开启；端口确认状态由 SDK 快照显示".into();
                s.error = None;
                Ok(())
            }
            Err(message) => {
                s.error = Some(message.clone());
                s.status = "开启未完成，本地业务服务继续可用".into();
                // An uninstalled candidate must remain privately owned for safe
                // checkpoint/revocation. No failed login future is dropped.
                let installed = s
                    .running
                    .as_ref()
                    .is_some_and(|r| Arc::ptr_eq(&r.client, &client));
                drop(s);
                if !installed {
                    let outcome = client.disable_remote().await;
                    if outcome.coordinator_revocation == CoordinatorRevocation::Pending {
                        self.disabled_clients.lock().await.push(client.clone());
                    }
                    if client.has_pending_grant().await {
                        self.grant_owners.lock().await.push(client);
                    }
                }
                Err(message)
            }
        }
    }
    pub async fn disable_remote(&self) -> Result<RemoteDisableOutcome, String> {
        use std::{future::Future, task::Poll};
        let (run, unique, generation) = {
            let mut s = self.inner.lock().await;
            let run = s.running.clone().ok_or("请先登录")?;
            s.remote_generation += 1;
            s.remote_action = "disabling".into();
            let mut clients = self.disabled_clients.lock().await;
            clients.push(run.client.clone());
            if let Some(client) = s.opening.take() {
                clients.push(client);
            }
            let mut unique: Vec<Arc<Client>> = Vec::new();
            for client in clients.iter() {
                if !unique.iter().any(|old| Arc::ptr_eq(old, client)) {
                    unique.push(client.clone());
                }
            }
            *clients = unique.clone();
            (run, unique, s.remote_generation)
        };
        // Poll every close once before awaiting any cloud result. The SDK closes
        // its atomic gate/lifetime before its first await, including candidates.
        let mut closing = Vec::new();
        for client in &unique {
            let mut future = Box::pin(client.disable_remote());
            let first = std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await;
            closing.push((future, first));
        }
        run.cancel.cancel();
        {
            let mut s = self.inner.lock().await;
            if s.remote_generation == generation {
                s.sdk_snapshot = Some(run.client.snapshot());
                s.local_closed = true;
                s.coordinator_revocation = Some(CoordinatorRevocation::Pending);
                s.devices.clear();
                s.directory_known = false;
                s.connection = None;
                s.status = "本机远程访问已关闭，本地服务可用；正在确认云端撤回".into();
                s.error = None;
            }
        }
        let mut pending = Vec::new();
        for (client, (future, first)) in unique.iter().zip(closing) {
            let outcome = match first {
                Poll::Ready(outcome) => outcome,
                Poll::Pending => future.await,
            };
            if outcome.coordinator_revocation == CoordinatorRevocation::Pending {
                pending.push(client.clone());
            }
            if client.has_pending_grant().await {
                let mut owners = self.grant_owners.lock().await;
                if !owners.iter().any(|old| Arc::ptr_eq(old, client)) {
                    owners.push(client.clone());
                }
            }
        }
        // The first SDK poll may already own the peer's RTC cleanup lock.
        // Finish driving those owned futures before Mapping::close joins the
        // same peer; awaiting the mapping first would strand its SDK owner.
        if let Some(mapping) = run.mapping.lock().await.take() {
            mapping.close().await;
        }
        let outcome = RemoteDisableOutcome {
            local_closed: true,
            coordinator_revocation: if pending.is_empty() {
                CoordinatorRevocation::Confirmed
            } else {
                CoordinatorRevocation::Pending
            },
        };
        let mut s = self.inner.lock().await;
        if s.remote_generation == generation {
            *self.disabled_clients.lock().await = pending;
            s.remote_action = "off".into();
            s.local_closed = true;
            s.coordinator_revocation = Some(outcome.coordinator_revocation);
            s.sdk_snapshot = Some(run.client.snapshot());
            s.status = "本机远程访问已关闭；本地服务和本机授权保留".into();
        }
        Ok(outcome)
    }
    pub async fn shutdown(&self, revoke: bool) -> Result<(), String> {
        self.shutdown_with_wait(revoke, Duration::from_secs(45))
            .await
    }
    async fn shutdown_with_wait(&self, revoke: bool, wait: Duration) -> Result<(), String> {
        if self.inner.lock().await.running.is_some() {
            let _ = self.disable_remote().await;
        }
        if let Some(tx) = self.browser.lock().await.clone() {
            #[cfg(target_os = "android")]
            crate::android_browser::cancel(&tx.id).await?;
            tx.cleanup_with_revoke(revoke).await?;
        }
        browser::recover(&self.data_dir).await?;
        let (runtime, config, pending) = {
            let mut s = self.inner.lock().await;
            if s.phase == "closing" {
                return Err("正在关闭本机连接，请稍候".into());
            }
            let pending = s.pending.take();
            if let Some(p) = &pending {
                p.cancel.cancel();
            }
            s.generation += 1;
            s.remote_generation += 1;
            s.phase = "closing".into();
            s.status = "正在关闭本机连接".into();
            (s.running.take(), s.config.clone(), pending)
        };
        let mut tasks: Vec<_> = self.retired.lock().await.drain(..).collect();
        if let Some(mut pending) = pending {
            if let Some(task) = pending.task.take() {
                tasks.push(task);
            }
        }
        let mut errors = vec![];
        let mut unfinished = vec![];
        for mut task in tasks {
            match tokio::time::timeout(wait, &mut task).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => errors.push("授权清理任务异常结束，请检查本机授权状态".to_string()),
                Err(_) => unfinished.push(task),
            }
        }
        let pending_authorization = !unfinished.is_empty();
        if pending_authorization {
            self.retired.lock().await.extend(unfinished);
            errors.push(
                "待取消的授权仍在安全保存中；完成前禁止重新登录或撤销，请稍后重试退出".into(),
            );
        }
        let credentials = CredentialStore::new(credential_path(&self.data_dir, &config));
        let mut auth_client = None;
        if let Some(runtime) = runtime {
            runtime.cancel.cancel();
            runtime.services.close().await;
            let mapping = runtime.mapping.lock().await.take();
            if let Some(mapping) = mapping {
                mapping.close().await;
            }
            if let Err(e) = runtime.client.close().await {
                errors.push(format!("本地连接已关闭，中央注销未确认：{e}"));
            }
            if runtime.client.has_pending_grant().await {
                self.grant_owners.lock().await.push(runtime.client.clone());
            }
            auth_client = Some(runtime.client.clone());
        }
        {
            let mut owners = self.grant_owners.lock().await;
            let mut kept = Vec::new();
            for owner in owners.drain(..) {
                if (if revoke && !pending_authorization {
                    owner.revoke_pending_grant().await
                } else {
                    owner.retry_pending_grant().await
                })
                .is_err()
                {
                    kept.push(owner);
                    errors.push("最新授权尚未保存，私有所有者已保留；暂不撤销或退出进程".into());
                }
            }
            *owners = kept;
        }
        // A late rotation must be persisted before revocation chooses its token.
        // Retaining the handle also keeps start_login's exclusion effective.
        if revoke
            && !pending_authorization
            && self.grant_owners.lock().await.is_empty()
            && credentials.exists()
        {
            let client = match auth_client {
                Some(client) => Ok(client),
                None => Client::new(Config {
                    base_url: config.base_url.clone(),
                    app_id: config.app_id.clone(),
                    ..Config::default()
                })
                .map_err(|e| e.to_string()),
            };
            match client {
                Ok(client) => {
                    if let Err(e) = credentials.logout(&client).await {
                        errors.push(format!("授权撤销失败，私有凭据已保留：{e}"));
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let mut s = self.inner.lock().await;
        s.phase = "signed_out".into();
        s.local = None;
        s.sdk_snapshot = None;
        s.remote_action = "off".into();
        s.local_closed = true;
        s.directory_known = false;
        s.devices.clear();
        s.records.clear();
        s.connection = None;
        s.status = if errors.is_empty() && revoke {
            "已退出并撤销本机授权".into()
        } else if errors.is_empty() {
            "本次会话已关闭，本机授权保留供下次恢复".into()
        } else {
            "本地连接已关闭，但部分注销操作未确认".into()
        };
        s.error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        };
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
}

#[cfg(test)]
mod browser_setup_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn browser_boundary_existing_authorization_exits_setup_without_network_or_transaction() {
        let root = std::env::temp_dir().join(format!(
            "cybergrid-existing-authorization-{}",
            hex::encode(rand::random::<[u8; 12]>())
        ));
        let state = LabState::new(root.join("data"), root.join("cache")).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = LabConfig {
            base_url: format!("http://{}", listener.local_addr().unwrap()),
            app_id: "fixture-native".into(),
            cloudflared_path: "/usr/bin/true".into(),
            ..Default::default()
        };
        // Only existence matters here. No credential parsing, refresh, overwrite,
        // revocation, callback listener, or browser transaction may be attempted.
        let path = credential_path(&state.data_dir, &config);
        let original = b"fixture-existing-authorization-not-parsed";
        std::fs::write(&path, original).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let result = tokio::time::timeout(Duration::from_secs(2), state.begin_web_login(config))
            .await
            .expect("existing authorization must fail before any HTTP request");
        assert!(matches!(
            result,
            Err(BrowserStartError::ExistingAuthorization)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
        assert!(state.browser.lock().await.is_none());
        let inner = state.inner.lock().await;
        assert!(inner.pending.is_none());
        assert_eq!(inner.phase, "signed_out");
        assert_eq!(
            inner.status,
            BrowserStartError::ExistingAuthorization.message()
        );
        assert_eq!(inner.error.as_deref(), Some(inner.status.as_str()));
        drop(inner);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            serde_json::to_value(BrowserStartError::ExistingAuthorization).unwrap(),
            "EXISTING_AUTHORIZATION"
        );
        assert_eq!(
            serde_json::to_value(BrowserStartError::from("untrusted-provider-detail")).unwrap(),
            "FAILED"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn browser_boundary_setup_cancel_fences_native_before_any_browser_start() {
        let root = std::env::temp_dir().join(format!(
            "cybergrid-browser-setup-{}",
            hex::encode(rand::random::<[u8; 12]>())
        ));
        let state = LabState::new(root.join("data"), root.join("cache")).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (observed, seen) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let serve = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0; 8192];
            assert!(s.read(&mut buf).await.unwrap() > 0);
            observed.send(()).unwrap();
            released.await.unwrap();
            let body=serde_json::json!({"provider":"logto","clientId":"fixture-native","issuer":"https://login.example.test/oidc","resource":"https://example.test/api","scope":"openid offline_access connect","deviceAuthorizationEndpoint":"https://login.example.test/oidc/device/auth","tokenEndpoint":"https://login.example.test/oidc/token","revocationEndpoint":"https://login.example.test/oidc/token/revocation"}).to_string();
            let h=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
            s.write_all(h.as_bytes()).await.unwrap();
        });
        let task = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .begin_web_login(LabConfig {
                        base_url: base,
                        app_id: "fixture-native".into(),
                        cloudflared_path: "/usr/bin/true".into(),
                        ..Default::default()
                    })
                    .await
            })
        };
        seen.await.unwrap();
        assert_eq!(state.view().await.phase, "authorizing");
        state.cancel_login().await.unwrap();
        release.send(()).unwrap();
        assert!(task.await.unwrap().is_err());
        serve.await.unwrap();
        assert!(state.browser.lock().await.is_none());
        assert!(state.inner.lock().await.pending.is_none());
        assert_eq!(state.view().await.phase, "signed_out");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
#[path = "state_remote_tests.rs"]
mod remote_tests;
