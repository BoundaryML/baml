//! Backend-neutral Project Studio query core (design 04-query-system).
//!
//! This crate owns the public SQL semantics: the versioned logical
//! catalog, DataFusion session/planning setup, ordinary-SQL lowering for
//! virtual BAML values (D7), the QueryScope/snapshot/provider/
//! ValueResolver contracts, query-global budgets and cancellation, and
//! the mandatory terminal [`outcome::QueryOutcome`] (D13).
//!
//! It must not depend on the CLI, the runtime host, the AWS SDK, or a
//! concrete SQLite/ClickHouse client, and it never invents a second
//! value codec or CID space — canonical value semantics come from
//! `bex_events::store::canon`.

pub mod budget;
pub mod capability;
pub mod catalog;
pub mod error;
pub mod outcome;
pub mod provider;
pub mod scope;
pub mod session;
pub mod value;

pub use error::{QueryError, QueryErrorCode};
pub use outcome::{QueryOutcome, ResultState, ValueEvaluations};
pub use session::{QueryExecution, QuerySession, QuerySessionBuilder};
