//! Synthetic RAM business state. No files, command execution, or cloud credentials.
use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

pub fn secret() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}
pub fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
pub fn one<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        None
    } else {
        Some(first)
    }
}
pub fn exact_host(headers: &HeaderMap, host: &str) -> bool {
    one(headers, "host") == Some(host)
        && !headers.contains_key("forwarded")
        && !headers.contains_key("x-forwarded-host")
        && !headers.contains_key("x-forwarded-proto")
}
pub fn response(status: StatusCode, body: Value) -> Response {
    let mut r = (status, Json(body)).into_response();
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    r.headers_mut().insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    r
}
pub fn denied() -> Response {
    response(StatusCode::FORBIDDEN, json!({"code":"FORBIDDEN"}))
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub account: Option<String>,
    pub remote: String,
}
#[derive(Default)]
struct Data {
    revision: u64,
    values: Vec<String>,
    account: Option<String>,
}
#[derive(Clone, Default)]
pub struct Workspace {
    data: Arc<RwLock<Data>>,
    remote_gate: Arc<AtomicBool>,
}
impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_remote_gate(remote_gate: Arc<AtomicBool>) -> Self {
        Self {
            remote_gate,
            ..Self::default()
        }
    }
    pub async fn summary(&self) -> Summary {
        let account = self.data.read().await.account.clone();
        // Read after the asynchronous account lock: closing the Host gate never
        // waits for an asynchronous summary update or a stale enable/cleanup task.
        Summary {
            account,
            remote: if self.remote_gate.load(Ordering::SeqCst) {
                "on"
            } else {
                "off"
            }
            .into(),
        }
    }
    pub async fn set_account(&self, account: Option<String>) {
        self.data.write().await.account = account;
    }
    pub async fn read(&self) -> Value {
        let s = self.data.read().await;
        json!({"revision":s.revision,"values":s.values})
    }
    pub async fn append(&self, value: String) -> Result<Value, &'static str> {
        if value.len() > 1024 {
            return Err("LIMIT");
        };
        let mut s = self.data.write().await;
        if s.values.len() >= 64 {
            s.values.remove(0);
        }
        s.revision += 1;
        s.values.push(value);
        Ok(json!({"revision":s.revision,"count":s.values.len()}))
    }
}
