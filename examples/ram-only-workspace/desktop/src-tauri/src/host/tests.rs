use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message as LocalMessage},
};

// Exercise the real cookie/Origin/CSRF entry and retain the same local session
// and WebSocket across remote lifecycle changes. No cloud browser login is used.
struct LocalProbe {
    http: reqwest::Client,
    origin: String,
    cookie: String,
    csrf: String,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}
impl LocalProbe {
    async fn start(host: &Host) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let origin = host.local.origin().to_owned();
        let launch = host.local.open_url().await.unwrap();
        let response = http
            .post(format!("{origin}/exchange"))
            .header("origin", &origin)
            .json(&serde_json::json!({"token":launch.split('#').nth(1).unwrap()}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let bootstrap: serde_json::Value = http
            .post(format!("{origin}/bootstrap"))
            .header("origin", &origin)
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let csrf = bootstrap["csrf"].as_str().unwrap().to_owned();
        let mut request = (origin.replace("http:", "ws:") + "/ws")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        let (mut ws, _) = connect_async(request).await.unwrap();
        ws.send(LocalMessage::Text(
            serde_json::json!({"csrf":csrf}).to_string().into(),
        ))
        .await
        .unwrap();
        Self {
            http,
            origin,
            cookie,
            csrf,
            ws,
        }
    }
    async fn assert_summary(&mut self, account: Option<&str>, remote: &str) {
        let response = self
            .http
            .post(format!("{}/state", self.origin))
            .header("origin", &self.origin)
            .header("cookie", &self.cookie)
            .header("x-csrf-token", &self.csrf)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let value: serde_json::Value = response.json().await.unwrap();
        assert_eq!(value["summary"]["remote"], remote);
        assert_eq!(value["summary"]["account"].as_str(), account);
        self.ws
            .send(LocalMessage::Text("local-still-live".into()))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), self.ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            LocalMessage::Text("local-still-live".into())
        );
    }
}
#[tokio::test]
async fn no_gateway_never_enables_or_registers_and_local_service_remains_live() {
    let host = Host::new(BuildConfig::default(), None).await.unwrap();
    let first = host.local.origin().to_owned();
    assert_eq!(host.enable().await, Err("GATEWAY_UNAVAILABLE"));
    assert!(host.view().await.local_closed);
    assert!(host.state.lock().await.client.is_none());
    let (a, b) = tokio::join!(host.disable(), host.disable());
    a.unwrap();
    b.unwrap();
    assert!(host.view().await.local_closed);
    assert_eq!(host.local.origin(), first);
    assert!(reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(&first)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    assert!(host.local.open_url().await.unwrap().contains('#'));
}
use crate::gateway_host::{CloseFuture, GatewayIdentity, GatewayLease, StartFuture};
use std::{net::SocketAddr, sync::atomic::AtomicUsize};
struct PausedFactory {
    entered: tokio::sync::mpsc::UnboundedSender<(u16, String)>,
    release: Arc<tokio::sync::Semaphore>,
    closed: Arc<AtomicUsize>,
}
struct Lease {
    endpoints: tokio::sync::watch::Sender<crate::gateway_host::GatewayEndpointState>,
    closed: Arc<AtomicUsize>,
}
impl GatewayLease for Lease {
    fn ingress(&self) -> SocketAddr {
        "127.0.0.1:9".parse().unwrap()
    }
    fn capabilities(&self) -> crate::gateway_host::GatewayCapabilities {
        crate::gateway_host::GatewayCapabilities {
            protocol: "anywork.remote.v1".into(),
            limits: cybergrid_sdk::BridgeLimits {
                max_frame_bytes: 65535,
                max_streams: 2,
                max_pending_requests: 16,
            },
        }
    }
    fn endpoint_updates(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::gateway_host::GatewayEndpointState> {
        self.endpoints.subscribe()
    }
    fn identity(&self) -> GatewayIdentity {
        GatewayIdentity {
            installation_id: "synthetic".into(),
            public_key: "synthetic".into(),
        }
    }
    fn close(self: Box<Self>) -> CloseFuture {
        Box::pin(async move {
            self.closed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}
impl GatewayFactory for PausedFactory {
    fn start(&self, c: LaunchContext) -> StartFuture {
        let release = self.release.clone();
        let closed = self.closed.clone();
        self.entered.send((c.local_port, c.local_token)).unwrap();
        Box::pin(async move {
            let p = release.acquire().await.unwrap();
            p.forget();
            Ok(Box::new(Lease {
                endpoints: tokio::sync::watch::channel(
                    crate::gateway_host::GatewayEndpointState::Pending,
                )
                .0,
                closed,
            }) as Box<dyn GatewayLease>)
        })
    }
}
#[tokio::test]
async fn actual_host_disable_seals_pending_gateway_immediately_and_joins_late_start_once() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let closed = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(PausedFactory {
        entered: tx,
        release: release.clone(),
        closed: closed.clone(),
    });
    let cfg = BuildConfig {
        spa_client: "spa".into(),
        product: "fixture".into(),
        browser_origins: vec!["https://web.example.test".into()],
        ..BuildConfig::default()
    };
    let host = Host::new(cfg, Some(factory)).await.unwrap();
    // Seed only the already-verified account fixture. This test does not claim cloud login.
    {
        let mut s = host.state.lock().await;
        s.account = Some("verified-fixture".into());
        s.sdk = 3;
        s.client = Some(host.client().unwrap());
    }
    let origin = host.local.origin().to_owned();
    let h = host.clone();
    let enable = tokio::spawn(async move { h.enable().await });
    let (port, token) = rx.recv().await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("http://127.0.0.1:{port}/api/info");
    assert!(http
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let h = host.clone();
    let disable = tokio::spawn(async move { h.disable().await });
    for _ in 0..50 {
        if host.view().await.remote == "disabling" {
            break;
        };
        tokio::task::yield_now().await;
    }
    assert!(host.view().await.local_closed);
    assert_eq!(host.view().await.remote, "disabling");
    let r = http.get(&url).bearer_auth(&token).send().await;
    assert!(r.is_err() || r.unwrap().status() == reqwest::StatusCode::FORBIDDEN);
    assert!(!disable.is_finished());
    assert!(!enable.is_finished());
    assert!(http
        .get(&origin)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let h = host.clone();
    let repeat = tokio::spawn(async move { h.disable().await });
    release.add_permits(1);
    assert!(enable.await.unwrap().is_err());
    disable.await.unwrap().unwrap();
    repeat.await.unwrap().unwrap();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(host.view().await.local_closed);
    assert_eq!(host.view().await.remote, "off");
    assert_eq!(host.local.origin(), origin);
}

struct BindFactory {
    upstream: tokio::sync::mpsc::UnboundedSender<(u16, String)>,
    bound: tokio::sync::mpsc::UnboundedSender<SocketAddr>,
    ingress: SocketAddr,
    release: Arc<tokio::sync::Semaphore>,
    settled: Arc<AtomicBool>,
    closed: Arc<AtomicUsize>,
}
struct BindLease {
    endpoints: tokio::sync::watch::Sender<crate::gateway_host::GatewayEndpointState>,
    bound: tokio::sync::mpsc::UnboundedSender<SocketAddr>,
    ingress: SocketAddr,
    release: Arc<tokio::sync::Semaphore>,
    settled: Arc<AtomicBool>,
    closed: Arc<AtomicUsize>,
}
impl GatewayLease for BindLease {
    fn ingress(&self) -> SocketAddr {
        self.ingress
    }
    fn capabilities(&self) -> crate::gateway_host::GatewayCapabilities {
        crate::gateway_host::GatewayCapabilities {
            protocol: "anywork.remote.v1".into(),
            limits: cybergrid_sdk::BridgeLimits {
                max_frame_bytes: 65535,
                max_streams: 2,
                max_pending_requests: 16,
            },
        }
    }
    fn endpoint_updates(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::gateway_host::GatewayEndpointState> {
        self.endpoints.subscribe()
    }
    fn identity(&self) -> GatewayIdentity {
        GatewayIdentity {
            installation_id: "synthetic-held-bind".into(),
            public_key: "not-a-crypto-identity".into(),
        }
    }
    fn bind_guarded_ingress<'a>(
        &'a mut self,
        address: SocketAddr,
        cancel: CancellationToken,
    ) -> crate::gateway_host::BindFuture<'a> {
        Box::pin(async move {
            self.bound.send(address).unwrap();
            let permit = self.release.acquire().await.unwrap();
            permit.forget();
            assert!(cancel.is_cancelled());
            self.settled.store(true, Ordering::SeqCst);
            // Deliberately return late success; the host must never commit it.
            Ok(())
        })
    }
    fn close(self: Box<Self>) -> CloseFuture {
        Box::pin(async move {
            assert!(self.settled.load(Ordering::SeqCst));
            self.closed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}
impl GatewayFactory for BindFactory {
    fn start(&self, c: LaunchContext) -> StartFuture {
        self.upstream.send((c.local_port, c.local_token)).unwrap();
        let lease = BindLease {
            endpoints: tokio::sync::watch::channel(
                crate::gateway_host::GatewayEndpointState::Pending,
            )
            .0,
            bound: self.bound.clone(),
            ingress: self.ingress,
            release: self.release.clone(),
            settled: self.settled.clone(),
            closed: self.closed.clone(),
        };
        Box::pin(async move { Ok(Box::new(lease) as Box<dyn GatewayLease>) })
    }
}
#[tokio::test]
async fn actual_host_held_bind_closes_gate_and_upstream_before_owned_bind_settles() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{timeout, Duration};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ingress = listener.local_addr().unwrap();
    let echo = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 5];
        socket.read_exact(&mut bytes).await.unwrap();
        socket.write_all(&bytes).await.unwrap();
        // The independent host gate must close both sides, without lease.close.
        assert_eq!(socket.read(&mut bytes).await.unwrap(), 0);
    });
    let (upstream_tx, mut upstream_rx) = tokio::sync::mpsc::unbounded_channel();
    let (bound_tx, mut bound_rx) = tokio::sync::mpsc::unbounded_channel();
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let settled = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            spa_client: "spa".into(),
            product: "fixture".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(BindFactory {
            upstream: upstream_tx,
            bound: bound_tx,
            ingress,
            release: release.clone(),
            settled: settled.clone(),
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    {
        let mut s = host.state.lock().await;
        s.account = Some("verified-fixture".into());
        s.sdk = 3;
        s.client = Some(host.client().unwrap());
    }
    let mut local = LocalProbe::start(&host).await;
    local.assert_summary(None, "off").await;
    let h = host.clone();
    let enable = tokio::spawn(async move { h.enable().await });
    let guarded = timeout(Duration::from_secs(1), bound_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_ne!(guarded, ingress);
    let (port, token) = upstream_rx.recv().await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let upstream = format!("http://127.0.0.1:{port}/api/info");
    assert!(http
        .get(&upstream)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let mut active = TcpStream::connect(guarded).await.unwrap();
    active.write_all(b"hello").await.unwrap();
    let mut bytes = [0; 5];
    active.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"hello");
    let state_hold = host.state.lock().await;
    let h = host.clone();
    let mut disable = Box::pin(async move { h.disable().await });
    assert!(futures_util::poll!(disable.as_mut()).is_pending());
    local.assert_summary(None, "off").await;
    // Even with state held, close admission immediately seals the real streams.
    let h = host.clone();
    let mut newer_enable = Box::pin(async move { h.enable().await });
    assert!(matches!(
        futures_util::poll!(newer_enable.as_mut()),
        std::task::Poll::Ready(Err("REMOTE_BUSY"))
    ));
    assert_eq!(
        timeout(Duration::from_secs(1), active.read(&mut bytes))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    timeout(Duration::from_secs(1), echo)
        .await
        .unwrap()
        .unwrap();
    let result = http.get(&upstream).bearer_auth(&token).send().await;
    assert!(result.is_err() || result.unwrap().status() == reqwest::StatusCode::FORBIDDEN);
    assert!(TcpStream::connect(guarded).await.is_err());
    assert!(!settled.load(Ordering::SeqCst));
    assert_eq!(closed.load(Ordering::SeqCst), 0);
    assert!(!enable.is_finished());
    assert!(http
        .get(host.local.origin())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    // Cancel only the public waiter while its owned close is queued on state.
    drop(disable);
    drop(state_hold);
    timeout(Duration::from_secs(1), async {
        loop {
            if host.view().await.remote == "disabling" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(host.view().await.local_closed);
    assert!(!enable.is_finished());
    assert!(host.remote_control.lock().unwrap().closing > 0);
    let h = host.clone();
    let repeated = tokio::spawn(async move { h.disable().await });
    release.add_permits(1);
    timeout(Duration::from_secs(2), async {
        assert_eq!(enable.await.unwrap(), Err("REMOTE_START_FAILED"));
        repeated.await.unwrap().unwrap();
    })
    .await
    .unwrap();
    assert!(settled.load(Ordering::SeqCst));
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(host.view().await.local_closed);
    assert_eq!(host.view().await.remote, "off");
    local.assert_summary(None, "off").await;
    assert!(http
        .get(host.local.origin())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
}

#[tokio::test]
async fn queued_public_enable_is_fenced_before_start_even_when_disable_waiter_is_cancelled() {
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (upstream_tx, mut upstream_rx) = tokio::sync::mpsc::unbounded_channel();
    let (bound_tx, mut bound_rx) = tokio::sync::mpsc::unbounded_channel();
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            spa_client: "spa".into(),
            product: "fixture".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(BindFactory {
            upstream: upstream_tx,
            bound: bound_tx,
            ingress: target.local_addr().unwrap(),
            release: Arc::new(tokio::sync::Semaphore::new(0)),
            settled: Arc::new(AtomicBool::new(false)),
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    {
        let mut s = host.state.lock().await;
        s.account = Some("verified-fixture".into());
        s.sdk = 3;
        s.client = Some(host.client().unwrap());
    }
    // This is the independent counterexample's exact public-call ordering.
    let state_hold = host.state.lock().await;
    let h = host.clone();
    let mut enable = Box::pin(async move { h.enable().await });
    assert!(futures_util::poll!(enable.as_mut()).is_pending());
    let reserved_cancel = host.remote_control.lock().unwrap().cancel.clone();
    let h = host.clone();
    let mut disable = Box::pin(async move { h.disable().await });
    assert!(futures_util::poll!(disable.as_mut()).is_pending());
    assert!(reserved_cancel.is_cancelled());
    let h = host.clone();
    let mut newer_enable = Box::pin(async move { h.enable().await });
    assert!(matches!(
        futures_util::poll!(newer_enable.as_mut()),
        std::task::Poll::Ready(Err("REMOTE_BUSY"))
    ));
    drop(disable); // The close must still acquire state and finish.
    drop(state_hold);
    assert_eq!(
        timeout(Duration::from_secs(1), enable).await.unwrap(),
        Err("CANCELLED")
    );
    timeout(Duration::from_secs(1), async {
        loop {
            if host.remote_control.lock().unwrap().closing == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(host.state.lock().await.client.is_none());
    assert!(upstream_rx.try_recv().is_err()); // No bearer listener was created.
    assert!(bound_rx.try_recv().is_err()); // No guarded/public address was minted.
    assert_eq!(closed.load(Ordering::SeqCst), 0); // No lease exists to close.
    assert!(timeout(Duration::from_millis(30), target.accept())
        .await
        .is_err());
    assert!(host.view().await.local_closed);
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(http
        .get(host.local.origin())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let (first, duplicate) = tokio::join!(host.disable(), host.disable());
    first.unwrap();
    duplicate.unwrap();
    assert!(host.view().await.local_closed);
}

#[tokio::test]
async fn cancelled_queued_enable_releases_its_reservation_without_dispatch() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            spa_client: "spa".into(),
            product: "fixture".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(PausedFactory {
            entered: tx,
            release: Arc::new(tokio::sync::Semaphore::new(0)),
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    let state_hold = host.state.lock().await;
    let h = host.clone();
    let mut enable = Box::pin(async move { h.enable().await });
    assert!(futures_util::poll!(enable.as_mut()).is_pending());
    let reserved_cancel = host.remote_control.lock().unwrap().cancel.clone();
    drop(enable);
    assert!(reserved_cancel.is_cancelled());
    assert!(!host.remote_control.lock().unwrap().reserved);
    drop(state_hold);
    // A fresh request is admitted and reaches the real login-state check.
    assert_eq!(host.enable().await, Err("NOT_LOGGED_IN"));
    assert!(rx.try_recv().is_err());
    assert_eq!(closed.load(Ordering::SeqCst), 0);
    host.disable().await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(http
        .get(host.local.origin())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
}
// Actual coordinator/Redis/JWK fixture. Lifecycle ingress is synthetic TCP, not Noise/WAN.
struct PublishedFactory {
    address: SocketAddr,
    endpoint: tokio::sync::watch::Sender<crate::gateway_host::GatewayEndpointState>,
    closed: Arc<AtomicUsize>,
}
struct PublishedLease {
    address: SocketAddr,
    endpoint: tokio::sync::watch::Sender<crate::gateway_host::GatewayEndpointState>,
    closed: Arc<AtomicUsize>,
}
impl GatewayFactory for PublishedFactory {
    fn start(&self, _c: LaunchContext) -> StartFuture {
        let lease = PublishedLease {
            address: self.address,
            endpoint: self.endpoint.clone(),
            closed: self.closed.clone(),
        };
        Box::pin(async move { Ok(Box::new(lease) as Box<dyn GatewayLease>) })
    }
}
impl GatewayLease for PublishedLease {
    fn ingress(&self) -> SocketAddr {
        self.address
    }
    fn identity(&self) -> GatewayIdentity {
        GatewayIdentity {
            installation_id: "synthetic-installation".into(),
            public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        }
    }
    fn capabilities(&self) -> crate::gateway_host::GatewayCapabilities {
        crate::gateway_host::GatewayCapabilities {
            protocol: "anywork.remote.v1".into(),
            limits: cybergrid_sdk::BridgeLimits {
                max_frame_bytes: 65535,
                max_streams: 2,
                max_pending_requests: 8,
            },
        }
    }
    fn endpoint_updates(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::gateway_host::GatewayEndpointState> {
        self.endpoint.subscribe()
    }
    fn bind_guarded_ingress<'a>(
        &'a mut self,
        _a: SocketAddr,
        _c: CancellationToken,
    ) -> crate::gateway_host::BindFuture<'a> {
        Box::pin(async move {
            self.endpoint
                .send_replace(crate::gateway_host::GatewayEndpointState::Available(
                    crate::gateway_host::GatewayEndpoint {
                        revision: 1,
                        provider_id: "quick-tunnel".into(),
                        url: "wss://synthetic-fixture.trycloudflare.com/remote/v1/ws".into(),
                    },
                ));
            Ok(())
        })
    }
    fn close(self: Box<Self>) -> CloseFuture {
        Box::pin(async move {
            self.closed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}
struct CoordinatorFixture {
    child: tokio::process::Child,
    info: serde_json::Value,
}
impl CoordinatorFixture {
    async fn start() -> Self {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let mut child = tokio::process::Command::new("node")
            .arg("--import")
            .arg(root.join("server/node_modules/tsx/dist/loader.mjs"))
            .arg(root.join("sdk-rust/test-support/coordinator.ts"))
            .env("REQUIRE_REDIS", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            reader.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        Self {
            child,
            info: serde_json::from_str(&line).expect("private fixture readiness"),
        }
    }
    async fn stop(mut self) {
        self.child.stdin.take();
        tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait())
            .await
            .unwrap()
            .unwrap();
    }
}
impl Drop for CoordinatorFixture {
    fn drop(&mut self) {
        self.child.stdin.take();
    }
}
#[tokio::test]
#[ignore = "complete repository Node/server dependencies and real Redis required"]
async fn real_local_summary_tracks_host_gate_and_logout_without_replacing_local_session() {
    use crate::gateway_host::GatewayEndpointState;
    let fixture = CoordinatorFixture::start().await;
    let (endpoint, _) = tokio::sync::watch::channel(GatewayEndpointState::Pending);
    let closed = Arc::new(AtomicUsize::new(0));
    let config = BuildConfig {
        server: fixture.info["left"].as_str().unwrap().into(),
        issuer: "https://fixture.example.test/oidc".into(),
        client: "desktop".into(),
        resource: "https://fixture.example.test/api".into(),
        product: "desktop".into(),
        spa_client: "browser".into(),
        browser_origins: vec!["https://web.example.test".into()],
        ..BuildConfig::default()
    };
    // An empty, owned OAuth transport lets end_owner exercise its real barrier.
    // Tokens below come only from the controlled real ES384/JWK coordinator.
    let oauth = OAuthTransport::new(Binding {
        issuer: config.issuer.clone(),
        endpoint: "https://fixture.example.test".into(),
        client_id: config.client.clone(),
        resource: config.resource.clone(),
        scopes: config.scopes.clone(),
        redirect_uri: REDIRECT.into(),
    })
    .unwrap();
    let host = Host::new(
        config,
        Some(Arc::new(PublishedFactory {
            address: "127.0.0.1:9".parse().unwrap(),
            endpoint,
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    let mut local = LocalProbe::start(&host).await;
    local.assert_summary(None, "off").await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    host.broker
        .attach(
            1,
            Arc::new(move |r| tx.send(r).map_err(|_| "fixture closed")),
        )
        .await
        .unwrap();
    {
        let mut s = host.state.lock().await;
        s.owner = 1;
        s.oauth = Some(oauth);
    }
    let broker = host.broker.clone();
    let token = fixture.info["tokens"]["desktop"]
        .as_str()
        .unwrap()
        .to_owned();
    let replies = tokio::spawn(async move {
        while let Some(r) = rx.recv().await {
            broker
                .reply(crate::auth_provider::TokenReply {
                    request_id: r.request_id,
                    owner_generation: r.owner_generation,
                    sdk_generation: r.sdk_generation,
                    access_token: Some(token.clone()),
                    failure: None,
                })
                .await
                .unwrap();
        }
    });
    let sdk = host.prepare_sdk(1).await.unwrap();
    host.login(1, sdk).await.unwrap();
    let account = host.workspace.summary().await.account.unwrap();
    local.assert_summary(Some(&account), "off").await;
    host.enable().await.unwrap();
    assert!(!host.view().await.local_closed);
    local.assert_summary(Some(&account), "on").await;
    // Synchronous seal is visible even while the asynchronous Host state lock
    // is held. Neither a summary writer nor cloud cleanup can be required.
    let state = host.state.lock().await;
    host.seal();
    local.assert_summary(Some(&account), "off").await;
    drop(state);
    host.disable().await.unwrap();
    local.assert_summary(Some(&account), "off").await;
    let sdk = host.prepare_sdk(1).await.unwrap();
    host.login(1, sdk).await.unwrap();
    host.enable().await.unwrap();
    local.assert_summary(Some(&account), "on").await;
    let state = host.state.lock().await;
    let mut logout = Box::pin(host.end_owner(1));
    assert!(futures_util::poll!(logout.as_mut()).is_pending());
    local.assert_summary(Some(&account), "off").await;
    drop(state);
    logout.await.unwrap();
    local.assert_summary(None, "off").await;
    assert_eq!(closed.load(Ordering::SeqCst), 2);
    assert!(host.state.lock().await.oauth.is_none());
    local.ws.close(None).await.unwrap();
    replies.abort();
    fixture.stop().await;
}
#[tokio::test]
#[ignore = "complete repository Node/server dependencies and real Redis required"]
async fn real_sdk_publication_and_synchronous_native_close_preserve_local_workspace() {
    use crate::gateway_host::GatewayEndpointState;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fixture = CoordinatorFixture::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo = listener.local_addr().unwrap();
    let echo_task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    let (endpoint, _) = tokio::sync::watch::channel(GatewayEndpointState::Pending);
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            server: fixture.info["left"].as_str().unwrap().into(),
            issuer: "https://fixture.example.test/oidc".into(),
            client: "desktop".into(),
            resource: "https://fixture.example.test/api".into(),
            product: "desktop".into(),
            spa_client: "browser".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(PublishedFactory {
            address: echo,
            endpoint: endpoint.clone(),
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    host.broker
        .attach(
            1,
            Arc::new(move |r| tx.send(r).map_err(|_| "fixture closed")),
        )
        .await
        .unwrap();
    host.broker.bind(1, 1).await.unwrap();
    {
        let mut s = host.state.lock().await;
        s.owner = 1;
        s.sdk = 1;
    }
    let broker = host.broker.clone();
    let token = fixture.info["tokens"]["desktop"]
        .as_str()
        .unwrap()
        .to_owned();
    let replies = tokio::spawn(async move {
        while let Some(r) = rx.recv().await {
            broker
                .reply(crate::auth_provider::TokenReply {
                    request_id: r.request_id,
                    owner_generation: r.owner_generation,
                    sdk_generation: r.sdk_generation,
                    access_token: Some(token.clone()),
                    failure: None,
                })
                .await
                .unwrap();
        }
    });
    host.login(1, 1).await.unwrap();
    let native = host.state.lock().await.client.clone().unwrap();
    native
        .expose(cybergrid_sdk::Port::tcp("echo", echo.port(), "synthetic"))
        .await
        .unwrap();
    assert!(!native.remote_access_enabled());
    host.enable().await.unwrap();
    assert_eq!(host.view().await.browser_bridge, "connectable");
    let snapshot = native.directory(None).await.unwrap();
    let session = native.device_session().await.unwrap();
    let published = snapshot
        .devices
        .iter()
        .find(|d| d.id == session.device_id)
        .unwrap();
    assert_eq!(published.bridges.len(), 1);
    assert_eq!(published.bridges[0].limits.max_pending_requests, 8);
    assert_eq!(published.bridges[0].address_revision, 1);
    assert_eq!(published.incarnation, session.incarnation);
    let remote = Client::new(Config {
        base_url: fixture.info["right"].as_str().unwrap().into(),
        app_id: "mobile".into(),
        stun_servers: vec![],
        direct_timeout: std::time::Duration::from_secs(5),
        ..Config::default()
    })
    .unwrap();
    remote
        .login_with_access_token(fixture.info["tokens"]["mobile"].as_str().unwrap(), "peer")
        .await
        .unwrap();
    let mapping = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        remote.map(&session.device_id, "echo", "127.0.0.1:0".parse().unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    let mut stream = tokio::net::TcpStream::connect(mapping.addr).await.unwrap();
    stream.write_all(b"before").await.unwrap();
    let mut bytes = [0; 6];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"before");
    // Real wall-time default renewal, not a shortened clock or private flag.
    let expiry = published.bridges[0].expires_at.clone();
    tokio::time::sleep(std::time::Duration::from_secs(61)).await;
    let renewed = native.directory(None).await.unwrap();
    let renewed = renewed
        .devices
        .iter()
        .find(|d| d.id == session.device_id)
        .unwrap();
    assert_eq!(renewed.bridges[0].address_revision, 1);
    assert!(renewed.bridges[0].expires_at > expiry);
    assert!(renewed.publish_revision > published.publish_revision);
    stream.write_all(b"renew!").await.unwrap();
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"renew!");
    host.seal();
    assert!(!native.remote_access_enabled());
    assert_eq!(
        native.snapshot().publication.phase,
        cybergrid_sdk::StatePhase::Disabled
    );
    let ended = tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut bytes))
        .await
        .unwrap();
    assert!(ended.is_err() || ended.unwrap() == 0);
    host.disable().await.unwrap();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(host.view().await.local_closed);
    assert!(reqwest::get(host.local.origin())
        .await
        .unwrap()
        .status()
        .is_success());
    remote.close().await.unwrap();
    mapping.close().await;
    replies.abort();
    echo_task.abort();
    fixture.stop().await;
}
#[tokio::test]
#[ignore = "complete repository Node/server dependencies and real Redis required"]
async fn actual_endpoint_loss_seals_native_and_withdraws_without_any_browser_poll() {
    use crate::gateway_host::GatewayEndpointState;
    let fixture = CoordinatorFixture::start().await;
    let (endpoint, _) = tokio::sync::watch::channel(GatewayEndpointState::Pending);
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            server: fixture.info["left"].as_str().unwrap().into(),
            issuer: "https://fixture.example.test/oidc".into(),
            client: "desktop".into(),
            resource: "https://fixture.example.test/api".into(),
            product: "desktop".into(),
            spa_client: "browser".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(PublishedFactory {
            address: "127.0.0.1:9".parse().unwrap(),
            endpoint: endpoint.clone(),
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    let client = host.client().unwrap();
    let session = client
        .login_with_access_token(
            fixture.info["tokens"]["desktop"].as_str().unwrap(),
            "fixture",
        )
        .await
        .unwrap();
    {
        let mut s = host.state.lock().await;
        s.account = Some(session.user.id);
        s.client = Some(client.clone());
        s.sdk = 1;
    }
    host.enable().await.unwrap();
    assert!(client.remote_access_enabled());
    let mut local = LocalProbe::start(&host).await;
    let state = host.state.lock().await;
    endpoint.send_replace(GatewayEndpointState::Unavailable { revision: 2 });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while client.remote_access_enabled() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // The synchronous gate and reason do not wait for this asynchronous lock.
    assert!(!host.gate.load(Ordering::SeqCst));
    assert_eq!(
        host.remote_control
            .lock()
            .unwrap()
            .close_reason
            .map(AutomaticCloseReason::code),
        Some("GATEWAY_ENDPOINT_UNAVAILABLE")
    );
    drop(state);
    host.disable().await.unwrap();
    assert!(host.view().await.local_closed);
    assert_eq!(
        host.view().await.error,
        Some("GATEWAY_ENDPOINT_UNAVAILABLE")
    );
    local.assert_summary(None, "off").await;
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(reqwest::get(host.local.origin())
        .await
        .unwrap()
        .status()
        .is_success());
    fixture.stop().await;
}

#[tokio::test]
async fn automatic_close_reason_is_first_cause_and_cannot_cross_generation() {
    let host = Host::new(BuildConfig::default(), None).await.unwrap();
    let mut local = LocalProbe::start(&host).await;
    let first = host.reserve_enable().unwrap();
    host.gate.store(true, Ordering::SeqCst);
    let state = host.state.lock().await;
    host.seal_generation_with_reason(
        first.generation,
        AutomaticCloseReason::PublicationFailure("DIRECTORY_UNAVAILABLE"),
    );
    assert!(!host.gate.load(Ordering::SeqCst));
    host.seal_generation_with_reason(
        first.generation,
        AutomaticCloseReason::PublicationOwnerEnded,
    );
    drop(state);
    host.disable().await.unwrap();
    assert_eq!(host.view().await.error, Some("DIRECTORY_UNAVAILABLE"));
    drop(first);
    let second = host.reserve_enable().unwrap();
    host.gate.store(true, Ordering::SeqCst);
    host.seal_generation_with_reason(
        second.generation - 1,
        AutomaticCloseReason::PublicationExpired,
    );
    assert!(host.gate.load(Ordering::SeqCst));
    assert_eq!(host.view().await.error, None);
    // External errors are never echoed, even when they are static strings.
    host.seal_generation_with_reason(
        second.generation,
        AutomaticCloseReason::PublicationFailure("untrusted fixture detail"),
    );
    assert_eq!(
        host.view().await.error,
        Some("REMOTE_START_OR_PUBLICATION_FAILED")
    );
    local.assert_summary(None, "off").await;
}

#[tokio::test]
#[ignore = "complete repository Node/server dependencies and real Redis required; actual 60s publisher tick"]
async fn actual_publication_http_failure_records_reason_and_keeps_local_stream() {
    use crate::gateway_host::GatewayEndpointState;
    let fixture = CoordinatorFixture::start().await;
    let (endpoint, _) = tokio::sync::watch::channel(GatewayEndpointState::Pending);
    let closed = Arc::new(AtomicUsize::new(0));
    let host = Host::new(
        BuildConfig {
            server: fixture.info["left"].as_str().unwrap().into(),
            issuer: "https://fixture.example.test/oidc".into(),
            client: "desktop".into(),
            resource: "https://fixture.example.test/api".into(),
            product: "desktop".into(),
            spa_client: "browser".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(PublishedFactory {
            address: "127.0.0.1:9".parse().unwrap(),
            endpoint,
            closed: closed.clone(),
        })),
    )
    .await
    .unwrap();
    let client = host.client().unwrap();
    let session = client
        .login_with_access_token(
            fixture.info["tokens"]["desktop"].as_str().unwrap(),
            "fixture",
        )
        .await
        .unwrap();
    {
        let mut state = host.state.lock().await;
        state.account = Some(session.user.id);
        state.client = Some(client.clone());
        state.sdk = 1;
    }
    let mut local = LocalProbe::start(&host).await;
    host.enable().await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(http
        .post(format!(
            "{}/test/control",
            fixture.info["left"].as_str().unwrap()
        ))
        .json(&serde_json::json!({"configFailure":true}))
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    tokio::time::timeout(std::time::Duration::from_secs(65), async {
        while host.gate.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!client.remote_access_enabled());
    host.disable().await.unwrap();
    assert_eq!(host.view().await.error, Some("POLICY_UNAVAILABLE"));
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    local.assert_summary(None, "off").await;
    fixture.stop().await;
}

#[tokio::test]
#[ignore = "real Node/Redis/JWK stack and actual default 20s SDK heartbeat required"]
async fn real_sampled_publication_survives_later_same_intent_heartbeat() {
    assert_sampled_publication(SampledPublicationCase::LaterHeartbeat).await;
}

#[tokio::test]
#[ignore = "real Node/Redis/JWK stack required"]
async fn real_sampled_publication_rejects_obsolete_or_changed_confirmation() {
    for case in [
        SampledPublicationCase::OlderRevision,
        SampledPublicationCase::DifferentDescriptor,
        SampledPublicationCase::ChangedIntent,
        SampledPublicationCase::Cancelled,
    ] {
        assert_sampled_publication(case).await;
    }
}

#[derive(Clone, Copy, Debug)]
enum SampledPublicationCase {
    LaterHeartbeat,
    OlderRevision,
    DifferentDescriptor,
    ChangedIntent,
    Cancelled,
}

async fn assert_sampled_publication(case: SampledPublicationCase) {
    use crate::gateway_host::GatewayEndpointState;
    use axum::{body::Body, extract::Request, response::Response, routing::any, Router};
    let fixture = CoordinatorFixture::start().await;
    let upstream = fixture.info["left"].as_str().unwrap().to_owned();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let directory_count = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (sampled_tx, mut sampled_rx) = tokio::sync::mpsc::unbounded_channel();
    let route = {
        let release = release.clone();
        move |request: Request| {
            let upstream = upstream.clone();
            let http = http.clone();
            let directory_count = directory_count.clone();
            let release = release.clone();
            let sampled_tx = sampled_tx.clone();
            async move {
                let is_directory = request.uri().path() == "/api/v1/directory";
                let url = format!("{upstream}{}", request.uri());
                let (mut parts, body) = request.into_parts();
                for name in ["host", "connection", "transfer-encoding", "content-length"] {
                    parts.headers.remove(name);
                }
                let body = axum::body::to_bytes(body, 2 << 20).await.unwrap();
                let response = http
                    .request(parts.method, url)
                    .headers(parts.headers)
                    .body(body)
                    .send()
                    .await
                    .unwrap();
                let status = response.status();
                let mut headers = response.headers().clone();
                for name in ["connection", "transfer-encoding", "content-length"] {
                    headers.remove(name);
                }
                let mut bytes = response.bytes().await.unwrap();
                if is_directory && directory_count.fetch_add(1, Ordering::SeqCst) == 1 {
                    // Hold only the confirmation response, after the real server
                    // has atomically sampled Redis. Other HTTP operations proceed.
                    let mut body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    let revision = body["devices"][0]["publishRevision"].as_u64().unwrap();
                    sampled_tx.send(revision).unwrap();
                    release.acquire().await.unwrap().forget();
                    match case {
                        SampledPublicationCase::OlderRevision => {
                            body["devices"][0]["publishRevision"] = 0.into();
                        }
                        SampledPublicationCase::DifferentDescriptor => {
                            body["devices"][0]["bridges"][0]["url"] =
                                "wss://different-fixture.trycloudflare.com/remote/v1/ws".into();
                        }
                        _ => {}
                    }
                    bytes = serde_json::to_vec(&body).unwrap().into();
                }
                let mut response = Response::new(Body::from(bytes));
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                response
            }
        }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let stop = CancellationToken::new();
    let proxy_task = tokio::spawn({
        let stop = stop.clone();
        async move {
            axum::serve(listener, Router::new().fallback(any(route)))
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
                .unwrap();
        }
    });
    let (endpoint, _) = tokio::sync::watch::channel(GatewayEndpointState::Pending);
    let host = Host::new(
        BuildConfig {
            server: proxy,
            issuer: "https://fixture.example.test/oidc".into(),
            client: "desktop".into(),
            resource: "https://fixture.example.test/api".into(),
            product: "desktop".into(),
            spa_client: "browser".into(),
            browser_origins: vec!["https://web.example.test".into()],
            ..BuildConfig::default()
        },
        Some(Arc::new(PublishedFactory {
            address: "127.0.0.1:9".parse().unwrap(),
            endpoint,
            closed: Arc::new(AtomicUsize::new(0)),
        })),
    )
    .await
    .unwrap();
    let client = host.client().unwrap();
    let session = client
        .login_with_access_token(
            fixture.info["tokens"]["desktop"].as_str().unwrap(),
            "fixture",
        )
        .await
        .unwrap();
    {
        let mut state = host.state.lock().await;
        state.account = Some(session.user.id);
        state.client = Some(client.clone());
        state.sdk = 1;
    }
    let mut local = LocalProbe::start(&host).await;
    let enable = tokio::spawn({
        let host = host.clone();
        async move { host.enable().await }
    });
    let sampled_revision =
        tokio::time::timeout(std::time::Duration::from_secs(10), sampled_rx.recv())
            .await
            .unwrap()
            .unwrap();
    let intent = client.snapshot().intent_revision;
    match case {
        SampledPublicationCase::LaterHeartbeat => {
            tokio::time::timeout(std::time::Duration::from_secs(25), async {
                while client.snapshot().server_publish_revision.unwrap() <= sampled_revision {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let progressed = client.snapshot();
            assert_eq!(progressed.intent_revision, intent);
            assert_eq!(progressed.confirmed_intent_revision, Some(intent));
            assert_eq!(
                progressed.publication.phase,
                cybergrid_sdk::StatePhase::Ready
            );
        }
        SampledPublicationCase::ChangedIntent => {
            client
                .expose(cybergrid_sdk::Port::tcp("new-intent", 65000, "synthetic"))
                .await
                .unwrap();
            assert!(client.snapshot().intent_revision > intent);
        }
        SampledPublicationCase::Cancelled => host.seal(),
        _ => {}
    }
    release.add_permits(1);
    let result = enable.await.unwrap();
    // Always drain the real owner/fixture before asserting the regression.
    let was_open = !host.view().await.local_closed;
    let reason = host.view().await.error;
    host.disable().await.unwrap();
    local.assert_summary(None, "off").await;
    stop.cancel();
    proxy_task.await.unwrap();
    fixture.stop().await;
    println!("sampled_publication_case={case:?}; remote_open={was_open}; reason={reason:?}");
    if matches!(case, SampledPublicationCase::LaterHeartbeat) {
        assert_eq!(result, Ok(()), "unexpected fixed Host error: {reason:?}");
        assert!(was_open);
    } else {
        assert_eq!(result, Err("REMOTE_START_FAILED"));
        assert!(!was_open);
        if !matches!(case, SampledPublicationCase::Cancelled) {
            assert_eq!(reason, Some("PUBLICATION_UNCONFIRMED"));
        }
    }
}
