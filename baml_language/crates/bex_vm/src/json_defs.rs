//! The heap's declarations as the JSON inhabitance analysis reads them
//! ([`baml_type::json_inhabit`]): for the LLM output check, for the empty
//! values schema-aligned parsing fills `@skip` fields with, and for naming the
//! union members `baml.json` decoding drops.

use baml_type::json_inhabit::{JsonClass, JsonDefs, JsonField, JsonStatus, JsonVariant};
use bex_vm_types::{RealizedTy, RuntimeTy, TypeHead, types::Object};

use crate::BexVm;

/// A type the LLM output check rejects wherever a model's answer would have
/// to fill it, each with the type as the check shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputReject {
    /// `unknown`, which admits every value, so no schema describes it.
    Unknown(String),
    /// An interface, which admits values of types not known here.
    Interface(String),
    /// An enum variant type, which the output format has no form for.
    Unrenderable(String),
}

/// The heap's declarations.
pub(crate) struct HeapDefs<'vm> {
    pub vm: &'vm BexVm,
    /// Whether `baml.json` decoding reads them, which takes every type as it
    /// is: `unknown` and an interface admit JSON values, and an enum variant
    /// type is a JSON string. The LLM output check rejects those.
    pub decoding: bool,
}

impl<'vm> HeapDefs<'vm> {
    /// The declarations as the LLM output check reads them.
    pub(crate) fn output(vm: &'vm BexVm) -> Self {
        Self {
            vm,
            decoding: false,
        }
    }

    /// The declarations as `baml.json` decoding reads them.
    pub(crate) fn decoding(vm: &'vm BexVm) -> Self {
        Self { vm, decoding: true }
    }
}

impl JsonDefs<TypeHead> for HeapDefs<'_> {
    type Error = OutputReject;

    fn class(
        &self,
        head: &TypeHead,
        args: &[RuntimeTy],
    ) -> Result<JsonClass<TypeHead>, OutputReject> {
        let Object::Class(class) = self.vm.get_object(head.ptr()) else {
            unreachable!("a class type's head is a class: {head:?}")
        };
        let args: Vec<RealizedTy> = args
            .iter()
            .filter_map(|arg| RealizedTy::try_from(arg).ok())
            .collect();
        let fields = class
            .fields
            .iter()
            .map(|field| {
                // A runtime-built field carries the exact type it was built
                // from; a declared one realizes at the class's arguments.
                let ty = match &field.runtime_type {
                    Some(runtime) => RuntimeTy::from(&runtime.ty),
                    None => field
                        .field_template
                        .substitute(&args, self.vm)
                        .map_or_else(|_| field.field_type.clone(), |ty| RuntimeTy::from(&ty)),
                };
                JsonField {
                    name: field.name.clone(),
                    alias: field.alias.clone(),
                    skip: field.skip,
                    ty,
                }
            })
            .collect();
        Ok(JsonClass { fields })
    }

    fn variants(&self, head: &TypeHead) -> Result<Vec<JsonVariant>, OutputReject> {
        let Object::Enum(enm) = self.vm.get_object(head.ptr()) else {
            unreachable!("an enum type's head is an enum: {head:?}")
        };
        Ok(enm
            .variants
            .iter()
            .map(|variant| JsonVariant {
                name: variant.name.clone(),
                skip: variant.skip,
            })
            .collect())
    }

    fn alias_target(&self, head: &TypeHead) -> Result<RuntimeTy, OutputReject> {
        let target = self
            .vm
            .type_alias_definition(head.ptr())
            .unwrap_or_else(|| unreachable!("a type alias's head is a type alias: {head:?}"));
        Ok(RuntimeTy::from(target))
    }

    fn opaque(&self, ty: &RuntimeTy) -> Result<JsonStatus, OutputReject> {
        match ty {
            _ if self.decoding => Ok(JsonStatus::Json),
            RuntimeTy::Media(_) => Ok(JsonStatus::Json),
            RuntimeTy::Interface(..) => Err(OutputReject::Interface(ty.to_string())),
            _ => Err(OutputReject::Unknown(ty.to_string())),
        }
    }

    fn check(&self, ty: &RuntimeTy) -> Result<(), OutputReject> {
        match ty {
            _ if self.decoding => Ok(()),
            RuntimeTy::EnumVariant(..) => Err(OutputReject::Unrenderable(ty.to_string())),
            _ => Ok(()),
        }
    }
}
