//! Native scans used by test registration. Read the current records on every
//! call: public collector arrays and registration names can change in place.

use bex_heap::TlabHolder;
use bex_str::BexStr;
use bex_vm_types::{
    ArrayReadGuard, MapReadGuard,
    types::{Instance, Value},
};

use crate::{
    BexVm,
    errors::VmBamlError,
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
    include!(concat!(env!("OUT_DIR"), "/testingfunctions_generated.rs"));
}
pub use generated::*;

/// The VM's native implementations for the `testing` package.
pub struct PackageTestingImpl;

impl BamlPackageTesting for PackageTestingImpl {
    fn _test_name_count(vm: &BexVm, entries: &[Value], full_name: &BexStr) -> i64 {
        registration_name_count(
            entries.iter().map(|entry| {
                view::TestRegistration {
                    instance: vm.as_instance(entry).expect("TestRegistration instance"),
                }
                .name(vm)
            }),
            full_name,
        )
    }

    fn _testset_name_count(vm: &BexVm, entries: &[Value], full_name: &BexStr) -> i64 {
        registration_name_count(
            entries.iter().map(|entry| {
                view::TestSetRegistration {
                    instance: vm.as_instance(entry).expect("TestSetRegistration instance"),
                }
                .name(vm)
            }),
            full_name,
        )
    }
}

/// Preserve the existing rule exactly: an explicit `name#anything` counts as a
/// duplicate of `name`, and duplicate entries each count. A native scan avoids
/// a VM call, loop bookkeeping and string allocation for every prior entry.
fn registration_name_count<'a>(names: impl Iterator<Item = &'a str>, full_name: &str) -> i64 {
    names.fold(0, |count, name| {
        count
            + i64::from(
                name.strip_prefix(full_name)
                    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('#')),
            )
    })
}
