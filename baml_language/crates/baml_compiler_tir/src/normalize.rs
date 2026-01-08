//! Type normalization and subtyping.
//!
//! This module converts surface `Ty` types to an internal `StructuralTy`
//! representation where all type aliases are resolved. Recursive aliases
//! are represented using Mu types.

use std::collections::{HashMap, HashSet};

use baml_base::Name;

use crate::types::{LiteralValue, Ty};

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Check if `sub` is a subtype of `sup`, resolving type aliases.
pub(crate) fn is_subtype_of(sub: &Ty, sup: &Ty, aliases: &HashMap<Name, Ty>) -> bool {
    let recursive = find_recursive_aliases(aliases);
    let sub_norm = normalize(sub, aliases, &recursive);
    let sup_norm = normalize(sup, aliases, &recursive);
    sub_norm.is_subtype_of(&sup_norm, &mut HashSet::new())
}

// ═══════════════════════════════════════════════════════════════════════════
// STRUCTURAL TYPE (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Normalized structural type. All aliases resolved, recursion explicit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum StructuralTy {
    // Primitives
    Int,
    Float,
    String,
    Bool,
    Null,
    // Media
    Image,
    Audio,
    Video,
    Pdf,
    // Literal
    Literal(LiteralValue),
    // User-defined (resolved by name)
    Class(Name),
    Enum(Name),
    // Constructors
    Optional(Box<StructuralTy>),
    List(Box<StructuralTy>),
    Map {
        key: Box<StructuralTy>,
        value: Box<StructuralTy>,
    },
    Union(Vec<StructuralTy>),
    Function {
        params: Vec<StructuralTy>,
        ret: Box<StructuralTy>,
    },
    // Recursion
    Mu {
        var: Name,
        body: Box<StructuralTy>,
    },
    TyVar(Name),
    // Special
    Unknown,
    Error,
    Void,
    WatchAccessor(Box<StructuralTy>),
}

impl StructuralTy {
    /// Equirecursive subtyping with co-inductive assumptions.
    fn is_subtype_of(
        &self,
        other: &StructuralTy,
        assumptions: &mut HashSet<(StructuralTy, StructuralTy)>,
    ) -> bool {
        // Co-inductive: if we've assumed this pair, it holds
        let pair = (self.clone(), other.clone());
        if assumptions.contains(&pair) {
            return true;
        }

        // Reflexivity
        if self == other {
            return true;
        }

        // Error recovery
        if matches!(self, StructuralTy::Unknown | StructuralTy::Error)
            || matches!(other, StructuralTy::Unknown | StructuralTy::Error)
        {
            return true;
        }

        assumptions.insert(pair.clone());

        let result = match (self, other) {
            // Mu unfolding
            (StructuralTy::Mu { var, body }, other) => {
                let unfolded = substitute(body, var, self);
                unfolded.is_subtype_of(other, assumptions)
            }
            (self_ty, StructuralTy::Mu { var, body }) => {
                let unfolded = substitute(body, var, other);
                self_ty.is_subtype_of(&unfolded, assumptions)
            }

            // TyVar (inside Mu bodies)
            (StructuralTy::TyVar(v1), StructuralTy::TyVar(v2)) => v1 == v2,

            // Null <: Optional<T>
            (StructuralTy::Null, StructuralTy::Optional(_)) => true,

            // T <: Optional<T>
            (inner, StructuralTy::Optional(opt_inner)) => {
                inner.is_subtype_of(opt_inner, assumptions)
            }

            // Optional<T> <: T | null
            (StructuralTy::Optional(inner), StructuralTy::Union(types)) => {
                types.contains(&StructuralTy::Null)
                    && types.iter().any(|t| inner.is_subtype_of(t, assumptions))
            }

            // T <: T | U
            (inner, StructuralTy::Union(types)) => {
                types.iter().any(|t| inner.is_subtype_of(t, assumptions))
            }

            // Union<T1, T2> <: U iff all Ti <: U
            (StructuralTy::Union(types), other) => {
                types.iter().all(|t| t.is_subtype_of(other, assumptions))
            }

            // List covariance
            (StructuralTy::List(inner1), StructuralTy::List(inner2)) => {
                inner1.is_subtype_of(inner2, assumptions)
            }

            // Map covariance in value, invariant in key
            (
                StructuralTy::Map { key: k1, value: v1 },
                StructuralTy::Map { key: k2, value: v2 },
            ) => k1 == k2 && v1.is_subtype_of(v2, assumptions),

            // Int <: Float
            (StructuralTy::Int, StructuralTy::Float) => true,

            // Literal types are subtypes of their base types
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Int) => true,
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::Float(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::String(_)), StructuralTy::String) => true,
            (StructuralTy::Literal(LiteralValue::Bool(_)), StructuralTy::Bool) => true,

            _ => false,
        };

        assumptions.remove(&pair);
        result
    }
}

/// Substitute `TyVar` with replacement in type.
fn substitute(ty: &StructuralTy, var: &Name, replacement: &StructuralTy) -> StructuralTy {
    match ty {
        StructuralTy::TyVar(v) if v == var => replacement.clone(),
        StructuralTy::Optional(inner) => {
            StructuralTy::Optional(Box::new(substitute(inner, var, replacement)))
        }
        StructuralTy::List(inner) => {
            StructuralTy::List(Box::new(substitute(inner, var, replacement)))
        }
        StructuralTy::Map { key, value } => StructuralTy::Map {
            key: Box::new(substitute(key, var, replacement)),
            value: Box::new(substitute(value, var, replacement)),
        },
        StructuralTy::Union(types) => StructuralTy::Union(
            types
                .iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
        ),
        StructuralTy::Function { params, ret } => StructuralTy::Function {
            params: params
                .iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
            ret: Box::new(substitute(ret, var, replacement)),
        },
        StructuralTy::Mu { var: v, body } if v != var => StructuralTy::Mu {
            var: v.clone(),
            body: Box::new(substitute(body, var, replacement)),
        },
        StructuralTy::WatchAccessor(inner) => {
            StructuralTy::WatchAccessor(Box::new(substitute(inner, var, replacement)))
        }
        _ => ty.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION (private)
// ═══════════════════════════════════════════════════════════════════════════

fn normalize(ty: &Ty, aliases: &HashMap<Name, Ty>, recursive: &HashSet<Name>) -> StructuralTy {
    let mut expanding = HashSet::new();
    normalize_impl(ty, aliases, recursive, &mut expanding)
}

fn normalize_impl(
    ty: &Ty,
    aliases: &HashMap<Name, Ty>,
    recursive: &HashSet<Name>,
    expanding: &mut HashSet<Name>,
) -> StructuralTy {
    match ty {
        // Direct conversions
        Ty::Int => StructuralTy::Int,
        Ty::Float => StructuralTy::Float,
        Ty::String => StructuralTy::String,
        Ty::Bool => StructuralTy::Bool,
        Ty::Null => StructuralTy::Null,
        Ty::Image => StructuralTy::Image,
        Ty::Audio => StructuralTy::Audio,
        Ty::Video => StructuralTy::Video,
        Ty::Pdf => StructuralTy::Pdf,
        Ty::Unknown => StructuralTy::Unknown,
        Ty::Error => StructuralTy::Error,
        Ty::Void => StructuralTy::Void,
        Ty::Literal(lit) => StructuralTy::Literal(lit.clone()),
        Ty::Class(name) => StructuralTy::Class(name.clone()),
        Ty::Enum(name) => StructuralTy::Enum(name.clone()),
        Ty::WatchAccessor(inner) => StructuralTy::WatchAccessor(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),

        // Named: resolve alias
        Ty::Named(name) => {
            if expanding.contains(name) {
                // Back-reference in recursive expansion
                return StructuralTy::TyVar(name.clone());
            }

            if let Some(alias_ty) = aliases.get(name) {
                if recursive.contains(name) {
                    // Recursive: wrap in Mu
                    expanding.insert(name.clone());
                    let body = normalize_impl(alias_ty, aliases, recursive, expanding);
                    expanding.remove(name);
                    StructuralTy::Mu {
                        var: name.clone(),
                        body: Box::new(body),
                    }
                } else {
                    // Non-recursive: expand inline
                    normalize_impl(alias_ty, aliases, recursive, expanding)
                }
            } else {
                // Not an alias - this shouldn't happen if TIR lowering is correct.
                // Classes/enums should be Ty::Class/Ty::Enum, not Ty::Named.
                // Treat as error for now (error recovery will handle it).
                StructuralTy::Error
            }
        }

        // Type constructors
        Ty::Optional(inner) => StructuralTy::Optional(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::List(inner) => StructuralTy::List(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::Map { key, value } => StructuralTy::Map {
            key: Box::new(normalize_impl(key, aliases, recursive, expanding)),
            value: Box::new(normalize_impl(value, aliases, recursive, expanding)),
        },
        Ty::Union(types) => StructuralTy::Union(
            types
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
        ),
        Ty::Function { params, ret } => StructuralTy::Function {
            params: params
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
            ret: Box::new(normalize_impl(ret, aliases, recursive, expanding)),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CYCLE DETECTION (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Find all recursive type aliases via DFS.
fn find_recursive_aliases(aliases: &HashMap<Name, Ty>) -> HashSet<Name> {
    let mut recursive = HashSet::new();
    for name in aliases.keys() {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        if has_cycle(name, aliases, &mut visited, &mut stack) {
            recursive.insert(name.clone());
        }
    }
    recursive
}

fn has_cycle(
    name: &Name,
    aliases: &HashMap<Name, Ty>,
    visited: &mut HashSet<Name>,
    stack: &mut HashSet<Name>,
) -> bool {
    if stack.contains(name) {
        return true;
    }
    if visited.contains(name) {
        return false;
    }
    visited.insert(name.clone());
    stack.insert(name.clone());
    let result = aliases
        .get(name)
        .is_some_and(|ty| ty_has_cycle(ty, aliases, visited, stack));
    stack.remove(name);
    result
}

fn ty_has_cycle(
    ty: &Ty,
    aliases: &HashMap<Name, Ty>,
    visited: &mut HashSet<Name>,
    stack: &mut HashSet<Name>,
) -> bool {
    match ty {
        Ty::Named(name) if aliases.contains_key(name) => has_cycle(name, aliases, visited, stack),
        Ty::Optional(inner) | Ty::List(inner) => ty_has_cycle(inner, aliases, visited, stack),
        Ty::Map { key, value } => {
            ty_has_cycle(key, aliases, visited, stack)
                || ty_has_cycle(value, aliases, visited, stack)
        }
        Ty::Union(types) => types
            .iter()
            .any(|t| ty_has_cycle(t, aliases, visited, stack)),
        Ty::Function { params, ret } => {
            params
                .iter()
                .any(|t| ty_has_cycle(t, aliases, visited, stack))
                || ty_has_cycle(ret, aliases, visited, stack)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(Name::new("MyInt"), Ty::Int);

        // MyInt <: int should be true
        assert!(is_subtype_of(
            &Ty::Named(Name::new("MyInt")),
            &Ty::Int,
            &aliases
        ));

        // int <: MyInt should also be true (same structural type)
        assert!(is_subtype_of(
            &Ty::Int,
            &Ty::Named(Name::new("MyInt")),
            &aliases
        ));
    }

    #[test]
    fn test_transitive_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(Name::new("MyInt"), Ty::Int);
        aliases.insert(Name::new("AnotherInt"), Ty::Named(Name::new("MyInt")));

        // AnotherInt <: int
        assert!(is_subtype_of(
            &Ty::Named(Name::new("AnotherInt")),
            &Ty::Int,
            &aliases
        ));

        // AnotherInt <: MyInt
        assert!(is_subtype_of(
            &Ty::Named(Name::new("AnotherInt")),
            &Ty::Named(Name::new("MyInt")),
            &aliases
        ));
    }

    #[test]
    fn test_union_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            Name::new("IntOrString"),
            Ty::Union(vec![Ty::Int, Ty::String]),
        );

        // int <: IntOrString
        assert!(is_subtype_of(
            &Ty::Int,
            &Ty::Named(Name::new("IntOrString")),
            &aliases
        ));

        // string <: IntOrString
        assert!(is_subtype_of(
            &Ty::String,
            &Ty::Named(Name::new("IntOrString")),
            &aliases
        ));

        // bool NOT <: IntOrString
        assert!(!is_subtype_of(
            &Ty::Bool,
            &Ty::Named(Name::new("IntOrString")),
            &aliases
        ));
    }

    #[test]
    fn test_recursive_alias_detection() {
        let mut aliases = HashMap::new();
        // type List = int | List (simplified recursive type)
        aliases.insert(
            Name::new("List"),
            Ty::Union(vec![Ty::Null, Ty::Named(Name::new("List"))]),
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(recursive.contains(&Name::new("List")));
    }

    #[test]
    fn test_non_recursive_not_marked() {
        let mut aliases = HashMap::new();
        aliases.insert(Name::new("MyInt"), Ty::Int);

        let recursive = find_recursive_aliases(&aliases);
        assert!(!recursive.contains(&Name::new("MyInt")));
    }

    #[test]
    fn test_void_not_subtype_of_map() {
        let aliases = HashMap::new();
        let void_ty = Ty::Void;
        let map_ty = Ty::Map {
            key: Box::new(Ty::String),
            value: Box::new(Ty::Bool),
        };

        // Void should NOT be a subtype of Map
        assert!(!is_subtype_of(&void_ty, &map_ty, &aliases));
    }
}
