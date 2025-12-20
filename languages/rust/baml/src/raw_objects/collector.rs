//! Collector and related types (FunctionLog, Usage)
//!
//! These wrap FFI pointers to collector objects managed by the BAML runtime.
//! Full implementation in Phase 12.

use crate::proto::baml_cffi_v1::BamlObjectType;

use super::{define_raw_object_wrapper, RawObject, RawObjectTrait};

define_raw_object_wrapper! {
    /// Collector for function call logs
    Collector => ObjectCollector
}

define_raw_object_wrapper! {
    /// Function call log entry
    FunctionLog => ObjectFunctionLog
}

define_raw_object_wrapper! {
    /// Usage information for an LLM call
    Usage => ObjectUsage
}
