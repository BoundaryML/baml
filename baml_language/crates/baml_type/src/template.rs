//! `TyTemplate` — a parallel to `Ty` that may contain `TypeArgRef(N)` leaves.
//!
//! Used by the `LoadType` VM instruction to represent type expressions that
//! contain references to generic type parameters.  A `TyTemplate` with no
//! `TypeArgRef` leaves is "fully concrete" and can be materialised into a `Ty`
//! without a substitution environment.
//!
//! # Design rationale
//!
//! `TyTemplate` is deliberately kept separate from `Ty` (rather than adding a
//! `TypeArgRef` variant to `Ty` itself).  This ensures that post-concretisation
//! `Ty` values that reach the runtime via `Object::Type` never contain
//! unresolved references, making the invariant easy to audit.
//!
//! # De Bruijn indexing
//!
//! `TypeArgRef(n)` refers to the n-th entry in the enclosing call-frame's
//! `type_args` vector (0-based).  The compiler assigns indices using
//! `function_generic_params` ordering so that the mapping is stable across
//! compilation.

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Ty, TyAttr, TypeName};

/// A type expression that may contain unresolved generic-parameter references.
#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum TyTemplate {
    /// Fully concrete leaf — no substitution needed.
    Concrete(Ty),
    /// De Bruijn index into the enclosing frame's `type_args`.
    TypeArgRef(u32),
    /// `T[]`
    Array(Box<TyTemplate>),
    /// `T1 | T2 | ...` (a member being `Concrete(null)` encodes optionality)
    Union(Vec<TyTemplate>),
    /// `map<K, V>`
    Map(Box<TyTemplate>, Box<TyTemplate>),
    /// `Class<A1, A2, ...>` (generic class instantiation)
    Class(TypeName, Vec<TyTemplate>),
    /// `Interface<A1, Assoc = A2, ...>` (generic interface instantiation)
    Interface(TypeName, Vec<TyTemplate>, Vec<(crate::Name, TyTemplate)>),
    /// Matches any type at this position. Only meaningful as a type-argument
    /// of a `Class` template in an `IsType` guard (BEP-044): it lets a partial
    /// guard pin some class type-args while leaving others unconstrained — e.g.
    /// `Pair<string, _>` to dispatch `implements Getter<L>` on a requested
    /// `Getter<string>` without constraining `R`. Never materialized via
    /// `LoadType`.
    Wildcard,
}

impl TyTemplate {
    /// Walk the template once, substituting each `TypeArgRef(n)` with
    /// `type_args[n]`.
    ///
    /// If `n` is out of range (e.g. when a generic function is called from a
    /// non-generic context that didn't supply type arguments), the substitution
    /// falls back to `Ty::unknown()` rather than panicking.  This handles
    /// stdlib paths where T/S are declared in the function signature for
    /// type-checking purposes but the corresponding Rust sys-op implementation
    /// doesn't use the type-arg slot.
    pub fn substitute(&self, type_args: &[Ty]) -> Ty {
        match self {
            Self::Concrete(t) => t.clone(),
            Self::TypeArgRef(n) => type_args
                .get(*n as usize)
                .cloned()
                .unwrap_or_else(Ty::unknown),
            Self::Array(inner) => Ty::list(inner.substitute(type_args)),
            Self::Union(parts) => Ty::union(parts.iter().map(|p| p.substitute(type_args))),
            Self::Map(k, v) => Ty::Map {
                key: Box::new(k.substitute(type_args)),
                value: Box::new(v.substitute(type_args)),
                attr: TyAttr::default(),
            },
            Self::Class(name, args) => {
                let resolved: Vec<Ty> = args.iter().map(|a| a.substitute(type_args)).collect();
                Ty::Class(name.clone(), resolved, TyAttr::default())
            }
            Self::Interface(name, args, associated_bindings) => {
                let resolved_args: Vec<Ty> = args.iter().map(|a| a.substitute(type_args)).collect();
                let resolved_bindings = associated_bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.substitute(type_args)))
                    .collect();
                Ty::Interface(
                    name.clone(),
                    resolved_args,
                    resolved_bindings,
                    TyAttr::default(),
                )
            }
            // A wildcard never reaches `LoadType` materialization; if it ever
            // does, fall back to `unknown` rather than panicking.
            Self::Wildcard => Ty::unknown(),
        }
    }

    /// Returns `true` when no `TypeArgRef` appears anywhere in the template.
    pub fn is_fully_concrete(&self) -> bool {
        match self {
            Self::Concrete(_) => true,
            Self::TypeArgRef(_) => false,
            Self::Array(inner) => inner.is_fully_concrete(),
            Self::Union(parts) => parts.iter().all(TyTemplate::is_fully_concrete),
            Self::Map(k, v) => k.is_fully_concrete() && v.is_fully_concrete(),
            Self::Class(_, args) => args.iter().all(TyTemplate::is_fully_concrete),
            Self::Interface(_, args, associated_bindings) => {
                args.iter().all(TyTemplate::is_fully_concrete)
                    && associated_bindings
                        .iter()
                        .all(|(_, ty)| ty.is_fully_concrete())
            }
            // No `TypeArgRef` to substitute, so it needs no environment.
            Self::Wildcard => true,
        }
    }
}

impl fmt::Display for TyTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concrete(ty) => write!(f, "{ty}"),
            Self::TypeArgRef(n) => write!(f, "#{n}"),
            Self::Array(inner) => write!(f, "{inner}[]"),
            Self::Union(parts) => {
                // `?` is sugar that exists only in source/lowering; after that a
                // nullable type is a plain union.
                let strs: Vec<String> = parts.iter().map(ToString::to_string).collect();
                write!(f, "{}", strs.join(" | "))
            }
            Self::Map(k, v) => write!(f, "map<{k}, {v}>"),
            Self::Class(name, args) => {
                write!(f, "{name}")?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{arg}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Interface(name, args, associated_bindings) => {
                write!(f, "{name}")?;
                if !args.is_empty() || !associated_bindings.is_empty() {
                    write!(f, "<")?;
                    let mut first = true;
                    for arg in args {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{arg}")?;
                    }
                    for (name, ty) in associated_bindings {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{name} = {ty}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Wildcard => write!(f, "_"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_template_substitutes_to_itself() {
        let tmpl = TyTemplate::Concrete(Ty::int());
        assert_eq!(tmpl.substitute(&[]), Ty::int());
        assert!(tmpl.is_fully_concrete());
    }

    #[test]
    fn type_arg_ref_substitutes_correctly() {
        let tmpl = TyTemplate::TypeArgRef(0);
        let ty = tmpl.substitute(&[Ty::string()]);
        assert_eq!(ty, Ty::string());
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn array_of_type_arg_ref() {
        // Array(TypeArgRef(0)) with type_args=[int] → int[]
        let tmpl = TyTemplate::Array(Box::new(TyTemplate::TypeArgRef(0)));
        let ty = tmpl.substitute(&[Ty::int()]);
        assert_eq!(ty, Ty::list(Ty::int()));
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn optional_of_type_arg_ref() {
        // (TypeArgRef(0) | null) with type_args=[string] → string?
        let tmpl = TyTemplate::Union(vec![
            TyTemplate::TypeArgRef(0),
            TyTemplate::Concrete(Ty::null()),
        ]);
        let ty = tmpl.substitute(&[Ty::string()]);
        assert_eq!(ty, Ty::optional(Ty::string()));
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn concrete_array_is_fully_concrete() {
        let tmpl = TyTemplate::Array(Box::new(TyTemplate::Concrete(Ty::int())));
        assert!(tmpl.is_fully_concrete());
    }

    #[test]
    fn union_of_concrete_is_fully_concrete() {
        let tmpl = TyTemplate::Union(vec![
            TyTemplate::Concrete(Ty::int()),
            TyTemplate::Concrete(Ty::string()),
        ]);
        assert!(tmpl.is_fully_concrete());
        let ty = tmpl.substitute(&[]);
        assert_eq!(ty, Ty::union([Ty::int(), Ty::string()]));
    }

    #[test]
    fn union_containing_type_arg_ref_not_concrete() {
        let tmpl = TyTemplate::Union(vec![
            TyTemplate::Concrete(Ty::int()),
            TyTemplate::TypeArgRef(0),
        ]);
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn class_with_type_arg_ref_substitution() {
        // Class("Container", [TypeArgRef(0)]) with type_args=[user_class("User")]
        // → Ty::Class("Container", [user_class("User")], _)
        let tmpl = TyTemplate::Class(
            TypeName::local(crate::Name::new("Container")),
            vec![TyTemplate::TypeArgRef(0)],
        );
        let user = Ty::user_class("User");
        let result = tmpl.substitute(std::slice::from_ref(&user));
        assert_eq!(
            result,
            Ty::class_with_args(TypeName::local(crate::Name::new("Container")), vec![user])
        );
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn class_no_args_is_fully_concrete() {
        let tmpl = TyTemplate::Class(TypeName::local(crate::Name::new("User")), vec![]);
        assert!(tmpl.is_fully_concrete());
        let result = tmpl.substitute(&[]);
        assert_eq!(
            result,
            Ty::Class(
                TypeName::local(crate::Name::new("User")),
                vec![],
                crate::TyAttr::default()
            )
        );
    }
}
