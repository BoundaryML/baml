//! Native cancellation sources retained without forwarding tasks or polling.

use std::{future::Future, pin::Pin, sync::Arc};

use tokio_util::sync::CancellationToken;

pub trait CancellationObserver: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone)]
pub enum CancellationSource {
    Token(CancellationToken),
    Observer(Arc<dyn CancellationObserver>),
}

impl std::fmt::Debug for CancellationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationSource")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl From<CancellationToken> for CancellationSource {
    fn from(token: CancellationToken) -> Self {
        Self::Token(token)
    }
}

impl CancellationSource {
    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Token(token) => token.is_cancelled(),
            Self::Observer(source) => source.is_cancelled(),
        }
    }

    pub async fn cancelled(&self) {
        match self {
            Self::Token(token) => token.cancelled().await,
            Self::Observer(source) => source.cancelled().await,
        }
    }
}
