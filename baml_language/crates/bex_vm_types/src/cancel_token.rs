//! The runtime value behind a `baml.spawn.CancelToken`.
//!
//! A `CancelToken` instance holds an `Object::RustData` whose payload is an
//! `Arc<CancelTokenData>`. The data wraps a Tokio [`CancellationToken`] and
//! remembers the tokens it was composed from (`CancelToken.any`). A plain
//! `CancellationToken` has no identity and does not expose its links, so a
//! heap snapshot could neither tell two handles of one token apart from two
//! tokens nor rebuild the link from a composite to its inputs. The `Arc`
//! address is the identity, and `sources` is the link list.
//!
//! A composite fires when any of its sources fires. The link is one watcher
//! task per source ([`CancelTokenData::watchers`]); this crate has no
//! executor, so the caller spawns them.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// Payload of a `baml.spawn.CancelToken`.
#[derive(Debug)]
pub struct CancelTokenData {
    token: CancellationToken,
    /// Tokens whose cancellation cancels this one. Empty for a token from
    /// `CancelToken.new()`. The relation is one-directional and a composite
    /// is built from tokens that already exist, so it has no cycles.
    sources: Vec<Arc<CancelTokenData>>,
}

impl CancelTokenData {
    /// A fresh token that nothing else cancels.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            token: CancellationToken::new(),
            sources: Vec::new(),
        })
    }

    /// A token that fires when any of `sources` fires. The caller must spawn
    /// [`Self::watchers`] for the link to take effect.
    #[must_use]
    pub fn composite(sources: Vec<Arc<CancelTokenData>>) -> Arc<Self> {
        Arc::new(Self {
            token: CancellationToken::new(),
            sources,
        })
    }

    /// Rebuild a token from a snapshot. A cancelled token stays cancelled.
    /// The caller must spawn [`Self::watchers`].
    #[must_use]
    pub fn restored(cancelled: bool, sources: Vec<Arc<CancelTokenData>>) -> Arc<Self> {
        let data = Self::composite(sources);
        if cancelled {
            data.token.cancel();
        }
        data
    }

    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    #[must_use]
    pub fn sources(&self) -> &[Arc<CancelTokenData>] {
        &self.sources
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// One future per source that cancels this token when the source fires.
    /// Each ends when this token is cancelled, so none outlives the link.
    /// Empty when the token is already cancelled.
    #[must_use]
    pub fn watchers(&self) -> Vec<impl std::future::Future<Output = ()> + Send + 'static> {
        if self.token.is_cancelled() {
            return Vec::new();
        }
        self.sources
            .iter()
            .map(|source| link(source.token.clone(), self.token.clone()))
            .collect()
    }
}

/// Resolves after cancelling `target` because `source` fired, or when
/// `target` was cancelled by someone else.
pub async fn link(source: CancellationToken, target: CancellationToken) {
    tokio::select! {
        biased;
        () = source.cancelled() => target.cancel(),
        () = target.cancelled() => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_composite_fires_with_a_source_and_not_the_other_way_round() {
        let a = CancelTokenData::new();
        let b = CancelTokenData::new();
        let any = CancelTokenData::composite(vec![Arc::clone(&a), Arc::clone(&b)]);
        for watcher in any.watchers() {
            tokio::spawn(watcher);
        }
        let other = CancelTokenData::composite(vec![Arc::clone(&a)]);
        for watcher in other.watchers() {
            tokio::spawn(watcher);
        }
        other.token().cancel();
        tokio::task::yield_now().await;
        assert!(!a.is_cancelled(), "a composite does not cancel its sources");

        b.token().cancel();
        any.token().cancelled().await;
        assert!(!a.is_cancelled());
    }

    #[tokio::test]
    async fn a_restored_cancelled_token_needs_no_watchers() {
        let source = CancelTokenData::new();
        let restored = CancelTokenData::restored(true, vec![source]);
        assert!(restored.is_cancelled());
        assert!(restored.watchers().is_empty());
    }
}
