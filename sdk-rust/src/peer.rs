use crate::{
    client::Client,
    error::{protocol, transport},
    mux::{framed_stream, Mux},
    Error, Result,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

pub struct Peer {
    pub device_id: String,
    generation: u64,
    pub transport: String,
    pub(crate) mux: Arc<Mux>,
    client: std::sync::Weak<Client>,
    pub(crate) rtc: Mutex<Option<Arc<crate::rtc::Attempt>>>,
}
impl Client {
    pub(crate) async fn remember_peer(
        self: &Arc<Self>,
        id: String,
        path: String,
        mux: Arc<Mux>,
    ) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        let peer = Arc::new(Peer {
            generation: rt.generation,
            device_id: id,
            transport: path,
            mux,
            client: Arc::downgrade(self),
            rtc: Mutex::new(None),
        });
        {
            let mut peers = rt.peers.lock().await;
            peers.retain(|p| !p.mux.cancel.is_cancelled());
            if peers.len() >= 64 {
                peer.close().await;
                return Err(protocol("maximum 64 peers"));
            }
            peers.push(peer.clone());
        }
        self.event(
            "multiplexer-ready",
            Some(&peer.transport),
            Some(&peer.device_id),
            None,
        );
        let p = peer.clone();
        tokio::spawn(async move {
            p.serve().await;
            p.close().await;
            if let Some(c) = p.client.upgrade() {
                if c.generation() == p.generation {
                    c.event("mapping-disconnected", None, Some(&p.device_id), None);
                }
            }
        });
        Ok(peer)
    }
    pub async fn connect(self: &Arc<Self>, id: &str) -> Result<Arc<Peer>> {
        let d = self.device(id).await?;
        if self
            .device_session()
            .await
            .is_some_and(|s| s.device_id == id)
        {
            return Err(protocol("cannot connect to self"));
        }
        self.event("direct-attempt", Some("webrtc-direct"), Some(id), None);
        let direct = self.dial_direct(&d).await;
        if let Ok(p) = direct {
            return Ok(p);
        }
        if self.active().await?.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.event(
            "fallback-attempt",
            Some("cloudflare-tunnel"),
            Some(id),
            None,
        );
        tokio::time::timeout(self.cfg.fallback_timeout,async{
            loop{
                let current=self.device(id).await?;
                if current.tunnel_url.is_some(){return self.dial_tunnel(&current).await;}
                let rt=self.active().await?;tokio::select!{_=rt.cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
            }
        }).await.map_err(|_|Error::Timeout("remote tunnel readiness or authentication"))?
    }
    pub async fn map(
        self: &Arc<Self>,
        device: &str,
        port: &str,
        local: SocketAddr,
    ) -> Result<Mapping> {
        if !local.ip().is_loopback() {
            return Err(protocol("mapping must listen on loopback"));
        }
        let peer = self.connect(device).await?;
        let probe = match peer.dial(port).await {
            Ok(p) => p,
            Err(e) => {
                peer.close().await;
                return Err(e);
            }
        };
        drop(probe);
        let listener = match TcpListener::bind(local).await {
            Ok(l) => l,
            Err(e) => {
                peer.close().await;
                return Err(e.into());
            }
        };
        let addr = listener.local_addr()?;
        let cancel = self.mapping_cancel();
        let task_cancel = cancel.clone();
        let current = Arc::new(Mutex::new(Some(peer.clone())));
        let slot = current.clone();
        let client = Arc::downgrade(self);
        let device = device.to_owned();
        let port = port.to_owned();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {_=task_cancel.cancelled()=>break,accepted=listener.accept()=>{
                    let Ok((mut local,_))=accepted else{break};
                    let client=client.clone();let slot=slot.clone();let device=device.clone();let port=port.clone();let cancel=task_cancel.clone();
                    tokio::spawn(async move {
                        let Some(c)=client.upgrade()else{return};
                        // Serialize replacement per mapping. The listener survives a
                        // coordinator reset; existing flows are never replayed.
                        let peer={
                            let mut current=slot.lock().await;
                            if current.as_ref().is_none_or(|p|p.is_closed()) {
                                c.event("mapping-disconnected",None,Some(&device),None);
                                *current=None;
                                let result=tokio::select!{_=cancel.cancelled()=>return,r=tokio::time::timeout((c.cfg.direct_timeout+c.cfg.fallback_timeout+Duration::from_secs(2)).min(Duration::from_secs(90)),c.connect(&device))=>r};
                                if let Ok(Ok(p))=result {c.event("mapping-ready",Some(&p.transport),Some(&device),None);*current=Some(p);}
                            }
                            current.clone()
                        };
                        if let Some(p)=peer {
                            if let Ok(mut remote)=p.dial(&port).await {tokio::select!{_=cancel.cancelled()=>{},_=tokio::io::copy_bidirectional(&mut local,&mut remote)=>{}}}
                        }
                    });
                }}
            }
        });
        Ok(Mapping {
            addr,
            peer,
            current,
            cancel,
            task: Mutex::new(Some(task)),
        })
    }
}
impl Peer {
    pub async fn dial(&self, port: &str) -> Result<DuplexStream> {
        if port.is_empty() || port.len() > 64 {
            return Err(protocol("invalid port ID"));
        }
        let mut raw = self.mux.open().await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            raw.write_u16(port.len() as u16).await?;
            raw.write_all(port.as_bytes()).await?;
            if raw.read_u8().await? != 0 {
                return Err(protocol("remote port unavailable"));
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::Timeout("remote port open"))??;
        if let Some(c) = self.client.upgrade() {
            c.event(
                "port-open",
                Some(&self.transport),
                Some(&self.device_id),
                None,
            );
        }
        Ok(framed_stream(raw, self.mux.cancel.child_token()))
    }
    async fn serve(self: &Arc<Self>) {
        loop {
            let mut raw = match self.mux.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let Some(c) = self.client.upgrade() else {
                break;
            };
            let cancel = self.mux.cancel.child_token();
            tokio::spawn(async move {
                let result: Result<TcpStream> =
                    tokio::time::timeout(Duration::from_secs(10), async {
                        let n = raw.read_u16().await? as usize;
                        if n == 0 || n > 64 {
                            return Err(protocol("invalid port selection"));
                        }
                        let mut b = vec![0; n];
                        raw.read_exact(&mut b).await?;
                        let id =
                            std::str::from_utf8(&b).map_err(|_| protocol("invalid port UTF8"))?;
                        c.active().await?;
                        let port = c.ports.read().await.get(id).cloned();
                        if let Some(port) = port {
                            if let Ok(Ok(stream)) = tokio::time::timeout(
                                Duration::from_secs(5),
                                TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port.port)),
                            )
                            .await
                            {
                                raw.write_u8(0).await?;
                                return Ok(stream);
                            }
                        }
                        raw.write_u8(1).await?;
                        Err(transport("requested port not exposed or unavailable"))
                    })
                    .await
                    .map_err(|_| Error::Timeout("port selection"))
                    .and_then(|r| r);
                if let Ok(mut target) = result {
                    let mut conn = framed_stream(raw, cancel.child_token());
                    tokio::select! {_=cancel.cancelled()=>{},_=tokio::io::copy_bidirectional(&mut conn,&mut target)=>{}}
                }
            });
        }
    }
    pub async fn close(&self) {
        self.mux.cancel.cancel();
        if let Some(a) = self.rtc.lock().await.take() {
            a.close().await;
        }
    }
    pub fn is_closed(&self) -> bool {
        self.mux.cancel.is_cancelled()
    }
}
pub struct Mapping {
    pub addr: SocketAddr,
    /// Initial peer retained for source compatibility. Use current_transport for live state.
    pub peer: Arc<Peer>,
    current: Arc<Mutex<Option<Arc<Peer>>>>,
    cancel: CancellationToken,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Mapping {
    pub fn transport(&self) -> &str {
        &self.peer.transport
    }
    pub async fn current_transport(&self) -> Option<String> {
        self.current
            .lock()
            .await
            .as_ref()
            .filter(|p| !p.is_closed())
            .map(|p| p.transport.clone())
    }
    pub async fn close(&self) {
        self.cancel.cancel();
        if let Some(peer) = self.current.lock().await.take() {
            peer.close().await;
        }
        self.peer.close().await;
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.peer.mux.cancel.cancel();
        if let Ok(current) = self.current.try_lock() {
            if let Some(peer) = current.as_ref() {
                peer.mux.cancel.cancel();
            }
        }
    }
}
