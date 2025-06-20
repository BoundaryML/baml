use std::sync::Arc;

use baml_types::tracing::events::TraceEvent;

pub type TraceEventWithMeta = TraceEvent<'static, baml_types::FieldType>;

pub trait Storage {
    fn put(&self, event: Arc<TraceEventWithMeta>);

    fn clear(&self);

    fn get_all(&self) -> Vec<Arc<TraceEventWithMeta>>;
}
