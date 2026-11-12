use super::*;
use crate::gateway_host::GatewayEndpointState;
use chrono::{DateTime, SecondsFormat, Utc};
use cybergrid_sdk::{BrowserBridge, StatePhase};
use tokio::{
    sync::watch,
    time::{Duration, Instant},
};
const BRIDGE_ID: &str = "workspace";
const MAX_SAFE: u64 = 9_007_199_254_740_991;
pub(crate) struct AddressRecord {
    scope: (String, String),
    descriptor: BrowserBridge,
    deadline: Instant,
    action: u64,
}
impl AddressRecord {
    fn acknowledge(&mut self, candidate: Instant) -> Result<(), &'static str> {
        if Instant::now() >= self.deadline || Instant::now() >= candidate {
            return Err("PUBLICATION_EXPIRED");
        }
        self.deadline = candidate;
        Ok(())
    }
}
struct PublicationOwnerGuard {
    host: std::sync::Weak<Host>,
    action: u64,
}
impl Drop for PublicationOwnerGuard {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            if host.is_generation(self.action) {
                host.seal_generation_with_reason(
                    self.action,
                    AutomaticCloseReason::PublicationOwnerEnded,
                );
                let owner = host.clone();
                host.runtime.spawn(async move {
                    let _ = owner.disable().await;
                });
            }
        }
    }
}
impl Host {
    pub(super) async fn publish_initial(
        self: &Arc<Self>,
        client: &Arc<Client>,
        runtime: &mut GatewayRuntime,
        _sdk: u64,
        action: u64,
        cancel: &CancellationToken,
    ) -> Result<(), &'static str> {
        let endpoint = match runtime.endpoint_snapshot()? {
            GatewayEndpointState::Available(e) if e.revision > 0 && e.revision <= MAX_SAFE => e,
            _ => return Err("GATEWAY_ENDPOINT_UNAVAILABLE"),
        };
        let identity = runtime.lease.identity();
        let caps = runtime.lease.capabilities();
        if caps.protocol != "anywork.remote.v1"
            || caps.limits.max_streams > 2
            || caps.limits.max_pending_requests > 8
        {
            return Err("GATEWAY_CAPABILITY_MISMATCH");
        }
        let descriptor = BrowserBridge {
            id: BRIDGE_ID.into(),
            protocol: caps.protocol,
            provider_id: endpoint.provider_id,
            url: endpoint.url,
            installation_id: identity.installation_id,
            public_key: identity.public_key,
            address_revision: 1,
            expires_at: String::new(),
            limits: caps.limits,
        };
        self.publish_round(client, descriptor, action, cancel, true)
            .await
    }
    async fn publish_round(
        self: &Arc<Self>,
        client: &Arc<Client>,
        mut descriptor: BrowserBridge,
        action: u64,
        cancel: &CancellationToken,
        enable: bool,
    ) -> Result<(), &'static str> {
        let session = client.device_session().await.ok_or("NO_SESSION")?;
        let product = session.product_id.as_deref().unwrap_or(&session.app_id);
        if self.config.product.is_empty() || product != self.config.product {
            return Err("PRODUCT_MISMATCH");
        }
        let config = client
            .auth_config()
            .await
            .map_err(|_| "POLICY_UNAVAILABLE")?;
        let policy = config.endpoint_policy.as_ref().ok_or("POLICY_REQUIRED")?;
        let started = Instant::now();
        let sample = client
            .directory(None)
            .await
            .map_err(|_| "DIRECTORY_UNAVAILABLE")?;
        if sample.product_id != product || sample.epoch != session.coordinator_epoch {
            return Err("SCOPE_CHANGED");
        }
        descriptor.expires_at = (sample.sampled_at + chrono::Duration::seconds(180))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let scope = (
            session.coordinator_epoch.clone(),
            session.incarnation.clone(),
        );
        {
            let mut history = self.bridge_history.lock().unwrap();
            descriptor.address_revision =
                if let Some(old) = history.as_ref().filter(|v| v.scope == scope) {
                    let mut a = descriptor.clone();
                    let mut b = old.descriptor.clone();
                    a.expires_at.clear();
                    b.expires_at.clear();
                    a.address_revision = 0;
                    b.address_revision = 0;
                    if a == b {
                        old.descriptor.address_revision
                    } else {
                        old.descriptor
                            .address_revision
                            .checked_add(1)
                            .filter(|n| *n <= MAX_SAFE)
                            .ok_or("ADDRESS_REVISION_EXHAUSTED")?
                    }
                } else {
                    1
                };
            descriptor
                .validate(Some(policy))
                .map_err(|_| "INVALID_GATEWAY_DESCRIPTOR")?;
            // Remember accepted local identity even if its network result is unknown.
            let old_deadline = history
                .as_ref()
                .filter(|v| v.action == action)
                .map(|v| v.deadline);
            *history = Some(AddressRecord {
                scope: scope.clone(),
                descriptor: descriptor.clone(),
                deadline: old_deadline.unwrap_or(started + Duration::from_secs(60)),
                action,
            });
        }
        if cancel.is_cancelled() || !self.is_generation(action) {
            return Err("CANCELLED");
        }
        client
            .publish_bridge(descriptor.clone())
            .await
            .map_err(|_| "PUBLICATION_UNAVAILABLE")?;
        if enable {
            client
                .enable_remote()
                .await
                .map_err(|_| "NATIVE_ENABLE_FAILED")?;
        }
        // Pin an acknowledged publication before requesting its directory
        // confirmation. The SDK's independent heartbeat can acknowledge a
        // later revision while this finite directory response is in flight.
        let acknowledged = client.snapshot();
        let expected_intent = acknowledged.intent_revision;
        let minimum_revision = acknowledged
            .server_publish_revision
            .filter(|_| {
                acknowledged.publication.phase == StatePhase::Ready
                    && acknowledged.confirmed_intent_revision == Some(expected_intent)
            })
            .ok_or("PUBLICATION_UNCONFIRMED")?;
        let confirmed_start = Instant::now();
        let directory = client
            .directory(None)
            .await
            .map_err(|_| "DIRECTORY_UNAVAILABLE")?;
        let latest = client.device_session().await.ok_or("NO_SESSION")?;
        let snapshot = client.snapshot();
        let found = directory
            .devices
            .iter()
            .find(|d| {
                d.id == session.device_id
                    && d.incarnation == session.incarnation
                    && d.online
                    && d.publish_revision >= minimum_revision
            })
            .and_then(|d| d.bridges.iter().find(|b| **b == descriptor));
        if cancel.is_cancelled()
            || !self.is_generation(action)
            || latest.incarnation != session.incarnation
            || latest.coordinator_epoch != session.coordinator_epoch
            || directory.product_id != product
            || directory.epoch != session.coordinator_epoch
            || snapshot.publication.phase != StatePhase::Ready
            || snapshot.confirmed_intent_revision != Some(expected_intent)
            || snapshot.intent_revision != expected_intent
            || snapshot
                .server_publish_revision
                .is_none_or(|revision| revision < minimum_revision)
            || found.is_none()
        {
            return Err("PUBLICATION_UNCONFIRMED");
        }
        let expires: DateTime<Utc> = DateTime::parse_from_rfc3339(&descriptor.expires_at)
            .map_err(|_| "INVALID_EXPIRY")?
            .with_timezone(&Utc);
        let remaining = (expires - directory.sampled_at)
            .to_std()
            .map_err(|_| "PUBLICATION_EXPIRED")?;
        let deadline = (started + Duration::from_secs(180))
            .min(confirmed_start + remaining)
            .checked_sub(Duration::from_secs(1))
            .ok_or("PUBLICATION_EXPIRED")?;
        if Instant::now() >= deadline {
            return Err("PUBLICATION_EXPIRED");
        }
        let mut history = self.bridge_history.lock().unwrap();
        let old = history
            .as_mut()
            .filter(|v| v.action == action && v.scope == scope && v.descriptor == descriptor)
            .ok_or("STALE_PUBLICATION")?;
        if cancel.is_cancelled() || !self.is_generation(action) {
            return Err("CANCELLED");
        }
        // A late ACK cannot win a scheduler race against the independent watchdog.
        old.acknowledge(deadline)
    }
    pub(super) fn start_publisher(
        self: &Arc<Self>,
        client: Arc<Client>,
        mut endpoints: watch::Receiver<GatewayEndpointState>,
        _sdk: u64,
        action: u64,
        cancel: CancellationToken,
    ) {
        let initial = endpoints.borrow_and_update().clone();
        let host = self.clone();
        let guard_cancel = cancel.clone();
        // Independent of a slow publish request and of every browser/JS poll.
        self.runtime.spawn(async move {
            let _owner = PublicationOwnerGuard {
                host: Arc::downgrade(&host),
                action,
            };
            let reason = loop {
                let deadline = host
                    .bridge_history
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|v| v.action == action)
                    .map(|v| v.deadline);
                let Some(deadline) = deadline else {
                    break AutomaticCloseReason::PublicationOwnerEnded;
                };
                tokio::select! { biased;
                    _ = guard_cancel.cancelled() => break AutomaticCloseReason::PublicationOwnerEnded,
                    r = endpoints.changed() => {
                        if r.is_ok() && *endpoints.borrow_and_update() == initial { continue; }
                        break if r.is_err() { AutomaticCloseReason::endpoint(None) } else { AutomaticCloseReason::endpoint(Some(&endpoints.borrow_and_update())) };
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        let renewed = host.bridge_history.lock().unwrap().as_ref()
                            .is_some_and(|v| v.action == action && v.deadline > Instant::now());
                        if renewed { continue; } break AutomaticCloseReason::PublicationExpired;
                    }
                }
            };
            if host.is_generation(action) {
                host.seal_generation_with_reason(action, reason);
                let _ = host.disable().await;
            }
        });
        let host = self.clone();
        self.runtime.spawn(async move {
            let _owner = PublicationOwnerGuard {
                host: Arc::downgrade(&host),
                action,
            };
            loop {
                tokio::select! { biased; _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {} }
                let descriptor = host
                    .bridge_history
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|v| v.action == action)
                    .map(|v| v.descriptor.clone());
                let Some(descriptor) = descriptor else {
                    break;
                };
                // Await the actual dispatched operation to completion, including after seal.
                if let Err(error) = host
                    .publish_round(&client, descriptor, action, &cancel, false)
                    .await
                {
                    if host.is_generation(action) {
                        host.seal_generation_with_reason(
                            action,
                            AutomaticCloseReason::PublicationFailure(error),
                        );
                        let _ = host.disable().await;
                    }
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn actual_watchdog_expiry_closes_without_browser_poll_and_preserves_reason() {
        let host = Host::new(BuildConfig::default(), None).await.unwrap();
        let reservation = host.reserve_enable().unwrap();
        let action = reservation.generation;
        let client = host.client().unwrap();
        // Real watchdog timer; no altered clock or published/network readiness claim.
        *host.bridge_history.lock().unwrap() = Some(AddressRecord {
            scope: ("fixture".into(), "fixture".into()),
            action,
            descriptor: serde_json::from_value(serde_json::json!({
                "id":"workspace", "protocol":"anywork.remote.v1", "providerId":"quick-tunnel",
                "url":"wss://fixture.trycloudflare.com/remote/v1/ws", "installationId":"fixture",
                "publicKey":"fixture", "addressRevision":1, "expiresAt":"",
                "limits":{"maxFrameBytes":4096,"maxStreams":2,"maxPendingRequests":8}
            }))
            .unwrap(),
            deadline: Instant::now() + Duration::from_millis(20),
        });
        host.gate.store(true, Ordering::SeqCst);
        let (_sender, endpoints) = watch::channel(GatewayEndpointState::Pending);
        host.start_publisher(client, endpoints, 0, action, reservation.cancel.clone());
        let state = host.state.lock().await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while host.gate.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            host.remote_control
                .lock()
                .unwrap()
                .close_reason
                .map(AutomaticCloseReason::code),
            Some("PUBLICATION_EXPIRED")
        );
        drop(state);
        host.disable().await.unwrap();
        assert_eq!(host.view().await.error, Some("PUBLICATION_EXPIRED"));
        assert!(reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(host.local.origin())
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
    }
    #[test]
    fn delayed_success_cannot_revive_an_expired_deadline_before_watchdog_runs() {
        let now = Instant::now();
        let mut r = AddressRecord {
            scope: ("epoch".into(), "incarnation".into()),
            action: 1,
            descriptor: BrowserBridge {
                id: "workspace".into(),
                protocol: "anywork.remote.v1".into(),
                provider_id: "quick-tunnel".into(),
                url: "wss://example.trycloudflare.com/remote/v1/ws".into(),
                installation_id: "fixture".into(),
                public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                address_revision: 1,
                expires_at: String::new(),
                limits: cybergrid_sdk::BridgeLimits {
                    max_frame_bytes: 4096,
                    max_streams: 2,
                    max_pending_requests: 8,
                },
            },
            deadline: now - Duration::from_secs(1),
        };
        let original = r.deadline;
        assert_eq!(
            r.acknowledge(now + Duration::from_secs(180)),
            Err("PUBLICATION_EXPIRED")
        );
        assert_eq!(r.deadline, original);
        r.deadline = now + Duration::from_secs(20);
        r.acknowledge(now + Duration::from_secs(100)).unwrap();
        assert_eq!(r.deadline, now + Duration::from_secs(100));
    }
}
