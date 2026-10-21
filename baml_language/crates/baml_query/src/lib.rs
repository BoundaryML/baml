//! Backend-neutral SQL core for BAML profiles (TASK/baml-query-scope.md).
//!
//! This crate owns the public SQL semantics: the versioned logical
//! catalog and its rendered profiles, `DataFusion` session/planning setup,
//! ordinary-SQL lowering for virtual BAML values, the `QueryScope`/
//! snapshot/provider/ValueResolver contracts, query-global budgets and
//! cancellation, and the mandatory terminal [`outcome::QueryOutcome`].
//!
//! It must not depend on the engine, the events transport, the CLI, or a
//! concrete storage client — providers live behind
//! [`provider::RelationProviderFactory`] in separate crates.

pub mod budget;
pub mod capability;
pub mod catalog;
pub mod error;
pub mod outcome;
pub mod provider;
pub mod scope;
pub mod session;
pub mod value;

pub use catalog::{Catalog, CatalogProfile, RelationDef, ViewDef, Visibility};
pub use error::{QueryError, QueryErrorCode};
pub use outcome::{QueryOutcome, ResultState, ValueEvaluations};
pub use session::{QueryExecution, QuerySession, QuerySessionBuilder};
