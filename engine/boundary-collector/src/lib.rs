use std::sync::Arc;

use baml_runtime::{tracingv2::storage::storage::Collector, BamlRuntime};

const BOUNDARY_COLLECTOR_NAME: &str = "boundary-collector";

struct BoundaryCollector {
    collector: Arc<Collector>,
    // This collector will listen to all events and fire them off.
}

impl BoundaryCollector {
    pub fn new() -> Self {
        Self { collector: Arc::new(Collector::new(Some(BOUNDARY_COLLECTOR_NAME.into()))) }
    }

    pub fn attach_runtime(&self, runtime: &BamlRuntime) {
        let hash = runtime.create_hash();
        runtime.register_collector(self.collector.clone());
    }
}