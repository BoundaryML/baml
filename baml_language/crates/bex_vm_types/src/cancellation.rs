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

/// What a `baml.spawn.CancelToken` holds: it has fired once its own token or
/// any of `inputs` has. Cancelling fires only `own`, so cancelling a
/// `CancelToken.any` composite never reaches its inputs, and linking the token
/// into a plan links every one of them.
///
/// A composite keeps its inputs rather than forwarding them into a fresh token,
/// because a forwarder has nothing to end it: no signal marks the composite's
/// last clone being dropped, so a forwarding task holds every input until one
/// fires — for a long-lived input, for the life of the program. Holding them
/// also makes `is_cancelled` exact at once, where a forwarder answered only
/// after it had been scheduled.
#[derive(Debug)]
pub struct CancelTokenState {
    own: CancellationToken,
    /// Flattened: an input that is itself a composite contributes its tokens.
    inputs: Box<[CancellationSource]>,
}

impl CancelTokenState {
    /// A token that fires on `own` or any of `inputs`.
    pub fn new(own: CancellationToken, inputs: impl Into<Box<[CancellationSource]>>) -> Self {
        Self {
            own,
            inputs: inputs.into(),
        }
    }

    /// A fresh, un-cancelled token with no inputs.
    pub fn fresh() -> Self {
        Self::new(CancellationToken::new(), [])
    }

    /// Every source whose firing fires this token.
    pub fn sources(&self) -> impl Iterator<Item = CancellationSource> + '_ {
        std::iter::once(self.own.clone().into()).chain(self.inputs.iter().cloned())
    }

    pub fn is_cancelled(&self) -> bool {
        self.sources().any(|source| source.is_cancelled())
    }

    /// Fire this token's own source, never its inputs. Returns whether this
    /// call fired it: `false` when the token had already fired, its own way or
    /// through an input. Concurrent calls may both report `true`.
    pub fn cancel(&self) -> bool {
        let was_cancelled = self.is_cancelled();
        self.own.cancel();
        !was_cancelled
    }

    /// The bytes it holds beyond the tokens, which it shares with the tasks
    /// they cancel.
    pub fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(size_of_val(&*self.inputs));
    }
}
