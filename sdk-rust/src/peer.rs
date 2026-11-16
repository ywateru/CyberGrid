use crate::{
    client::Client,
    diagnostics::DiagnosticHandle,
    error::{protocol, transport},
    mux::{Mux, StreamOwner},
    CloseReason, ConnectOptions, DiagnosticSnapshot, DiagnosticStage, DirectStrategy, Error,
    OpenOptions, Result, Transport,
};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{watch, Mutex},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub(crate) struct DialFence {
    pub cancel: CancellationToken,
    pub generation: u64,
    pub timeout: Option<Duration>,
    pub diagnostic: Option<DiagnosticHandle>,
    winner: Option<(Arc<AtomicU8>, u8)>,
}
struct PendingPeer(Option<Arc<Peer>>);
impl PendingPeer {
    fn release(mut self) -> Arc<Peer> {
        self.0.take().unwrap()
    }
}
impl Drop for PendingPeer {
    fn drop(&mut self) {
        if let Some(peer) = self.0.take() {
            peer.begin_close(CloseReason::Cancelled);
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
            timeout: None,
            diagnostic: None,
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
            winner: Some((winner, path)),
            ..Self::single(cancel, generation)
        }
    }
    fn options(mut self, timeout: Option<Duration>, diagnostic: DiagnosticHandle) -> Self {
        self.timeout = timeout;
        self.diagnostic = Some(diagnostic);
        self
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
struct ConnectGuard {
    cancel: CancellationToken,
    diagnostic: DiagnosticHandle,
    finished: bool,
}
impl Drop for ConnectGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
        if !self.finished {
            self.diagnostic.stop(CloseReason::Cancelled);
            // Drop cannot await provider cleanup. Keep this terminal attempt
            // marked pending; explicit cancellation awaits cleanup instead.
        }
    }
}

/// An explicitly shared, authenticated transport. Clone the Arc, not the connection.
/// Unlike legacy Client::map, a closed Peer never reconnects across generations.
pub struct Peer {
    pub device_id: String,
    generation: u64,
    pub transport: String,
    pub(crate) mux: Arc<Mux>,
    client: std::sync::Weak<Client>,
    diagnostic: DiagnosticHandle,
    close_done: watch::Sender<bool>,
    mapping_slots: Arc<tokio::sync::Semaphore>,
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
            mux.stop(CloseReason::Cancelled);
            mux.wait().await;
            return Err(Error::Cancelled);
        }
        let diagnostic = fence
            .and_then(|f| f.diagnostic.as_ref())
            .cloned()
            .unwrap_or_else(|| {
                self.diagnostics
                    .start(generation, DiagnosticStage::Authentication)
            })
            .child(DiagnosticStage::Multiplexer, "session");
        diagnostic.stage(
            DiagnosticStage::Multiplexer,
            Some(Transport::from_label(&path)),
        );
        mux.set_diagnostic(diagnostic.clone());
        let peer = Arc::new(Peer {
            generation,
            device_id: id,
            transport: path,
            mux,
            client: Arc::downgrade(self),
            diagnostic,
            close_done: watch::channel(false).0,
            mapping_slots: Arc::new(tokio::sync::Semaphore::new(64)),
            rtc: Mutex::new(None),
            iroh: Mutex::new(None),
        });
        {
            let mut peers = rt.peers.lock().await;
            if self.generation() != generation
                || rt.cancel.is_cancelled()
                || fence.is_some_and(|f| f.claim().is_err())
            {
                peer.begin_close(CloseReason::Cancelled);
                peer.shutdown_transport().await;
                peer.mux.wait().await;
                peer.diagnostic.complete();
                return Err(Error::Cancelled);
            }
            peers.retain(|p| !p.is_closed());
            if peers.len() >= 64 {
                peer.begin_close(CloseReason::Protocol);
                peer.mux.wait().await;
                peer.diagnostic.complete();
                return Err(protocol("maximum 64 peers"));
            }
            peers.push(peer.clone());
        }
        peer.diagnostic.ready();
        self.event(
            "multiplexer-ready",
            Some(&peer.transport),
            Some(&peer.device_id),
            None,
        );
        let p = peer.clone();
        tokio::spawn(async move {
            p.serve().await;
            if p.diagnostic.snapshot().reason.is_none() {
                let reason = p
                    .client
                    .upgrade()
                    .map(|c| {
                        if c.generation() != p.generation {
                            CloseReason::GenerationChanged
                        } else if c.is_closed() {
                            c.terminal_reason().unwrap_or(CloseReason::ClientClosed)
                        } else {
                            CloseReason::CoordinationLost
                        }
                    })
                    .unwrap_or(CloseReason::ClientClosed);
                p.begin_close(reason);
            }
            p.shutdown_transport().await;
            p.mux.wait().await;
            p.diagnostic.complete();
            p.close_done.send_replace(true);
            if let Some(c) = p.client.upgrade() {
                if c.generation() == p.generation {
                    c.event(
                        "mapping-disconnected",
                        Some(&p.transport),
                        Some(&p.device_id),
                        None,
                    );
                }
            }
        });
        Ok(peer)
    }
    pub async fn connect(self: &Arc<Self>, id: &str) -> Result<Arc<Peer>> {
        self.connect_with_cancel(id, self.closed_token().child_token())
            .await
    }
    /// Errors include an identity-free diagnostic snapshot. Deadline includes
    /// directory lookup, signaling, path selection and authenticated winner cleanup.
    pub async fn connect_with_options(
        self: &Arc<Self>,
        id: &str,
        options: ConnectOptions,
    ) -> Result<Arc<Peer>> {
        options.validate()?;
        let diagnostic = self
            .diagnostics
            .start(self.generation(), DiagnosticStage::Connect);
        self.connect_operation(
            id,
            self.closed_token().child_token(),
            &options,
            diagnostic.clone(),
        )
        .await
        .map_err(|_| Error::Operation {
            diagnostic: Box::new(diagnostic.snapshot()),
        })
    }
    async fn connect_with_cancel(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Arc<Peer>> {
        let diagnostic = self
            .diagnostics
            .start(self.generation(), DiagnosticStage::Connect);
        self.connect_operation(id, cancel, &ConnectOptions::default(), diagnostic)
            .await
    }
    async fn connect_operation(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
        options: &ConnectOptions,
        diagnostic: DiagnosticHandle,
    ) -> Result<Arc<Peer>> {
        let mut guard = ConnectGuard {
            cancel: cancel.clone(),
            diagnostic: diagnostic.clone(),
            finished: false,
        };
        let mut config = self.cfg.clone();
        if let Some(value) = options.direct_strategy {
            config.direct_strategy = value;
        }
        if let Some(value) = options.direct_timeout {
            config.direct_timeout = value;
        }
        if let Some(value) = options.iroh_timeout {
            config.iroh_timeout = value;
        }
        if let Some(value) = options.fallback_timeout {
            config.fallback_timeout = value;
        }
        let deadline = options
            .operation
            .deadline
            .unwrap_or_else(|| Instant::now() + config.connection_budget());
        let external_cancel = options.operation.cancel.clone().unwrap_or_default();
        let operation = self.connect_chain(id, cancel.clone(), options, &diagnostic);
        tokio::pin!(operation);
        let result = tokio::select! {
            biased;
            _ = external_cancel.cancelled() => { diagnostic.stop(CloseReason::Cancelled); cancel.cancel(); if let Ok(peer) = operation.await { peer.begin_close(CloseReason::Cancelled); peer.close().await; } Err(Error::Cancelled) },
            _ = cancel.cancelled() => { diagnostic.stop(CloseReason::Cancelled); if let Ok(peer) = operation.await { peer.begin_close(CloseReason::Cancelled); peer.close().await; } Err(Error::Cancelled) },
            _ = tokio::time::sleep_until(deadline) => { diagnostic.stop(CloseReason::DeadlineExceeded); cancel.cancel(); if let Ok(peer) = operation.await { peer.begin_close(CloseReason::DeadlineExceeded); peer.close().await; } Err(Error::Timeout("peer connection budget")) },
            result = &mut operation => {
                if external_cancel.is_cancelled() || cancel.is_cancelled() || Instant::now() >= deadline {
                    let reason = if external_cancel.is_cancelled() || cancel.is_cancelled() { CloseReason::Cancelled } else { CloseReason::DeadlineExceeded };
                    diagnostic.stop(reason);
                    if let Ok(peer) = result { peer.begin_close(reason); peer.close().await; }
                    if reason == CloseReason::Cancelled { Err(Error::Cancelled) } else { Err(Error::Timeout("peer connection budget")) }
                } else { result }
            }
        };
        match &result {
            Ok(peer) => {
                diagnostic.stage(
                    DiagnosticStage::Connect,
                    Some(Transport::from_label(&peer.transport)),
                );
                diagnostic.ready();
            }
            Err(error) => diagnostic.stop(CloseReason::from_error(error)),
        }
        diagnostic.complete();
        guard.finished = true;
        result
    }
    async fn connect_chain(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
        options: &ConnectOptions,
        diagnostic: &DiagnosticHandle,
    ) -> Result<Arc<Peer>> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let rt = self.active().await?;
        diagnostic.stage(DiagnosticStage::Directory, None);
        let d = tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), result = self.device(id) => result? };
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
        let path_diagnostic = |stage, transport| {
            let child = diagnostic.child(stage, "path");
            child.stage(stage, Some(transport));
            child
        };
        let rtc_fence = || {
            DialFence::single(cancel.child_token(), rt.generation).options(
                options.direct_timeout,
                path_diagnostic(DiagnosticStage::WebRtc, Transport::WebRtc),
            )
        };
        let iroh_fence = || {
            DialFence::single(cancel.child_token(), rt.generation).options(
                options.iroh_timeout,
                path_diagnostic(DiagnosticStage::Iroh, Transport::Iroh),
            )
        };
        if let Some(only) = options.only_transport {
            match only {
                Transport::WebRtc | Transport::Iroh => {
                    let (stage, fence) = if only == Transport::WebRtc {
                        (DiagnosticStage::WebRtc, rtc_fence())
                    } else {
                        (DiagnosticStage::Iroh, iroh_fence())
                    };
                    diagnostic.stage(stage, Some(only));
                    if only == Transport::Iroh && !has_iroh {
                        diagnostic.stop(CloseReason::TransportUnavailable);
                        if let Some(d) = &fence.diagnostic {
                            d.stop(CloseReason::TransportUnavailable);
                            d.complete();
                        }
                        return Err(transport("direct iroh disabled or unavailable"));
                    }
                    self.event(
                        if only == Transport::WebRtc {
                            "direct-attempt"
                        } else {
                            "iroh-attempt"
                        },
                        Some(if only == Transport::WebRtc {
                            "webrtc-direct"
                        } else {
                            "iroh-direct"
                        }),
                        Some(id),
                        None,
                    );
                    let result = if only == Transport::WebRtc {
                        self.dial_direct_fenced(&d, &fence).await
                    } else {
                        self.dial_iroh_fenced(&d, &fence).await
                    };
                    if result.is_err() {
                        if let Some(d) = &fence.diagnostic {
                            let state = d.snapshot();
                            diagnostic.stage(state.stage, state.transport);
                            if let Some(reason) = state.reason {
                                diagnostic.stop(reason);
                            }
                        }
                    }
                    return result;
                }
                Transport::Cloudflare => {}
            }
        } else {
            diagnostic.stage(DiagnosticStage::WebRtc, Some(Transport::WebRtc));
            self.event("direct-attempt", Some("webrtc-direct"), Some(id), None);
            let direct = if options.direct_strategy.unwrap_or(self.cfg.direct_strategy)
                == DirectStrategy::Parallel
                && has_iroh
            {
                self.event("iroh-attempt", Some("iroh-direct"), Some(id), None);
                let winner = Arc::new(AtomicU8::new(0));
                let rtc_fence =
                    DialFence::competing(cancel.child_token(), rt.generation, winner.clone(), 1)
                        .options(
                            options.direct_timeout,
                            path_diagnostic(DiagnosticStage::WebRtc, Transport::WebRtc),
                        );
                let iroh_fence =
                    DialFence::competing(cancel.child_token(), rt.generation, winner, 2).options(
                        options.iroh_timeout,
                        path_diagnostic(DiagnosticStage::Iroh, Transport::Iroh),
                    );
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
                            loser.begin_close(CloseReason::Cancelled);
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
                match self.dial_direct_fenced(&d, &rtc_fence()).await {
                    Ok(peer) => Ok(peer),
                    Err(error) => {
                        if cancel.is_cancelled()
                            || rt.cancel.is_cancelled()
                            || self.generation() != rt.generation
                        {
                            return Err(Error::Cancelled);
                        }
                        if has_iroh {
                            diagnostic.stage(DiagnosticStage::Iroh, Some(Transport::Iroh));
                            self.event("iroh-attempt", Some("iroh-direct"), Some(id), None);
                            self.dial_iroh_fenced(&d, &iroh_fence()).await
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
                    peer.begin_close(CloseReason::Cancelled);
                    peer.close().await;
                    return Err(Error::Cancelled);
                }
                return Ok(peer);
            }
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
        diagnostic.stage(DiagnosticStage::Tunnel, Some(Transport::Cloudflare));
        let fence = DialFence::single(cancel.child_token(), rt.generation).options(
            None,
            path_diagnostic(DiagnosticStage::Tunnel, Transport::Cloudflare),
        );
        let result = tokio::select! { biased; _ = cancel.cancelled() => Err(Error::Cancelled), result = tokio::time::timeout(options.fallback_timeout.unwrap_or(self.cfg.fallback_timeout), async {
            loop {
                let current = self.device(id).await?;
                if self.generation() != rt.generation || rt.cancel.is_cancelled() { return Err(Error::Cancelled); }
                if current.tunnel_url.is_some() { return self.dial_tunnel_fenced(&current, &fence).await; }
                tokio::select! { biased; _ = rt.cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(Duration::from_millis(500)) => {} }
            }
        }) => result.map_err(|_| Error::Timeout("remote tunnel readiness or authentication")).and_then(|r| r) };
        if let Some(d) = &fence.diagnostic {
            if let Err(error) = &result {
                d.stop(CloseReason::from_error(error));
                let state = d.snapshot();
                diagnostic.stage(state.stage, state.transport);
                if let Some(reason) = state.reason {
                    diagnostic.stop(reason);
                }
            }
            d.complete();
        }
        result
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
        let pending = PendingPeer(Some(peer.clone()));
        let diagnostic = peer
            .diagnostic
            .child(DiagnosticStage::Forwarding, "mapping");
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
        diagnostic.ready();
        let task_diagnostic = diagnostic.clone();
        let task = tokio::spawn(async move {
            let mut work = tokio::task::JoinSet::new();
            loop {
                tokio::select! {_=task_cancel.cancelled()=>break,_=work.join_next(),if !work.is_empty()=>{},accepted=listener.accept(),if work.len()<64=>{
                    let Ok((mut local,_))=accepted else{break};
                    let client=client.clone();let slot=slot.clone();let device=device.clone();let port=port.clone();let cancel=task_cancel.clone();let diagnostic=task_diagnostic.clone();
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
                            if let Ok(mut remote)=p.dial_for_mapping(&port,cancel.clone(),&diagnostic).await {tokio::select!{_=cancel.cancelled()=>{},_=tokio::io::copy_bidirectional(&mut local,&mut remote)=>{}}}
                        }
                    });
                }}
            }
            task_cancel.cancel();
            while work.join_next().await.is_some() {}
            task_diagnostic.stop(CloseReason::LocalClose);
            task_diagnostic.complete();
        });
        pending.release();
        Ok(Mapping {
            owns_peer: true,
            diagnostic,
            addr,
            peer,
            current,
            cancel,
            task: Mutex::new(Some(task)),
        })
    }
}
impl Peer {
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        self.diagnostic.snapshot()
    }
    pub fn watch_state(&self) -> watch::Receiver<DiagnosticSnapshot> {
        self.diagnostic.watch()
    }
    pub(crate) fn begin_close(&self, reason: CloseReason) {
        let reason = if reason == CloseReason::LocalClose
            && self.is_closed()
            && self.diagnostic.snapshot().reason.is_none()
        {
            self.client
                .upgrade()
                .map(|c| {
                    c.terminal_reason()
                        .unwrap_or(if c.generation() != self.generation {
                            CloseReason::GenerationChanged
                        } else {
                            CloseReason::CoordinationLost
                        })
                })
                .unwrap_or(CloseReason::ClientClosed)
        } else {
            reason
        };
        self.mux.stop(reason);
    }
    pub async fn dial(&self, port: &str) -> Result<DuplexStream> {
        let diagnostic = self.diagnostic.child(DiagnosticStage::PortOpen, "stream");
        Ok(self
            .dial_operation(port, OpenOptions::default(), diagnostic)
            .await?
            .into_inner())
    }
    pub async fn dial_with_options(&self, port: &str, options: OpenOptions) -> Result<PeerStream> {
        if options.port_open_timeout.is_zero() {
            return Err(Error::Config("port open timeout must be positive".into()));
        }
        let diagnostic = self.diagnostic.child(DiagnosticStage::PortOpen, "stream");
        self.dial_operation(port, options, diagnostic.clone())
            .await
            .map_err(|_| Error::Operation {
                diagnostic: Box::new(diagnostic.snapshot()),
            })
    }
    async fn dial_for_mapping(
        &self,
        port: &str,
        cancel: CancellationToken,
        mapping: &DiagnosticHandle,
    ) -> Result<PeerStream> {
        let diagnostic = mapping.child(DiagnosticStage::PortOpen, "stream");
        self.dial_operation(
            port,
            OpenOptions {
                operation: crate::OperationOptions {
                    cancel: Some(cancel),
                    deadline: None,
                },
                ..OpenOptions::default()
            },
            diagnostic,
        )
        .await
    }
    async fn dial_operation(
        &self,
        port: &str,
        options: OpenOptions,
        diagnostic: DiagnosticHandle,
    ) -> Result<PeerStream> {
        let started = Instant::now();
        let phase_deadline = started
            .checked_add(options.port_open_timeout)
            .ok_or_else(|| Error::Config("port open timeout is too large".into()))?;
        let deadline = options
            .operation
            .deadline
            .map_or(phase_deadline, |d| d.min(phase_deadline));
        let timeout_reason = if options
            .operation
            .deadline
            .is_some_and(|d| d <= phase_deadline)
        {
            CloseReason::DeadlineExceeded
        } else {
            CloseReason::StageTimeout
        };
        let cancel = options.operation.cancel.clone().unwrap_or_default();
        let mut allocated = None;
        let operation = async {
            if port.is_empty() || port.len() > 64 {
                return Err(protocol("invalid port ID"));
            }
            let stream = self
                .mux
                .open_with_diagnostic(Some(diagnostic.clone()))
                .await?;
            let owner = stream.owner.clone();
            allocated = Some(owner.clone());
            let mut raw = stream.io;
            raw.write_u16(port.len() as u16).await?;
            raw.write_all(port.as_bytes()).await?;
            if raw.read_u8().await? != 0 {
                diagnostic.stop(CloseReason::PortUnavailable);
                return Err(protocol("remote port unavailable"));
            }
            let io = self.mux.frame(crate::mux::MuxStream {
                io: raw,
                owner: owner.clone(),
            });
            Ok(PeerStream {
                io: Some(io),
                owner,
            })
        };
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => { diagnostic.stop(CloseReason::Cancelled); Err(Error::Cancelled) },
            _ = self.mux.cancel.cancelled() => { diagnostic.stop(self.snapshot().reason.unwrap_or(CloseReason::Unknown)); Err(transport("peer closed")) },
            _ = tokio::time::sleep_until(deadline) => { diagnostic.stop(timeout_reason); Err(Error::Timeout("remote port open")) },
            result = operation => result,
        };
        match result {
            Ok(mut stream) => {
                if cancel.is_cancelled() || self.is_closed() || Instant::now() >= deadline {
                    let reason = if cancel.is_cancelled() {
                        CloseReason::Cancelled
                    } else if self.is_closed() {
                        self.snapshot().reason.unwrap_or(CloseReason::Unknown)
                    } else {
                        timeout_reason
                    };
                    stream.owner.stop(reason);
                    stream.close().await;
                    return Err(if reason == CloseReason::Cancelled {
                        Error::Cancelled
                    } else {
                        Error::Timeout("remote port open")
                    });
                }
                diagnostic.stage(DiagnosticStage::Forwarding, None);
                diagnostic.ready();
                if let Some(c) = self.client.upgrade() {
                    c.event(
                        "port-open",
                        Some(&self.transport),
                        Some(&self.device_id),
                        None,
                    );
                }
                Ok(stream)
            }
            Err(error) => {
                diagnostic.stop(CloseReason::from_error(&error));
                if let Some(owner) = allocated {
                    owner.stop(diagnostic.snapshot().reason.unwrap_or(CloseReason::Unknown));
                    owner.wait().await;
                }
                diagnostic.complete();
                Err(error)
            }
        }
    }
    /// Shared mapping: closing it never closes the Peer. It stops with this session
    /// and never silently reconnects under a new generation or transport.
    pub async fn map(self: &Arc<Self>, port: &str, local: SocketAddr) -> Result<Mapping> {
        self.map_with_options(port, local, OpenOptions::default())
            .await
    }
    pub async fn map_with_options(
        self: &Arc<Self>,
        port: &str,
        local: SocketAddr,
        options: OpenOptions,
    ) -> Result<Mapping> {
        if !local.ip().is_loopback() {
            return Err(protocol("mapping must listen on loopback"));
        }
        if options.port_open_timeout.is_zero() {
            return Err(Error::Config("port open timeout must be positive".into()));
        }
        let diagnostic = self.diagnostic.child(DiagnosticStage::PortOpen, "mapping");
        let result = self
            .create_shared_mapping(port, local, options, diagnostic.clone())
            .await;
        result.map_err(|error| {
            diagnostic.stop(CloseReason::from_error(&error));
            diagnostic.complete();
            Error::Operation {
                diagnostic: Box::new(diagnostic.snapshot()),
            }
        })
    }
    async fn create_shared_mapping(
        self: &Arc<Self>,
        port: &str,
        local: SocketAddr,
        options: OpenOptions,
        diagnostic: DiagnosticHandle,
    ) -> Result<Mapping> {
        let permit = self
            .mapping_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| protocol("maximum 64 shared mappings"))?;
        // Probe is a real target TCP open. Its normal close is a stream event only.
        let probe_diagnostic = diagnostic.child(DiagnosticStage::PortOpen, "stream");
        let mut probe = self
            .dial_operation(port, options.clone(), probe_diagnostic)
            .await?;
        probe.close().await;
        let external = options.operation.cancel.clone().unwrap_or_default();
        let bind = TcpListener::bind(local);
        let deadline = options
            .operation
            .deadline
            .unwrap_or_else(|| Instant::now() + options.port_open_timeout);
        let listener = tokio::select! {
            biased;
            _ = external.cancelled() => return Err(Error::Cancelled),
            _ = self.mux.cancel.cancelled() => return Err(transport("peer closed")),
            _ = tokio::time::sleep_until(deadline) => { diagnostic.stop(CloseReason::DeadlineExceeded); return Err(Error::Timeout("mapping setup")); },
            result = bind => result?,
        };
        let addr = listener.local_addr()?;
        if external.is_cancelled() {
            diagnostic.stop(CloseReason::Cancelled);
            return Err(Error::Cancelled);
        }
        if self.is_closed() {
            diagnostic.stop(self.snapshot().reason.unwrap_or(CloseReason::Unknown));
            return Err(transport("peer closed"));
        }
        if Instant::now() >= deadline {
            diagnostic.stop(CloseReason::DeadlineExceeded);
            return Err(Error::Timeout("mapping setup"));
        }
        let cancel = self.mux.cancel.child_token();
        let task_cancel = cancel.clone();
        let peer = self.clone();
        let port = port.to_owned();
        let task_diagnostic = diagnostic.clone();
        let timeout = options.port_open_timeout;
        diagnostic.stage(DiagnosticStage::Forwarding, None);
        diagnostic.ready();
        let task = self.mux.tasks.spawn(async move {
            let _permit = permit;
            let mut work = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = task_cancel.cancelled() => break,
                    _ = work.join_next(), if !work.is_empty() => {},
                    accepted = listener.accept(), if work.len() < 64 => {
                        let Ok((mut local, _)) = accepted else { task_diagnostic.stop(CloseReason::Io); break; };
                        let peer = peer.clone(); let port = port.clone(); let cancel = task_cancel.clone(); let diagnostic = task_diagnostic.clone();
                        work.spawn(async move {
                            let stream_diagnostic = diagnostic.child(DiagnosticStage::PortOpen, "stream");
                            let options = OpenOptions { port_open_timeout: timeout, operation: crate::OperationOptions { cancel: Some(cancel.clone()), deadline: None } };
                            if let Ok(mut remote) = peer.dial_operation(&port, options, stream_diagnostic).await {
                                tokio::select! { biased; _ = cancel.cancelled() => {}, _ = tokio::io::copy_bidirectional(&mut local, &mut remote) => {} }
                                remote.close().await;
                            }
                        });
                    }
                }
            }
            task_cancel.cancel();
            while work.join_next().await.is_some() {}
            task_diagnostic.stop(peer.snapshot().reason.unwrap_or(CloseReason::LocalClose)); task_diagnostic.complete();
        });
        Ok(Mapping {
            addr,
            peer: self.clone(),
            current: Arc::new(Mutex::new(Some(self.clone()))),
            cancel,
            task: Mutex::new(Some(task)),
            owns_peer: false,
            diagnostic,
        })
    }
    async fn serve(self: &Arc<Self>) {
        let mut work = tokio::task::JoinSet::new();
        loop {
            let stream = tokio::select! {
                biased;
                _ = self.mux.cancel.cancelled() => break,
                _ = work.join_next(), if !work.is_empty() => continue,
                result = self.mux.accept_tracked() => { let Ok(stream) = result else { break }; stream },
            };
            let Some(c) = self.client.upgrade() else {
                stream.owner.stop(CloseReason::ClientClosed);
                break;
            };
            let mux = self.mux.clone();
            work.spawn(async move {
                let owner = stream.owner.clone(); let diagnostic = owner.diagnostic.clone(); let cancel = owner.cancel.clone();
                let mut raw = stream.io;
                let result: Result<TcpStream> = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(Error::Cancelled),
                    result = tokio::time::timeout(Duration::from_secs(10), async {
                        let n = raw.read_u16().await? as usize;
                        if n == 0 || n > 64 { return Err(protocol("invalid port selection")); }
                        let mut b = vec![0; n]; raw.read_exact(&mut b).await?;
                        let id = std::str::from_utf8(&b).map_err(|_| protocol("invalid port UTF8"))?;
                        c.active().await?; c.require_remote_access()?;
                        let port = c.ports.read().await.get(id).cloned();
                        if let Some(port) = port {
                            if let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port.port))).await {
                                c.require_remote_access()?; c.active().await?;
                                if cancel.is_cancelled() { return Err(Error::Cancelled); }
                                raw.write_u8(0).await?; return Ok(stream);
                            }
                        }
                        raw.write_u8(1).await?;
                        if let Some(d) = &diagnostic { d.stop(CloseReason::PortUnavailable); }
                        Err(transport("requested port not exposed or unavailable"))
                    }) => result.map_err(|_| Error::Timeout("port selection")).and_then(|r| r),
                };
                match result {
                    Ok(mut target) => {
                        if let Some(d) = &diagnostic { d.stage(DiagnosticStage::Forwarding, None); d.ready(); }
                        let mut conn = mux.frame(crate::mux::MuxStream { io: raw, owner: owner.clone() });
                        tokio::select! { biased; _ = cancel.cancelled() => {}, result = tokio::io::copy_bidirectional(&mut conn, &mut target) => {
                            if let Err(error) = result { owner.stop(if error.kind() == std::io::ErrorKind::UnexpectedEof { CloseReason::RemoteEof } else { CloseReason::Io }); }
                        } }
                    }
                    Err(error) => {
                        // A rejection byte must drain before the smux FIN. It is
                        // a normal port-level response, not a session failure.
                        if diagnostic.as_ref().is_none_or(|d| d.snapshot().reason != Some(CloseReason::PortUnavailable)) { owner.stop(CloseReason::from_error(&error)); }
                    }
                }
            });
        }
        while work.join_next().await.is_some() {}
    }
    async fn shutdown_transport(&self) {
        if let Some(connection) = self.iroh.lock().await.take() {
            connection.close(iroh::endpoint::VarInt::from_u32(0), b"peer closed");
        }
        if let Some(a) = self.rtc.lock().await.take() {
            a.close().await;
        }
    }
    /// Idempotent, cancellation-safe close: all callers wait for the same cleanup.
    pub async fn close(&self) {
        self.begin_close(CloseReason::LocalClose);
        let mut done = self.close_done.subscribe();
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
    pub fn is_closed(&self) -> bool {
        self.mux.cancel.is_cancelled()
    }
}

/// A byte stream with a local correlation ID and a recoverable terminal snapshot.
pub struct PeerStream {
    io: Option<DuplexStream>,
    owner: Arc<StreamOwner>,
}
impl PeerStream {
    fn into_inner(mut self) -> DuplexStream {
        self.io.take().unwrap()
    }
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        self.owner.diagnostic.as_ref().unwrap().snapshot()
    }
    pub fn watch_state(&self) -> watch::Receiver<DiagnosticSnapshot> {
        self.owner.diagnostic.as_ref().unwrap().watch()
    }
    pub async fn close(&mut self) {
        self.owner.stop(CloseReason::LocalClose);
        self.io.take();
        self.owner.wait().await;
    }
}
impl Drop for PeerStream {
    fn drop(&mut self) {
        if self.io.is_some() {
            self.owner.stop(CloseReason::LocalClose);
        }
    }
}
impl AsyncRead for PeerStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut().io.as_mut() {
            Some(io) => Pin::new(io).poll_read(cx, buf),
            None => Poll::Ready(Ok(())),
        }
    }
}
impl AsyncWrite for PeerStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut().io.as_mut() {
            Some(io) => Pin::new(io).poll_write(cx, buf),
            None => Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut().io.as_mut() {
            Some(io) => Pin::new(io).poll_flush(cx),
            None => Poll::Ready(Ok(())),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut().io.as_mut() {
            Some(io) => Pin::new(io).poll_shutdown(cx),
            None => Poll::Ready(Ok(())),
        }
    }
}
pub struct Mapping {
    pub addr: SocketAddr,
    /// Initial peer retained for source compatibility. Use current_transport for live state.
    pub peer: Arc<Peer>,
    current: Arc<Mutex<Option<Arc<Peer>>>>,
    cancel: CancellationToken,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    owns_peer: bool,
    diagnostic: DiagnosticHandle,
}
impl Mapping {
    pub fn transport(&self) -> &str {
        &self.peer.transport
    }
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        self.diagnostic.snapshot()
    }
    pub fn watch_state(&self) -> watch::Receiver<DiagnosticSnapshot> {
        self.diagnostic.watch()
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
        self.diagnostic.stop(CloseReason::LocalClose);
        self.cancel.cancel();
        if self.owns_peer {
            if let Some(peer) = self.current.lock().await.take() {
                peer.close().await;
            }
            self.peer.close().await;
        }
        // Keep the handle in its slot until completion: another concurrent close
        // must not return early, and dropping this close future cannot detach it.
        let mut task = self.task.lock().await;
        if let Some(task) = task.as_mut() {
            let _ = task.await;
        }
        task.take();
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        self.diagnostic.stop(CloseReason::LocalClose);
        self.cancel.cancel();
        if self.owns_peer {
            self.peer.begin_close(CloseReason::LocalClose);
            if let Ok(current) = self.current.try_lock() {
                if let Some(peer) = current.as_ref() {
                    peer.begin_close(CloseReason::LocalClose);
                }
            }
        }
    }
}
