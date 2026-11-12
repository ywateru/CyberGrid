//! Transaction-scoped official-SDK bridge. OAuth journals are private cleanup
//! owners, never refresh credentials; only SDK validated import transfers owner.
use crate::state::LabConfig;
use cybergrid_sdk::{AuthConfig, Client, GrantImportOutcome, OAuthTokens};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{oneshot, Mutex};
#[cfg(any(not(target_os = "android"), test))]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;
use url::Url;

const LIMIT: usize = 1024 * 1024;
const CALLBACK: &str = "http://127.0.0.1:17896/callback";
pub fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else {
        "desktop"
    }
}
fn failure() -> String {
    "浏览器授权未完成；私有授权材料会保留供安全清理".into()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tokens {
    #[serde(default)]
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub scope: String,
}
impl Tokens {
    pub fn oauth(&self) -> Result<OAuthTokens, String> {
        if self.access_token.is_empty()
            || self.refresh_token.is_empty()
            || self.access_token.len() > LIMIT
            || self.refresh_token.len() > LIMIT
            || !self.token_type.eq_ignore_ascii_case("Bearer")
            || self.expires_in == 0
            || self.expires_in > 86400 * 30
        {
            return Err(failure());
        }
        Ok(OAuthTokens {
            access_token: self.access_token.clone(),
            refresh_token: self.refresh_token.clone(),
            expires_in: self.expires_in,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(self.expires_in as i64),
            token_type: self.token_type.clone(),
            scope: self.scope.clone(),
        })
    }
}
#[derive(Serialize, Deserialize)]
struct Journal {
    version: u8,
    config: LabConfig,
    expected: AuthConfig,
    tokens: Tokens,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Begin {
    pub transaction_id: String,
    pub generation: u64,
    pub platform: &'static str,
    pub endpoint: String,
    pub issuer: String,
    pub app_id: String,
    pub resource: String,
    pub scope: String,
    pub redirect_uri: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}
#[derive(Serialize)]
pub struct FetchResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: String,
}
pub struct Transaction {
    pub id: String,
    pub config: LabConfig,
    pub expected: AuthConfig,
    pub client: Arc<Client>,
    pub cancel: CancellationToken,
    pub operation: Mutex<()>,
    pub source_active: std::sync::atomic::AtomicBool,
    pub source_settled: tokio::sync::Notify,
    latest: Mutex<Option<Tokens>>,
    journal: PathBuf,
    pub finish: Mutex<Option<oneshot::Sender<Option<String>>>>,
    callback: Mutex<Option<oneshot::Receiver<String>>>,
    pub owned: Mutex<Option<GrantImportOutcome>>,
    lease: Mutex<Option<std::fs::File>>,
    http: reqwest::Client,
    discovery: Mutex<Option<serde_json::Value>>,
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", hex::encode(rand::random::<[u8; 8]>())));
    let mut o = std::fs::OpenOptions::new();
    o.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let result = (|| {
        let mut f = o.open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&temp, path)?;
        std::fs::File::open(path.parent().unwrap())?.sync_all()
    })()
    .map_err(|_| failure());
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
pub fn write_diagnostic(data: &Path, value: &impl Serialize) -> Result<(), String> {
    atomic(
        &data.join("browser-diagnostic.json"),
        &serde_json::to_vec(value).map_err(|_| failure())?,
    )
}
pub fn journal_path(data: &Path) -> PathBuf {
    data.join("browser-grant-pending.json")
}
fn journal_lock(data: &Path) -> Result<std::fs::File, String> {
    use fs2::FileExt;
    let path = data.join("browser-grant.lock");
    let mut o = std::fs::OpenOptions::new();
    o.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = match o.open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            o.create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                o.mode(0o600);
            }
            match o.open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    o.create_new(false);
                    o.open(&path).map_err(|_| failure())?
                }
                Err(_) => return Err(failure()),
            }
        }
        Err(_) => return Err(failure()),
    };
    let m = f.metadata().map_err(|_| failure())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o600 {
            return Err(failure());
        }
    }
    f.try_lock_exclusive()
        .map_err(|_| "另一个进程正在处理浏览器授权，请等待其安全结束".to_string())?;
    Ok(f)
}
pub async fn recover(data: &Path) -> Result<(), String> {
    let _lease = journal_lock(data)?;
    let path = journal_path(data);
    if !path.exists() {
        return Ok(());
    }
    let bytes = read_private(&path)?;
    let j: Journal = serde_json::from_slice(&bytes).map_err(|_| failure())?;
    if j.version != 1 {
        return Err(failure());
    }
    let c = Client::new(cybergrid_sdk::Config {
        base_url: j.config.base_url.clone(),
        app_id: j.config.app_id.clone(),
        ..Default::default()
    })
    .map_err(|_| failure())?;
    c.use_credential_store(cybergrid_sdk::CredentialStore::new(
        crate::state::credential_path(data, &j.config),
    ))
    .await
    .map_err(|_| failure())?;
    match c
        .browser_grant_checkpoint(&j.expected, &j.tokens.refresh_token)
        .await
        .map_err(|_| failure())?
    {
        cybergrid_sdk::BrowserGrantCheckpoint::Matching => {
            std::fs::remove_file(path).map_err(|_| failure())?;
            return Ok(());
        }
        cybergrid_sdk::BrowserGrantCheckpoint::OtherGrant => {
            return Err(
                "浏览器待清理记录与本机授权关系不明确；已保留两者，禁止自动撤销或新授权".into(),
            )
        }
        cybergrid_sdk::BrowserGrantCheckpoint::Empty => (),
    }
    let current = c.auth_config().await.map_err(|_| failure())?;
    if !current
        .same_client_binding(&j.expected, &j.config.app_id)
        .map_err(|_| failure())?
    {
        return Err("待清理授权的可信配置已改变；保留私有文件并禁止新授权".into());
    }
    c.revoke_refresh_token(&j.tokens.refresh_token)
        .await
        .map_err(|_| failure())?;
    std::fs::remove_file(path).map_err(|_| failure())?;
    Ok(())
}
fn read_private(path: &Path) -> Result<Vec<u8>, String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = opts.open(path).map_err(|_| failure())?;
    let m = f.metadata().map_err(|_| failure())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o600 {
            return Err(failure());
        }
    }
    if !m.is_file() || m.len() > LIMIT as u64 {
        return Err(failure());
    }
    use std::io::Read;
    let mut b = Vec::new();
    f.take(LIMIT as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| failure())?;
    Ok(b)
}
impl Transaction {
    pub async fn new(
        config: LabConfig,
        expected: AuthConfig,
        client: Arc<Client>,
        data: &Path,
        generation: u64,
        cancel: CancellationToken,
        finish: oneshot::Sender<Option<String>>,
    ) -> Result<(Arc<Self>, Begin), String> {
        let lease = journal_lock(data)?;
        if journal_path(data).exists() {
            return Err("存在待清理的浏览器授权，请先恢复安全清理".into());
        }
        let issuer = Url::parse(&expected.issuer).map_err(|_| failure())?;
        let endpoint = issuer.origin().ascii_serialization();
        let (callback_tx, callback_rx) = oneshot::channel();
        #[cfg(not(target_os = "android"))]
        let listener = TcpListener::bind("127.0.0.1:17896")
            .await
            .map_err(|_| "本机回调端口17896已占用，请释放端口后重试网页登录".to_string())?;
        let t = Arc::new(Self {
            id: hex::encode(rand::random::<[u8; 24]>()),
            config: config.clone(),
            expected: expected.clone(),
            client,
            cancel,
            operation: Mutex::new(()),
            source_active: std::sync::atomic::AtomicBool::new(false),
            source_settled: tokio::sync::Notify::new(),
            latest: Mutex::new(None),
            journal: journal_path(data),
            finish: Mutex::new(Some(finish)),
            callback: Mutex::new(Some(callback_rx)),
            owned: Mutex::new(None),
            lease: Mutex::new(Some(lease)),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(45))
                .build()
                .map_err(|_| failure())?,
            discovery: Mutex::new(None),
        });
        #[cfg(not(target_os = "android"))]
        {
            let tx = t.clone();
            tokio::spawn(async move {
                let result = tokio::select! {_=tx.cancel.cancelled()=>None,r=tokio::time::timeout(Duration::from_secs(600),receive_callback(listener))=>r.ok().and_then(Result::ok)};
                if let Some(url) = result {
                    let _ = callback_tx.send(url);
                }
            });
        }
        #[cfg(target_os = "android")]
        drop(callback_tx);
        let begin = Begin {
            transaction_id: t.id.clone(),
            generation,
            platform: platform(),
            endpoint,
            issuer: expected.issuer.clone(),
            app_id: expected.client_id.clone(),
            resource: expected.resource.clone(),
            scope: expected.scope.clone(),
            redirect_uri: if cfg!(target_os = "android") {
                "tech.tinadev.cybergrid.lab://tech.tinadev.cybergrid.lab/callback".into()
            } else {
                CALLBACK.into()
            },
        };
        Ok((t, begin))
    }
    pub async fn stage(&self, tokens: Tokens) -> Result<(), String> {
        if tokens.refresh_token.is_empty() || tokens.refresh_token.len() > LIMIT {
            return Err(failure());
        }
        // Keep RAM owner even if persistence fails. No response is released to JS.
        *self.latest.lock().await = Some(tokens.clone());
        let j = Journal {
            version: 1,
            config: self.config.clone(),
            expected: self.expected.clone(),
            tokens,
        };
        let bytes = serde_json::to_vec(&j).map_err(|_| failure())?;
        if bytes.len() > LIMIT {
            return Err(failure());
        }
        atomic(&self.journal, &bytes)
    }
    pub async fn fetch(&self, r: FetchRequest) -> Result<FetchResponse, String> {
        let _lock = self.operation.lock().await;
        if self.cancel.is_cancelled() || self.owned.lock().await.is_some() {
            return Err(failure());
        }
        if r.url.len() > 4096 || r.body.as_ref().is_some_and(|b| b.len() > 65536) {
            return Err(failure());
        }
        let u = Url::parse(&r.url).map_err(|_| failure())?;
        let issuer = Url::parse(&self.expected.issuer).map_err(|_| failure())?;
        if u.scheme() != "https"
            || u.origin() != issuer.origin()
            || !u.username().is_empty()
            || u.password().is_some()
            || u.fragment().is_some()
            || u.query().is_some()
        {
            return Err(failure());
        }
        let discovery_urls = [
            format!(
                "{}/.well-known/openid-configuration",
                self.expected.issuer.trim_end_matches('/')
            ),
            format!(
                "{}/.well-known/openid-configuration",
                issuer.origin().ascii_serialization()
            ),
        ];
        let is_discovery = discovery_urls.contains(&r.url);
        let is_token = r.url == self.expected.token_endpoint;
        let jwks = self
            .discovery
            .lock()
            .await
            .as_ref()
            .and_then(|d| d.get("jwks_uri"))
            .and_then(|v| v.as_str())
            .map(String::from);
        if !((r.method == "GET" && (is_discovery || jwks.as_ref() == Some(&r.url)))
            || (r.method == "POST" && is_token))
        {
            return Err(failure());
        }
        let mut req = self.http.request(
            if r.method == "GET" {
                reqwest::Method::GET
            } else {
                reqwest::Method::POST
            },
            u,
        );
        for (k, v) in &r.headers {
            match k.to_ascii_lowercase().as_str() {
                "accept" | "content-type" => {
                    if v.len() > 256 || v.contains(['\r', '\n']) {
                        return Err(failure());
                    }
                    req = req.header(k, v);
                }
                _ => return Err(failure()),
            }
        }
        if is_token {
            let body = r.body.as_deref().ok_or_else(failure)?;
            let pairs: Vec<_> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            let one = |key: &str| -> Option<&str> {
                let mut values = pairs.iter().filter(|(k, _)| k == key);
                let first = values.next()?.1.as_str();
                if values.next().is_some() {
                    None
                } else {
                    Some(first)
                }
            };
            if one("client_id") != Some(self.expected.client_id.as_str()) {
                return Err(failure());
            }
            match one("grant_type") {
                Some("authorization_code") => {
                    if one("redirect_uri") != Some(CALLBACK)
                        || one("code").is_none_or(str::is_empty)
                        || one("code_verifier").is_none_or(str::is_empty)
                    {
                        return Err(failure());
                    }
                }
                Some("refresh_token") => {
                    let previous = self.latest.lock().await;
                    if one("refresh_token") != previous.as_ref().map(|t| t.refresh_token.as_str())
                        || one("resource").is_some_and(|r| r != self.expected.resource)
                    {
                        return Err(failure());
                    }
                }
                _ => return Err(failure()),
            }
        }
        if let Some(body) = r.body {
            req = req.body(body);
        }
        // Once sent, never select/drop token response on UI cancellation.
        let response = req.send().await.map_err(|_| failure())?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
            if bytes.len() + chunk.len() > LIMIT {
                return Err(failure());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !(200..300).contains(&status) {
            return Ok(FetchResponse {
                status,
                headers: BTreeMap::new(),
                body: "{\"error\":\"authorization_failed\"}".into(),
            });
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| failure())?;
        if is_discovery {
            if value.get("issuer").and_then(|v| v.as_str()) != Some(&self.expected.issuer)
                || value.get("token_endpoint").and_then(|v| v.as_str())
                    != Some(&self.expected.token_endpoint)
            {
                return Err(failure());
            }
            for key in ["authorization_endpoint", "jwks_uri"] {
                let p = Url::parse(
                    value
                        .get(key)
                        .and_then(|v| v.as_str())
                        .ok_or_else(failure)?,
                )
                .map_err(|_| failure())?;
                if p.scheme() != "https"
                    || p.origin() != issuer.origin()
                    || !p.username().is_empty()
                    || p.query().is_some()
                    || p.fragment().is_some()
                {
                    return Err(failure());
                }
            }
            *self.discovery.lock().await = Some(value.clone());
        }
        if is_token {
            self.capture_token_response(&value).await?;
        }
        Ok(FetchResponse {
            status,
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body: String::from_utf8(bytes).map_err(|_| failure())?,
        })
    }
    async fn capture_token_response(&self, value: &serde_json::Value) -> Result<(), String> {
        let previous = self.latest.lock().await.clone();
        let tokens = Tokens {
            access_token: value
                .get("access_token")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            refresh_token: value
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| previous.as_ref().map(|t| t.refresh_token.clone()))
                .ok_or_else(failure)?,
            id_token: value
                .get("id_token")
                .and_then(|v| v.as_str())
                .map(String::from),
            expires_in: value
                .get("expires_in")
                .and_then(|v| v.as_u64())
                .unwrap_or_default(),
            token_type: value
                .get("token_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            scope: value
                .get("scope")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| previous.map(|t| t.scope))
                .unwrap_or_default(),
        };
        self.stage(tokens.clone()).await?;
        tokens.oauth()?;
        Ok(())
    }
    pub async fn authorize_url(&self, raw: &str) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err(failure());
        }
        let u = Url::parse(raw).map_err(|_| failure())?;
        let d = self.discovery.lock().await;
        let base = d
            .as_ref()
            .and_then(|v| v.get("authorization_endpoint"))
            .and_then(|v| v.as_str())
            .ok_or_else(failure)?;
        let mut without = u.clone();
        without.set_query(None);
        let q: BTreeMap<_, _> = u.query_pairs().into_owned().collect();
        if without.as_str() != base
            || u.fragment().is_some()
            || q.get("client_id") != Some(&self.expected.client_id)
            || q.get("redirect_uri").map(String::as_str) != Some(CALLBACK)
            || q.get("response_type").map(String::as_str) != Some("code")
            || q.get("code_challenge_method").map(String::as_str) != Some("S256")
            || q.get("state").is_none_or(String::is_empty)
            || q.get("code_challenge").is_none_or(String::is_empty)
        {
            return Err(failure());
        }
        Ok(())
    }
    pub async fn callback(&self) -> Result<String, String> {
        let rx = self.callback.lock().await.take().ok_or_else(failure)?;
        tokio::select! {_=self.cancel.cancelled()=>Err(failure()),r=rx=>r.map_err(|_|failure())}
    }
    pub async fn commit(&self, tokens: Tokens) -> Result<GrantImportOutcome, String> {
        let _guard = self.operation.lock().await;
        if let Some(owned) = *self.owned.lock().await {
            return Ok(owned);
        }
        if self.cancel.is_cancelled() {
            return Err(failure());
        }
        let latest = self.latest.lock().await.clone().ok_or_else(failure)?;
        if latest.access_token != tokens.access_token
            || latest.refresh_token != tokens.refresh_token
        {
            return Err(failure());
        }
        let outcome = self
            .client
            .import_browser_grant(&self.expected, tokens.oauth()?)
            .await
            .map_err(|_| failure())?;
        *self.owned.lock().await = Some(outcome);
        // ACK ownership even if journal unlink fails, but never install a session until
        // the journal is gone. A remaining journal is startup-cleanup-only.
        let removed = outcome == GrantImportOutcome::OwnedPersisted
            && match std::fs::remove_file(&self.journal) {
                Ok(()) => true,
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
        if outcome == GrantImportOutcome::OwnedPersisted && removed {
            if let Some(sender) = self.finish.lock().await.take() {
                let _ = sender.send(Some(tokens.access_token));
            }
        } else if let Some(sender) = self.finish.lock().await.take() {
            let _ = sender.send(None);
        }
        if removed {
            self.lease.lock().await.take();
        }
        Ok(outcome)
    }
    pub async fn cleanup(&self) -> Result<(), String> {
        self.cleanup_with_revoke(false).await
    }
    pub async fn cleanup_with_revoke(&self, revoke: bool) -> Result<(), String> {
        self.cancel.cancel();
        if self.source_active.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::timeout(Duration::from_secs(45), async {
                loop {
                    let notified = self.source_settled.notified();
                    if !self.source_active.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    notified.await;
                }
            })
            .await
            .map_err(|_| "系统浏览器授权仍在安全结束；请稍后重试取消".to_string())?;
        }
        let _guard = self.operation.lock().await;
        let owned = *self.owned.lock().await;
        if owned == Some(GrantImportOutcome::OwnedPending) {
            // A live pending owner must settle through the original Client and
            // its retained file lease. Never classify it as a fresh empty store.
            if revoke {
                self.client
                    .revoke_pending_grant()
                    .await
                    .map_err(|_| failure())?;
            } else {
                self.client
                    .retry_pending_grant()
                    .await
                    .map_err(|_| "最新授权仍待安全保存；请恢复存储或选择退出并撤销".to_string())?;
            }
            *self.owned.lock().await = Some(GrantImportOutcome::OwnedPersisted);
        }
        if self.owned.lock().await.is_some() {
            match std::fs::remove_file(&self.journal) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(failure()),
            }
        }
        if self.owned.lock().await.is_none() {
            if let Some(t) = self.latest.lock().await.as_ref() {
                let current = self.client.auth_config().await.map_err(|_| failure())?;
                if !current
                    .same_client_binding(&self.expected, &self.config.app_id)
                    .map_err(|_| failure())?
                {
                    return Err(failure());
                }
                self.client
                    .revoke_refresh_token(&t.refresh_token)
                    .await
                    .map_err(|_| failure())?;
            }
            match std::fs::remove_file(&self.journal) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(failure()),
            }
        }
        if let Some(sender) = self.finish.lock().await.take() {
            let _ = sender.send(None);
        }
        self.lease.lock().await.take();
        Ok(())
    }
}
#[cfg(not(target_os = "android"))]
async fn receive_callback(listener: TcpListener) -> Result<String, String> {
    // Ignore unrelated probes without consuming the transaction callback.
    for _ in 0..16 {
        let (mut socket, remote) = listener.accept().await.map_err(|_| failure())?;
        if !remote.ip().is_loopback() {
            continue;
        }
        let mut b = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut chunk = [0u8; 1024];
                let n = socket.read(&mut chunk).await.map_err(|_| failure())?;
                if n == 0 || b.len() + n > 16384 {
                    return Err(failure());
                }
                b.extend_from_slice(&chunk[..n]);
                if b.windows(4).any(|s| s == b"\r\n\r\n") {
                    return Ok(());
                }
            }
        })
        .await;
        let request = String::from_utf8_lossy(&b);
        let first = request.lines().next().unwrap_or("");
        let parts: Vec<_> = first.split(' ').collect();
        let accepted = result.is_ok_and(|r| r.is_ok())
            && parts.len() == 3
            && parts[0] == "GET"
            && parts[1].starts_with("/callback?")
            && parts[2] == "HTTP/1.1"
            && request
                .lines()
                .any(|l| l.eq_ignore_ascii_case("host: 127.0.0.1:17896"));
        let body = if accepted {
            "授权回调已接收，请返回 CyberGrid Lab。"
        } else {
            "Invalid callback"
        };
        let status = if accepted {
            "200 OK"
        } else {
            "400 Bad Request"
        };
        let _=socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await;
        if accepted {
            return Ok(format!("http://127.0.0.1:17896{}", parts[1]));
        }
    }
    Err(failure())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cybergrid-browser-boundary-{}",
            hex::encode(rand::random::<[u8; 12]>())
        ));
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        p
    }
    fn expected() -> AuthConfig {
        AuthConfig {
            registration_admission: None,
            endpoint_policy: None,
            product_id: None,
            policy_version: None,
            products: None,
            login_flow: None,
            coordination: None,
            provider: "logto".into(),
            issuer: "https://login.example.test/oidc".into(),
            client_id: "fixture-native".into(),
            resource: "https://example.test/api".into(),
            scope: "openid offline_access profile email connect".into(),
            device_authorization_endpoint: "https://login.example.test/oidc/device/auth".into(),
            token_endpoint: "https://login.example.test/oidc/token".into(),
            revocation_endpoint: Some("https://login.example.test/oidc/token/revocation".into()),
        }
    }
    fn tokens(refresh: &str) -> Tokens {
        Tokens {
            access_token: "fixture-resource-at".into(),
            refresh_token: refresh.into(),
            id_token: None,
            expires_in: 3600,
            token_type: "Bearer".into(),
            scope: "connect".into(),
        }
    }
    fn tx(data: &Path, config: LabConfig, client: Arc<Client>) -> Arc<Transaction> {
        let (send, _) = oneshot::channel();
        Arc::new(Transaction {
            id: hex::encode(rand::random::<[u8; 24]>()),
            config,
            expected: expected(),
            client,
            cancel: CancellationToken::new(),
            operation: Mutex::new(()),
            source_active: std::sync::atomic::AtomicBool::new(false),
            source_settled: tokio::sync::Notify::new(),
            latest: Mutex::new(None),
            journal: journal_path(data),
            finish: Mutex::new(Some(send)),
            callback: Mutex::new(None),
            owned: Mutex::new(None),
            lease: Mutex::new(Some(journal_lock(data).unwrap())),
            http: reqwest::Client::new(),
            discovery: Mutex::new(None),
        })
    }
    async fn coordinator(config: AuthConfig) -> (String, tokio::task::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else {
                    break;
                };
                let config = config.clone();
                tokio::spawn(async move {
                    let mut b = [0; 8192];
                    let _ = s.read(&mut b).await;
                    let body = serde_json::to_vec(&config).unwrap();
                    let h=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                    s.write_all(h.as_bytes()).await.unwrap();
                    s.write_all(&body).await.unwrap();
                });
            }
        });
        (base, task)
    }
    fn client(base: &str) -> Arc<Client> {
        Client::new(cybergrid_sdk::Config {
            base_url: base.into(),
            app_id: "fixture-native".into(),
            ..Default::default()
        })
        .unwrap()
    }
    #[tokio::test]
    async fn browser_boundary_malformed_response_keeps_rotated_refresh_and_private_lease() {
        let data = root();
        let config = LabConfig {
            base_url: "http://127.0.0.1:9".into(),
            app_id: "fixture-native".into(),
            ..Default::default()
        };
        let t = tx(&data, config, client("http://127.0.0.1:9"));
        t.stage(tokens("older-fixture-refresh")).await.unwrap();
        assert!(t
            .capture_token_response(
                &serde_json::json!({"refresh_token":"newest-fixture-refresh","expires_in":0})
            )
            .await
            .is_err());
        assert_eq!(
            t.latest.lock().await.as_ref().unwrap().refresh_token,
            "newest-fixture-refresh"
        );
        let j: Journal =
            serde_json::from_slice(&read_private(&journal_path(&data)).unwrap()).unwrap();
        assert_eq!(j.tokens.refresh_token, "newest-fixture-refresh");
        assert_eq!(j.tokens.access_token, "");
        assert!(
            journal_lock(&data).is_err(),
            "active transaction retains cross-process exclusion"
        );
        drop(t);
        assert!(journal_lock(&data).is_ok());
        std::fs::remove_dir_all(data).unwrap();
    }
    #[tokio::test]
    async fn browser_boundary_restart_reconciles_matching_checkpoint_and_blocks_ambiguous_grant() {
        let data = root();
        let (base, server) = coordinator(expected()).await;
        let c = client(&base);
        let cfg = LabConfig {
            base_url: base.clone(),
            app_id: "fixture-native".into(),
            ..Default::default()
        };
        let store = cybergrid_sdk::CredentialStore::new(crate::state::credential_path(&data, &cfg));
        c.use_credential_store(store.clone()).await.unwrap();
        let t = tx(&data, cfg.clone(), c.clone());
        let material = tokens("saved-fixture-refresh");
        t.stage(material.clone()).await.unwrap();
        assert_eq!(
            c.import_browser_grant(&expected(), material.oauth().unwrap())
                .await
                .unwrap(),
            GrantImportOutcome::OwnedPersisted
        );
        let original = std::fs::read(store.path()).unwrap();
        drop(t);
        drop(c);
        // Model process death after v4 checkpoint but before journal unlink.
        recover(&data).await.unwrap();
        assert!(!journal_path(&data).exists());
        assert_eq!(std::fs::read(store.path()).unwrap(), original);
        let j = Journal {
            version: 1,
            config: cfg,
            expected: expected(),
            tokens: tokens("older-ambiguous-refresh"),
        };
        atomic(&journal_path(&data), &serde_json::to_vec(&j).unwrap()).unwrap();
        assert!(recover(&data).await.is_err());
        assert!(journal_path(&data).exists());
        assert_eq!(std::fs::read(store.path()).unwrap(), original);
        server.abort();
        let _ = server.await;
        std::fs::remove_dir_all(data).unwrap();
    }
    #[tokio::test]
    async fn browser_boundary_live_pending_never_becomes_fresh_journal_cleanup() {
        let data = root();
        let (base, server) = coordinator(expected()).await;
        let c = client(&base);
        let cfg = LabConfig {
            base_url: base,
            app_id: "fixture-native".into(),
            ..Default::default()
        };
        let storage = data.join("credential-store");
        std::fs::create_dir(&storage).unwrap();
        let store = cybergrid_sdk::CredentialStore::new(storage.join("native.json"));
        c.use_credential_store(store.clone()).await.unwrap();
        assert_eq!(
            c.browser_grant_checkpoint(&expected(), "pending-fixture-refresh")
                .await
                .unwrap(),
            cybergrid_sdk::BrowserGrantCheckpoint::Empty
        );
        let t = tx(&data, cfg, c.clone());
        let material = tokens("pending-fixture-refresh");
        t.stage(material.clone()).await.unwrap();
        std::fs::set_permissions(&storage, std::fs::Permissions::from_mode(0o500)).unwrap();
        assert_eq!(
            t.commit(material).await.unwrap(),
            GrantImportOutcome::OwnedPending
        );
        assert!(c.has_pending_grant().await);
        assert!(
            journal_path(&data).exists(),
            "pending ACK retains journal even when its parent is writable"
        );
        atomic(&data.join("writable-check.json"), b"{}").unwrap();
        assert!(t.cleanup().await.is_err());
        assert!(
            journal_lock(&data).is_err(),
            "failed live checkpoint keeps journal exclusion"
        );
        std::fs::set_permissions(&storage, std::fs::Permissions::from_mode(0o700)).unwrap();
        t.cleanup().await.unwrap();
        assert!(!c.has_pending_grant().await);
        assert!(store.exists());
        assert!(!journal_path(&data).exists());
        assert_eq!(
            c.browser_grant_checkpoint(&expected(), "pending-fixture-refresh")
                .await
                .unwrap(),
            cybergrid_sdk::BrowserGrantCheckpoint::Matching
        );
        server.abort();
        let _ = server.await;
        drop(t);
        std::fs::remove_dir_all(data).unwrap();
    }
    #[tokio::test]
    async fn browser_boundary_broker_rejects_arbitrary_urls_and_token_requests_before_network() {
        let data = root();
        let t = tx(&data, LabConfig::default(), client("http://127.0.0.1:9"));
        for url in [
            "http://login.example.test/oidc/token",
            "https://attacker.example/token",
            "https://login.example.test/oidc/token?x=1",
            "https://login.example.test/arbitrary",
        ] {
            assert!(t
                .fetch(FetchRequest {
                    url: url.into(),
                    method: "GET".into(),
                    headers: BTreeMap::new(),
                    body: None
                })
                .await
                .is_err());
        }
        assert!(t
            .fetch(FetchRequest {
                url: t.expected.token_endpoint.clone(),
                method: "POST".into(),
                headers: BTreeMap::new(),
                body: Some("grant_type=refresh_token&client_id=other&refresh_token=fixture".into())
            })
            .await
            .is_err());
        drop(t);
        std::fs::remove_dir_all(data).unwrap();
    }
}
