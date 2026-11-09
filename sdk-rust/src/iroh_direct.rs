//! Direct-IP QUIC only. The coordinator supplies addresses, never business data.
use crate::{
    client::{Client, Runtime},
    error::{protocol, transport},
    mux::BoxIo,
    Device, Error, IrohInfo, Peer, Result,
};
use iroh::{
    endpoint::{
        Builder, Connection, PortmapperConfig, QuicTransportConfig, RecvStream, SendStream, VarInt,
    },
    Endpoint, EndpointAddr, RelayMode, TransportAddr,
};
use std::{
    io,
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{Mutex, Semaphore},
};

const ALPN: &[u8] = b"cybergrid/iroh-direct/1";
pub(crate) struct DirectEndpoint {
    pub endpoint: Endpoint,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    handshakes: Arc<Semaphore>,
}
impl DirectEndpoint {
    pub async fn bind() -> Result<Arc<Self>> {
        // Empty means no N0 preset, relay maps, pkarr, DNS lookup or publisher.
        // Explicit settings protect this policy from changes in upstream defaults.
        let endpoint = Builder::empty()
            .crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .relay_mode(RelayMode::Disabled)
            .clear_address_lookup()
            .portmapper_config(PortmapperConfig::Disabled)
            .net_report_config(iroh::NetReportConfig::minimal())
            .alpns(vec![ALPN.to_vec()])
            .transport_config(
                QuicTransportConfig::builder()
                    .max_concurrent_bidi_streams(VarInt::from_u32(1))
                    .max_concurrent_uni_streams(VarInt::from_u32(0))
                    .datagram_receive_buffer_size(None)
                    .receive_window(VarInt::from_u32(1 << 20))
                    .stream_receive_window(VarInt::from_u32(1 << 20))
                    .send_window(1 << 20)
                    .keep_alive_interval(Duration::from_secs(5))
                    .max_idle_timeout(Some(
                        Duration::from_secs(30)
                            .try_into()
                            .map_err(|_| protocol("invalid QUIC idle timeout"))?,
                    ))
                    .build(),
            )
            .bind()
            .await
            .map_err(|_| transport("direct iroh UDP endpoint unavailable"))?;
        Ok(Arc::new(Self {
            endpoint,
            task: Mutex::new(None),
            handshakes: Arc::new(Semaphore::new(16)),
        }))
    }
    pub fn info(&self) -> IrohInfo {
        let mut addresses: Vec<_> = self
            .endpoint
            .addr()
            .ip_addrs()
            .copied()
            .filter_map(crate::types::canonical_iroh_address)
            .collect();
        addresses.sort();
        addresses.dedup();
        addresses.truncate(8);
        IrohInfo {
            endpoint_id: hex::encode(self.endpoint.id().as_bytes()),
            addresses,
        }
    }
    pub async fn close(&self) {
        self.endpoint.close().await;
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }
}

// One QUIC byte stream carries the unchanged inner TLS + smux protocol. Retain
// the connection until both halves drop, including failed/cancelled TLS setup.
struct QuicIo {
    send: SendStream,
    recv: RecvStream,
    connection: Connection,
}
impl AsyncRead for QuicIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}
impl AsyncWrite for QuicIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.send), cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_shutdown(cx)
    }
}
impl Drop for QuicIo {
    fn drop(&mut self) {
        let _ = self.send.reset(VarInt::from_u32(0));
        let _ = self.recv.stop(VarInt::from_u32(0));
        self.connection
            .close(VarInt::from_u32(0), b"byte stream closed");
    }
}
fn direct(connection: &Connection) -> bool {
    let paths = connection.paths();
    paths.iter().any(|p| p.is_selected() && p.is_ip()) && paths.iter().all(|p| !p.is_relay())
}
async fn attach(peer: &Arc<Peer>, connection: Connection) -> Result<()> {
    let mut slot = peer.iroh.lock().await;
    if peer.is_closed() {
        connection.close(VarInt::from_u32(0), b"peer closed");
        return Err(Error::Cancelled);
    }
    *slot = Some(connection);
    Ok(())
}
impl Client {
    pub(crate) async fn spawn_iroh(self: &Arc<Self>, rt: Arc<Runtime>) {
        let Some(owner) = rt.iroh.clone() else { return };
        let client = Arc::downgrade(self);
        let endpoint = owner.endpoint.clone();
        let permits = owner.handshakes.clone();
        let cancel = rt.cancel.clone();
        let generation = rt.generation;
        let budget = self.cfg.iroh_timeout;
        let task = tokio::spawn(async move {
            let mut work = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = work.join_next(), if !work.is_empty() => {},
                    incoming = endpoint.accept() => {
                        let Some(incoming) = incoming else { break };
                        let Ok(permit) = permits.clone().try_acquire_owned() else { incoming.refuse(); continue };
                        let Some(c) = client.upgrade() else { incoming.refuse(); break };
                        let stop = cancel.clone();
                        work.spawn(async move {
                            let _permit = permit;
                            let operation = async {
                                let conn = incoming.await.map_err(|_| transport("direct QUIC handshake rejected"))?;
                                let guard = ConnectionGuard(Some(conn.clone()));
                                if !direct(&conn) { return Err(protocol("iroh requires a direct IP path")); }
                                // An outer QUIC public key alone never grants namespace access.
                                let remote = hex::encode(conn.remote_id().as_bytes());
                                let device = c.devices().await?.into_iter().find(|d| d.online && d.iroh.as_ref().is_some_and(|i| i.endpoint_id == remote))
                                    .ok_or_else(|| protocol("iroh endpoint outside authenticated namespace"))?;
                                if c.generation() != generation { return Err(Error::Cancelled); }
                                let (send, recv) = conn.accept_bi().await.map_err(|_| transport("direct QUIC stream unavailable"))?;
                                let raw: BoxIo = Box::new(QuicIo { send, recv, connection: conn.clone() });
                                let peer = c.secure_server_bound(raw, "iroh-direct", Some(&device), Some(generation)).await?;
                                if c.generation() != generation || stop.is_cancelled() { peer.close().await; return Err(Error::Cancelled); }
                                attach(&peer, conn).await?;
                                guard.disarm();
                                Ok(())
                            };
                            tokio::select! { _ = stop.cancelled() => {}, _ = tokio::time::timeout(budget, operation) => {} }
                        });
                    }
                }
            }
            // Cancelling owned handshakes drops their raw streams and connection guards.
            work.abort_all();
            while work.join_next().await.is_some() {}
            endpoint.close().await;
        });
        *owner.task.lock().await = Some(task);
    }

    #[cfg(test)]
    pub(crate) async fn dial_iroh(self: &Arc<Self>, device: &Device) -> Result<Arc<Peer>> {
        let fence = crate::peer::DialFence::single(
            self.closed_token().child_token(),
            self.active().await?.generation,
        );
        self.dial_iroh_fenced(device, &fence).await
    }
    pub(crate) async fn dial_iroh_fenced(
        self: &Arc<Self>,
        device: &Device,
        fence: &crate::peer::DialFence,
    ) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        if fence.generation != rt.generation || fence.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let owner = rt
            .iroh
            .as_ref()
            .ok_or_else(|| transport("direct iroh disabled or unavailable"))?;
        let info = device
            .iroh
            .as_ref()
            .ok_or_else(|| transport("peer has no direct iroh metadata"))?;
        info.validate()?;
        if info.addresses.is_empty() {
            return Err(transport("peer has no direct iroh addresses"));
        }
        let endpoint_id = iroh::EndpointId::from_str(&info.endpoint_id)
            .map_err(|_| protocol("invalid iroh endpoint identity"))?;
        let address = EndpointAddr::from_parts(
            endpoint_id,
            info.addresses.iter().copied().map(TransportAddr::Ip),
        );
        let operation = async {
            let conn = owner
                .endpoint
                .connect(address, ALPN)
                .await
                .map_err(|_| transport("direct iroh connection failed"))?;
            let guard = ConnectionGuard(Some(conn.clone()));
            if conn.remote_id() != endpoint_id || !direct(&conn) {
                return Err(protocol("direct iroh identity/path rejected"));
            }
            let (send, recv) = conn
                .open_bi()
                .await
                .map_err(|_| transport("direct iroh stream unavailable"))?;
            let raw: BoxIo = Box::new(QuicIo {
                send,
                recv,
                connection: conn.clone(),
            });
            let peer = self
                .secure_client_fenced(raw, device, "iroh-direct", Some(fence))
                .await?;
            if rt.cancel.is_cancelled() || self.generation() != rt.generation {
                peer.close().await;
                return Err(Error::Cancelled);
            }
            attach(&peer, conn).await?;
            guard.disarm();
            Ok(peer)
        };
        tokio::select! { _ = rt.cancel.cancelled() => Err(Error::Cancelled), _ = fence.cancel.cancelled() => Err(Error::Cancelled), result = tokio::time::timeout(self.cfg.iroh_timeout, operation) => result.map_err(|_| Error::Timeout("direct iroh handshake"))? }
    }
}
struct ConnectionGuard(Option<Connection>);
impl ConnectionGuard {
    fn disarm(mut self) {
        self.0.take();
    }
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.take() {
            c.close(VarInt::from_u32(0), b"setup ended");
        }
    }
}
