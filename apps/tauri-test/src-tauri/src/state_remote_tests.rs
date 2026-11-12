//! Contract tests use the actual Lab host, SDK, HTTP coordination and mTLS data path.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex as StdMutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

#[derive(Default)]
struct Control {
    sessions: HashMap<String, String>,
    attempts: HashMap<String, Value>,
    devices: HashMap<String, Value>,
    signals: Vec<Value>,
    n: u64,
    delete_fails: bool,
    expire_next_directory: bool,
}
#[derive(Default)]
struct Pause {
    armed: AtomicBool,
    entered: Notify,
    release: Notify,
}
struct Coordinator {
    base: String,
    state: Arc<StdMutex<Control>>,
    stop: CancellationToken,
    publication: Arc<Pause>,
    revocation: Arc<Pause>,
    directory: Arc<Pause>,
}
impl Coordinator {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(StdMutex::new(Control::default()));
        let control = state.clone();
        let publication = Arc::new(Pause::default());
        let pause = publication.clone();
        let revocation = Arc::new(Pause::default());
        let revoke_pause = revocation.clone();
        let directory = Arc::new(Pause::default());
        let directory_pause = directory.clone();
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    accepted = listener.accept() => {
                        let Ok((mut stream,_)) = accepted else {break}; let state=control.clone();let pause=pause.clone();let revoke_pause=revoke_pause.clone();let directory_pause=directory_pause.clone();
                        tokio::spawn(async move {
                            let mut head=Vec::new(); let mut byte=[0];
                            while !head.ends_with(b"\r\n\r\n") { if head.len()>16384 || stream.read_exact(&mut byte).await.is_err(){return} head.push(byte[0]); }
                            let header=String::from_utf8_lossy(&head); let mut parts=header.lines().next().unwrap().split_whitespace();
                            let method=parts.next().unwrap(); let path=parts.next().unwrap();
                            let length=header.lines().find_map(|line|line.to_ascii_lowercase().strip_prefix("content-length:").and_then(|s|s.trim().parse::<usize>().ok())).unwrap_or(0);
                            if length>65536{return} let mut body=vec![0;length]; if stream.read_exact(&mut body).await.is_err(){return}
                            let auth=header.lines().find_map(|line|line.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("authorization")).map(|(_,v)|v.trim())).unwrap_or("");
                            let (status,value)=reply(&mut state.lock().unwrap(),method,path,&body,auth);
                            if method=="PUT" && pause.armed.swap(false,Ordering::SeqCst){pause.entered.notify_one();pause.release.notified().await;}
                            if method=="DELETE" && revoke_pause.armed.swap(false,Ordering::SeqCst){revoke_pause.entered.notify_one();revoke_pause.release.notified().await;}
                            if method=="GET" && path=="/api/v1/devices" && directory_pause.armed.swap(false,Ordering::SeqCst){directory_pause.entered.notify_one();directory_pause.release.notified().await;}
                            let body=serde_json::to_vec(&value).unwrap();
                            let head=format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nX-CyberGrid-Epoch: lab-epoch\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                            let _=stream.write_all(head.as_bytes()).await; let _=stream.write_all(&body).await;
                        });
                    }
                }
            }
        });
        Self {
            base,
            state,
            stop,
            publication,
            revocation,
            directory,
        }
    }
    fn client(base: &str) -> Result<Arc<Client>, String> {
        Client::new(Config {
            base_url: base.into(),
            app_id: "RealA".into(),
            stun_servers: vec![],
            iroh_enabled: false,
            direct_timeout: Duration::from_secs(12),
            fallback_timeout: Duration::from_secs(1),
            poll_interval: Duration::from_millis(20),
            ..Config::default()
        })
        .map_err(|e| e.to_string())
    }
    async fn host(&self, name: &str, owner: Arc<dyn AuthProvider>) -> Arc<LabState> {
        let directory = std::env::temp_dir().join(format!(
            "cybergrid-lab-remote-{}",
            hex::encode(rand::random::<[u8; 16]>())
        ));
        let mut state = LabState::new(directory.join("data"), directory.join("cache")).unwrap();
        let base = self.base.clone();
        Arc::get_mut(&mut state).unwrap().client_factory =
            Some(Arc::new(move |_| Self::client(&base)));
        let config = LabConfig {
            base_url: self.base.clone(),
            app_id: "RealA".into(),
            device_name: name.into(),
            ..LabConfig::default()
        };
        {
            let mut inner = state.inner.lock().await;
            inner.generation = 1;
            inner.config = config.clone();
        }
        let client = Self::client(&self.base).unwrap();
        client.set_auth_provider(owner.clone()).await;
        state
            .install_session(
                &config,
                &client,
                owner,
                &CancellationToken::new(),
                1,
                "resource-token",
            )
            .await
            .unwrap();
        state
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
fn reply(s: &mut Control, method: &str, path: &str, body: &[u8], auth: &str) -> (u16, Value) {
    if path.starts_with("/api/v1/auth/config?") {
        return (
            200,
            json!({"coordination":{"protocol":"memory-v1","epoch":"lab-epoch","heartbeatSeconds":20,"leaseSeconds":65,"signalTtlSeconds":120,"maxSignalWaitSeconds":15},"provider":"logto","issuer":"https://login.example.test/oidc","clientId":"RealA","resource":"https://api.example.test","scope":"openid offline_access connect","deviceAuthorizationEndpoint":"https://login.example.test/oidc/device/auth","tokenEndpoint":"https://login.example.test/oidc/token","revocationEndpoint":"https://login.example.test/oidc/token/revocation"}),
        );
    }
    let who = s.sessions.get(auth).cloned().unwrap_or_default();
    if method == "POST" && path == "/api/v1/auth/exchange" {
        let v: Value = serde_json::from_slice(body).unwrap();
        let request = v["requestId"].as_str().unwrap();
        if let Some(old) = s.attempts.get(request) {
            return (200, old.clone());
        }
        let index = s.attempts.len() + 1;
        let id = format!("device-{index}");
        let key = format!("key-{index}");
        let incarnation = format!("00000000-0000-4000-8000-{index:012x}");
        s.sessions.insert(format!("Bearer {key}"), id.clone());
        s.devices.insert(id.clone(),json!({"id":id,"name":v["deviceName"],"online":true,"lastSeen":chrono::Utc::now(),"ports":[],"tunnelUrl":null,"certificateFingerprint":null,"incarnation":incarnation,"publishRevision":0}));
        let out = json!({"coordinatorEpoch":"lab-epoch","incarnation":incarnation,"purpose":"device","appId":"RealA","sessionKey":key,"expiresAt":chrono::Utc::now()+chrono::Duration::hours(1),"deviceId":id,"user":{"id":"same-user"}});
        s.attempts.insert(request.into(), out.clone());
        return (200, out);
    }
    if method == "PUT" && path.starts_with("/api/v1/devices/self") {
        let v: Value = serde_json::from_slice(body).unwrap();
        let Some(d) = s.devices.get_mut(&who) else {
            return (401, json!({"error":{"code":"INVALID_SESSION"}}));
        };
        for field in [
            "ports",
            "tunnelUrl",
            "certificateFingerprint",
            "iroh",
            "publishRevision",
        ] {
            d[field] = v.get(field).cloned().unwrap_or(Value::Null);
        }
        return (200, json!({"coordinatorEpoch":"lab-epoch","device":d}));
    }
    if method == "GET" && path == "/api/v1/devices" {
        if std::mem::take(&mut s.expire_next_directory) {
            return (401, json!({"error":{"code":"SESSION_EXPIRED"}}));
        }
        return (
            200,
            json!({"coordinatorEpoch":"lab-epoch","devices":s.devices.values().collect::<Vec<_>>()}),
        );
    }
    if method == "POST" && path == "/api/v1/signals" {
        let mut v: Value = serde_json::from_slice(body).unwrap();
        s.n += 1;
        v["id"] = json!(s.n);
        v["fromDeviceId"] = json!(who);
        v["fromIncarnation"] = s.devices[&who]["incarnation"].clone();
        v["coordinatorEpoch"] = json!("lab-epoch");
        s.signals.push(v);
        return (200, json!({"coordinatorEpoch":"lab-epoch","id":s.n}));
    }
    if method == "GET" && path.starts_with("/api/v1/signals?") {
        let after = path
            .split("after=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        return (
            200,
            json!({"coordinatorEpoch":"lab-epoch","signals":s.signals.iter().filter(|v|v["id"].as_u64().unwrap()>after&&v["toDeviceId"]==who).collect::<Vec<_>>(),"cursor":s.n}),
        );
    }
    if method == "DELETE" && path == "/api/v1/session" {
        return if s.delete_fails {
            (503, json!({"error":{"code":"UNAVAILABLE"}}))
        } else {
            if let Some(d) = s.devices.get_mut(&who) {
                d["ports"] = json!([]);
                d["online"] = json!(false);
            }
            (200, json!({"ok":true}))
        };
    }
    (404, json!({"error":{"code":"NOT_FOUND"}}))
}
#[derive(Default)]
struct Owner {
    block: AtomicBool,
    calls: AtomicUsize,
    entered: Notify,
    release: Notify,
}
impl AuthProvider for Owner {
    fn access_token<'a>(&'a self, _: &'a Client) -> cybergrid_sdk::AuthFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            if self.block.load(Ordering::SeqCst) {
                self.release.notified().await;
            }
            Ok("resource-token".into())
        })
    }
}
async fn echo_roundtrip(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.unwrap();
    let mut got = vec![0; bytes.len()];
    stream.read_exact(&mut got).await.unwrap();
    assert_eq!(got, bytes);
}
async fn cleanup(state: &Arc<LabState>) {
    let root = state.data_dir.parent().unwrap().to_path_buf();
    state.shutdown(false).await.unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_real_mtls_default_off_bidirectional_disable_offline_reopen_keeps_local_services() {
    let coordinator = Coordinator::start().await;
    let owner = Arc::new(Owner::default());
    let a = coordinator.host("a", owner.clone()).await;
    let b = coordinator.host("b", Arc::new(Owner::default())).await;
    let first = a.runtime().await.unwrap();
    let second = b.runtime().await.unwrap();
    assert!(coordinator
        .state
        .lock()
        .unwrap()
        .devices
        .values()
        .all(|d| d["ports"].as_array().unwrap().is_empty()));
    assert!(a.connect_device(second.device_id.clone()).await.is_err());
    assert!(a.send_message("off".into()).await.is_err());
    assert!(a.probe_device(second.device_id.clone()).await.is_err());
    let mut local = TcpStream::connect(("127.0.0.1", first.services.echo_port))
        .await
        .unwrap();
    echo_roundtrip(&mut local, b"before close").await;
    eprintln!("flow: enable both");
    a.enable_remote().await.unwrap();
    b.enable_remote().await.unwrap();
    a.refresh_devices().await.unwrap();
    b.refresh_devices().await.unwrap();
    eprintln!("flow: host connect/send/probe");
    a.connect_device(second.device_id.clone()).await.unwrap();
    a.send_message("real message before close".into())
        .await
        .unwrap();
    b.probe_device(first.device_id.clone()).await.unwrap();
    eprintln!("flow: connect streams");
    let pa = first.client.connect(&second.device_id).await.unwrap();
    let pb = second.client.connect(&first.device_id).await.unwrap();
    let mut ab = pa.dial(protocol::ECHO_PORT).await.unwrap();
    let mut ba = pb.dial(protocol::ECHO_PORT).await.unwrap();
    eprintln!("flow: stream roundtrips");
    ab.write_all(b"ab").await.unwrap();
    let mut bytes = [0; 2];
    tokio::time::timeout(Duration::from_secs(5), ab.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    ba.write_all(b"ba").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), ba.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    coordinator.state.lock().unwrap().delete_fails = true;
    coordinator.revocation.armed.store(true, Ordering::SeqCst);
    eprintln!("flow: offline close");
    let closing = {
        let a = a.clone();
        tokio::spawn(async move { a.disable_remote().await.unwrap() })
    };
    tokio::time::timeout(
        Duration::from_secs(8),
        coordinator.revocation.entered.notified(),
    )
    .await
    .expect("host must drive SDK cleanup before joining its mapping");
    assert!(!closing.is_finished());
    assert!(a.view().await.local_closed);
    assert_eq!(a.view().await.remote_action, "disabling");
    assert!(!first.client.remote_access_enabled());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), ab.read(&mut bytes))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), ba.read(&mut bytes))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    echo_roundtrip(&mut local, b"during offline remote close").await;
    protocol::send_message(
        ([127, 0, 0, 1], first.services.message_port).into(),
        "local",
        &first.device_id,
        "local inbox continues while remote disabled",
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if a.view().await.records.iter().any(|record| {
                record.message.as_deref() == Some("local inbox continues while remote disabled")
            }) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Both remote streams are already EOF and local inbox/echo remain usable
    // while the coordinator still withholds its revocation response.
    assert!(!closing.is_finished());
    coordinator.revocation.release.notify_one();
    let outcome = tokio::time::timeout(Duration::from_secs(3), closing)
        .await
        .unwrap()
        .unwrap();
    assert!(outcome.local_closed);
    assert_eq!(
        outcome.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    assert_eq!(
        a.disable_remote().await.unwrap().coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    assert!(a.connect_device(second.device_id.clone()).await.is_err());
    assert!(!a.view().await.directory_known);
    assert!(Arc::ptr_eq(
        &a.runtime().await.unwrap().services,
        &first.services
    ));
    coordinator.state.lock().unwrap().delete_fails = false;
    eprintln!("flow: reopen");
    a.enable_remote().await.unwrap();
    let reopened = a.runtime().await.unwrap();
    assert!(!Arc::ptr_eq(&reopened.client, &first.client));
    assert!(Arc::ptr_eq(&reopened.owner, &first.owner));
    assert!(Arc::ptr_eq(&reopened.services, &first.services));
    assert_ne!(reopened.device_id, first.device_id);
    assert_eq!(reopened.services.message_port, first.services.message_port);
    echo_roundtrip(&mut local, b"after reopen same local socket").await;
    let mut late = first.client.snapshot();
    late.remote_enabled = true;
    late.revision = u64::MAX;
    late.coordination.phase = StatePhase::Ready;
    late.publication.phase = StatePhase::Ready;
    a.accept_snapshot(&first, late).await;
    assert_ne!(a.view().await.sdk_snapshot.unwrap().revision, u64::MAX);
    assert_eq!(a.view().await.local.unwrap().device_id, reopened.device_id);
    assert_eq!(
        a.view().await.sdk_snapshot.unwrap().generation,
        reopened.client.snapshot().generation
    );
    protocol::send_message(
        ([127, 0, 0, 1], first.services.message_port).into(),
        "local",
        &reopened.device_id,
        "new ACK identity",
    )
    .await
    .unwrap();
    eprintln!("flow: reopened business");
    a.refresh_devices().await.unwrap();
    b.refresh_devices().await.unwrap();
    a.connect_device(second.device_id.clone()).await.unwrap();
    a.send_message("after reopen".into()).await.unwrap();
    b.probe_device(reopened.device_id.clone()).await.unwrap();
    eprintln!("flow: final close/cleanup");
    a.disable_remote().await.unwrap();
    b.disable_remote().await.unwrap();
    assert!(!first.client.snapshot().remote_enabled);
    cleanup(&a).await;
    cleanup(&b).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_wins_during_owned_provider_and_new_enable_waits_owner_checkpoint() {
    let coordinator = Coordinator::start().await;
    let owner = Arc::new(Owner::default());
    let a = coordinator.host("a", owner.clone()).await;
    a.enable_remote().await.unwrap();
    a.disable_remote().await.unwrap();
    owner.block.store(true, Ordering::SeqCst);
    a.start_enable_remote().await.unwrap();
    owner.entered.notified().await;
    let opening = a.inner.lock().await.opening.clone().unwrap();
    let closing = {
        let a = a.clone();
        tokio::spawn(async move { a.disable_remote().await.unwrap() })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while !a.view().await.local_closed
            || !opening
                .snapshot()
                .publication
                .phase
                .eq(&StatePhase::Disabled)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(a.enable_remote().await.is_ok() || a.view().await.remote_action == "disabling");
    assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
    assert!(!opening.remote_access_enabled());
    let services = a.runtime().await.unwrap().services.clone();
    let mut local = TcpStream::connect(("127.0.0.1", services.echo_port))
        .await
        .unwrap();
    echo_roundtrip(&mut local, b"pending owner still local").await;
    owner.block.store(false, Ordering::SeqCst);
    owner.release.notify_one();
    closing.await.unwrap();
    // The previous owned enable holds single-flight until its provider/checkpoint settles.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(lock) = a.remote_operation.try_lock() {
                drop(lock);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    a.enable_remote().await.unwrap();
    assert_eq!(owner.calls.load(Ordering::SeqCst), 2);
    assert!(a.runtime().await.unwrap().client.remote_access_enabled());
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(a.view().await.remote_action, "on");
    echo_roundtrip(&mut local, b"new generation local").await;
    cleanup(&a).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_during_publication_response_keeps_local_gate_closed_and_fences_late_success() {
    let coordinator = Coordinator::start().await;
    let a = coordinator.host("a", Arc::new(Owner::default())).await;
    let original = a.runtime().await.unwrap();
    coordinator.publication.armed.store(true, Ordering::SeqCst);
    a.start_enable_remote().await.unwrap();
    coordinator.publication.entered.notified().await;
    assert!(original.client.remote_access_enabled());
    let closing = {
        let a = a.clone();
        tokio::spawn(async move { a.disable_remote().await.unwrap() })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let view = a.view().await;
            if view.local_closed && view.remote_action == "disabling" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!closing.is_finished());
    assert!(!original.client.remote_access_enabled());
    assert!(a.probe_device("other".into()).await.is_err());
    let mut local = TcpStream::connect(("127.0.0.1", original.services.echo_port))
        .await
        .unwrap();
    echo_roundtrip(&mut local, b"cloud response pending local continues").await;
    assert!(a.connect_device("other".into()).await.is_err());
    coordinator.publication.release.notify_one();
    let outcome = tokio::time::timeout(Duration::from_secs(3), closing)
        .await
        .unwrap()
        .unwrap();
    assert!(outcome.local_closed);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(lock) = a.remote_operation.try_lock() {
                drop(lock);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(a.view().await.remote_action, "off");
    assert!(a.view().await.local_closed);
    assert!(!a.view().await.directory_known);
    a.enable_remote().await.unwrap();
    let current = a.runtime().await.unwrap();
    assert!(!Arc::ptr_eq(&current.client, &original.client));
    assert!(Arc::ptr_eq(&current.services, &original.services));
    echo_roundtrip(&mut local, b"late publication cannot restart local").await;
    assert!(a.view().await.sdk_snapshot.unwrap().remote_enabled);
    cleanup(&a).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parsed_adb_remote_controls_dispatch_to_real_host_and_safe_snapshot() {
    let coordinator = Coordinator::start().await;
    let state = coordinator.host("adb", Arc::new(Owner::default())).await;
    for (sequence, action) in [
        (1, "enable-remote"),
        (2, "disable-remote"),
        (3, "disable-remote"),
    ] {
        let request = serde_json::to_vec(&json!({"version":1,"controlEpoch":"a".repeat(32),"sequence":sequence,"requestId":format!("00000000-0000-4000-8000-{sequence:012x}"),"action":action,"payload":{}})).unwrap();
        let view = crate::adb_control::dispatch_public_test(&state, &request)
            .await
            .unwrap();
        assert!(view.get("localClosed").is_some());
        if action == "enable-remote" {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if state.view().await.remote_action == "on" {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(coordinator
                .state
                .lock()
                .unwrap()
                .devices
                .values()
                .any(|d| d["ports"].as_array().unwrap().len() == 2));
        } else {
            assert_eq!(view["localClosed"], true);
            assert_eq!(view["coordinatorRevocation"], "confirmed");
            assert_eq!(view["remoteAction"], "off");
            assert!(coordinator
                .state
                .lock()
                .unwrap()
                .devices
                .values()
                .all(|d| d["ports"].as_array().unwrap().is_empty()));
        }
        let encoded = serde_json::to_string(&view).unwrap();
        for secret in [
            "resource-token",
            "key-1",
            "registrationKey",
            "accessToken",
            "refreshToken",
        ] {
            assert!(!encoded.contains(secret));
        }
    }
    let service = state.runtime().await.unwrap().services.clone();
    protocol::probe(([127, 0, 0, 1], service.echo_port).into())
        .await
        .unwrap();
    cleanup(&state).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_joins_old_clients_background_provider_before_reopen_uses_grant() {
    let coordinator = Coordinator::start().await;
    let owner = Arc::new(Owner::default());
    let state = coordinator.host("a", owner.clone()).await;
    state.enable_remote().await.unwrap();
    let old = state.runtime().await.unwrap();
    owner.block.store(true, Ordering::SeqCst);
    coordinator.state.lock().unwrap().expire_next_directory = true;
    assert!(state.refresh_devices().await.is_err());
    tokio::time::timeout(Duration::from_secs(2), owner.entered.notified())
        .await
        .unwrap();
    assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
    let closing = {
        let state = state.clone();
        tokio::spawn(async move { state.disable_remote().await.unwrap() })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.view().await.remote_action != "disabling" || !state.view().await.local_closed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!closing.is_finished());
    assert!(state.start_enable_remote().await.is_err());
    assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
    let mut local = TcpStream::connect(("127.0.0.1", old.services.echo_port))
        .await
        .unwrap();
    echo_roundtrip(&mut local, b"old background grant finishing").await;
    owner.block.store(false, Ordering::SeqCst);
    owner.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), closing)
        .await
        .unwrap()
        .unwrap();
    state.enable_remote().await.unwrap();
    assert_eq!(owner.calls.load(Ordering::SeqCst), 2);
    let new = state.runtime().await.unwrap();
    assert!(Arc::ptr_eq(&new.owner, &old.owner));
    assert!(!Arc::ptr_eq(&new.client, &old.client));
    echo_roundtrip(&mut local, b"new client same owner local service").await;
    assert_eq!(state.view().await.remote_action, "on");
    cleanup(&state).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reserved_enable_worker_cannot_cross_a_completed_close() {
    let coordinator = Coordinator::start().await;
    let owner = Arc::new(Owner::default());
    let state = coordinator.host("a", owner.clone()).await;
    // Model an IPC-owned worker reserved before close but scheduled after it.
    let reserved = state.remote_enable_generation().await.unwrap();
    state.disable_remote().await.unwrap();
    assert!(state.enable_remote_generation(reserved).await.is_err());
    assert_eq!(owner.calls.load(Ordering::SeqCst), 0);
    assert!(state.view().await.local_closed);
    assert_eq!(state.view().await.remote_action, "off");
    assert!(coordinator
        .state
        .lock()
        .unwrap()
        .devices
        .values()
        .all(|d| d["ports"].as_array().unwrap().is_empty()));
    state.start_enable_remote().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.view().await.remote_action != "on" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
    cleanup(&state).await;
}

// Run the shipped CLI logic against real private files and its exact constant
// no-overwrite shell publisher, substituting only the ADB transport. No phone.
const LOCAL_CLI: &str = r#"
import importlib.util,json,pathlib,subprocess,sys,os
os.umask(0o077)
spec=importlib.util.spec_from_file_location('control',sys.argv[1]);control=importlib.util.module_from_spec(spec);spec.loader.exec_module(control)
root=pathlib.Path(sys.argv[2]);capture=root/sys.argv[4]
class Local:
 def live_pid(self): return json.loads((root/'files/adb-control/status.json').read_text())['pid']
 def read(self,name):
  path=root/'files/adb-control'/name
  return json.loads(path.read_text()) if path.exists() else None
 def publish(self,request):
  body=json.dumps(request,separators=(',',':'),ensure_ascii=False).encode()
  result=subprocess.run(['sh','-c',control.PUBLISH],cwd=root,input=body,capture_output=True)
  if result.returncode: raise control.Failure('ADB_ERROR','fixed publisher refused occupied mailbox')
  capture.write_bytes(body)
if sys.argv[3]=='raw':
 result=subprocess.run(['sh','-c',control.PUBLISH],cwd=root,input=sys.stdin.buffer.read(),capture_output=True)
 print(json.dumps({'published':result.returncode==0}))
else:
 try: print(json.dumps(control.main(['--serial','fixture','--timeout','8',sys.argv[3]],lambda _:Local())))
 except control.Failure as error: print(json.dumps({'ok':False,'code':error.code}))
"#;
fn cli_process(directory: &Path, action: &str, capture: &str, raw: Option<&[u8]>) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/android-adb.py");
    let mut child = Command::new("python3")
        .args(["-c", LOCAL_CLI])
        .arg(script)
        .arg(directory)
        .arg(action)
        .arg(capture)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = raw {
        child.stdin.as_mut().unwrap().write_all(bytes).unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
async fn control_file_until(root: &Path, file: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(bytes) = std::fs::read(root.join(file)) {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    if predicate(&value) {
                        return value;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
async fn mailbox_free(root: &Path) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while root.join("request.json").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_adb_files_live_close_and_priority_disable_survive_slow_refresh_and_pending_duplicate()
{
    use std::os::unix::fs::PermissionsExt;
    let c = Coordinator::start().await;
    let a = c.host("adb-priority", Arc::new(Owner::default())).await;
    a.enable_remote().await.unwrap();
    let directory = a.data_dir.parent().unwrap().to_path_buf();
    let root = directory.join("files/adb-control");
    let worker = {
        let root = root.clone();
        let a = a.clone();
        tokio::spawn(crate::adb_control::run_test(root, a))
    };
    control_file_until(&root, "status.json", |v| v["highwater"] == 0).await;
    c.directory.armed.store(true, Ordering::SeqCst);
    let refresh = {
        let directory = directory.clone();
        tokio::task::spawn_blocking(move || {
            cli_process(&directory, "refresh", "refresh.request", None)
        })
    };
    tokio::time::timeout(Duration::from_secs(2), c.directory.entered.notified())
        .await
        .unwrap();
    mailbox_free(&root).await;
    eprintln!("adb HTTP GET held: durable sequence 1, mailbox released, remote gate open");
    let original = std::fs::read(directory.join("refresh.request")).unwrap();
    assert_eq!(
        cli_process(&directory, "raw", "unused", Some(&original))["published"],
        true
    );
    mailbox_free(&root).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        !refresh.is_finished(),
        "same owned pending replay must await original result, not UNKNOWN"
    );
    let status: Value =
        serde_json::from_slice(&std::fs::read(root.join("status.json")).unwrap()).unwrap();
    assert_eq!(status["highwater"], 1);
    assert!(status["completed"].as_array().unwrap().is_empty());
    eprintln!(
        "adb pending refresh replay: original CLI waiting, no terminal outcome, mailbox free"
    );
    c.state.lock().unwrap().delete_fails = true;
    c.revocation.armed.store(true, Ordering::SeqCst);
    let disable = {
        let directory = directory.clone();
        tokio::task::spawn_blocking(move || {
            cli_process(&directory, "disable-remote", "disable.request", None)
        })
    };
    tokio::time::timeout(Duration::from_secs(2), c.revocation.entered.notified())
        .await
        .unwrap();
    let status = tokio::time::timeout(
        Duration::from_secs(1),
        control_file_until(&root, "status.json", |v| {
            v["state"]["localClosed"] == true && v["state"]["remoteAction"] == "disabling"
        }),
    )
    .await
    .unwrap();
    assert_eq!(status["state"]["coordinatorRevocation"], "pending");
    eprintln!(
        "adb HTTP DELETE held: status localClosed={}, remoteAction={}, revocation={}, highwater={}",
        status["state"]["localClosed"],
        status["state"]["remoteAction"],
        status["state"]["coordinatorRevocation"],
        status["highwater"]
    );
    assert!(!disable.is_finished());
    assert!(!a.runtime().await.unwrap().client.remote_access_enabled());
    let close_generation = a.inner.lock().await.remote_generation;
    let original_disable = std::fs::read(directory.join("disable.request")).unwrap();
    mailbox_free(&root).await;
    assert_eq!(
        cli_process(&directory, "raw", "unused", Some(&original_disable))["published"],
        true
    );
    mailbox_free(&root).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!disable.is_finished());
    assert_eq!(a.inner.lock().await.remote_generation, close_generation);
    // A new ordinary command cannot use the occupied operation slot. It is
    // durably claimed as BUSY, consuming sequence 3 without another dispatch.
    assert_eq!(
        cli_process(&directory, "refresh", "busy.request", None)["code"],
        "BUSY"
    );
    assert_eq!(a.inner.lock().await.remote_generation, close_generation);
    let state = a.runtime().await.unwrap();
    let mut local = TcpStream::connect(("127.0.0.1", state.services.echo_port))
        .await
        .unwrap();
    echo_roundtrip(&mut local, b"ADB cloud held local continues").await;
    // The old HTTP reply is still held; cancellation completes the owned refresh
    // without reviving its old directory or delaying priority closure.
    let refresh = refresh.await.unwrap();
    assert_eq!(refresh["code"], "APP_ERROR");
    c.directory.release.notify_one();
    c.revocation.release.notify_one();
    let disabled = disable.await.unwrap();
    assert_eq!(disabled["ok"], true);
    assert_eq!(disabled["state"]["localClosed"], true);
    assert_eq!(disabled["state"]["coordinatorRevocation"], "pending");
    assert!(!a.view().await.directory_known);
    assert!(a.view().await.local_closed);
    eprintln!("adb held HTTP responses released: old refresh APP_ERROR, close OK/Pending, live directory unknown and gate closed");
    assert_eq!(a.inner.lock().await.remote_generation, close_generation);
    let latest = control_file_until(&root, "result.json", |v| {
        v["sequence"] == 3 && v["code"] == "BUSY"
    })
    .await;
    assert_eq!(latest["state"]["localClosed"], true);
    let mut gap: Value = serde_json::from_slice(&original_disable).unwrap();
    gap["sequence"] = json!(5);
    gap["requestId"] = json!("00000000-0000-4000-8000-000000000005");
    assert_eq!(
        cli_process(
            &directory,
            "raw",
            "unused",
            Some(&serde_json::to_vec(&gap).unwrap())
        )["published"],
        true
    );
    mailbox_free(&root).await;
    control_file_until(&root, "status.json", |v| {
        v["completed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["code"] == "SEQUENCE_GAP")
    })
    .await;
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(root.join("status.json")).unwrap()).unwrap()
            ["highwater"],
        3
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(root.join("result.json")).unwrap()).unwrap()
            ["sequence"],
        3
    );
    // Completed replay also cannot execute another SDK close or republish stale state.
    assert_eq!(
        cli_process(&directory, "raw", "unused", Some(&original_disable))["published"],
        true
    );
    mailbox_free(&root).await;
    assert_eq!(a.inner.lock().await.remote_generation, close_generation);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(root.join("result.json")).unwrap()).unwrap()
            ["sequence"],
        3
    );
    for name in ["status.json", "result.json", "claim.json"] {
        assert_eq!(
            std::fs::metadata(root.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let status: Value =
        serde_json::from_slice(&std::fs::read(root.join("status.json")).unwrap()).unwrap();
    assert!(status["completed"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.get("state").is_none()));
    let encoded = serde_json::to_string(&status).unwrap();
    for secret in [
        "resource-token",
        "key-1",
        "registrationKey",
        "accessToken",
        "refreshToken",
    ] {
        assert!(!encoded.contains(secret));
    }
    c.state.lock().unwrap().delete_fails = false;
    assert_eq!(
        cli_process(&directory, "close", "close.request", None)["ok"],
        true
    );
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
    cleanup(&a).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_adb_output_failure_stops_admission_but_joins_dispatched_close_owner() {
    let c = Coordinator::start().await;
    let a = c.host("adb-io", Arc::new(Owner::default())).await;
    a.enable_remote().await.unwrap();
    let directory = a.data_dir.parent().unwrap().to_path_buf();
    let root = directory.join("files/adb-control");
    let worker = {
        let root = root.clone();
        let a = a.clone();
        tokio::spawn(crate::adb_control::run_test(root, a))
    };
    control_file_until(&root, "status.json", |v| v["highwater"] == 0).await;
    c.state.lock().unwrap().delete_fails = true;
    c.revocation.armed.store(true, Ordering::SeqCst);
    let disable = {
        let directory = directory.clone();
        tokio::task::spawn_blocking(move || {
            cli_process(&directory, "disable-remote", "disable.request", None)
        })
    };
    tokio::time::timeout(Duration::from_secs(2), c.revocation.entered.notified())
        .await
        .unwrap();
    control_file_until(&root, "status.json", |v| v["state"]["localClosed"] == true).await;
    let blocked = root.join("status.tmp");
    std::fs::create_dir(&blocked).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !worker.is_finished(),
        "output failure cannot drop its already-dispatched cleanup owner"
    );
    assert!(!disable.is_finished());
    assert!(a.view().await.local_closed);
    assert_eq!(a.view().await.remote_action, "disabling");
    eprintln!("adb status output IO failed while DELETE held: cleanup future still owned");
    c.revocation.release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.view().await.remote_action, "off");
    assert_eq!(
        a.view().await.coordinator_revocation,
        Some(CoordinatorRevocation::Pending)
    );
    assert_eq!(disable.await.unwrap()["ok"], true);
    std::fs::remove_dir(blocked).unwrap();
    c.state.lock().unwrap().delete_fails = false;
    cleanup(&a).await;
}
