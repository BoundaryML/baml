//! Shared TIR → baml_type conversion.
//!
//! Called from two sites:
//! 1. VIR lowering (`lower_ty`)
//! 2. VIR schema lowering (`schema_lower::convert_ty`)

use std::collections::{HashMap, HashSet};

use baml_base::{Literal, Name};
use baml_compiler_tir::{self, LiteralValue, Namespace, QualifiedName};

use crate::{Ty, TypeName};

/// Convert a `QualifiedName` to a `TypeName`, pre-computing the display string.
pub fn fqn_to_type_name(fqn: &QualifiedName) -> TypeName {
    let display_name = Name::new(fqn.display());
    let module_path = match &fqn.namespace {
        Namespace::Local => vec![],
        Namespace::Builtin { path } => {
            let mut p = vec![Name::new("baml")];
            p.extend(path.iter().cloned());
            p
        }
        Namespace::BamlStd { path } => {
            let mut p = vec![Name::new("baml")];
            p.extend(path.iter().cloned());
            p
        }
        Namespace::UserModule { module_path } => module_path.clone(),
        Namespace::Package {
            package_name,
            module_path,
        } => {
            let mut p = vec![package_name.clone()];
            p.extend(module_path.iter().cloned());
            p
        }
    };
    TypeName {
        name: fqn.name.clone(),
        module_path,
        display_name,
    }
}

/// Convert a TIR `LiteralValue` to a `baml_base::Literal`.
fn convert_literal(lit: &LiteralValue) -> Literal {
    match lit {
        LiteralValue::Int(i) => Literal::Int(*i),
        LiteralValue::Float(s) => Literal::Float(s.clone()),
        LiteralValue::String(s) => Literal::String(s.clone()),
        LiteralValue::Bool(b) => Literal::Bool(*b),
    }
}

/// Convert a TIR type to `baml_type::Ty`.
///
/// This is the shared conversion called from both VIR lowering and schema extraction.
/// - Converts `QualifiedName` → `TypeName`
/// - Expands non-recursive type aliases using `aliases` map
/// - Preserves literal types (no erasure)
/// - Returns `Err` if a recursive type alias is encountered (caller decides policy)
///
/// `aliases`: the `HashMap<Name, baml_compiler_tir::Ty>` from `type_aliases(db, project)`
/// `recursive_aliases`: result of `find_recursive_aliases(&aliases)`
pub fn convert_tir_ty(
    tir_ty: &baml_compiler_tir::Ty,
    aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> Result<Ty, String> {
    match tir_ty {
        baml_compiler_tir::Ty::Int { attr } => Ok(Ty::Int { attr: attr.clone() }),
        baml_compiler_tir::Ty::Float { attr } => Ok(Ty::Float { attr: attr.clone() }),
        baml_compiler_tir::Ty::String { attr } => Ok(Ty::String { attr: attr.clone() }),
        baml_compiler_tir::Ty::Bool { attr } => Ok(Ty::Bool { attr: attr.clone() }),
        baml_compiler_tir::Ty::Null { attr } => Ok(Ty::Null { attr: attr.clone() }),
        baml_compiler_tir::Ty::Media(kind, attr) => Ok(Ty::Media(*kind, attr.clone())),

        baml_compiler_tir::Ty::Literal(lit, attr) => {
            Ok(Ty::Literal(convert_literal(lit), attr.clone()))
        }

        baml_compiler_tir::Ty::Class(fqn, attr) => {
            // Check if this builtin type has a dedicated VM heap variant.
            // Most builtins are Object::Instance, but PromptAst wraps an opaque
            // Rust ADT and has its own Object variant.
            if baml_compiler_tir::is_prompt_ast_class(fqn) {
                return Ok(Ty::opaque_builtin(
                    "baml.llm.PromptAst",
                    "baml.llm.PromptAst",
                    attr.clone(),
                ));
            }
            Ok(Ty::Class(fqn_to_type_name(fqn), attr.clone()))
        }
        baml_compiler_tir::Ty::Enum(fqn, attr) => Ok(Ty::Enum(fqn_to_type_name(fqn), attr.clone())),

        baml_compiler_tir::Ty::TypeAlias(fqn, attr) => {
            let name = &fqn.name;
            if recursive_aliases.contains(name) {
                // Recursive alias — cannot expand. Return as TypeAlias for
                // compiler-level subtyping, or error at runtime boundary.
                Ok(Ty::TypeAlias(fqn_to_type_name(fqn), attr.clone()))
            } else if let Some(resolved) = aliases.get(name) {
                // Non-recursive alias — expand inline, preserving source attr if non-default
                let expanded = convert_tir_ty(resolved, aliases, recursive_aliases)?;
                if attr.is_default() {
                    Ok(expanded)
                } else {
                    Ok(expanded.with_attr(attr.clone()))
                }
            } else {
                // Alias not found — shouldn't happen after validation
                Ok(Ty::Null { attr: attr.clone() })
            }
        }

        baml_compiler_tir::Ty::Optional(inner, attr) => Ok(Ty::Optional(
            Box::new(convert_tir_ty(inner, aliases, recursive_aliases)?),
            attr.clone(),
        )),
        baml_compiler_tir::Ty::List(inner, attr) => Ok(Ty::List(
            Box::new(convert_tir_ty(inner, aliases, recursive_aliases)?),
            attr.clone(),
        )),
        baml_compiler_tir::Ty::Map { key, value, attr } => Ok(Ty::Map {
            key: Box::new(convert_tir_ty(key, aliases, recursive_aliases)?),
            value: Box::new(convert_tir_ty(value, aliases, recursive_aliases)?),
            attr: attr.clone(),
        }),
        baml_compiler_tir::Ty::Union(types, attr) => {
            let converted: Result<Vec<_>, _> = types
                .iter()
                .map(|t| convert_tir_ty(t, aliases, recursive_aliases))
                .collect();
            Ok(Ty::Union(converted?, attr.clone()))
        }

        baml_compiler_tir::Ty::Function { params, ret, attr } => {
            let converted_params: Result<Vec<_>, _> = params
                .iter()
                .map(|(_, t)| convert_tir_ty(t, aliases, recursive_aliases))
                .collect();
            Ok(Ty::Function {
                params: converted_params?,
                ret: Box::new(convert_tir_ty(ret, aliases, recursive_aliases)?),
                attr: attr.clone(),
            })
        }

        // Unknown and Error are TIR-only error recovery types.
        // All real type checking happens in TIR; by the time we convert to
        // baml_type, these just mean "no meaningful type" → map to Null.
        baml_compiler_tir::Ty::Unknown { attr } => Ok(Ty::Null { attr: attr.clone() }),
        baml_compiler_tir::Ty::Error { attr, .. } => Ok(Ty::Null { attr: attr.clone() }),
        baml_compiler_tir::Ty::Void { attr } => Ok(Ty::Void { attr: attr.clone() }),
        baml_compiler_tir::Ty::Resource { attr } => Ok(Ty::opaque_builtin(
            "baml.llm.Resource",
            "baml.llm.Resource",
            attr.clone(),
        )),
        // BuiltinUnknown is preserved for VIR type checking at call sites.
        baml_compiler_tir::Ty::BuiltinUnknown { attr } => {
            Ok(Ty::BuiltinUnknown { attr: attr.clone() })
        }
        baml_compiler_tir::Ty::Type { attr } => Ok(Ty::opaque_builtin(
            "baml.reflect.Type",
            "type",
            attr.clone(),
        )),

        baml_compiler_tir::Ty::WatchAccessor(inner, attr) => Ok(Ty::WatchAccessor(
            Box::new(convert_tir_ty(inner, aliases, recursive_aliases)?),
            attr.clone(),
        )),
    }
}

/// Sanitize a `baml_type::Ty` for runtime use.
///
/// Converts compiler-only variants to safe runtime equivalents,
/// matching the behavior of the old `convert_tir_ty_to_program_ty`.
/// Called after `convert_tir_ty` in the schema extraction path.
pub fn sanitize_for_runtime(ty: Ty) -> Result<Ty, String> {
    match ty {
        // Compiler-only → Null (preserves backwards compatibility)
        // Note: Unknown/Error/Never don't exist in baml_type::Ty — they were
        // already mapped to Null/Void during convert_tir_ty.
        Ty::Void { attr } => Ok(Ty::Null { attr }),
        Ty::BuiltinUnknown { attr } => Ok(Ty::BuiltinUnknown { attr }),
        Ty::Function {
            params, ret, attr, ..
        } => Ok(Ty::Function {
            params: params
                .into_iter()
                .map(sanitize_for_runtime)
                .collect::<Result<Vec<_>, _>>()?,
            ret: Box::new(sanitize_for_runtime(*ret)?),
            attr,
        }),
        // WatchAccessor → recursively sanitize inner type, preserving wrapper
        Ty::WatchAccessor(inner, attr) => Ok(Ty::WatchAccessor(
            Box::new(sanitize_for_runtime(*inner)?),
            attr,
        )),
        // Recursive TypeAlias → error
        Ty::TypeAlias(ref tn, _) => Err(format!(
            "Recursive type alias '{}' cannot be used in class fields or function return types",
            tn.display_name
        )),
        // Recurse into containers
        Ty::Optional(inner, attr) => {
            Ok(Ty::Optional(Box::new(sanitize_for_runtime(*inner)?), attr))
        }
        Ty::List(inner, attr) => Ok(Ty::List(Box::new(sanitize_for_runtime(*inner)?), attr)),
        Ty::Map { key, value, attr } => Ok(Ty::Map {
            key: Box::new(sanitize_for_runtime(*key)?),
            value: Box::new(sanitize_for_runtime(*value)?),
            attr,
        }),
        Ty::Union(members, attr) => {
            let sanitized: Result<Vec<_>, _> =
                members.into_iter().map(sanitize_for_runtime).collect();
            Ok(Ty::Union(sanitized?, attr))
        }
        // All other variants pass through
        other => Ok(other),
    }
}
