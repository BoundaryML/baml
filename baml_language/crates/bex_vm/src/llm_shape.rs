//! What a model writes in place of a type: the `Shape` of the type's
//! `baml.FromLlmOutput` implementation.
//!
//! Everything that describes or reads a model's answer asks this: the parser
//! (`baml.sap`), and through the engine, the output format and
//! `baml.json.schema`. A type with no implementation is described and read by
//! its fields or variants.

use bex_vm_types::{RealizedTy, TypeHead, errors::VmInternalError, types::Object};

use crate::{BexVm, package_baml::resolve::ImplResolver};

/// The name of the associated type a `baml.FromLlmOutput` implementation
/// binds.
const SHAPE: &str = "Shape";

/// The `Shape` that the `baml.FromLlmOutput` implementation of `ty`, a class
/// or enum type, binds, realized at `ty`. `None` when `ty` has no
/// implementation, is not a class or enum, or is media, which the parser and
/// the output format treat as their own kind.
///
/// # Errors
///
/// The implementation's binding does not realize at `ty`: the impl table and
/// the type disagree, which no accepted program does.
pub fn llm_shape(vm: &BexVm, ty: &RealizedTy) -> Result<Option<RealizedTy>, VmInternalError> {
    let head = match ty {
        RealizedTy::Class(head, _) | RealizedTy::Enum(head) => *head,
        _ => return Ok(None),
    };
    if vm.stdlib_heads().media_kind(head).is_some() {
        return Ok(None);
    }
    // Without the standard library there is no interface to implement.
    let Some(interface) = vm.lang_head(crate::lang_items::BAML_FROM_LLM_OUTPUT) else {
        return Ok(None);
    };
    let resolver = resolver_for(vm, head);
    let Some((rule, bindings)) = resolver.resolve_implements_rule(ty, interface, &[]) else {
        return Ok(None);
    };
    let Some((_, template)) = rule
        .interface_assoc
        .iter()
        .find(|(name, _)| name.as_str() == SHAPE)
    else {
        return Err(VmInternalError::TypeSubstitution {
            message: format!("the `baml.FromLlmOutput` implementation of `{ty}` binds no `Shape`"),
        });
    };
    template
        .substitute(&bindings, vm)
        .map(Some)
        .map_err(|e| VmInternalError::TypeSubstitution {
            message: e.to_string(),
        })
}

/// The resolver for a type-directed lookup: there is no receiver value, so it
/// roots in the world that owns the declaration (its package for a runtime
/// declaration, the program's for a static one), as `baml.json` decoding does
/// for `baml.FromJson`.
fn resolver_for(vm: &BexVm, head: TypeHead) -> ImplResolver<'_> {
    let owner = match vm.get_object(head.ptr()) {
        Object::Class(class) => class.owner.package(),
        Object::Enum(enm) => enm.owner.package(),
        _ => None,
    }
    .and_then(|owner| vm.runtime_owner(owner));
    match owner {
        Some(owner) => ImplResolver::for_package(vm, owner),
        None => ImplResolver::new(vm),
    }
}

/// Every class instantiation and enum that `ty` reaches where a model's answer
/// fills it, each with its `Shape`: the types the output format and
/// `baml.json.schema` describe by their shape. A shaped type's shape is walked
/// too, and its fields are not. Empty when `ty` reaches none, or when the
/// parser cannot model `ty` (the describer then reports why).
#[must_use]
pub fn llm_shapes(vm: &BexVm, ty: &RealizedTy) -> Vec<(RealizedTy, RealizedTy)> {
    crate::package_baml::sap::shapes_reached(vm, ty)
}
