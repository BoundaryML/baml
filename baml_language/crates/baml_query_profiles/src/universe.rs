//! The bound store universe (TASK/baml-query-scope.md §5.6).
//!
//! `bind` reads every stream's meta plane once and freezes: header,
//! liveness (`stream.lock`), committed high-waters, and per-execution
//! summaries. Later commits are invisible — reads stay bounded to the
//! bound high-waters, so re-running the same SQL against an unchanged
//! store is deterministic.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_query::{
    error::{QueryError, QueryErrorCode},
    scope::Snapshot,
};
use bex_prof_store::prof::backend::{ExecutionReader, ExecutionSummary, ReadError, StreamReader};
use sha2::Digest as _;

/// One bound stream: the meta-plane reader plus its frozen execution
/// summaries.
pub(crate) struct BoundStream {
    pub(crate) reader: StreamReader,
    pub(crate) executions: Vec<ExecutionSummary>,
}

/// The bound universe: every stream of one `profiles-v1` store.
pub(crate) struct ProfilesUniverse {
    root: PathBuf,
    pub(crate) streams: Vec<BoundStream>,
    generation: String,
    projected_through: u64,
}

impl ProfilesUniverse {
    /// Bind a `profiles-v1` store root into a fixed universe.
    pub(crate) fn bind(root: &Path) -> Result<Arc<ProfilesUniverse>, QueryError> {
        if !root.exists() {
            return Err(QueryError::new(
                QueryErrorCode::DependencyUnavailable,
                format!("no profile store at {}", root.display()),
            )
            .with_remedy("run a BAML program first: the profiler writes .baml/profiles-v1"));
        }
        let stream_ids =
            bex_prof_store::prof::backend::list_streams(root).map_err(|e| read_error(&e))?;
        let mut streams = Vec::with_capacity(stream_ids.len());
        for stream in stream_ids {
            let reader = StreamReader::open(root, stream).map_err(|e| read_error(&e))?;
            let executions = reader.executions();
            streams.push(BoundStream { reader, executions });
        }

        // Generation: sha256 over the sorted (stream_id, meta_hw, data_hw)
        // triples (§5.6) — the identity of the committed prefix this
        // universe can see.
        let mut rows: Vec<([u8; 16], u64, u64)> = streams
            .iter()
            .map(|s| {
                (
                    s.reader.stream.0.0,
                    s.reader.high_water.meta,
                    s.reader.high_water.data,
                )
            })
            .collect();
        rows.sort_unstable();
        let mut hash = sha2::Sha256::new();
        hash.update(b"baml-query-generation-v1");
        for (stream, meta, data) in &rows {
            hash.update(stream);
            hash.update(meta.to_be_bytes());
            hash.update(data.to_be_bytes());
        }
        let generation = hex::encode(hash.finalize());
        let projected_through = streams
            .iter()
            .map(|s| s.reader.high_water.data)
            .max()
            .unwrap_or(0);

        Ok(Arc::new(ProfilesUniverse {
            root: root.to_path_buf(),
            streams,
            generation,
            projected_through,
        }))
    }

    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            catalog_version: baml_query::catalog::CATALOG_V1.to_string(),
            generation: self.generation.clone(),
            projected_through: Some(self.projected_through.to_string()),
        }
    }

    /// Every execution summary across streams.
    pub(crate) fn executions(&self) -> impl Iterator<Item = (&BoundStream, &ExecutionSummary)> {
        self.streams
            .iter()
            .flat_map(|stream| stream.executions.iter().map(move |e| (stream, e)))
    }
}

impl BoundStream {
    /// The execution reader for one of this stream's summaries.
    pub(crate) fn execution_reader(
        &self,
        summary: &ExecutionSummary,
    ) -> Result<ExecutionReader, QueryError> {
        self.reader
            .execution(summary.id)
            .map_err(|e| read_error(&e))
    }
}

/// Map a store read error to a typed query error.
pub(crate) fn read_error(err: &ReadError) -> QueryError {
    let code = match err {
        ReadError::Io { .. } => QueryErrorCode::DependencyUnavailable,
        _ => QueryErrorCode::ArtifactCorrupt,
    };
    QueryError::new(code, err.to_string())
}
