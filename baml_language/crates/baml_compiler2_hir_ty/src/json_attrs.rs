//! The mistakes `@json` attributes make possible, as compile errors.
//!
//! The attributes describe how a class reads and writes JSON, so each of these
//! is a class whose JSON would quietly not be what its declaration says: an
//! override the attributes never reach, a key held by the wrong field, or a
//! union whose members can't be told apart by their tags.

use baml_base::{Literal, Name, SourceRoot};
use baml_compiler2_hir::{
    item_tree::ClassFieldAttrs,
    loc::{ClassLoc, DeclRef},
};
use baml_type::{
    DeclName, StructuralInterface, Ty,
    json_tags::{Untagged, tag_tree},
};
use text_size::TextRange;

use crate::diagnostics::TirTypeError;

/// The interface each class or enum in `package` overrides, by its name and
/// sorted by it: `baml.ToJson` or `baml.FromJson`, when an impl provides the
/// method, for any instantiation. An impl's own package holds it, since the
/// type's package or the interface's has to.
#[salsa::tracked(returns(ref))]
pub fn json_overrides(
    db: &dyn baml_compiler2_hir::Db,
    package: SourceRoot,
) -> Vec<(DeclName, &'static str)> {
    let mut overrides = Vec::new();
    for &block in crate::impls::package_impl_locs(db, package) {
        let Some(facts) = crate::impls::impl_facts(db, block).resolved() else {
            continue;
        };
        let (interface, method) =
            match crate::impls::structural_interface(db, &facts.interface.name) {
                Some(StructuralInterface::ToJson) => ("baml.ToJson", "to_json"),
                Some(StructuralInterface::FromJson) => ("baml.FromJson", "from_json"),
                _ => continue,
            };
        let provides = facts.methods.iter().any(|&function| {
            baml_compiler2_hir::item_data::function_data(db, function)
                .name
                .as_str()
                == method
        });
        if !provides {
            continue;
        }
        if let Ty::Class(head, _) | Ty::Enum(head) = facts.for_ty_pattern.to_plain() {
            overrides.push((head, interface));
        }
    }
    overrides.sort();
    overrides
}

/// The interface `head` overrides, in [`json_overrides`]' list.
pub fn override_of<'a>(
    overrides: &'a [(DeclName, &'static str)],
    head: &DeclName,
) -> Option<&'a str> {
    let at = overrides.partition_point(|(name, _)| name < head);
    overrides
        .get(at)
        .filter(|(name, _)| name == head)
        .map(|(_, interface)| *interface)
}

/// The compile errors of `class`'s own `@json` attributes, at the field or
/// class name they are about.
pub fn class_errors<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassLoc<'db>,
    head: &DeclName,
    overrides: &[(DeclName, &'static str)],
) -> Vec<(TextRange, TirTypeError)> {
    let data = baml_compiler2_hir::item_data::class_data(db, class);
    let source_map = baml_compiler2_hir::item_data::class_source_map(db, class);
    let fields = crate::lower::resolve_class_fields(db, class);
    let mut errors = Vec::new();

    let mut held_rest = false;
    for (index, (name, ty, attrs)) in fields.iter().enumerate() {
        let span = source_map.field_name_spans[index];
        if attrs.json_flatten || attrs.json_fallback {
            if !is_json_object(db, ty) {
                errors.push((
                    span,
                    TirTypeError::JsonRestFieldType {
                        attribute: rest_attribute(attrs),
                        field: name.clone(),
                    },
                ));
            }
            if attrs.json_rename.is_some() {
                errors.push((
                    span,
                    TirTypeError::JsonRestRenamed {
                        attribute: rest_attribute(attrs),
                        field: name.clone(),
                    },
                ));
            }
            if attrs.skip {
                errors.push((
                    span,
                    TirTypeError::JsonAttributeSkipped {
                        field: name.clone(),
                    },
                ));
            }
            if held_rest || (attrs.json_flatten && attrs.json_fallback) {
                errors.push((
                    span,
                    TirTypeError::JsonRestRepeated {
                        class: data.name.clone(),
                    },
                ));
            }
            held_rest = true;
        }
    }

    if fields.iter().any(|(_, _, attrs)| attrs.json_fallback) {
        let mut has_tag = false;
        for (index, (name, ty, attrs)) in fields.iter().enumerate() {
            if attrs.json_fallback || attrs.json_flatten {
                continue;
            }
            let span = source_map.field_name_spans[index];
            if literal_of(db, ty).is_none() {
                errors.push((
                    span,
                    TirTypeError::JsonFallbackFieldNotLiteral {
                        class: data.name.clone(),
                        field: name.clone(),
                    },
                ));
            } else if attrs.skip {
                errors.push((
                    span,
                    TirTypeError::JsonAttributeSkipped {
                        field: name.clone(),
                    },
                ));
            } else {
                has_tag = true;
            }
        }
        if !has_tag {
            errors.push((
                source_map.name_span,
                TirTypeError::JsonFallbackNoTag {
                    class: data.name.clone(),
                },
            ));
        }
    }

    let uses_attributes = data.attrs.json_omit_nulls
        || fields.iter().any(|(_, _, attrs)| {
            attrs.json_rename.is_some() || attrs.json_flatten || attrs.json_fallback
        });
    if uses_attributes && let Some(interface) = override_of(overrides, head) {
        errors.push((
            source_map.name_span,
            TirTypeError::JsonAttributesWithOverride {
                name: data.name.clone(),
                interface: interface.to_string(),
            },
        ));
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

/// `ty`, or the type an alias of it resolves to.
fn resolved(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Ty {
    let mut seen = Vec::new();
    let mut current = ty.clone();
    while let Ty::TypeAlias(head) = &current {
        if crate::impls::is_json_alias(db, head) || seen.contains(head) {
            break;
        }
        let Some(alias) = crate::layout::alias_ref_of(db, head) else {
            break;
        };
        seen.push(head.clone());
        current = crate::layout::alias_value(db, alias);
    }
    current
}

/// Whether `ty` is `map<string, json>`, through any aliases.
fn is_json_object(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    match resolved(db, ty) {
        Ty::Map { key, value } => {
            matches!(resolved(db, &key), Ty::String) && is_json(db, &resolved(db, &value))
        }
        _ => false,
    }
}

fn is_json(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    matches!(ty, Ty::TypeAlias(head) if crate::impls::is_json_alias(db, head))
}

/// The literal `ty` is, through any aliases.
fn literal_of(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Option<Literal> {
    match resolved(db, ty) {
        Ty::Literal(literal, _) => Some(literal),
        _ => None,
    }
}

/// Whether a field of type `ty` is optional: its type has a `null` arm, or it
/// names an alias that resolves to one. `json` holds `null` as a value.
pub fn is_optional(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> bool {
    matches!(resolved(db, ty), Ty::Union(members) if members.iter().any(|member| matches!(member, Ty::Null)))
}

/// The first union in `ty` that can't pick a member by tag: it has a
/// `@json.fallback` member, and a member the tags can't tell apart.
pub fn union_error(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Option<TirTypeError> {
    match ty {
        Ty::Union(members) => {
            let mut flat = Vec::new();
            members_of(db, ty, &mut Vec::new(), &mut flat);
            fallback_union_error(db, &flat)
                .or_else(|| members.iter().find_map(|member| union_error(db, member)))
        }
        Ty::List(element) => union_error(db, element),
        Ty::Map { value, .. } => union_error(db, value),
        Ty::Class(_, args) => args.iter().find_map(|arg| union_error(db, arg)),
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

/// The class `ty` names, with what the union check needs to know about it: a
/// class from another package is taken at its word.
fn source_fields(
    db: &dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<Vec<(Name, Ty, ClassFieldAttrs)>> {
    match crate::layout::class_ref_of(db, head)? {
        DeclRef::Source(class) => Some(crate::lower::resolve_class_fields(db, class).clone()),
        DeclRef::External(_) => None,
    }
}

/// Each literal field among `fields`, by its JSON key.
fn literals(
    db: &dyn baml_compiler2_hir::Db,
    fields: &[(Name, Ty, ClassFieldAttrs)],
) -> Vec<(String, Literal)> {
    fields
        .iter()
        .filter(|(_, _, attrs)| !attrs.skip)
        .filter_map(|(name, ty, attrs)| {
            let key = attrs.json_rename.as_deref().unwrap_or(name.as_str());
            Some((key.to_string(), literal_of(db, ty)?))
        })
        .collect()
}

fn fallback_union_error(db: &dyn baml_compiler2_hir::Db, members: &[Ty]) -> Option<TirTypeError> {
    let mut fallback: Option<(Name, Vec<(String, Literal)>)> = None;
    let mut tagged: Vec<(&Ty, Vec<(String, Literal)>)> = Vec::new();
    let mut untagged = Vec::new();
    for member in members {
        match member {
            Ty::Class(head, _) => {
                // A class this check can't see is taken at its word.
                let Some(fields) = source_fields(db, head) else {
                    continue;
                };
                let is_fallback = fields.iter().any(|(_, _, attrs)| attrs.json_fallback);
                if is_fallback && fallback.is_none() {
                    fallback = Some((head.name().clone(), literals(db, &fields)));
                } else if is_fallback {
                    untagged.push(member);
                } else {
                    tagged.push((member, literals(db, &fields)));
                }
            }
            // Each reads any object, with nothing to match on.
            Ty::Map { .. } | Ty::Unknown | Ty::Interface(..) | Ty::Media(_) | Ty::TypeVar(_) => {
                untagged.push(member);
            }
            member if is_json(db, member) => untagged.push(member),
            _ => {}
        }
    }
    let (name, tags) = fallback?;
    if let Some(member) = untagged.first() {
        return Some(TirTypeError::JsonFallbackUntaggedMember {
            fallback: name,
            member: (*member).clone(),
        });
    }
    let literal_sets: Vec<_> = tagged
        .iter()
        .map(|(_, literals)| literals.clone())
        .collect();
    match tag_tree(&literal_sets, &tags) {
        Ok(_) | Err(Untagged::NoTag) => None,
        Err(Untagged::Member(index)) => Some(TirTypeError::JsonFallbackUntaggedMember {
            fallback: name,
            member: tagged[index].0.clone(),
        }),
        Err(Untagged::Overlap(first, second)) => Some(TirTypeError::JsonFallbackOverlap {
            first: tagged[first].0.clone(),
            second: tagged[second].0.clone(),
        }),
    }
}
