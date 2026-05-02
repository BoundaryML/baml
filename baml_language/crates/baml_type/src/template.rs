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

use crate::{Ty, TyAttr, TypeName};

/// A type expression that may contain unresolved generic-parameter references.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TyTemplate {
    /// Fully concrete leaf — no substitution needed.
    Concrete(Ty),
    /// De Bruijn index into the enclosing frame's `type_args`.
    TypeArgRef(u32),
    /// `T[]`
    Array(Box<TyTemplate>),
    /// `T?`
    Optional(Box<TyTemplate>),
    /// `T1 | T2 | ...`
    Union(Vec<TyTemplate>),
    /// `map<K, V>`
    Map(Box<TyTemplate>, Box<TyTemplate>),
    /// `Class<A1, A2, ...>` (generic class instantiation)
    Class(TypeName, Vec<TyTemplate>),
}

impl TyTemplate {
    /// Walk the template once, substituting each `TypeArgRef(n)` with
    /// `type_args[n]`.  Panics if `n` is out of range.
    pub fn substitute(&self, type_args: &[Ty]) -> Ty {
        match self {
            Self::Concrete(t) => t.clone(),
            Self::TypeArgRef(n) => type_args[*n as usize].clone(),
            Self::Array(inner) => Ty::list(inner.substitute(type_args)),
            Self::Optional(inner) => Ty::optional(inner.substitute(type_args)),
            Self::Union(parts) => Ty::union(parts.iter().map(|p| p.substitute(type_args))),
            Self::Map(k, v) => Ty::Map {
                key: Box::new(k.substitute(type_args)),
                value: Box::new(v.substitute(type_args)),
                attr: TyAttr::default(),
            },
            Self::Class(name, args) => {
                // Build a Class with the resolved type args embedded.
                // Class args are not yet first-class in `Ty`, so we
                // produce a plain `Ty::Class` for the monomorphic case and
                // extend this when parametric class support is added.
                let _ = args; // args are resolved but Ty::Class ignores them for now
                Ty::Class(name.clone(), TyAttr::default())
            }
        }
    }

    /// Returns `true` when no `TypeArgRef` appears anywhere in the template.
    pub fn is_fully_concrete(&self) -> bool {
        match self {
            Self::Concrete(_) => true,
            Self::TypeArgRef(_) => false,
            Self::Array(inner) | Self::Optional(inner) => inner.is_fully_concrete(),
            Self::Union(parts) => parts.iter().all(TyTemplate::is_fully_concrete),
            Self::Map(k, v) => k.is_fully_concrete() && v.is_fully_concrete(),
            Self::Class(_, args) => args.iter().all(TyTemplate::is_fully_concrete),
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
        // Optional(TypeArgRef(0)) with type_args=[string] → string?
        let tmpl = TyTemplate::Optional(Box::new(TyTemplate::TypeArgRef(0)));
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
}
