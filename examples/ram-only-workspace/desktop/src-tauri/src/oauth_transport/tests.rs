use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Semaphore,
};
struct Fixture {
    broker: Arc<OAuthTransport>,
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
    count: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
async fn fixture(timeout: Duration, malformed: bool) -> Fixture {
    let cert = rcgen::generate_simple_self_signed(vec!["identity.example.test".into()]).unwrap();
    let server = tokio_rustls::rustls::ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.cert.der().clone()],
        tokio_rustls::rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into()),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let issuer = format!("https://identity.example.test:{}/oidc", address.port());
    let binding = Binding {
        issuer: issuer.clone(),
        endpoint: format!("https://identity.example.test:{}", address.port()),
        client_id: "native".into(),
        resource: "https://resource.example.test".into(),
        scopes: vec!["connect".into()],
        redirect_uri: crate::oauth_callback::REDIRECT.into(),
    };
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(reqwest::Certificate::from_der(cert.cert.der()).unwrap())
        .resolve("identity.example.test", address)
        .timeout(timeout)
        .build()
        .unwrap();
    let broker = OAuthTransport::with_http(binding, http).unwrap();
    let entered = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let count = Arc::new(AtomicUsize::new(0));
    let e = entered.clone();
    let r = release.clone();
    let n = count.clone();
    let tls = tokio_rustls::TlsAcceptor::from(Arc::new(server));
    let task = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let tls = tls.clone();
            let e = e.clone();
            let r = r.clone();
            let n = n.clone();
            let issuer = issuer.clone();
            tokio::spawn(async move {
                let Ok(mut s) = tls.accept(socket).await else {
                    return;
                };
                let mut bytes = Vec::new();
                loop {
                    let mut b = [0; 4096];
                    let v = s.read(&mut b).await.unwrap();
                    if v == 0 {
                        return;
                    };
                    bytes.extend_from_slice(&b[..v]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let text = String::from_utf8_lossy(&bytes[..end]);
                        let len = text
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&bytes);
                let body = if text.starts_with("GET ") {
                    json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/auth"),"token_endpoint":format!("{issuer}/token"),"jwks_uri":format!("{issuer}/jwks")}).to_string()
                } else {
                    n.fetch_add(1, Ordering::SeqCst);
                    e.add_permits(1);
                    let p = r.acquire().await.unwrap();
                    p.forget();
                    if malformed {
                        "{\"refresh_token\":\"minted-rotation\",".into()
                    } else {
                        json!({"refresh_token":"minted-rotation","access_token":"synthetic-access","expires_in":3600,"token_type":"Bearer"}).to_string()
                    }
                };
                let _=s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
            });
        }
    });
    Fixture {
        broker,
        entered,
        release,
        count,
        task,
    }
}
fn code(b: &OAuthTransport) -> FetchRequest {
    FetchRequest {
        url: format!("{}/token", b.binding.issuer),
        method: "POST".into(),
        headers: BTreeMap::from([(
            "content-type".into(),
            "application/x-www-form-urlencoded".into(),
        )]),
        body: Some(
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("client_id", "native")
                .append_pair("grant_type", "authorization_code")
                .append_pair("redirect_uri", &b.binding.redirect_uri)
                .append_pair("code", "synthetic")
                .append_pair("code_verifier", "synthetic-verifier")
                .finish(),
        ),
    }
}
use serde_json::json;
#[tokio::test]
async fn real_tls_cancelled_ipc_waiter_keeps_rotation_owned_and_idempotent() {
    let f = fixture(Duration::from_secs(3), false).await;
    let request = code(&f.broker);
    let b = f.broker.clone();
    let r = request.clone();
    let caller = tokio::spawn(async move { b.fetch(1, r).await });
    let permit = f.entered.acquire().await.unwrap();
    permit.forget();
    caller.abort();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), f.broker.barrier())
            .await
            .is_err()
    );
    f.release.add_permits(1);
    f.broker.barrier().await;
    let reply = f.broker.fetch(1, request.clone()).await.unwrap();
    assert!(reply.body.contains("minted-rotation"));
    assert_eq!(f.count.load(Ordering::SeqCst), 1);
    let mut modified = request;
    modified.body = Some("changed".into());
    assert!(matches!(
        f.broker.fetch(1, modified).await,
        Err("SEQUENCE_CONFLICT")
    ));
    assert!(f.broker.state.lock().await.refresh_hash.is_some());
    f.task.abort();
}
#[tokio::test]
async fn real_tls_unknown_timeout_poison_does_not_resend_old_grant() {
    let f = fixture(Duration::from_millis(80), false).await;
    let request = code(&f.broker);
    assert!(matches!(
        f.broker.fetch(1, request.clone()).await,
        Err("OUTCOME_UNKNOWN")
    ));
    assert!(matches!(
        f.broker.fetch(2, request).await,
        Err("OUTCOME_UNKNOWN")
    ));
    assert_eq!(f.count.load(Ordering::SeqCst), 1);
    f.release.add_permits(1);
    f.task.abort();
}
#[tokio::test]
async fn malformed_minted_reply_is_retained_in_ram_and_poisoned() {
    let f = fixture(Duration::from_secs(1), true).await;
    f.release.add_permits(1);
    assert!(matches!(
        f.broker.fetch(1, code(&f.broker)).await,
        Err("OUTCOME_UNKNOWN")
    ));
    let s = f.broker.state.lock().await;
    assert!(s.poisoned);
    assert!(s
        .received_reply
        .as_ref()
        .unwrap()
        .body
        .contains("minted-rotation"));
    drop(s);
    f.task.abort();
}
