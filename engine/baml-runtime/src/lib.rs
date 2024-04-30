pub(crate) mod internal;

mod runtime;
mod runtime_interface;
mod types;

use std::path::PathBuf;

use runtime::BamlRuntime;
pub use runtime_interface::*;
pub use types::*;

pub use types::RuntimeContext;

/// Load a runtime from a directory
pub fn load_runtime_from_dir(path: &PathBuf) -> anyhow::Result<impl IBamlRuntime> {
    BamlRuntime::from_directory(path)
}
