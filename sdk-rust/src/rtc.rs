use crate::{
    client::Client,
    error::{protocol, transport},
    mux::BoxIo,
    types::Signal,
    Device, Error, Peer, Result,
};
use bytes::Bytes;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, watch, Mutex},
};
use tokio_util::sync::CancellationToken;
use webrtc::{
    api::{setting_engine::SettingEngine, APIBuilder},
    data_channel::RTCDataChannel,
    ice::network_type::NetworkType,
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::{sdp_type::RTCSdpType, session_description::RTCSessionDescription},
        RTCPeerConnection,
    },
};

pub(crate) struct Attempt {
    pub id: String,
    pub remote: String,
    pub pc: Arc<RTCPeerConnection>,
    client: std::sync::Weak<Client>,
    runtime: std::sync::Weak<crate::client::Runtime>,
    remote_ready: Mutex<bool>,
    candidates: Mutex<Vec<RTCIceCandidateInit>>,
    ready_tx: mpsc::Sender<BoxIo>,
    ready_rx: Mutex<mpsc::Receiver<BoxIo>>,
    signal_ready: watch::Sender<bool>,
    pub cancel: CancellationToken,
    closed: AtomicBool,
    close_done: watch::Sender<bool>,
    pub connected: AtomicBool,
}
impl Attempt {
    pub async fn close(self: &Arc<Self>) {
        let mut done = self.close_done.subscribe();
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.cancel.cancel();
            let a = self.clone();
            // Own cleanup independently of callers that may themselves be
            // cancelled. Every concurrent close waits for the same completion.
            tokio::spawn(async move {
                let _ = a.pc.close().await;
                if let Some(rt) = a.runtime.upgrade() {
                    rt.attempts.lock().await.remove(&a.id);
                }
                a.close_done.send_replace(true);
            });
        }
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
    async fn set_remote(&self, d: RTCSessionDescription) -> Result<()> {
        self.pc
            .set_remote_description(d)
            .await
            .map_err(|_| transport("invalid remote SDP"))?;
        let mut ready = self.remote_ready.lock().await;
        *ready = true;
        let candidates = std::mem::take(&mut *self.candidates.lock().await);
        for c in candidates {
            self.pc
                .add_ice_candidate(c)
                .await
                .map_err(|_| transport("remote ICE candidate rejected"))?;
        }
        Ok(())
    }
}
fn detached_io(
    dc: Arc<webrtc::data::data_channel::DataChannel>,
    cancel: CancellationToken,
) -> BoxIo {
    let (app, worker) = tokio::io::duplex(65536);
    let (mut outgoing, mut incoming) = tokio::io::split(worker);
    let d = dc.clone();
    let c = cancel.clone();
    tokio::spawn(async move {
        let mut b = vec![0; 65536];
        loop {
            tokio::select! {_=c.cancelled()=>break,r=d.read_data_channel(&mut b)=>{
                let n=match r{Ok((n,false))if n>0=>n,_=>break};let sent=tokio::select!{_=c.cancelled()=>break,r=incoming.write_all(&b[..n])=>r};if sent.is_err(){break;}
            }}
        }
        c.cancel();
        let _ = incoming.shutdown().await;
        let _ = d.close().await;
    });
    tokio::spawn(async move {
        let mut b = vec![0; 16384];
        loop {
            tokio::select! {_=cancel.cancelled()=>break,r=outgoing.read(&mut b)=>{
                let n=match r{Ok(n)if n>0=>n,_=>break};let packet=Bytes::copy_from_slice(&b[..n]);let result=tokio::select!{_=cancel.cancelled()=>break,r=dc.write(&packet)=>r};if !result.is_ok_and(|n2|n2==n){break;}
            }}
        }
        cancel.cancel();
        let _ = dc.close().await;
    });
    Box::new(app)
}
fn attach(dc: Arc<RTCDataChannel>, a: Arc<Attempt>, initiator: bool) {
    let channel = dc.clone();
    dc.on_open(Box::new(move||{
        let dc=channel.clone();let a=a.clone();Box::pin(async move{
            if a.cancel.is_cancelled(){return;}
            let raw=match dc.detach().await{Ok(d)=>detached_io(d,a.cancel.child_token()),Err(_)=>{a.cancel.cancel();return}};
            if initiator{if a.ready_tx.try_send(raw).is_err(){a.cancel.cancel();}}
            else if let Some(c)=a.client.upgrade(){
                let result=tokio::select!{_=a.cancel.cancelled()=>return,r=c.secure_server(raw,"webrtc-direct")=>r};
                match result{Ok(p)=>{a.connected.store(true,Ordering::SeqCst);*p.rtc.lock().await=Some(a.clone());},Err(e)=>{c.report(&e);a.close().await;}}
            }
        })
    }));
}
impl Client {
    async fn new_attempt(
        self: &Arc<Self>,
        id: String,
        remote: String,
        initiator: bool,
        allocated: Option<&std::sync::Mutex<Option<Arc<Attempt>>>>,
    ) -> Result<Arc<Attempt>> {
        let rt = self.active().await?;
        if rt.attempts.lock().await.len() >= 32 {
            return Err(protocol("maximum 32 pending connections"));
        }
        let mut settings = SettingEngine::default();
        settings.detach_data_channels();
        settings.set_include_loopback_candidate(self.cfg.interface_name.is_none());
        settings.set_ice_timeouts(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(2)),
        );
        settings.set_network_types(vec![NetworkType::Udp4, NetworkType::Udp6]);
        if let Some(name) = self.cfg.interface_name.clone() {
            settings.set_interface_filter(Box::new(move |n| n == name));
        }
        let api = APIBuilder::new().with_setting_engine(settings).build();
        let config = RTCConfiguration {
            ice_servers: self
                .cfg
                .stun_servers
                .iter()
                .map(|s| RTCIceServer {
                    urls: vec![s.clone()],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let pc = Arc::new(
            api.new_peer_connection(config)
                .await
                .map_err(|_| transport("WebRTC creation failed"))?,
        );
        let (tx, rx) = mpsc::channel(1);
        let (signal_ready, _) = watch::channel(false);
        let a = Arc::new(Attempt {
            id: id.clone(),
            remote: remote.clone(),
            pc: pc.clone(),
            client: Arc::downgrade(self),
            runtime: Arc::downgrade(&rt),
            remote_ready: Mutex::new(false),
            candidates: Mutex::new(Vec::new()),
            ready_tx: tx,
            ready_rx: Mutex::new(rx),
            signal_ready,
            cancel: rt.cancel.child_token(),
            closed: AtomicBool::new(false),
            close_done: watch::channel(false).0,
            connected: AtomicBool::new(false),
        });
        // Install ownership before the first await after allocation: creation
        // failure, timeout and a dropped caller must all retire the connection.
        let mut cleanup = AttemptCleanup::new(a.clone());
        if let Some(allocated) = allocated {
            *allocated.lock().unwrap() = Some(a.clone());
        }
        rt.attempts.lock().await.insert(id, a.clone());
        let w = Arc::downgrade(&a);
        pc.on_ice_candidate(Box::new(move|candidate|{
            let a=w.upgrade();Box::pin(async move{let(Some(a),Some(candidate))=(a,candidate)else{return};let mut ready=a.signal_ready.subscribe();
                while !*ready.borrow(){tokio::select!{_=a.cancel.cancelled()=>return,r=ready.changed()=>if r.is_err(){return}}}
                if a.cancel.is_cancelled(){return;}
                if let(Some(c),Ok(candidate))=(a.client.upgrade(),candidate.to_json()){
                    match serde_json::to_value(candidate){Ok(payload)=>if let Err(e)=c.send_signal(&a.remote,&a.id,"candidate",payload).await{c.report(&e);},Err(_)=>c.report(&protocol("cannot encode candidate"))}
                }
            })
        }));
        let w = Arc::downgrade(&a);
        pc.on_peer_connection_state_change(Box::new(move |state| {
            let a = w.upgrade();
            Box::pin(async move {
                if matches!(
                    state,
                    RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
                ) {
                    if let Some(a) = a {
                        a.cancel.cancel();
                    }
                }
            })
        }));
        if initiator {
            let dc = pc
                .create_data_channel("cybergrid-v1", None)
                .await
                .map_err(|_| transport("cannot create DataChannel"))?;
            attach(dc, a.clone(), true);
        } else {
            let w = Arc::downgrade(&a);
            pc.on_data_channel(Box::new(move |dc| {
                let a = w.upgrade();
                Box::pin(async move {
                    if dc.label() != "cybergrid-v1" {
                        let _ = dc.close().await;
                        return;
                    }
                    if let Some(a) = a {
                        attach(dc, a, false);
                    }
                })
            }));
        }
        let t = a.clone();
        let lifetime = self
            .cfg
            .direct_timeout
            .saturating_mul(2)
            .saturating_add(Duration::from_secs(15));
        tokio::spawn(async move {
            tokio::select! {_=t.cancel.cancelled()=>{},_=tokio::time::sleep(lifetime)=>{if t.connected.load(Ordering::SeqCst){t.cancel.cancelled().await;}}}
            t.close().await;
        });
        cleanup.armed = false;
        Ok(a)
    }
    pub(crate) async fn dial_direct_fenced(
        self: &Arc<Self>,
        d: &Device,
        fence: &crate::peer::DialFence,
    ) -> Result<Arc<Peer>> {
        let allocated = std::sync::Mutex::new(None);
        let operation = tokio::time::timeout(self.cfg.direct_timeout, async {
            let a = self
                .new_attempt(
                    uuid::Uuid::new_v4().simple().to_string(),
                    d.id.clone(),
                    true,
                    Some(&allocated),
                )
                .await?;
            let _cleanup = AttemptCleanup::new(a.clone());
            let offer =
                a.pc.create_offer(None)
                    .await
                    .map_err(|_| transport("create offer failed"))?;
            a.pc.set_local_description(offer.clone())
                .await
                .map_err(|_| transport("local offer rejected"))?;
            self.send_signal(
                &a.remote,
                &a.id,
                "offer",
                serde_json::to_value(offer).map_err(|_| protocol("cannot encode offer"))?,
            )
            .await?;
            a.signal_ready.send_replace(true);
            let raw = tokio::select! {_=a.cancel.cancelled()=>return Err(transport("WebRTC connection closed")),r=async{a.ready_rx.lock().await.recv().await}=>r.ok_or_else(||transport("DataChannel did not open"))?};
            let p = self
                .secure_client_fenced(raw, d, "webrtc-direct", Some(fence))
                .await?;
            *p.rtc.lock().await = Some(a.clone());
            a.connected.store(true, Ordering::SeqCst);
            Ok(p)
        });
        let result = tokio::select! { _ = fence.cancel.cancelled() => Err(Error::Cancelled), result = operation => result.map_err(|_|Error::Timeout("WebRTC direct")).and_then(|v| v) };
        if result.is_err() {
            let attempt = allocated.lock().unwrap().take();
            if let Some(a) = attempt {
                a.close().await;
            }
        }
        result
    }
    pub(crate) async fn handle_signal(self: &Arc<Self>, s: Signal) -> Result<()> {
        if s.payload.len() > 65536
            || s.connection_id.is_empty()
            || s.connection_id.len() > 128
            || s.from_device_id.is_empty()
            || s.from_device_id.len() > 128
            || s.id == 0
        {
            return Err(protocol("invalid signal envelope"));
        }
        let rt = self.active().await?;
        let current = self
            .device_session()
            .await
            .ok_or(crate::Error::NotLoggedIn)?;
        if s.coordinator_epoch != current.coordinator_epoch
            || s.to_incarnation != current.incarnation
        {
            return Ok(());
        }
        let existing = rt.attempts.lock().await.get(&s.connection_id).cloned();
        if s.kind == "offer" {
            if existing.is_some() {
                return Ok(());
            }
            self.device(&s.from_device_id).await?;
            let offer: RTCSessionDescription =
                serde_json::from_str(&s.payload).map_err(|_| protocol("invalid offer JSON"))?;
            if offer.sdp_type != RTCSdpType::Offer {
                return Err(protocol("expected SDP offer"));
            }
            if rt.cancel.is_cancelled() || self.generation() != rt.generation {
                return Err(crate::Error::Cancelled);
            }
            let a = self
                .new_attempt(s.connection_id, s.from_device_id, false, None)
                .await?;
            let result: Result<()> = async {
                a.set_remote(offer).await?;
                let answer =
                    a.pc.create_answer(None)
                        .await
                        .map_err(|_| transport("cannot create answer"))?;
                a.pc.set_local_description(answer.clone())
                    .await
                    .map_err(|_| transport("local answer rejected"))?;
                self.send_signal(
                    &a.remote,
                    &a.id,
                    "answer",
                    serde_json::to_value(answer).map_err(|_| protocol("cannot encode answer"))?,
                )
                .await?;
                a.signal_ready.send_replace(true);
                Ok(())
            }
            .await;
            if result.is_err() {
                a.close().await;
            }
            return result;
        }
        let Some(a) = existing else { return Ok(()) };
        if a.remote != s.from_device_id {
            return Err(protocol("signaling sender mismatch"));
        }
        match s.kind.as_str() {
            "answer" => {
                let answer: RTCSessionDescription = serde_json::from_str(&s.payload)
                    .map_err(|_| protocol("invalid answer JSON"))?;
                if answer.sdp_type != RTCSdpType::Answer {
                    return Err(protocol("expected SDP answer"));
                }
                a.set_remote(answer).await
            }
            "candidate" => {
                let candidate: RTCIceCandidateInit =
                    serde_json::from_str(&s.payload).map_err(|_| protocol("invalid ICE JSON"))?;
                let ready = a.remote_ready.lock().await;
                if !*ready {
                    let mut candidates = a.candidates.lock().await;
                    if candidates.len() >= 128 {
                        return Err(protocol("candidate buffer exceeded"));
                    }
                    candidates.push(candidate);
                    Ok(())
                } else {
                    a.pc.add_ice_candidate(candidate)
                        .await
                        .map_err(|_| transport("remote candidate rejected"))
                }
            }
            "close" => {
                a.close().await;
                Ok(())
            }
            _ => Err(protocol("unknown signal kind")),
        }
    }
}

struct AttemptCleanup {
    attempt: Arc<Attempt>,
    armed: bool,
}
impl AttemptCleanup {
    fn new(attempt: Arc<Attempt>) -> Self {
        Self {
            attempt,
            armed: true,
        }
    }
}
impl Drop for AttemptCleanup {
    fn drop(&mut self) {
        if self.armed && !self.attempt.connected.load(Ordering::SeqCst) {
            self.attempt.cancel.cancel();
            let a = self.attempt.clone();
            tokio::spawn(async move {
                a.close().await;
            });
        }
    }
}
