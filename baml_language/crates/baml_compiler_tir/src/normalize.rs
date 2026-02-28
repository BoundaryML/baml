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
    Media(baml_base::MediaKind),
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
    Resource,
    /// Internal-only type for builtins - any type is assignable to it.
    BuiltinUnknown,
    WatchAccessor(Box<StructuralTy>),
    Type,
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

        // Error recovery: Unknown/Error are compatible with anything to prevent cascading errors
        if matches!(self, StructuralTy::Unknown | StructuralTy::Error)
            || matches!(other, StructuralTy::Unknown | StructuralTy::Error)
        {
            return true;
        }

        // BuiltinUnknown: any type can be passed where BuiltinUnknown is expected,
        // but BuiltinUnknown cannot be used where a specific type is required.
        // T <: BuiltinUnknown  (any type assignable TO BuiltinUnknown)
        if matches!(other, StructuralTy::BuiltinUnknown) {
            return true;
        }
        // BuiltinUnknown </: T  (BuiltinUnknown not assignable to other types)
        // This is handled implicitly by not having a rule that returns true

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
            // (with error-recovery: Unknown keys are compatible with any key type)
            (
                StructuralTy::Map { key: k1, value: v1 },
                StructuralTy::Map { key: k2, value: v2 },
            ) => {
                let keys_compatible = k1 == k2
                    || matches!(k1.as_ref(), StructuralTy::Unknown | StructuralTy::Error)
                    || matches!(k2.as_ref(), StructuralTy::Unknown | StructuralTy::Error);
                keys_compatible && v1.is_subtype_of(v2, assumptions)
            }

            // Int <: Float
            (StructuralTy::Int, StructuralTy::Float) => true,

            // Literal types are subtypes of their base types
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Int) => true,
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::Float(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::String(_)), StructuralTy::String) => true,
            (StructuralTy::Literal(LiteralValue::Bool(_)), StructuralTy::Bool) => true,

            // Function subtyping: contravariant params, covariant return
            // (P1) -> R1 <: (P2) -> R2  iff  P2 <: P1 (contravariant) and R1 <: R2 (covariant)
            (
                StructuralTy::Function {
                    params: params1,
                    ret: ret1,
                },
                StructuralTy::Function {
                    params: params2,
                    ret: ret2,
                },
            ) => {
                // Return type is covariant: R1 <: R2
                if !ret1.is_subtype_of(ret2, assumptions) {
                    return false;
                }

                // Parameters are contravariant and treated as a "struct-like" tuple:
                // - () is a supertype of any param list (nullary function accepts anything)
                // - (T) is a supertype of () if T has a default/is optional (not modeled yet)
                // - For same-arity, each param is contravariant: P2[i] <: P1[i]
                //
                // A function with fewer parameters is more general (supertype) because
                // it ignores extra arguments. Think of it like a struct with optional fields.
                if params2.len() > params1.len() {
                    // sup expects more params than sub provides - sub can't satisfy sup's interface
                    return false;
                }

                // For each parameter position that sup expects, check contravariance
                // params2[i] <: params1[i] (note: reversed direction for contravariance)
                for (p1, p2) in params1.iter().zip(params2.iter()) {
                    if !p2.is_subtype_of(p1, assumptions) {
                        return false;
                    }
                }

                true
            }

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

/// Check if a type is valid as a map key.
///
/// Valid key types are: string, string literals, and unions of valid key types.
/// Enums are NOT valid (they're structurally distinct from string literals).
fn is_valid_map_key_type(ty: &Ty, aliases: &HashMap<Name, Ty>) -> bool {
    fn can_be_key(structural_ty: &StructuralTy) -> bool {
        match structural_ty {
            StructuralTy::String => true,
            StructuralTy::Literal(literal_value) => match literal_value {
                LiteralValue::String(_) => true,
                LiteralValue::Int(_) => false,
                LiteralValue::Float(_) => false,
                LiteralValue::Bool(_) => false,
            },
            StructuralTy::Error => true,
            StructuralTy::Union(variants) => variants.iter().all(can_be_key),
            StructuralTy::Int => false,
            StructuralTy::Float => false,
            StructuralTy::Bool => false,
            StructuralTy::Null => false,
            StructuralTy::Media(_) => false,
            StructuralTy::Class(_) => false,
            StructuralTy::Enum(_) => true,
            StructuralTy::Optional(_) => false,
            StructuralTy::List(_) => false,
            StructuralTy::Map { .. } => false,
            StructuralTy::Function { .. } => false,
            StructuralTy::Mu { .. } => false,
            StructuralTy::TyVar(_) => false,
            StructuralTy::Unknown => false,
            StructuralTy::Void => false,
            StructuralTy::Resource => false,
            StructuralTy::BuiltinUnknown => false,
            StructuralTy::WatchAccessor(_) => false,
            StructuralTy::Type => false,
        }
    }
    let recursive = find_recursive_aliases(aliases);
    let norm = normalize(ty, aliases, &recursive);
    can_be_key(&norm)
}

/// Find all invalid map key types within a type (recursively).
///
/// Returns a list of the invalid key types found. The caller should create
/// appropriate diagnostics for each.
pub(crate) fn find_invalid_map_keys(ty: &Ty, aliases: &HashMap<Name, Ty>) -> Vec<Ty> {
    let mut invalid_keys = Vec::new();
    find_invalid_map_keys_recursive(ty, aliases, &mut invalid_keys);
    invalid_keys
}

fn find_invalid_map_keys_recursive(
    ty: &Ty,
    aliases: &HashMap<Name, Ty>,
    invalid_keys: &mut Vec<Ty>,
) {
    match ty {
        Ty::Map { key, value, .. } => {
            if !is_valid_map_key_type(key, aliases) {
                invalid_keys.push((**key).clone());
            }
            find_invalid_map_keys_recursive(key, aliases, invalid_keys);
            find_invalid_map_keys_recursive(value, aliases, invalid_keys);
        }
        Ty::List(inner, _) => find_invalid_map_keys_recursive(inner, aliases, invalid_keys),
        Ty::Optional(inner, _) => find_invalid_map_keys_recursive(inner, aliases, invalid_keys),
        Ty::Union(types, _) => {
            for t in types {
                find_invalid_map_keys_recursive(t, aliases, invalid_keys);
            }
        }
        Ty::Function { params, ret, .. } => {
            for (_, p) in params {
                find_invalid_map_keys_recursive(p, aliases, invalid_keys);
            }
            find_invalid_map_keys_recursive(ret, aliases, invalid_keys);
        }
        _ => {}
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
        Ty::Int { .. } => StructuralTy::Int,
        Ty::Float { .. } => StructuralTy::Float,
        Ty::String { .. } => StructuralTy::String,
        Ty::Bool { .. } => StructuralTy::Bool,
        Ty::Null { .. } => StructuralTy::Null,
        Ty::Media(kind, _) => StructuralTy::Media(*kind),
        Ty::Unknown { .. } => StructuralTy::Unknown,
        Ty::Error { .. } => StructuralTy::Error,
        Ty::Void { .. } => StructuralTy::Void,
        Ty::Resource { .. } => StructuralTy::Resource,
        Ty::BuiltinUnknown { .. } => StructuralTy::BuiltinUnknown,
        Ty::Type { .. } => StructuralTy::Type,
        Ty::Literal(lit, _) => StructuralTy::Literal(lit.clone()),
        Ty::Class(fqn, _) => StructuralTy::Class(fqn.name.clone()),
        Ty::Enum(fqn, _) => StructuralTy::Enum(fqn.name.clone()),
        Ty::WatchAccessor(inner, _) => StructuralTy::WatchAccessor(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),

        // TypeAlias: resolve alias
        Ty::TypeAlias(fqn, _) => {
            let name = &fqn.name;
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
                // Not a known alias - this shouldn't happen if TIR lowering is correct.
                // Treat as error for now (error recovery will handle it).
                StructuralTy::Error
            }
        }

        // Type constructors
        Ty::Optional(inner, _) => StructuralTy::Optional(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::List(inner, _) => StructuralTy::List(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::Map { key, value, .. } => StructuralTy::Map {
            key: Box::new(normalize_impl(key, aliases, recursive, expanding)),
            value: Box::new(normalize_impl(value, aliases, recursive, expanding)),
        },
        Ty::Union(types, _) => StructuralTy::Union(
            types
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
        ),
        Ty::Function { params, ret, .. } => StructuralTy::Function {
            params: params
                .iter()
                .map(|(_, t)| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
            ret: Box::new(normalize_impl(ret, aliases, recursive, expanding)),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CYCLE DETECTION (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Find all recursive type aliases via DFS.
pub fn find_recursive_aliases(aliases: &HashMap<Name, Ty>) -> HashSet<Name> {
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
        Ty::TypeAlias(fqn, _) if aliases.contains_key(&fqn.name) => {
            has_cycle(&fqn.name, aliases, visited, stack)
        }
        Ty::Optional(inner, _) | Ty::List(inner, _) => ty_has_cycle(inner, aliases, visited, stack),
        Ty::Map { key, value, .. } => {
            ty_has_cycle(key, aliases, visited, stack)
                || ty_has_cycle(value, aliases, visited, stack)
        }
        Ty::Union(types, _) => types
            .iter()
            .any(|t| ty_has_cycle(t, aliases, visited, stack)),
        Ty::Function { params, ret, .. } => {
            params
                .iter()
                .any(|(_, t)| ty_has_cycle(t, aliases, visited, stack))
                || ty_has_cycle(ret, aliases, visited, stack)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_base::TyAttr;
    use baml_compiler_hir::QualifiedName;

    use super::*;

    /// Helper to create a type alias type
    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(QualifiedName::local(Name::new(name)), TyAttr::default())
    }

    #[test]
    fn test_simple_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            Name::new("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );

        // MyInt <: int should be true
        assert!(is_subtype_of(
            &type_alias("MyInt"),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &aliases
        ));

        // int <: MyInt should also be true (same structural type)
        assert!(is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &type_alias("MyInt"),
            &aliases
        ));
    }

    #[test]
    fn test_transitive_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            Name::new("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );
        aliases.insert(Name::new("AnotherInt"), type_alias("MyInt"));

        // AnotherInt <: int
        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &aliases
        ));

        // AnotherInt <: MyInt
        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &type_alias("MyInt"),
            &aliases
        ));
    }

    #[test]
    fn test_union_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            Name::new("IntOrString"),
            Ty::Union(
                vec![
                    Ty::Int {
                        attr: TyAttr::default(),
                    },
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ],
                TyAttr::default(),
            ),
        );

        // int <: IntOrString
        assert!(is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &aliases
        ));

        // string <: IntOrString
        assert!(is_subtype_of(
            &Ty::String {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &aliases
        ));

        // bool NOT <: IntOrString
        assert!(!is_subtype_of(
            &Ty::Bool {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &aliases
        ));
    }

    #[test]
    fn test_recursive_alias_detection() {
        let mut aliases = HashMap::new();
        // type List = int | List (simplified recursive type)
        aliases.insert(
            Name::new("List"),
            Ty::Union(
                vec![
                    Ty::Null {
                        attr: TyAttr::default(),
                    },
                    type_alias("List"),
                ],
                TyAttr::default(),
            ),
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(recursive.contains(&Name::new("List")));
    }

    #[test]
    fn test_non_recursive_not_marked() {
        let mut aliases = HashMap::new();
        aliases.insert(
            Name::new("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(!recursive.contains(&Name::new("MyInt")));
    }

    #[test]
    fn test_void_not_subtype_of_map() {
        let aliases = HashMap::new();
        let void_ty = Ty::Void {
            attr: TyAttr::default(),
        };
        let map_ty = Ty::Map {
            key: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            value: Box::new(Ty::Bool {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        // Void should NOT be a subtype of Map
        assert!(!is_subtype_of(&void_ty, &map_ty, &aliases));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // FUNCTION SUBTYPING TESTS
    // ═══════════════════════════════════════════════════════════════════════

    /// Helper to create a function type (without parameter names)
    fn func(params: Vec<Ty>, ret: Ty) -> Ty {
        Ty::Function {
            params: params.into_iter().map(|t| (None, t)).collect(),
            ret: Box::new(ret),
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn test_function_reflexivity() {
        let aliases = HashMap::new();

        // (int) -> string <: (int) -> string
        let f = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        assert!(is_subtype_of(&f, &f, &aliases));
    }

    #[test]
    fn test_function_covariant_return() {
        let aliases = HashMap::new();

        // (int) -> int <: (int) -> float  (because int <: float)
        let f1 = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::Int {
                attr: TyAttr::default(),
            },
        );
        let f2 = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::Float {
                attr: TyAttr::default(),
            },
        );
        assert!(is_subtype_of(&f1, &f2, &aliases));

        // (int) -> float NOT <: (int) -> int
        assert!(!is_subtype_of(&f2, &f1, &aliases));
    }

    #[test]
    fn test_function_contravariant_params() {
        let aliases = HashMap::new();

        // (float) -> string <: (int) -> string  (because int <: float, contravariance)
        // A function that accepts float can be used where one accepting int is expected
        let f1 = func(
            vec![Ty::Float {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        let f2 = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        assert!(is_subtype_of(&f1, &f2, &aliases));

        // (int) -> string NOT <: (float) -> string
        assert!(!is_subtype_of(&f2, &f1, &aliases));
    }

    #[test]
    fn test_function_unit_supertype_of_params() {
        let aliases = HashMap::new();

        // () -> string is a supertype of (int) -> string
        // A nullary function can be used where a unary function is expected
        // (it just ignores the argument)
        let nullary = func(
            vec![],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        let unary = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );

        // (int) -> string <: () -> string
        assert!(is_subtype_of(&unary, &nullary, &aliases));

        // () -> string NOT <: (int) -> string
        // A nullary function cannot substitute for one that requires an argument
        assert!(!is_subtype_of(&nullary, &unary, &aliases));
    }

    #[test]
    fn test_function_fewer_params_is_supertype() {
        let aliases = HashMap::new();

        // (int, string) -> bool <: (int) -> bool
        // A function with more params can be used where fewer are expected
        let binary = func(
            vec![
                Ty::Int {
                    attr: TyAttr::default(),
                },
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );
        let unary = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );

        assert!(is_subtype_of(&binary, &unary, &aliases));

        // (int) -> bool NOT <: (int, string) -> bool
        assert!(!is_subtype_of(&unary, &binary, &aliases));
    }

    #[test]
    fn test_function_combined_variance() {
        let aliases = HashMap::new();

        // (float) -> int <: (int) -> float
        // - Param: int <: float (contravariant, so float in sub, int in sup)
        // - Return: int <: float (covariant)
        let f1 = func(
            vec![Ty::Float {
                attr: TyAttr::default(),
            }],
            Ty::Int {
                attr: TyAttr::default(),
            },
        );
        let f2 = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::Float {
                attr: TyAttr::default(),
            },
        );
        assert!(is_subtype_of(&f1, &f2, &aliases));
    }

    #[test]
    fn test_function_multiple_params_contravariance() {
        let aliases = HashMap::new();

        // (float, string) -> bool <: (int, string) -> bool
        // First param: int <: float (contravariant)
        // Second param: string = string
        let f1 = func(
            vec![
                Ty::Float {
                    attr: TyAttr::default(),
                },
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );
        let f2 = func(
            vec![
                Ty::Int {
                    attr: TyAttr::default(),
                },
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );
        assert!(is_subtype_of(&f1, &f2, &aliases));

        // (int, string) -> bool NOT <: (float, string) -> bool
        assert!(!is_subtype_of(&f2, &f1, &aliases));
    }

    #[test]
    fn test_function_not_subtype_of_non_function() {
        let aliases = HashMap::new();

        let f = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );

        // Function is not a subtype of primitive types
        assert!(!is_subtype_of(
            &f,
            &Ty::Int {
                attr: TyAttr::default()
            },
            &aliases
        ));
        assert!(!is_subtype_of(
            &f,
            &Ty::String {
                attr: TyAttr::default()
            },
            &aliases
        ));

        // Primitive types are not subtypes of functions
        assert!(!is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &f,
            &aliases
        ));
    }

    #[test]
    fn test_function_in_union() {
        let aliases = HashMap::new();

        let f = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        let union = Ty::Union(
            vec![
                f.clone(),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );

        // (int) -> string <: ((int) -> string) | null
        assert!(is_subtype_of(&f, &union, &aliases));

        // null <: ((int) -> string) | null
        assert!(is_subtype_of(
            &Ty::Null {
                attr: TyAttr::default()
            },
            &union,
            &aliases
        ));
    }

    #[test]
    fn test_higher_order_function() {
        let aliases = HashMap::new();

        // A function that takes a function as parameter
        // ((int) -> string) -> bool
        let inner_fn_int = func(
            vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        let higher_order_int = func(
            vec![inner_fn_int],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );

        // Reflexivity
        assert!(is_subtype_of(
            &higher_order_int,
            &higher_order_int,
            &aliases
        ));

        // Higher-order contravariance works like this:
        // ((float) -> string) -> bool  vs  ((int) -> string) -> bool
        //
        // The outer function's param is contravariant, so we need:
        //   (int) -> string <: (float) -> string
        //
        // For (int) -> string <: (float) -> string:
        //   - params are contravariant: need float <: int (FALSE! float is supertype of int)
        //
        // So ((float) -> string) -> bool is NOT <: ((int) -> string) -> bool
        let inner_fn_float = func(
            vec![Ty::Float {
                attr: TyAttr::default(),
            }],
            Ty::String {
                attr: TyAttr::default(),
            },
        );
        let higher_order_float = func(
            vec![inner_fn_float],
            Ty::Bool {
                attr: TyAttr::default(),
            },
        );
        assert!(!is_subtype_of(
            &higher_order_float,
            &higher_order_int,
            &aliases
        ));

        // The correct direction:
        // ((int) -> string) -> bool <: ((float) -> string) -> bool
        //
        // Outer param is contravariant, so need:
        //   (float) -> string <: (int) -> string
        //
        // For (float) -> string <: (int) -> string:
        //   - params contravariant: need int <: float (TRUE!)
        //   - return covariant: string = string (TRUE!)
        //
        // So ((int) -> string) -> bool <: ((float) -> string) -> bool
        assert!(is_subtype_of(
            &higher_order_int,
            &higher_order_float,
            &aliases
        ));
    }
}
