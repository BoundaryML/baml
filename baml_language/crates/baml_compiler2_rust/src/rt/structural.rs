//! Resolve language-provided witnesses through the checker's structural rules.
use super::*;

pub(super) fn fallback(
    db: &dyn Db,
    plan: &Plan<'_>,
    concrete: &RuntimeTy,
    iface: &baml_compiler2_mir::TyTemplateInterface,
    method: &str,
) -> Gen<Option<(usize, Vec<RuntimeTy>)>> {
    if let Some(target) = super::formatting::structural_fallback(plan, concrete, iface, method)? {
        return Ok(Some(target));
    }
    if baml_compiler2_hir_ty::impls::structural_interface(db, &iface.name).is_none()
        || !iface.generics.is_empty()
        || !iface.associated_types.is_empty()
    {
        return Ok(None);
    }
    let requested = baml_type::interned::InferInterface {
        name: iface.name.clone(),
        generics: Box::new([]),
        associated_types: Box::new([]),
    };
    let ty = baml_compiler2_hir_ty::impls::interned_ty(&baml_type::Ty::from(concrete.clone()));
    let Some(baml_compiler2_hir_ty::impls::ResolvedImplementation::StructuralDefault(kind)) =
        baml_compiler2_hir_ty::impls::resolve_implementation(db, &ty, &requested)
    else {
        return Ok(None);
    };
    if method != kind.primary_method() {
        let interface = plan
            .index
            .interface_at(&iface.name)
            .ok_or_else(|| no("structural interface declaration"))?;
        let Object::Interface(def) =
            &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(interface)]
        else {
            unreachable!()
        };
        return Ok(def
            .methods
            .iter()
            .find(|m| m.name == method)
            .and_then(|m| m.default)
            .map(|f| (f.raw(), vec![concrete.clone()])));
    }
    let name = format!("baml.{}", kind.native_function());
    let target = plan
        .program
        .objects
        .iter()
        .enumerate()
        .find_map(|(id, object)| {
            (matches!(object, Object::Function(_)) && plan.index.display(id) == name).then_some(id)
        })
        .ok_or_else(|| no(format!("unlinked structural default {name}")))?;
    let frame = if kind.native_type_arg_count() == 0 {
        Vec::new()
    } else {
        vec![concrete.clone()]
    };
    Ok(Some((target, frame)))
}

/// Membership through the language-provided structural default, including
/// Hash component eligibility, exactly as the VM resolver decides it.
pub(super) fn default_applies(
    db: &dyn Db,
    concrete: &RuntimeTy,
    iface: &baml_compiler2_mir::TyTemplateInterface,
) -> bool {
    if baml_compiler2_hir_ty::impls::structural_interface(db, &iface.name).is_none()
        || !iface.generics.is_empty()
        || !iface.associated_types.is_empty()
    {
        return false;
    }
    let requested = baml_type::interned::InferInterface {
        name: iface.name.clone(),
        generics: Box::new([]),
        associated_types: Box::new([]),
    };
    let ty = baml_compiler2_hir_ty::impls::interned_ty(&baml_type::Ty::from(concrete.clone()));
    matches!(
        baml_compiler2_hir_ty::impls::resolve_implementation(db, &ty, &requested),
        Some(baml_compiler2_hir_ty::impls::ResolvedImplementation::StructuralDefault(_))
    )
}
