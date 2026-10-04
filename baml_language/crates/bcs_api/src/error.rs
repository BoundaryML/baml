//! Native-client failures keep protocol bodies and credentials out of diagnostics.
use crate::{auth::HttpFailure, credentials::RenewalError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    #[error("Cannot {operation} Boundary login in the OS credential store")]
    Storage {
        operation: &'static str,
        #[source]
        source: keyring::Error,
    },
}

impl Error {
    pub fn status(&self) -> Option<reqwest::StatusCode> {
        match self {
            Self::Http(failure) => Some(failure.0),
            Self::Renewal(RenewalError::Rejected(status)) => Some(*status),
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
