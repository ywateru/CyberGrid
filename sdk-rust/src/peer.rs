use crate::{
    client::Client,
    error::{protocol, transport},
    mux::{framed_stream, Mux},
    DirectStrategy, Error, Result,
};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

pub(crate) struct DialFence {
    pub cancel: CancellationToken,
    pub generation: u64,
    winner: Option<(Arc<AtomicU8>, u8)>,
}
// A fully authenticated winner must remain owned while the losing attempt is
// being joined. Dropping an in-flight connect also closes that winner.
struct PendingPeer(Option<Arc<Peer>>);
impl PendingPeer {
    fn release(mut self) -> Arc<Peer> {
        self.0.take().unwrap()
    }
}
impl Drop for PendingPeer {
    fn drop(&mut self) {
        if let Some(peer) = self.0.take() {
            peer.mux.cancel.cancel();
            tokio::spawn(async move {
                peer.close().await;
            });
        }
    }
}
impl DialFence {
    pub fn single(cancel: CancellationToken, generation: u64) -> Self {
        Self {
            cancel,
            generation,
            winner: None,
        }
    }
    fn competing(
        cancel: CancellationToken,
        generation: u64,
        winner: Arc<AtomicU8>,
        path: u8,
    ) -> Self {
        Self {
            cancel,
            generation,
            winner: Some((winner, path)),
        }
    }
    pub fn claim(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some((winner, path)) = &self.winner {
            match winner.compare_exchange(0, *path, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {}
                Err(value) if value == *path => {}
                _ => return Err(Error::Cancelled),
            }
        }
        Ok(())
    }
}

pub struct Peer {
    pub device_id: String,
    generation: u64,
    pub transport: String,
    pub(crate) mux: Arc<Mux>,
    client: std::sync::Weak<Client>,
    pub(crate) rtc: Mutex<Option<Arc<crate::rtc::Attempt>>>,
    pub(crate) iroh: Mutex<Option<iroh::endpoint::Connection>>,
}
impl Client {
    pub(crate) async fn remember_peer(
        self: &Arc<Self>,
        id: String,
        path: String,
        mux: Arc<Mux>,
        generation: u64,
        fence: Option<&DialFence>,
    ) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        if rt.generation != generation
            || rt.cancel.is_cancelled()
            || fence.is_some_and(|f| f.cancel.is_cancelled())
        {
            mux.cancel.cancel();
            return Err(Error::Cancelled);
        }
        let peer = Arc::new(Peer {
            generation: rt.generation,
            device_id: id,
            transport: path,
            mux,
            client: Arc::downgrade(self),
            rtc: Mutex::new(None),
            iroh: Mutex::new(None),
        });
        {
            let mut peers = rt.peers.lock().await;
            if self.generation() != generation
                || rt.cancel.is_cancelled()
                || fence.is_some_and(|f| f.claim().is_err())
            {
                peer.close().await;
                return Err(Error::Cancelled);
            }
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
        self.connect_with_cancel(id, self.closed_token().child_token())
            .await
    }
    async fn connect_with_cancel(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Arc<Peer>> {
        let _guard = cancel.clone().drop_guard();
        let operation = self.connect_chain(id, cancel.clone());
        tokio::pin!(operation);
        tokio::select! {
            result = &mut operation => {
                if cancel.is_cancelled() {
                    if let Ok(peer) = result { peer.close().await; }
                    Err(Error::Cancelled)
                } else { result }
            },
            _ = tokio::time::sleep(self.cfg.connection_budget()) => {
                cancel.cancel();
                // Both direct futures observe cancellation and finish their owned
                // cleanup before the public operation returns a deadline error.
                if let Ok(peer) = operation.await { peer.close().await; }
                Err(Error::Timeout("peer connection budget"))
            }
        }
    }
    async fn connect_chain(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        let d = tokio::select! { _ = cancel.cancelled() => return Err(Error::Cancelled), result = self.device(id) => result? };
        if self
            .device_session()
            .await
            .is_some_and(|s| s.device_id == id)
        {
            return Err(protocol("cannot connect to self"));
        }
        let has_iroh = rt.iroh.is_some()
            && d.iroh
                .as_ref()
                .is_some_and(|info| !info.addresses.is_empty());
        self.event("direct-attempt", Some("webrtc-direct"), Some(id), None);
        let direct = if self.cfg.direct_strategy == DirectStrategy::Parallel && has_iroh {
            self.event("iroh-attempt", Some("iroh-direct"), Some(id), None);
            let winner = Arc::new(AtomicU8::new(0));
            let rtc_fence =
                DialFence::competing(cancel.child_token(), rt.generation, winner.clone(), 1);
            let iroh_fence = DialFence::competing(cancel.child_token(), rt.generation, winner, 2);
            let rtc = self.dial_direct_fenced(&d, &rtc_fence);
            let iroh = self.dial_iroh_fenced(&d, &iroh_fence);
            tokio::pin!(rtc, iroh);
            let (first_rtc, first) = tokio::select! { result = &mut rtc => (true, result), result = &mut iroh => (false, result) };
            match first {
                Ok(peer) => {
                    let pending = PendingPeer(Some(peer));
                    let loser = if first_rtc {
                        iroh_fence.cancel.cancel();
                        iroh.await
                    } else {
                        rtc_fence.cancel.cancel();
                        rtc.await
                    };
                    if let Ok(loser) = loser {
                        loser.close().await;
                    }
                    Ok(pending.release())
                }
                Err(_) => {
                    if first_rtc {
                        iroh.await
                    } else {
                        rtc.await
                    }
                }
            }
        } else {
            let rtc_fence = DialFence::single(cancel.child_token(), rt.generation);
            match self.dial_direct_fenced(&d, &rtc_fence).await {
                Ok(peer) => Ok(peer),
                Err(error) => {
                    if cancel.is_cancelled()
                        || rt.cancel.is_cancelled()
                        || self.generation() != rt.generation
                    {
                        return Err(Error::Cancelled);
                    }
                    if has_iroh {
                        self.event("iroh-attempt", Some("iroh-direct"), Some(id), None);
                        self.dial_iroh_fenced(
                            &d,
                            &DialFence::single(cancel.child_token(), rt.generation),
                        )
                        .await
                    } else {
                        self.event("iroh-skipped", Some("iroh-direct"), Some(id), None);
                        Err(error)
                    }
                }
            }
        };
        if let Ok(peer) = direct {
            if cancel.is_cancelled()
                || rt.cancel.is_cancelled()
                || self.generation() != rt.generation
            {
                peer.close().await;
                return Err(Error::Cancelled);
            }
            return Ok(peer);
        }
        if cancel.is_cancelled() || rt.cancel.is_cancelled() || self.generation() != rt.generation {
            return Err(Error::Cancelled);
        }
        self.event(
            "fallback-attempt",
            Some("cloudflare-tunnel"),
            Some(id),
            None,
        );
        tokio::select! { _ = cancel.cancelled() => Err(Error::Cancelled), result = tokio::time::timeout(self.cfg.fallback_timeout, async {
            loop {
                let current = self.device(id).await?;
                if self.generation() != rt.generation || rt.cancel.is_cancelled() { return Err(Error::Cancelled); }
                if current.tunnel_url.is_some() { return self.dial_tunnel(&current).await; }
                tokio::select! { _ = rt.cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(Duration::from_millis(500)) => {} }
            }
        }) => result.map_err(|_| Error::Timeout("remote tunnel readiness or authentication"))? }
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
            let mut work = tokio::task::JoinSet::new();
            loop {
                tokio::select! {_=task_cancel.cancelled()=>break,_=work.join_next(),if !work.is_empty()=>{},accepted=listener.accept()=>{
                    let Ok((mut local,_))=accepted else{break};
                    let client=client.clone();let slot=slot.clone();let device=device.clone();let port=port.clone();let cancel=task_cancel.clone();
                    work.spawn(async move {
                        let Some(c)=client.upgrade()else{return};
                        // Serialize replacement per mapping. The listener survives a
                        // coordinator reset; existing flows are never replayed.
                        let peer={
                            let mut current=slot.lock().await;
                            if current.as_ref().is_none_or(|p|p.is_closed()) {
                                c.event("mapping-disconnected",None,Some(&device),None);
                                *current=None;
                                let result=c.connect_with_cancel(&device,cancel.child_token()).await;
                                if let Ok(p)=result {c.event("mapping-ready",Some(&p.transport),Some(&device),None);*current=Some(p);}
                            }
                            current.clone()
                        };
                        if let Some(p)=peer {
                            if let Ok(mut remote)=p.dial(&port).await {tokio::select!{_=cancel.cancelled()=>{},_=tokio::io::copy_bidirectional(&mut local,&mut remote)=>{}}}
                        }
                    });
                }}
            }
            task_cancel.cancel();
            while work.join_next().await.is_some() {}
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
                        c.require_remote_access()?;
                        let port = c.ports.read().await.get(id).cloned();
                        if let Some(port) = port {
                            if let Ok(Ok(stream)) = tokio::time::timeout(
                                Duration::from_secs(5),
                                TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port.port)),
                            )
                            .await
                            {
                                c.require_remote_access()?;
                                c.active().await?;
                                if cancel.is_cancelled() {
                                    return Err(Error::Cancelled);
                                }
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
        if let Some(connection) = self.iroh.lock().await.take() {
            connection.close(iroh::endpoint::VarInt::from_u32(0), b"peer closed");
        }
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
