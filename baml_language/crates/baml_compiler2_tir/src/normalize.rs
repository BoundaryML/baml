//! Type normalization and subtyping.
//!
//! Converts surface `Ty` types to an internal `StructuralTy` where all type
//! aliases are resolved. Recursive aliases are represented using Mu types with
//! equirecursive (co-inductive) subtyping.

use std::collections::{HashMap, HashSet};

use baml_base::Name;
pub use baml_type::ResolvedAliases;

use crate::ty::{FunctionParamMode, LiteralValue, MediaKind, QualifiedTypeName, Ty};

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Build a [`ResolvedAliases`] environment from a bare alias map, computing
/// its recursive-alias set once. This is the only sanctioned constructor on
/// the compiler side: every comparison entry point below takes the full
/// environment, so the O(aliases + edges) cycle analysis is paid once per
/// environment instead of once per type comparison.
pub fn resolved_aliases_from_map(aliases: HashMap<QualifiedTypeName, Ty>) -> ResolvedAliases {
    let recursive = find_recursive_aliases(&aliases);
    ResolvedAliases { aliases, recursive }
}

/// Check if `sub` is a subtype of `sup`, resolving type aliases.
pub(crate) fn is_subtype_of(sub: &Ty, sup: &Ty, env: &ResolvedAliases) -> bool {
    // Fast path: `normalize` is a pure function of `(ty, env)`, so
    // structurally identical types always normalize identically, and
    // `StructuralTy::is_subtype_of` is reflexive. Skips the double
    // normalization for the common `T <: T` query.
    if sub == sup {
        return true;
    }
    let sub_norm = normalize(sub, &env.aliases, &env.recursive);
    let sup_norm = normalize(sup, &env.aliases, &env.recursive);
    sub_norm.is_subtype_of(&sup_norm, &mut HashSet::new())
}

/// Check invariant type equality after resolving aliases and canonicalizing
/// order-insensitive type forms such as unions.
///
/// This is not an assignability/subtyping check. Use it for invariant positions
/// where two type spellings may differ but still denote the same type, such as
/// interface field implementations.
pub fn is_same_normalized_type(lhs: &Ty, rhs: &Ty, env: &ResolvedAliases) -> bool {
    // Fast path: `normalize` is a pure function of `(ty, env)`, so
    // structurally identical types always normalize (and canonicalize)
    // identically. Skips the double normalization for the very common
    // identical-spelling case.
    if lhs == rhs {
        return true;
    }
    normalize(lhs, &env.aliases, &env.recursive).canonicalize()
        == normalize(rhs, &env.aliases, &env.recursive).canonicalize()
}

/// Find all recursive type aliases via DFS.
///
/// A single memoized DFS over the whole alias graph: `state` maps each alias
/// to `None` while it is on the DFS stack and to `Some(reaches_cycle)` once
/// fully explored, so every alias body is walked at most once — O(nodes +
/// edges) total rather than a fresh DFS per alias. ("Reaches a cycle" is a
/// property of the alias alone, independent of which root the DFS entered
/// from, so memoizing it across roots is sound.)
pub fn find_recursive_aliases(
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashSet<QualifiedTypeName> {
    let mut state = HashMap::new();
    let mut recursive = HashSet::new();
    for name in aliases.keys() {
        if has_cycle(name, aliases, &mut state) {
            recursive.insert(name.clone());
        }
    }
    recursive
}

// ═══════════════════════════════════════════════════════════════════════════
// STRUCTURAL TYPE (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Normalized structural type. All aliases resolved, recursion explicit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum StructuralTy {
    // Primitives
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Image,
    Audio,
    Video,
    Pdf,
    Uint8Array,
    // Literal
    Literal(baml_base::Literal),
    // User-defined (resolved by qualified name)
    /// Generic class arguments are compared invariantly — `Foo<int>` and `Foo<string>`
    /// are not subtypes of each other.
    Class(QualifiedTypeName, Vec<StructuralTy>),
    /// Interface type (BEP-044). Distinct from `Class` so that nominal
    /// `class T implements I` subtyping doesn't accidentally fire on the
    /// structural class-name match.
    Interface(
        QualifiedTypeName,
        Vec<StructuralTy>,
        Vec<(Name, StructuralTy)>,
    ),
    Enum(QualifiedTypeName),
    EnumVariant(QualifiedTypeName, Name),
    // Constructors
    List(Box<StructuralTy>),
    Map {
        key: Box<StructuralTy>,
        value: Box<StructuralTy>,
    },
    Union(Vec<StructuralTy>),
    Function {
        params: Vec<StructuralFunctionParam>,
        ret: Box<StructuralTy>,
        throws: Box<StructuralTy>,
    },
    // Recursion
    Mu {
        var: QualifiedTypeName,
        body: Box<StructuralTy>,
    },
    TyVar(QualifiedTypeName),
    /// A generic type parameter — opaque, only subtypes itself and `BuiltinUnknown`.
    TypeVar(Name),
    /// Symbolic associated type projection — opaque unless already resolved.
    AssociatedTypeProjection {
        base: Box<StructuralTy>,
        interface: Option<Box<StructuralTy>>,
        member: Name,
    },
    // Special
    Never,
    Void,
    /// The `type` metatype keyword — opaque, only compatible with itself.
    Type,
    /// The explicit `unknown` keyword — top type (supertype of everything).
    BuiltinUnknown,
    Unknown,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct StructuralFunctionParam {
    name: Option<Name>,
    ty: StructuralTy,
    mode: FunctionParamMode,
}

impl StructuralTy {
    fn canonicalize(self) -> Self {
        match self {
            StructuralTy::Class(qn, args) => {
                StructuralTy::Class(qn, args.into_iter().map(Self::canonicalize).collect())
            }
            StructuralTy::Interface(qn, args, associated_bindings) => StructuralTy::Interface(
                qn,
                args.into_iter().map(Self::canonicalize).collect(),
                associated_bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize()))
                    .collect(),
            ),
            StructuralTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => StructuralTy::AssociatedTypeProjection {
                base: Box::new(base.canonicalize()),
                interface: interface.map(|interface| Box::new(interface.canonicalize())),
                member,
            },
            StructuralTy::List(inner) => StructuralTy::List(Box::new(inner.canonicalize())),
            StructuralTy::Map { key, value } => StructuralTy::Map {
                key: Box::new(key.canonicalize()),
                value: Box::new(value.canonicalize()),
            },
            StructuralTy::Union(types) => {
                let mut canonical = Vec::new();
                for ty in types {
                    match ty.canonicalize() {
                        StructuralTy::Union(inner) => canonical.extend(inner),
                        other => canonical.push(other),
                    }
                }
                canonical.sort();
                canonical.dedup();
                if canonical.len() == 1 {
                    canonical
                        .pop()
                        .expect("single-element canonical union should have an element")
                } else {
                    StructuralTy::Union(canonical)
                }
            }
            StructuralTy::Function {
                params,
                ret,
                throws,
            } => {
                let mut required = Vec::new();
                let mut optional = Vec::new();
                for param in params
                    .into_iter()
                    .map(StructuralFunctionParam::canonicalize)
                {
                    match param.mode {
                        FunctionParamMode::Required => required.push(param),
                        FunctionParamMode::Optional => optional.push(param),
                    }
                }
                optional.sort();
                required.extend(optional);
                StructuralTy::Function {
                    params: required,
                    ret: Box::new(ret.canonicalize()),
                    throws: Box::new(throws.canonicalize()),
                }
            }
            StructuralTy::Mu { var, body } => StructuralTy::Mu {
                var,
                body: Box::new(body.canonicalize()),
            },
            other => other,
        }
    }

    /// Equirecursive subtyping with co-inductive assumptions.
    ///
    /// Purely structural: there are no representation-changing coercions in the
    /// subtype relation (`int` is not a subtype of `bigint` or `float`). The
    /// only numeric widenings are representation-preserving literal-to-base
    /// (`literal 1 <: int`, `1n <: bigint`).
    fn is_subtype_of(
        &self,
        other: &StructuralTy,
        assumptions: &mut HashSet<(StructuralTy, StructuralTy)>,
    ) -> bool {
        // Co-inductive: if we've assumed this pair, it holds.
        let pair = (self.clone(), other.clone());
        if assumptions.contains(&pair) {
            return true;
        }

        // Reflexivity
        if self == other {
            return true;
        }

        // Never is the bottom type — subtype of everything
        if matches!(self, StructuralTy::Never) {
            return true;
        }

        // BuiltinUnknown is the top type — everything is a subtype of it.
        // But BuiltinUnknown itself is NOT a subtype of specific types
        // (unlike the error-recovery Unknown which is bidirectionally compatible).
        if matches!(other, StructuralTy::BuiltinUnknown) {
            return true;
        }
        // If self is BuiltinUnknown and other is not BuiltinUnknown (reflexivity
        // already handled equal case above), it's not a subtype.
        if matches!(self, StructuralTy::BuiltinUnknown) {
            return false;
        }

        // Void is only compatible with itself (handled by reflexivity above)
        if matches!(self, StructuralTy::Void) || matches!(other, StructuralTy::Void) {
            return false;
        }

        // Type is only compatible with itself (handled by reflexivity above)
        if matches!(self, StructuralTy::Type) || matches!(other, StructuralTy::Type) {
            return false;
        }

        // Error recovery: Unknown/Error are compatible with anything
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

            // Union<T1, T2> <: U iff all Ti <: U.
            // (Order matters: this must come before the `T <: Union` rule
            // below — otherwise `Union(int, string) <: Union(int, string,
            // float)` matches `T <: Union` with T=Union, and asks whether
            // the LHS is one of the RHS members, which it isn't.)
            (StructuralTy::Union(types), other) => {
                types.iter().all(|t| t.is_subtype_of(other, assumptions))
            }

            // T <: T | U
            (inner, StructuralTy::Union(types)) => {
                types.iter().any(|t| inner.is_subtype_of(t, assumptions))
            }

            // List invariance: element-wise structural subtyping. `int[]` is
            // NOT a subtype of `bigint[]` (the element rule `int <: bigint` no
            // longer exists). `never <: T` stays free, so empty lists
            // (`list<never>`) remain assignable anywhere.
            (StructuralTy::List(inner1), StructuralTy::List(inner2)) => {
                inner1.is_subtype_of(inner2, assumptions)
            }

            // Map: invariant in both key and value. The relation is purely
            // structural, so `map<string, int>` is not a subtype of
            // `map<string, bigint>`. `never`/literal-key widening stays free:
            //   - map<never, T>   <: map<K, V>    (empty map is assignable anywhere)
            //   - map<"lit", T>   <: map<string, V>  (literal keys widen to string)
            //   - map<string, T>  <: map<string, V>  (same key type)
            //
            // This allows passing `{}` (`map<never, never>`) and `{"foo": "bar"}`
            // (`map<literal "foo", literal "bar">`) to functions like
            // baml.llm.build_request(), which takes `map<string, unknown>`.
            (
                StructuralTy::Map { key: k1, value: v1 },
                StructuralTy::Map { key: k2, value: v2 },
            ) => k1.is_subtype_of(k2, assumptions) && v1.is_subtype_of(v2, assumptions),

            // Numeric types do not widen across representations: `int` is
            // neither a subtype of `bigint` (i64 vs heap BigInt) nor of `float`
            // (i64 values past 2^53 are not exactly representable as f64). Write
            // the target literal explicitly (`1n`, `1.0`), or rely on the
            // `bigint × int` operators / the FFI boundary to convert at runtime.

            // Literal types are subtypes of their (same-representation) base
            // type only: `literal 1 <: int`, `1n <: bigint`. An int literal is
            // NOT a subtype of `bigint` — write `1n`.
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Int) => true,
            (StructuralTy::Literal(LiteralValue::Bigint(_)), StructuralTy::Bigint) => true,
            (StructuralTy::Literal(LiteralValue::Float(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::String(_)), StructuralTy::String) => true,
            (StructuralTy::Literal(LiteralValue::Bool(_)), StructuralTy::Bool) => true,

            // EnumVariant(E, V) <: Enum(E)
            (StructuralTy::EnumVariant(e, _), StructuralTy::Enum(sup_e)) => e == sup_e,

            // Function subtyping: contravariant params, covariant return and
            // throws. As everywhere, the relation is structural — there is no
            // numeric coercion, so `fn() -> int` is not usable as
            // `fn() -> bigint`, nor `fn(bigint)` as `fn(int)`.
            (
                StructuralTy::Function {
                    params: params1,
                    ret: ret1,
                    throws: throws1,
                },
                StructuralTy::Function {
                    params: params2,
                    ret: ret2,
                    throws: throws2,
                },
            ) => {
                if !ret1.is_subtype_of(ret2, assumptions) {
                    return false;
                }
                if !throws1.is_subtype_of(throws2, assumptions) {
                    return false;
                }
                function_params_subtype(params1, params2, assumptions)
            }

            // TypeVar (generic parameter) is opaque — only subtypes itself
            // (reflexivity above) and BuiltinUnknown (early check above).
            // Placed here rather than as an early guard so that Union/Optional
            // decomposition rules above can match first (e.g. T <: T | U).
            (StructuralTy::TypeVar(v1), StructuralTy::TypeVar(v2)) => v1 == v2,
            (
                StructuralTy::AssociatedTypeProjection {
                    base: lb,
                    interface: li,
                    member: lm,
                },
                StructuralTy::AssociatedTypeProjection {
                    base: rb,
                    interface: ri,
                    member: rm,
                },
            ) => lm == rm && lb == rb && li == ri,

            _ => false,
        };

        assumptions.remove(&pair);
        result
    }
}

impl StructuralFunctionParam {
    fn canonicalize(self) -> Self {
        Self {
            name: match self.mode {
                FunctionParamMode::Required => None,
                FunctionParamMode::Optional => self.name,
            },
            ty: self.ty.canonicalize(),
            mode: self.mode,
        }
    }
}

/// Subtype check for function parameter lists (contravariant).
fn function_params_subtype(
    sub_params: &[StructuralFunctionParam],
    sup_params: &[StructuralFunctionParam],
    assumptions: &mut HashSet<(StructuralTy, StructuralTy)>,
) -> bool {
    let sub_required: Vec<_> = sub_params
        .iter()
        .filter(|param| matches!(param.mode, FunctionParamMode::Required))
        .collect();
    let sup_required: Vec<_> = sup_params
        .iter()
        .filter(|param| matches!(param.mode, FunctionParamMode::Required))
        .collect();

    if sub_required.len() != sup_required.len() {
        return false;
    }

    for (sub, sup) in sub_required.iter().zip(sup_required.iter()) {
        if !sup.ty.is_subtype_of(&sub.ty, assumptions) {
            return false;
        }
    }

    for sup in sup_params
        .iter()
        .filter(|param| matches!(param.mode, FunctionParamMode::Optional))
    {
        let Some(name) = &sup.name else {
            return false;
        };
        let Some(sub) = sub_params.iter().find(|param| {
            matches!(param.mode, FunctionParamMode::Optional) && param.name.as_ref() == Some(name)
        }) else {
            return false;
        };
        if !sup.ty.is_subtype_of(&sub.ty, assumptions) {
            return false;
        }
    }

    true
}

/// Substitute `TyVar` with replacement in type.
fn substitute(
    ty: &StructuralTy,
    var: &QualifiedTypeName,
    replacement: &StructuralTy,
) -> StructuralTy {
    match ty {
        StructuralTy::TyVar(v) if v == var => replacement.clone(),
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
        StructuralTy::Function {
            params,
            ret,
            throws,
        } => StructuralTy::Function {
            params: params
                .iter()
                .map(|param| StructuralFunctionParam {
                    name: param.name.clone(),
                    ty: substitute(&param.ty, var, replacement),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(substitute(ret, var, replacement)),
            throws: Box::new(substitute(throws, var, replacement)),
        },
        StructuralTy::Class(qn, args) => StructuralTy::Class(
            qn.clone(),
            args.iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
        ),
        StructuralTy::Interface(qn, args, associated_bindings) => StructuralTy::Interface(
            qn.clone(),
            args.iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
            associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), substitute(ty, var, replacement)))
                .collect(),
        ),
        StructuralTy::AssociatedTypeProjection {
            base,
            interface,
            member,
        } => StructuralTy::AssociatedTypeProjection {
            base: Box::new(substitute(base, var, replacement)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute(interface, var, replacement))),
            member: member.clone(),
        },
        StructuralTy::Mu { var: v, body } if v != var => StructuralTy::Mu {
            var: v.clone(),
            body: Box::new(substitute(body, var, replacement)),
        },
        _ => ty.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION (private)
// ═══════════════════════════════════════════════════════════════════════════

fn normalize(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    recursive: &HashSet<QualifiedTypeName>,
) -> StructuralTy {
    let mut expanding = HashSet::new();
    normalize_impl(ty, aliases, recursive, &mut expanding)
}

fn normalize_impl(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    recursive: &HashSet<QualifiedTypeName>,
    expanding: &mut HashSet<QualifiedTypeName>,
) -> StructuralTy {
    match ty {
        Ty::Int { .. } => StructuralTy::Int,
        Ty::Bigint { .. } => StructuralTy::Bigint,
        Ty::Float { .. } => StructuralTy::Float,
        Ty::String { .. } => StructuralTy::String,
        Ty::Bool { .. } => StructuralTy::Bool,
        Ty::Null { .. } => StructuralTy::Null,
        Ty::Media(MediaKind::Image, _) => StructuralTy::Image,
        Ty::Media(MediaKind::Audio, _) => StructuralTy::Audio,
        Ty::Media(MediaKind::Video, _) => StructuralTy::Video,
        Ty::Media(MediaKind::Pdf, _) => StructuralTy::Pdf,
        // `MediaKind::Generic` is never produced by TIR (`lower_type_expr`
        // maps generic media to `Unknown`); handled defensively for
        // exhaustiveness over the shared `MediaKind`.
        Ty::Media(MediaKind::Generic, _) => StructuralTy::Unknown,
        Ty::Uint8Array { .. } => StructuralTy::Uint8Array,
        Ty::Never { .. } => StructuralTy::Never,
        Ty::Void { .. } => StructuralTy::Void,
        Ty::BuiltinUnknown { .. } => StructuralTy::BuiltinUnknown,
        Ty::Unknown { .. } => StructuralTy::Unknown,
        // INVARIANT: a `_` inference hole is filled (or replaced with `Ty::Error`)
        // during inference, before any structural equivalence/subtype check. A
        // hole has no sound normal form — treating it as "matches anything" would
        // make `Box<int>` and `Box<string>` both equal to `Box<_>` and break
        // transitivity — so reaching here is a compiler bug. (See `builder`'s
        // `let`-binding hole path, which infers, fills, then checks.)
        Ty::Infer { .. } => unreachable!(
            "inference hole `_` reached structural normalization; it must be \
             filled (or replaced with `Ty::Error`) during inference before any \
             equivalence/subtype check"
        ),
        Ty::Error { .. } => StructuralTy::Error,
        Ty::Literal(lit, _freshness, _) => StructuralTy::Literal(lit.clone()),
        Ty::Class(qn, type_args, _) => {
            let args = type_args
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect();
            StructuralTy::Class(qn.clone(), args)
        }
        Ty::Interface(qn, type_args, associated_bindings, _) => {
            let args = type_args
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect();
            let mut associated_bindings: Vec<_> = associated_bindings
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        normalize_impl(ty, aliases, recursive, expanding),
                    )
                })
                .collect();
            associated_bindings.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
            StructuralTy::Interface(qn.clone(), args, associated_bindings)
        }
        Ty::Enum(qn, _) => StructuralTy::Enum(qn.clone()),
        Ty::EnumVariant(qn, v, _) => StructuralTy::EnumVariant(qn.clone(), v.clone()),

        Ty::TypeAlias(qn, _) => {
            if expanding.contains(qn) {
                return StructuralTy::TyVar(qn.clone());
            }

            if let Some(alias_ty) = aliases.get(qn) {
                if recursive.contains(qn) {
                    expanding.insert(qn.clone());
                    let body = normalize_impl(alias_ty, aliases, recursive, expanding);
                    expanding.remove(qn);
                    StructuralTy::Mu {
                        var: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    normalize_impl(alias_ty, aliases, recursive, expanding)
                }
            } else {
                StructuralTy::Error
            }
        }

        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => StructuralTy::List(Box::new(
            normalize_impl(inner, aliases, recursive, expanding),
        )),
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => StructuralTy::Map {
            key: Box::new(normalize_impl(key, aliases, recursive, expanding)),
            value: Box::new(normalize_impl(value, aliases, recursive, expanding)),
        },
        Ty::Union(types, _) => StructuralTy::Union(
            types
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => StructuralTy::Function {
            params: params
                .iter()
                .map(|param| StructuralFunctionParam {
                    name: param.name.clone(),
                    ty: normalize_impl(&param.ty, aliases, recursive, expanding),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(normalize_impl(ret, aliases, recursive, expanding)),
            throws: Box::new(normalize_impl(throws, aliases, recursive, expanding)),
        },
        Ty::TypeVar(name, _) => StructuralTy::TypeVar(name.clone()),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => StructuralTy::AssociatedTypeProjection {
            base: Box::new(normalize_impl(base, aliases, recursive, expanding)),
            interface: interface.as_ref().map(|interface| {
                Box::new(normalize_impl(
                    &interface.to_ty(),
                    aliases,
                    recursive,
                    expanding,
                ))
            }),
            member: member.clone(),
        },
        // `$rust_type` — opaque Rust-managed state. Treated as Unknown
        // in the structural type system (cannot be constructed or destructured
        // by user code).
        Ty::RustType { .. } => StructuralTy::Unknown,
        // `type` — the BAML metatype keyword. Strict: only compatible with `type`.
        Ty::Type { .. } => StructuralTy::Type,
        // Phase C lowers `Future<T, E>` structurally as `Unknown`; the
        // pattern-matching machinery treats futures as opaque (they have
        // no destructure syntax). The dedicated Ty::Future variant still
        // carries the value/error types for the MIR lowering of `await`.
        Ty::Future(..) => StructuralTy::Unknown,
        // `Resource`/`PromptAst`/`WatchAccessor` are never produced by TIR; the
        // arms exist only so the match stays exhaustive over the shared
        // `baml_type::Ty`. Treat them as opaque (`Unknown`) like other
        // non-destructurable types.
        Ty::Resource { .. } | Ty::PromptAst { .. } | Ty::WatchAccessor(..) => StructuralTy::Unknown,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CYCLE DETECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Does expanding `name` ever reach a cycle in the alias graph?
///
/// `state` is the shared memo for [`find_recursive_aliases`]: `None` marks an
/// alias currently on the DFS stack (so hitting it again is a back-edge, i.e.
/// a real cycle), `Some(result)` a fully explored alias.
fn has_cycle(
    name: &QualifiedTypeName,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    state: &mut HashMap<QualifiedTypeName, Option<bool>>,
) -> bool {
    match state.get(name) {
        // On the current DFS stack — a back-edge, i.e. a real cycle.
        Some(None) => return true,
        // Fully explored on an earlier visit.
        Some(&Some(result)) => return result,
        None => {}
    }
    state.insert(name.clone(), None);
    let result = aliases
        .get(name)
        .is_some_and(|ty| ty_has_cycle(ty, aliases, state));
    state.insert(name.clone(), Some(result));
    result
}

fn ty_has_cycle(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    state: &mut HashMap<QualifiedTypeName, Option<bool>>,
) -> bool {
    match ty {
        Ty::TypeAlias(qn, _) if aliases.contains_key(qn) => has_cycle(qn, aliases, state),
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => ty_has_cycle(inner, aliases, state),
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
            ty_has_cycle(key, aliases, state) || ty_has_cycle(value, aliases, state)
        }
        Ty::Union(types, _) => types.iter().any(|t| ty_has_cycle(t, aliases, state)),
        Ty::Class(_, type_args, _) => type_args.iter().any(|t| ty_has_cycle(t, aliases, state)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(|t| ty_has_cycle(t, aliases, state))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| ty_has_cycle(ty, aliases, state))
        }
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            ty_has_cycle(base, aliases, state)
                || interface.as_ref().is_some_and(|interface| {
                    interface.tys().any(|t| ty_has_cycle(t, aliases, state))
                })
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| ty_has_cycle(&param.ty, aliases, state))
                || ty_has_cycle(ret, aliases, state)
                || ty_has_cycle(throws, aliases, state)
        }
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// INVALID CYCLE DETECTION (Tarjan's SCC + structural edges)
// ═══════════════════════════════════════════════════════════════════════════
//
// Mirrors the approach in `baml_compiler_tir/src/cycles.rs`:
// 1. Build a dependency graph tracking structural vs non-structural edges.
//    "Structural" means the reference goes through List or Map, which provide
//    a termination point (empty container). Optional and Union are pass-through.
// 2. Find SCCs via Tarjan's algorithm (deterministic ordering).
// 3. A cycle is valid if it has at least one structural edge within the SCC.
//    Otherwise every member gets an AliasCycle diagnostic.

/// Find all type aliases that participate in **invalid** (unguarded) cycles.
///
/// An edge is "structural" if it passes through a `List` or `Map` constructor,
/// which provides a base case for termination (empty list/map). `Optional` and
/// `Union` are pass-through — they do NOT create structural context.
///
/// A cycle is valid if at least one edge within the SCC is structural.
/// Otherwise all members are flagged as invalid.
///
/// Returns a set of qualified type names that should receive cycle diagnostics.
pub fn find_invalid_alias_cycles(
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashSet<QualifiedTypeName> {
    // 1. Build the graph + structural edge set
    let GraphResult {
        graph,
        structural_edges,
    } = build_alias_graph(aliases);

    // 2. Find SCCs via Tarjan's (deterministic, only real cycles)
    let sccs = Tarjan::components(&graph);

    // 3. For each SCC, check if it has at least one structural edge
    let mut invalid = HashSet::new();
    for scc in &sccs {
        let scc_set: HashSet<&QualifiedTypeName> = scc.iter().collect();
        let has_structural = structural_edges
            .iter()
            .any(|(from, to)| scc_set.contains(from) && scc_set.contains(to));

        if !has_structural {
            for name in scc {
                invalid.insert(name.clone());
            }
        }
    }

    invalid
}

/// Result of building a type alias dependency graph.
struct GraphResult {
    /// The full dependency graph (all edges, structural + non-structural).
    graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    /// Edges that go through structural types (List/Map).
    structural_edges: HashSet<(QualifiedTypeName, QualifiedTypeName)>,
}

/// Build a graph of type alias dependencies, tracking which edges are structural.
fn build_alias_graph(aliases: &HashMap<QualifiedTypeName, Ty>) -> GraphResult {
    let mut graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> = HashMap::new();
    let mut structural_edges: HashSet<(QualifiedTypeName, QualifiedTypeName)> = HashSet::new();

    for (alias_name, ty) in aliases {
        let (mut non_structural, structural) = extract_type_alias_deps(ty, aliases);

        for dep in &structural {
            structural_edges.insert((alias_name.clone(), dep.clone()));
        }

        // Graph has ALL edges (structural + non-structural combined)
        non_structural.extend(structural);
        graph.insert(alias_name.clone(), non_structural);
    }

    GraphResult {
        graph,
        structural_edges,
    }
}

/// Extract type alias dependencies from a resolved type.
///
/// Returns `(non_structural_deps, structural_deps)` where structural means
/// the reference goes through `List` or `Map` (which provide a termination
/// point via empty container). `Union` (including a nullable `T | null`) is
/// pass-through — it does NOT create structural context.
fn extract_type_alias_deps(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> (HashSet<QualifiedTypeName>, HashSet<QualifiedTypeName>) {
    fn visit(
        ty: &Ty,
        aliases: &HashMap<QualifiedTypeName, Ty>,
        non_structural: &mut HashSet<QualifiedTypeName>,
        structural: &mut HashSet<QualifiedTypeName>,
        in_structural: bool,
    ) {
        match ty {
            Ty::TypeAlias(qn, _) if aliases.contains_key(qn) => {
                if in_structural {
                    structural.insert(qn.clone());
                } else {
                    non_structural.insert(qn.clone());
                }
            }
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
                // List provides structural guard (can be empty)
                visit(inner, aliases, non_structural, structural, true);
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
                // Map provides structural guard (can be empty)
                visit(key, aliases, non_structural, structural, true);
                visit(value, aliases, non_structural, structural, true);
            }
            Ty::Union(members, _) => {
                // Union passes through the structural context
                for m in members {
                    visit(m, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Class(_, type_args, _) => {
                // Nominal type_args are pass-through for cycle classification.
                // User-defined nominal types are not structural guards like List/Map,
                // so their generic arguments inherit the surrounding context.
                for t in type_args {
                    visit(t, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Interface(_, type_args, associated_bindings, _) => {
                for t in type_args {
                    visit(t, aliases, non_structural, structural, in_structural);
                }
                for (_, ty) in associated_bindings {
                    visit(ty, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                visit(base, aliases, non_structural, structural, in_structural);
                if let Some(interface) = interface {
                    for ty in interface.tys() {
                        visit(ty, aliases, non_structural, structural, in_structural);
                    }
                }
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    visit(
                        &param.ty,
                        aliases,
                        non_structural,
                        structural,
                        in_structural,
                    );
                }
                visit(ret, aliases, non_structural, structural, in_structural);
                visit(throws, aliases, non_structural, structural, in_structural);
            }
            _ => {}
        }
    }

    let mut non_structural = HashSet::new();
    let mut structural = HashSet::new();
    visit(ty, aliases, &mut non_structural, &mut structural, false);
    (non_structural, structural)
}

// ── Tarjan's SCC ─────────────────────────────────────────────────────────────
//
// Adapted from `baml_compiler_tir/src/cycles.rs` — deterministic ordering
// via sorted traversal, component reversal, and rotation to minimum element.

/// State of each node for Tarjan's algorithm.
#[derive(Clone, Copy)]
struct NodeState {
    index: usize,
    low_link: usize,
    on_stack: bool,
}

/// Tarjan's strongly connected components algorithm.
///
/// Only returns real cycles (multi-node SCCs or single nodes with self-loops).
/// Components are sorted deterministically.
struct Tarjan<'g> {
    graph: &'g HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    index: usize,
    stack: Vec<QualifiedTypeName>,
    state: HashMap<QualifiedTypeName, NodeState>,
    components: Vec<Vec<QualifiedTypeName>>,
}

impl<'g> Tarjan<'g> {
    const UNVISITED: usize = usize::MAX;

    fn components(
        graph: &'g HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    ) -> Vec<Vec<QualifiedTypeName>> {
        let mut tarjan = Self {
            graph,
            index: 0,
            stack: Vec::new(),
            state: graph
                .keys()
                .map(|node| {
                    (
                        node.clone(),
                        NodeState {
                            index: Self::UNVISITED,
                            low_link: Self::UNVISITED,
                            on_stack: false,
                        },
                    )
                })
                .collect(),
            components: Vec::new(),
        };

        // Sort nodes for deterministic traversal order.
        let mut nodes: Vec<_> = graph.keys().cloned().collect();
        nodes.sort_by_key(std::string::ToString::to_string);

        for node in &nodes {
            if tarjan.state[node].index == Self::UNVISITED {
                tarjan.strong_connect(node);
            }
        }

        // Sort components by first element for deterministic output.
        tarjan
            .components
            .sort_by(|a, b| a[0].to_string().cmp(&b[0].to_string()));

        tarjan.components
    }

    fn strong_connect(&mut self, node_id: &QualifiedTypeName) {
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };
        self.index += 1;
        self.state.insert(node_id.clone(), node);
        self.stack.push(node_id.clone());

        // Sort successors for deterministic DFS order.
        let mut successors: Vec<_> = self.graph[node_id].iter().collect();
        successors.sort_by_key(std::string::ToString::to_string);

        for successor_id in successors {
            let mut successor = self.state[successor_id];
            if successor.index == Self::UNVISITED {
                self.strong_connect(successor_id);
                successor = self.state[successor_id];
                node.low_link = std::cmp::min(node.low_link, successor.low_link);
            } else if successor.on_stack {
                node.low_link = std::cmp::min(node.low_link, successor.index);
            }
        }

        self.state.insert(node_id.clone(), node);

        if node.low_link == node.index {
            let mut component = Vec::new();
            while let Some(top) = self.stack.pop() {
                if let Some(st) = self.state.get_mut(&top) {
                    st.on_stack = false;
                }
                let is_root = &top == node_id;
                component.push(top);
                if is_root {
                    break;
                }
            }

            // Reverse: stack pop order → DFS visitation order
            component.reverse();

            // Only keep real cycles: multi-node or single node with self-loop.
            let is_cycle = component.len() > 1
                || (component.len() == 1 && self.graph[node_id].contains(node_id));

            if is_cycle {
                // Rotate to start at the lexicographically smallest element
                // for deterministic cycle paths.
                if let Some(min_idx) = component
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.to_string().cmp(&b.to_string()))
                    .map(|(i, _)| i)
                {
                    component.rotate_left(min_idx);
                }
                self.components.push(component);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLASS REQUIRED-FIELD CYCLE DETECTION
// ═══════════════════════════════════════════════════════════════════════════
//
// Classes with required (non-optional, non-list, non-map) fields that form
// a cycle are impossible to construct at runtime. We detect these using the
// same Tarjan's SCC infrastructure.
//
// Unlike type alias cycles, there is no "structural guard" exemption —
// every SCC found is unconditionally an error.

/// A class cycle: the names participating and a formatted path string.
pub struct ClassCycleInfo {
    /// All class names in this cycle.
    pub members: Vec<QualifiedTypeName>,
    /// Human-readable cycle path, e.g. "A -> B -> A".
    pub cycle_path: String,
}

/// Find all classes that participate in unconstructable required-field cycles.
///
/// A "required" field is one that is NOT optional, NOT a list, and NOT a map.
/// Optional/list/map fields can be null/empty, breaking the construction chain.
///
/// Returns a list of `ClassCycleInfo`, one per SCC found.
pub fn find_invalid_class_cycles(
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
) -> Vec<ClassCycleInfo> {
    let graph = build_class_graph(class_fields, type_aliases);
    let sccs = Tarjan::components(&graph);

    sccs.into_iter()
        .map(|scc| {
            let cycle_path = format_cycle_path(&scc);
            ClassCycleInfo {
                members: scc,
                cycle_path,
            }
        })
        .collect()
}

/// Build a dependency graph of classes based on required field types.
fn build_class_graph(
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> {
    let mut graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> = HashMap::new();

    // All classes must be in the graph (even if they have no required deps)
    for class_name in class_fields.keys() {
        graph.entry(class_name.clone()).or_default();
    }

    for (class_name, fields) in class_fields {
        let mut deps = HashSet::new();
        for (_field_name, field_ty) in fields {
            extract_required_class_deps(
                field_ty,
                class_fields,
                type_aliases,
                &mut deps,
                false, // not optional
                false, // not in list/map
                &mut HashSet::new(),
            );
        }
        graph.insert(class_name.clone(), deps);
    }

    graph
}

/// Extract required class dependencies from a field type.
///
/// A class reference is "required" only if it is NOT behind Optional, List,
/// or Map. Type aliases are resolved transparently.
fn extract_required_class_deps(
    ty: &Ty,
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
    deps: &mut HashSet<QualifiedTypeName>,
    optional: bool,
    in_list_or_map: bool,
    visiting: &mut HashSet<QualifiedTypeName>,
) {
    match ty {
        // Only add if the field is truly required.
        Ty::Class(qn, _, _) if !optional && !in_list_or_map && class_fields.contains_key(qn) => {
            deps.insert(qn.clone());
        }
        Ty::Class(_, _, _) => {}
        // Resolve through type aliases (only if still required context).
        Ty::TypeAlias(qn, _) if !optional && !in_list_or_map && !visiting.contains(qn) => {
            if let Some(alias_ty) = type_aliases.get(qn) {
                visiting.insert(qn.clone());
                extract_required_class_deps(
                    alias_ty,
                    class_fields,
                    type_aliases,
                    deps,
                    optional,
                    in_list_or_map,
                    visiting,
                );
                visiting.remove(qn);
            }
        }
        Ty::TypeAlias(_, _) => {}
        // `T?` lowers to `Union([T, Null])` (canary removed the `Ty::Optional`
        // variant), so optionals are handled by the `Ty::Union` arm below —
        // which already yields no hard dependency (Null breaks it).
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            // List breaks the hard dependency (can be empty)
            extract_required_class_deps(
                inner,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
        }
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
            // Map breaks the hard dependency (can be empty)
            extract_required_class_deps(
                key,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
            extract_required_class_deps(
                value,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
        }
        Ty::Union(members, _) => {
            // Union: only a hard dependency if ALL variants lead to the same class.
            // If any variant provides an alternative (e.g. string), the cycle is broken.
            let mut variant_deps_list = Vec::new();
            for member in members {
                let mut variant_deps = HashSet::new();
                extract_required_class_deps(
                    member,
                    class_fields,
                    type_aliases,
                    &mut variant_deps,
                    optional,
                    in_list_or_map,
                    visiting,
                );
                variant_deps_list.push(variant_deps);
            }
            // Only add if ALL variants produce the same single dep
            if !variant_deps_list.is_empty() {
                let first = &variant_deps_list[0];
                if first.len() == 1 && variant_deps_list.iter().all(|d| d == first) {
                    deps.extend(first.iter().cloned());
                }
            }
        }
        _ => {}
    }
}

/// Format a cycle path as "A -> B -> C -> A".
fn format_cycle_path(cycle: &[QualifiedTypeName]) -> String {
    if cycle.len() == 1 {
        cycle[0].to_string()
    } else {
        let mut path: Vec<String> = cycle.iter().map(std::string::ToString::to_string).collect();
        path.push(cycle[0].to_string());
        path.join(" -> ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::{Freshness, FunctionParamTy, TyAttr};

    fn qn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("test"), vec![], Name::new(name))
    }

    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(qn(name), TyAttr::default())
    }

    fn required_param(ty: Ty) -> FunctionParamTy {
        FunctionParamTy::required(None, ty)
    }

    fn optional_param(name: &str, ty: Ty) -> FunctionParamTy {
        FunctionParamTy::optional(Some(Name::new(name)), ty)
    }

    #[test]
    fn same_normalized_type_accepts_union_reordering() {
        let aliases = HashMap::new();
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let string = Ty::String {
            attr: TyAttr::default(),
        };
        let lhs = Ty::Union(vec![int.clone(), string.clone()], TyAttr::default());
        let rhs = Ty::Union(vec![string, int], TyAttr::default());

        assert!(is_same_normalized_type(
            &lhs,
            &rhs,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn same_normalized_type_rejects_narrower_union_member() {
        let aliases = HashMap::new();
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let string = Ty::String {
            attr: TyAttr::default(),
        };
        let union = Ty::Union(vec![int.clone(), string], TyAttr::default());

        assert!(!is_same_normalized_type(
            &int,
            &union,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn same_normalized_type_expands_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("IntOrString"),
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
        let direct = Ty::Union(
            vec![
                Ty::String {
                    attr: TyAttr::default(),
                },
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );

        assert!(is_same_normalized_type(
            &type_alias("IntOrString"),
            &direct,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
    }

    #[test]
    fn same_normalized_type_ignores_required_function_param_names() {
        let aliases = HashMap::new();
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let string = Ty::String {
            attr: TyAttr::default(),
        };
        let lhs = Ty::Function {
            params: vec![FunctionParamTy::required(Some(Name::new("x")), int.clone())],
            ret: Box::new(string.clone()),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let rhs = Ty::Function {
            params: vec![FunctionParamTy::required(Some(Name::new("y")), int)],
            ret: Box::new(string),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert!(is_same_normalized_type(
            &lhs,
            &rhs,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn same_normalized_type_sorts_optional_function_params() {
        let aliases = HashMap::new();
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let string = Ty::String {
            attr: TyAttr::default(),
        };
        let bool_ty = Ty::Bool {
            attr: TyAttr::default(),
        };
        let lhs = Ty::Function {
            params: vec![
                FunctionParamTy::optional(Some(Name::new("a")), int.clone()),
                FunctionParamTy::optional(Some(Name::new("b")), string.clone()),
            ],
            ret: Box::new(bool_ty.clone()),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let rhs = Ty::Function {
            params: vec![
                FunctionParamTy::optional(Some(Name::new("b")), string),
                FunctionParamTy::optional(Some(Name::new("a")), int),
            ],
            ret: Box::new(bool_ty),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert!(is_same_normalized_type(
            &lhs,
            &rhs,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_simple_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );

        assert!(is_subtype_of(
            &type_alias("MyInt"),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &type_alias("MyInt"),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
    }

    #[test]
    fn test_transitive_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );
        aliases.insert(qn("AnotherInt"), type_alias("MyInt"));

        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &type_alias("MyInt"),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
    }

    #[test]
    fn test_union_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("IntOrString"),
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

        assert!(is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::String {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::Bool {
                attr: TyAttr::default()
            },
            &type_alias("IntOrString"),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
    }

    #[test]
    fn test_recursive_alias_detection() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("List"),
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
        assert!(recursive.contains(&qn("List")));
    }

    #[test]
    fn test_non_recursive_not_marked() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(!recursive.contains(&qn("MyInt")));
    }

    #[test]
    fn test_never_is_bottom() {
        let aliases = HashMap::new();

        assert!(is_subtype_of(
            &Ty::Never {
                attr: TyAttr::default()
            },
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::Never {
                attr: TyAttr::default()
            },
            &Ty::String {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::Never {
                attr: TyAttr::default()
            },
            &Ty::Class(qn("Foo"), vec![], TyAttr::default()),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::Never {
                attr: TyAttr::default()
            },
            &Ty::optional(Ty::Int {
                attr: TyAttr::default()
            }),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_int_not_subtype_of_float() {
        // `Int <: Float` was removed entirely (lossy: i64 values past 2^53
        // are not exactly representable as f64). The widening, if needed,
        // must be explicit.
        let aliases = HashMap::new();
        assert!(!is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &Ty::Float {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::Float {
                attr: TyAttr::default()
            },
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_int_not_subtype_of_bigint() {
        // `int` is not a subtype of `bigint`: the relation is purely
        // structural (i64 and heap BigInt are different representations).
        // Widening happens only at the `bigint × int` operators and the FFI
        // boundary, never as an implicit move/subtype coercion.
        let aliases = HashMap::new();
        assert!(!is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &Ty::Bigint {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_literal_widens() {
        let aliases = HashMap::new();
        // Fresh and Regular should both be subtypes of their base primitive
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh, TyAttr::default()),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // `Literal(Int) <: Float` was removed (lossy past 2^53). The
        // widening, if needed, must be explicit.
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular, TyAttr::default()),
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular, TyAttr::default()),
            &Ty::Bigint {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular, TyAttr::default()),
            &Ty::Float {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &Ty::Literal(
                LiteralValue::String("hi".into()),
                Freshness::Fresh,
                TyAttr::default()
            ),
            &Ty::String {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh, TyAttr::default()),
            &Ty::String {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // Freshness is ignored for subtyping: Fresh(1) <: Regular(1)
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh, TyAttr::default()),
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular, TyAttr::default()),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_enum_variant_subtype_of_enum() {
        let aliases = HashMap::new();
        assert!(is_subtype_of(
            &Ty::EnumVariant(qn("Color"), Name::new("Red"), TyAttr::default()),
            &Ty::Enum(qn("Color"), TyAttr::default()),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &Ty::EnumVariant(qn("Color"), Name::new("Red"), TyAttr::default()),
            &Ty::Enum(qn("Shape"), TyAttr::default()),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_covariant_return() {
        // Function return is covariant, but checked *coercion-free*: a function
        // value has no boundary at which to widen its result, so the covariance
        // uses coercion-free relations (`EnumVariant <: Enum`). The coercive
        // `int → bigint` pair is rejected in both directions.
        let aliases = HashMap::new();

        let returns_literal_string = Ty::Function {
            params: vec![required_param(Ty::Int {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::Literal(
                LiteralValue::String("ok".into()),
                Freshness::Fresh,
                TyAttr::default(),
            )),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let returns_string = Ty::Function {
            params: vec![required_param(Ty::Int {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &returns_literal_string,
            &returns_string,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &returns_string,
            &returns_literal_string,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));

        let returns_variant = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::EnumVariant(
                qn("Color"),
                Name::new("Red"),
                TyAttr::default(),
            )),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let returns_enum = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Enum(qn("Color"), TyAttr::default())),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &returns_variant,
            &returns_enum,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &returns_enum,
            &returns_variant,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));

        // `fn() -> int` is NOT usable as `fn() -> bigint`: there is no site to
        // insert the `int → bigint` coercion on the returned value.
        let returns_int = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let returns_bigint = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Bigint {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(!is_subtype_of(
            &returns_int,
            &returns_bigint,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &returns_bigint,
            &returns_int,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_contravariant_params() {
        // Function params are contravariant, and — like the return position —
        // checked coercion-free. A function accepting the wider `Enum` stands
        // in for one accepting an `EnumVariant`. The coercive `int`/`bigint`
        // pair is rejected in both directions.
        let aliases = HashMap::new();

        let accepts_string = Ty::Function {
            params: vec![required_param(Ty::String {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let accepts_literal_string = Ty::Function {
            params: vec![required_param(Ty::Literal(
                LiteralValue::String("hi".into()),
                Freshness::Fresh,
                TyAttr::default(),
            ))],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &accepts_string,
            &accepts_literal_string,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &accepts_literal_string,
            &accepts_string,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));

        let accepts_enum = Ty::Function {
            params: vec![required_param(Ty::Enum(qn("Color"), TyAttr::default()))],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let accepts_variant = Ty::Function {
            params: vec![required_param(Ty::EnumVariant(
                qn("Color"),
                Name::new("Red"),
                TyAttr::default(),
            ))],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &accepts_enum,
            &accepts_variant,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &accepts_variant,
            &accepts_enum,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));

        // `fn(bigint)` is NOT usable as `fn(int)`: the caller's `int` argument
        // has no site at which to widen before reaching the `bigint` callee.
        let accepts_bigint = Ty::Function {
            params: vec![required_param(Ty::Bigint {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let accepts_int = Ty::Function {
            params: vec![required_param(Ty::Int {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        assert!(!is_subtype_of(
            &accepts_bigint,
            &accepts_int,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &accepts_int,
            &accepts_bigint,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_covariant_throws() {
        let aliases = HashMap::new();
        let f1 = Ty::Function {
            params: vec![required_param(Ty::Int {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let f2 = Ty::Function {
            params: vec![required_param(Ty::Int {
                attr: TyAttr::default(),
            })],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Union(
                vec![
                    Ty::Enum(qn("IoError"), TyAttr::default()),
                    Ty::Enum(qn("Timeout"), TyAttr::default()),
                ],
                TyAttr::default(),
            )),
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &f1,
            &f2,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &f2,
            &f1,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_optional_param_dropping_subtyping() {
        let aliases = HashMap::new();
        let with_two_optionals = Ty::Function {
            params: vec![
                required_param(Ty::String {
                    attr: TyAttr::default(),
                }),
                optional_param(
                    "max",
                    Ty::Int {
                        attr: TyAttr::default(),
                    },
                ),
                optional_param(
                    "filter",
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ),
            ],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let with_one_optional = Ty::Function {
            params: vec![
                required_param(Ty::String {
                    attr: TyAttr::default(),
                }),
                optional_param(
                    "filter",
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ),
            ],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert!(is_subtype_of(
            &with_two_optionals,
            &with_one_optional,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &with_one_optional,
            &with_two_optionals,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_optional_param_order_is_insignificant() {
        let aliases = HashMap::new();
        let f1 = Ty::Function {
            params: vec![
                required_param(Ty::String {
                    attr: TyAttr::default(),
                }),
                optional_param(
                    "max",
                    Ty::Int {
                        attr: TyAttr::default(),
                    },
                ),
                optional_param(
                    "filter",
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ),
            ],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let f2 = Ty::Function {
            params: vec![
                required_param(Ty::String {
                    attr: TyAttr::default(),
                }),
                optional_param(
                    "filter",
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ),
                optional_param(
                    "max",
                    Ty::Int {
                        attr: TyAttr::default(),
                    },
                ),
            ],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert!(is_subtype_of(
            &f1,
            &f2,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(is_subtype_of(
            &f2,
            &f1,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_function_optional_and_required_params_are_incomparable() {
        let aliases = HashMap::new();
        let optional = Ty::Function {
            params: vec![optional_param(
                "value",
                Ty::Int {
                    attr: TyAttr::default(),
                },
            )],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let required = Ty::Function {
            params: vec![FunctionParamTy::required(
                Some(Name::new("value")),
                Ty::Int {
                    attr: TyAttr::default(),
                },
            )],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert!(!is_subtype_of(
            &optional,
            &required,
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        assert!(!is_subtype_of(
            &required,
            &optional,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_optional_subtyping() {
        let aliases = HashMap::new();
        // int <: int?
        assert!(is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &Ty::optional(Ty::Int {
                attr: TyAttr::default()
            }),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // null <: int?
        assert!(is_subtype_of(
            &Ty::Null {
                attr: TyAttr::default()
            },
            &Ty::optional(Ty::Int {
                attr: TyAttr::default()
            }),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // string NOT <: int?
        assert!(!is_subtype_of(
            &Ty::String {
                attr: TyAttr::default()
            },
            &Ty::optional(Ty::Int {
                attr: TyAttr::default()
            }),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    // ── Evolving container tests ────────────────────────────────────────────

    #[test]
    fn test_evolving_list_subtype_of_list() {
        let aliases = HashMap::new();
        // EvolvingList(int) <: List(int)
        assert!(is_subtype_of(
            &Ty::EvolvingList(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // List(int) <: EvolvingList(int)
        assert!(is_subtype_of(
            &Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &Ty::EvolvingList(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_evolving_list_is_invariant() {
        let aliases = HashMap::new();
        // `List`/`EvolvingList` are invariant. So `EvolvingList(int)` is NOT a
        // subtype of `List(bigint)` — `int` is not a subtype of `bigint`
        // anywhere, including inside containers.
        assert!(!is_subtype_of(
            &Ty::EvolvingList(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &Ty::List(
                Box::new(Ty::Bigint {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // EvolvingList(string) NOT <: List(int) — unrelated primitives.
        assert!(!is_subtype_of(
            &Ty::EvolvingList(
                Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // Free widenings inside the container survive coercion-free
        // recursion: a literal int element widens to its base type.
        assert!(is_subtype_of(
            &Ty::EvolvingList(
                Box::new(Ty::Literal(
                    LiteralValue::Int(1),
                    Freshness::Fresh,
                    TyAttr::default(),
                )),
                TyAttr::default()
            ),
            &Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_evolving_list_never_is_bottom() {
        let aliases = HashMap::new();
        // EvolvingList(Never) <: List(int) — empty evolving is assignable anywhere
        assert!(is_subtype_of(
            &Ty::EvolvingList(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            ),
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_evolving_map_subtype_of_map() {
        let aliases = HashMap::new();
        // EvolvingMap(string, int) <: Map(string, int)
        assert!(is_subtype_of(
            &Ty::EvolvingMap(
                Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default(),
            ),
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_map_key_covariance_never() {
        let aliases = HashMap::new();
        // map<never, never> <: map<string, unknown> — empty map assignable anywhere
        assert!(is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::BuiltinUnknown {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // map<never, never> <: map<string, int>
        assert!(is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_map_key_covariance_literal() {
        let aliases = HashMap::new();
        // map<"name", "World"> <: map<string, unknown>
        assert!(is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::Literal(
                    LiteralValue::String("name".into()),
                    Freshness::Fresh,
                    TyAttr::default()
                )),
                value: Box::new(Ty::Literal(
                    LiteralValue::String("World".into()),
                    Freshness::Fresh,
                    TyAttr::default()
                )),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::BuiltinUnknown {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // map<"name", "World"> <: map<string, string>
        assert!(is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::Literal(
                    LiteralValue::String("name".into()),
                    Freshness::Fresh,
                    TyAttr::default()
                )),
                value: Box::new(Ty::Literal(
                    LiteralValue::String("World".into()),
                    Freshness::Fresh,
                    TyAttr::default()
                )),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_map_invariant_no_int_to_float_value_widening() {
        let aliases = HashMap::new();
        // `Map` is invariant — the value-type recursion is coercion-free.
        // `int <: float` was removed entirely (as a lossy/unsound rule),
        // so `map<string, int>` is **not** a subtype of `map<string, float>`.
        assert!(!is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Float {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_map_invariant_no_int_to_bigint_value_widening() {
        let aliases = HashMap::new();
        // `Map` is invariant, and `int` is not a subtype of `bigint` anywhere
        // (no implicit numeric widening), so `map<string, int>` is not a
        // subtype of `map<string, bigint>`.
        assert!(!is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Bigint {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_map_key_not_supertype() {
        let aliases = HashMap::new();
        // map<string, int> NOT <: map<"name", int> — widening key is not subtyping
        assert!(!is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::Literal(
                    LiteralValue::String("name".into()),
                    Freshness::Fresh,
                    TyAttr::default()
                )),
                value: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // map<string, string> NOT <: map<int, string> — incompatible key types
        assert!(!is_subtype_of(
            &Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &Ty::Map {
                key: Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            },
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_make_evolving() {
        // List(Never) → EvolvingList(Never)
        assert_eq!(
            Ty::List(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .make_evolving(),
            Ty::EvolvingList(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
        );
        // Map(Never, Never) → EvolvingMap(Never, Never)
        assert_eq!(
            Ty::Map {
                key: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                value: Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                attr: TyAttr::default(),
            }
            .make_evolving(),
            Ty::EvolvingMap(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
        );
        // Non-empty List passes through
        assert_eq!(
            Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .make_evolving(),
            Ty::List(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
        );
        // Non-container passes through
        assert_eq!(
            Ty::Int {
                attr: TyAttr::default()
            }
            .make_evolving(),
            Ty::Int {
                attr: TyAttr::default()
            }
        );
    }

    #[test]
    fn test_evolving_display() {
        assert_eq!(
            Ty::EvolvingList(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .render_canonical(),
            "_[]"
        );
        assert_eq!(
            Ty::EvolvingList(
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .render_canonical(),
            "int[] (evolving)"
        );
        assert_eq!(
            Ty::EvolvingMap(
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                Box::new(Ty::Never {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .render_canonical(),
            "map<_, _>"
        );
        assert_eq!(
            Ty::EvolvingMap(
                Box::new(Ty::String {
                    attr: TyAttr::default()
                }),
                Box::new(Ty::Int {
                    attr: TyAttr::default()
                }),
                TyAttr::default()
            )
            .render_canonical(),
            "map<string, int> (evolving)"
        );
    }

    // ── Literal-union subtype tests ─────────────────────────────────────────

    #[test]
    fn test_union_of_int_literals_subtype_of_int() {
        let aliases = HashMap::new();
        let sub = Ty::Union(
            vec![
                Ty::Literal(LiteralValue::Int(1), Freshness::Fresh, TyAttr::default()),
                Ty::Literal(LiteralValue::Int(2), Freshness::Fresh, TyAttr::default()),
                Ty::Literal(LiteralValue::Int(3), Freshness::Fresh, TyAttr::default()),
            ],
            TyAttr::default(),
        );
        let sup = Ty::Int {
            attr: TyAttr::default(),
        };
        assert!(is_subtype_of(
            &sub,
            &sup,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_list_of_literal_union_subtype_of_list_int() {
        let aliases = HashMap::new();
        let sub = Ty::List(
            Box::new(Ty::Union(
                vec![
                    Ty::Literal(LiteralValue::Int(1), Freshness::Fresh, TyAttr::default()),
                    Ty::Literal(LiteralValue::Int(2), Freshness::Fresh, TyAttr::default()),
                ],
                TyAttr::default(),
            )),
            TyAttr::default(),
        );
        let sup = Ty::List(
            Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );
        assert!(is_subtype_of(
            &sub,
            &sup,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_union_of_int_and_float_literals_not_subtype_of_float() {
        // `Literal(Int) <: Float` was removed entirely (i64 → f64 is lossy
        // past 2^53), so a union containing an int literal cannot widen to
        // `Float` even via the structural Union-vs-T rule (which requires
        // every member to be a subtype).
        let aliases = HashMap::new();
        let sub = Ty::Union(
            vec![
                Ty::Literal(LiteralValue::Int(1), Freshness::Fresh, TyAttr::default()),
                Ty::Literal(
                    LiteralValue::Float("2.0".to_string()),
                    Freshness::Fresh,
                    TyAttr::default(),
                ),
            ],
            TyAttr::default(),
        );
        let sup = Ty::Float {
            attr: TyAttr::default(),
        };
        assert!(!is_subtype_of(
            &sub,
            &sup,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_typevar_subtype_of_union_containing_same_typevar() {
        let aliases = HashMap::new();
        let t = Ty::TypeVar(Name::new("T"), TyAttr::default());
        let union = Ty::Union(
            vec![
                t.clone(),
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        // T <: T | String should hold
        assert!(is_subtype_of(
            &t,
            &union,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_typevar_not_subtype_of_union_without_same_typevar() {
        let aliases = HashMap::new();
        let t = Ty::TypeVar(Name::new("T"), TyAttr::default());
        let union = Ty::Union(
            vec![
                Ty::Int {
                    attr: TyAttr::default(),
                },
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        // T <: Int | String should NOT hold (T is opaque)
        assert!(!is_subtype_of(
            &t,
            &union,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_typevar_subtype_of_optional_same_typevar() {
        let aliases = HashMap::new();
        let t = Ty::TypeVar(Name::new("T"), TyAttr::default());
        let opt_t = Ty::optional(t.clone());
        // T <: T? should hold
        assert!(is_subtype_of(
            &t,
            &opt_t,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_typevar_not_subtype_of_concrete() {
        let aliases = HashMap::new();
        let t = Ty::TypeVar(Name::new("T"), TyAttr::default());
        // T <: Int should NOT hold
        assert!(!is_subtype_of(
            &t,
            &Ty::Int {
                attr: TyAttr::default()
            },
            &crate::normalize::resolved_aliases_from_map(aliases.clone())
        ));
        // Int <: T should NOT hold
        assert!(!is_subtype_of(
            &Ty::Int {
                attr: TyAttr::default()
            },
            &t,
            &crate::normalize::resolved_aliases_from_map(aliases)
        ));
    }

    #[test]
    fn test_recursive_alias_through_class_type_arg_is_detected() {
        // `type A = Box<A>` — recursion goes through a class generic argument.
        // Without descending into class type_args, cycle detection would miss
        // this and normalization would recurse forever.
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("A"),
            Ty::Class(qn("Box"), vec![type_alias("A")], TyAttr::default()),
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(
            recursive.contains(&qn("A")),
            "expected `type A = Box<A>` to be detected as recursive, got {recursive:?}"
        );
    }

    #[test]
    fn test_recursive_alias_through_interface_type_arg_is_detected() {
        // `type A = BoxLike<A>` — recursion goes through an interface generic
        // argument and must be detected just like class generic arguments.
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("A"),
            Ty::Interface(
                qn("BoxLike"),
                vec![type_alias("A")],
                vec![],
                TyAttr::default(),
            ),
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(
            recursive.contains(&qn("A")),
            "expected `type A = BoxLike<A>` to be detected as recursive, got {recursive:?}"
        );
    }
}
