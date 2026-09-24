//! Local profile-store providers for `baml_query`
//! (TASK/baml-query-scope.md §3.1, §5).
//!
//! Binds one `.baml/profiles-v1` store into a fixed snapshot (§5.6),
//! serves the catalog-v1 relations from the stream readers through a
//! bounded per-execution fold cache (§5.4), and resolves virtual value
//! handles from the CAS (§5.5, B4). Never depends on
//! `bex_engine`/`bex_vm` — evidence reads go through `bex_prof_store`.

// The session constructors and the resolver are the crate's public
// surface; everything else is provider plumbing. Keeping the store
// types (`bex_prof_store` readers, folds, row builders) crate-private
// stops a future consumer (the playground endpoint) from walking the
// store around the QuerySession's snapshot/budget/authorization seam.
pub mod decode;
mod fold;
mod relations;
mod resolver;
mod universe;

use std::path::Path;

use baml_query::{
    budget::{CancellationToken, QueryBudgets},
    capability::CapabilityRegistry,
    catalog::CatalogProfile,
    error::QueryError,
    scope::QueryScope,
    session::{QuerySession, QuerySessionBuilder},
};
pub use resolver::{ProfilesResolver, loss_unavailability};
use universe::ProfilesUniverse;

/// Bind a `profiles-v1` store and build a ready query session with the
/// public catalog profile and unlimited budgets.
pub async fn profiles_session(store_root: &Path) -> Result<QuerySession, QueryError> {
    profiles_session_with(
        store_root,
        CatalogProfile::public(),
        QueryBudgets::unlimited(),
        CancellationToken::new(),
    )
    .await
}

/// [`profiles_session`] with an explicit profile, budgets, and
/// cancellation.
pub async fn profiles_session_with(
    store_root: &Path,
    profile: CatalogProfile,
    budgets: QueryBudgets,
    cancel: CancellationToken,
) -> Result<QuerySession, QueryError> {
    let universe = ProfilesUniverse::bind(store_root)?;
    let folds = fold::FoldCache::new(budgets.max_fold_bytes);
    let resolver = ProfilesResolver::new(store_root.to_path_buf());
    let factory = relations::ProfilesFactory::new(universe.clone(), folds);
    QuerySessionBuilder::new(
        profile,
        QueryScope::local(),
        universe.snapshot(),
        resolver,
        factory,
    )
    .with_budgets(budgets)
    .with_cancellation(cancel)
    .with_capabilities(CapabilityRegistry::new())
    .build()
    .await
}

/// Materialize canonical physical rows for a trusted evidence projector.
/// Values remain opaque handles; this does not evaluate user SQL or decode CAS bodies.
pub fn projection_batches(
    store_root: &Path,
    max_fold_bytes: u64,
) -> Result<Vec<datafusion::arrow::record_batch::RecordBatch>, QueryError> {
    let universe = ProfilesUniverse::bind(store_root)?;
    let folds = fold::FoldCache::new(max_fold_bytes);
    CatalogProfile::public()
        .relations()
        .iter()
        .map(|relation| relations::build_relation_batch(relation, &universe, &folds, None))
        .collect()
}
