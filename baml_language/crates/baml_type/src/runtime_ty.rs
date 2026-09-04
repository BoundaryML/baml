//! Inherent impls and conversions for [`RuntimeTy`], the deep runtime-facing
//! subset of [`Ty`].
//!
//! [`RuntimeTy`] (and [`ConcreteTy`]) are *defined* in [`crate::family`] by the
//! `ty_family!` macro; this module holds their hand-written behaviour. A
//! `RuntimeTy` contains only the variants that can legitimately exist outside
//! the compiler, and its nested positions hold `RuntimeTy` (not `Ty`), so a
//! value is statically free of compiler-only variants all the way down.
//!
//! Conversions:
//! - [`RuntimeTy::try_from`] (`&Ty`/`Ty`) is fallible: it rejects the
//!   compiler-only variants (`Error`, `Infer`) even when nested, returning
//!   [`NotRuntimeTy`].
//! - [`Ty::from`] (`RuntimeTy`/`&RuntimeTy`) is infallible.

use std::collections::{HashMap, HashSet};

use crate::{
    Freshness, Interface, Name, NotRuntimeTy, QualifiedTypeName, RuntimeFunctionParamTy,
    RuntimeInterface, RuntimeTy, Ty, TyAttr, TypeName,
};

// Head-agnostic: none of these mention a nominal head, so they are defined for
// every head representation rather than only the compiler's. A bare
// `RuntimeTy::int()` still means `RuntimeTy<TypeName>` — a type path uses the
// parameter's default — so the runtime spells its own instantiation explicitly.
// The nominal constructors below stay at `TypeName`, since building a head from
// a `&str` is exactly the thing only a name-headed type can do.
impl<N: Clone> RuntimeTy<N> {
    // --- Primitive constructors (default TyAttr) ---

    /// `int` with default attributes.
    pub fn int() -> Self {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }

    /// `bigint` with default attributes.
    pub fn bigint() -> Self {
        RuntimeTy::Bigint {
            attr: TyAttr::default(),
        }
    }

    /// `float` with default attributes.
    pub fn float() -> Self {
        RuntimeTy::Float {
            attr: TyAttr::default(),
        }
    }

    /// `string` with default attributes.
    pub fn string() -> Self {
        RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }

    /// `bool` with default attributes.
    pub fn bool() -> Self {
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        }
    }

    /// `null` with default attributes.
    pub fn null() -> Self {
        RuntimeTy::Null {
            attr: TyAttr::default(),
        }
    }

    /// `uint8array` with default attributes.
    pub fn uint8array() -> Self {
        RuntimeTy::Uint8Array {
            attr: TyAttr::default(),
        }
    }

    // --- Compound constructors (default TyAttr) ---

    /// `T[]` (list) with default attributes.
    pub fn list(inner: RuntimeTy<N>) -> Self {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }

    /// `map<K, V>` with default attributes.
    pub fn map(key: RuntimeTy<N>, value: RuntimeTy<N>) -> Self {
        RuntimeTy::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }

    /// `A | B | ...` (union) with default attributes.
    pub fn union(members: impl IntoIterator<Item = RuntimeTy<N>>) -> Self {
        RuntimeTy::Union(members.into_iter().collect(), TyAttr::default())
    }

    /// `T?` (optional) — sugar for `T | null`. Mirrors [`Ty::optional`]: the
    /// result is flattened and idempotent.
    pub fn optional(inner: RuntimeTy<N>) -> Self {
        match inner {
            RuntimeTy::Union(members, attr) => {
                if members.iter().any(RuntimeTy::is_null) {
                    RuntimeTy::Union(members, attr)
                } else {
                    let mut members = members.into_vec();
                    members.push(RuntimeTy::null());
                    RuntimeTy::Union(members.into(), attr)
                }
            }
            n @ RuntimeTy::Null { .. } => n,
            other => RuntimeTy::Union(Box::new([other, RuntimeTy::null()]), TyAttr::default()),
        }
    }

    /// `unknown` (the top type) with default attributes.
    pub fn unknown() -> Self {
        RuntimeTy::Unknown {
            attr: TyAttr::default(),
        }
    }

    /// Opaque resource handle type (file, socket, HTTP response body).
    pub fn resource() -> Self {
        RuntimeTy::Resource {
            attr: TyAttr::default(),
        }
    }

    /// Opaque structured prompt tree type for LLM calls.
    pub fn prompt_ast() -> Self {
        RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        }
    }

    /// Meta-type — a runtime value that wraps a [`RuntimeTy`].
    pub fn type_type() -> Self {
        RuntimeTy::Type {
            attr: TyAttr::default(),
        }
    }

    // --- Queries ---

    /// True if this is exactly the `null` type.
    pub fn is_null(&self) -> bool {
        matches!(self, RuntimeTy::Null { .. })
    }

    /// True if this is a union that includes `null` — i.e. an optional type.
    pub fn is_nullable_union(&self) -> bool {
        matches!(self, RuntimeTy::Union(members, _) if members.iter().any(RuntimeTy::is_null))
    }

    // --- Transforms ---

    /// Remove `null` from a nullable union, collapsing the result. The inverse
    /// of [`RuntimeTy::optional`]; mirrors [`Ty::strip_null`].
    pub fn strip_null(&self) -> RuntimeTy<N> {
        match self {
            RuntimeTy::Union(members, attr) => {
                let non_null: Box<[RuntimeTy<N>]> =
                    members.iter().filter(|m| !m.is_null()).cloned().collect();
                match non_null.len() {
                    0 => self.clone(),
                    1 => non_null
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| unreachable!("len checked")),
                    _ => RuntimeTy::Union(non_null, attr.clone()),
                }
            }
            _ => self.clone(),
        }
    }

    // --- Rendering ---
    //
    // These reuse `Ty`'s implementation so the rendering logic lives in exactly
    // one place. The upcast is [`RuntimeTy::as_ty`] — a zero-cost borrow, not a
    // clone — so sharing the algorithm costs nothing; the value remains a
    // statically runtime-safe `RuntimeTy`. (Subtyping/equivalence have no method
    // form: they need nominal facts, so callers go through
    // [`crate::normalize`]'s `TypeContext` entry points with the richest context
    // the site can reach.)
}

/// The nominal constructors, which only a name-headed type can offer: a head
/// built from a `&str` is a name, and a runtime head has no such spelling.
impl RuntimeTy {
    /// `Class(name)` with default attributes (local module path), no type args.
    pub fn class(name: &str) -> Self {
        RuntimeTy::Class(
            TypeName::local(name.into()),
            Box::new([]),
            TyAttr::default(),
        )
    }

    /// `Class(name, args)` — a parametric class instantiation.
    pub fn class_with_args(name: TypeName, args: Box<[RuntimeTy]>) -> Self {
        RuntimeTy::Class(name, args, TyAttr::default())
    }

    /// `Class(name)` under the implicit `user` package, no type args.
    pub fn user_class(name: &str) -> Self {
        RuntimeTy::Class(
            TypeName::local(Name::new(name)),
            Box::new([]),
            TyAttr::default(),
        )
    }
}

impl<N: Clone + crate::HeadDisplay> std::fmt::Display for RuntimeTy<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_ty(), f)
    }
}

// ── Ty → RuntimeTy erasure ───────────────────────────────────────────────────
// The erasing counterpart of `RuntimeTy::try_from`: where `try_from` *rejects*
// compiler-only variants, `lower_to_runtime` *erases* them and additionally
// expands non-recursive type aliases inline. This is the single boundary the
// compiler crosses to hand a `Ty` to the runtime.

/// The resolved type-alias environment needed to erase a [`Ty`] into a
/// [`RuntimeTy`]: the alias targets to expand and the set of recursive aliases
/// to keep opaque. Built per package by the compiler (see
/// `baml_compiler2_mir::resolved_aliases_for_package`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedAliases {
    pub aliases: HashMap<QualifiedTypeName, Ty>,
    pub recursive: HashSet<QualifiedTypeName>,
}

impl ResolvedAliases {
    /// Build the environment from the collected alias targets, computing
    /// the recursive set (DFS cycle detection) here - the one constructor,
    /// so a caller cannot pair aliases with a stale recursive set.
    pub fn from_aliases(aliases: HashMap<QualifiedTypeName, Ty>) -> ResolvedAliases {
        let mut recursive = HashSet::new();
        for name in aliases.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if has_cycle(name, &aliases, &mut visited, &mut stack) {
                recursive.insert(name.clone());
            }
        }
        ResolvedAliases { aliases, recursive }
    }

    /// Lower a [`Ty`] into a [`RuntimeTy`] using this alias environment.
    ///
    /// This is the compiler's ergonomic entry point and asserts the conversion
    /// succeeds. Per the type-system golden rule (prefer compiler errors over
    /// type-erasure), reaching here with an `Unknown`/`Error` sentinel means an
    /// error-recovery type slipped past type-checking into MIR lowering — a
    /// compiler bug — so it panics loudly rather than silently producing a
    /// degraded type. Callers that genuinely tolerate failure use
    /// [`lower_to_runtime`] directly.
    pub fn convert(&self, ty: &Ty) -> RuntimeTy {
        lower_to_runtime(ty, self).unwrap_or_else(|e| {
            unreachable!("{e}: an error-recovery type reached runtime lowering")
        })
    }
}

/// Lower a compiler-facing [`Ty`] into a runtime-safe [`RuntimeTy`], expanding
/// non-recursive type aliases inline. Every other variant —
/// including `Never`, `TypeVar`, and `AssociatedTypeProjection` — maps
/// faithfully to its same-named [`RuntimeTy`] variant: the runtime carries them
/// for reflection and dynamic dispatch, and erasing them would violate the
/// type contract.
///
/// Fails with [`NotRuntimeTy`] on the error-recovery sentinel `Error` and on an
/// unfilled `Infer` hole: those exist only during compilation, so a type-checked program can
/// never contain one. Reaching this boundary with one is a compiler bug — we
/// surface it instead of erasing it to a degraded runtime type.
pub fn lower_to_runtime(ty: &Ty, resolved: &ResolvedAliases) -> Result<RuntimeTy, NotRuntimeTy> {
    Ok(match ty {
        // Primitives — same-named runtime variant.
        Ty::Int { attr } => RuntimeTy::Int { attr: attr.clone() },
        Ty::Bigint { attr } => RuntimeTy::Bigint { attr: attr.clone() },
        Ty::Float { attr } => RuntimeTy::Float { attr: attr.clone() },
        Ty::String { attr } => RuntimeTy::String { attr: attr.clone() },
        Ty::Bool { attr } => RuntimeTy::Bool { attr: attr.clone() },
        Ty::Null { attr } => RuntimeTy::Null { attr: attr.clone() },
        Ty::Uint8Array { attr } => RuntimeTy::Uint8Array { attr: attr.clone() },
        Ty::Media(kind, attr) => RuntimeTy::Media(*kind, attr.clone()),

        // Named types
        Ty::Class(qtn, type_args, attr) => {
            RuntimeTy::Class(qtn.clone(), lower_vec(type_args, resolved)?, attr.clone())
        }
        Ty::Interface(qtn, type_args, associated_bindings, attr) => {
            let resolved_args = lower_vec(type_args, resolved)?;
            let resolved_bindings = associated_bindings
                .iter()
                .map(|(name, ty)| Ok((name.clone(), lower_to_runtime(ty, resolved)?)))
                .collect::<Result<Box<[_]>, NotRuntimeTy>>()?;
            RuntimeTy::Interface(qtn.clone(), resolved_args, resolved_bindings, attr.clone())
        }
        Ty::Enum(qtn, attr) => RuntimeTy::Enum(qtn.clone(), attr.clone()),
        Ty::TypeAlias(qtn, attr) => {
            if resolved.recursive.contains(qtn) {
                // Keep recursive aliases opaque — they need runtime resolution
                RuntimeTy::TypeAlias(qtn.clone(), attr.clone())
            } else if let Some(target) = resolved.aliases.get(qtn) {
                // Expand non-recursive aliases inline
                lower_to_runtime(target, resolved)?
            } else {
                // An alias the environment cannot see is a name nothing will
                // ever declare: it cannot be expanded here and, unlike a
                // recursive alias, no pooled declaration will exist for the
                // runtime to resolve it against. Carrying it opaque bakes a
                // dangling reference into the program image, so the completeness
                // precondition (own package + every dependency; see
                // `TypeContext::alias_def`) is enforced rather than assumed.
                return Err(NotRuntimeTy {
                    variant: "TypeAlias (not in the resolved-alias environment)",
                });
            }
        }

        // EnumVariant → preserve variant-level type info
        Ty::EnumVariant(qtn, variant, attr) => {
            RuntimeTy::EnumVariant(qtn.clone(), variant.clone(), attr.clone())
        }

        // Containers
        Ty::List(inner, attr) => {
            RuntimeTy::List(Box::new(lower_to_runtime(inner, resolved)?), attr.clone())
        }
        Ty::Map {
            key: k,
            value: v,
            attr,
        } => RuntimeTy::Map {
            key: Box::new(lower_to_runtime(k, resolved)?),
            value: Box::new(lower_to_runtime(v, resolved)?),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => RuntimeTy::Union(lower_vec(members, resolved)?, attr.clone()),
        // Freshness is a compiler-only flag; runtime literal types are uniform,
        // so normalize to `Regular` at the boundary.
        Ty::Literal(lit, _freshness, attr) => {
            RuntimeTy::Literal(lit.clone(), Freshness::Regular, attr.clone())
        }

        // Functions — preserve the param metadata; body type-vars (captured from
        // the enclosing context) are resolved faithfully by the recursive
        // `lower_to_runtime` calls.
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => RuntimeTy::Function {
            params: params
                .iter()
                .map(|param| {
                    Ok(RuntimeFunctionParamTy {
                        name: param.name.clone(),
                        ty: lower_to_runtime(&param.ty, resolved)?,
                        mode: param.mode,
                    })
                })
                .collect::<Result<Box<[_]>, NotRuntimeTy>>()?,
            ret: Box::new(lower_to_runtime(ret, resolved)?),
            throws: Box::new(lower_to_runtime(throws, resolved)?),
            attr: attr.clone(),
        },

        // Bottom, opaque-leaf, and reflection types map faithfully.
        Ty::Never { attr } => RuntimeTy::Never { attr: attr.clone() },
        Ty::Void { attr } => RuntimeTy::Void { attr: attr.clone() },
        Ty::Unknown { attr } => RuntimeTy::Unknown { attr: attr.clone() },
        Ty::RustType { attr } => RuntimeTy::RustType { attr: attr.clone() },
        Ty::Type { attr } => RuntimeTy::Type { attr: attr.clone() },
        Ty::Resource { attr } => RuntimeTy::Resource { attr: attr.clone() },
        Ty::PromptAst { attr } => RuntimeTy::PromptAst { attr: attr.clone() },
        Ty::TypeVar(name, attr) => RuntimeTy::TypeVar(name.clone(), attr.clone()),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => RuntimeTy::AssociatedTypeProjection {
            base: Box::new(lower_to_runtime(base, resolved)?),
            interface: Box::new(lower_interface_to_runtime(interface, resolved)?),
            member: member.clone(),
            attr: attr.clone(),
        },

        // BEP-034: future types pass through unchanged with both
        // value and error type parameters mapped.
        Ty::Future(value, error, attr) => RuntimeTy::Future(
            Box::new(lower_to_runtime(value, resolved)?),
            Box::new(lower_to_runtime(error, resolved)?),
            attr.clone(),
        ),
        // Error-recovery sentinels cannot exist in a type-checked program.
        Ty::Error { .. } => return Err(NotRuntimeTy { variant: "Error" }),
    })
}

/// Lower each [`Ty`] in `tys`, short-circuiting on the first error-recovery
/// sentinel encountered (at any nesting depth).
fn lower_vec(tys: &[Ty], resolved: &ResolvedAliases) -> Result<Box<[RuntimeTy]>, NotRuntimeTy> {
    tys.iter().map(|t| lower_to_runtime(t, resolved)).collect()
}

/// Lower an interface *constraint* (the `as I` of an associated-type
/// projection) to its runtime form, lowering every generic argument and
/// associated-type binding. Mirrors the `Ty::Interface` arm of
/// [`lower_to_runtime`].
fn lower_interface_to_runtime(
    interface: &Interface,
    resolved: &ResolvedAliases,
) -> Result<RuntimeInterface, NotRuntimeTy> {
    Ok(RuntimeInterface {
        name: interface.name.clone(),
        generics: lower_vec(&interface.generics, resolved)?,
        associated_types: interface
            .associated_types
            .iter()
            .map(|(name, ty)| Ok((name.clone(), lower_to_runtime(ty, resolved)?)))
            .collect::<Result<Box<[_]>, NotRuntimeTy>>()?,
    })
}

fn has_cycle(
    name: &QualifiedTypeName,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    visited: &mut HashSet<QualifiedTypeName>,
    stack: &mut HashSet<QualifiedTypeName>,
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
    aliases: &HashMap<QualifiedTypeName, Ty>,
    visited: &mut HashSet<QualifiedTypeName>,
    stack: &mut HashSet<QualifiedTypeName>,
) -> bool {
    match ty {
        Ty::TypeAlias(qn, _) if aliases.contains_key(qn) => has_cycle(qn, aliases, visited, stack),
        Ty::List(inner, _) => ty_has_cycle(inner, aliases, visited, stack),
        Ty::Map { key, value, .. } => {
            ty_has_cycle(key, aliases, visited, stack)
                || ty_has_cycle(value, aliases, visited, stack)
        }
        Ty::Union(types, _) => types
            .iter()
            .any(|t| ty_has_cycle(t, aliases, visited, stack)),
        Ty::Class(_, type_args, _) => type_args
            .iter()
            .any(|t| ty_has_cycle(t, aliases, visited, stack)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args
                .iter()
                .any(|t| ty_has_cycle(t, aliases, visited, stack))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| ty_has_cycle(ty, aliases, visited, stack))
        }
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            ty_has_cycle(base, aliases, visited, stack)
                || interface
                    .tys()
                    .any(|t| ty_has_cycle(t, aliases, visited, stack))
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| ty_has_cycle(&param.ty, aliases, visited, stack))
                || ty_has_cycle(ret, aliases, visited, stack)
                || ty_has_cycle(throws, aliases, visited, stack)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoweringTy;

    fn def() -> TyAttr {
        TyAttr::default()
    }

    /// The head-free constructors build at any head, while a bare path still
    /// means the compiler's.
    ///
    /// Both halves matter. The runtime needs `list`/`union`/`optional` at its
    /// own head — they describe structure and mention no name — and every
    /// existing `RuntimeTy::int()` call site must keep resolving to `TypeName`,
    /// which it does because a type path applies the parameter's default.
    #[test]
    fn head_free_constructors_build_at_any_head() {
        let at_default = RuntimeTy::optional(RuntimeTy::list(RuntimeTy::int()));
        let _: RuntimeTy<QualifiedTypeName> = at_default.clone();

        // The same structure at a head that is not a name at all.
        let interned: RuntimeTy<u32> =
            RuntimeTy::optional(RuntimeTy::list(RuntimeTy::<u32>::int()));
        assert_eq!(interned.strip_null(), RuntimeTy::list(RuntimeTy::int()));
        assert!(interned.is_nullable_union());

        // Nominal construction stays name-only: a head built from a `&str` is a
        // name, so it has no meaning at an interned head.
        assert_eq!(
            RuntimeTy::class_with_args(TypeName::local(Name::new("P")), Box::new([])),
            RuntimeTy::class("P"),
        );
    }

    fn qtn(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    /// `Ty::from(RuntimeTy::try_from(&ty)) == ty` for a set of deeply nested
    /// runtime types.
    fn assert_round_trips(ty: Ty) {
        let runtime =
            RuntimeTy::try_from(&ty).unwrap_or_else(|e| panic!("expected a runtime type, got {e}"));
        assert_eq!(Ty::from(runtime), ty);
    }

    #[test]
    fn round_trip_nested_list_of_class() {
        // list<Class<int>>
        let ty: Ty = Ty::List(
            Box::new(Ty::Class(
                qtn("Box"),
                Box::new([Ty::Int { attr: def() }]),
                def(),
            )),
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_map() {
        let ty: Ty = Ty::Map {
            key: Box::new(Ty::String { attr: def() }),
            value: Box::new(Ty::List(Box::new(Ty::Bool { attr: def() }), def())),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_union() {
        let ty: Ty = Ty::Union(
            Box::new([
                Ty::Int { attr: def() },
                Ty::String { attr: def() },
                Ty::Null { attr: def() },
            ]),
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_function() {
        let ty: Ty = Ty::Function {
            params: Box::new([
                crate::FunctionParamTy::required(Some(Name::new("a")), Ty::Int { attr: def() }),
                crate::FunctionParamTy::optional(
                    Some(Name::new("b")),
                    Ty::List(Box::new(Ty::Float { attr: def() }), def()),
                ),
            ]),
            ret: Box::new(Ty::Bool { attr: def() }),
            throws: Box::new(Ty::Void { attr: def() }),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_interface_with_associated_bindings() {
        let ty: Ty = Ty::Interface(
            qtn("Iterator"),
            Box::new([Ty::Int { attr: def() }]),
            Box::new([(Name::new("Item"), Ty::String { attr: def() })]),
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_associated_type_projection() {
        let ty: Ty = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::type_var("T")),
            interface: Box::new(Interface {
                name: qtn("Iterator"),
                generics: Box::new([]),
                associated_types: Box::new([]),
            }),
            member: Name::new("Item"),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn nested_infer_in_list_blocks_conversion() {
        let ty: LoweringTy = LoweringTy::List(Box::new(LoweringTy::Infer { attr: def() }), def());
        assert_eq!(
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Infer" })
        );
    }

    #[test]
    fn nested_error_in_map_value_blocks_conversion() {
        let ty: Ty = Ty::Map {
            key: Box::new(Ty::String { attr: def() }),
            value: Box::new(Ty::Error { attr: def() }),
            attr: def(),
        };
        assert_eq!(
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Error" })
        );
    }

    #[test]
    fn nested_error_in_union_blocks_conversion() {
        let ty: Ty = Ty::Union(
            Box::new([Ty::Int { attr: def() }, Ty::Error { attr: def() }]),
            def(),
        );
        assert_eq!(
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Error" })
        );
    }

    #[test]
    fn nested_infer_in_function_ret_blocks_conversion() {
        let ty: LoweringTy = LoweringTy::Function {
            params: Box::new([]),
            ret: Box::new(LoweringTy::Infer { attr: def() }),
            throws: Box::new(LoweringTy::Void { attr: def() }),
            attr: def(),
        };
        assert_eq!(
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Infer" })
        );
    }
}
