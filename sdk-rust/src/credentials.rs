use crate::{AuthConfig, Client, Error, OAuthTokens, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    version: u8,
    server: String,
    app_id: String,
    issuer: String,
    client_id: String,
    resource: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    device_authorization_endpoint: String,
    #[serde(default)]
    token_endpoint: String,
    #[serde(default)]
    revocation_endpoint: Option<String>,
    refresh_token: String,
}
impl Credentials {
    fn binding(&self, c: &Client) -> Result<()> {
        if self.version != 4
            || self.refresh_token.is_empty()
            || self.server != c.cfg.base_url
            || self.app_id != c.cfg.app_id
            || self.client_id != c.cfg.app_id
        {
            return Err(Error::Credentials("version4/full OAuth binding required; version3 is not automatically imported; server/appId/clientId mismatch; existing file preserved, authorize independently".into()));
        }
        Ok(())
    }
    fn validate(&self, c: &Client, a: &AuthConfig) -> Result<()> {
        self.binding(c)?;
        if self.issuer != a.issuer
            || self.client_id != a.client_id
            || self.resource != a.resource
            || self.scope != a.scope
            || self.device_authorization_endpoint != a.device_authorization_endpoint
            || self.token_endpoint != a.token_endpoint
            || self.revocation_endpoint != a.revocation_endpoint
        {
            return Err(Error::Credentials(
                "immutable OAuth configuration differs from current configuration".into(),
            ));
        }
        Ok(())
    }
}
/// Explicit per-device private storage. Never discovers or imports Go files.
#[derive(Clone)]
pub struct CredentialStore {
    path: PathBuf,
    retained_lease: Arc<std::sync::Mutex<Option<File>>>,
    pending: Arc<tokio::sync::Mutex<Option<OAuthTokens>>>,
    latest_committed: Arc<tokio::sync::Mutex<Option<String>>>,
    pending_binding: Arc<tokio::sync::Mutex<Option<(String, String, AuthConfig)>>>,
    operation: Arc<tokio::sync::Mutex<()>>,
    pub(crate) auth_gate: Arc<tokio::sync::Mutex<()>>,
}
impl CredentialStore {
    pub(crate) async fn browser_checkpoint(
        &self,
        client: &Client,
        expected: &AuthConfig,
        refresh: &str,
    ) -> Result<crate::BrowserGrantCheckpoint> {
        if self.pending.lock().await.is_some() {
            return Ok(crate::BrowserGrantCheckpoint::OtherGrant);
        }
        match std::fs::symlink_metadata(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(crate::BrowserGrantCheckpoint::Empty)
            }
            Err(e) => Err(e.into()),
            Ok(_) => {
                use std::os::unix::fs::MetadataExt;
                let mut f = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                    .open(&self.path)?;
                let m = f.metadata()?;
                if !m.is_file()
                    || m.mode() & 0o7777 != 0o600
                    || m.uid() != unsafe { libc::geteuid() }
                {
                    return Err(Error::Credentials(
                        "browser checkpoint requires owned regular 0600 file".into(),
                    ));
                }
                let mut bytes = Vec::new();
                Read::by_ref(&mut f).take(65537).read_to_end(&mut bytes)?;
                if bytes.len() > 65536 {
                    return Err(Error::Credentials("file too large".into()));
                }
                let saved: Credentials = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Credentials("invalid private JSON".into()))?;
                saved.validate(client, expected)?;
                Ok(if saved.refresh_token == refresh {
                    crate::BrowserGrantCheckpoint::Matching
                } else {
                    crate::BrowserGrantCheckpoint::OtherGrant
                })
            }
        }
    }
    /// Called only with the Client grant gate, this store's authorization gate,
    /// and its cross-process file lease held. Existing independent grants are
    /// never replaced; only a previously accepted handoff can be acknowledged.
    pub(crate) async fn browser_import_ack(
        &self,
        client: &Client,
        refresh: &str,
    ) -> Result<crate::GrantImportOutcome> {
        let latest = client.latest_grant.lock().await.clone();
        if latest.as_deref() == Some(refresh) {
            if self
                .pending
                .lock()
                .await
                .as_ref()
                .is_some_and(|p| p.refresh_token == refresh)
            {
                return Ok(crate::GrantImportOutcome::OwnedPending);
            }
            if self.latest_committed.lock().await.as_deref() == Some(refresh) {
                return Ok(crate::GrantImportOutcome::OwnedPersisted);
            }
        }
        Err(Error::Credentials(
            "browser handoff grant was rotated or revoked".into(),
        ))
    }
    pub(crate) async fn require_empty_browser_destination(&self, client: &Client) -> Result<()> {
        if client.latest_grant.lock().await.is_some() {
            return Err(Error::Credentials(
                "browser handoff cannot replace an owned grant".into(),
            ));
        }
        if self.pending.lock().await.is_some()
            || self.latest_committed.lock().await.is_some()
            || match std::fs::symlink_metadata(&self.path) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            }
        {
            return Err(Error::Credentials(
                "browser handoff requires an empty credential store".into(),
            ));
        }
        Ok(())
    }
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            retained_lease: Arc::new(std::sync::Mutex::new(None)),
            pending: Arc::new(tokio::sync::Mutex::new(None)),
            latest_committed: Arc::new(tokio::sync::Mutex::new(None)),
            pending_binding: Arc::new(tokio::sync::Mutex::new(None)),
            operation: Arc::new(tokio::sync::Mutex::new(())),
            auth_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    fn read(&self) -> Result<Credentials> {
        let mut f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.path)?;
        Self::private_regular(&f)?;
        let mut b = Vec::new();
        Read::by_ref(&mut f).take(65537).read_to_end(&mut b)?;
        if b.len() > 65536 {
            return Err(Error::Credentials("file too large".into()));
        }
        serde_json::from_slice(&b).map_err(|_| Error::Credentials("invalid private JSON".into()))
    }
    fn private_regular(f: &File) -> Result<()> {
        let m = f.metadata()?;
        if !m.is_file() || m.permissions().mode() & 0o077 != 0 {
            return Err(Error::Credentials(
                "must be private regular file (0600)".into(),
            ));
        }
        Ok(())
    }
    pub(crate) async fn lock(&self) -> Result<File> {
        if let Some(file) = self
            .retained_lease
            .lock()
            .expect("private lease mutex")
            .as_ref()
        {
            return Ok(file.try_clone()?);
        }

        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::Credentials("file needs parent directory".into()))?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        let mut lock = self.path.as_os_str().to_os_string();
        lock.push(".lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(PathBuf::from(lock))?;
        Self::private_regular(&file)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Timeout("credential lock"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    fn write(&self, c: &Credentials) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::Credentials("missing parent".into()))?;
        if let Ok(meta) = std::fs::symlink_metadata(&self.path) {
            if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
                return Err(Error::Credentials("refusing unsafe existing path".into()));
            }
            let old = self.read()?;
            if old.version != 4
                || old.server != c.server
                || old.app_id != c.app_id
                || old.client_id != c.client_id
                || old.issuer != c.issuer
                || old.resource != c.resource
                || old.scope != c.scope
                || old.device_authorization_endpoint != c.device_authorization_endpoint
                || old.token_endpoint != c.token_endpoint
                || old.revocation_endpoint != c.revocation_endpoint
            {
                return Err(Error::Credentials(
                    "refusing old or differently bound credentials".into(),
                ));
            }
        }
        let mut f = tempfile::NamedTempFile::new_in(parent)?;
        f.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        serde_json::to_writer(&mut f, c)
            .map_err(|_| Error::Credentials("cannot encode credentials".into()))?;
        f.write_all(b"\n")?;
        f.as_file().sync_all()?;
        f.persist(&self.path).map_err(|e| Error::Io(e.error))?;
        File::open(parent)?.sync_all()?;
        Ok(())
    }
    pub(crate) async fn pending_tokens(&self) -> Option<OAuthTokens> {
        self.pending.lock().await.clone()
    }
    pub async fn has_pending(&self) -> bool {
        self.pending.lock().await.is_some()
    }
    pub async fn save_new(&self, client: &Client, tokens: &OAuthTokens) -> Result<()> {
        let _auth = self.auth_gate.lock().await;
        if client
            .latest_grant
            .lock()
            .await
            .as_ref()
            .is_some_and(|latest| latest != &tokens.refresh_token)
        {
            return Err(Error::Credentials(
                "refusing an older grant; newest owner retained".into(),
            ));
        }
        if client.latest_grant.lock().await.is_some()
            && self
                .latest_committed
                .lock()
                .await
                .as_ref()
                .is_some_and(|latest| latest != &tokens.refresh_token)
        {
            return Err(Error::Credentials(
                "shared store has a newer grant; bind before authorization".into(),
            ));
        }
        let file = self.lock().await?;
        if self.path.exists() && client.latest_grant.lock().await.is_some() {
            let existing = self.read()?;
            existing.binding(client)?;
            let pending = self.pending.lock().await.clone();
            if existing.refresh_token != tokens.refresh_token
                && pending
                    .as_ref()
                    .is_none_or(|t| t.refresh_token != tokens.refresh_token)
            {
                return Err(Error::Credentials(
                    "existing file has a newer grant; bind store before minting".into(),
                ));
            }
        }
        self.save_under_file_lock(client, tokens, &file).await
    }
    pub(crate) async fn save_checkpoint(
        &self,
        client: &Client,
        tokens: &OAuthTokens,
    ) -> Result<()> {
        *self.pending.lock().await = Some(tokens.clone());
        if let Some(config) = client.auth.lock().await.clone() {
            *self.pending_binding.lock().await = Some((
                client.cfg.base_url.clone(),
                client.cfg.app_id.clone(),
                config,
            ));
        }
        let _file = self.lock().await?;
        self.save_under_file_lock(client, tokens, &_file).await
    }
    pub(crate) async fn save_under_file_lock(
        &self,
        client: &Client,
        tokens: &OAuthTokens,
        file: &File,
    ) -> Result<()> {
        let _operation = self.operation.lock().await;
        *self.pending.lock().await = Some(tokens.clone());
        let result = async {
            if tokens.refresh_token.is_empty() {
                return Err(Error::Credentials("refresh token required".into()));
            }
            let cached = client.auth.lock().await.clone();
            // A newly minted grant must be persisted without another fallible network
            // round trip; the producing flow already verified this immutable config.
            let a = match cached {
                Some(a) => a,
                None => client.auth_config().await?,
            };
            *self.pending_binding.lock().await = Some((
                client.cfg.base_url.clone(),
                client.cfg.app_id.clone(),
                a.clone(),
            ));
            self.write(&Credentials {
                version: 4,
                server: client.cfg.base_url.clone(),
                app_id: client.cfg.app_id.clone(),
                issuer: a.issuer,
                client_id: a.client_id,
                resource: a.resource,
                scope: a.scope,
                device_authorization_endpoint: a.device_authorization_endpoint,
                token_endpoint: a.token_endpoint,
                revocation_endpoint: a.revocation_endpoint,
                refresh_token: tokens.refresh_token.clone(),
            })?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                *self.latest_committed.lock().await = Some(tokens.refresh_token.clone());
                self.pending.lock().await.take();
                self.retained_lease
                    .lock()
                    .expect("private lease mutex")
                    .take();
                Ok(())
            }
            Err(error) => {
                *self.retained_lease.lock().expect("private lease mutex") = Some(file.try_clone()?);
                Err(error)
            }
        }
    }
    /// Serializes read/check/refresh/atomic rotation. Persists before API exchange.
    pub async fn refresh(&self, client: &Client) -> Result<OAuthTokens> {
        client.use_credential_store(self.clone()).await?;
        client.refresh_access_token("").await
    }
    pub(crate) async fn latest_refresh(&self, client: &Client, file: &File) -> Result<String> {
        let pending = self.pending.lock().await.clone();
        if let Some(t) = pending {
            self.save_under_file_lock(client, &t, file).await?;
        }
        let c = self.read()?;
        c.binding(client)?;
        let a = client.auth_config().await?;
        c.validate(client, &a)?;
        Ok(c.refresh_token)
    }
    /// Revoke first; every failure preserves the private file.
    pub async fn logout(&self, client: &Client) -> Result<()> {
        let _auth = self.auth_gate.lock().await;
        let file = self.lock().await?;
        let pending = self.pending.lock().await.clone();
        if let Some(t) = pending {
            let binding = self.pending_binding.lock().await.clone().ok_or_else(|| {
                Error::Credentials(
                    "pending grant binding unavailable; producing owner retained".into(),
                )
            })?;
            let mut expected = binding.2;
            let mut current = client.auth_config().await?;
            expected.coordination = None;
            current.coordination = None;
            if binding.0 != client.cfg.base_url
                || binding.1 != client.cfg.app_id
                || expected != current
            {
                return Err(Error::Credentials(
                    "pending grant OAuth binding differs".into(),
                ));
            }
            // The latest grant is already privately bound to this producing Client.
            // Revoke it even while its checkpoint destination is unavailable.
            if let Err(error) = client.revoke_refresh_token(&t.refresh_token).await {
                *self.retained_lease.lock().expect("private lease mutex") = Some(file.try_clone()?);
                return Err(error);
            }
            self.pending.lock().await.take();
            client.clear_revoked_grant(&t.refresh_token).await;
            self.retained_lease
                .lock()
                .expect("private lease mutex")
                .take();
            FileExt::unlock(&file)?;
            if self.path.is_file() {
                std::fs::remove_file(&self.path)?;
            }
            return Ok(());
        }
        let c = self.read()?;
        c.binding(client)?;
        let a = client.auth_config().await?;
        c.validate(client, &a)?;
        client.revoke_refresh_token(&c.refresh_token).await?;
        std::fs::remove_file(&self.path)?;
        client.clear_revoked_grant(&c.refresh_token).await;
        self.retained_lease
            .lock()
            .expect("private lease mutex")
            .take();
        Ok(())
    }
}

impl std::fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialStore{private}")
    }
}
impl crate::AuthProvider for CredentialStore {
    fn access_token<'a>(&'a self, client: &'a Client) -> crate::AuthFuture<'a> {
        Box::pin(async move { Ok(self.refresh(client).await?.access_token) })
    }
}
