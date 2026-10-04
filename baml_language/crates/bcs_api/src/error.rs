//! Native-client failures retain public API error fields and keep credentials out of diagnostics.
use crate::{auth::HttpFailure, credentials::RenewalError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No Boundary credential is available")]
    MissingCredentials,
    #[error("Login code expired; run `baml auth login` again")]
    LoginExpired,
    #[error("Login was denied in the browser")]
    LoginDenied,
    #[error(transparent)]
    Environment(#[from] baml_env::EnvError),
    #[error("Invalid BOUNDARY_API_URL")]
    Url(#[from] url::ParseError),
    #[error("Invalid Boundary endpoint: {0}")]
    Endpoint(&'static str),
    #[error("Boundary credential belongs to another endpoint")]
    EndpointMismatch,
    #[error("Invalid cloud query ID")]
    QueryId,
    #[error("Boundary transport failed")]
    Transport(#[source] reqwest::Error),
    #[error("Failed to read Boundary response")]
    Read(#[from] std::io::Error),
    #[error("{0}")]
    Http(#[from] HttpFailure),
    #[error(transparent)]
    Renewal(#[from] RenewalError),
    #[error("{0}")]
    Protocol(&'static str),
    #[error("Could not encode Boundary request")]
    Encode(#[from] serde_json::Error),
    #[error("Cannot {operation} Boundary login in the local OS credential store at {location}")]
    Storage {
        operation: &'static str,
        location: crate::auth::CredentialStoreLocation,
        #[source]
        source: keyring::Error,
    },
}

impl Error {
    pub fn status(&self) -> Option<reqwest::StatusCode> {
        match self {
            Self::Http(failure) | Self::Renewal(RenewalError::Rejected(failure)) => {
                Some(failure.status)
            }
            _ => None,
        }
    }
    pub fn http_failure(&self) -> Option<&HttpFailure> {
        match self {
            Self::Http(failure) | Self::Renewal(RenewalError::Rejected(failure)) => Some(failure),
            _ => None,
        }
    }
    pub(crate) fn transport(error: reqwest::Error) -> Self {
        Self::Transport(error.without_url())
    }
}

pub(crate) fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Protocol(message))
    }
}
