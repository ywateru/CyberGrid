//! The browser cookie entry. It never accepts the Gateway's internal bearer.
use axum::{
    extract::{
        ws::{Message, WebSocketUpgrade},
        DefaultBodyLimit, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::SinkExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use workspace_core::{denied, equal, exact_host, one, response, secret, Workspace};
#[derive(Clone)]
struct Session {
    csrf: String,
    expires: Instant,
}
struct Context {
    host: String,
    origin: String,
    cookie: String,
    instance: String,
    workspace: Workspace,
    launches: Mutex<HashMap<String, Instant>>,
    sessions: Mutex<HashMap<String, Session>>,
    stop: CancellationToken,
    streams: Arc<tokio::sync::Semaphore>,
}
pub struct LocalWorkspace {
    ctx: Arc<Context>,
    task: Option<JoinHandle<()>>,
}
impl LocalWorkspace {
    pub async fn start(workspace: Workspace) -> Result<Self, &'static str> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "LOCAL_BIND")?;
        let host = listener.local_addr().map_err(|_| "LOCAL_BIND")?.to_string();
        let instance = secret();
        let ctx = Arc::new(Context {
            origin: format!("http://{host}"),
            host,
            cookie: format!("cg_{}", &instance[..16]),
            instance,
            workspace,
            launches: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            stop: CancellationToken::new(),
            streams: Arc::new(tokio::sync::Semaphore::new(32)),
        });
        let router = Router::new()
            .route("/", get(page))
            .route("/app.js", get(script))
            .route("/exchange", post(exchange))
            .route("/bootstrap", post(bootstrap))
            .route("/state", post(state))
            .route("/append", post(append))
            .route("/ws", get(ws))
            .layer(DefaultBodyLimit::max(4096))
            .with_state(ctx.clone());
        let stop = ctx.stop.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await;
        });
        Ok(Self {
            ctx,
            task: Some(task),
        })
    }
    pub fn origin(&self) -> &str {
        &self.ctx.origin
    }
    pub async fn open_url(&self) -> Result<String, &'static str> {
        self.open_url_for(Duration::from_secs(30)).await
    }
    async fn open_url_for(&self, ttl: Duration) -> Result<String, &'static str> {
        let mut launches = self.ctx.launches.lock().await;
        launches.retain(|_, deadline| *deadline > Instant::now());
        if launches.len() >= 16 {
            return Err("BUSY");
        };
        let token = secret();
        launches.insert(
            token.clone(),
            Instant::now() + ttl.min(Duration::from_secs(30)),
        );
        Ok(format!("{}/#{token}", self.origin()))
    }
    pub async fn close(&mut self) {
        self.ctx.stop.cancel();
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
    }
}
impl Drop for LocalWorkspace {
    fn drop(&mut self) {
        self.ctx.stop.cancel();
    }
}
fn boundary(c: &Context, h: &HeaderMap) -> bool {
    exact_host(h, &c.host)
        && one(h, "origin") == Some(c.origin.as_str())
        && !h.contains_key("authorization")
}
async fn session(c: &Context, h: &HeaderMap, csrf: bool) -> Option<Session> {
    if !boundary(c, h) {
        return None;
    };
    let cookie = one(h, "cookie")?;
    let mut matches = cookie
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .filter(|(k, _)| *k == c.cookie);
    let token = matches.next()?.1;
    if matches.next().is_some() {
        return None;
    };
    let mut sessions = c.sessions.lock().await;
    sessions.retain(|_, s| s.expires > Instant::now());
    let s = sessions.get(token)?.clone();
    if csrf && !one(h, "x-csrf-token").is_some_and(|v| equal(v, &s.csrf)) {
        return None;
    };
    Some(s)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    token: String,
}
async fn exchange(
    State(c): State<Arc<Context>>,
    h: HeaderMap,
    Json(body): Json<Launch>,
) -> Response {
    if !boundary(&c, &h) || body.token.len() != 64 {
        return denied();
    };
    let valid = c
        .launches
        .lock()
        .await
        .remove(&body.token)
        .is_some_and(|d| d > Instant::now());
    if !valid {
        return denied();
    };
    let mut sessions = c.sessions.lock().await;
    sessions.retain(|_, s| s.expires > Instant::now());
    if sessions.len() >= 16 {
        return response(StatusCode::TOO_MANY_REQUESTS, json!({"code":"LIMIT"}));
    };
    let token = secret();
    sessions.insert(
        token.clone(),
        Session {
            csrf: secret(),
            expires: Instant::now() + Duration::from_secs(3600),
        },
    );
    let mut r = response(StatusCode::OK, json!({"ok":true}));
    r.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=3600",
            c.cookie, token
        )
        .parse()
        .unwrap(),
    );
    r
}
async fn bootstrap(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    match session(&c, &h, false).await {
        Some(s) => response(StatusCode::OK, json!({"csrf":s.csrf,"instance":c.instance})),
        None => denied(),
    }
}
async fn state(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    if session(&c, &h, true).await.is_none() {
        return denied();
    };
    response(
        StatusCode::OK,
        json!({"workspace":c.workspace.read().await,"summary":c.workspace.summary().await}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    value: String,
}
async fn append(State(c): State<Arc<Context>>, h: HeaderMap, Json(body): Json<Write>) -> Response {
    if session(&c, &h, true).await.is_none() {
        return denied();
    };
    match c.workspace.append(body.value).await {
        Ok(v) => response(StatusCode::OK, v),
        Err(_) => response(StatusCode::BAD_REQUEST, json!({"code":"LIMIT"})),
    }
}
async fn ws(State(c): State<Arc<Context>>, h: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let Some(s) = session(&c, &h, false).await else {
        return denied();
    };
    let Ok(permit) = c.streams.clone().try_acquire_owned() else {
        return denied();
    };
    upgrade.max_message_size(4096).max_frame_size(4096).on_upgrade(move|mut socket|async move{
  let _permit=permit;let first=tokio::select!{_=c.stop.cancelled()=>return,r=tokio::time::timeout(Duration::from_secs(3),socket.recv())=>r};
  let valid=matches!(first,Ok(Some(Ok(Message::Text(ref t)))) if serde_json::from_str::<Value>(t).ok().and_then(|v|v.get("csrf").and_then(Value::as_str).map(|v|equal(v,&s.csrf)))==Some(true));if !valid{let _=socket.close().await;return};
  loop{tokio::select!{_=c.stop.cancelled()=>break,_=tokio::time::sleep_until(s.expires.into())=>break,m=socket.recv()=>match m{Some(Ok(Message::Text(t)))=>{if send(&mut socket,&c.stop,Message::Text(t)).await.is_err(){break}},Some(Ok(Message::Binary(b)))=>{if send(&mut socket,&c.stop,Message::Binary(b)).await.is_err(){break}},Some(Ok(Message::Ping(p)))=>{if send(&mut socket,&c.stop,Message::Pong(p)).await.is_err(){break}},Some(Ok(Message::Pong(_)))=>{},_=>break}}}let _=socket.close().await;
 }).into_response()
}
fn static_headers(mut r: Response) -> Response {
    r.headers_mut().insert(header::CONTENT_SECURITY_POLICY,"default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
    r.headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    r.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    r
}
async fn page(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    if !exact_host(&h, &c.host) {
        return denied();
    };
    static_headers(Html(include_str!("../index.html")).into_response())
}
async fn script(State(c): State<Arc<Context>>, h: HeaderMap) -> Response {
    if !exact_host(&h, &c.host) {
        return denied();
    };
    static_headers(
        (
            [(header::CONTENT_TYPE, "text/javascript")],
            include_str!("../app.js"),
        )
            .into_response(),
    )
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
