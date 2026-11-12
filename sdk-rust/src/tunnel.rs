use crate::{
    client::{Client, Runtime},
    error::{protocol, transport},
    mux::BoxIo,
    CloudflaredConfig, Device, Error, Peer, Result,
};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::Command,
    sync::mpsc,
};
use tokio_tungstenite::{
    tungstenite::{self, Message},
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;
use url::Url;

pub fn validate_tunnel_url(value: &str) -> Result<()> {
    crate::endpoints::quick_tunnel(value).map(|_| ())
}
pub(crate) fn device_websocket_url(raw: &str) -> Result<String> {
    let u = Url::parse(raw).map_err(|_| protocol("invalid tunnel"))?;
    Ok(format!(
        "wss://{}/connect",
        &u[url::Position::BeforeHost..url::Position::AfterPort]
    ))
}
fn allocated(line: &str) -> Option<String> {
    line.split('|')
        .map(str::trim)
        .find(|p| {
            p.starts_with("https://")
                && *p != "https://api.trycloudflare.com"
                && validate_tunnel_url(p).is_ok()
        })
        .map(str::to_owned)
}
fn ws_io<S>(ws: WebSocketStream<S>, cancel: CancellationToken) -> BoxIo
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (app, worker) = tokio::io::duplex(65536);
    let (mut input, mut output) = tokio::io::split(worker);
    let (mut sink, mut source) = ws.split();
    let (pong_tx, mut pong_rx) = mpsc::channel(8);
    let c = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {_=c.cancelled()=>break,message=source.next()=>{
                match message{Some(Ok(Message::Binary(b)))=>{if b.len()>1<<20{break;}let result=tokio::select!{_=c.cancelled()=>break,r=output.write_all(&b)=>r};if result.is_err(){break;}},Some(Ok(Message::Ping(p)))=>{if pong_tx.send(p).await.is_err(){break;}},Some(Ok(Message::Pong(_)))=>{},_=>break}
            }}
        }
        c.cancel();
        let _ = output.shutdown().await;
    });
    tokio::spawn(async move {
        let mut b = vec![0; 16384];
        loop {
            tokio::select! {_=cancel.cancelled()=>break,p=pong_rx.recv()=>{if let Some(p)=p{if sink.send(Message::Pong(p)).await.is_err(){break;}}},r=input.read(&mut b)=>{
                let n=match r{Ok(0)|Err(_)=>break,Ok(n)=>n};let sent=tokio::select!{_=cancel.cancelled()=>break,r=sink.send(Message::Binary(b[..n].to_vec().into()))=>r};if sent.is_err(){break;}
            }}
        }
        cancel.cancel();
    });
    Box::new(app)
}
impl Client {
    pub(crate) fn spawn_origin(self: &Arc<Self>, listener: TcpListener, rt: Arc<Runtime>) {
        let c = Arc::downgrade(self);
        tokio::spawn(async move {
            let permits = Arc::new(tokio::sync::Semaphore::new(32));
            loop {
                tokio::select! {_=rt.cancel.cancelled()=>break,incoming=listener.accept()=>{
                    let Ok((socket,_))=incoming else{break};let Ok(permit)=permits.clone().try_acquire_owned()else{drop(socket);continue;};let c=c.clone();let cancel=rt.cancel.child_token();
                    tokio::spawn(async move{
                        let _permit=permit;let Some(client)=c.upgrade()else{return};
                        // Tungstenite requires this unboxed HTTP error response in Callback.
                        #[allow(clippy::result_large_err)]
                        let callback=|req:&tungstenite::handshake::server::Request,mut response:tungstenite::handshake::server::Response|{
                            if req.method()!=tungstenite::http::Method::GET||req.uri().path()!="/connect"||req.uri().query().is_some()||req.headers().get("Origin").is_some()||!req.headers().get("Sec-WebSocket-Protocol").and_then(|v|v.to_str().ok()).is_some_and(|v|v.split(',').any(|s|s.trim()=="cybergrid-v1")){
                                return Err(tungstenite::http::Response::builder().status(400).body(Some("invalid CyberGrid WebSocket request".into())).expect("static response"));
                            }
                            response.headers_mut().insert("Sec-WebSocket-Protocol",tungstenite::http::HeaderValue::from_static("cybergrid-v1"));Ok(response)
                        };
                        let ws=tokio::select!{_=cancel.cancelled()=>return,r=tokio::time::timeout(Duration::from_secs(5),tokio_tungstenite::accept_hdr_async_with_config(socket,callback,Some(tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(1<<20)).max_frame_size(Some(1<<20)))))=>match r{Ok(Ok(ws))=>ws,_=>return}};
                        client.event("websocket-upgraded",Some("cloudflare-tunnel"),None,None);
                        let raw=ws_io(ws,cancel.clone());
                        let result=tokio::select!{_=cancel.cancelled()=>return,r=client.secure_server(raw,"cloudflare-tunnel")=>r};
                        if let Err(e)=result{client.report(&e);cancel.cancel();}
                    });
                }}
            }
        });
    }
    pub(crate) async fn dial_tunnel(self: &Arc<Self>, d: &Device) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        let url = d
            .tunnel_url
            .as_deref()
            .ok_or_else(|| protocol("remote tunnel not ready"))?;
        self.validate_endpoint(url, true).await?;
        let endpoint = device_websocket_url(url)?;
        self.dial_ws_endpoint(d, &endpoint, rt.cancel.child_token())
            .await
    }
    pub(crate) async fn dial_ws_endpoint(
        self: &Arc<Self>,
        d: &Device,
        endpoint: &str,
        cancel: CancellationToken,
    ) -> Result<Arc<Peer>> {
        use tungstenite::client::IntoClientRequest;
        let guard = cancel.clone().drop_guard();
        let mut request = endpoint
            .into_client_request()
            .map_err(|_| protocol("invalid WebSocket URL"))?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            tungstenite::http::HeaderValue::from_static("cybergrid-v1"),
        );
        self.event(
            "websocket-dial",
            Some("cloudflare-tunnel"),
            Some(&d.id),
            None,
        );
        #[cfg(test)]
        let connector = self
            .ws_connector
            .read()
            .expect("test connector lock")
            .clone()
            .map(tokio_tungstenite::Connector::Rustls);
        #[cfg(not(test))]
        let connector = None;
        let (ws, response) = tokio::select! {_=cancel.cancelled()=>return Err(Error::Cancelled),r=tokio::time::timeout(Duration::from_secs(10),tokio_tungstenite::connect_async_tls_with_config(request,Some(tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(1<<20)).max_frame_size(Some(1<<20))),true,connector))=>r.map_err(|_|Error::Timeout("WebSocket connect"))?.map_err(|_|transport("WebSocket upgrade failed"))?};
        if response
            .headers()
            .get("Sec-WebSocket-Protocol")
            .and_then(|v| v.to_str().ok())
            != Some("cybergrid-v1")
        {
            cancel.cancel();
            return Err(protocol("WebSocket subprotocol mismatch"));
        }
        let raw = ws_io(ws, cancel.clone());
        self.event(
            "websocket-upgraded",
            Some("cloudflare-tunnel"),
            Some(&d.id),
            None,
        );
        let result = tokio::select! {_=cancel.cancelled()=>Err(Error::Cancelled),r=self.secure_client(raw,d,"cloudflare-tunnel")=>r};
        if result.is_ok() {
            guard.disarm();
        }
        result
    }
    pub(crate) fn spawn_provider(
        self: &Arc<Self>,
        p: CloudflaredConfig,
        rt: Arc<Runtime>,
    ) -> tokio::task::JoinHandle<()> {
        let c = Arc::downgrade(self);
        tokio::spawn(async move {
            for attempt in 0..5u32 {
                if rt.cancel.is_cancelled() {
                    return;
                }
                let result = run_provider(
                    p.clone(),
                    format!("http://{}", rt.origin_addr),
                    rt.cancel.clone(),
                    c.clone(),
                    rt.clone(),
                )
                .await;
                *rt.tunnel_url.write().await = None;
                let Some(client) = c.upgrade() else { return };
                if rt.cancel.is_cancelled() {
                    return;
                }
                if let Err(e) = result {
                    client.report(&e);
                }
                if let Err(e) = client.heartbeat().await {
                    client.report(&e);
                }
                if attempt == 4 {
                    client.event(
                        "provider-failed",
                        Some("cloudflare-tunnel"),
                        None,
                        Some("provider restart budget exhausted".into()),
                    );
                    return;
                }
                client.event(
                    "provider-restarting",
                    Some("cloudflare-tunnel"),
                    None,
                    Some(format!("restart attempt {} of 4", attempt + 1)),
                );
                tokio::select! {_=rt.cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_secs(1u64<<attempt))=>{}}
            }
        })
    }
}
async fn run_provider(
    mut p: CloudflaredConfig,
    origin: String,
    cancel: CancellationToken,
    client: std::sync::Weak<Client>,
    rt: Arc<Runtime>,
) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(read) = p.env_provider.as_ref() {
        p.env
            .extend(read().map_err(|_| transport("provider environment unavailable"))?);
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let dir = if let Some(path) = p.temp_dir {
        std::fs::create_dir_all(&path)?;
        tempfile::Builder::new()
            .prefix("cybergrid-tunnel-")
            .tempdir_in(path)?
    } else {
        tempfile::Builder::new()
            .prefix("cybergrid-tunnel-")
            .tempdir()?
    };
    let config = dir.path().join("config.yml");
    std::fs::write(&config, b"{}\n")?;
    let executable = crate::provider::resolve(&p.source, &p.binary)?;
    let mut cmd = Command::new(executable);
    cmd.args(["tunnel", "--config"]).arg(config).args([
        "--no-autoupdate",
        "--url",
        &origin,
        "--protocol",
        "http2",
        "--metrics",
        "127.0.0.1:0",
    ]);
    cmd.current_dir(dir.path()).env_clear();
    const SAFE: &[&str] = &[
        "PATH",
        "HOME",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "ANDROID_ROOT",
        "ANDROID_DATA",
        "LD_LIBRARY_PATH",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "CLOUDFLARED_DNS_SERVER",
    ];
    for key in SAFE {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    for (k, v) in p.env {
        if !SAFE.contains(&k.as_str()) {
            return Err(protocol("provider environment key is not allowed"));
        }
        cmd.env(k, v);
    }
    cmd.kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|_| transport("Quick Tunnel provider missing or not executable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| transport("provider output unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| transport("provider error output unavailable"))?;
    let (tx, mut rx) = mpsc::channel::<String>(32);
    for output in [
        Box::new(stdout) as Box<dyn AsyncRead + Send + Unpin>,
        Box::new(stderr),
    ] {
        let tx = tx.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            // A provider cannot consume unbounded log memory or leak raw logs.
            let mut reader = BufReader::new(output.take(1 << 20));
            loop {
                let mut line = String::new();
                let result =
                    tokio::select! {_=cancel.cancelled()=>break,r=reader.read_line(&mut line)=>r};
                match result {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(line).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    drop(tx);
    let startup = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(startup);
    let mut url = None;
    let mut registered = false;
    let mut published = false;
    let result = loop {
        tokio::select! {
                    _=cancel.cancelled()=>break Ok(()),
                    status=child.wait()=>break Err(transport(format!("Quick Tunnel provider exited (success={})",status.is_ok_and(|s|s.success())))),
                    _=&mut startup,if !published=>break Err(Error::Timeout("Quick Tunnel registration (30 seconds)")),
                    line=rx.recv()=>{
                        let Some(line)=line else{break Err(transport("provider log stream closed before exit"));};
                        if let Some(u)=allocated(line.trim()){url=Some(u);}
                        if line.contains("Registered tunnel connection"){registered=true;}
                        if !published&&registered&&url.is_some()&&!cancel.is_cancelled(){
                            *rt.tunnel_url.write().await=url.take();published=true;
                            if let Some(c)=client.upgrade(){if rt.cancel.is_cancelled()||c.generation()!=rt.generation{continue;}
        if let Err(e)=c.heartbeat().await{c.report(&e);}
        if !rt.cancel.is_cancelled()&&c.generation()==rt.generation{c.event("tunnel-ready",Some("cloudflare-tunnel"),None,None);}}
                        }
                    }
                }
    };
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    result
}
