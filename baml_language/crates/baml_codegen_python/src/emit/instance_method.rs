//! `PyInstanceMethod` — stub for an instance method binding.
//!
//! Same "no IR support yet" caveat as `PyStaticMethod`. The type
//! exists so G5 has a slot to render into once the IR gains method
//! kinds.

use crate::emit::function::SyncAsync;

#[allow(dead_code)]
pub(crate) struct PyInstanceMethod {
    pub(crate) py_name: String,
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    pub(crate) parent_class_py_name: String,
    // deferred to G5: param_names: Vec<String>,
    // deferred to G5: the __define_instance_method(...) RHS.
}
