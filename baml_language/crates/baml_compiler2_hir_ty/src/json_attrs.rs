//! The mistakes `@json` attributes make possible, as compile errors.
//!
//! The attributes describe how a class reads and writes JSON, so each of these
//! is a class whose JSON would quietly not be what its declaration says: an
//! override the attributes never reach, a key held by the wrong field, or a
//! union member that takes the objects meant for the fallback.

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::{
    item_tree::ClassFieldAttrs,
    loc::{ClassLoc, DeclRef},
};
use baml_type::{DeclName, Ty};
use text_size::TextRange;

use crate::diagnostics::TirTypeError;

/// The compile errors of `class`'s own `@json` attributes, at the field or
/// class name they are about.
pub fn class_errors<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassLoc<'db>,
    head: &DeclName,
    viewer: SourceRoot,
) -> Vec<(TextRange, TirTypeError)> {
    let data = baml_compiler2_hir::item_data::class_data(db, class);
    let source_map = baml_compiler2_hir::item_data::class_source_map(db, class);
    let fields = crate::lower::resolve_class_fields(db, class);
    let mut errors = Vec::new();

    for (index, (name, ty, attrs)) in fields.iter().enumerate() {
        if !attrs.json_flatten && !attrs.json_fallback {
            continue;
        }
        if !is_json_object(db, ty, &mut Vec::new()) {
            errors.push((
                source_map.field_name_spans[index],
                TirTypeError::JsonRestFieldType {
                    attribute: rest_attribute(attrs),
                    field: name.clone(),
                },
            ));
        }
        if attrs.json_rename.is_some() {
            errors.push((
                source_map.field_name_spans[index],
                TirTypeError::JsonRestRenamed {
                    attribute: rest_attribute(attrs),
                    field: name.clone(),
                },
            ));
        }
    }

    let rest: Vec<usize> = fields
        .iter()
        .enumerate()
        .filter(|(_, (_, _, attrs))| attrs.json_flatten || attrs.json_fallback)
        .map(|(index, _)| index)
        .collect();
    if let [_, extra @ ..] = rest.as_slice() {
        for &index in extra {
            errors.push((
                source_map.field_name_spans[index],
                TirTypeError::JsonRestRepeated {
                    class: data.name.clone(),
                },
            ));
        }
    }

    if fields.iter().any(|(_, _, attrs)| attrs.json_fallback) {
        for (index, (name, ty, attrs)) in fields.iter().enumerate() {
            if !attrs.json_fallback && !attrs.json_flatten && !matches!(ty, Ty::Literal(..)) {
                errors.push((
                    source_map.field_name_spans[index],
                    TirTypeError::JsonFallbackFieldNotLiteral {
                        class: data.name.clone(),
                        field: name.clone(),
                    },
                ));
            }
        }
    }

    let uses_attributes = data.attrs.json_omit_nulls
        || fields.iter().any(|(_, _, attrs)| {
            attrs.json_rename.is_some() || attrs.json_flatten || attrs.json_fallback
        });
    if uses_attributes {
        let args = crate::lower::class_generic_frame(db, class)
            .into_iter()
            .map(Ty::TypeVar)
            .collect();
        let self_ty = Ty::Class(head.clone(), args);
        let provides = |method: &str| {
            let method = Name::new(method);
            crate::impls::impls_for_type(db, viewer, &self_ty)
                .iter()
                .any(|implementation| implementation.provided_method(db, &method).is_some())
        };
        for (method, interface) in [("to_json", "baml.ToJson"), ("from_json", "baml.FromJson")] {
            if provides(method) {
                errors.push((
                    source_map.name_span,
                    TirTypeError::JsonAttributesWithOverride {
                        class: data.name.clone(),
                        interface,
                    },
                ));
            }
        }
    }
    errors
}

fn rest_attribute(attrs: &ClassFieldAttrs) -> &'static str {
    if attrs.json_fallback {
        "@json.fallback"
    } else {
        "@json.flatten"
    }
}

/// Whether `ty` is `map<string, json>`. An alias met again while unfolding
/// is a cycle, which holds no map.
fn is_json_object(db: &dyn baml_compiler2_hir::Db, ty: &Ty, unfolding: &mut Vec<DeclName>) -> bool {
    match ty {
        Ty::Map { key, value } => matches!(**key, Ty::String) && is_json(db, value),
        Ty::TypeAlias(head) => {
            if crate::impls::is_json_alias(db, head) || unfolding.contains(head) {
                return false;
            }
            unfolding.push(head.clone());
            let is_object = crate::layout::alias_ref_of(db, head).is_some_and(|alias| {
                is_json_object(db, &crate::layout::alias_value(db, alias), unfolding)
            });
            unfolding.pop();
            is_object
        }
        _ => false,
    }
}

fn is_json(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    matches!(ty, Ty::TypeAlias(head) if crate::impls::is_json_alias(db, head))
}

/// Whether a field of type `ty` is optional: its type has a `null` arm, or it
/// names an alias that resolves to one. `json` holds `null` as a value.
pub fn is_optional(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    let mut members = Vec::new();
    match ty {
        Ty::Union(..) | Ty::TypeAlias(..) => members_of(db, ty, &mut Vec::new(), &mut members),
        _ => return false,
    }
    members.iter().any(|member| matches!(member, Ty::Null))
}

/// The first union in `ty` with a `@json.fallback` member and another member
/// that reads an object without a tag, so the fallback would never be reached.
pub fn untagged_member_error(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Option<TirTypeError> {
    match ty {
        Ty::Union(..) => {
            let mut members = Vec::new();
            members_of(db, ty, &mut Vec::new(), &mut members);
            let fallback = members
                .iter()
                .find_map(|member| fallback_class(db, member))?;
            // A second fallback is untagged too: it takes any object.
            let untagged = members
                .iter()
                .find(|member| match fallback_class(db, member) {
                    Some(other) => other != fallback,
                    None => !is_tagged(db, member),
                })?;
            Some(TirTypeError::JsonFallbackUntaggedMember {
                fallback,
                member: untagged.clone(),
            })
        }
        Ty::List(element) => untagged_member_error(db, element),
        Ty::Map { value, .. } => untagged_member_error(db, value),
        _ => None,
    }
}

/// The non-alias members of `ty`, unions flattened. `json` stays one member.
fn members_of(
    db: &dyn baml_compiler2_hir::Db,
    ty: &Ty,
    unfolding: &mut Vec<DeclName>,
    out: &mut Vec<Ty>,
) {
    match ty {
        Ty::Union(members) => {
            for member in members {
                members_of(db, member, unfolding, out);
            }
        }
        Ty::TypeAlias(head) if !is_json(db, ty) => {
            if unfolding.contains(head) {
                return;
            }
            if let Some(alias) = crate::layout::alias_ref_of(db, head) {
                unfolding.push(head.clone());
                members_of(db, &crate::layout::alias_value(db, alias), unfolding, out);
                unfolding.pop();
            }
        }
        _ => out.push(ty.clone()),
    }
}

/// The name of the class `ty` when it has a `@json.fallback` field.
fn fallback_class(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Option<Name> {
    let Ty::Class(head, _) = ty else {
        return None;
    };
    let DeclRef::Source(class) = crate::layout::class_ref_of(db, head)? else {
        return None;
    };
    crate::lower::resolve_class_fields(db, class)
        .iter()
        .any(|(_, _, attrs)| attrs.json_fallback)
        .then(|| head.name().clone())
}

/// Whether a value of `ty` cannot be an object read with no tag to match: a
/// class with a literal field, or anything that does not read objects. A class
/// declared in another package is taken at its word.
fn is_tagged(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    match ty {
        Ty::Map { .. } | Ty::Unknown => false,
        ty if is_json(db, ty) => false,
        Ty::Class(head, _) => match crate::layout::class_ref_of(db, head) {
            Some(DeclRef::Source(class)) => crate::lower::resolve_class_fields(db, class)
                .iter()
                .any(|(_, ty, attrs)| matches!(ty, Ty::Literal(..)) && !attrs.skip),
            _ => true,
        },
        _ => true,
    }
}
