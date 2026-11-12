//! Public state contains fixed classifications and no credentials or provider errors.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatePhase {
    Unknown,
    Disconnected,
    Authorizing,
    Authenticated,
    Connecting,
    Ready,
    Recovering,
    RequiresLogin,
    Unavailable,
    Disabled,
    Pending,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateReason {
    Initial,
    AuthorizationRequested,
    AuthorizationUnavailable,
    ExchangeOutcomeUnknown,
    CoordinationReady,
    CoordinationLost,
    DirectoryUnavailable,
    AdmissionOutcomeUnknown,
    AdmissionExpired,
    LocalIntentAccepted,
    PublicationConfirmed,
    RemoteDisabled,
    TransportConnecting,
    TransportReady,
    TransportDisconnected,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerState {
    pub phase: StatePhase,
    pub reason: StateReason,
    pub retryable: bool,
}
impl LayerState {
    pub(crate) fn new(phase: StatePhase, reason: StateReason, retryable: bool) -> Self {
        Self {
            phase,
            reason,
            retryable,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BusinessAuthorization {
    #[default]
    Unknown,
    Authorized,
    Denied,
    Expired,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub revision: u64,
    pub generation: u64,
    pub epoch: Option<String>,
    pub identity: LayerState,
    pub coordination: LayerState,
    pub publication: LayerState,
    pub reachability: LayerState,
    /// Host assertion, never inferred from a working transport.
    pub business_authorization: BusinessAuthorization,
    pub remote_enabled: bool,
    pub intent_revision: u64,
    pub confirmed_intent_revision: Option<u64>,
    pub server_publish_revision: Option<u64>,
}
impl Default for StateSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            generation: 0,
            epoch: None,
            identity: LayerState::new(StatePhase::Disconnected, StateReason::Initial, false),
            coordination: LayerState::new(StatePhase::Disconnected, StateReason::Initial, false),
            publication: LayerState::new(StatePhase::Disabled, StateReason::RemoteDisabled, false),
            reachability: LayerState::new(StatePhase::Unknown, StateReason::Initial, false),
            business_authorization: BusinessAuthorization::Unknown,
            remote_enabled: false,
            intent_revision: 0,
            confirmed_intent_revision: None,
            server_publish_revision: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoordinatorRevocation {
    Confirmed,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDisableOutcome {
    pub local_closed: bool,
    pub coordinator_revocation: CoordinatorRevocation,
}
