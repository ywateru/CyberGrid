use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Lease {
    endpoints: tokio::sync::watch::Sender<crate::gateway_host::GatewayEndpointState>,
    address: SocketAddr,
    closed: Arc<AtomicUsize>,
}
impl GatewayLease for Lease {
    fn ingress(&self) -> SocketAddr {
        self.address
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
            installation_id: "fixture".into(),
            public_key: "synthetic-not-a-crypto-identity".into(),
        }
    }
    fn close(self: Box<Self>) -> CloseFuture {
        Box::pin(async move {
            self.closed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}
struct Factory {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Semaphore>,
    closed: Arc<AtomicUsize>,
}
#[tokio::test]
async fn default_bind_hook_is_unavailable_and_closes_the_actual_lease() {
    let factory = Arc::new(Factory {
        entered: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Semaphore::new(1)),
        closed: Arc::new(AtomicUsize::new(0)),
    });
    let result = GatewayRuntime::create(
        factory.clone(),
        Workspace::new(),
        LaunchContext {
            generation: 1,
            local_port: 0,
            local_token: String::new(),
            issuer: "https://issuer.test/oidc".into(),
            resource: "https://resource.test".into(),
            allowed_spa_client: "spa".into(),
            owner_subject: "owner".into(),
            allowed_origins: vec!["https://web.test".into()],
            cancel: CancellationToken::new(),
        },
    )
    .await;
    assert!(matches!(result, Err("GATEWAY_BIND_UNAVAILABLE")));
    assert_eq!(factory.closed.load(Ordering::SeqCst), 1);
}
impl GatewayFactory for Factory {
    fn start(&self, _context: LaunchContext) -> StartFuture {
        let entered = self.entered.clone();
        let release = self.release.clone();
        let closed = self.closed.clone();
        Box::pin(async move {
            entered.notify_one();
            let permit = release.acquire().await.unwrap();
            permit.forget();
            Ok(Box::new(Lease {
                endpoints: tokio::sync::watch::channel(
                    crate::gateway_host::GatewayEndpointState::Pending,
                )
                .0,
                address: "127.0.0.1:9".parse().unwrap(),
                closed,
            }) as Box<dyn GatewayLease>)
        })
    }
}
#[tokio::test]
async fn late_actual_factory_start_is_owned_then_closed_without_installing_gate() {
    let factory = Arc::new(Factory {
        entered: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Semaphore::new(0)),
        closed: Arc::new(AtomicUsize::new(0)),
    });
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    let f = factory.clone();
    let task = tokio::spawn(async move {
        GatewayRuntime::create(
            f,
            Workspace::new(),
            LaunchContext {
                generation: 1,
                local_port: 0,
                local_token: String::new(),
                issuer: "https://issuer.test/oidc".into(),
                resource: "https://resource.test".into(),
                allowed_spa_client: "spa".into(),
                owner_subject: "owner".into(),
                allowed_origins: vec!["https://web.test".into()],
                cancel: c,
            },
        )
        .await
    });
    factory.entered.notified().await;
    cancel.cancel();
    assert!(!task.is_finished());
    factory.release.add_permits(1);
    assert!(matches!(task.await.unwrap(), Err("CANCELLED")));
    assert_eq!(factory.closed.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn tcp_gate_seals_active_and_inflight_sockets_without_stopping_target() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut b = [0; 64];
                while let Ok(n) = s.read(&mut b).await {
                    if n == 0 {
                        break;
                    };
                    if s.write_all(&b[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    let mut gate = TcpGate::start(target).await.unwrap();
    let mut socket = TcpStream::connect(gate.address).await.unwrap();
    socket.write_all(b"before").await.unwrap();
    let mut b = [0; 6];
    socket.read_exact(&mut b).await.unwrap();
    assert_eq!(&b, b"before");
    let mut inflight = TcpStream::connect(gate.address).await.unwrap();
    // Hold the gate's own listener while exercising the synchronous admission
    // flag. A released ephemeral port may immediately belong to another test.
    gate.open.store(false, Ordering::SeqCst);
    let mut rejected = TcpStream::connect(gate.address).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), rejected.read(&mut b))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    gate.seal();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), socket.read(&mut b))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), inflight.read(&mut b))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    gate.close().await;
    assert!(gate.task.is_none());
    let mut local = TcpStream::connect(target).await.unwrap();
    local.write_all(b"local!").await.unwrap();
    local.read_exact(&mut b).await.unwrap();
    assert_eq!(&b, b"local!");
    server.abort();
}
#[test]
fn endpoint_sequences_reject_mutated_or_rewound_and_unsafe_revisions() {
    let initial = GatewayEndpointState::Pending;
    let a = GatewayEndpointState::Available(GatewayEndpoint {
        revision: 1,
        provider_id: "quick-tunnel".into(),
        url: "wss://sample.trycloudflare.com/remote/v1/ws".into(),
    });
    assert!(a.follows(&initial));
    assert!(a.follows(&a));
    let mut altered = a.clone();
    if let GatewayEndpointState::Available(e) = &mut altered {
        e.url.push('x');
    }
    assert!(!altered.follows(&a));
    assert!(!initial.follows(&a));
    assert!(GatewayEndpointState::Unavailable { revision: 2 }.follows(&a));
    assert!(!GatewayEndpointState::Closed { revision: 0 }.follows(&initial));
    assert!(!GatewayEndpointState::Closed {
        revision: 9_007_199_254_740_992
    }
    .follows(&a));
}
struct SequenceFactory {
    endpoint: watch::Sender<GatewayEndpointState>,
}
struct SequenceLease {
    endpoint: watch::Sender<GatewayEndpointState>,
}
impl GatewayFactory for SequenceFactory {
    fn start(&self, _: LaunchContext) -> StartFuture {
        let endpoint = self.endpoint.clone();
        Box::pin(async move { Ok(Box::new(SequenceLease { endpoint }) as Box<dyn GatewayLease>) })
    }
}
impl GatewayLease for SequenceLease {
    fn ingress(&self) -> SocketAddr {
        "127.0.0.1:9".parse().unwrap()
    }
    fn identity(&self) -> GatewayIdentity {
        GatewayIdentity {
            installation_id: "sequence".into(),
            public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        }
    }
    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            protocol: "anywork.remote.v1".into(),
            limits: cybergrid_sdk::BridgeLimits {
                max_frame_bytes: 65535,
                max_streams: 2,
                max_pending_requests: 8,
            },
        }
    }
    fn endpoint_updates(&self) -> watch::Receiver<GatewayEndpointState> {
        self.endpoint.subscribe()
    }
    fn bind_guarded_ingress<'a>(
        &'a mut self,
        _: SocketAddr,
        _: CancellationToken,
    ) -> BindFuture<'a> {
        Box::pin(async move {
            self.endpoint.send_replace(sequence(1));
            Ok(())
        })
    }
    fn close(self: Box<Self>) -> CloseFuture {
        Box::pin(async { Ok(()) })
    }
}
fn sequence(revision: u64) -> GatewayEndpointState {
    GatewayEndpointState::Available(GatewayEndpoint {
        revision,
        provider_id: "quick-tunnel".into(),
        url: format!("wss://sequence-{revision}.trycloudflare.com/remote/v1/ws"),
    })
}
#[tokio::test]
async fn same_owned_monitor_retains_observed_revision_after_bind_and_rejects_rollback() {
    let (tx, _) = watch::channel(GatewayEndpointState::Pending);
    let cancel = CancellationToken::new();
    let runtime = GatewayRuntime::create(
        Arc::new(SequenceFactory {
            endpoint: tx.clone(),
        }),
        Workspace::new(),
        LaunchContext {
            generation: 1,
            local_port: 0,
            local_token: String::new(),
            issuer: "https://issuer.test/oidc".into(),
            resource: "https://resource.test".into(),
            allowed_spa_client: "spa".into(),
            owner_subject: "owner".into(),
            allowed_origins: vec!["https://web.test".into()],
            cancel: cancel.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(runtime.endpoint_snapshot().unwrap(), sequence(1));
    tx.send_replace(sequence(2));
    tokio::time::timeout(Duration::from_secs(1), async {
        while runtime.observed.lock().unwrap().revision() != Some(2) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tx.send_replace(sequence(1));
    tokio::time::timeout(Duration::from_secs(1), cancel.cancelled())
        .await
        .unwrap();
    assert!(runtime.endpoint_snapshot().is_err());
    runtime.close().await.unwrap();
}
