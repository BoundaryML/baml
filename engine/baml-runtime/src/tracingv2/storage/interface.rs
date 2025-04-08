use baml_types::tracing::events::TraceEvent;
use std::sync::Arc;

pub trait Storage {
    fn put(&self, event: Arc<TraceEvent>);

    fn clear(&self);

    fn get_all(&self) -> Vec<Arc<TraceEvent>>;
}
