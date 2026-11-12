//! Correlated access-token IPC. Cancellation ends a caller's wait, not token ownership.
use cybergrid_sdk::{AuthFuture, AuthProvider, AuthRequestReason, AuthTokenRequest, Client, Error};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::{Mutex, Notify};
use workspace_core::secret;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRequest {
    pub request_id: String,
    pub owner_generation: u64,
    pub sdk_generation: u64,
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub min_validity: u64,
    pub force_refresh: bool,
    pub reason: &'static str,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenReply {
    pub request_id: String,
    pub owner_generation: u64,
    pub sdk_generation: u64,
    pub access_token: Option<String>,
    pub failure: Option<String>,
}
type Sink = Arc<dyn Fn(TokenRequest) -> Result<(), &'static str> + Send + Sync>;
struct Pending {
    owner: u64,
    sdk: u64,
    result: Mutex<Option<Result<String, ()>>>,
    done: Notify,
}
#[derive(Default)]
struct Inner {
    owner: u64,
    sdk: u64,
    sink: Option<Sink>,
    pending: HashMap<String, Arc<Pending>>,
    receipts: VecDeque<(String, u64, u64, [u8; 32])>,
}
#[derive(Default)]
pub struct TokenBroker {
    inner: Mutex<Inner>,
    settled: Notify,
}
impl TokenBroker {
    pub async fn attach(&self, owner: u64, sink: Sink) -> Result<(), &'static str> {
        let mut s = self.inner.lock().await;
        if owner == 0 || !s.pending.is_empty() || s.sink.is_some() {
            return Err("OWNER_BUSY");
        };
        s.owner = owner;
        s.sink = Some(sink);
        Ok(())
    }
    pub async fn bind(&self, owner: u64, sdk: u64) -> Result<(), &'static str> {
        loop {
            self.barrier().await;
            let mut s = self.inner.lock().await;
            if !s.pending.is_empty() {
                continue;
            }
            if s.owner != owner || s.sink.is_none() {
                return Err("OWNER_MISMATCH");
            }
            s.sdk = sdk;
            return Ok(());
        }
    }
    pub async fn detach(&self, owner: u64) -> Result<(), &'static str> {
        loop {
            self.barrier().await;
            let mut s = self.inner.lock().await;
            if !s.pending.is_empty() {
                continue;
            }
            if s.sink.is_some() && s.owner != owner {
                return Err("OWNER_MISMATCH");
            }
            s.sink = None;
            return Ok(());
        }
    }
    pub async fn barrier(&self) {
        loop {
            let notified = self.settled.notified();
            if self.inner.lock().await.pending.is_empty() {
                return;
            };
            notified.await
        }
    }
    pub async fn reply(&self, r: TokenReply) -> Result<(), &'static str> {
        if r.access_token.is_some() == r.failure.is_some()
            || r.access_token
                .as_ref()
                .is_some_and(|t| t.is_empty() || t.len() > 16384)
            || r.failure.as_deref().is_some_and(|f| {
                !matches!(
                    f,
                    "AUTH_UNAVAILABLE"
                        | "OUTCOME_UNKNOWN"
                        | "BINDING_MISMATCH"
                        | "BUSY"
                        | "CANCELLED"
                )
            })
        {
            return Err("INVALID_REPLY");
        };
        let digest: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&r).map_err(|_| "INVALID_REPLY")?).into();
        let mut s = self.inner.lock().await;
        if let Some((_, owner, sdk, prior)) =
            s.receipts.iter().find(|(id, _, _, _)| id == &r.request_id)
        {
            return if *owner == r.owner_generation && *sdk == r.sdk_generation && *prior == digest {
                Ok(())
            } else {
                Err("SEQUENCE_CONFLICT")
            };
        };
        let p = s.pending.get(&r.request_id).cloned().ok_or("STALE_REPLY")?;
        if p.owner != r.owner_generation || p.sdk != r.sdk_generation {
            return Err("STALE_REPLY");
        };
        let mut result = p.result.lock().await;
        if result.is_some() {
            return Err("DUPLICATE_REPLY");
        };
        *result = Some(r.access_token.ok_or(()));
        s.receipts
            .push_back((r.request_id, r.owner_generation, r.sdk_generation, digest));
        while s.receipts.len() > 16 {
            s.receipts.pop_front();
        }
        p.done.notify_waiters();
        Ok(())
    }
    async fn request(
        self: Arc<Self>,
        sdk: u64,
        r: &AuthTokenRequest,
    ) -> cybergrid_sdk::Result<String> {
        let id = secret();
        let p;
        {
            let mut s = self.inner.lock().await;
            if s.sdk != sdk || s.sink.is_none() {
                return Err(Error::AuthorizationUnavailable);
            };
            if s.pending.len() >= 2 {
                return Err(Error::AuthorizationUnavailable);
            };
            p = Arc::new(Pending {
                owner: s.owner,
                sdk,
                result: Mutex::new(None),
                done: Notify::new(),
            });
            let msg = TokenRequest {
                request_id: id.clone(),
                owner_generation: s.owner,
                sdk_generation: sdk,
                issuer: r.issuer.clone(),
                client_id: r.client_id.clone(),
                resource: r.resource.clone(),
                scopes: r.scopes.clone(),
                min_validity: r.min_validity,
                force_refresh: r.force_refresh,
                reason: match r.reason {
                    AuthRequestReason::Connect => "connect",
                    AuthRequestReason::CoordinationRecovery => "coordination-recovery",
                    AuthRequestReason::AuthorizationRejected => "authorization-rejected",
                },
            };
            // Insert before dispatch; the receiver may reply immediately.
            s.pending.insert(id.clone(), p.clone());
            if s.sink.as_ref().unwrap()(msg).is_err() {
                s.pending.remove(&id);
                self.settled.notify_waiters();
                return Err(Error::AuthorizationUnavailable);
            }
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let broker = self.clone();
        tokio::spawn(async move {
            let result = loop {
                let n = p.done.notified();
                if let Some(r) = p.result.lock().await.take() {
                    break r;
                };
                n.await;
            };
            broker.inner.lock().await.pending.remove(&id);
            broker.settled.notify_waiters();
            let _ = tx.send(result);
        });
        tokio::select! {biased;_=r.cancel.cancelled()=>Err(Error::Cancelled),result=rx=>result.map_err(|_|Error::AuthorizationUnavailable)?.map_err(|_|Error::AuthorizationUnavailable)}
    }
}
pub struct IpcProvider {
    pub broker: Arc<TokenBroker>,
    pub sdk_generation: u64,
}
impl AuthProvider for IpcProvider {
    fn access_token_for<'a>(
        &'a self,
        _client: &'a Client,
        r: &'a AuthTokenRequest,
    ) -> AuthFuture<'a> {
        Box::pin(self.broker.clone().request(self.sdk_generation, r))
    }
}
#[cfg(test)]
mod tests;
