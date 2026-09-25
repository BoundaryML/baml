//! Local SQL over Btel recordings.
//!
//! Recording stays BTEL files plus CAS blobs. The first query creates a
//! disposable SQLite index beside them (`.baml/btel/query.sqlite`); every
//! refresh applies only files the index has not applied, exactly once, and
//! queries run in a fixed read snapshot. Captured values stay in the CAS and
//! are decoded only when a query evaluates them.
//!
//! ```text
//! SQL -> sqlparser -> BAML-value translation -> SQLite -> rows + outcome
//!                                                  |
//!                                      lazy, verified CAS decoding
//! ```
#![cfg(not(target_arch = "wasm32"))]

pub mod catalog;
pub mod format;
pub mod functions;
pub mod ingest;
mod outcomes;
pub mod query;
pub mod schema;
pub mod sql;
pub mod store;

use std::{path::Path, sync::Arc, time::Duration};

use btel_reader::{
    cas::{CasLimits, CasStore},
    layout::SourceLayout,
};
pub use ingest::{RefreshMetrics, RefreshOptions};
pub use query::{Budgets, Outcome, QueryRequest, QueryResult, Status};
use rusqlite::Connection;
/// A bound SQL parameter value for `QueryRequest::params`.
pub use rusqlite::types::Value as SqlParam;
pub use store::StoreOptions;

#[derive(Debug)]
pub enum Error {
    /// The user's SQL cannot be translated or prepared.
    Sql(sql::SqlError),
    /// A query budget was exceeded.
    Budget(String),
    /// Another process held the writer lock for the whole wait.
    LockTimeout(Duration),
    Io(String),
    Unsupported(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(error) => write!(f, "{error}"),
            Self::Budget(message) => f.write_str(message),
            Self::LockTimeout(timeout) => write!(
                f,
                "another process is still indexing these recordings after {timeout:?}; retry, or pass --no-refresh to query the existing index"
            ),
            Self::Io(message) => write!(f, "{message}"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
            Self::Sqlite(error) => write!(f, "query index: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

#[derive(Clone, Debug, Default)]
pub struct IndexOptions {
    pub store: StoreOptions,
    pub refresh: RefreshOptions,
    pub cas: CasLimits,
    pub values: functions::ValueLimits,
}

/// One project's recordings and their derived index.
pub struct Index {
    conn: Connection,
    layout: SourceLayout,
    slot: functions::ContextSlot,
    options: IndexOptions,
    views_ready: bool,
    /// No recordings directory: queries run against an empty in-memory index.
    source_missing: bool,
}

impl Index {
    /// Open the index for `<project>/.baml/btel`, creating the database file
    /// if recordings exist. Without a recordings directory, nothing is
    /// created and queries answer from an empty in-memory index.
    pub fn open(layout: SourceLayout, options: IndexOptions) -> Result<Self, Error> {
        let source_missing = !btel_reader::discovery::exists(&layout.recordings);
        let conn = if source_missing {
            let mut conn = Connection::open_in_memory()?;
            store::ensure_schema(&mut conn)?;
            conn
        } else {
            store::open(
                &store::database_path(&layout),
                &options.store,
                &store::lock_path(&layout),
                options.refresh.lock_timeout,
            )?
        };
        let slot = functions::ContextSlot::default();
        functions::register(&conn, &slot)?;
        Ok(Self {
            conn,
            layout,
            slot,
            options,
            views_ready: false,
            source_missing,
        })
    }

    pub fn for_project(project_root: &Path, options: IndexOptions) -> Result<Self, Error> {
        Self::open(SourceLayout::for_project(project_root), options)
    }

    pub fn layout(&self) -> &SourceLayout {
        &self.layout
    }

    pub fn source_missing(&self) -> bool {
        self.source_missing
    }

    /// Reconcile newly completed files. Holds the process-safe writer lock
    /// for the duration; waits up to `RefreshOptions::lock_timeout`.
    pub fn refresh(&mut self) -> Result<RefreshMetrics, Error> {
        if self.source_missing {
            return Ok(RefreshMetrics::default());
        }
        ingest::refresh(&mut self.conn, &self.layout, &self.options.refresh)
    }

    fn ensure_views(&mut self) -> Result<(), Error> {
        if !store::versions_match(&self.conn)? {
            return Err(Error::Unsupported(
                "the query index is missing or was built by a different version; refresh to (re)build it"
                    .into(),
            ));
        }
        if !self.views_ready {
            // Later relations may read earlier ones: drop in reverse order.
            for relation in catalog::RELATIONS.iter().rev() {
                self.conn
                    .execute_batch(&format!("DROP VIEW IF EXISTS temp.{}", relation.name))?;
            }
            for relation in catalog::RELATIONS {
                self.conn.execute_batch(&relation.create_sql())?;
            }
            self.views_ready = true;
        }
        Ok(())
    }

    /// Run one read-only query in a fixed snapshot of the index.
    pub fn query(&mut self, request: &QueryRequest) -> Result<QueryResult, Error> {
        self.ensure_views()?;
        let context = functions::QueryContext::new(
            CasStore::new(self.layout.cas.clone(), self.options.cas),
            self.options.values,
            request
                .budgets
                .max_duration
                .map(|d| std::time::Instant::now() + d),
        );
        let mut result = query::run(&mut self.conn, &self.slot, context, request)?;
        result.outcome.source_missing = self.source_missing;
        Ok(result)
    }

    /// Refresh, then query. The typical fresh-process path.
    pub fn refresh_and_query(&mut self, request: &QueryRequest) -> Result<QueryResult, Error> {
        let metrics = self.refresh()?;
        let mut result = self.query(request)?;
        result.outcome.refresh = Some(metrics);
        Ok(result)
    }

    /// Direct access for tests and diagnostics; bypasses translation.
    #[doc(hidden)]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Whether the value functions are shared-safe to hand to another thread.
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Arc<functions::QueryContext>>();
};
