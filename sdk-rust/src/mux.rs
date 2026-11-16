//! Bounded smux v1, interoperable with xtaci/smux. Slow receive streams are
//! terminated explicitly rather than blocking the sole frame/keepalive reader.
use crate::{
    diagnostics::DiagnosticHandle,
    error::{protocol, transport},
    CloseReason, DiagnosticStage, Result,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream},
    sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore},
    time::Instant,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub(crate) trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}
pub(crate) type BoxIo = Box<dyn Io>;
const MAX_FRAME: usize = 32768;
const MAX_STREAMS: usize = 64;
struct Frame {
    cmd: u8,
    sid: u32,
    data: Vec<u8>,
}
struct Packet {
    data: Vec<u8>,
    _permit: OwnedSemaphorePermit,
}
struct Slot {
    input: mpsc::Sender<Packet>,
    owner: Arc<StreamOwner>,
}
pub(crate) struct StreamOwner {
    pub cancel: CancellationToken,
    pub diagnostic: Option<DiagnosticHandle>,
    tasks: TaskTracker,
    done: tokio::sync::watch::Sender<bool>,
}
impl StreamOwner {
    pub fn stop(&self, reason: CloseReason) {
        if let Some(d) = &self.diagnostic {
            d.stop(reason);
        }
        self.cancel.cancel();
    }
    pub async fn wait(&self) {
        let mut done = self.done.subscribe();
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
}
pub(crate) struct MuxStream {
    pub io: DuplexStream,
    pub owner: Arc<StreamOwner>,
}
pub(crate) struct Mux {
    writer: mpsc::Sender<Frame>,
    slots: Mutex<HashMap<u32, Slot>>,
    accept_rx: Mutex<mpsc::Receiver<MuxStream>>,
    accept_tx: mpsc::Sender<MuxStream>,
    next: AtomicU32,
    client: bool,
    pub cancel: CancellationToken,
    budget: Arc<Semaphore>,
    last_frame: Mutex<Instant>,
    diagnostic: std::sync::Mutex<Option<DiagnosticHandle>>,
    pub tasks: TaskTracker,
}
impl Mux {
    #[cfg(test)]
    pub async fn stream_count(&self) -> usize {
        self.slots.lock().await.len()
    }
    pub fn set_diagnostic(&self, diagnostic: DiagnosticHandle) {
        *self.diagnostic.lock().unwrap() = Some(diagnostic);
    }
    pub fn stop(&self, reason: CloseReason) {
        if let Some(d) = self.diagnostic.lock().unwrap().as_ref() {
            d.stop(reason);
        }
        self.cancel.cancel();
    }
    pub async fn wait(&self) {
        self.tasks.close();
        self.tasks.wait().await;
    }
    pub fn start(io: BoxIo, client: bool, cancel: CancellationToken) -> Arc<Self> {
        let (tx, mut rx) = mpsc::channel::<Frame>(32);
        let (atx, arx) = mpsc::channel(MAX_STREAMS);
        let mux = Arc::new(Self {
            writer: tx,
            slots: Mutex::new(HashMap::new()),
            accept_rx: Mutex::new(arx),
            accept_tx: atx,
            next: AtomicU32::new(if client { 3 } else { 2 }),
            client,
            cancel,
            budget: Arc::new(Semaphore::new(3 << 20)),
            last_frame: Mutex::new(Instant::now()),
            diagnostic: std::sync::Mutex::new(None),
            tasks: TaskTracker::new(),
        });
        let (mut read, mut write) = tokio::io::split(io);
        let m = mux.clone();
        mux.tasks.spawn(async move {
            loop {
                let frame = tokio::select! { biased; _ = m.cancel.cancelled() => break, frame = rx.recv() => { let Some(frame) = frame else { break }; frame } };
                let mut header = [1, frame.cmd, 0, 0, 0, 0, 0, 0];
                header[2..4].copy_from_slice(&(frame.data.len() as u16).to_le_bytes());
                header[4..8].copy_from_slice(&frame.sid.to_le_bytes());
                let result = tokio::select! { biased; _ = m.cancel.cancelled() => break, r = async { write.write_all(&header).await?; write.write_all(&frame.data).await?; write.flush().await } => r };
                if result.is_err() { m.stop(CloseReason::Io); break; }
            }
            m.cancel.cancel();
        });
        let m = mux.clone();
        mux.tasks.spawn(async move {
            let result: Result<()> = async {
                loop {
                    let mut h = [0u8; 8];
                    tokio::select! { biased; _ = m.cancel.cancelled() => return Ok(()), r = read.read_exact(&mut h) => { r?; } }
                    if h[0] != 1 { return Err(protocol("smux version must be 1")); }
                    let size = u16::from_le_bytes([h[2], h[3]]) as usize;
                    let sid = u32::from_le_bytes(h[4..8].try_into().map_err(|_| protocol("invalid stream ID"))?);
                    if size > MAX_FRAME { return Err(protocol("smux frame exceeds limit")); }
                    match h[1] {
                        0 => {
                            if size != 0 || sid == 0 || sid % 2 == u32::from(m.client) { return Err(protocol("invalid smux SYN parity/length")); }
                            let stream = m.make_stream(sid, None).await?;
                            if let Err(error) = m.accept_tx.try_send(stream) {
                                error.into_inner().owner.stop(CloseReason::ReceiveBackpressure);
                                m.reject_stream(sid).await;
                            }
                        }
                        1 => {
                            if size != 0 { return Err(protocol("smux FIN has payload")); }
                            // Preserve queued bytes and the application-layer half-close.
                            // Dropping input drains its queue before delivering EOF.
                            m.slots.lock().await.remove(&sid);
                        }
                        2 => {
                            // One temporary frame is bounded independently of the aggregate
                            // receive budget. Never await a stream queue in this reader.
                            let mut data = vec![0; size];
                            tokio::select! { biased; _ = m.cancel.cancelled() => return Ok(()), r = read.read_exact(&mut data) => { r?; } }
                            let slots = m.slots.lock().await;
                            if let Some(slot) = slots.get(&sid) {
                                let permit = m.budget.clone().try_acquire_many_owned(size as u32);
                                let accepted = if let Ok(permit) = permit { slot.input.try_send(Packet { data, _permit: permit }).is_ok() } else { false };
                                if !accepted { slot.owner.stop(CloseReason::ReceiveBackpressure); drop(slots); m.reject_stream(sid).await; }
                            }
                        }
                        3 => { if size != 0 { return Err(protocol("smux NOP has payload")); } }
                        _ => return Err(protocol("unknown smux command")),
                    }
                    *m.last_frame.lock().await = Instant::now();
                }
            }.await;
            if let Err(error) = result { m.stop(CloseReason::from_error(&error)); }
            m.cancel.cancel();
            let slots = std::mem::take(&mut *m.slots.lock().await);
            let reason = m.diagnostic.lock().unwrap().as_ref().and_then(|d| d.snapshot().reason).unwrap_or(CloseReason::Unknown);
            for (_, slot) in slots { slot.owner.stop(reason); }
        });
        let m = mux.clone();
        mux.tasks.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            tick.tick().await;
            loop {
                tokio::select! { biased; _ = m.cancel.cancelled() => break, _ = tick.tick() => {
                    if m.last_frame.lock().await.elapsed() > Duration::from_secs(30) { m.stop(CloseReason::KeepaliveTimeout); break; }
                    // A blocked writer must not disable the next activity check.
                    if m.writer.try_send(Frame { cmd: 3, sid: 0, data: Vec::new() }).is_err() { continue; }
                } }
            }
        });
        mux
    }
    async fn reject_stream(&self, sid: u32) {
        self.slots.lock().await.remove(&sid);
        if self
            .writer
            .try_send(Frame {
                cmd: 1,
                sid,
                data: Vec::new(),
            })
            .is_err()
        {
            self.stop(CloseReason::ReceiveBackpressure);
        }
    }
    async fn send(&self, cmd: u8, sid: u32, data: Vec<u8>) -> Result<()> {
        tokio::select! { biased; _ = self.cancel.cancelled() => Err(transport("smux closed")), r = self.writer.send(Frame { cmd, sid, data }) => r.map_err(|_| transport("smux writer closed")) }
    }
    async fn make_stream(
        self: &Arc<Self>,
        sid: u32,
        diagnostic: Option<DiagnosticHandle>,
    ) -> Result<MuxStream> {
        let (app, worker) = tokio::io::duplex(8192);
        // Allow ordinary bursts to drain; total memory remains bounded by budget.
        let (input, mut input_rx) = mpsc::channel::<Packet>(32);
        let diagnostic = diagnostic.or_else(|| {
            self.diagnostic
                .lock()
                .unwrap()
                .as_ref()
                .map(|d| d.child(DiagnosticStage::PortOpen, "stream"))
        });
        let owner = Arc::new(StreamOwner {
            cancel: self.cancel.child_token(),
            diagnostic,
            tasks: TaskTracker::new(),
            done: tokio::sync::watch::channel(false).0,
        });
        {
            let mut slots = self.slots.lock().await;
            if slots.len() >= MAX_STREAMS || slots.contains_key(&sid) {
                return Err(protocol("smux stream capacity or duplicate ID"));
            }
            slots.insert(
                sid,
                Slot {
                    input,
                    owner: owner.clone(),
                },
            );
        }
        let (mut r, mut w) = tokio::io::split(worker);
        let o = owner.clone();
        self.tasks.spawn(owner.tasks.track_future(async move {
            loop {
                let packet = tokio::select! { biased; _ = o.cancel.cancelled() => break, p = input_rx.recv() => { let Some(p) = p else { break }; p } };
                let result = tokio::select! { biased; _ = o.cancel.cancelled() => break, r = w.write_all(&packet.data) => r };
                if result.is_err() { break; }
            }
            let _ = w.shutdown().await;
        }));
        let m = self.clone();
        let o = owner.clone();
        self.tasks.spawn(owner.tasks.track_future(async move {
            let mut b = vec![0; MAX_FRAME];
            loop {
                let result = tokio::select! { biased; _ = o.cancel.cancelled() => break, r = r.read(&mut b) => r };
                match result { Ok(0) | Err(_) => break, Ok(n) => { let sent = tokio::select! { biased; _ = o.cancel.cancelled() => break, result = m.send(2, sid, b[..n].to_vec()) => result }; if sent.is_err() { break; } } }
            }
            // Graceful FIN follows all queued data. Explicit cancellation must
            // not wait indefinitely for the writer; fail the session explicitly
            // if even its FIN cannot be queued, rather than silently leaking a
            // remote stream or dropping bytes and reporting success.
            let fin = if o.cancel.is_cancelled() {
                m.writer.try_send(Frame { cmd: 1, sid, data: Vec::new() }).is_ok()
            } else {
                tokio::select! {
                    biased;
                    _ = o.cancel.cancelled() => m.writer.try_send(Frame { cmd: 1, sid, data: Vec::new() }).is_ok(),
                    result = m.send(1, sid, Vec::new()) => result.is_ok(),
                }
            };
            if !fin && !m.cancel.is_cancelled() { m.stop(CloseReason::ReceiveBackpressure); }
            m.slots.lock().await.remove(&sid);
            o.cancel.cancel();
        }));
        let o = owner.clone();
        let m = self.clone();
        self.tasks.spawn(async move {
            o.cancel.cancelled().await;
            o.tasks.close();
            o.tasks.wait().await;
            if let Some(d) = &o.diagnostic {
                if d.snapshot().reason.is_none() {
                    d.stop(
                        m.diagnostic
                            .lock()
                            .unwrap()
                            .as_ref()
                            .and_then(|p| p.snapshot().reason)
                            .unwrap_or(CloseReason::Unknown),
                    );
                }
                d.complete();
            }
            o.done.send_replace(true);
        });
        Ok(MuxStream { io: app, owner })
    }
    #[cfg(test)]
    pub async fn open_tracked(self: &Arc<Self>) -> Result<MuxStream> {
        self.open_with_diagnostic(None).await
    }
    pub async fn open_with_diagnostic(
        self: &Arc<Self>,
        diagnostic: Option<DiagnosticHandle>,
    ) -> Result<MuxStream> {
        let sid = self.next.fetch_add(2, Ordering::SeqCst);
        if sid > u32::MAX - 2 {
            self.stop(CloseReason::Protocol);
            return Err(protocol("smux stream ID exhausted"));
        }
        let stream = self.make_stream(sid, diagnostic).await?;
        if let Err(error) = self.send(0, sid, Vec::new()).await {
            stream.owner.stop(CloseReason::from_error(&error));
            return Err(error);
        }
        Ok(stream)
    }
    #[cfg(test)]
    pub async fn open(self: &Arc<Self>) -> Result<DuplexStream> {
        Ok(self.open_tracked().await?.io)
    }
    pub async fn accept_tracked(&self) -> Result<MuxStream> {
        tokio::select! { biased; _ = self.cancel.cancelled() => Err(transport("smux closed")), s = async { self.accept_rx.lock().await.recv().await } => s.ok_or_else(|| transport("smux accept closed")) }
    }
    #[cfg(test)]
    pub async fn accept(&self) -> Result<DuplexStream> {
        Ok(self.accept_tracked().await?.io)
    }
    pub fn frame(self: &Arc<Self>, stream: MuxStream) -> DuplexStream {
        let owner = stream.owner.clone();
        let (app, work) = frame_worker(stream.io, owner.cancel.clone(), owner.diagnostic.clone());
        self.tasks.spawn(owner.tasks.track_future(work));
        app
    }
}
fn frame_worker(
    raw: DuplexStream,
    cancel: CancellationToken,
    diagnostic: Option<DiagnosticHandle>,
) -> (DuplexStream, impl std::future::Future<Output = ()> + Send) {
    let (app, worker) = tokio::io::duplex(32768);
    let work = async move {
        let (mut raw_r, mut raw_w) = tokio::io::split(raw);
        let (mut app_r, mut app_w) = tokio::io::split(worker);
        let inbound = async {
            loop {
                let n = raw_r.read_u32().await?;
                if n == 0 {
                    app_w.shutdown().await?;
                    return Ok::<_, std::io::Error>(());
                }
                if n > 32768 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid application frame size",
                    ));
                }
                let mut b = vec![0; n as usize];
                raw_r.read_exact(&mut b).await?;
                app_w.write_all(&b).await?;
            }
        };
        let outbound = async {
            let mut b = vec![0; 32768];
            loop {
                let n = app_r.read(&mut b).await?;
                raw_w.write_u32(n as u32).await?;
                if n == 0 {
                    return Ok::<_, std::io::Error>(());
                }
                raw_w.write_all(&b[..n]).await?;
            }
        };
        tokio::pin!(inbound, outbound);
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => None,
            r = &mut inbound => { if r.is_ok() { tokio::select! { biased; _ = cancel.cancelled() => None, r = outbound => Some(r) } } else { Some(r) } },
            r = &mut outbound => { if r.is_ok() { tokio::select! { biased; _ = cancel.cancelled() => None, r = inbound => Some(r) } } else { Some(r) } },
        };
        if let Some(d) = diagnostic {
            let reason = match result {
                Some(Ok(())) => CloseReason::RemoteFin,
                Some(Err(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    CloseReason::RemoteEof
                }
                Some(Err(ref e)) if e.kind() == std::io::ErrorKind::InvalidData => {
                    CloseReason::Protocol
                }
                Some(Err(_)) => CloseReason::Io,
                None => CloseReason::Unknown,
            };
            d.stop(reason);
        }
        // On a graceful FIN, drop raw only after both directions complete.
        // The smux writer must drain its duplex buffer before sending its FIN.
        // Cancelling here would discard queued final bytes/acknowledgements.
        if !matches!(result, Some(Ok(()))) {
            cancel.cancel();
        }
    };
    (app, work)
}
/// Test/legacy adapter for the BE32 data/FIN protocol.
#[cfg(test)]
pub(crate) fn framed_stream(raw: DuplexStream, cancel: CancellationToken) -> DuplexStream {
    let (app, work) = frame_worker(raw, cancel, None);
    tokio::spawn(work);
    app
}
