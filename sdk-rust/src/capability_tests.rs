use super::*;
use crate::{validate_tunnel_url, BridgeLimits, BrowserBridge, EndpointPolicy, EndpointProvider};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
struct Node {
    child: Child,
    left: String,
    right: String,
    tokens: Value,
    http: reqwest::Client,
}
impl Node {
    async fn start() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new("node")
            .arg("--import")
            .arg(root.join("../server/node_modules/tsx/dist/loader.mjs"))
            .arg(root.join("test-support/coordinator.ts"))
            .env("REQUIRE_REDIS", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("Node+server dependencies required");
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let info: Value = serde_json::from_str(&line).expect("Node fixture readiness");
        Self {
            child,
            left: info["left"].as_str().unwrap().into(),
            right: info["right"].as_str().unwrap().into(),
            tokens: info["tokens"].clone(),
            http: reqwest::Client::new(),
        }
    }
    fn client(&self, mobile: bool) -> Arc<Client> {
        Client::new(Config {
            base_url: if mobile {
                self.right.clone()
            } else {
                self.left.clone()
            },
            app_id: if mobile { "mobile" } else { "desktop" }.into(),
            iroh_enabled: false,
            stun_servers: vec![],
            ..Config::default()
        })
        .unwrap()
    }
    fn token(&self, name: &str) -> &str {
        self.tokens[name].as_str().unwrap()
    }
    async fn control(&self, v: Value) {
        let r = self
            .http
            .post(format!("{}/test/control", self.left))
            .json(&v)
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }
    async fn stats(&self) -> Value {
        self.http
            .get(format!("{}/test/stats", self.left))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    async fn stop(mut self) {
        self.child.stdin.take();
        tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .unwrap()
            .unwrap();
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.child.stdin.take();
    }
}
#[derive(Default)]
struct Owner {
    token: Mutex<String>,
    requests: Mutex<Vec<(AuthRequestReason, bool)>>,
}
impl AuthProvider for Owner {
    fn access_token_for<'a>(
        &'a self,
        _: &'a Client,
        request: &'a AuthTokenRequest,
    ) -> AuthFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .unwrap()
                .push((request.reason, request.force_refresh));
            Ok(self.token.lock().unwrap().clone())
        })
    }
}
fn bridge() -> BrowserBridge {
    use base64::Engine;
    BrowserBridge {
        id: "bridge".into(),
        protocol: "noise.ik.v1".into(),
        provider_id: "quick-tunnel".into(),
        url: "wss://fixture.trycloudflare.com/bridge".into(),
        installation_id: "installation".into(),
        public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 32]),
        address_revision: 1,
        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(4))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        limits: BridgeLimits {
            max_frame_bytes: 4096,
            max_streams: 4,
            max_pending_requests: 8,
        },
    }
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_node_redis_jwk_finite_chain_two_native_directory_bridge_intent_and_policy() {
    let n = Node::start().await;
    let a = n.client(false);
    let b = n.client(true);
    let sa = a
        .login_with_access_token(n.token("desktop"), "desktop")
        .await
        .unwrap();
    let sb = b
        .login_with_access_token(n.token("mobile"), "mobile")
        .await
        .unwrap();
    assert_eq!(sa.product_id.as_deref(), Some("desktop"));
    assert_eq!(sb.product_id, sa.product_id);
    assert!(!a.remote_access_enabled());
    a.publish_bridge(bridge()).await.unwrap();
    assert!(a
        .directory(None)
        .await
        .unwrap()
        .devices
        .iter()
        .all(|d| d.bridges.is_empty()));
    a.enable_remote().await.unwrap();
    let first = b.directory(None).await.unwrap();
    assert_eq!(first.devices.len(), 2);
    assert_eq!(first.devices_for_installation("installation").count(), 1);
    assert_eq!(
        first
            .devices
            .iter()
            .find(|d| d.id == sa.device_id)
            .unwrap()
            .bridges
            .len(),
        1
    );
    let same = b.directory(Some(&first.revision)).await.unwrap();
    assert!(!same.changed);
    assert!(!same.reset);
    let mut changed = bridge();
    changed.public_key = first
        .devices
        .iter()
        .find(|d| d.id == sa.device_id)
        .unwrap()
        .bridges[0]
        .public_key
        .clone();
    changed.limits.max_streams = 3;
    assert!(a.publish_bridge(changed.clone()).await.is_err());
    changed.address_revision = 2;
    a.publish_bridge(changed).await.unwrap();
    assert!(b.directory(Some(&first.revision)).await.unwrap().changed);
    a.withdraw_bridge("bridge").await.unwrap();
    assert!(b
        .directory(None)
        .await
        .unwrap()
        .devices
        .iter()
        .all(|d| d.bridges.is_empty()));
    assert!(a.publish_bridge(bridge()).await.is_err());
    let formal = json!({"version":1,"providers":[{"id":"work","origins":["https://remote.example.test:8443"],"paths":["/","/bridge"]}]});
    n.control(json!({"policy":formal})).await;
    a.auth_config().await.unwrap();
    a.update_device_tunnel_url(Some("https://remote.example.test:8443"))
        .await
        .unwrap();
    let mut p = Port::tcp("http", 1234, "http");
    p.tunnel_url = Some("https://remote.example.test:8443/bridge".into());
    a.expose(p).await.unwrap();
    let d = b.directory(None).await.unwrap();
    let remote = d.devices.iter().find(|d| d.id == sa.device_id).unwrap();
    assert_eq!(
        remote.tunnel_url.as_deref(),
        Some("https://remote.example.test:8443")
    );
    assert_eq!(
        remote.ports[0].tunnel_url.as_deref(),
        Some("https://remote.example.test:8443/bridge")
    );
    assert!(a
        .update_device_tunnel_url(Some("https://remote.example.test:8443/bridge"))
        .await
        .is_err());
    assert!(a
        .update_port_tunnel_url("http", Some("https://remote.example.test:8444/bridge"))
        .await
        .is_err());
    a.close().await.unwrap();
    b.close().await.unwrap();
    n.stop().await;
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_unknown_mint_and_register_replay_exact_private_operations_without_token_rotation() {
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    n.control(json!({"losses":{"mint":1,"register":1}})).await;
    assert!(matches!(
        c.login_with_auth_provider("original").await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    assert!(matches!(
        c.login_with_access_token(n.token("stranger"), "changed")
            .await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    let s = c
        .login_with_access_token(n.token("stranger"), "changed")
        .await
        .unwrap();
    assert_eq!(s.user.id, "owner");
    let calls = n.stats().await;
    let mint = calls["calls"]["mint"].as_array().unwrap();
    assert_eq!(mint.len(), 2);
    assert_eq!(mint[0], mint[1]);
    let reg = calls["calls"]["register"].as_array().unwrap();
    assert_eq!(reg.len(), 2);
    assert_eq!(reg[0], reg[1]);
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    c.close().await.unwrap();
    n.stop().await;
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_cancelled_mint_remains_owned_and_late_result_cannot_replace_request() {
    let n = Node::start().await;
    let c = n.client(false);
    n.control(json!({"hold":"mint"})).await;
    let clone = c.clone();
    let token = n.token("desktop").to_owned();
    let login =
        tokio::spawn(async move { clone.login_with_access_token(&token, "original").await });
    for _ in 0..100 {
        if n.stats().await["calls"]["mint"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    login.abort();
    let _ = login.await;
    n.control(json!({"release":true})).await;
    let s = c
        .login_with_access_token(n.token("stranger"), "replacement")
        .await
        .unwrap();
    assert_eq!(s.user.id, "owner");
    let calls = n.stats().await;
    assert_eq!(calls["calls"]["mint"][0], calls["calls"]["mint"][1]);
    c.close().await.unwrap();
    n.stop().await;
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_trusted_new_epoch_retires_uncertain_finite_operation_and_preserves_logical_identity()
{
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    let first = c.login_with_auth_provider("original").await.unwrap();
    // Make the current known generation unavailable and hold its next exchange result.
    c.active().await.unwrap().cancel.cancel();
    n.control(json!({"losses":{"register":1}})).await;
    assert!(matches!(
        c.login_with_auth_provider("ignored").await,
        Err(Error::Protocol(_))
    ));
    // A fresh Client tests initial unknown epoch replacement without mutating the known owner.
    let d = n.client(false);
    d.set_auth_provider(owner.clone()).await;
    assert!(matches!(
        d.login_with_auth_provider("new").await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    let old_id = n.stats().await["registeredIds"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    n.control(json!({"epoch":true})).await;
    let next = d.login_with_auth_provider("ignored").await.unwrap();
    assert_ne!(next.coordinator_epoch, first.coordinator_epoch);
    assert_eq!(next.device_id, old_id);
    assert_eq!(owner.requests.lock().unwrap().len(), 3);
    assert!(d.close().await.is_ok());
    let _ = c.close().await;
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_formal_wss_authority_port_connect_inner_mtls_and_stream_data() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    let n = Node::start().await;
    let a = n.client(false);
    let b = n.client(true);
    let sa = a
        .login_with_access_token(n.token("desktop"), "a")
        .await
        .unwrap();
    let sb = b
        .login_with_access_token(n.token("mobile"), "b")
        .await
        .unwrap();
    let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_port = echo.local_addr().unwrap().port();
    let echo_task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = echo.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
                let _ = w.shutdown().await;
            });
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["127.0.0.1".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
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
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let connector = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    *a.ws_connector.write().unwrap() = Some(Arc::new(connector));
    let origin = b.active().await.unwrap().origin_addr;
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let proxy = tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            let seen = count.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(socket).await else {
                    return;
                };
                let mut head = Vec::new();
                let mut one = [0];
                while !head.ends_with(b"\r\n\r\n") {
                    if head.len() > 16384 || tls.read_exact(&mut one).await.is_err() {
                        return;
                    }
                    head.push(one[0]);
                }
                let h = String::from_utf8(head.clone()).unwrap();
                assert!(h.starts_with("GET /connect HTTP/1.1\r\n"));
                assert!(h.to_lowercase().contains(&format!("host: {addr}\r\n")));
                seen.fetch_add(1, Ordering::SeqCst);
                let mut upstream = TcpStream::connect(origin).await.unwrap();
                upstream.write_all(&head).await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut tls, &mut upstream).await;
            });
        }
    });
    let root = format!("https://{addr}");
    n.control(json!({"policy":{"version":1,"providers":[{"id":"fixture","origins":[root],"paths":["/"]}]}})).await;
    b.auth_config().await.unwrap();
    b.update_device_tunnel_url(Some(&root)).await.unwrap();
    b.expose(Port::tcp("echo", echo_port, "echo"))
        .await
        .unwrap();
    b.enable_remote().await.unwrap();
    let d = a
        .devices()
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.id == sb.device_id)
        .unwrap();
    assert_eq!(d.tunnel_url.as_deref(), Some(root.as_str()));
    assert_eq!(
        crate::tunnel::device_websocket_url(&root).unwrap(),
        format!("wss://{addr}/connect")
    );
    let p = a.dial_tunnel(&d).await.unwrap();
    let stream = p.dial("echo").await.unwrap();
    let bytes = vec![91u8; 65536];
    let (mut r, mut w) = tokio::io::split(stream);
    let read = tokio::spawn(async move {
        let mut out = Vec::new();
        r.read_to_end(&mut out).await.unwrap();
        out
    });
    w.write_all(&bytes).await.unwrap();
    w.shutdown().await.unwrap();
    assert_eq!(read.await.unwrap(), bytes);
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert_ne!(sa.device_id, sb.device_id);
    a.close().await.unwrap();
    b.close().await.unwrap();
    assert!(p.is_closed());
    echo_task.abort();
    proxy.abort();
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_same_epoch_unknown_registration_after_jwk_identity_rejection_keeps_original_token_and_id(
) {
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    n.control(json!({"losses":{"register":1}})).await;
    assert!(matches!(
        c.login_with_auth_provider("original").await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    n.control(json!({"rejectIdentity":true})).await;
    for _ in 0..2 {
        assert!(matches!(
            c.login_with_auth_provider("replacement").await,
            Err(Error::ExchangeOutcomeUnknown)
        ));
    }
    let calls = n.stats().await;
    let rs = calls["calls"]["register"].as_array().unwrap();
    assert_eq!(rs.len(), 3);
    assert!(rs.iter().all(|h| h == &rs[0]));
    assert_eq!(calls["calls"]["mint"].as_array().unwrap().len(), 1);
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    assert_eq!(
        c.disable_remote().await.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    n.stop().await;
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_cancelled_register_remains_unknown_after_commit_and_close_reports_pending() {
    let n = Node::start().await;
    let c = n.client(false);
    n.control(json!({"hold":"register"})).await;
    let clone = c.clone();
    let token = n.token("desktop").to_owned();
    let login =
        tokio::spawn(async move { clone.login_with_access_token(&token, "original").await });
    for _ in 0..100 {
        if n.stats().await["registeredIds"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    login.abort();
    let _ = login.await;
    n.control(json!({"release":true})).await;
    assert_eq!(
        c.disable_remote().await.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    assert!(!c.remote_access_enabled());
    n.stop().await;
}
fn empty_policy() -> EndpointPolicy {
    use sha2::{Digest, Sha256};
    EndpointPolicy {
        version: 1,
        policy_version: hex::encode(Sha256::digest(br#"["cybergrid-endpoint-policy-v1",[]]"#)),
        providers: vec![],
    }
}
fn formal_policy() -> EndpointPolicy {
    use sha2::{Digest, Sha256};
    let provider = EndpointProvider {
        id: "work".into(),
        origins: vec!["https://remote.example.test:8443".into()],
        paths: vec!["/".into(), "/bridge".into()],
    };
    let bytes = serde_json::to_vec(&json!([
        "cybergrid-endpoint-policy-v1",
        [[provider.id, provider.origins, provider.paths]]
    ]))
    .unwrap();
    EndpointPolicy {
        version: 1,
        policy_version: hex::encode(Sha256::digest(bytes)),
        providers: vec![provider],
    }
}
fn auth_value() -> Value {
    json!({"provider":"logto","issuer":"https://tenant.example.test/oidc","clientId":"desktop","resource":"https://resource.example.test","scope":"openid offline_access connect","deviceAuthorizationEndpoint":"https://tenant.example.test/oidc/device/auth","tokenEndpoint":"https://tenant.example.test/oidc/token"})
}
#[test]
fn strict_capability_negotiation_and_immutable_grant_binding_are_separate_from_metadata() {
    let legacy: AuthConfig = serde_json::from_value(auth_value()).unwrap();
    legacy.validate("desktop").unwrap();
    let roundtrip: AuthConfig =
        serde_json::from_value(serde_json::to_value(&legacy).unwrap()).unwrap();
    assert_eq!(roundtrip, legacy);
    for value in [
        Value::Null,
        json!({"version":3,"challengeSeconds":120,"ticketSeconds":86400}),
        json!({"version":2,"challengeSeconds":121,"ticketSeconds":86400}),
        json!({"version":2,"challengeSeconds":120,"ticketSeconds":86400,"extra":true}),
    ] {
        let mut v = auth_value();
        v["registrationAdmission"] = value;
        let result = serde_json::from_value::<AuthConfig>(v);
        assert!(result.is_err() || result.unwrap().validate("desktop").is_err());
    }
    let mut v = auth_value();
    v["endpointPolicy"] = json!(formal_policy());
    v["registrationAdmission"] = json!({"version":2,"challengeSeconds":120,"ticketSeconds":86400});
    v["productId"] = json!("desktop");
    v["policyVersion"] = json!("a".repeat(64));
    v["products"] = json!([{"id":"desktop","name":"Workspace"}]);
    let current: AuthConfig = serde_json::from_value(v).unwrap();
    current.validate("desktop").unwrap();
    assert!(legacy.same_oauth_binding(&current));
    assert!(legacy.same_binding(&current, "desktop"));
    assert!(legacy.same_client_binding(&current, "desktop").unwrap());
    assert!(legacy
        .same_client_binding(&current, "other-client")
        .is_err());
    for field in ["productId", "policyVersion", "products"] {
        let mut malformed = auth_value();
        malformed[field] = Value::Null;
        assert!(serde_json::from_value::<AuthConfig>(malformed).is_err());
    }
    let mut incomplete = auth_value();
    incomplete["products"] = json!([]);
    assert!(serde_json::from_value::<AuthConfig>(incomplete)
        .unwrap()
        .validate("desktop")
        .is_err());
    let mut metadata = current.clone();
    metadata.endpoint_policy = Some(empty_policy());
    metadata.products.as_mut().unwrap()[0].name = "New name".into();
    assert!(current.same_client_binding(&metadata, "desktop").unwrap());
    for field in [
        "provider",
        "issuer",
        "clientId",
        "resource",
        "scope",
        "loginFlow",
        "deviceAuthorizationEndpoint",
        "tokenEndpoint",
        "revocationEndpoint",
    ] {
        let mut changed = serde_json::to_value(&legacy).unwrap();
        changed[field] = json!("changed");
        let changed: AuthConfig = serde_json::from_value(changed).unwrap();
        assert!(!legacy.same_oauth_binding(&changed), "{field}");
    }
    let mut moved = current.clone();
    moved.product_id = Some("other".into());
    assert!(moved.same_oauth_binding(&current));
    assert!(!moved.same_binding(&current, "desktop"));
    let mut changed = current.clone();
    changed.resource = "https://other.example.test".into();
    assert!(!changed.same_oauth_binding(&current));
}
#[test]
fn strict_endpoint_raw_spelling_origin_ports_paths_and_bridge_limits() {
    let p = formal_policy();
    p.validate().unwrap();
    empty_policy().validate().unwrap();
    assert_eq!(
        p.tunnel_url("https://remote.example.test:8443/", true)
            .unwrap(),
        "https://remote.example.test:8443"
    );
    assert_eq!(
        p.bridge_url("wss://remote.example.test:8443/bridge", "work")
            .unwrap(),
        "wss://remote.example.test:8443/bridge"
    );
    assert_eq!(
        crate::tunnel::device_websocket_url("https://[2001:db8::1]:8443").unwrap(),
        "wss://[2001:db8::1]:8443/connect"
    );
    for raw in [
        "https://REMOTE.example.test:8443/bridge",
        "https://remote.example.test:8443/a/../bridge",
        "https://remote.example.test:8443/%62ridge",
        "https://remote.example.test:8443//bridge",
        "https://remote.example.test:8443/bridge?",
        "https://remote.example.test:8443/bridge#",
        "https://remote.example.test:8444/bridge",
        "https://remote.example.test:443/bridge",
    ] {
        assert!(p.tunnel_url(raw, false).is_err(), "{raw}");
    }
    for raw in [
        "wss://fixture.trycloudflare.com:443/bridge",
        "wss://fixture.trycloudflare.com/a/../bridge",
        "wss://fixture.trycloudflare.com/bridge?",
        "wss://fixture.trycloudflare.com/%62ridge",
    ] {
        assert!(empty_policy().bridge_url(raw, "quick-tunnel").is_err());
    }
    assert!(validate_tunnel_url(&format!("https://{}.trycloudflare.com", "a".repeat(63))).is_ok());
    assert!(validate_tunnel_url(&format!("https://{}.trycloudflare.com", "a".repeat(64))).is_err());
    let mut b = bridge();
    b.validate(None).unwrap();
    b.limits.max_streams = 33;
    assert!(b.validate(None).is_err());
    let mut v = json!(bridge());
    v.as_object_mut().unwrap().remove("limits");
    assert!(serde_json::from_value::<BrowserBridge>(v).is_err());
    let mut bad = p.clone();
    bad.policy_version = "a".repeat(64);
    assert!(bad.validate().is_err());
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_known_session_expiry_uses_one_fresh_owner_authorization_and_stable_logical_device() {
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    let first = c.login_with_auth_provider("stable").await.unwrap();
    // Model the client's known absolute deadline without altering Redis TIME or claiming 24h elapsed.
    c.session.write().await.as_mut().unwrap().expires_at =
        chrono::Utc::now() - chrono::Duration::seconds(1);
    assert!(c.devices().await.is_err());
    let next = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(s) = c.device_session().await {
                if s.incarnation != first.incarnation && c.active().await.is_ok() {
                    break s;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(next.device_id, first.device_id);
    assert_eq!(next.user.id, first.user.id);
    assert!(!c.remote_access_enabled());
    assert_eq!(
        owner.requests.lock().unwrap().as_slice(),
        [
            (AuthRequestReason::Connect, false),
            (AuthRequestReason::CoordinationRecovery, true)
        ]
    );
    assert_eq!(
        n.stats().await["calls"]["mint"].as_array().unwrap().len(),
        2
    );
    c.close().await.unwrap();
    n.stop().await;
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_product_move_closes_local_gate_and_cannot_rebind_existing_client() {
    let n = Node::start().await;
    let c = n.client(false);
    c.login_with_access_token(n.token("desktop"), "stable")
        .await
        .unwrap();
    c.enable_remote().await.unwrap();
    n.control(json!({"products":{"version":1,"products":[{"id":"mobile","name":"Moved","nativeClientIds":["desktop","mobile"],"browserClientIds":["browser"]}]}})).await;
    assert!(c.auth_config().await.is_err());
    assert!(c.is_closed());
    assert!(!c.remote_access_enabled());
    assert!(c.devices().await.is_err());
    assert!(c
        .login_with_access_token(n.token("desktop"), "moved")
        .await
        .is_err());
    let _ = c.close().await;
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_admission_expired_unknown_stops_replay_without_implicit_new_domain() {
    for (claim, operation) in [("challenge", "mint"), ("ticket", "register")] {
        let n = Node::start().await;
        let c = n.client(false);
        let owner = Arc::new(Owner::default());
        *owner.token.lock().unwrap() = n.token("desktop").into();
        c.set_auth_provider(owner.clone()).await;
        n.control(json!({"expiredClaim":claim,"losses":{operation:1}}))
            .await;
        assert!(matches!(
            c.login_with_auth_provider("original").await,
            Err(Error::ExchangeOutcomeUnknown)
        ));
        assert!(matches!(
            c.login_with_auth_provider("replacement").await,
            Err(Error::ExchangeOutcomeUnknown)
        ));
        assert!(
            c.pending_registration
                .lock()
                .await
                .as_ref()
                .unwrap()
                .expired,
            "stats={}",
            n.stats().await
        );
        assert_eq!(
            c.snapshot().coordination.reason,
            StateReason::AdmissionExpired
        );
        assert!(!c.snapshot().coordination.retryable);
        let stats = n.stats().await;
        let requests = stats["calls"][operation].as_array().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0], requests[1]);
        // Recovery must not create a fresh domain, even with an available token owner.
        assert!(matches!(
            c.exchange_pending(None, AuthRequestReason::CoordinationRecovery)
                .await,
            Err(Error::ExchangeOutcomeUnknown)
        ));
        assert!(matches!(
            c.login_with_access_token(n.token("stranger"), "different-owner")
                .await,
            Err(Error::ExchangeOutcomeUnknown)
        ));
        assert_eq!(owner.requests.lock().unwrap().len(), 1);
        assert_eq!(
            n.stats().await["calls"][operation],
            stats["calls"][operation]
        );
        assert_eq!(
            c.disable_remote().await.coordinator_revocation,
            CoordinatorRevocation::Pending
        );
        n.stop().await;
    }
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_legacy_no_admission_and_required_capability_failure_never_falls_back() {
    let n = Node::start().await;
    n.control(json!({"mode":"legacy-v1"})).await;
    let c = n.client(false);
    c.login_with_access_token(n.token("desktop"), "legacy")
        .await
        .unwrap();
    let calls = n.stats().await;
    assert!(calls["calls"]["mint"].is_null());
    assert!(calls["calls"]["challenge"].is_null());
    assert_eq!(c.directory(None).await.unwrap().devices.len(), 1);
    c.close().await.unwrap();
    n.stop().await;
    for capability in [
        Value::Null,
        json!({"version":3,"challengeSeconds":120,"ticketSeconds":86400}),
    ] {
        let n = Node::start().await;
        n.control(json!({"configPatch":{"registrationAdmission":capability}}))
            .await;
        let c = n.client(false);
        assert!(c
            .login_with_access_token(n.token("desktop"), "rejected")
            .await
            .is_err());
        assert!(n.stats().await["calls"].as_object().unwrap().is_empty());
        n.stop().await;
    }
}
#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_directory_policy_refresh_failure_reports_unknown_and_never_returns_stale_ready() {
    let n = Node::start().await;
    let c = n.client(false);
    c.login_with_access_token(n.token("desktop"), "owner")
        .await
        .unwrap();
    c.directory(None).await.unwrap();
    n.control(json!({"policy":{"version":1,"providers":[{"id":"new","origins":["https://new.example.test:8443"],"paths":["/"]}]},"configFailure":true})).await;
    assert!(c.directory(None).await.is_err());
    assert_eq!(
        c.snapshot().reachability.reason,
        StateReason::DirectoryUnavailable
    );
    n.control(json!({"configFailure":false})).await;
    let s = c.directory(None).await.unwrap();
    assert_eq!(s.devices.len(), 1);
    c.close().await.unwrap();
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_known_owner_expired_unknown_requires_explicit_fresh_then_retains_pending_close_fact()
{
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    let first = c.login_with_auth_provider("stable").await.unwrap();
    // Only public Client calls: a new actual coordination epoch invalidates the
    // old runtime and starts the real supervisor. No private flags or wallclock.
    n.control(json!({"epoch":true,"expiredClaim":"ticket","losses":{"register":1}}))
        .await;
    assert!(c.directory(None).await.is_err());
    tokio::time::timeout(Duration::from_secs(12), async {
        while c.snapshot().coordination.reason != StateReason::AdmissionExpired {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("supervisor must reach authoritative admission expiry");
    assert!(!c.remote_access_enabled());
    assert_eq!(c.device_session().await.unwrap().user.id, first.user.id);
    let calls = n.stats().await;
    let replay = calls["calls"]["register"].as_array().unwrap();
    assert_eq!(replay.len(), 3); // original login, lost register, exact retry
    assert_eq!(replay[1], replay[2]);
    assert_eq!(
        owner.requests.lock().unwrap().as_slice(),
        &[
            (AuthRequestReason::Connect, false),
            (AuthRequestReason::CoordinationRecovery, false),
        ]
    );
    n.control(json!({"expiredClaim":null})).await;
    let next = tokio::time::timeout(
        Duration::from_secs(3),
        c.login_with_auth_provider("ignored"),
    )
    .await
    .expect("explicit fresh must acquire mutation after expired recovery")
    .unwrap();
    assert_eq!(next.user.id, first.user.id);
    assert_eq!(next.device_id, first.device_id);
    assert_ne!(next.coordinator_epoch, first.coordinator_epoch);
    assert!(!c.remote_access_enabled());
    assert_eq!(
        owner.requests.lock().unwrap().as_slice(),
        &[
            (AuthRequestReason::Connect, false),
            (AuthRequestReason::CoordinationRecovery, false),
            (AuthRequestReason::Connect, true),
        ]
    );
    assert_eq!(
        c.disable_remote().await.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_finite_same_epoch_capability_deletion_is_rejected_without_downgrade_but_new_epoch_can_be_legacy(
) {
    let n = Node::start().await;
    let c = n.client(false);
    n.control(json!({"losses":{"mint":1}})).await;
    assert!(matches!(
        c.login_with_access_token(n.token("desktop"), "stable")
            .await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    let old = c.auth.lock().await.clone().unwrap();
    n.control(json!({"configOmit":["registrationAdmission"]}))
        .await;
    assert!(c.auth_config().await.is_err());
    assert_eq!(*c.auth.lock().await, Some(old));
    let s = c
        .login_with_access_token(n.token("stranger"), "changed")
        .await
        .unwrap();
    assert_eq!(s.user.id, "owner");
    let stats = n.stats().await;
    let calls = stats["calls"]["mint"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    let d = n.client(false);
    n.control(json!({"configOmit":[],"losses":{"register":1}}))
        .await;
    assert!(matches!(
        d.login_with_access_token(n.token("desktop"), "new").await,
        Err(Error::ExchangeOutcomeUnknown)
    ));
    n.control(json!({"mode":"legacy-v1"})).await;
    let next = d
        .login_with_access_token(n.token("desktop"), "ignored")
        .await
        .unwrap();
    assert_eq!(next.user.id, "owner");
    assert!(d
        .auth
        .lock()
        .await
        .as_ref()
        .unwrap()
        .registration_admission
        .is_none());
    d.close().await.unwrap();
    let _ = c.close().await;
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_public_expired_recovery_rejects_fresh_different_subject_and_keeps_pending_close() {
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    let first = c.login_with_auth_provider("verified").await.unwrap();
    n.control(json!({"epoch":true,"expiredClaim":"ticket","losses":{"register":1}}))
        .await;
    assert!(c.directory(None).await.is_err());
    tokio::time::timeout(Duration::from_secs(12), async {
        while c.snapshot().coordination.reason != StateReason::AdmissionExpired {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    n.control(json!({"expiredClaim":null})).await;
    *owner.token.lock().unwrap() = n.token("stranger").into();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        c.login_with_auth_provider("different"),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::IdentityChanged)));
    assert_eq!(c.device_session().await.unwrap().user.id, first.user.id);
    assert!(!c.remote_access_enabled());
    assert_eq!(
        owner.requests.lock().unwrap().last(),
        Some(&(AuthRequestReason::Connect, true))
    );
    assert_eq!(
        c.disable_remote().await.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    n.stop().await;
}

#[tokio::test]
#[ignore = "requires repository Node dependencies and a real Redis binary; run with --include-ignored"]
async fn real_public_known_owner_unknown_401_cannot_enter_expired_fresh_branch() {
    let n = Node::start().await;
    let c = n.client(false);
    let owner = Arc::new(Owner::default());
    *owner.token.lock().unwrap() = n.token("desktop").into();
    c.set_auth_provider(owner.clone()).await;
    c.login_with_auth_provider("verified").await.unwrap();
    n.control(json!({"epoch":true,"losses":{"register":1}}))
        .await;
    assert!(c.directory(None).await.is_err());
    tokio::time::timeout(Duration::from_secs(10), async {
        while n.stats().await["calls"]["register"]
            .as_array()
            .is_none_or(|v| v.len() < 2)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    n.control(json!({"rejectIdentity":true})).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let stats = n.stats().await;
            if stats["outcomes"]["register"]
                .as_array()
                .is_some_and(|v| v.iter().any(|code| code == "INVALID_IDENTITY"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let explicit = tokio::time::timeout(
        Duration::from_millis(100),
        c.login_with_auth_provider("fresh-not-authorized"),
    )
    .await;
    assert!(!matches!(explicit, Ok(Ok(_))));
    assert_ne!(
        c.snapshot().coordination.reason,
        StateReason::AdmissionExpired
    );
    let stats = n.stats().await;
    let calls = stats["calls"]["register"].as_array().unwrap();
    assert!(calls.len() >= 3);
    assert!(calls[1..].iter().all(|h| h == &calls[1]));
    assert_eq!(
        owner.requests.lock().unwrap().as_slice(),
        &[
            (AuthRequestReason::Connect, false),
            (AuthRequestReason::CoordinationRecovery, false),
        ]
    );
    assert!(!c.remote_access_enabled());
    assert_eq!(
        c.disable_remote().await.coordinator_revocation,
        CoordinatorRevocation::Pending
    );
    n.stop().await;
}
