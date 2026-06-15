//! Canonical type normalization, equivalence, and subtyping for [`Ty`].
//!
//! This is the **current-context** type algebra: it answers "do these two types
//! denote the same set of values *as the program is written*?" and "is every
//! value of `sub` also a value of `sup`?". Type variables and interface
//! existentials are compared **by identity** (`T` is the same type as `T`, `dyn I`
//! is the same type as `dyn I`, but `dyn I` is *not* the same type as a concrete
//! implementor `C`). This is distinct from the coherence overlap checker's
//! "possible-worlds" view (could these *ever* overlap under some instantiation?),
//! which is intentionally more permissive.
//!
//! # Representation
//!
//! Everything operates on [`Ty`] — the widest representation. The runtime
//! subenum layers ([`crate::RuntimeTy`], [`crate::RealizedTy`],
//! [`crate::ConcreteTy`]) are subsets of `Ty`, so a runtime caller widens up via
//! the infallible `Ty::from(..)` and calls the same checker. The compiler-only
//! variants (`Unknown`, `Error`, `EvolvingList`, `EvolvingMap`) are handled here
//! because inference asks subtype questions while those variants are live; a
//! runtime caller simply never produces them.
//!
//! # Context
//!
//! The nominal facts the algebra needs — what a type alias expands to, whether a
//! concrete type implements an interface, an interface's required interfaces, a
//! type variable's bound, and an enum's full variant set — are supplied by a
//! [`TypeContext`] the caller implements over its own registries. Following the
//! golden rule (a value never violates its static type), **every lookup fails
//! safe**: when the context cannot answer, the algebra declines to collapse,
//! absorb, or equate, so a missing fact can only yield "not *necessarily*
//! equivalent / subtype", never a false claim of equivalence or membership.

use std::collections::HashSet;

use crate::{
    FunctionParamMode, FunctionParamTy, Literal, MediaKind, Name, QualifiedTypeName, Ty, TyAttr,
};

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Semantic lookups the type algebra needs, supplied by the caller over its own
/// registries.
///
/// Every method **fails safe**: a `None`/`false` answer (whether "definitely
/// not" or merely "cannot determine") makes the algebra conservative — it will
/// not collapse, absorb, or equate what it cannot confirm. A missing fact
/// therefore degrades only to "not necessarily equivalent / subtype".
pub trait TypeContext {
    /// The type a type alias expands to, or `None` if the alias is unknown.
    ///
    /// Recursion is discovered structurally (an alias that re-references itself
    /// during expansion becomes a μ-binder), so this need only return the direct
    /// right-hand side. An unknown alias is treated as an opaque leaf (equal only
    /// to the same unknown alias), never equated to any expansion.
    ///
    /// **Completeness is a precondition.** Well-formed BAML never contains an
    /// unresolved alias — a bad name is a compile error (`Ty::Unknown`) and only
    /// recursive aliases survive lowering as `Ty::TypeAlias`, so the map must
    /// cover the package's own aliases plus every exported dependency alias.
    /// This holds for dynamically compiled/grafted packages too: each is a
    /// complete, valid package before it is loaded. A `None` therefore is not an
    /// expected "partial context" — it means the supplied map was incomplete,
    /// and the algebra degrades conservatively (opaque, never equated; "not
    /// necessarily equivalent") rather than panicking or over-equating.
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty>;

    /// Whether the non-interface, non-type-variable `concrete` type implements
    /// `interface` (a [`Ty::Interface`]), accounting for the interface's generic
    /// arguments, associated-type bindings, and the impl's bounds.
    ///
    /// Powers `C <: I` subtyping and the `C | I == I` union absorption (a
    /// concrete member subsumed by an existential member). `false` ⇒ no
    /// membership is claimed.
    fn implements_interface(&self, concrete: &Ty, interface: &Ty) -> bool;

    /// The declared bound of type variable `name` (an interface or a union of
    /// interfaces), or `None` if it is unbounded or unknown.
    ///
    /// Powers `T <: I` (and the `T | I == I` absorption) when `T`'s bound
    /// is — or transitively requires — `I`.
    fn type_var_bound(&self, name: &Name) -> Option<Ty>;

    /// Whether interface `sub` requires interface `sup` (reflexively and
    /// transitively), accounting for generic arguments. Both are
    /// [`Ty::Interface`] values.
    ///
    /// Powers `A <: B` subtyping and the `A | B == B` absorption for
    /// existentials. `false` ⇒ no requirement is claimed.
    fn interface_requires(&self, sub: &Ty, sup: &Ty) -> bool;

    /// The complete set of variant names of an enum, or `None` if the enum is
    /// unknown.
    ///
    /// Powers the completeness collapse `E.A | E.B | … == E` (a union of *all* of
    /// an enum's variants is the enum itself). `None` ⇒ no collapse.
    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>>;
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Normalize `ty` to its canonical form and render it back as a [`Ty`].
///
/// Two types are [`equivalent`] iff their canonical forms are structurally
/// equal. The canonical form applies the full set-theoretic algebra (union
/// flatten/sort/dedup, `never` removal, `unknown` absorption, literal-into-base
/// and enum-completeness collapse, interface absorption, alias expansion) so
/// that distinct spellings of the same type converge.
pub fn normalize<C: TypeContext>(ty: &Ty, ctx: &C) -> Ty {
    canonical(ty, ctx).into_ty()
}

/// Whether `a` and `b` denote the same type under the current context.
///
/// This is invariant equality, not assignability: use it where two spellings
/// must denote *the same* type (e.g. exact-type operator operands, interface
/// field implementations), not merely compatible ones.
pub fn equivalent<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    canonical(a, ctx) == canonical(b, ctx)
}

/// Whether every value of `sub` is also a value of `sup` under the current
/// context (the subset relation).
pub fn is_subtype<C: TypeContext>(sub: &Ty, sup: &Ty, ctx: &C) -> bool {
    let sub = canonical(sub, ctx);
    let sup = canonical(sup, ctx);
    sub.is_subtype_of(&sup, ctx, &mut HashSet::new())
}

/// Normalize and canonicalize in one step (the shared entry point).
fn canonical<C: TypeContext>(ty: &Ty, ctx: &C) -> NormalTy {
    NormalTy::from_ty(ty, ctx, &mut HashSet::new()).canonicalize(ctx)
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZED TYPE (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Normalized structural type: aliases resolved, attributes and literal
/// freshness erased, recursion made explicit with μ-binders.
///
/// Ordering (`PartialOrd`/`Ord`) is the canonical sort key for union members; it
/// has no semantic meaning beyond producing a deterministic canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NormalTy {
    // Primitive leaves
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Uint8Array,
    Media(MediaKind),
    // Nominal opaque leaves — each compatible only with itself.
    Void,
    RustType,
    Type,
    Resource,
    PromptAst,
    // Literal — a single value as a type. Freshness is erased.
    Literal(Literal),
    // Nominal references
    Class(QualifiedTypeName, Vec<NormalTy>),
    Interface(QualifiedTypeName, Vec<NormalTy>, Vec<(Name, NormalTy)>),
    Enum(QualifiedTypeName),
    EnumVariant(QualifiedTypeName, Name),
    // Constructors
    List(Box<NormalTy>),
    Map {
        key: Box<NormalTy>,
        value: Box<NormalTy>,
    },
    Union(Vec<NormalTy>),
    Function {
        params: Vec<NormalParam>,
        ret: Box<NormalTy>,
        throws: Box<NormalTy>,
    },
    Future(Box<NormalTy>, Box<NormalTy>),
    WatchAccessor(Box<NormalTy>),
    AssociatedTypeProjection {
        base: Box<NormalTy>,
        interface: Option<Box<NormalTy>>,
        member: Name,
    },
    // Recursion
    Mu {
        var: QualifiedTypeName,
        body: Box<NormalTy>,
    },
    /// μ-bound recursion variable (a back-reference to an enclosing [`NormalTy::Mu`]).
    RecVar(QualifiedTypeName),
    /// A generic type parameter — opaque, compatible only with itself, its
    /// bound's supertypes, and the top type.
    TypeVar(Name),
    /// An alias the context could not resolve — opaque, equal only to the same
    /// unresolved alias (fail-safe; never equated to an expansion).
    OpaqueAlias(QualifiedTypeName),
    // Special forms
    /// Bottom — a subtype of every type.
    Never,
    /// The explicit `unknown` keyword — top, a supertype of every type.
    BuiltinUnknown,
    /// Error-recovery sentinel — bidirectionally compatible to suppress cascades.
    Unknown,
    /// Hard-error sentinel — bidirectionally compatible, like [`NormalTy::Unknown`].
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct NormalParam {
    name: Option<Name>,
    ty: NormalTy,
    mode: FunctionParamMode,
}

impl NormalTy {
    // ── conversion in: Ty → NormalTy ───────────────────────────────────────

    fn from_ty<C: TypeContext>(
        ty: &Ty,
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
    ) -> NormalTy {
        match ty {
            Ty::Int { .. } => NormalTy::Int,
            Ty::Bigint { .. } => NormalTy::Bigint,
            Ty::Float { .. } => NormalTy::Float,
            Ty::String { .. } => NormalTy::String,
            Ty::Bool { .. } => NormalTy::Bool,
            Ty::Null { .. } => NormalTy::Null,
            Ty::Uint8Array { .. } => NormalTy::Uint8Array,
            Ty::Media(kind, _) => NormalTy::Media(*kind),
            Ty::Void { .. } => NormalTy::Void,
            Ty::RustType { .. } => NormalTy::RustType,
            Ty::Type { .. } => NormalTy::Type,
            Ty::Resource { .. } => NormalTy::Resource,
            Ty::PromptAst { .. } => NormalTy::PromptAst,
            Ty::BuiltinUnknown { .. } => NormalTy::BuiltinUnknown,
            Ty::Never { .. } => NormalTy::Never,
            Ty::Unknown { .. } => NormalTy::Unknown,
            Ty::Error { .. } => NormalTy::Error,
            // Freshness is a compiler-only widening flag, irrelevant to type identity.
            Ty::Literal(lit, _freshness, _) => NormalTy::Literal(lit.clone()),
            Ty::Class(qn, args, _) => {
                NormalTy::Class(qn.clone(), Self::from_tys(args, ctx, expanding))
            }
            Ty::Interface(qn, args, bindings, _) => {
                let mut bindings: Vec<_> = bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from_ty(ty, ctx, expanding)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(qn.clone(), Self::from_tys(args, ctx, expanding), bindings)
            }
            Ty::Enum(qn, _) => NormalTy::Enum(qn.clone()),
            Ty::EnumVariant(qn, v, _) => NormalTy::EnumVariant(qn.clone(), v.clone()),
            // Evolving containers are the list/map analogues during inference;
            // their type identity is the same as the frozen form.
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
                NormalTy::List(Box::new(Self::from_ty(inner, ctx, expanding)))
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => NormalTy::Map {
                key: Box::new(Self::from_ty(key, ctx, expanding)),
                value: Box::new(Self::from_ty(value, ctx, expanding)),
            },
            Ty::Union(members, _) => NormalTy::Union(Self::from_tys(members, ctx, expanding)),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => NormalTy::Function {
                params: params
                    .iter()
                    .map(|p| NormalParam {
                        name: p.name.clone(),
                        ty: Self::from_ty(&p.ty, ctx, expanding),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Self::from_ty(ret, ctx, expanding)),
                throws: Box::new(Self::from_ty(throws, ctx, expanding)),
            },
            Ty::Future(value, error, _) => NormalTy::Future(
                Box::new(Self::from_ty(value, ctx, expanding)),
                Box::new(Self::from_ty(error, ctx, expanding)),
            ),
            Ty::WatchAccessor(inner, _) => {
                NormalTy::WatchAccessor(Box::new(Self::from_ty(inner, ctx, expanding)))
            }
            Ty::TypeVar(name, _) => NormalTy::TypeVar(name.clone()),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(Self::from_ty(base, ctx, expanding)),
                interface: interface
                    .as_ref()
                    .map(|i| Box::new(Self::from_ty(i, ctx, expanding))),
                member: member.clone(),
            },
            Ty::TypeAlias(qn, _) => {
                if expanding.contains(qn) {
                    // Back-edge: we are already expanding this alias, so this is
                    // the recursive occurrence. The enclosing expansion wraps the
                    // result in a μ-binder over `qn`.
                    return NormalTy::RecVar(qn.clone());
                }
                let Some(def) = ctx.alias_def(qn) else {
                    // Fail-safe: an unresolvable alias is opaque, never equated
                    // to an expansion we cannot see.
                    return NormalTy::OpaqueAlias(qn.clone());
                };
                expanding.insert(qn.clone());
                let body = Self::from_ty(&def, ctx, expanding);
                expanding.remove(qn);
                if body.mentions_rec_var(qn) {
                    NormalTy::Mu {
                        var: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
        }
    }

    fn from_tys<C: TypeContext>(
        tys: &[Ty],
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
    ) -> Vec<NormalTy> {
        tys.iter()
            .map(|t| Self::from_ty(t, ctx, expanding))
            .collect()
    }

    /// Whether this type contains a back-reference to μ-variable `var`.
    fn mentions_rec_var(&self, var: &QualifiedTypeName) -> bool {
        match self {
            NormalTy::RecVar(v) => v == var,
            // A nested μ shadowing the same name rebinds it; stop descending.
            NormalTy::Mu { var: v, body } => v != var && body.mentions_rec_var(var),
            NormalTy::Class(_, args) => args.iter().any(|a| a.mentions_rec_var(var)),
            NormalTy::Interface(_, args, bindings) => {
                args.iter().any(|a| a.mentions_rec_var(var))
                    || bindings.iter().any(|(_, t)| t.mentions_rec_var(var))
            }
            NormalTy::List(inner) | NormalTy::WatchAccessor(inner) => inner.mentions_rec_var(var),
            NormalTy::Map { key, value } => {
                key.mentions_rec_var(var) || value.mentions_rec_var(var)
            }
            NormalTy::Union(members) => members.iter().any(|m| m.mentions_rec_var(var)),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().any(|p| p.ty.mentions_rec_var(var))
                    || ret.mentions_rec_var(var)
                    || throws.mentions_rec_var(var)
            }
            NormalTy::Future(value, error) => {
                value.mentions_rec_var(var) || error.mentions_rec_var(var)
            }
            NormalTy::AssociatedTypeProjection {
                base, interface, ..
            } => {
                base.mentions_rec_var(var)
                    || interface.as_ref().is_some_and(|i| i.mentions_rec_var(var))
            }
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::Void
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::TypeVar(_)
            | NormalTy::OpaqueAlias(_)
            | NormalTy::Never
            | NormalTy::BuiltinUnknown
            | NormalTy::Unknown
            | NormalTy::Error => false,
        }
    }

    // ── canonicalization ───────────────────────────────────────────────────

    /// Rewrite to a unique canonical form: children canonicalized bottom-up,
    /// unions reduced by the full set algebra.
    fn canonicalize<C: TypeContext>(self, ctx: &C) -> NormalTy {
        match self {
            NormalTy::Class(qn, args) => {
                NormalTy::Class(qn, args.into_iter().map(|a| a.canonicalize(ctx)).collect())
            }
            NormalTy::Interface(qn, args, bindings) => {
                let mut bindings: Vec<_> = bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize(ctx)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(
                    qn,
                    args.into_iter().map(|a| a.canonicalize(ctx)).collect(),
                    bindings,
                )
            }
            NormalTy::List(inner) => NormalTy::List(Box::new(inner.canonicalize(ctx))),
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.canonicalize(ctx)),
                value: Box::new(value.canonicalize(ctx)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.canonicalize(ctx)),
                Box::new(error.canonicalize(ctx)),
            ),
            NormalTy::WatchAccessor(inner) => {
                NormalTy::WatchAccessor(Box::new(inner.canonicalize(ctx)))
            }
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.canonicalize(ctx)),
                interface: interface.map(|i| Box::new(i.canonicalize(ctx))),
                member,
            },
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                // Required params are positional (names erased, order preserved);
                // optional params are keyed by name (order-insensitive, sorted).
                let mut required = Vec::new();
                let mut optional = Vec::new();
                for p in params {
                    let ty = p.ty.canonicalize(ctx);
                    match p.mode {
                        FunctionParamMode::Required => required.push(NormalParam {
                            name: None,
                            ty,
                            mode: FunctionParamMode::Required,
                        }),
                        FunctionParamMode::Optional => optional.push(NormalParam {
                            name: p.name,
                            ty,
                            mode: FunctionParamMode::Optional,
                        }),
                    }
                }
                optional.sort();
                required.extend(optional);
                NormalTy::Function {
                    params: required,
                    ret: Box::new(ret.canonicalize(ctx)),
                    throws: Box::new(throws.canonicalize(ctx)),
                }
            }
            NormalTy::Mu { var, body } => {
                let body = body.canonicalize(ctx);
                // A μ whose body no longer mentions its variable is not actually
                // recursive (an absorption may have removed the back-edge).
                if body.mentions_rec_var(&var) {
                    NormalTy::Mu {
                        var,
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
            NormalTy::Union(members) => {
                let members = members.into_iter().map(|m| m.canonicalize(ctx)).collect();
                canonicalize_union(members, ctx)
            }
            leaf => leaf,
        }
    }

    // ── subtyping ──────────────────────────────────────────────────────────

    /// Equirecursive subtyping with co-inductive assumptions. Operands must
    /// already be canonical.
    ///
    /// Purely structural except for the nominal facts drawn from the context
    /// (`C <: I`, `A <: B` via requires, `T <: I` via bound). There are no
    /// representation-changing coercions: `int` is a subtype of neither `bigint`
    /// nor `float`; the only widenings are literal-into-base and variant-into-enum.
    fn is_subtype_of<C: TypeContext>(
        &self,
        sup: &NormalTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        let pair = (self.clone(), sup.clone());
        if assumptions.contains(&pair) {
            return true;
        }
        if self == sup {
            return true;
        }
        // Bottom is a subtype of everything; top is a supertype of everything.
        if matches!(self, NormalTy::Never) {
            return true;
        }
        if matches!(sup, NormalTy::BuiltinUnknown) {
            return true;
        }
        // `unknown` (top) is a subtype of nothing else (reflexivity handled above).
        if matches!(self, NormalTy::BuiltinUnknown) {
            return false;
        }
        // Error-recovery sentinels are bidirectionally compatible to suppress
        // cascading diagnostics. A runtime caller never produces these.
        if matches!(self, NormalTy::Unknown | NormalTy::Error)
            || matches!(sup, NormalTy::Unknown | NormalTy::Error)
        {
            return true;
        }

        assumptions.insert(pair.clone());
        let result = match (self, sup) {
            // μ-unfolding (equirecursive).
            (NormalTy::Mu { var, body }, _) => {
                substitute(body, var, self).is_subtype_of(sup, ctx, assumptions)
            }
            (_, NormalTy::Mu { var, body }) => {
                self.is_subtype_of(&substitute(body, var, sup), ctx, assumptions)
            }

            // Union decomposition. `Union <: T` must precede `T <: Union` so a
            // union on the left is not mistaken for a single member of the right.
            (NormalTy::Union(members), _) => members
                .iter()
                .all(|m| m.is_subtype_of(sup, ctx, assumptions)),
            (_, NormalTy::Union(members)) => members
                .iter()
                .any(|m| self.is_subtype_of(m, ctx, assumptions)),

            // A type variable is a subtype of `sup` if its bound is. (Same-var
            // reflexivity and `T <: T | U` are handled by the rules above.)
            (NormalTy::TypeVar(name), _) => ctx
                .type_var_bound(name)
                .is_some_and(|bound| canonical(&bound, ctx).is_subtype_of(sup, ctx, assumptions)),

            // Concrete (or any non-interface) type implementing an interface.
            (sub, NormalTy::Interface(..)) if !matches!(sub, NormalTy::Interface(..)) => {
                ctx.implements_interface(&sub.clone().into_ty(), &sup.clone().into_ty())
            }
            // Interface-to-interface: `A <: B` iff `A` requires `B`.
            (NormalTy::Interface(..), NormalTy::Interface(..)) => {
                ctx.interface_requires(&self.clone().into_ty(), &sup.clone().into_ty())
            }

            // Same-class generic arguments are invariant. A "hole" (an inference
            // placeholder) on either side matches anything; otherwise the
            // arguments must be mutual subtypes (the definition of invariant
            // compatibility, of which structural equality is the special case).
            (NormalTy::Class(q1, a1), NormalTy::Class(q2, a2))
                if q1 == q2 && a1.len() == a2.len() =>
            {
                a1.iter().zip(a2.iter()).all(|(a, b)| {
                    is_hole(a)
                        || is_hole(b)
                        || (a.is_subtype_of(b, ctx, assumptions)
                            && b.is_subtype_of(a, ctx, assumptions))
                })
            }

            // List/Map are invariant structurally (no element coercion); `never`
            // and literal-key widening flow through the recursive checks.
            (NormalTy::List(a), NormalTy::List(b)) => a.is_subtype_of(b, ctx, assumptions),
            (NormalTy::Map { key: k1, value: v1 }, NormalTy::Map { key: k2, value: v2 }) => {
                k1.is_subtype_of(k2, ctx, assumptions) && v1.is_subtype_of(v2, ctx, assumptions)
            }

            // Future/WatchAccessor are invariant containers.
            (NormalTy::Future(v1, e1), NormalTy::Future(v2, e2)) => {
                v1.is_subtype_of(v2, ctx, assumptions)
                    && v2.is_subtype_of(v1, ctx, assumptions)
                    && e1.is_subtype_of(e2, ctx, assumptions)
                    && e2.is_subtype_of(e1, ctx, assumptions)
            }
            (NormalTy::WatchAccessor(a), NormalTy::WatchAccessor(b)) => {
                a.is_subtype_of(b, ctx, assumptions) && b.is_subtype_of(a, ctx, assumptions)
            }

            // Literal types are subtypes of their (same-representation) base only.
            (NormalTy::Literal(Literal::Int(_)), NormalTy::Int) => true,
            (NormalTy::Literal(Literal::Bigint(_)), NormalTy::Bigint) => true,
            (NormalTy::Literal(Literal::Float(_)), NormalTy::Float) => true,
            (NormalTy::Literal(Literal::String(_)), NormalTy::String) => true,
            (NormalTy::Literal(Literal::Bool(_)), NormalTy::Bool) => true,

            // A variant is a subtype of its enum.
            (NormalTy::EnumVariant(e, _), NormalTy::Enum(sup_e)) => e == sup_e,

            // Function subtyping: contravariant params, covariant return/throws.
            (
                NormalTy::Function {
                    params: p1,
                    ret: r1,
                    throws: t1,
                },
                NormalTy::Function {
                    params: p2,
                    ret: r2,
                    throws: t2,
                },
            ) => {
                r1.is_subtype_of(r2, ctx, assumptions)
                    && t1.is_subtype_of(t2, ctx, assumptions)
                    && params_subtype(p1, p2, ctx, assumptions)
            }

            _ => false,
        };
        assumptions.remove(&pair);
        result
    }

    // ── conversion out: NormalTy → Ty ──────────────────────────────────────

    fn into_ty(self) -> Ty {
        let attr = TyAttr::default();
        match self {
            NormalTy::Int => Ty::Int { attr },
            NormalTy::Bigint => Ty::Bigint { attr },
            NormalTy::Float => Ty::Float { attr },
            NormalTy::String => Ty::String { attr },
            NormalTy::Bool => Ty::Bool { attr },
            NormalTy::Null => Ty::Null { attr },
            NormalTy::Uint8Array => Ty::Uint8Array { attr },
            NormalTy::Media(kind) => Ty::Media(kind, attr),
            NormalTy::Void => Ty::Void { attr },
            NormalTy::RustType => Ty::RustType { attr },
            NormalTy::Type => Ty::Type { attr },
            NormalTy::Resource => Ty::Resource { attr },
            NormalTy::PromptAst => Ty::PromptAst { attr },
            NormalTy::BuiltinUnknown => Ty::BuiltinUnknown { attr },
            NormalTy::Never => Ty::Never { attr },
            NormalTy::Unknown => Ty::Unknown { attr },
            NormalTy::Error => Ty::Error { attr },
            NormalTy::Literal(lit) => Ty::Literal(lit, crate::Freshness::Regular, attr),
            NormalTy::Class(qn, args) => Ty::Class(qn, into_tys(args), attr),
            NormalTy::Interface(qn, args, bindings) => Ty::Interface(
                qn,
                into_tys(args),
                bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.into_ty()))
                    .collect(),
                attr,
            ),
            NormalTy::Enum(qn) => Ty::Enum(qn, attr),
            NormalTy::EnumVariant(qn, v) => Ty::EnumVariant(qn, v, attr),
            NormalTy::List(inner) => Ty::List(Box::new(inner.into_ty()), attr),
            NormalTy::Map { key, value } => Ty::Map {
                key: Box::new(key.into_ty()),
                value: Box::new(value.into_ty()),
                attr,
            },
            NormalTy::Union(members) => Ty::Union(into_tys(members), attr),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => Ty::Function {
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                params: params
                    .into_iter()
                    .map(|p| FunctionParamTy {
                        name: p.name,
                        ty: p.ty.into_ty(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.into_ty()),
                throws: Box::new(throws.into_ty()),
                attr,
            },
            NormalTy::Future(value, error) => {
                Ty::Future(Box::new(value.into_ty()), Box::new(error.into_ty()), attr)
            }
            NormalTy::WatchAccessor(inner) => Ty::WatchAccessor(Box::new(inner.into_ty()), attr),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(base.into_ty()),
                interface: interface.map(|i| Box::new(i.into_ty())),
                member,
                attr,
            },
            NormalTy::TypeVar(name) => Ty::TypeVar(name, attr),
            // μ-binders and recursion variables round-trip through the alias name.
            NormalTy::Mu { body, .. } => body.into_ty(),
            NormalTy::RecVar(qn) | NormalTy::OpaqueAlias(qn) => Ty::TypeAlias(qn, attr),
        }
    }
}

fn into_tys(tys: Vec<NormalTy>) -> Vec<Ty> {
    tys.into_iter().map(NormalTy::into_ty).collect()
}

/// Inference placeholders that match any type in invariant positions.
fn is_hole(ty: &NormalTy) -> bool {
    matches!(ty, NormalTy::Unknown | NormalTy::BuiltinUnknown)
}

/// Error-recovery sentinels excluded from union absorption (they would otherwise
/// swallow real members during error recovery and fabricate equivalences).
fn is_sentinel(ty: &NormalTy) -> bool {
    matches!(ty, NormalTy::Unknown | NormalTy::Error)
}

// ═══════════════════════════════════════════════════════════════════════════
// UNION CANONICALIZATION
// ═══════════════════════════════════════════════════════════════════════════

/// Reduce a union of already-canonical members to canonical form: flatten,
/// remove `never`, absorb under `unknown`, collapse complete enums, absorb
/// subtype-members, then sort/dedup and unwrap singletons.
fn canonicalize_union<C: TypeContext>(members: Vec<NormalTy>, ctx: &C) -> NormalTy {
    // Flatten one level (members are canonical, but a μ-unfold or alias could
    // surface a nested union) and drop `never`.
    let mut flat: Vec<NormalTy> = Vec::new();
    for m in members {
        match m {
            NormalTy::Union(inner) => flat.extend(inner),
            NormalTy::Never => {}
            other => flat.push(other),
        }
    }

    // `unknown` (top) absorbs everything.
    if flat.iter().any(|m| matches!(m, NormalTy::BuiltinUnknown)) {
        return NormalTy::BuiltinUnknown;
    }

    flat.sort();
    flat.dedup();

    collapse_complete_enums(&mut flat, ctx);
    let mut flat = absorb_subtypes(&flat, ctx);

    flat.sort();
    flat.dedup();
    match flat.len() {
        0 => NormalTy::Never,
        1 => flat.pop().unwrap_or_else(|| unreachable!("len checked")),
        _ => NormalTy::Union(flat),
    }
}

/// Replace a complete set of an enum's variants with the enum itself
/// (`E.A | E.B | … == E`). A bare `Enum(E)` already present absorbs its variants
/// via the subtype pass, so this only handles the all-variants-no-enum case.
fn collapse_complete_enums<C: TypeContext>(members: &mut Vec<NormalTy>, ctx: &C) {
    // Distinct enums that have at least one variant present.
    let mut enums: Vec<QualifiedTypeName> = members
        .iter()
        .filter_map(|m| match m {
            NormalTy::EnumVariant(e, _) => Some(e.clone()),
            _ => None,
        })
        .collect();
    enums.sort();
    enums.dedup();

    for e in enums {
        let Some(all) = ctx.enum_variants(&e) else {
            continue; // unknown enum → no collapse (fail-safe)
        };
        let present: HashSet<&Name> = members
            .iter()
            .filter_map(|m| match m {
                NormalTy::EnumVariant(en, v) if *en == e => Some(v),
                _ => None,
            })
            .collect();
        if !all.is_empty() && all.iter().all(|v| present.contains(v)) {
            members.retain(|m| !matches!(m, NormalTy::EnumVariant(en, _) if *en == e));
            members.push(NormalTy::Enum(e));
        }
    }
}

/// Remove any member subsumed by another (`X | Y == Y` when `X <: Y`). Covers
/// literal-into-base, variant-into-enum, `C | I == I`, `A | B == B`, and
/// `T | I == I`. Error-recovery sentinels never absorb or are absorbed.
fn absorb_subtypes<C: TypeContext>(members: &[NormalTy], ctx: &C) -> Vec<NormalTy> {
    let n = members.len();
    let mut keep = vec![true; n];
    for i in 0..n {
        if is_sentinel(&members[i]) {
            continue;
        }
        for j in 0..n {
            if i == j || !keep[j] || is_sentinel(&members[j]) {
                continue;
            }
            if !members[i].is_subtype_of(&members[j], ctx, &mut HashSet::new()) {
                continue;
            }
            // `members[i] <: members[j]`. Drop `i`, unless they are mutual
            // subtypes (equivalent but not structurally equal — e.g. cyclic
            // `requires`); then keep the lower index deterministically.
            let mutual = members[j].is_subtype_of(&members[i], ctx, &mut HashSet::new());
            if !mutual || j < i {
                keep[i] = false;
                break;
            }
        }
    }
    (0..n)
        .filter(|&i| keep[i])
        .map(|i| members[i].clone())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBSTITUTION (μ-unfolding) & FUNCTION PARAMETERS
// ═══════════════════════════════════════════════════════════════════════════

/// Substitute recursion variable `var` with `replacement` (one μ-unfold step).
fn substitute(ty: &NormalTy, var: &QualifiedTypeName, replacement: &NormalTy) -> NormalTy {
    match ty {
        NormalTy::RecVar(v) if v == var => replacement.clone(),
        NormalTy::Class(qn, args) => NormalTy::Class(
            qn.clone(),
            args.iter()
                .map(|a| substitute(a, var, replacement))
                .collect(),
        ),
        NormalTy::Interface(qn, args, bindings) => NormalTy::Interface(
            qn.clone(),
            args.iter()
                .map(|a| substitute(a, var, replacement))
                .collect(),
            bindings
                .iter()
                .map(|(n, t)| (n.clone(), substitute(t, var, replacement)))
                .collect(),
        ),
        NormalTy::List(inner) => NormalTy::List(Box::new(substitute(inner, var, replacement))),
        NormalTy::WatchAccessor(inner) => {
            NormalTy::WatchAccessor(Box::new(substitute(inner, var, replacement)))
        }
        NormalTy::Map { key, value } => NormalTy::Map {
            key: Box::new(substitute(key, var, replacement)),
            value: Box::new(substitute(value, var, replacement)),
        },
        NormalTy::Union(members) => NormalTy::Union(
            members
                .iter()
                .map(|m| substitute(m, var, replacement))
                .collect(),
        ),
        NormalTy::Function {
            params,
            ret,
            throws,
        } => NormalTy::Function {
            params: params
                .iter()
                .map(|p| NormalParam {
                    name: p.name.clone(),
                    ty: substitute(&p.ty, var, replacement),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(substitute(ret, var, replacement)),
            throws: Box::new(substitute(throws, var, replacement)),
        },
        NormalTy::Future(value, error) => NormalTy::Future(
            Box::new(substitute(value, var, replacement)),
            Box::new(substitute(error, var, replacement)),
        ),
        NormalTy::AssociatedTypeProjection {
            base,
            interface,
            member,
        } => NormalTy::AssociatedTypeProjection {
            base: Box::new(substitute(base, var, replacement)),
            interface: interface
                .as_ref()
                .map(|i| Box::new(substitute(i, var, replacement))),
            member: member.clone(),
        },
        // A nested μ binding the same name shadows it; do not substitute inside.
        NormalTy::Mu { var: v, body } if v != var => NormalTy::Mu {
            var: v.clone(),
            body: Box::new(substitute(body, var, replacement)),
        },
        _ => ty.clone(),
    }
}

/// Function parameter-list subtyping (contravariant): required params positional
/// and matched in order, optional params matched by name.
fn params_subtype<C: TypeContext>(
    sub: &[NormalParam],
    sup: &[NormalParam],
    ctx: &C,
    assumptions: &mut HashSet<(NormalTy, NormalTy)>,
) -> bool {
    let sub_required: Vec<&NormalParam> = sub.iter().filter(|p| is_required(p)).collect();
    let sup_required: Vec<&NormalParam> = sup.iter().filter(|p| is_required(p)).collect();
    if sub_required.len() != sup_required.len() {
        return false;
    }
    for (sub, sup) in sub_required.iter().zip(sup_required.iter()) {
        if !sup.ty.is_subtype_of(&sub.ty, ctx, assumptions) {
            return false;
        }
    }
    for sup in sup.iter().filter(|p| !is_required(p)) {
        let Some(name) = &sup.name else {
            return false;
        };
        let Some(sub) = sub
            .iter()
            .find(|p| !is_required(p) && p.name.as_ref() == Some(name))
        else {
            return false;
        };
        if !sup.ty.is_subtype_of(&sub.ty, ctx, assumptions) {
            return false;
        }
    }
    true
}

fn is_required(p: &NormalParam) -> bool {
    matches!(p.mode, FunctionParamMode::Required)
}

#[cfg(test)]
mod tests;
