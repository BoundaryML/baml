//! Index slots: `t[<here>]`.
//!
//! A tuple receiver's index selects a slot, so the slots themselves are
//! offered as the constant indices that read them, each detailed with the
//! slot's type. A union of tuples reads only the positions every variant
//! has — whatever their total arities — typed by the join of the variants'
//! slot types. A tuple index must name its slot statically, so the only
//! names offered beside the slots are those whose type is an integer
//! literal naming one of them (`i: 0`); an `int` local would not type-check
//! there. A list or map index is an ordinary expression, and the caller
//! offers the ordinary names for it.

use std::collections::HashMap;

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::Definition,
    item_data,
    resolve::{ScopeName, ScopeNameKind},
    scope::ScopeKind,
};
use text_size::TextSize;

use super::{completions::Completions, values};

/// Completes a tuple receiver's index: its common slots and the names that
/// can read one. Returns `false` (offering nothing) when the receiver is
/// not a tuple, so the caller completes an ordinary expression.
pub(crate) fn complete(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    offset: TextSize,
    receiver: &baml_type::Ty,
    optional: bool,
    out: &mut Completions<'_>,
) -> bool {
    let receiver = if optional {
        receiver.strip_null()
    } else {
        receiver.clone()
    };
    let Some(variants) = crate::resolve::tuple_variants(db, &receiver) else {
        return false;
    };
    let slots = crate::resolve::common_tuple_slots(&variants);
    for (slot, ty) in slots.iter().enumerate() {
        out.add_tuple_slot(slot, ty);
    }

    let locals = local_types_at(db, file, offset);
    let reads_a_slot = |ty: Option<&baml_type::Ty>| {
        ty.and_then(|ty| int_literal(db, ty, 0))
            .and_then(|n| usize::try_from(n).ok())
            .is_some_and(|n| n < slots.len())
    };
    values::complete_names(db, file, offset, out, |entry: &ScopeName<'_>| {
        match &entry.kind {
            ScopeNameKind::Local { .. } => {
                reads_a_slot(locals.get(&entry.name).and_then(Option::as_ref))
            }
            ScopeNameKind::Item(Definition::Let(binding)) => reads_a_slot(
                baml_compiler2_hir_ty::globals::global_value_type(db, *binding).as_ref(),
            ),
            // A package roots a qualified path, which can reach a global
            // of a literal type.
            ScopeNameKind::Package => true,
            ScopeNameKind::Item(_) => false,
        }
    });
    true
}

/// The value of an integer literal type, through aliases.
fn int_literal(db: &dyn baml_compiler2_hir::Db, ty: &baml_type::Ty, depth: usize) -> Option<i64> {
    // Alias chains are finite in a well-formed program; the bound only
    // stops a malformed (cyclic) one.
    if depth > 32 {
        return None;
    }
    match ty {
        baml_type::Ty::Literal(baml_base::Literal::Int(n), _) => Some(*n),
        baml_type::Ty::TypeAlias(name) => {
            let alias = baml_compiler2_hir_ty::layout::alias_ref_of(db, name)?;
            let value = baml_compiler2_hir_ty::layout::alias_value(db, alias);
            int_literal(db, &value, depth + 1)
        }
        _ => None,
    }
}

/// The type of each local visible at `offset`, keyed by name and shadowed
/// the way [`baml_compiler2_hir::resolve::names_in_scope_at`] shadows (the
/// innermost spelling wins). `None` where the type is not known — a lambda
/// parameter, or a binding inference has not typed.
fn local_types_at(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    offset: TextSize,
) -> HashMap<Name, Option<baml_type::Ty>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let mut out = HashMap::new();
    for ancestor_id in index.ancestor_scopes(scope_id) {
        let scope = &index.scopes[ancestor_id.index() as usize];
        if matches!(scope.kind, ScopeKind::Class) && ancestor_id != scope_id {
            continue;
        }
        let bindings = &index.scope_bindings[ancestor_id.index() as usize];
        for binding in bindings.bindings.iter().rev() {
            if index.binding_visible_at(binding, offset) && !out.contains_key(&binding.name) {
                let ty = crate::info::find_binding_ty_in_scopes(
                    db,
                    index,
                    ancestor_id,
                    binding.bind_pattern,
                );
                out.insert(binding.name.clone(), ty);
            }
        }
        for (name, param_idx) in &bindings.params {
            if out.contains_key(name) {
                continue;
            }
            let ty = match item_data::scope_owner(db, index.scope_ids[ancestor_id.index() as usize])
            {
                Some(item_data::ScopeOwner::Function(function)) => {
                    baml_compiler2_hir_ty::lower::function_signature(db, function)
                        .params
                        .get(*param_idx)
                        .map(|param| param.ty.clone())
                }
                _ => None,
            };
            out.insert(name.clone(), ty);
        }
    }
    out
}
