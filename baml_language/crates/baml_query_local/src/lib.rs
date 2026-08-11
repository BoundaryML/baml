//! Local Project Studio query providers (Q2).
//!
//! Binds one `.baml` tree into a fixed query universe (D10), serves the
//! catalog-v1 relations directly from canonical artifacts through the
//! fold reader, and resolves virtual value handles from the canonical
//! CAS. No second codec, CID space, or value model exists here — reads
//! go through `bex_events` canon and `bex_query` readers.
//!
//! Physical strategy (benchmark-owned, not architecture): relations
//! materialize as in-memory Arrow batches at first use within a bound
//! session. Provider state is fully rebuildable by construction — there
//! is nothing on disk to delete.

pub mod relations;
pub mod resolver;
pub mod universe;

use std::sync::Arc;

use baml_query::budget::{CancellationToken, QueryBudgets};
use baml_query::capability::CapabilityRegistry;
use baml_query::error::QueryError;
use baml_query::scope::QueryScope;
use baml_query::session::{QuerySession, QuerySessionBuilder};

pub use resolver::LocalValueResolver;
pub use universe::LocalUniverse;

/// Bind a `.baml` tree and build a ready query session.
pub fn local_session(baml_dir: &std::path::Path) -> Result<QuerySession, QueryError> {
    local_session_with(
        baml_dir,
        QueryBudgets::unlimited(),
        CancellationToken::new(),
    )
}

/// [`local_session`] with explicit budgets/cancellation.
pub fn local_session_with(
    baml_dir: &std::path::Path,
    budgets: QueryBudgets,
    cancel: CancellationToken,
) -> Result<QuerySession, QueryError> {
    let universe = Arc::new(LocalUniverse::bind(baml_dir)?);
    let resolver = Arc::new(LocalValueResolver::new(universe.clone()));
    let factory = Arc::new(relations::LocalProviderFactory::new(universe.clone()));
    QuerySessionBuilder::new(
        baml_query::catalog::catalog_v1(),
        QueryScope::local(),
        universe.snapshot(),
        resolver,
        factory,
    )
    .with_budgets(budgets)
    .with_cancellation(cancel)
    .with_capabilities(CapabilityRegistry::new())
    .build()
}
