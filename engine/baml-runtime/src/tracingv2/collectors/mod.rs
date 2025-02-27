pub mod in_process;
mod boundary_db;

use baml_types::tracing::events::FunctionId;
pub use in_process::Collector as InProcessCollector;
pub use boundary_db::{Collector as BoundaryStudioCollector, BoundaryStudioConfigBuilder};

pub(super) fn finish_function_id(id: &FunctionId) {
    in_process::drop_function_log_inner(id);
}
