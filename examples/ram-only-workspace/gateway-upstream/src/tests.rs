use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        protocol::frame::{
            coding::{Data, OpCode},
            Frame,
        },
        Message as Wire,
    },
};
#[tokio::test]
async fn actual_gateway_bearer_http_ws_isolated_from_browser_and_close_preserves_local_ws() {
    let workspace = Workspace::new();
    let mut local = local_workspace::LocalWorkspace::start(workspace.clone())
        .await
        .unwrap();
    let mut upstream = GatewayUpstream::start(workspace).await.unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let origin = format!("http://127.0.0.1:{}", upstream.port);
    assert_eq!(
        http.get(format!("{origin}/api/info"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    for (key, value) in [
        ("origin", local.origin()),
        ("cookie", "irrelevant-cookie"),
        ("host", "localhost:1"),
    ] {
        assert_eq!(
            http.get(format!("{origin}/api/info"))
                .bearer_auth(upstream.local_token())
                .header(key, value)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        )
    }
    let read = http
        .get(format!("{origin}/api/info"))
        .bearer_auth(upstream.local_token())
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(
        http.post(format!("{origin}/api/sessions"))
            .bearer_auth(upstream.local_token())
            .json(&json!({"value":"synthetic"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        http.post(format!("{}/bootstrap", local.origin()))
            .header("origin", local.origin())
            .bearer_auth(upstream.local_token())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let launch = local.open_url().await.unwrap();
    let token = launch.split('#').nth(1).unwrap();
    let r = http
        .post(format!("{}/exchange", local.origin()))
        .header("origin", local.origin())
        .json(&json!({"token":token}))
        .send()
        .await
        .unwrap();
    let cookie = r.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let body = http
        .post(format!("{}/bootstrap", local.origin()))
        .header("origin", local.origin())
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let csrf = body["csrf"].as_str().unwrap();
    let mut req = (local.origin().replace("http:", "ws:") + "/ws")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("origin", local.origin().parse().unwrap());
    req.headers_mut().insert("cookie", cookie.parse().unwrap());
    let (mut browser, _) = connect_async(req).await.unwrap();
    browser
        .send(Wire::Text(json!({"csrf":csrf}).to_string().into()))
        .await
        .unwrap();
    let url = format!("ws://127.0.0.1:{}/ws/terminal/synthetic", upstream.port);
    let (mut ws, _) = connect_async(&url).await.unwrap();
    ws.send(Wire::Text(
        json!({"token":upstream.local_token()}).to_string().into(),
    ))
    .await
    .unwrap();
    ws.send(Wire::Text(
        json!({"type":"input","data":"duplex"}).to_string().into(),
    ))
    .await
    .unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Wire::Binary(b"duplex".to_vec().into())
    );
    let (mut unauthed, _) = connect_async(&url).await.unwrap();
    unauthed.send(Wire::Text("not-token".into())).await.unwrap();
    assert!(matches!(
        unauthed.next().await,
        Some(Ok(Wire::Close(_))) | None
    ));
    upstream.seal();
    let eof = tokio::time::timeout(Duration::from_secs(1), ws.next())
        .await
        .unwrap();
    assert!(matches!(eof, Some(Ok(Wire::Close(_))) | None));
    assert!(connect_async(&url).await.is_err());
    browser
        .send(Wire::Text(
            "same-local-connection-after-remote-close".into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        browser.next().await.unwrap().unwrap(),
        Wire::Text("same-local-connection-after-remote-close".into())
    );
    browser.close(None).await.unwrap();
    upstream.close().await;
    local.close().await;
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn authenticated(upstream: &GatewayUpstream) -> Socket {
    let (mut socket, _) = connect_async(format!(
        "ws://127.0.0.1:{}/ws/terminal/synthetic",
        upstream.port
    ))
    .await
    .unwrap();
    socket
        .send(Wire::Text(
            json!({"token":upstream.local_token()}).to_string().into(),
        ))
        .await
        .unwrap();
    socket
}

async fn assert_closed(socket: &mut Socket) {
    let next = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("invalid remote input must close within the budget");
    assert!(matches!(
        next,
        Some(Ok(Wire::Close(_))) | Some(Err(_)) | None
    ));
}

#[tokio::test]
async fn terminal_echo_returns_only_data_bytes_and_handles_multiple_utf8_inputs_and_ping() {
    let mut upstream = GatewayUpstream::start(Workspace::new()).await.unwrap();
    let mut socket = authenticated(&upstream).await;
    for data in [
        "CyberGrid 合成回声🙂",
        "line\n\"quoted\"\\slash\t",
        "",
        "next",
    ] {
        socket
            .send(Wire::Text(
                json!({"type":"input","data":data}).to_string().into(),
            ))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            Wire::Binary(data.as_bytes().to_vec().into())
        );
    }
    socket
        .send(Wire::Ping(b"alive".to_vec().into()))
        .await
        .unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap(),
        Wire::Pong(b"alive".to_vec().into())
    );
    socket.close(None).await.unwrap();
    upstream.close().await;
}

#[tokio::test]
async fn terminal_rejects_invalid_envelopes_without_closing_an_independent_remote_stream() {
    let mut upstream = GatewayUpstream::start(Workspace::new()).await.unwrap();
    let mut independent = authenticated(&upstream).await;
    let invalid = [
        "plain-text",
        "null",
        "[]",
        r#"{"data":"missing type"}"#,
        r#"{"type":"input"}"#,
        r#"{"type":"resize","cols":80,"rows":24}"#,
        r#"{"type":{"input":null},"data":"must-not-echo"}"#,
        r#"{"type":{},"data":"must-not-echo"}"#,
        r#"{"type":["input"],"data":"must-not-echo"}"#,
        r#"{"type":1,"data":"must-not-echo"}"#,
        r#"{"type":true,"data":"must-not-echo"}"#,
        r#"{"type":null,"data":"must-not-echo"}"#,
        r#"{"type":"Input","data":"must-not-echo"}"#,
        r#"{"type":"input ","data":"must-not-echo"}"#,
        r#"{"type":"input","data":null}"#,
        r#"{"type":"input","data":12}"#,
        r#"{"type":"input","data":{}}"#,
        r#"{"type":"input","data":[]}"#,
        r#"{"type":"input","data":"x","extra":true}"#,
        r#"{"type":"input","type":"input","data":"x"}"#,
        r#"{"type":"input","data":"x","data":"y"}"#,
    ];
    for message in invalid {
        let mut socket = authenticated(&upstream).await;
        socket.send(Wire::Text(message.into())).await.unwrap();
        assert_closed(&mut socket).await;
    }
    let mut binary = authenticated(&upstream).await;
    binary
        .send(Wire::Binary(
            br#"{"type":"input","data":"x"}"#.to_vec().into(),
        ))
        .await
        .unwrap();
    assert_closed(&mut binary).await;
    independent
        .send(Wire::Text(
            json!({"type":"input","data":"still usable"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        independent.next().await.unwrap().unwrap(),
        Wire::Binary(b"still usable".to_vec().into())
    );
    independent.close(None).await.unwrap();
    upstream.close().await;
}

#[tokio::test]
async fn terminal_enforces_the_existing_4096_byte_wire_message_limit() {
    let mut upstream = GatewayUpstream::start(Workspace::new()).await.unwrap();
    let overhead = json!({"type":"input","data":""}).to_string().len();
    let data = "x".repeat(4096 - overhead);
    let envelope = json!({"type":"input","data":data}).to_string();
    assert_eq!(envelope.len(), 4096);
    let mut socket = authenticated(&upstream).await;
    socket.send(Wire::Text(envelope.into())).await.unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap(),
        Wire::Binary(data.into_bytes().into())
    );
    socket
        .send(Wire::Text(
            json!({"type":"input","data":"x".repeat(4097-overhead)})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_closed(&mut socket).await;
    upstream.close().await;
}

#[tokio::test]
async fn terminal_reassembles_split_utf8_but_rejects_an_oversized_fragmented_message() {
    let mut upstream = GatewayUpstream::start(Workspace::new()).await.unwrap();
    let mut socket = authenticated(&upstream).await;
    let data = "合成🙂";
    let envelope = json!({"type":"input","data":data}).to_string().into_bytes();
    let split = envelope.iter().position(|byte| *byte >= 0x80).unwrap() + 1;
    for (bytes, opcode, final_frame) in [
        (&envelope[..split], Data::Text, false),
        (&envelope[split..], Data::Continue, true),
    ] {
        socket
            .send(Wire::Frame(Frame::message(
                bytes.to_vec(),
                OpCode::Data(opcode),
                final_frame,
            )))
            .await
            .unwrap();
    }
    assert_eq!(
        socket.next().await.unwrap().unwrap(),
        Wire::Binary(data.as_bytes().to_vec().into())
    );
    let overhead = json!({"type":"input","data":""}).to_string().len();
    let oversized = json!({"type":"input","data":"x".repeat(4097-overhead)})
        .to_string()
        .into_bytes();
    assert_eq!(oversized.len(), 4097);
    for (bytes, opcode, final_frame) in [
        (&oversized[..2048], Data::Text, false),
        (&oversized[2048..], Data::Continue, true),
    ] {
        socket
            .send(Wire::Frame(Frame::message(
                bytes.to_vec(),
                OpCode::Data(opcode),
                final_frame,
            )))
            .await
            .unwrap();
    }
    assert_closed(&mut socket).await;
    upstream.close().await;
}
