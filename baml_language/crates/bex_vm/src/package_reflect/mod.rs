//! Native implementations for the root `reflect` stdlib package.
//!
//! The generated trait hierarchy is built from `baml_std/reflect`, so every
//! `$rust_function` declared by the package requires a Rust implementation.

pub(crate) mod reflect;
pub(crate) mod runtime_class_builder;
mod type_class;
pub(crate) mod type_kinds;

use bex_heap::TlabHolder;
use bex_vm_types::{
    ArrayReadGuard, MapReadGuard,
    types::{Instance, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::VmRustFnError,
    package_baml::{
        Continuation, ImplResolver, NativeCallResult, NativeFunction, NativeFunctionResult,
        PassThroughContinuation, resolve,
    },
};

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
    include!(concat!(env!("OUT_DIR"), "/reflectfunctions_generated.rs"));
}
pub use generated::*;

/// The VM's native implementations for the `reflect` package.
pub struct PackageReflectImpl;
