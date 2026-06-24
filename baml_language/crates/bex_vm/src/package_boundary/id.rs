use crate::{
    BexVm,
    package_baml::{NativeCallResult, NativeFunctionResult},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::Value;

pub(super) fn current(vm: &mut BexVm, _args: &[Value]) -> NativeCallResult {
    let result: NativeFunctionResult = (|| {
        let id = crate::package_baml::id::current_runtime_id(vm).map_or_else(
            || bex_str::BexStr::from(""),
            |id| bex_str::BexStr::from(id.as_str()),
        );
        Ok(Value::object(vm.alloc_string(id)))
    })();
    match result {
        Ok(value) => NativeCallResult::Done(value),
        Err(err) => NativeCallResult::Error(err),
    }
}
