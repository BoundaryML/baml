//! Native implementations for the narrow Rust-backed surface in the `ai`
//! stdlib package. Prompt assembly lives here because it must preserve media
//! values and split rendered fragments on typed role markers.

use bex_heap::TlabHolder;
use bex_vm_types::{
    ArrayReadGuard, MapReadGuard,
    types::{Instance, Value},
};

use crate::{
    BexVm,
    errors::VmRustFnError,
    package_baml::{NativeCallResult, NativeFunction, NativeFunctionResult},
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
    include!(concat!(env!("OUT_DIR"), "/aifunctions_generated.rs"));
}
pub use generated::*;

/// The VM's native implementations for the `ai` package.
pub struct PackageAiImpl;
