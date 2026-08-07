use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{QueryContext, QueryError, Result, ValueId};

#[async_trait]
pub trait ValueStore: Send + Sync {
    async fn get_many(
        &self,
        ids: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, Bytes>>;
}

#[derive(Clone)]
pub struct LocalBlobStore {
    root: Arc<Path>,
}

impl LocalBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Arc::from(root.into()),
        }
    }

    pub fn path_for(&self, id: ValueId) -> PathBuf {
        self.root
            .join("values")
            .join(format!("{}.blob", id.to_hex()))
    }

    pub async fn put(&self, id: ValueId, bytes: &[u8]) -> Result<()> {
        let values_dir = self.root.join("values");
        tokio::fs::create_dir_all(&values_dir).await?;
        tokio::fs::write(self.path_for(id), bytes).await?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }
}

#[async_trait]
impl ValueStore for LocalBlobStore {
    async fn get_many(
        &self,
        ids: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, Bytes>> {
        let started = Instant::now();
        let mut output = HashMap::with_capacity(ids.len());
        let mut total_bytes: usize = 0;
        context
            .metrics
            .blob_requests
            .fetch_add(ids.len(), std::sync::atomic::Ordering::Relaxed);
        let result = (|| {
            for id in ids.iter().copied() {
                context.check_cancelled()?;
                let path = self.path_for(id);
                let bytes = std::fs::read(&path).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        QueryError::MissingValue {
                            value_id: id.to_string(),
                            path: path.clone(),
                        }
                    } else {
                        QueryError::Io(error)
                    }
                })?;
                total_bytes = total_bytes.saturating_add(bytes.len());
                context
                    .metrics
                    .blob_bytes
                    .fetch_add(bytes.len(), std::sync::atomic::Ordering::Relaxed);
                if total_bytes > context.budgets.max_blob_bytes {
                    return Err(QueryError::ValueLimit);
                }
                output.insert(id, Bytes::from(bytes));
            }
            Ok(output)
        })();
        context.metrics.record_blob_read_duration(started.elapsed());
        result
    }
}
