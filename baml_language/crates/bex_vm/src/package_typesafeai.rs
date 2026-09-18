//! Small value-construction bridges for the reflection-driven JEV adapter.

use bex_heap::TlabHolder;
use bex_vm_types::{
    ArrayReadGuard, MapReadGuard, RealizedTy,
    types::{Instance, Object, Value},
};

use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
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
    include!(concat!(
        env!("OUT_DIR"),
        "/typesafeaifunctions_generated.rs"
    ));
}
pub use generated::*;

pub struct PackageTypesafeaiImpl;

fn invalid(message: impl Into<String>) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: message.into(),
    }
    .into()
}

fn type_arg(vm: &BexVm, value: Value) -> Result<RealizedTy, VmRustFnError> {
    match value.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::Type(ty)) => Ok(ty.ty.clone()),
        _ => Err(invalid("expected reflect.Type")),
    }
}

impl BamlNamespaceInternal for PackageTypesafeaiImpl {
    fn enum_value(
        vm: &mut BexVm,
        ty: &Value,
        name: &bex_str::BexStr,
    ) -> Result<Value, VmRustFnError> {
        let RealizedTy::Enum(head) = type_arg(vm, *ty)? else {
            return Err(invalid("expected an enum type"));
        };
        let Object::Enum(enm) = vm.get_object(head.ptr()) else {
            return Err(invalid("expected enum declaration"));
        };
        let index = enm
            .variants
            .iter()
            .position(|variant| variant.name.as_str() == name.as_str())
            .ok_or_else(|| invalid("unknown enum variant"))?;
        Ok(Value::object(vm.alloc_variant(head.ptr(), index)))
    }

    fn class_value(vm: &mut BexVm, ty: &Value, fields: &[Value]) -> Result<Value, VmRustFnError> {
        let RealizedTy::Class(head, args) = type_arg(vm, *ty)? else {
            return Err(invalid("expected a class type"));
        };
        let Object::Class(class) = vm.get_object(head.ptr()) else {
            return Err(invalid("expected class declaration"));
        };
        if fields.len() != class.fields.len() {
            return Err(invalid("wrong field count for JEV class"));
        }
        for (field, value) in class.fields.iter().zip(fields) {
            let expected = field
                .field_template
                .substitute(&args, vm)
                .map_err(|_| invalid("unresolved JEV class field type"))?;
            let actual = vm
                .value_singleton_ty(*value)
                .ok_or_else(|| invalid("invalid JEV class field value"))?;
            if !baml_type::normalize::is_subtype(actual.as_ty(), expected.as_ty(), vm) {
                return Err(invalid(format!(
                    "wrong value type for field {}",
                    field.name
                )));
            }
        }
        Ok(Value::object(vm.tlab.alloc(Object::Instance(
            Instance::new(head.ptr(), args, fields.to_vec()),
        ))))
    }
}

impl BamlPackageTypesafeai for PackageTypesafeaiImpl {}
