//! Internal Gateway-only listener. No browser cookies, accounts, or control APIs.
use axum::{
    extract::{
        ws::{Message, WebSocketUpgrade},
        DefaultBodyLimit, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures_util::SinkExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use workspace_core::{denied, equal, exact_host, one, response, secret, Workspace};
struct Context {
    host: String,
    bearer: String,
    workspace: Workspace,
    open: AtomicBool,
    stop: CancellationToken,
    streams: Arc<tokio::sync::Semaphore>,
}
pub struct GatewayUpstream {
    ctx: Arc<Context>,
    pub port: u16,
    task: Option<JoinHandle<()>>,
}
impl GatewayUpstream {
    pub async fn start(workspace: Workspace) -> Result<Self, &'static str> {
        Self::start_with_cancel(workspace, CancellationToken::new()).await
    }
    pub async fn start_with_cancel(
        workspace: Workspace,
        stop: CancellationToken,
    ) -> Result<Self, &'static str> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "UPSTREAM_BIND")?;
        let addr = listener.local_addr().map_err(|_| "UPSTREAM_BIND")?;
        let ctx = Arc::new(Context {
            host: addr.to_string(),
            bearer: secret(),
            workspace,
            open: AtomicBool::new(true),
            stop,
            streams: Arc::new(tokio::sync::Semaphore::new(32)),
        });
        let routes = Router::new()
            .route("/api/info", get(info))
            .route("/api/sessions", get(read).post(write))
            .route("/api/events/poll", get(read))
            .route("/ws/terminal/{id}", get(ws))
            .layer(DefaultBodyLimit::max(4096))
            .with_state(ctx.clone());
        let stop = ctx.stop.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, routes)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await;
        });
        Ok(Self {
            ctx,
            port: addr.port(),
            task: Some(task),
        })
    }
    // This capability only goes to the explicitly injected in-process Gateway factory.
    pub fn local_token(&self) -> &str {
        &self.ctx.bearer
    }
    pub fn seal(&self) {
        self.ctx.open.store(false, Ordering::SeqCst);
        self.ctx.stop.cancel();
    }
    pub async fn close(&mut self) {
        self.seal();
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
    }
}
impl Drop for GatewayUpstream {
    fn drop(&mut self) {
        self.seal();
    }
}
fn boundary(c: &Context, h: &HeaderMap) -> bool {
    c.open.load(Ordering::SeqCst)
        && !c.stop.is_cancelled()
        && exact_host(h, &c.host)
        && !h.contains_key("origin")
        && !h.contains_key("cookie")
}
fn authorized(c: &Context, h: &HeaderMap) -> bool {
    boundary(c, h)
        && one(h, "authorization")
            .and_then(|s| s.strip_prefix("Bearer "))
            .is_some_and(|v| equal(v, &c.bearer))
}
async fn info(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    if !authorized(&c, &h) {
        return denied();
    };
    response(
        StatusCode::OK,
        json!({"name":"Synthetic workspace","version":1}),
    )
}
async fn read(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    if !authorized(&c, &h) {
        return denied();
    };
    response(StatusCode::OK, c.workspace.read().await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    value: String,
}
async fn write(State(c): State<Arc<Context>>, h: HeaderMap, Json(v): Json<Write>) -> Response {
    if !authorized(&c, &h) {
        return denied();
    };
    match c.workspace.append(v.value).await {
        Ok(v) => response(StatusCode::OK, v),
        Err(_) => response(StatusCode::BAD_REQUEST, json!({"code":"LIMIT"})),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalInput {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}
async fn ws(State(c): State<Arc<Context>>, h: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    if !boundary(&c, &h) || h.contains_key("authorization") {
        return denied();
    };
    let Ok(permit) = c.streams.clone().try_acquire_owned() else {
        return denied();
    };
    upgrade
        .max_frame_size(4096)
        .max_message_size(4096)
        .on_upgrade(move |mut socket| async move {
            let _permit = permit;
            let first = tokio::select! {
                _ = c.stop.cancelled() => return,
                r = tokio::time::timeout(Duration::from_secs(3), socket.recv()) => r,
            };
            let good = matches!(first, Ok(Some(Ok(Message::Text(ref text)))) if
                serde_json::from_str::<Value>(text).ok()
                    .and_then(|value| value.get("token").and_then(Value::as_str)
                        .map(|value| equal(value, &c.bearer))) == Some(true));
            if !good || !c.open.load(Ordering::SeqCst) {
                let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
                return;
            }
            loop {
                tokio::select! {
                    biased;
                    _ = c.stop.cancelled() => break,
                    message = socket.recv() => match message {
                        Some(Ok(Message::Text(text))) => {
                            let Ok(input) = serde_json::from_str::<TerminalInput>(&text) else {
                                break;
                            };
                            if input.kind != "input" || input.data.len() > 4096 {
                                break;
                            }
                            if send(&mut socket, &c.stop, Message::Binary(input.data.into_bytes().into())).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            if send(&mut socket, &c.stop, Message::Pong(payload)).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {}
                        _ => break,
                    },
                }
            }
            let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
        })
        .into_response()
}
async fn send(
    socket: &mut axum::extract::ws::WebSocket,
    stop: &CancellationToken,
    message: Message,
) -> Result<(), ()> {
    tokio::select! {biased;_=stop.cancelled()=>Err(()),r=tokio::time::timeout(Duration::from_secs(1),socket.send(message))=>r.map_err(|_|())?.map_err(|_|())}
}
#[cfg(test)]
mod tests;
