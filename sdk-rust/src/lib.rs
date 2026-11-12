//! Native Rust CyberGrid SDK. Business bytes travel only between endpoints.
mod auth;
mod capabilities;
mod client;
mod credentials;
mod endpoints;
mod error;
mod iroh_direct;
mod mux;
mod peer;
mod provider;
mod rtc;
mod secure;
mod state;
mod tunnel;
mod types;

pub use auth::{
    AuthConfig, BrowserGrantCheckpoint, Coordination, DeviceAuthorization, GrantImportOutcome,
    OAuthTokens, PublicProduct, RegistrationAdmission,
};
pub use client::{AuthFuture, AuthProvider, AuthRequestReason, AuthTokenRequest, Client};
pub use credentials::CredentialStore;
pub use endpoints::{
    BridgeLimits, BrowserBridge, DirectorySnapshot, EndpointPolicy, EndpointProvider,
};
pub use error::{Error, Result};
pub use peer::{Mapping, Peer};
pub use state::{
    BusinessAuthorization, CoordinatorRevocation, LayerState, RemoteDisableOutcome, StatePhase,
    StateReason, StateSnapshot,
};
pub use tokio_util::sync::CancellationToken;
pub use tunnel::validate_tunnel_url;
pub use types::{
    Application, CloudflaredConfig, Config, Device, DirectStrategy, Event, IrohInfo, Port,
    ProviderEnvironment, Session, User,
};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
