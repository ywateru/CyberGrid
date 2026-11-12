//! External bridge lifecycle injection; no external protocol or dependency is bundled.
use gateway_upstream::GatewayUpstream;
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use workspace_core::Workspace;
pub type CloseFuture = Pin<Box<dyn Future<Output = Result<(), &'static str>> + Send>>;
pub type StartFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn GatewayLease>, &'static str>> + Send>>;
pub type BindFuture<'a> = Pin<Box<dyn Future<Output = Result<(), &'static str>> + Send + 'a>>;
// No Debug/Serialize. This RAM capability must never be logged or put in argv.
pub struct LaunchContext {
    pub generation: u64,
    pub local_port: u16,
    pub local_token: String,
    pub issuer: String,
    pub resource: String,
    pub allowed_spa_client: String,
    pub owner_subject: String,
    pub allowed_origins: Vec<String>,
    pub cancel: CancellationToken,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayIdentity {
    pub installation_id: String,
    pub public_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayCapabilities {
    pub protocol: String,
    pub limits: cybergrid_sdk::BridgeLimits,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEndpoint {
    pub revision: u64,
    pub provider_id: String,
    pub url: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayEndpointState {
    Pending,
    Available(GatewayEndpoint),
    Unavailable { revision: u64 },
    Closed { revision: u64 },
}
impl GatewayEndpointState {
    pub fn revision(&self) -> Option<u64> {
        match self {
            Self::Pending => None,
            Self::Available(e) => Some(e.revision),
            Self::Unavailable { revision } | Self::Closed { revision } => Some(*revision),
        }
    }
    pub fn follows(&self, previous: &Self) -> bool {
        if self == previous {
            return true;
        }
        match (previous.revision(), self.revision()) {
            (_, Some(n)) if n == 0 || n > 9_007_199_254_740_991 => false,
            (None, Some(_)) => true,
            (Some(a), Some(b)) => b > a,
            _ => false,
        }
    }
}
pub trait GatewayLease: Send {
    fn ingress(&self) -> SocketAddr;
    fn identity(&self) -> GatewayIdentity;
    fn capabilities(&self) -> GatewayCapabilities;
    /// Public endpoint metadata only. Keep the producer owned until close.
    fn endpoint_updates(&self) -> watch::Receiver<GatewayEndpointState>;
    /// Start/retain the actual public ingress targeting only the host gate.
    /// The owner drives this borrowed future to completion even after cancellation.
    fn bind_guarded_ingress<'a>(
        &'a mut self,
        _address: SocketAddr,
        _cancel: CancellationToken,
    ) -> BindFuture<'a> {
        Box::pin(async { Err("GATEWAY_BIND_UNAVAILABLE") })
    }
    // Adapters can implement this by owning the actual Gateway and awaiting gateway.close().
    fn close(self: Box<Self>) -> CloseFuture;
}
pub trait GatewayFactory: Send + Sync {
    fn start(&self, context: LaunchContext) -> StartFuture;
}
/// The public tunnel must point only to this host-controlled TCP ingress.
pub struct TcpGate {
    pub address: SocketAddr,
    open: Arc<AtomicBool>,
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl TcpGate {
    pub async fn start(target: SocketAddr) -> Result<Self, &'static str> {
        Self::start_with_cancel(target, CancellationToken::new()).await
    }
    pub async fn start_with_cancel(
        target: SocketAddr,
        stop: CancellationToken,
    ) -> Result<Self, &'static str> {
        if target.ip() != Ipv4Addr::LOCALHOST || target.port() == 0 {
            return Err("INVALID_GATEWAY_INGRESS");
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "GATE_BIND")?;
        let address = listener.local_addr().map_err(|_| "GATE_BIND")?;
        let open = Arc::new(AtomicBool::new(true));
        let cancel = stop.clone();
        let active = open.clone();
        let task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {biased;_=cancel.cancelled()=>break,Some(_)=children.join_next(),if !children.is_empty()=>{},r=listener.accept()=>{let Ok((mut inbound,_))=r else{break};if !active.load(Ordering::SeqCst)||children.len()>=32{continue};let stop=cancel.clone();children.spawn(async move{tokio::select!{biased;_=stop.cancelled()=>{},_=async{let mut outbound=tokio::time::timeout(Duration::from_secs(5),TcpStream::connect(target)).await.map_err(|_|())?.map_err(|_|())?;tokio::io::copy_bidirectional(&mut inbound,&mut outbound).await.map_err(|_|())?;Ok::<(),()>(())}=>{}}});}}
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        Ok(Self {
            address,
            open,
            stop,
            task: Some(task),
        })
    }
    pub fn seal(&self) {
        self.open.store(false, Ordering::SeqCst);
        self.stop.cancel();
    }
    pub async fn close(&mut self) {
        self.seal();
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
    }
}
impl Drop for TcpGate {
    fn drop(&mut self) {
        self.seal();
    }
}
pub struct GatewayRuntime {
    pub upstream: GatewayUpstream,
    pub gate: TcpGate,
    pub lease: Box<dyn GatewayLease>,
    pub endpoints: watch::Receiver<GatewayEndpointState>,
    monitor: JoinHandle<()>,
    observed: Arc<std::sync::Mutex<GatewayEndpointState>>,
}
impl GatewayRuntime {
    /// Validates against the same history observed during bind and throughout runtime.
    pub fn endpoint_snapshot(&self) -> Result<GatewayEndpointState, &'static str> {
        let mut previous = self.observed.lock().unwrap();
        let state = self.endpoints.borrow().clone();
        if !state.follows(&previous) || self.endpoints.has_changed().is_err() {
            self.seal();
            return Err("GATEWAY_ENDPOINT_INVALID");
        }
        *previous = state.clone();
        Ok(state)
    }
    pub fn seal(&self) {
        self.gate.seal();
        self.upstream.seal();
    }
    pub async fn close(mut self) -> Result<(), &'static str> {
        self.seal();
        self.gate.close().await;
        self.upstream.close().await;
        let _ = self.monitor.await;
        self.lease.close().await
    }
    pub async fn create(
        factory: Arc<dyn GatewayFactory>,
        workspace: Workspace,
        mut context: LaunchContext,
    ) -> Result<Self, &'static str> {
        let mut upstream =
            GatewayUpstream::start_with_cancel(workspace, context.cancel.clone()).await?;
        context.local_port = upstream.port;
        context.local_token = upstream.local_token().into();
        let cancel = context.cancel.clone();
        // The caller must own this complete future even when cancelling its UI wait.
        let mut lease = match factory.start(context).await {
            Ok(v) => v,
            Err(e) => {
                upstream.close().await;
                return Err(e);
            }
        };
        if cancel.is_cancelled() {
            upstream.close().await;
            let _ = lease.close().await;
            return Err("CANCELLED");
        };
        let endpoints = lease.endpoint_updates(); // Subscribe before bind starts.
        let mut gate = match TcpGate::start_with_cancel(lease.ingress(), cancel.clone()).await {
            Ok(v) => v,
            Err(e) => {
                upstream.close().await;
                let _ = lease.close().await;
                return Err(e);
            }
        };
        // No select/drop on cancellation: the shared token seals the independently
        // running gate/upstream immediately, while the owned bind still settles.
        // A separate observer seals ingress even while an owned bind is pending.
        let mut observer = endpoints.clone();
        let observer_cancel = cancel.clone();
        let observed = Arc::new(std::sync::Mutex::new(GatewayEndpointState::Pending));
        let observed_monitor = observed.clone();
        let monitor = tokio::spawn(async move {
            loop {
                let (state, valid) = {
                    let mut previous = observed_monitor.lock().unwrap();
                    let state = observer.borrow_and_update().clone();
                    let valid = state.follows(&previous);
                    if valid {
                        *previous = state.clone();
                    }
                    (state, valid)
                };
                if !valid
                    || matches!(
                        state,
                        GatewayEndpointState::Unavailable { .. }
                            | GatewayEndpointState::Closed { .. }
                    )
                {
                    observer_cancel.cancel();
                    break;
                }
                tokio::select! { biased;
                    _ = observer_cancel.cancelled() => break,
                    r = observer.changed() => if r.is_err() { observer_cancel.cancel(); break; }
                }
            }
        });
        let bound = lease
            .bind_guarded_ingress(gate.address, cancel.clone())
            .await;
        // Handshake with the same observed history; never reset it at bind completion.
        let valid = {
            let mut previous = observed.lock().unwrap();
            let final_state = endpoints.borrow().clone();
            if final_state.follows(&previous) {
                *previous = final_state;
                true
            } else {
                false
            }
        };
        if !valid || endpoints.has_changed().is_err() {
            cancel.cancel();
        }
        if let Err(error) = bound {
            gate.close().await;
            upstream.close().await;
            let _ = monitor.await;
            let _ = lease.close().await;
            return Err(error);
        }
        if cancel.is_cancelled() {
            let runtime = Self {
                upstream,
                gate,
                lease,
                endpoints,
                monitor,
                observed,
            };
            let _ = runtime.close().await;
            return Err("CANCELLED");
        };
        Ok(Self {
            upstream,
            gate,
            lease,
            endpoints,
            monitor,
            observed,
        })
    }
}
#[cfg(test)]
mod tests;
