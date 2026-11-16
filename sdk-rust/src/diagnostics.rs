//! Local, bounded diagnostics. No device IDs, addresses or provider error text.
use crate::Error;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{broadcast, watch},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    WebRtc,
    Iroh,
    Cloudflare,
}
impl Transport {
    pub(crate) fn from_label(label: &str) -> Self {
        match label {
            "webrtc-direct" => Self::WebRtc,
            "iroh-direct" => Self::Iroh,
            _ => Self::Cloudflare,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticStage {
    Connect,
    Directory,
    WebRtc,
    Iroh,
    Tunnel,
    Authentication,
    PortOpen,
    Forwarding,
    Multiplexer,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticPhase {
    Starting,
    Ready,
    Closing,
    Closed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseReason {
    LocalClose,
    Cancelled,
    DeadlineExceeded,
    StageTimeout,
    KeepaliveTimeout,
    RemoteEof,
    RemoteFin,
    Io,
    Protocol,
    Authentication,
    PortUnavailable,
    GenerationChanged,
    RemoteDisabled,
    ClientClosed,
    CoordinationLost,
    TransportUnavailable,
    ReceiveBackpressure,
    Unknown,
}
impl CloseReason {
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::DeadlineExceeded
                | Self::StageTimeout
                | Self::KeepaliveTimeout
                | Self::RemoteEof
                | Self::Io
                | Self::CoordinationLost
                | Self::ReceiveBackpressure
        )
    }
    pub(crate) fn from_error(error: &Error) -> Self {
        match error {
            Error::Cancelled => Self::Cancelled,
            Error::Timeout(_) => Self::StageTimeout,
            Error::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Self::RemoteEof,
            Error::Io(_) | Error::Http(_) => Self::Io,
            Error::Protocol(_) => Self::Protocol,
            Error::NotLoggedIn => Self::CoordinationLost,
            Error::IdentityChanged => Self::Authentication,
            Error::Operation { diagnostic } => diagnostic.reason.unwrap_or(Self::Unknown),
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSnapshot {
    pub id: String,
    pub attempt_id: Option<String>,
    pub session_id: Option<String>,
    pub mapping_id: Option<String>,
    pub stream_id: Option<String>,
    pub revision: u64,
    pub generation: u64,
    pub transport: Option<Transport>,
    pub stage: DiagnosticStage,
    pub phase: DiagnosticPhase,
    pub reason: Option<CloseReason>,
    pub elapsed_ms: u64,
    /// Whether a NEW connection may be attempted; never permission to replay bytes.
    pub retryable: bool,
    pub cleanup_pending: bool,
}
#[derive(Clone)]
pub(crate) struct Diagnostics {
    events: broadcast::Sender<DiagnosticSnapshot>,
    recent: Arc<Mutex<Vec<DiagnosticSnapshot>>>,
}
impl Diagnostics {
    pub fn new() -> Self {
        Self {
            events: broadcast::channel(256).0,
            recent: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<DiagnosticSnapshot> {
        self.events.subscribe()
    }
    pub fn snapshot(&self) -> Vec<DiagnosticSnapshot> {
        self.recent.lock().unwrap().clone()
    }
    fn publish(&self, snapshot: DiagnosticSnapshot) {
        let mut recent = self.recent.lock().unwrap();
        if let Some(previous) = recent.iter_mut().find(|s| s.id == snapshot.id) {
            *previous = snapshot.clone();
        } else {
            if recent.len() == 256 {
                recent.remove(0);
            }
            recent.push(snapshot.clone());
        }
        let _ = self.events.send(snapshot);
    }
    pub fn start(&self, generation: u64, stage: DiagnosticStage) -> DiagnosticHandle {
        let id = uuid::Uuid::new_v4().to_string();
        DiagnosticHandle::new(
            self.clone(),
            DiagnosticSnapshot {
                attempt_id: Some(id.clone()),
                id,
                session_id: None,
                mapping_id: None,
                stream_id: None,
                revision: 0,
                generation,
                transport: None,
                stage,
                phase: DiagnosticPhase::Starting,
                reason: None,
                elapsed_ms: 0,
                retryable: false,
                cleanup_pending: false,
            },
        )
    }
}
#[derive(Clone)]
pub(crate) struct DiagnosticHandle(Arc<DiagnosticInner>);
struct DiagnosticInner {
    store: Diagnostics,
    state: watch::Sender<DiagnosticSnapshot>,
    started: Instant,
}
impl DiagnosticHandle {
    fn new(store: Diagnostics, state: DiagnosticSnapshot) -> Self {
        store.publish(state.clone());
        Self(Arc::new(DiagnosticInner {
            store,
            state: watch::channel(state).0,
            started: Instant::now(),
        }))
    }
    pub fn child(&self, stage: DiagnosticStage, scope: &str) -> Self {
        let mut state = self.snapshot();
        state.id = uuid::Uuid::new_v4().to_string();
        match scope {
            "session" => state.session_id = Some(state.id.clone()),
            "mapping" => state.mapping_id = Some(state.id.clone()),
            "stream" => state.stream_id = Some(state.id.clone()),
            _ => {}
        }
        state.revision = 0;
        state.stage = stage;
        state.phase = DiagnosticPhase::Starting;
        state.reason = None;
        state.retryable = false;
        state.elapsed_ms = 0;
        state.cleanup_pending = false;
        Self::new(self.0.store.clone(), state)
    }
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        self.0.state.borrow().clone()
    }
    pub fn watch(&self) -> watch::Receiver<DiagnosticSnapshot> {
        self.0.state.subscribe()
    }
    fn update(&self, change: impl FnOnce(&mut DiagnosticSnapshot)) {
        // Serialize publication with the watch revision, including concurrent close.
        self.0.state.send_modify(|state| {
            change(state);
            state.revision += 1;
            state.elapsed_ms = self.0.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            self.0.store.publish(state.clone());
        });
    }
    pub fn stage(&self, stage: DiagnosticStage, transport: Option<Transport>) {
        self.update(|s| {
            if s.reason.is_none() {
                s.stage = stage;
                s.transport = transport.or(s.transport);
            }
        });
    }
    pub fn ready(&self) {
        self.update(|s| {
            if s.reason.is_none() {
                s.phase = DiagnosticPhase::Ready;
            }
        });
    }
    pub fn stop(&self, reason: CloseReason) {
        self.update(|s| {
            if s.reason.is_none() {
                s.reason = Some(reason);
                s.retryable = reason.retryable();
                s.phase = DiagnosticPhase::Closing;
                s.cleanup_pending = true;
            }
        });
    }
    pub fn complete(&self) {
        self.update(|s| {
            s.phase = DiagnosticPhase::Closed;
            s.cleanup_pending = false;
        });
    }
}
