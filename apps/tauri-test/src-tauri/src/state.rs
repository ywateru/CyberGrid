use crate::{
    platform, protocol,
    services::{Incoming, LocalServices},
};
use cybergrid_sdk::{Client, Config, CredentialStore, Device, Mapping, Port};
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
    pub cloudflared_path: String,
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
        let error = state.start_login(config, false).await.unwrap_err();
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
            app_id: String::new(),
            device_name: if cfg!(target_os = "android") {
                "Android".into()
            } else {
                "Mac".into()
            },
            force_fallback: false,
            cloudflared_path: String::new(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicAuth {
    verification_uri: String,
    user_code: String,
    expires_at: String,
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
    config: LabConfig,
    can_resume: bool,
    error: Option<String>,
    authorization: Option<PublicAuth>,
    local: Option<LocalDevice>,
    devices: Vec<PublicDevice>,
    records: Vec<Record>,
    connection: Option<Connection>,
    status: String,
}
struct Pending {
    cancel: CancellationToken,
    generation: u64,
    task: Option<tokio::task::JoinHandle<()>>,
}
struct Running {
    client: Arc<Client>,
    services: Arc<LocalServices>,
    cancel: CancellationToken,
    mapping: Mutex<Option<Arc<Mapping>>>,
    operation: Mutex<()>,
    device_id: String,
}
struct Inner {
    phase: String,
    config: LabConfig,
    error: Option<String>,
    authorization: Option<PublicAuth>,
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
}
pub struct LabState {
    inner: Mutex<Inner>,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    retired: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    grant_owners: Mutex<Vec<Arc<Client>>>,
    pub exiting: AtomicBool,
}
fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "时间不可用".into())
}
fn credential_path(data: &Path, config: &LabConfig) -> PathBuf {
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
                config,
                error: None,
                authorization: None,
                local: None,
                devices: vec![],
                records: vec![],
                connection: None,
                status: "使用同一个账号与应用，在两端分别登录".into(),
                generation: 0,
                pending: None,
                running: None,
                record_id: 0,
                sdk_generation: 0,
            }),
            data_dir,
            cache_dir,
            retired: Mutex::new(Vec::new()),
            grant_owners: Mutex::new(Vec::new()),
            exiting: AtomicBool::new(false),
        }))
    }
    pub async fn view(&self) -> View {
        let s = self.inner.lock().await;
        View {
            phase: s.phase.clone(),
            config: s.config.clone(),
            can_resume: credential_path(&self.data_dir, &s.config).exists(),
            error: s.error.clone(),
            authorization: s.authorization.clone(),
            local: s.local.clone(),
            devices: s.devices.iter().map(PublicDevice::from).collect(),
            records: s.records.clone(),
            connection: s.connection.clone(),
            status: s.status.clone(),
        }
    }
    fn create_client(&self, config: &LabConfig) -> Result<Arc<Client>, String> {
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
        let c = Config {
            base_url: config.base_url.clone(),
            app_id: config.app_id.clone(),
            direct_timeout: if config.force_fallback {
                Duration::from_nanos(1)
            } else {
                Duration::from_secs(12)
            },
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
            s.phase = if resume {
                "connecting".into()
            } else {
                "authorizing".into()
            };
            s.error = None;
            s.records.clear();
            s.sdk_generation = 0;
            s.authorization = None;
            s.status = if resume {
                "正在恢复本机授权".into()
            } else {
                "正在取得设备授权码".into()
            };
            generation
        };
        let state = self.clone();
        let task = tokio::spawn(async move {
            state
                .login_worker(config, client, credentials, cancel, generation, resume)
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
        resume: bool,
    ) {
        let binding = client
            .use_credential_store(credentials.clone())
            .await
            .map_err(|error| error.to_string());
        client
            .set_auth_provider(Arc::new(credentials.clone()))
            .await;
        let flow = async {
            binding?;
            let tokens = if resume {
                credentials
                    .refresh(&client)
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                let auth = client
                    .begin_device_authorization()
                    .await
                    .map_err(|e| e.to_string())?;
                {
                    let mut s = self.inner.lock().await;
                    if s.generation != generation {
                        return Err("登录已取消".into());
                    }
                    s.authorization = Some(PublicAuth {
                        verification_uri: auth
                            .verification_uri_complete
                            .clone()
                            .unwrap_or_else(|| auth.verification_uri.clone()),
                        user_code: auth.user_code.clone(),
                        expires_at: (time::OffsetDateTime::now_utc()
                            + time::Duration::seconds(auth.expires_in.min(i64::MAX as u64) as i64))
                        .format(&time::format_description::well_known::Rfc3339)
                        .map_err(|_| "授权过期时间无效".to_string())?,
                    });
                    s.status = "请在 Logto 托管页面确认设备码并登录".into();
                }
                let tokens = client
                    .complete_device_authorization(&auth, &cancel)
                    .await
                    .map_err(|e| e.to_string())?;
                tokens
            };
            {
                let mut s = self.inner.lock().await;
                if s.generation != generation || cancel.is_cancelled() {
                    return Err("登录已取消，本机授权可在下次恢复或撤销".into());
                }
                s.phase = "connecting".into();
                s.authorization = None;
                s.status = "授权成功，正在建立本机设备会话".into();
            }
            let session = client
                .login_with_access_token(&tokens.access_token, &config.device_name)
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
                services,
                cancel: CancellationToken::new(),
                mapping: Mutex::new(None),
                operation: Mutex::new(()),
                device_id: session.device_id.clone(),
            });
            {
                let mut s = self.inner.lock().await;
                if s.generation != generation || cancel.is_cancelled() {
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
                s.status = "已登录，正在准备端到端连接入口".into();
                s.error = None;
            }
            self.spawn_runtime(runtime, rx);
            Ok::<(), String>(())
        };
        // Device Flow observes cancellation itself. Never drop an in-flight
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
                s.authorization = None;
                s.status = "登录未完成；已有授权文件会保留，避免丢失撤销材料".into();
                s.error = if cancel.is_cancelled() {
                    None
                } else {
                    Some(error)
                };
            }
        }
    }
    fn spawn_runtime(
        self: &Arc<Self>,
        runtime: Arc<Running>,
        mut incoming: mpsc::Receiver<Incoming>,
    ) {
        let mut events = runtime.client.subscribe();
        let state = self.clone();
        let run = runtime.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _=run.cancel.cancelled()=>break,
                    Some(message)=incoming.recv()=>{
                        let mut s=state.inner.lock().await;
                        if s.running.as_ref().is_none_or(|r|!Arc::ptr_eq(r,&run)){break;}
                        let peer=message.peer_id.as_ref().and_then(|id|s.devices.iter().find(|d|&d.id==id)).map(|d|d.name.clone()).unwrap_or_else(||message.peer_id.clone().unwrap_or_else(||"远端设备".into()));
                        s.record_id+=1;let id=s.record_id;
                        s.records.push(Record{id,time:timestamp(),direction:"received".into(),peer,kind:message.kind.into(),message:message.text,bytes:message.bytes,sha256:message.sha256,transport:Some("已认证的端到端端口".into())});
                        if s.records.len()>100{s.records.remove(0);}
                    },
                    event=events.recv()=>match event {
                        Ok(event)=>{
                            {let s=state.inner.lock().await;if event.generation<s.sdk_generation{continue;}}
                            if event.phase=="requires-login" {state.lose_session(&run,"身份或应用授权已失效，请重新登录；本机授权保留".into()).await;break;}
                            let mut s=state.inner.lock().await;
                            if s.running.as_ref().is_none_or(|r|!Arc::ptr_eq(r,&run)){break;}
                            s.sdk_generation=event.generation;
                            if event.phase=="recovering" {s.phase="recovering".into();for device in &mut s.devices {device.online=false;}if let Some(connection)=s.connection.as_mut(){connection.connected=false;connection.transport="等待新代际通道".into();}}
                            if event.phase=="coordination-ready" {s.phase="ready".into();s.error=None;}
                            if event.phase=="mapping-disconnected" {if let Some(connection)=s.connection.as_mut(){connection.connected=false;}}
                            if event.phase=="mapping-ready" {if let Some(connection)=s.connection.as_mut(){if event.peer_id.as_ref()==Some(&connection.peer_id){connection.connected=true;connection.transport=event.transport.clone().unwrap_or_default();}}}
                            s.status=match event.phase.as_str(){
                                "recovering"=>"协调服务已失效，正在自动恢复；旧 TCP 流已中断，业务字节不会重放".into(),
                                "coordination-ready"=>"新代际已注册，端口已重新发布；本地映射等待新的连接".into(),
                                "mapping-disconnected"=>"远端通道已断开，本地映射意图保留".into(),
                                "mapping-ready"=>"本地映射已建立新的加密通道".into(),
                                "provider-restarting"=>"匿名隧道异常，正在有限重启".into(),
                                "provider-failed"=>"匿名隧道重启次数已用完；直连仍可尝试".into(),
                                "direct-attempt"=>"正在尝试 WebRTC 直连".into(),
                                "fallback-attempt"=>"直连未建立，正在尝试 Cloudflare 回退".into(),
                                "tunnel-ready"=>"匿名隧道已就绪，可以接受另一端连接".into(),
                                "port-open"=>"远端端口已打开".into(),
                                "error"=>"连接发生错误，请查看提示".into(),
                                "session-ready"=>"本机设备会话已就绪".into(),
                                "multiplexer-ready"=>"端到端加密通道已就绪".into(),
                                "mutual-tls-ready"=>"双向 TLS 身份认证通过".into(),
                                _=>"连接状态已更新".into()
                            };
                            if event.phase=="error"{s.error=event.message;}
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{state.inner.lock().await.error=Some("连接状态事件过多，请刷新设备状态".into());},
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
                tokio::select! {
                    _=runtime.cancel.cancelled()=>break,
                    _=tick.tick()=>{
                        let result=runtime.client.devices().await;

                        let mapping=runtime.mapping.lock().await.clone();
                        let path=match mapping {Some(m)=>m.current_transport().await,None=>None};
                        let mut s=state.inner.lock().await;
                        if s.running.as_ref().is_none_or(|r|!Arc::ptr_eq(r,&runtime)){break;}
                        if let Some(connection)=s.connection.as_mut(){connection.connected=path.is_some();if let Some(path)=path{connection.transport=path;}}
                        match result {Ok(devices)=>s.devices=devices,Err(error)=>{if s.phase!="recovering"{s.error=Some(error.to_string());s.status="设备刷新失败，等待协调状态更新".into();}}}
                    }
                }
            }
        });
    }
    async fn lose_session(&self, runtime: &Arc<Running>, message: String) {
        let current = {
            let mut s = self.inner.lock().await;
            if s.running.as_ref().is_some_and(|r| Arc::ptr_eq(r, runtime)) {
                s.running = None;
                s.phase = "signed_out".into();
                s.local = None;
                s.devices.clear();
                s.connection = None;
                s.error = Some(message);
                s.status = "本机授权保留，可恢复登录或撤销".into();
                true
            } else {
                false
            }
        };
        if current {
            runtime.cancel.cancel();
            runtime.services.close().await;
            let mapping = runtime.mapping.lock().await.take();
            if let Some(mapping) = mapping {
                mapping.close().await;
            }
            let _ = runtime.client.close().await;
            if runtime.client.has_pending_grant().await {
                self.grant_owners.lock().await.push(runtime.client.clone());
            }
        }
    }
    pub async fn cancel_login(&self) -> Result<(), String> {
        let pending = {
            let mut s = self.inner.lock().await;
            let pending = s.pending.take();
            if let Some(p) = &pending {
                p.cancel.cancel();
                s.generation = p.generation + 1;
                s.phase = "signed_out".into();
                s.authorization = None;
                s.status = "已取消设备授权；已有本机授权仍可恢复或撤销".into();
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
        let devices = tokio::select! {_=runtime.cancel.cancelled()=>return Err("会话已关闭".into()),d=runtime.client.devices()=>d.map_err(|e|e.to_string())?};
        let mut s = self.inner.lock().await;
        if s.running.as_ref().is_some_and(|r| Arc::ptr_eq(r, &runtime)) {
            s.devices = devices;
            s.error = None;
            s.status = "设备列表已刷新".into();
        }
        Ok(())
    }
    pub async fn connect_device(&self, id: String) -> Result<(), String> {
        let runtime = self.runtime().await?;
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
        if s.running.as_ref().is_some_and(|r| Arc::ptr_eq(r, &runtime)) {
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
        s.connection = None;
        s.status = "已断开当前设备连接".into();
        Ok(())
    }
    pub async fn send_message(&self, text: String) -> Result<(), String> {
        let runtime = self.runtime().await?;
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
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime)) {
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
        if s.running.as_ref().is_none_or(|r| !Arc::ptr_eq(r, &runtime)) {
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
    pub async fn shutdown(&self, revoke: bool) -> Result<(), String> {
        self.shutdown_with_wait(revoke, Duration::from_secs(45))
            .await
    }
    async fn shutdown_with_wait(&self, revoke: bool, wait: Duration) -> Result<(), String> {
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
            s.phase = "closing".into();
            s.authorization = None;
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
