use anyhow::Result;

use super::api_wrapper::{core_types::LogSchema, APIWrapper, BoundaryAPI};

use crate::TraceStats;

pub(super) struct NonThreadedTracer {
    options: APIWrapper,
}

impl NonThreadedTracer {
    pub fn new(api_config: &APIWrapper, _max_batch_size: usize, _stats: TraceStats) -> Self {
        Self {
            options: api_config.clone(),
        }
    }

    pub fn flush(&self) -> Result<()> {
        Ok(())
    }

    pub async fn submit(&self, event: LogSchema) -> Result<()> {
        self.options.log_schema(&event).await
    }
}
