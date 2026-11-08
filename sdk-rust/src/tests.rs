use super::*;
use crate::mux::{framed_stream, Mux};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

type Handler = Arc<dyn Fn(String, String, Vec<u8>, String) -> (u16, Value) + Send + Sync>;
struct Fixture {
    base: String,
    cert: Vec<u8>,
    cancel: CancellationToken,
}
impl Fixture {
    async fn new(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["127.0.0.1".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {_=stop.cancelled()=>break,res=listener.accept()=>{
                    let Ok((socket,_))=res else{break};let handler=handler.clone();let acceptor=acceptor.clone();
                    tokio::spawn(async move{let Ok(mut s)=acceptor.accept(socket).await else{return};let mut head=Vec::new();let mut one=[0];
                        while !head.ends_with(b"\r\n\r\n"){if head.len()>16384||s.read_exact(&mut one).await.is_err(){return;}head.push(one[0]);}
                        let h=String::from_utf8_lossy(&head);let mut lines=h.lines();let request=lines.next().unwrap_or("");let mut parts=request.split_whitespace();let method=parts.next().unwrap_or("").to_owned();let path=parts.next().unwrap_or("").to_owned();
                        let length=lines.find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);if length>65536{return;}let mut body=vec![0;length];if s.read_exact(&mut body).await.is_err(){return;}
                        let(status,value)=handler(method,path,body,h.lines().find_map(|l|l.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("authorization")).map(|(_,v)|v.trim().to_owned())).unwrap_or_default());let bytes=serde_json::to_vec(&value).unwrap();let header=format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nX-CyberGrid-Epoch: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",value.get("coordinatorEpoch").and_then(Value::as_str).unwrap_or("fixture-epoch"),bytes.len());let _=s.write_all(header.as_bytes()).await;let _=s.write_all(&bytes).await;let _=s.shutdown().await;
                    });
                }}
            }
        });
        Self {
            base: format!("https://{addr}"),
            cert: cert.der().to_vec(),
            cancel,
        }
    }
    fn client(&self, app: &str) -> Arc<Client> {
        let mut c = Client::new(Config {
            base_url: self.base.clone(),
            app_id: app.into(),
            stun_servers: vec![],
            direct_timeout: Duration::from_secs(10),
            ..Config::default()
        })
        .unwrap();
        Arc::get_mut(&mut c).unwrap().http = reqwest::Client::builder()
            .add_root_certificate(reqwest::Certificate::from_der(&self.cert).unwrap())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
fn public_config(base: &str, app: &str) -> Value {
    json!({"coordination":{"protocol":"memory-v1","epoch":"fixture-epoch","heartbeatSeconds":20,"leaseSeconds":65,"signalTtlSeconds":120,"maxSignalWaitSeconds":15},"provider":"logto","issuer":format!("{base}/oidc"),"clientId":app,"resource":"https://cybergrid.aixgg.com/api","scope":"openid offline_access profile email connect","deviceAuthorizationEndpoint":format!("{base}/oidc/device/auth"),"tokenEndpoint":format!("{base}/oidc/token"),"revocationEndpoint":format!("{base}/oidc/token/revocation")})
}

#[test]
fn config_and_tunnel_security() {
    assert!(Client::new(Config::default()).is_err());
    for base in [
        "http://example.com",
        "https://u:p@example.com",
        "https://example.com?token=x",
        "https://example.com/#x",
    ] {
        assert!(Client::new(Config {
            base_url: base.into(),
            app_id: "RealCaseID".into(),
            ..Config::default()
        })
        .is_err());
    }
    assert!(Client::new(Config {
        base_url: "http://127.0.0.1:99".into(),
        app_id: "RealCaseID".into(),
        ..Config::default()
    })
    .is_ok());
    assert!(Client::new(Config {
        app_id: "a".into(),
        stun_servers: vec!["turn:example.com".into()],
        ..Config::default()
    })
    .is_err());
    for u in [
        "https://a.b.trycloudflare.com",
        "http://a.trycloudflare.com",
        "https://a.trycloudflare.com:444",
        "https://a.trycloudflare.com/connect",
        "https://u@a.trycloudflare.com",
        "https://a.trycloudflare.com?q=x",
    ] {
        assert!(validate_tunnel_url(u).is_err());
    }
    assert!(validate_tunnel_url("https://some-real-name.trycloudflare.com/").is_ok());
}
#[test]
fn identity_certificate_pin_name_expiry_and_algorithm() {
    let identity = crate::secure::Identity::new("device-one").unwrap();
    let now = rustls::pki_types::UnixTime::now().as_secs();
    assert!(crate::secure::check_certificate(
        identity.cert.as_ref(),
        "device-one",
        &identity.fingerprint,
        now
    )
    .is_ok());
    assert!(crate::secure::check_certificate(
        identity.cert.as_ref(),
        "device-two",
        &identity.fingerprint,
        now
    )
    .is_err());
    assert!(crate::secure::check_certificate(
        identity.cert.as_ref(),
        "device-one",
        &"0".repeat(64),
        now
    )
    .is_err());
    assert!(crate::secure::check_certificate(
        identity.cert.as_ref(),
        "device-one",
        &identity.fingerprint,
        now + 172800
    )
    .is_err());
    let key = rcgen::KeyPair::generate().unwrap();
    let mut p = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    p.distinguished_name
        .push(rcgen::DnType::CommonName, "device-one");
    let cert = p.self_signed(&key).unwrap();
    let pin = hex::encode(Sha256::digest(cert.der()));
    assert!(
        crate::secure::check_certificate(cert.der().as_ref(), "device-one", &pin, now).is_err()
    );
}
#[tokio::test]
async fn smux_fin_drains_queued_payload_and_odd_even_stream_ids() {
    let (a, b) = tokio::io::duplex(256);
    let cancel = CancellationToken::new();
    let mux = Mux::start(Box::new(a), true, cancel.clone());
    let (mut r, mut w) = tokio::io::split(b);
    let mut stream = mux.open().await.unwrap();
    let mut header = [0; 8];
    r.read_exact(&mut header).await.unwrap();
    assert_eq!(header, [1, 0, 0, 0, 3, 0, 0, 0]);
    let data: Vec<_> = (0..65536).map(|i| (i % 251) as u8).collect();
    let expected = data.clone();
    let write = tokio::spawn(async move {
        for chunk in data.chunks(32768) {
            let mut h = [1, 2, 0, 0, 3, 0, 0, 0];
            h[2..4].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
            w.write_all(&h).await.unwrap();
            w.write_all(chunk).await.unwrap();
        }
        w.write_all(&[1, 1, 0, 0, 3, 0, 0, 0]).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut received = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, expected);
    cancel.cancel();
    write.abort();
}
#[tokio::test]
async fn framed_stream_preserves_half_close_and_final_response() {
    let (a, b) = tokio::io::duplex(65536);
    let cancel = CancellationToken::new();
    let mut framed = framed_stream(a, cancel.clone());
    let remote = tokio::spawn(async move {
        let mut b = b;
        let n = b.read_u32().await.unwrap();
        assert_eq!(n, 5);
        let mut bytes = [0; 5];
        b.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"hello");
        assert_eq!(b.read_u32().await.unwrap(), 0);
        b.write_u32(5).await.unwrap();
        b.write_all(b"reply").await.unwrap();
        b.write_u32(0).await.unwrap();
    });
    framed.write_all(b"hello").await.unwrap();
    framed.shutdown().await.unwrap();
    let mut out = Vec::new();
    framed.read_to_end(&mut out).await.unwrap();
    assert_eq!(&out, b"reply");
    remote.await.unwrap();
    cancel.cancel();
}
#[tokio::test]
async fn v2_cross_app_credentials_preserved_before_any_request() {
    let count = Arc::new(AtomicUsize::new(0));
    let x = count.clone();
    let f = Fixture::new(Arc::new(move |_, _, _, _| {
        x.fetch_add(1, Ordering::SeqCst);
        (500, json!({}))
    }))
    .await;
    let c = f.client("RealA");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private.json");
    let store = CredentialStore::new(&path);
    use std::os::unix::fs::PermissionsExt;
    for (version, app) in [(2, "RealA"), (3, "RealB")] {
        let text=json!({"version":version,"server":f.base,"appId":app,"clientId":app,"issuer":"https://provider.invalid/oidc","resource":"https://api.invalid","refreshToken":"sensitive-fixture"}).to_string();
        std::fs::write(&path, &text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(store.refresh(&c).await.is_err());
        assert!(store.logout(&c).await.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(dir.path().join("other"), &path).unwrap();
    assert!(store.refresh(&c).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn oauth_pending_slowdown_resource_rotation_and_redaction() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let requests2 = requests.clone();
    let f=Fixture::new(Arc::new(move|_,path,body,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        if path=="/oidc/device/auth"{return(200,json!({"device_code":"private-device-code","user_code":"ABCD-EFGH","verification_uri":format!("{}/activate",b.lock().unwrap()),"expires_in":120,"interval":0}));}
        let form=String::from_utf8(body).unwrap();requests2.lock().unwrap().push(form.clone());
        if form.contains("device_code"){
            return match p.fetch_add(1,Ordering::SeqCst){0=>(400,json!({"error":"authorization_pending","error_description":"NEVER-LOG"})),1=>(400,json!({"error":"slow_down"})),_=>(200,json!({"access_token":"opaque-initial","refresh_token":"initial-refresh","expires_in":3600,"token_type":"Bearer"}))};
        }
        if path=="/oidc/token"{assert!(form.contains("resource=https%3A%2F%2Fcybergrid.aixgg.com%2Fapi"));return(200,json!({"access_token":"resource-jwt","refresh_token":"rotated-refresh","expires_in":3600,"token_type":"Bearer"}));}
        (200,json!({}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let auth = c.begin_device_authorization().await.unwrap();
    assert_eq!(auth.interval, 5);
    assert!(!format!("{auth:?}").contains("private-device-code"));
    assert!(!serde_json::to_string(&auth)
        .unwrap()
        .contains("private-device-code"));
    let started = std::time::Instant::now();
    let tokens = c
        .complete_device_authorization(&auth, &CancellationToken::new())
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(19));
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    assert_eq!(tokens.access_token, "resource-jwt");
    assert_eq!(tokens.refresh_token, "rotated-refresh");
    assert!(!format!("{tokens:?}").contains("resource-jwt"));
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path().join("grant.json"));
    store.save_new(&c, &tokens).await.unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(saved["refreshToken"], "rotated-refresh");
    store.refresh(&c).await.unwrap();
    store.logout(&c).await.unwrap();
    assert!(!store.exists());
    assert!(requests
        .lock()
        .unwrap()
        .iter()
        .any(|s| s.contains("refresh_token=initial-refresh")));
}
#[tokio::test]
async fn cancellation_after_mint_preserves_latest_rotation_without_save_network() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let reject_config = Arc::new(AtomicUsize::new(0));
    let reject = reject_config.clone();
    let f = Fixture::new(Arc::new(move |_, path, body, _| {
        if path.starts_with("/api/v1/auth/config?") {
            return if reject.load(Ordering::SeqCst) == 0 {
                (200, public_config(&b.lock().unwrap(), "RealA"))
            } else {
                (503, json!({}))
            };
        }
        if path == "/oidc/device/auth" {
            return (200, json!({"device_code":"fixture-device-code","user_code":"CODE","verification_uri":format!("{}/activate",b.lock().unwrap()),"expires_in":60,"interval":5}));
        }
        let body = String::from_utf8(body).unwrap();
        if body.contains("device_code") {
            stop.cancel();
            return (200, json!({"access_token":"opaque-fixture","refresh_token":"first-fixture-refresh","expires_in":3600,"token_type":"Bearer"}));
        }
        reject.store(1, Ordering::SeqCst);
        (200, json!({"access_token":"resource-fixture","refresh_token":"latest-fixture-refresh","expires_in":3600,"token_type":"Bearer"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let auth = c.begin_device_authorization().await.unwrap();
    let tokens = c
        .complete_device_authorization(&auth, &cancel)
        .await
        .unwrap();
    assert!(cancel.is_cancelled());
    assert_eq!(tokens.refresh_token, "latest-fixture-refresh");
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path().join("grant.json"));
    store.save_new(&c, &tokens).await.unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(saved["refreshToken"], "latest-fixture-refresh");
    assert!(c.auth_config().await.is_err());
}
#[tokio::test]
async fn oauth_cancel_and_configuration_mismatch() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let wrong = Arc::new(AtomicUsize::new(0));
    let w = wrong.clone();
    let f=Fixture::new(Arc::new(move|_,path,_,_|{if path.starts_with("/api/v1/auth/config?"){let app=if w.load(Ordering::SeqCst)==0{"RealA"}else{"WrongApp"};return(200,public_config(&b.lock().unwrap(),app));}
        if path=="/oidc/device/auth"{return(200,json!({"device_code":"private","user_code":"CODE","verification_uri":format!("{}/activate",b.lock().unwrap()),"expires_in":60,"interval":5}));}counter.fetch_add(1,Ordering::SeqCst);(400,json!({"error":"access_denied"}))})).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let auth = c.begin_device_authorization().await.unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        c.complete_device_authorization(&auth, &cancel).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    wrong.store(1, Ordering::SeqCst);
    assert!(c.auth_config().await.is_err());
}

#[derive(Default)]
struct Control {
    epoch: String,
    fail_next_publish: bool,
    attempts: HashMap<String, Value>,
    registrations: Vec<Value>,
    devices: HashMap<String, Value>,
    signals: Vec<Value>,
    n: u64,
}
async fn control() -> (Fixture, Arc<Mutex<Control>>) {
    let state = Arc::new(Mutex::new(Control {
        epoch: "fixture-epoch".into(),
        ..Control::default()
    }));
    let st = state.clone();
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let f=Fixture::new(Arc::new(move|method,path,body,auth|{
        let mut s=st.lock().unwrap();
        if path.starts_with("/api/v1/auth/config?"){let mut config=public_config(&b.lock().unwrap(),"RealA");config["coordination"]["epoch"]=json!(s.epoch);return(200,config);}
        let who=format!("id-{}",auth.strip_prefix("Bearer key-").unwrap_or("missing"));
        if method=="POST"&&path=="/api/v1/auth/exchange"{let v:Value=serde_json::from_slice(&body).unwrap();s.registrations.push(v.clone());let request=v["requestId"].as_str().unwrap().to_owned();if let Some(old)=s.attempts.get(&request){return(200,old.clone());}let name=v["deviceName"].as_str().unwrap();let id=format!("id-{name}");let incarnation=uuid::Uuid::new_v4().to_string();s.devices.insert(id.clone(),json!({"incarnation":incarnation,"publishRevision":0,"id":id,"name":name,"online":true,"lastSeen":chrono::Utc::now(),"ports":[],"tunnelUrl":null,"certificateFingerprint":null}));let out=json!({"coordinatorEpoch":s.epoch,"incarnation":incarnation,"purpose":"device","appId":v["appId"],"sessionKey":format!("key-{name}"),"expiresAt":chrono::Utc::now()+chrono::Duration::hours(1),"deviceId":id,"user":{"id":"fixture-user"}});s.attempts.insert(request,out.clone());return(200,out);}
        // Each fixture client routes its own heartbeat through a separate name query.
        if method=="PUT"&&path.starts_with("/api/v1/devices/self"){
            // Certificate CN maps to the fixture's generated device identity; first missing fingerprint receives each initial heartbeat.
            let body:Value=serde_json::from_slice(&body).unwrap();let fp=body["certificateFingerprint"].clone();
            let id=who.clone();
            let epoch=s.epoch.clone();let fail=s.fail_next_publish;s.fail_next_publish=false;let d=s.devices.get_mut(&id).unwrap();d["publishRevision"]=body["publishRevision"].clone();d["certificateFingerprint"]=fp;d["ports"]=body["ports"].clone();d["tunnelUrl"]=body.get("tunnelUrl").cloned().unwrap_or(Value::Null);if fail{return(599,json!({"coordinatorEpoch":epoch,"error":{"code":"RESPONSE_LOST"}}));}return(200,json!({"device":d,"coordinatorEpoch":epoch}));
        }
        if method=="GET"&&path=="/api/v1/devices"{return(200,json!({"coordinatorEpoch":s.epoch,"devices":s.devices.values().collect::<Vec<_>>()}));}
        if method=="POST"&&path=="/api/v1/signals"{let mut v:Value=serde_json::from_slice(&body).unwrap();s.n+=1;v["id"]=json!(s.n);let kind=v["kind"].as_str().unwrap().to_owned();v["fromDeviceId"]=json!(who);v["coordinatorEpoch"]=json!(s.epoch);v["fromIncarnation"]=s.devices[&who]["incarnation"].clone();assert!(matches!(kind.as_str(),"offer"|"answer"|"candidate"|"close"));s.signals.push(v);return(200,json!({"id":s.n,"coordinatorEpoch":s.epoch}));}
        if method=="GET"&&path.starts_with("/api/v1/signals?"){let after=path.split("after=").nth(1).unwrap().split('&').next().unwrap().parse::<u64>().unwrap();return(200,json!({"coordinatorEpoch":s.epoch,"signals":s.signals.iter().filter(|s|s["id"].as_u64().unwrap()>after&&s["toDeviceId"]==who).collect::<Vec<_>>(),"cursor":s.n}));}
        if method=="DELETE"&&path=="/api/v1/session"{return(200,json!({"ok":true}));}
        (404,json!({"error":{"code":"NOT_FOUND"}}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    (f, state)
}
async fn echo() -> (u16, tokio::task::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
                let _ = w.shutdown().await;
            });
        }
    });
    (port, task)
}
#[tokio::test]
async fn actual_local_websocket_mtls_smux_port_halfclose_wrong_pin_and_close() {
    let (f, state) = control().await;
    let a = f.client("RealA");
    let b = f.client("RealA");
    a.login_with_access_token("fixture-token-a", "a")
        .await
        .unwrap();
    b.login_with_access_token("fixture-token-b", "b")
        .await
        .unwrap();
    let (port, echo) = echo().await;
    b.expose(Port::tcp("echo", port, "echo")).await.unwrap();
    let device = a.device("id-b").await.unwrap();
    let rt = b.active().await.unwrap();
    let endpoint = format!("ws://{}/connect", rt.origin_addr);
    let p = a
        .dial_ws_endpoint(
            &device,
            &endpoint,
            a.active().await.unwrap().cancel.child_token(),
        )
        .await
        .unwrap();
    let mut stream = p.dial("echo").await.unwrap();
    let data = vec![42; 65536];
    let (mut r, mut w) = tokio::io::split(&mut stream);
    let (_, read) = tokio::join!(
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
    assert_eq!(read, data);
    assert!(p.dial("not-exposed").await.is_err());
    state.lock().unwrap().devices.get_mut("id-b").unwrap()["certificateFingerprint"] =
        json!("0".repeat(64));
    let bad = a.device("id-b").await.unwrap();
    assert!(a
        .dial_ws_endpoint(
            &bad,
            &endpoint,
            a.active().await.unwrap().cancel.child_token()
        )
        .await
        .is_err());
    a.close().await.unwrap();
    assert!(p.is_closed());
    b.close().await.unwrap();
    echo.abort();
}

#[tokio::test]
async fn actual_local_webrtc_mapping_and_idle_reuse() {
    let (f, _) = control().await;
    let a = f.client("RealA");
    let b = f.client("RealA");
    a.login_with_access_token("fixture-token-a", "a")
        .await
        .unwrap();
    b.login_with_access_token("fixture-token-b", "b")
        .await
        .unwrap();
    let (port, echo) = echo().await;
    b.expose(Port::tcp("echo", port, "echo")).await.unwrap();
    let peer = tokio::time::timeout(Duration::from_secs(15), a.connect("id-b"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer.transport, "webrtc-direct");
    for round in 0..2 {
        if round == 1 {
            tokio::time::sleep(Duration::from_secs(35)).await;
        }
        let mut stream = peer.dial("echo").await.unwrap();
        let data = vec![round + 7; 65536];
        let (mut r, mut w) = tokio::io::split(&mut stream);
        let (_, out) = tokio::join!(
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
        assert_eq!(out, data);
    }
    assert!(a
        .map("id-b", "echo", "0.0.0.0:0".parse().unwrap())
        .await
        .is_err());
    a.close().await.unwrap();
    assert!(peer.is_closed());
    b.close().await.unwrap();
    echo.abort();
}

#[tokio::test]
async fn resource_failure_keeps_first_grant_and_blocks_old_refresh() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let rotations = Arc::new(Mutex::new(Vec::<String>::new()));
    let rr = rotations.clone();
    let f=Fixture::new(Arc::new(move|_,path,body,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        if path=="/oidc/device/auth"{return(200,json!({"device_code":"private","user_code":"CODE","verification_uri":format!("{}/activate",b.lock().unwrap()),"expires_in":60,"interval":5}));}
        let text=String::from_utf8(body).unwrap();
        if text.contains("device_code") {return(200,json!({"access_token":"identity-access","refresh_token":"first-grant","expires_in":3600,"token_type":"Bearer"}));}
        rr.lock().unwrap().push(text);
        (503,json!({"error":"temporarily_unavailable"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path().join("grant.json"));
    c.use_credential_store(store.clone()).await.unwrap();
    let mut auth = c.begin_device_authorization().await.unwrap();
    auth.interval = 0;
    assert!(c
        .complete_device_authorization(&auth, &CancellationToken::new())
        .await
        .is_err());
    let file: Value = serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(file["refreshToken"], "first-grant");
    assert!(c
        .refresh_access_token("stale-unsafe-refresh")
        .await
        .is_err());
    assert_eq!(rotations.lock().unwrap().len(), 2);
    assert!(rotations
        .lock()
        .unwrap()
        .iter()
        .all(|s| s.contains("refresh_token=first-grant") && !s.contains("stale-unsafe")));
}

#[tokio::test]
async fn failed_checkpoint_retains_latest_and_never_rotates_old_file() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let cc = calls.clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("grant.json");
    let p = path.clone();
    let f=Fixture::new(Arc::new(move|_,path,_,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        cc.fetch_add(1,Ordering::SeqCst);
        std::fs::remove_file(&p).unwrap();std::fs::create_dir(&p).unwrap();
        (200,json!({"access_token":"new-access","refresh_token":"newest-owned","expires_in":3600,"token_type":"Bearer"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let store = CredentialStore::new(&path);
    let t = OAuthTokens {
        access_token: "old-access".into(),
        refresh_token: "old-grant".into(),
        expires_in: 3600,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        token_type: "Bearer".into(),
        scope: String::new(),
    };
    store.save_new(&c, &t).await.unwrap();
    assert!(store.refresh(&c).await.is_err());
    assert!(store.has_pending().await);
    assert!(c.has_pending_grant().await);
    assert!(store.refresh(&c).await.is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "failed disk checkpoint must prevent a second refresh"
    );
    std::fs::remove_dir(&path).unwrap();
    c.retry_pending_grant().await.unwrap();
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["refreshToken"], "newest-owned");
    assert!(!store.has_pending().await);
}

#[tokio::test]
async fn volatile_epoch_does_not_change_immutable_oauth_binding() {
    let epoch = Arc::new(AtomicUsize::new(1));
    let ee = epoch.clone();
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let f = Fixture::new(Arc::new(move |_, _, _, _| {
        let mut config = public_config(&b.lock().unwrap(), "RealA");
        config["coordination"]["epoch"] = json!(format!("boot-{}", ee.load(Ordering::SeqCst)));
        (200, config)
    }))
    .await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    assert_eq!(
        c.auth_config().await.unwrap().coordination.unwrap().epoch,
        "boot-1"
    );
    epoch.store(2, Ordering::SeqCst);
    assert_eq!(
        c.auth_config().await.unwrap().coordination.unwrap().epoch,
        "boot-2"
    );
}

#[tokio::test]
async fn same_origin_oauth_endpoint_and_scope_changes_are_rejected() {
    for field in [
        "tokenEndpoint",
        "revocationEndpoint",
        "deviceAuthorizationEndpoint",
        "scope",
    ] {
        let base = Arc::new(Mutex::new(String::new()));
        let b = base.clone();
        let changed = Arc::new(AtomicUsize::new(0));
        let flag = changed.clone();
        let f = Fixture::new(Arc::new(move |_, _, _, _| {
            let mut cfg = public_config(&b.lock().unwrap(), "RealA");
            if flag.load(Ordering::SeqCst) > 0 {
                cfg[field] = json!(if field == "scope" {
                    "openid offline_access connect profile".to_owned()
                } else {
                    format!("{}/oidc/changed", b.lock().unwrap())
                });
            }
            (200, cfg)
        }))
        .await;
        *base.lock().unwrap() = f.base.clone();
        let c = f.client("RealA");
        c.auth_config().await.unwrap();
        changed.store(1, Ordering::SeqCst);
        assert!(c.auth_config().await.is_err(), "accepted changed {field}");
    }
}

struct TestProvider {
    calls: AtomicUsize,
}
impl AuthProvider for TestProvider {
    fn access_token<'a>(&'a self, _: &'a Client) -> AuthFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok("recovered-resource".into())
        })
    }
}
#[tokio::test]
async fn epoch_reset_recovers_once_republishes_and_close_stops_recovery() {
    let (f, state) = control().await;
    let c = f.client("RealA");
    let auth = Arc::new(TestProvider {
        calls: AtomicUsize::new(0),
    });
    c.set_auth_provider(auth.clone()).await;
    let first = c.login_with_access_token("first", "a").await.unwrap();
    c.expose(Port::tcp("persist", 12345, "intent"))
        .await
        .unwrap();
    let old = c.active().await.unwrap();
    state.lock().unwrap().epoch = "second-boot".into();
    assert!(c.devices().await.is_err());
    assert!(old.cancel.is_cancelled());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(s) = c.device_session().await {
                if s.coordinator_epoch == "second-boot" && c.active().await.is_ok() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let next = c.device_session().await.unwrap();
    assert_eq!(first.device_id, next.device_id);
    assert_ne!(first.incarnation, next.incarnation);
    assert_eq!(auth.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        state.lock().unwrap().devices["id-a"]["ports"][0]["id"],
        "persist"
    );
    c.close().await.unwrap();
    state.lock().unwrap().epoch = "third-boot".into();
    assert!(c.devices().await.is_err());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(auth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn uncertain_first_snapshot_retires_attempt_before_changing_pin() {
    let (f, state) = control().await;
    let c = f.client("RealA");
    c.set_auth_provider(Arc::new(TestProvider {
        calls: AtomicUsize::new(0),
    }))
    .await;
    c.login_with_access_token("first", "a").await.unwrap();
    {
        let mut s = state.lock().unwrap();
        s.epoch = "new-boot".into();
        s.fail_next_publish = true;
    }
    assert!(c.devices().await.is_err());
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if c.device_session()
                .await
                .is_some_and(|s| s.coordinator_epoch == "new-boot")
                && c.active().await.is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    {
        let s = state.lock().unwrap();
        assert_eq!(s.registrations.len(), 3);
        assert_ne!(
            s.registrations[1]["requestId"],
            s.registrations[2]["requestId"]
        );
        assert_eq!(s.registrations[1]["registrationRevision"], 2);
        assert_eq!(s.registrations[2]["registrationRevision"], 3);
    }
    c.close().await.unwrap();
}

#[tokio::test]
async fn mapping_listener_survives_epoch_reset_and_only_new_tcp_uses_fresh_peer() {
    let (f, state) = control().await;
    let a = f.client("RealA");
    let b = f.client("RealA");
    for c in [&a, &b] {
        c.set_auth_provider(Arc::new(TestProvider {
            calls: AtomicUsize::new(0),
        }))
        .await;
    }
    a.login_with_access_token("a", "a").await.unwrap();
    b.login_with_access_token("b", "b").await.unwrap();
    let (port, echo) = echo().await;
    b.expose(Port::tcp("echo", port, "echo")).await.unwrap();
    let mapping = a
        .map("id-b", "echo", "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let mut old = tokio::net::TcpStream::connect(mapping.addr).await.unwrap();
    old.write_all(b"before").await.unwrap();
    let mut before = [0; 6];
    old.read_exact(&mut before).await.unwrap();
    assert_eq!(&before, b"before");
    state.lock().unwrap().epoch = "restarted-owner".into();
    let _ = a.devices().await;
    let _ = b.devices().await;
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if a.device_session()
                .await
                .is_some_and(|s| s.coordinator_epoch == "restarted-owner")
                && b.device_session()
                    .await
                    .is_some_and(|s| s.coordinator_epoch == "restarted-owner")
                && a.active().await.is_ok()
                && b.active().await.is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(mapping.current_transport().await.is_none());
    let eof = tokio::time::timeout(Duration::from_secs(2), old.read(&mut before))
        .await
        .unwrap();
    assert!(
        matches!(eof, Ok(0) | Err(_)),
        "old TCP must end instead of replaying bytes"
    );
    let mut next = tokio::net::TcpStream::connect(mapping.addr).await.unwrap();
    next.write_all(b"after").await.unwrap();
    let mut bytes = [0; 5];
    tokio::time::timeout(Duration::from_secs(8), next.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes, b"after");
    assert!(mapping.current_transport().await.is_some());
    mapping.close().await;
    a.close().await.unwrap();
    b.close().await.unwrap();
    echo.abort();
}

#[tokio::test]
async fn pending_checkpoint_and_next_rotation_share_one_authorization_gate() {
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let cc = calls.clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("grant.json");
    let p = path.clone();
    let f=Fixture::new(Arc::new(move|_,path,body,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        let n=cc.fetch_add(1,Ordering::SeqCst);
        if n==0 {std::fs::remove_file(&p).unwrap();std::fs::create_dir(&p).unwrap();}
        else {let form=url::form_urlencoded::parse(&body).collect::<HashMap<_,_>>();assert_eq!(form.get("refresh_token").unwrap(),"owned-one");}
        (200,json!({"access_token":"access","refresh_token":if n==0{"owned-one"}else{"owned-two"},"expires_in":3600,"token_type":"Bearer"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let c = f.client("RealA");
    let store = CredentialStore::new(&path);
    let old = OAuthTokens {
        access_token: "access".into(),
        refresh_token: "old".into(),
        expires_in: 3600,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        token_type: "Bearer".into(),
        scope: String::new(),
    };
    store.save_new(&c, &old).await.unwrap();
    assert!(store.refresh(&c).await.is_err());
    std::fs::remove_dir(&path).unwrap();
    let gate = store.auth_gate.lock().await;
    let retry_client = c.clone();
    let retry = tokio::spawn(async move { retry_client.retry_pending_grant().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let next_store = store.clone();
    let next_client = c.clone();
    let next = tokio::spawn(async move { next_store.refresh(&next_client).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "rotation must wait behind pending checkpoint"
    );
    drop(gate);
    retry.await.unwrap().unwrap();
    next.await.unwrap().unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["refreshToken"], "owned-two");
    assert!(!c.has_pending_grant().await);
    assert!(store.save_new(&c, &old).await.is_err());
}

#[tokio::test]
async fn provider_exit_withdraws_url_and_close_joins_child_without_respawn() {
    use std::os::unix::fs::PermissionsExt;
    let (f, state) = control().await;
    let mut c = f.client("RealA");
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("provider-fixture.sh");
    let record = root.path().join("pids");
    std::fs::write(&script,format!("#!/bin/sh\necho $$ >> '{}'\necho 'https://fixture-test.trycloudflare.com'\necho 'Registered tunnel connection'\nexec /bin/sleep 60\n",record.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    Arc::get_mut(&mut c).unwrap().cfg.cloudflared = Some(CloudflaredConfig::new(script));
    let reads = Arc::new(AtomicUsize::new(0));
    let reads_callback = reads.clone();
    Arc::get_mut(&mut c)
        .unwrap()
        .cfg
        .cloudflared
        .as_mut()
        .unwrap()
        .env_provider = Some(Arc::new(move || {
        reads_callback.fetch_add(1, Ordering::SeqCst);
        Ok(HashMap::new())
    }));
    let mut events = c.subscribe();
    c.login_with_access_token("first", "a").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if events.recv().await.unwrap().phase == "tunnel-ready" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let first: i32 = std::fs::read_to_string(&record)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(first, libc::SIGTERM) }, 0);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if events.recv().await.unwrap().phase == "provider-restarting" {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        state.lock().unwrap().devices["id-a"]["tunnelUrl"].is_null(),
        "failed URL must be withdrawn before restart"
    );
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if events.recv().await.unwrap().phase == "tunnel-ready" {
                break;
            }
        }
    })
    .await
    .unwrap();
    c.close().await.unwrap();
    let pids = std::fs::read_to_string(&record).unwrap();
    assert_eq!(pids.lines().count(), 2);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "each start must refresh platform environment"
    );
    for pid in pids.lines() {
        assert_eq!(
            unsafe { libc::kill(pid.parse().unwrap(), 0) },
            -1,
            "Close must reap its provider child"
        );
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        pids,
        "closed generation must not restart provider"
    );
}

#[tokio::test]
async fn v4_persisted_oauth_binding_rejects_endpoint_changes_in_a_new_client() {
    for field in [
        "tokenEndpoint",
        "revocationEndpoint",
        "deviceAuthorizationEndpoint",
        "scope",
    ] {
        let base = Arc::new(Mutex::new(String::new()));
        let b = base.clone();
        let changed = Arc::new(AtomicUsize::new(0));
        let flag = changed.clone();
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let f = Fixture::new(Arc::new(move |_, path, _, _| {
            if path.starts_with("/api/v1/auth/config?") {
                let mut cfg = public_config(&b.lock().unwrap(), "RealA");
                if flag.load(Ordering::SeqCst) > 0 {
                    cfg[field] = json!(if field == "scope" {
                        "openid offline_access connect profile".to_owned()
                    } else {
                        format!("{}/oidc/changed", b.lock().unwrap())
                    });
                }
                return (200, cfg);
            }
            count.fetch_add(1, Ordering::SeqCst);
            (400, json!({"error":"invalid_grant"}))
        }))
        .await;
        *base.lock().unwrap() = f.base.clone();
        let root = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(root.path().join("grant.json"));
        let c = f.client("RealA");
        let t = OAuthTokens {
            access_token: "access".into(),
            refresh_token: "private".into(),
            expires_in: 3600,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            token_type: "Bearer".into(),
            scope: String::new(),
        };
        store.save_new(&c, &t).await.unwrap();
        let before = std::fs::read(store.path()).unwrap();
        drop(c);
        changed.store(1, Ordering::SeqCst);
        let new_client = f.client("RealA");
        assert!(
            store.refresh(&new_client).await.is_err(),
            "accepted persisted {field} change"
        );
        assert_eq!(
            requests.load(Ordering::SeqCst),
            0,
            "must reject before sending refresh"
        );
        assert_eq!(std::fs::read(store.path()).unwrap(), before);
    }
}
#[tokio::test]
async fn v3_is_preserved_and_rejected_before_refresh_network() {
    use std::os::unix::fs::PermissionsExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let f = Fixture::new(Arc::new(move |_, _, _, _| {
        count.fetch_add(1, Ordering::SeqCst);
        (500, json!({}))
    }))
    .await;
    let c = f.client("RealA");
    let root = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(root.path().join("legacy.json"));
    let bytes=serde_json::to_vec(&json!({"version":3,"server":f.base,"appId":"RealA","issuer":"https://old.example/oidc","clientId":"RealA","resource":"https://cybergrid.aixgg.com/api","refreshToken":"old-private"})).unwrap();
    std::fs::write(store.path(), &bytes).unwrap();
    std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(store.refresh(&c).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
}

#[tokio::test]
async fn credential_cross_process_child() {
    let Ok(path) = std::env::var("CYBERGRID_LEASE_TEST_PATH") else {
        return;
    };
    let server = std::env::var("CYBERGRID_LEASE_TEST_SERVER").unwrap();
    let certificate = std::fs::read(std::env::var("CYBERGRID_LEASE_TEST_CERT").unwrap()).unwrap();
    let mut client = Client::new(Config {
        base_url: server,
        app_id: "RealA".into(),
        ..Config::default()
    })
    .unwrap();
    Arc::get_mut(&mut client).unwrap().http = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let store = CredentialStore::new(path);
    if std::env::var("CYBERGRID_LEASE_TEST_ACTION").unwrap() == "blocked" {
        assert!(
            tokio::time::timeout(Duration::from_millis(300), store.refresh(&client))
                .await
                .is_err()
        );
    } else {
        store.refresh(&client).await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disk_failure_holds_cross_process_lease_until_checkpoint_then_new_process_refreshes_latest()
{
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let storage = root.path().join("private");
    std::fs::create_dir(&storage).unwrap();
    let path = storage.join("grant.json");
    let blocked = storage.clone();
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let f=Fixture::new(Arc::new(move|_,path,body,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        let n=counter.fetch_add(1,Ordering::SeqCst);if n==0{std::fs::set_permissions(&blocked,std::fs::Permissions::from_mode(0o500)).unwrap();}else{let form=url::form_urlencoded::parse(&body).collect::<HashMap<_,_>>();assert_eq!(form.get("refresh_token").unwrap(),"latest-one");}
        (200,json!({"access_token":"access","refresh_token":if n==0{"latest-one"}else{"latest-two"},"expires_in":3600,"token_type":"Bearer"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let client = f.client("RealA");
    let store = CredentialStore::new(&path);
    let initial = OAuthTokens {
        access_token: "access".into(),
        refresh_token: "old-on-disk".into(),
        expires_in: 3600,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        token_type: "Bearer".into(),
        scope: String::new(),
    };
    store.save_new(&client, &initial).await.unwrap();
    assert!(store.refresh(&client).await.is_err());
    assert!(store.has_pending().await);
    let old: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(old["refreshToken"], "old-on-disk");
    let cert = root.path().join("fixture.der");
    std::fs::write(&cert, &f.cert).unwrap();
    let child = |action: &str| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::credential_cross_process_child",
                "--nocapture",
            ])
            .env("CYBERGRID_LEASE_TEST_PATH", &path)
            .env("CYBERGRID_LEASE_TEST_SERVER", &f.base)
            .env("CYBERGRID_LEASE_TEST_CERT", &cert)
            .env("CYBERGRID_LEASE_TEST_ACTION", action)
            .output()
            .unwrap()
    };
    let result = child("blocked");
    assert!(
        result.status.success(),
        "child lock barrier failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "another process must not send the old refresh"
    );
    std::fs::set_permissions(&storage, std::fs::Permissions::from_mode(0o700)).unwrap();
    tokio::time::timeout(Duration::from_secs(3), client.retry_pending_grant())
        .await
        .unwrap()
        .unwrap();
    let result = child("refresh");
    assert!(
        result.status.success(),
        "child refresh after release failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["refreshToken"], "latest-two");
}

#[tokio::test]
async fn pending_grant_can_be_revoked_after_close_while_disk_stays_unwritable() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let storage = root.path().join("private");
    std::fs::create_dir(&storage).unwrap();
    let path = storage.join("grant.json");
    let blocked = storage.clone();
    let base = Arc::new(Mutex::new(String::new()));
    let b = base.clone();
    let revoke = Arc::new(AtomicUsize::new(0));
    let revoked = revoke.clone();
    let f=Fixture::new(Arc::new(move|_,path,body,_|{
        if path.starts_with("/api/v1/auth/config?"){return(200,public_config(&b.lock().unwrap(),"RealA"));}
        if path.ends_with("/revocation"){let form=url::form_urlencoded::parse(&body).collect::<HashMap<_,_>>();assert_eq!(form.get("token").unwrap(),"newest-revocable");revoked.fetch_add(1,Ordering::SeqCst);return(200,json!({}));}
        std::fs::set_permissions(&blocked,std::fs::Permissions::from_mode(0o500)).unwrap();(200,json!({"access_token":"access","refresh_token":"newest-revocable","expires_in":3600,"token_type":"Bearer"}))
    })).await;
    *base.lock().unwrap() = f.base.clone();
    let client = f.client("RealA");
    let store = CredentialStore::new(&path);
    let initial = OAuthTokens {
        access_token: "access".into(),
        refresh_token: "old".into(),
        expires_in: 3600,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        token_type: "Bearer".into(),
        scope: String::new(),
    };
    store.save_new(&client, &initial).await.unwrap();
    assert!(store.refresh(&client).await.is_err());
    client.close().await.unwrap();
    // Remote revocation succeeds although deleting the stale local file cannot.
    assert!(client.revoke_pending_grant().await.is_err());
    assert_eq!(revoke.load(Ordering::SeqCst), 1);
    assert!(!client.has_pending_grant().await);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&lock).is_ok());
    std::fs::set_permissions(&storage, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_during_environment_callback_prevents_provider_spawn() {
    use std::os::unix::fs::PermissionsExt;
    let (f, _) = control().await;
    let mut c = f.client("RealA");
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("must-not-start.sh");
    let record = root.path().join("started");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho started > '{}'\n", record.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let notify = started.clone();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let release = barrier.clone();
    let mut provider = CloudflaredConfig::new(script);
    provider.env_provider = Some(Arc::new(move || {
        notify.notify_one();
        release.wait();
        Ok(HashMap::new())
    }));
    Arc::get_mut(&mut c).unwrap().cfg.cloudflared = Some(provider);
    c.login_with_access_token("first", "a").await.unwrap();
    started.notified().await;
    let close_client = c.clone();
    let close = tokio::spawn(async move { close_client.close().await });
    c.closed_token().cancelled().await;
    barrier.wait();
    close.await.unwrap().unwrap();
    assert!(
        !record.exists(),
        "Close must fence spawn after environment callback completes"
    );
}
