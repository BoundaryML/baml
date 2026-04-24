//! `PyStaticMethod` — stub for a static method binding.
//!
//! G2 emits nothing for this yet because the input IR's `Function`
//! struct doesn't currently carry a "method kind" flag (free / static
//! / instance). The type is introduced in G2 as a named container so
//! G5 can fill it in without adding the type then.

use crate::emit::function::SyncAsync;

#[allow(dead_code)]
pub(crate) struct PyStaticMethod {
    pub(crate) py_name: String,
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Which class body this method nests into.
    pub(crate) parent_class_py_name: String,
    // deferred to G5: param_names: Vec<String>,
    // deferred to G5: the staticmethod(__define_static_method(...)) RHS.
}
