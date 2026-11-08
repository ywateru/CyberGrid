use thiserror::Error;

/// Errors intentionally omit provider response bodies and all credentials.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("CyberGrid API {code} ({status})")]
    Api { status: u16, code: String },
    #[error("OAuth {code} ({status})")]
    OAuth { status: u16, code: String },
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("credential binding or storage failure: {0}")]
    Credentials(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("operation timed out: {0}")]
    Timeout(&'static str),
    #[error("not logged in or session unavailable")]
    NotLoggedIn,
    #[error("transport failed: {0}")]
    Transport(String),
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP request failed")]
    Http(#[from] reqwest::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub(crate) fn fatal(&self) -> bool {
        matches!(
            self,
            Self::Api {
                status: 401 | 403,
                ..
            }
        )
    }
}
pub(crate) fn protocol(s: impl Into<String>) -> Error {
    Error::Protocol(s.into())
}
pub(crate) fn transport(s: impl Into<String>) -> Error {
    Error::Transport(s.into())
}
