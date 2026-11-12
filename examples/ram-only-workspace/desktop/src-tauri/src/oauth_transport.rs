//! RAM-only native transport. An owned task outlives cancelled IPC waiters.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify};
use url::Url;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub issuer: String,
    pub endpoint: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}
impl Binding {
    pub fn validate(&self) -> Result<(), &'static str> {
        let u = Url::parse(&self.issuer).map_err(|_| "BINDING_MISMATCH")?;
        if u.scheme() != "https"
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || u.origin().ascii_serialization() != self.endpoint
            || self.client_id.is_empty()
            || self.scopes.is_empty()
        {
            return Err("BINDING_MISMATCH");
        };
        let resource = Url::parse(&self.resource).map_err(|_| "BINDING_MISMATCH")?;
        let mut scopes = self.scopes.clone();
        scopes.sort();
        scopes.dedup();
        if self.client_id.len() > 21
            || !self
                .client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || scopes.len() != self.scopes.len()
            || self
                .scopes
                .iter()
                .any(|s| s.is_empty() || s.split_whitespace().count() != 1)
        {
            return Err("BINDING_MISMATCH");
        }
        if resource.scheme() != "https"
            || resource.host_str().is_none()
            || !resource.username().is_empty()
            || resource.password().is_some()
            || resource.query().is_some()
            || resource.fragment().is_some()
        {
            return Err("BINDING_MISMATCH");
        };
        Ok(())
    }
    pub fn matches(&self, c: &cybergrid_sdk::AuthConfig) -> bool {
        let mut a = self.scopes.clone();
        a.sort();
        let mut b: Vec<_> = c.scope.split_whitespace().map(str::to_owned).collect();
        b.sort();
        c.issuer == self.issuer
            && c.client_id == self.client_id
            && c.resource == self.resource
            && a == b
            && c.provider == "logto"
            && c.token_endpoint == format!("{}/token", self.issuer.trim_end_matches('/'))
            && c.device_authorization_endpoint
                == format!("{}/device/auth", self.issuer.trim_end_matches('/'))
            && c.revocation_endpoint.as_deref()
                == Some(format!("{}/token/revocation", self.issuer.trim_end_matches('/')).as_str())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}
struct Operation {
    id: u64,
    digest: [u8; 32],
    result: Mutex<Option<Result<FetchResponse, &'static str>>>,
    done: Notify,
}
#[derive(Default)]
struct State {
    highwater: u64,
    current: Option<Arc<Operation>>,
    poisoned: bool,
    refresh_hash: Option<[u8; 32]>,
    discovery: Option<Value>,
    received_reply: Option<FetchResponse>,
}
pub struct OAuthTransport {
    pub binding: Binding,
    http: reqwest::Client,
    state: Mutex<State>,
}
impl OAuthTransport {
    pub fn new(binding: Binding) -> Result<Arc<Self>, &'static str> {
        binding.validate()?;
        Self::with_http(
            binding,
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(45))
                .build()
                .map_err(|_| "HTTP_SETUP")?,
        )
    }
    fn with_http(binding: Binding, http: reqwest::Client) -> Result<Arc<Self>, &'static str> {
        Ok(Arc::new(Self {
            binding,
            http,
            state: Mutex::new(State::default()),
        }))
    }
    pub async fn barrier(&self) {
        let current = self.state.lock().await.current.clone();
        if let Some(op) = current {
            loop {
                let n = op.done.notified();
                if op.result.lock().await.is_some() {
                    return;
                };
                n.await;
            }
        }
    }
    pub async fn fetch(
        self: &Arc<Self>,
        id: u64,
        request: FetchRequest,
    ) -> Result<FetchResponse, &'static str> {
        let bytes = serde_json::to_vec(&request).map_err(|_| "INVALID_REQUEST")?;
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let op;
        {
            let mut s = self.state.lock().await;
            if let Some(current) = &s.current {
                if current.id == id {
                    if current.digest != digest {
                        return Err("SEQUENCE_CONFLICT");
                    };
                    op = current.clone();
                    drop(s);
                    return wait(op).await;
                };
                if current.result.lock().await.is_none() {
                    return Err("BUSY");
                }
            }
            if id != s.highwater + 1 {
                return Err("SEQUENCE_CONFLICT");
            };
            self.validate(&request, &s)?;
            op = Arc::new(Operation {
                id,
                digest,
                result: Mutex::new(None),
                done: Notify::new(),
            });
            s.highwater = id;
            s.current = Some(op.clone());
        }
        let owner = self.clone();
        let work = op.clone();
        tokio::spawn(async move {
            let result = owner.send(request).await;
            *work.result.lock().await = Some(result);
            work.done.notify_waiters();
        });
        wait(op).await
    }
    fn validate(&self, r: &FetchRequest, s: &State) -> Result<(), &'static str> {
        if r.url.len() > 4096 || r.body.as_ref().is_some_and(|v| v.len() > 65536) {
            return Err("INVALID_REQUEST");
        };
        let u = Url::parse(&r.url).map_err(|_| "INVALID_REQUEST")?;
        if u.scheme() != "https"
            || u.origin().ascii_serialization() != self.binding.endpoint
            || u.query().is_some()
            || u.fragment().is_some()
            || !u.username().is_empty()
            || u.password().is_some()
        {
            return Err("INVALID_REQUEST");
        };
        let base = self.binding.issuer.trim_end_matches('/');
        let discovery = r.url == format!("{base}/.well-known/openid-configuration")
            || r.url == format!("{}/.well-known/openid-configuration", self.binding.endpoint);
        let token = r.url == format!("{base}/token");
        let jwks = s
            .discovery
            .as_ref()
            .and_then(|d| d.get("jwks_uri"))
            .and_then(Value::as_str)
            == Some(&r.url);
        if !((r.method == "GET" && (discovery || jwks))
            || (r.method == "POST" && token && !s.poisoned))
        {
            return Err(if s.poisoned {
                "OUTCOME_UNKNOWN"
            } else {
                "INVALID_REQUEST"
            });
        };
        for (k, v) in &r.headers {
            if !matches!(k.to_ascii_lowercase().as_str(), "accept" | "content-type")
                || v.len() > 256
                || v.contains(['\r', '\n'])
            {
                return Err("INVALID_REQUEST");
            }
        }
        if token {
            let body = r.body.as_deref().ok_or("INVALID_REQUEST")?;
            let pairs: Vec<_> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            let one = |k: &str| {
                let mut v = pairs.iter().filter(|(key, _)| key == k);
                let first = v.next()?.1.as_str();
                if v.next().is_some() {
                    None
                } else {
                    Some(first)
                }
            };
            if one("client_id") != Some(self.binding.client_id.as_str()) {
                return Err("BINDING_MISMATCH");
            };
            match one("grant_type") {
                Some("authorization_code") => {
                    if s.refresh_hash.is_some()
                        || one("redirect_uri") != Some(self.binding.redirect_uri.as_str())
                        || one("code").is_none_or(str::is_empty)
                        || one("code_verifier").is_none_or(str::is_empty)
                    {
                        return Err("INVALID_REQUEST");
                    }
                }
                Some("refresh_token") => {
                    let token = one("refresh_token").ok_or("INVALID_REQUEST")?;
                    let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
                    if s.refresh_hash != Some(hash)
                        || one("resource").is_some_and(|v| v != self.binding.resource)
                    {
                        return Err("BINDING_MISMATCH");
                    }
                }
                _ => return Err("INVALID_REQUEST"),
            }
        }
        Ok(())
    }
    async fn send(&self, r: FetchRequest) -> Result<FetchResponse, &'static str> {
        let token = r.method == "POST";
        let result = async {
            let mut request = self.http.request(
                if token {
                    reqwest::Method::POST
                } else {
                    reqwest::Method::GET
                },
                &r.url,
            );
            for (k, v) in r.headers {
                request = request.header(k, v)
            }
            if let Some(b) = r.body {
                request = request.body(b)
            };
            let mut res = request.send().await.map_err(|_| "OUTCOME_UNKNOWN")?;
            let status = res.status().as_u16();
            let mut bytes = Vec::new();
            while let Some(chunk) = res.chunk().await.map_err(|_| "OUTCOME_UNKNOWN")? {
                if bytes.len() + chunk.len() > 131072 {
                    return Err("OUTCOME_UNKNOWN");
                };
                bytes.extend_from_slice(&chunk)
            }
            let body = String::from_utf8(bytes).map_err(|_| "OUTCOME_UNKNOWN")?;
            if token {
                self.state.lock().await.received_reply = Some(FetchResponse {
                    status,
                    headers: BTreeMap::new(),
                    body: body.clone(),
                });
            }
            if token && status >= 500 {
                return Err("OUTCOME_UNKNOWN");
            }
            if !(200..300).contains(&status) {
                return Ok(FetchResponse {
                    status,
                    headers: BTreeMap::new(),
                    body: "{}".into(),
                });
            };
            let value: Value = serde_json::from_str(&body).map_err(|_| "OUTCOME_UNKNOWN")?;
            let mut s = self.state.lock().await;
            if token {
                if let Some(rt) = value.get("refresh_token").and_then(Value::as_str) {
                    if rt.is_empty() || rt.len() > 16384 {
                        return Err("OUTCOME_UNKNOWN");
                    };
                    s.refresh_hash = Some(Sha256::digest(rt.as_bytes()).into());
                }
            } else if r.url.contains(".well-known/") {
                let base = self.binding.issuer.trim_end_matches('/');
                if value.get("issuer").and_then(Value::as_str) != Some(&self.binding.issuer)
                    || value.get("token_endpoint").and_then(Value::as_str)
                        != Some(&format!("{base}/token"))
                {
                    return Err("BINDING_MISMATCH");
                };
                for key in ["authorization_endpoint", "jwks_uri"] {
                    let raw = value
                        .get(key)
                        .and_then(Value::as_str)
                        .ok_or("BINDING_MISMATCH")?;
                    let u = Url::parse(raw).map_err(|_| "BINDING_MISMATCH")?;
                    if u.scheme() != "https"
                        || u.origin().ascii_serialization() != self.binding.endpoint
                        || u.query().is_some()
                        || u.fragment().is_some()
                        || !u.username().is_empty()
                        || u.password().is_some()
                    {
                        return Err("BINDING_MISMATCH");
                    }
                }
                s.discovery = Some(value);
            }
            Ok(FetchResponse {
                status,
                body,
                headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            })
        }
        .await;
        if token && result.is_err() {
            self.state.lock().await.poisoned = true;
        }
        result
    }
    pub async fn authorize(&self, raw: &str) -> Result<(), &'static str> {
        let u = Url::parse(raw).map_err(|_| "INVALID_AUTHORIZE")?;
        let s = self.state.lock().await;
        let base = s
            .discovery
            .as_ref()
            .and_then(|v| v.get("authorization_endpoint"))
            .and_then(Value::as_str)
            .ok_or("INVALID_AUTHORIZE")?;
        let mut root = u.clone();
        root.set_query(None);
        let pairs: Vec<_> = u.query_pairs().into_owned().collect();
        let q: BTreeMap<_, _> = pairs.iter().cloned().collect();
        let mut requested: Vec<_> = q
            .get("scope")
            .map(|s| s.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        requested.sort();
        let mut expected = self.binding.scopes.clone();
        expected.sort();
        if raw.len() > 16384
            || pairs.len() > 32
            || pairs.len() != q.len()
            || requested != expected
            || q.get("resource") != Some(&self.binding.resource)
        {
            return Err("INVALID_AUTHORIZE");
        }
        if root.as_str() != base
            || u.fragment().is_some()
            || q.get("client_id") != Some(&self.binding.client_id)
            || q.get("redirect_uri") != Some(&self.binding.redirect_uri)
            || q.get("response_type").map(String::as_str) != Some("code")
            || q.get("code_challenge_method").map(String::as_str) != Some("S256")
            || q.get("state").is_none_or(String::is_empty)
            || q.get("code_challenge").is_none_or(String::is_empty)
        {
            return Err("INVALID_AUTHORIZE");
        };
        Ok(())
    }
}
async fn wait(op: Arc<Operation>) -> Result<FetchResponse, &'static str> {
    loop {
        let n = op.done.notified();
        if let Some(result) = op.result.lock().await.clone() {
            return result;
        };
        n.await;
    }
}
#[cfg(test)]
mod tests;
