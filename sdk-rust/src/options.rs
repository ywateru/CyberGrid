use crate::{DirectStrategy, Error, Result, Transport};
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Per-operation control. A deadline bounds setup, never the lifetime of a ready stream.
#[derive(Clone, Debug, Default)]
pub struct OperationOptions {
    pub deadline: Option<Instant>,
    pub cancel: Option<CancellationToken>,
}
/// Config supplies defaults. Overrides affect only this attempt.
#[derive(Clone, Debug, Default)]
pub struct ConnectOptions {
    pub operation: OperationOptions,
    pub direct_strategy: Option<DirectStrategy>,
    pub direct_timeout: Option<Duration>,
    pub iroh_timeout: Option<Duration>,
    pub fallback_timeout: Option<Duration>,
    /// Diagnostic mode: attempt exactly this path, without implicit fallback.
    pub only_transport: Option<Transport>,
}
#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub operation: OperationOptions,
    pub port_open_timeout: Duration,
}
impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            operation: OperationOptions::default(),
            port_open_timeout: Duration::from_secs(10),
        }
    }
}
impl ConnectOptions {
    pub(crate) fn validate(&self) -> Result<()> {
        if [
            self.direct_timeout,
            self.iroh_timeout,
            self.fallback_timeout,
        ]
        .into_iter()
        .flatten()
        .any(|t| t.is_zero())
        {
            return Err(Error::Config(
                "connection phase timeouts must be positive".into(),
            ));
        }
        Ok(())
    }
}
