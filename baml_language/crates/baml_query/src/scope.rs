//! `QueryScope` and snapshot binding (D10).
//!
//! Every ordinary query binds, before planning: the catalog version, the
//! provider generation, the projected-through barrier, the authorization
//! scope, and provider-specific snapshot handles. Later commits are
//! invisible; SQL can never widen the scope.

use serde::Serialize;

/// Which engine family executes resident work for this session. Backend
/// capability checks (D4) compare a function's requirement against this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Local providers over canonical artifacts / rebuildable stores.
    Local,
    /// Hosted DataFusion-over-ClickHouse (H-milestone work; named now so
    /// capability metadata is complete from the start).
    Clickhouse,
}

impl Backend {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Local => "local",
            Backend::Clickhouse => "clickhouse",
        }
    }
}

/// The fixed universe one query executes against (D10). Immutable once
/// bound; every resident read and every value hydration uses it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Public logical catalog version ("v1").
    pub catalog_version: String,
    /// Physical provider/projection generation. Local providers derive it
    /// from the bound artifact universe; hosted from the projection
    /// generation. Opaque to users, stable within the query.
    pub generation: String,
    /// Durable projected-through / evidence barrier description, when the
    /// provider has one (hosted projections; local fold watermarks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_through: Option<String>,
}

/// Authorization and binding context created by trusted code before
/// planning. Local scope is single-tenant; the fields exist so the
/// contract is uniform when hosted binding arrives.
#[derive(Debug, Clone)]
pub struct QueryScope {
    pub backend: Backend,
    /// May this scope read value content at all? A query that needs
    /// hydration under a scope without this right fails before execution.
    pub value_read_allowed: bool,
}

impl QueryScope {
    /// The local single-tenant scope with value reads enabled.
    #[must_use]
    pub fn local() -> QueryScope {
        QueryScope {
            backend: Backend::Local,
            value_read_allowed: true,
        }
    }
}
