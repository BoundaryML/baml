//! Native implementations for the `boundary` stdlib package.
//!
//! Dispatch and the class constructors are generated from `boundary`'s `.baml`
//! sources by `baml_builtins2_codegen`, exactly as for [`crate::package_baml`]:
//! declaring a `$rust_function` adds a required trait method, so the
//! declaration and its implementation cannot drift.

pub(crate) mod id;

// `Instance`/`Type`, the allocator trait, and the error types are referenced
// by the generated module.
use bex_heap::TlabHolder;
use bex_vm_types::types::{Instance, Type, Value};

use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
    package_baml::{NativeCallResult, NativeFunction, NativeFunctionResult},
};

// The `BamlPackageBoundary` trait hierarchy and its `get_native_fn`
// dispatcher, generated from the package's `.baml` declarations.
#[allow(
    unused_variables,
    unsafe_code,
    non_camel_case_types,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::get_first,
    clippy::iter_not_returning_iterator,
    clippy::needless_lifetimes,
    clippy::redundant_closure_call,
    clippy::new_ret_no_self,
    clippy::too_many_arguments,
    non_snake_case
)]
mod generated {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/boundaryfunctions_generated.rs"));
}
pub use generated::*;

/// The VM's `boundary` native implementations.
pub struct PackageBoundaryImpl;
