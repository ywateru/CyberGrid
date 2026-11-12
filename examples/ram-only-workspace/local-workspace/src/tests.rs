use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message as Wire},
};
async fn login(local: &LocalWorkspace, http: &reqwest::Client) -> (String, String) {
    let token = local
        .open_url()
        .await
        .unwrap()
        .split('#')
        .nth(1)
        .unwrap()
        .to_owned();
    let r = http
        .post(format!("{}/exchange", local.origin()))
        .header("origin", local.origin())
        .json(&json!({"token":token}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let cookie = r.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let r = http
        .post(format!("{}/bootstrap", local.origin()))
        .header("origin", local.origin())
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    let csrf = r.json::<Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}
#[tokio::test]
async fn real_http_one_time_expiry_host_origin_csrf_and_instance_boundary() {
    let mut a = LocalWorkspace::start(Workspace::new()).await.unwrap();
    let mut b = LocalWorkspace::start(Workspace::new()).await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(!http
        .get(a.origin())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("csrf"));
    let expired = a.open_url_for(Duration::ZERO).await.unwrap();
    let expired = expired.split('#').nth(1).unwrap();
    assert_eq!(
        http.post(format!("{}/exchange", a.origin()))
            .header("origin", a.origin())
            .json(&json!({"token":expired}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let launch = a.open_url().await.unwrap();
    let token = launch.split('#').nth(1).unwrap();
    for origin in [None, Some("http://evil.test"), Some(b.origin())] {
        let mut r = http
            .post(format!("{}/exchange", a.origin()))
            .json(&json!({"token":token}));
        if let Some(o) = origin {
            r = r.header("origin", o)
        };
        assert_eq!(r.send().await.unwrap().status(), StatusCode::FORBIDDEN)
    }
    let r = http
        .post(format!("{}/exchange", a.origin()))
        .header("origin", a.origin())
        .json(&json!({"token":token}))
        .send()
        .await
        .unwrap();
    let set = r.headers()["set-cookie"].to_str().unwrap();
    assert!(
        set.contains("HttpOnly") && set.contains("SameSite=Strict") && !set.contains("Domain=")
    );
    assert_eq!(
        http.post(format!("{}/exchange", a.origin()))
            .header("origin", a.origin())
            .json(&json!({"token":token}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let (cookie, csrf) = login(&a, &http).await;
    for (host, origin, forwards, proof) in [
        (
            Some("localhost:1"),
            Some(a.origin()),
            false,
            Some(csrf.as_str()),
        ),
        (None, None, false, Some(csrf.as_str())),
        (None, Some(b.origin()), false, Some(csrf.as_str())),
        (None, Some(a.origin()), true, Some(csrf.as_str())),
        (None, Some(a.origin()), false, None),
    ] {
        let mut r = http
            .post(format!("{}/state", a.origin()))
            .header("cookie", &cookie);
        if let Some(h) = host {
            r = r.header("host", h)
        }
        if let Some(o) = origin {
            r = r.header("origin", o)
        }
        if forwards {
            r = r.header("x-forwarded-host", "evil.test")
        }
        if let Some(v) = proof {
            r = r.header("x-csrf-token", v)
        }
        assert_eq!(r.send().await.unwrap().status(), StatusCode::FORBIDDEN);
    }
    assert_eq!(
        http.post(format!("{}/bootstrap", b.origin()))
            .header("origin", b.origin())
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let value = http
        .post(format!("{}/state", a.origin()))
        .header("origin", a.origin())
        .header("cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(value["summary"]["account"].is_null());
    assert_eq!(
        http.get(format!("{}/state", a.origin()))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    a.close().await;
    b.close().await;
}
#[tokio::test]
async fn real_ws_requires_cookie_origin_and_first_frame_csrf() {
    let mut a = LocalWorkspace::start(Workspace::new()).await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let (cookie, csrf) = login(&a, &http).await;
    let url = a.origin().replace("http:", "ws:") + "/ws";
    assert!(connect_async(&url).await.is_err());
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", a.origin().parse().unwrap());
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut wrong, _) = connect_async(request.clone()).await.unwrap();
    wrong
        .send(Wire::Text("{\"csrf\":\"wrong\"}".into()))
        .await
        .unwrap();
    assert!(matches!(
        wrong.next().await,
        Some(Ok(Wire::Close(_))) | None
    ));
    let (mut ws, _) = connect_async(request).await.unwrap();
    ws.send(Wire::Text(json!({"csrf":csrf}).to_string().into()))
        .await
        .unwrap();
    ws.send(Wire::Text("local-before".into())).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Wire::Text("local-before".into())
    );
    ws.close(None).await.unwrap();
    a.close().await;
}
