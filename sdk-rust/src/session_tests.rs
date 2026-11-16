use super::*;
use tokio::net::TcpStream;
use tokio::time::{timeout, Instant};

async fn ready_pair() -> (Fixture, Arc<Client>, Arc<Client>) {
    let (f, _) = control().await;
    let a = f.client("RealA");
    let b = f.client("RealA");
    a.login_with_access_token("a", "a").await.unwrap();
    a.enable_remote().await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    b.enable_remote().await.unwrap();
    (f, a, b)
}
async fn round_trip(addr: std::net::SocketAddr) {
    let mut socket = TcpStream::connect(addr).await.unwrap();
    let data = vec![37; 65536];
    let (mut r, mut w) = socket.split();
    let (_, received) = tokio::join!(
        async {
            w.write_all(&data).await.unwrap();
            w.shutdown().await.unwrap();
        },
        async {
            let mut out = Vec::new();
            r.read_to_end(&mut out).await.unwrap();
            out
        }
    );
    assert_eq!(received.len(), data.len(), "truncated half-close");
    assert!(received.iter().all(|byte| *byte == 37));
}
fn operation_reason(error: Error) -> CloseReason {
    match error {
        Error::Operation { diagnostic } => {
            assert!(!diagnostic.cleanup_pending);
            diagnostic.reason.unwrap()
        }
        other => panic!("missing structured diagnostic: {other}"),
    }
}

#[tokio::test]
async fn shared_session_two_ports_independent_close_drop_halfclose_and_join() {
    let (_f, a, b) = ready_pair().await;
    let (port1, echo1) = echo().await;
    let (port2, echo2) = echo().await;
    b.expose(Port::tcp("enrollment", port1, "test"))
        .await
        .unwrap();
    b.expose(Port::tcp("sync", port2, "test")).await.unwrap();
    let mut events = a.subscribe();
    let (peer, incoming) = authenticated_duplex(&a, &b).await;
    let first = peer
        .map("enrollment", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let second = peer
        .map("sync", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(first.snapshot().session_id, second.snapshot().session_id);
    assert_ne!(first.snapshot().mapping_id, second.snapshot().mapping_id);
    assert!(first.snapshot().mapping_id.is_some());
    let mut ready = 0;
    while let Ok(e) = events.try_recv() {
        if e.phase == "multiplexer-ready" {
            ready += 1;
        }
    }
    assert_eq!(
        ready, 1,
        "one authenticated connection serves both mappings"
    );
    timeout(Duration::from_secs(3), round_trip(first.addr))
        .await
        .unwrap();
    timeout(Duration::from_secs(3), first.close())
        .await
        .unwrap();
    assert!(TcpStream::connect(first.addr).await.is_err());
    assert!(!peer.is_closed());
    timeout(Duration::from_secs(3), round_trip(second.addr))
        .await
        .unwrap();
    let disposable = peer
        .map("sync", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    drop(disposable);
    tokio::task::yield_now().await;
    assert!(!peer.is_closed());
    let second_addr = second.addr;
    timeout(Duration::from_secs(3), peer.close()).await.unwrap();
    assert!(TcpStream::connect(second_addr).await.is_err());
    assert!(peer.mux.tasks.is_empty());
    assert_eq!(peer.snapshot().reason, Some(CloseReason::LocalClose));
    assert!(!peer.snapshot().cleanup_pending);
    timeout(Duration::from_secs(3), incoming.close())
        .await
        .unwrap();
    assert!(incoming.mux.tasks.is_empty());
    a.close().await.unwrap();
    b.close().await.unwrap();
    echo1.abort();
    echo2.abort();
}

#[tokio::test]
async fn public_connect_options_reuse_actual_iroh_and_cancel_does_not_close_ready_session() {
    let (f, _) = control().await;
    let a = iroh_fixture_client(&f, DirectStrategy::Sequential);
    let b = iroh_fixture_client(&f, DirectStrategy::Sequential);
    a.login_with_access_token("a", "a").await.unwrap();
    a.enable_remote().await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    b.enable_remote().await.unwrap();
    iroh_publish_ready(&a).await;
    iroh_publish_ready(&b).await;
    let (port, echo_task) = echo().await;
    b.expose(Port::tcp("one", port, "test")).await.unwrap();
    b.expose(Port::tcp("two", port, "test")).await.unwrap();
    let cancel = CancellationToken::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let peer = a
        .connect_with_options(
            "id-b",
            ConnectOptions {
                only_transport: Some(Transport::Iroh),
                operation: OperationOptions {
                    deadline: Some(deadline),
                    cancel: Some(cancel.clone()),
                },
                ..ConnectOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(peer.snapshot().transport, Some(Transport::Iroh));
    let one = peer
        .map("one", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let two = peer
        .map("two", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    cancel.cancel();
    timeout(Duration::from_secs(3), round_trip(two.addr))
        .await
        .unwrap();
    assert_eq!(live_peers(&a).await, 1);
    assert_eq!(live_peers(&b).await, 1);
    one.close().await;
    two.close().await;
    peer.close().await;
    a.close().await.unwrap();
    b.close().await.unwrap();
    echo_task.abort();
}

#[tokio::test]
async fn total_deadline_cancel_and_forced_path_do_not_fallback_or_leak() {
    let (f, state) = control().await;
    state.lock().unwrap().drop_signals = true;
    let a = f.client("RealA");
    let b = f.client("RealA");
    a.login_with_access_token("a", "a").await.unwrap();
    a.enable_remote().await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    b.enable_remote().await.unwrap();
    let start = Instant::now();
    let result = a
        .connect_with_options(
            "id-b",
            ConnectOptions {
                operation: OperationOptions {
                    deadline: Some(start + Duration::from_millis(100)),
                    cancel: None,
                },
                direct_timeout: Some(Duration::from_millis(25)),
                fallback_timeout: Some(Duration::from_secs(10)),
                ..ConnectOptions::default()
            },
        )
        .await;
    assert_eq!(
        operation_reason(result.err().unwrap()),
        CloseReason::DeadlineExceeded
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(a.active().await.unwrap().attempts.lock().await.is_empty());
    assert_eq!(live_peers(&a).await, 0);
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let request = a.connect_with_options(
        "id-b",
        ConnectOptions {
            operation: OperationOptions {
                cancel: Some(cancel),
                deadline: None,
            },
            ..ConnectOptions::default()
        },
    );
    let (result, _) = tokio::join!(request, async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        stop.cancel();
    });
    assert_eq!(
        operation_reason(result.err().unwrap()),
        CloseReason::Cancelled
    );
    assert!(a.active().await.unwrap().attempts.lock().await.is_empty());
    let mut events = a.subscribe();
    let result = a
        .connect_with_options(
            "id-b",
            ConnectOptions {
                only_transport: Some(Transport::WebRtc),
                direct_timeout: Some(Duration::from_millis(25)),
                ..ConnectOptions::default()
            },
        )
        .await;
    assert_eq!(
        operation_reason(result.err().unwrap()),
        CloseReason::StageTimeout
    );
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.phase, "fallback-attempt");
        assert_ne!(event.phase, "iroh-attempt");
    }
    a.close().await.unwrap();
    b.close().await.unwrap();
}

#[tokio::test]
async fn port_deadline_cancel_probe_failure_and_stream_diagnostics_are_local() {
    let (_f, a, b) = ready_pair().await;
    let (port, echo_task) = echo().await;
    b.expose(Port::tcp("echo", port, "test")).await.unwrap();
    let (peer, _) = authenticated_duplex(&a, &b).await;
    let error = peer
        .dial_with_options("absent", OpenOptions::default())
        .await
        .err()
        .unwrap();
    assert_eq!(operation_reason(error), CloseReason::PortUnavailable);
    assert!(!peer.is_closed());
    let mut stream = peer
        .dial_with_options("echo", OpenOptions::default())
        .await
        .unwrap();
    let state = stream.snapshot();
    assert!(state.stream_id.is_some());
    assert_eq!(state.session_id, peer.snapshot().session_id);
    stream.write_all(b"hello").await.unwrap();
    let mut buf = [0; 5];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello");
    let mut watch = stream.watch_state();
    stream.close().await;
    watch.changed().await.unwrap();
    assert_eq!(watch.borrow().reason, Some(CloseReason::LocalClose));
    assert!(!watch.borrow().cleanup_pending);
    let expired = OpenOptions {
        operation: OperationOptions {
            deadline: Some(Instant::now()),
            cancel: None,
        },
        ..OpenOptions::default()
    };
    assert_eq!(
        operation_reason(peer.dial_with_options("echo", expired).await.err().unwrap()),
        CloseReason::DeadlineExceeded
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(
        operation_reason(
            peer.dial_with_options(
                "echo",
                OpenOptions {
                    operation: OperationOptions {
                        cancel: Some(cancelled),
                        deadline: None
                    },
                    ..OpenOptions::default()
                }
            )
            .await
            .err()
            .unwrap()
        ),
        CloseReason::Cancelled
    );
    let records = a.diagnostics_snapshot();
    let json = serde_json::to_string(&records).unwrap();
    for forbidden in [
        "fixture-user",
        "id-b",
        "key-a",
        "127.0.0.1",
        "hello",
        "absent",
        "echo",
    ] {
        assert!(
            !json.contains(forbidden),
            "identity/content leaked: {forbidden}"
        );
    }
    assert!(!peer.is_closed());
    peer.close().await;
    a.close().await.unwrap();
    b.close().await.unwrap();
    echo_task.abort();
}

#[tokio::test]
async fn stalled_port_open_honors_phase_budget_and_cleans_stream_only() {
    let (_f, a, b) = ready_pair().await;
    let rt = a.active().await.unwrap();
    let (left, right) = tokio::io::duplex(65536);
    let mux = Mux::start(Box::new(left), true, rt.cancel.child_token());
    let remote = Mux::start(Box::new(right), false, CancellationToken::new());
    let peer = a
        .remember_peer(
            "id-b".into(),
            "iroh-direct".into(),
            mux,
            rt.generation,
            None,
        )
        .await
        .unwrap();
    let error = peer
        .dial_with_options(
            "stalled",
            OpenOptions {
                port_open_timeout: Duration::from_millis(25),
                ..OpenOptions::default()
            },
        )
        .await
        .err()
        .unwrap();
    assert_eq!(operation_reason(error), CloseReason::StageTimeout);
    assert_eq!(peer.mux.stream_count().await, 0);
    assert!(!peer.is_closed());
    peer.close().await;
    remote.stop(CloseReason::LocalClose);
    remote.wait().await;
    a.close().await.unwrap();
    b.close().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn keepalive_timeout_and_remote_eof_have_distinct_stable_reasons() {
    let store = crate::diagnostics::Diagnostics::new();
    let (left, right) = tokio::io::duplex(65536);
    let mux = Mux::start(Box::new(left), true, CancellationToken::new());
    let diagnostic = store.start(1, DiagnosticStage::Multiplexer);
    mux.set_diagnostic(diagnostic.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(31)).await;
    mux.cancel.cancelled().await;
    mux.wait().await;
    assert_eq!(
        diagnostic.snapshot().reason,
        Some(CloseReason::KeepaliveTimeout)
    );
    mux.stop(CloseReason::LocalClose);
    assert_eq!(
        diagnostic.snapshot().reason,
        Some(CloseReason::KeepaliveTimeout)
    );
    drop(right);
    let (left, right) = tokio::io::duplex(65536);
    let mux = Mux::start(Box::new(left), true, CancellationToken::new());
    let diagnostic = store.start(1, DiagnosticStage::Multiplexer);
    mux.set_diagnostic(diagnostic.clone());
    drop(right);
    mux.cancel.cancelled().await;
    mux.wait().await;
    assert_eq!(diagnostic.snapshot().reason, Some(CloseReason::RemoteEof));
}

#[tokio::test]
async fn receive_backpressure_closes_only_slow_stream_and_preserves_other_port() {
    let (left, right) = tokio::io::duplex(65536);
    let a = Mux::start(Box::new(left), true, CancellationToken::new());
    let b = Mux::start(Box::new(right), false, CancellationToken::new());
    let store = crate::diagnostics::Diagnostics::new();
    b.set_diagnostic(store.start(1, DiagnosticStage::Multiplexer));
    let mut slow_writer = a.open().await.unwrap();
    let slow_reader = b.accept_tracked().await.unwrap();
    let owner = slow_reader.owner.clone();
    let slow_diagnostic = owner.diagnostic.clone().unwrap();
    let flooding = tokio::spawn(async move {
        let _ = slow_writer.write_all(&vec![17; 2 << 20]).await;
    });
    timeout(Duration::from_secs(3), owner.cancel.cancelled())
        .await
        .unwrap();
    assert_eq!(
        slow_diagnostic.snapshot().reason,
        Some(CloseReason::ReceiveBackpressure)
    );
    assert!(!b.cancel.is_cancelled());
    let mut healthy_writer = a.open().await.unwrap();
    let mut healthy_reader = b.accept().await.unwrap();
    healthy_writer.write_all(b"healthy").await.unwrap();
    let mut data = [0; 7];
    timeout(Duration::from_secs(2), healthy_reader.read_exact(&mut data))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&data, b"healthy");
    flooding.await.unwrap();
    a.stop(CloseReason::LocalClose);
    b.stop(CloseReason::LocalClose);
    a.wait().await;
    b.wait().await;
    assert!(a.tasks.is_empty());
    assert!(b.tasks.is_empty());
}

#[tokio::test]
async fn shared_session_remote_disable_closes_listeners_and_reports_local_cause() {
    let (_f, a, b) = ready_pair().await;
    let (port, echo_task) = echo().await;
    b.expose(Port::tcp("echo", port, "test")).await.unwrap();
    let (peer, _) = authenticated_duplex(&a, &b).await;
    let mapping = peer
        .map("echo", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let mut stream = peer
        .dial_with_options("echo", OpenOptions::default())
        .await
        .unwrap();
    let outcome = timeout(Duration::from_secs(3), a.disable_remote())
        .await
        .unwrap();
    assert!(outcome.local_closed);
    assert_eq!(peer.snapshot().reason, Some(CloseReason::RemoteDisabled));
    assert!(!peer.snapshot().cleanup_pending);
    assert!(peer.mux.tasks.is_empty());
    assert!(TcpStream::connect(mapping.addr).await.is_err());
    let mut bytes = [0; 1];
    assert_eq!(stream.read(&mut bytes).await.unwrap(), 0);
    assert!(peer.dial("echo").await.is_err());
    b.close().await.unwrap();
    echo_task.abort();
}

#[tokio::test]
async fn shared_session_epoch_loss_does_not_reconnect_listener_or_replay() {
    let (f, state) = control().await;
    let a = f.client("RealA");
    let b = f.client("RealA");
    a.login_with_access_token("a", "a").await.unwrap();
    a.enable_remote().await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    b.enable_remote().await.unwrap();
    let (port, echo_task) = echo().await;
    b.expose(Port::tcp("echo", port, "test")).await.unwrap();
    let (peer, _) = authenticated_duplex(&a, &b).await;
    let mapping = peer
        .map("echo", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    state.lock().unwrap().epoch = "new-coordination-epoch".into();
    assert!(a.devices().await.is_err());
    timeout(Duration::from_secs(3), peer.close()).await.unwrap();
    assert_eq!(peer.snapshot().reason, Some(CloseReason::CoordinationLost));
    assert!(TcpStream::connect(mapping.addr).await.is_err());
    assert!(peer.dial("echo").await.is_err());
    assert!(peer.mux.tasks.is_empty());
    let _ = a.close().await;
    let _ = b.close().await;
    echo_task.abort();
}

#[tokio::test]
async fn per_call_parallel_override_beats_blocked_rtc_without_mutating_config() {
    let (f, state) = control().await;
    state.lock().unwrap().drop_signals = true;
    let a = iroh_fixture_client(&f, DirectStrategy::Sequential);
    let b = iroh_fixture_client(&f, DirectStrategy::Sequential);
    a.login_with_access_token("a", "a").await.unwrap();
    a.enable_remote().await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    b.enable_remote().await.unwrap();
    iroh_publish_ready(&a).await;
    iroh_publish_ready(&b).await;
    let peer = a
        .connect_with_options(
            "id-b",
            ConnectOptions {
                direct_strategy: Some(DirectStrategy::Parallel),
                direct_timeout: Some(Duration::from_secs(3)),
                operation: OperationOptions {
                    deadline: Some(Instant::now() + Duration::from_secs(2)),
                    cancel: None,
                },
                ..ConnectOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(peer.transport, "iroh-direct");
    assert_eq!(a.config().direct_strategy, DirectStrategy::Sequential);
    assert_eq!(a.config().direct_timeout, Duration::from_nanos(1));
    assert!(
        a.active().await.unwrap().attempts.lock().await.is_empty(),
        "losing RTC attempt must be joined"
    );
    peer.close().await;
    a.close().await.unwrap();
    b.close().await.unwrap();
}

#[tokio::test]
async fn diagnostic_lag_has_bounded_snapshot_and_handle_preserves_terminal_state() {
    let store = crate::diagnostics::Diagnostics::new();
    let mut events = store.subscribe();
    let retained = store.start(1, DiagnosticStage::PortOpen);
    let receiver = retained.watch();
    retained.stop(CloseReason::StageTimeout);
    retained.complete();
    for _ in 0..300 {
        let d = store.start(1, DiagnosticStage::Connect);
        d.ready();
        d.complete();
    }
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
    assert_eq!(store.snapshot().len(), 256);
    assert_eq!(receiver.borrow().reason, Some(CloseReason::StageTimeout));
    assert!(!receiver.borrow().cleanup_pending);
    retained.stop(CloseReason::LocalClose);
    assert_eq!(retained.snapshot().reason, Some(CloseReason::StageTimeout));
}
