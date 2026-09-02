//! Recursive hash-consed type representation - the planned future form of
//! [`Ty`](crate::Ty), introduced as a sibling so the existing enum (and every
//! downstream consumer) is untouched until the hir_ty cutover.
//!
//! Design (mirrors rust-analyzer's interned `Ty`, see
//! `baml_compiler2_hir_ty/README.md` for the roadmap):
//!
//! - [`Ty`] is a one-word handle into a global hash-cons pool. `Clone` is a
//!   refcount bump, `==` is a pointer compare, and `Hash` hashes the pointer.
//!   The pool guarantees structurally-identical kinds share one allocation,
//!   so pointer identity IS structural identity.
//! - Children of [`InferTy`] are handles, so interning is recursive: pool
//!   lookups hash and compare shallowly (child pointers, not child trees),
//!   and substructure is shared automatically.
//! - [`TypeFlags`] ("does this contain an inference variable / error /
//!   type variable...") are computed once at intern time and answered in
//!   O(1), which is what lets inference fold/resolve loops short-circuit.
//! - The pool is a global mutex-guarded set, NOT salsa: unlike compiler-local
//!   ids (`FunctionLoc`, `ScopeId`), types must outlive any database - they
//!   are held by the runtime, serialized, and crossed over FFI. This is the
//!   same reason rust-analyzer interns its types in a global pool despite
//!   being salsa-based throughout. Entries are freed when the last handle
//!   drops.
//!
//! The pool's kind ([`InferTy`]) and its twin satellites are *generated* by
//! [`ty_family!`](crate::Ty) (the `child: interned(..)` member in
//! `family.rs`): the finalized axes plus `infer`, children as handles, one
//! source of truth with the plain members. The spec-driven delta lives in the
//! axes: [`InferTy::InferVar`] carries an [`InferVar`], ALWAYS — the
//! syntactic `_` hole is unrepresentable interned (it lives on the `lower`
//! axis, in [`crate::LoweringTy`], and lowering never interns), so the
//! inference world is vars-only by construction. The hand-written pieces in
//! this module — pool, flags, child walkers, boundary conversions, and the
//! twins' constructor impls — attach to those generated types; the walkers
//! are exhaustive matches, so variant drift is a compile error.

use std::{
    cmp::Ordering,
    collections::HashSet,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use crate::{Freshness, FunctionParamMode, Name, TyAttr, TypeName};

// -- Flags --------------------------------------------------------------------

bitflags::bitflags! {
    /// Facts about a type computed once at intern time; the union of each
    /// node's own bit and all its children's flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct TypeFlags: u16 {
        /// Contains an `Infer` node (a live inference-table variable).
        const HAS_INFER = 1 << 0;
        /// Contains an `Error` sentinel.
        const HAS_ERROR = 1 << 1;
        /// Contains a named `TypeVar`.
        const HAS_TYPEVAR = 1 << 2;
        /// Contains an unresolved associated-type projection.
        const HAS_PROJECTION = 1 << 3;
        /// Contains a fresh (unwidened) literal.
        const HAS_FRESH_LITERAL = 1 << 4;
    }
}

// -- Handle -------------------------------------------------------------------

/// An inference-table variable index. Only ever allocated by the hir_ty
/// inference table and carried by [`InferTy::InferVar`] (the syntactic `_`
/// hole is unrepresentable interned: it lives in [`crate::LoweringTy`]).
///
/// Deliberately a bare index: the representation carries variable IDENTITY,
/// the inference table carries variable KIND. Distinctions like effect vars
/// (throws slots defaulting to `never`), diverging vars, and canonical
/// placeholders are table-side policy metadata keyed by this index -
/// rust-analyzer's `diverging_type_vars` side-set pattern - not new `InferTy`
/// variants. rustc needs kind-in-type (`IntVar`/`FloatVar`) only because
/// numeric literal defaulting changes structural unification; BAML has no
/// such rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InferVar(u32);

impl InferVar {
    pub fn new(index: u32) -> InferVar {
        InferVar(index)
    }

    pub fn index(self) -> u32 {
        self.0
    }
}

/// A hash-consed type: a one-word handle whose pointer identity is
/// structural identity. Construct via [`Ty::intern`] or the leaf helpers.
pub struct Ty(Arc<TyData>);

#[derive(PartialEq, Eq, Hash)]
struct TyData {
    flags: TypeFlags,
    kind: InferTy,
}

impl Ty {
    /// Interns `kind`, returning the unique handle for it. Flags are computed
    /// here; there is no other way to construct a `Ty`.
    pub fn intern(kind: InferTy) -> Ty {
        let data = TyData {
            flags: compute_flags(&kind),
            kind,
        };
        let mut pool = pool().lock().expect("ty intern pool poisoned");
        if let Some(existing) = pool.get(&data) {
            return Ty(Arc::clone(existing));
        }
        let arc = Arc::new(data);
        pool.insert(Arc::clone(&arc));
        Ty(arc)
    }

    pub fn kind(&self) -> &InferTy {
        &self.0.kind
    }

    pub fn flags(&self) -> TypeFlags {
        self.0.flags
    }

    /// Whether this type still contains inference variables.
    pub fn has_infer(&self) -> bool {
        self.0.flags.contains(TypeFlags::HAS_INFER)
    }

    /// Whether this type contains an `Error` sentinel.
    pub fn has_error(&self) -> bool {
        self.0.flags.contains(TypeFlags::HAS_ERROR)
    }

    /// Whether this type contains a rigid type variable.
    pub fn has_typevar(&self) -> bool {
        self.0.flags.contains(TypeFlags::HAS_TYPEVAR)
    }

    /// Whether this type contains an associated-type projection.
    pub fn has_projection(&self) -> bool {
        self.0.flags.contains(TypeFlags::HAS_PROJECTION)
    }
}

impl Clone for Ty {
    fn clone(&self) -> Ty {
        Ty(Arc::clone(&self.0))
    }
}

impl Drop for Ty {
    fn drop(&mut self) {
        // Evict when only this handle and the pool's entry remain. The count
        // is re-checked under the lock: a concurrent intern of the same kind
        // bumps the count before we can remove it, and a count of 2 under the
        // lock proves no other handle exists. The removed entry's `TyData` is
        // freed after the guard is released (this handle still holds it), so
        // child handles' recursive drops never run under the pool lock.
        if Arc::strong_count(&self.0) == 2 {
            let mut pool = pool().lock().expect("ty intern pool poisoned");
            if Arc::strong_count(&self.0) == 2 {
                pool.remove(&*self.0);
            }
        }
    }
}

/// Pointer equality; sound because the pool guarantees structurally equal
/// kinds share one allocation.
impl PartialEq for Ty {
    fn eq(&self, other: &Ty) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Ty {}

/// Pointer hash, consistent with `Eq` (and with structural identity, by the
/// pool invariant).
impl Hash for Ty {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// Structural, deterministic ordering (pointer order would vary run to run;
/// canonical union sorting needs stability). Fast path on pointer identity.
impl Ord for Ty {
    fn cmp(&self, other: &Ty) -> Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            return Ordering::Equal;
        }
        self.kind().cmp(other.kind())
    }
}
impl PartialOrd for Ty {
    fn partial_cmp(&self, other: &Ty) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Debug for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind().fmt(f)
    }
}

fn pool() -> &'static Mutex<HashSet<Arc<TyData>>> {
    static POOL: OnceLock<Mutex<HashSet<Arc<TyData>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashSet::new()))
}

// -- Kind ---------------------------------------------------------------------

/// The pool's kind and twin satellites, generated by `ty_family!` (the
/// `InferTy` member declaration in `family.rs`). Re-exported here so the
/// interned world's vocabulary lives in one namespace with the pool; the
/// hand-written pieces below attach to the generated types.
pub use crate::{InferFunctionParamTy, InferInterface, InferTy};

impl InferFunctionParamTy {
    pub fn required(name: Option<Name>, ty: Ty) -> InferFunctionParamTy {
        InferFunctionParamTy {
            name,
            ty,
            mode: FunctionParamMode::Required,
        }
    }

    pub fn optional(name: Option<Name>, ty: Ty) -> InferFunctionParamTy {
        InferFunctionParamTy {
            name,
            ty,
            mode: FunctionParamMode::Optional,
        }
    }
}

impl InferInterface {
    /// Sorts `associated_types` by name, mirroring the plain satellite's
    /// order-insensitivity invariant (and `Interface::new`'s signature —
    /// slices sort in place, no `Vec` needed).
    pub fn new(
        name: TypeName,
        generics: Box<[Ty]>,
        mut associated_types: Box<[(Name, Ty)]>,
    ) -> InferInterface {
        associated_types.sort_by(|(a, _), (b, _)| a.cmp(b));
        InferInterface {
            name,
            generics,
            associated_types,
        }
    }

    /// The interface reference an existential type carries, when `ty`
    /// is one - the single extraction every consumer shares (rustc has
    /// exactly one `TraitRef`; nobody hand-builds a parallel copy).
    pub fn of_ty(ty: &Ty) -> Option<InferInterface> {
        match ty.kind() {
            InferTy::Interface(name, args, pins, _) => Some(InferInterface::new(
                name.clone(),
                args.clone(),
                pins.clone(),
            )),
            _ => None,
        }
    }

    /// From the plain algebra's constraint satellite (the `TypeContext`
    /// boundary).
    pub fn from_constraint(interface: &crate::Interface) -> InferInterface {
        InferInterface::new(
            interface.name.clone(),
            interface
                .generics
                .iter()
                .map(Ty::from_plain)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            interface
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                .collect(),
        )
    }

    /// The existential type this reference denotes (`dyn`-style view).
    pub fn existential(&self) -> Ty {
        Ty::intern(InferTy::Interface(
            self.name.clone(),
            self.generics.clone(),
            self.associated_types.clone(),
            TyAttr::default(),
        ))
    }
}

// -- Flag computation ---------------------------------------------------------

/// Calls `visit` on each direct child type of `kind` (including types nested
/// in satellites: function params, interface generics and bindings).
pub fn for_each_child(kind: &InferTy, mut visit: impl FnMut(&Ty)) {
    match kind {
        InferTy::Int { .. }
        | InferTy::Bigint { .. }
        | InferTy::Float { .. }
        | InferTy::String { .. }
        | InferTy::Bool { .. }
        | InferTy::Null { .. }
        | InferTy::Uint8Array { .. }
        | InferTy::Media(..)
        | InferTy::Literal(..)
        | InferTy::Enum(..)
        | InferTy::EnumVariant(..)
        | InferTy::RustType { .. }
        | InferTy::Type { .. }
        | InferTy::Resource { .. }
        | InferTy::PromptAst { .. }
        | InferTy::Void { .. }
        | InferTy::TypeAlias(..)
        | InferTy::TypeVar(..)
        | InferTy::Unknown { .. }
        | InferTy::Never { .. }
        | InferTy::Error { .. }
        | InferTy::InferVar { .. } => {}
        InferTy::Class(_, args, _) => args.iter().for_each(visit),
        InferTy::Interface(_, args, assoc, _) => {
            args.iter().for_each(&mut visit);
            assoc.iter().for_each(|(_, ty)| visit(ty));
        }
        InferTy::List(inner, _) => visit(inner),
        InferTy::Map { key, value, .. } => {
            visit(key);
            visit(value);
        }
        InferTy::Union(members, _) => members.iter().for_each(visit),
        InferTy::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().for_each(|param| visit(&param.ty));
            visit(ret);
            visit(throws);
        }
        InferTy::Future(value, error, _) => {
            visit(value);
            visit(error);
        }
        InferTy::AssociatedTypeProjection {
            base, interface, ..
        } => {
            visit(base);
            interface.generics.iter().for_each(&mut visit);
            interface
                .associated_types
                .iter()
                .for_each(|(_, ty)| visit(ty));
        }
    }
}

impl InferTy {
    /// Rebuilds this kind with every direct child type replaced by
    /// `f(child)` (satellite-nested children included) - the rebuild dual of
    /// [`for_each_child`]. Leaf kinds clone unchanged. Callers intern the
    /// result; short-circuit on [`Ty::flags`] first when the fold cannot
    /// apply (e.g. no `HAS_INFER`).
    pub fn map_children(&self, mut f: impl FnMut(&Ty) -> Ty) -> InferTy {
        match self {
            InferTy::Int { .. }
            | InferTy::Bigint { .. }
            | InferTy::Float { .. }
            | InferTy::String { .. }
            | InferTy::Bool { .. }
            | InferTy::Null { .. }
            | InferTy::Uint8Array { .. }
            | InferTy::Media(..)
            | InferTy::Literal(..)
            | InferTy::Enum(..)
            | InferTy::EnumVariant(..)
            | InferTy::RustType { .. }
            | InferTy::Type { .. }
            | InferTy::Resource { .. }
            | InferTy::PromptAst { .. }
            | InferTy::Void { .. }
            | InferTy::TypeAlias(..)
            | InferTy::TypeVar(..)
            | InferTy::Unknown { .. }
            | InferTy::Never { .. }
            | InferTy::Error { .. }
            | InferTy::InferVar { .. } => self.clone(),
            InferTy::Class(name, args, attr) => InferTy::Class(
                name.clone(),
                args.iter().map(&mut f).collect(),
                attr.clone(),
            ),
            InferTy::Interface(name, args, assoc, attr) => InferTy::Interface(
                name.clone(),
                args.iter().map(&mut f).collect(),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), f(ty)))
                    .collect(),
                attr.clone(),
            ),
            InferTy::List(inner, attr) => InferTy::List(f(inner), attr.clone()),
            InferTy::Map { key, value, attr } => InferTy::Map {
                key: f(key),
                value: f(value),
                attr: attr.clone(),
            },
            InferTy::Union(members, attr) => {
                InferTy::Union(members.iter().map(&mut f).collect(), attr.clone())
            }
            InferTy::Function {
                params,
                ret,
                throws,
                attr,
            } => InferTy::Function {
                params: params
                    .iter()
                    .map(|param| InferFunctionParamTy {
                        name: param.name.clone(),
                        ty: f(&param.ty),
                        mode: param.mode,
                    })
                    .collect(),
                ret: f(ret),
                throws: f(throws),
                attr: attr.clone(),
            },
            InferTy::Future(value, error, attr) => {
                InferTy::Future(f(value), f(error), attr.clone())
            }
            InferTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => InferTy::AssociatedTypeProjection {
                base: f(base),
                interface: InferInterface {
                    name: interface.name.clone(),
                    generics: interface.generics.iter().map(&mut f).collect(),
                    associated_types: interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), f(ty)))
                        .collect(),
                },
                member: member.clone(),
                attr: attr.clone(),
            },
        }
    }
}

fn compute_flags(kind: &InferTy) -> TypeFlags {
    let own = match kind {
        InferTy::Literal(_, Freshness::Fresh, _) => TypeFlags::HAS_FRESH_LITERAL,
        InferTy::TypeVar(..) => TypeFlags::HAS_TYPEVAR,
        InferTy::AssociatedTypeProjection { .. } => TypeFlags::HAS_PROJECTION,
        InferTy::Error { .. } => TypeFlags::HAS_ERROR,
        InferTy::InferVar { .. } => TypeFlags::HAS_INFER,
        _ => TypeFlags::empty(),
    };
    let mut flags = own;
    for_each_child(kind, |ty| flags |= ty.flags());
    flags
}

// -- Conversions --------------------------------------------------------------

impl Ty {
    /// Interns the plain enum's structure. `Infer` becomes the var-less hole.
    ///
    pub fn from_plain(ty: &crate::Ty) -> Ty {
        let interned_all =
            |tys: &[crate::Ty]| -> Box<[Ty]> { tys.iter().map(Ty::from_plain).collect() };
        let kind = match ty {
            crate::Ty::Int { attr } => InferTy::Int { attr: attr.clone() },
            crate::Ty::Bigint { attr } => InferTy::Bigint { attr: attr.clone() },
            crate::Ty::Float { attr } => InferTy::Float { attr: attr.clone() },
            crate::Ty::String { attr } => InferTy::String { attr: attr.clone() },
            crate::Ty::Bool { attr } => InferTy::Bool { attr: attr.clone() },
            crate::Ty::Null { attr } => InferTy::Null { attr: attr.clone() },
            crate::Ty::Uint8Array { attr } => InferTy::Uint8Array { attr: attr.clone() },
            crate::Ty::Media(kind, attr) => InferTy::Media(*kind, attr.clone()),
            crate::Ty::Literal(lit, freshness, attr) => {
                InferTy::Literal(lit.clone(), *freshness, attr.clone())
            }
            crate::Ty::Class(name, args, attr) => {
                InferTy::Class(name.clone(), interned_all(args), attr.clone())
            }
            crate::Ty::Interface(name, args, assoc, attr) => InferTy::Interface(
                name.clone(),
                interned_all(args),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                    .collect(),
                attr.clone(),
            ),
            crate::Ty::Enum(name, attr) => InferTy::Enum(name.clone(), attr.clone()),
            crate::Ty::EnumVariant(name, variant, attr) => {
                InferTy::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            crate::Ty::List(inner, attr) => InferTy::List(Ty::from_plain(inner), attr.clone()),
            crate::Ty::Map { key, value, attr } => InferTy::Map {
                key: Ty::from_plain(key),
                value: Ty::from_plain(value),
                attr: attr.clone(),
            },
            crate::Ty::Union(members, attr) => InferTy::Union(interned_all(members), attr.clone()),
            crate::Ty::Function {
                params,
                ret,
                throws,
                attr,
            } => InferTy::Function {
                params: params
                    .iter()
                    .map(|param| InferFunctionParamTy {
                        name: param.name.clone(),
                        ty: Ty::from_plain(&param.ty),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Ty::from_plain(ret),
                throws: Ty::from_plain(throws),
                attr: attr.clone(),
            },
            crate::Ty::Future(value, error, attr) => {
                InferTy::Future(Ty::from_plain(value), Ty::from_plain(error), attr.clone())
            }
            crate::Ty::RustType { attr } => InferTy::RustType { attr: attr.clone() },
            crate::Ty::Type { attr } => InferTy::Type { attr: attr.clone() },
            crate::Ty::Resource { attr } => InferTy::Resource { attr: attr.clone() },
            crate::Ty::PromptAst { attr } => InferTy::PromptAst { attr: attr.clone() },
            crate::Ty::Void { attr } => InferTy::Void { attr: attr.clone() },
            crate::Ty::TypeAlias(name, attr) => InferTy::TypeAlias(name.clone(), attr.clone()),
            crate::Ty::TypeVar(param, attr) => InferTy::TypeVar(param.clone(), attr.clone()),
            crate::Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => InferTy::AssociatedTypeProjection {
                base: Ty::from_plain(base),
                interface: InferInterface::new(
                    interface.name.clone(),
                    interned_all(&interface.generics),
                    interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                        .collect(),
                ),
                member: member.clone(),
                attr: attr.clone(),
            },
            crate::Ty::Unknown { attr } => InferTy::Unknown { attr: attr.clone() },
            crate::Ty::Never { attr } => InferTy::Never { attr: attr.clone() },
            crate::Ty::Error { attr } => InferTy::Error { attr: attr.clone() },
        };
        Ty::intern(kind)
    }

    /// Whether every node in this tree is free of live inference
    /// variables — the [`ClosedTy`] invariant, answered in O(1) by the
    /// cached flags.
    pub fn is_closed(&self) -> bool {
        !self.has_infer()
    }

    /// The walk behind [`ClosedTy::to_plain`]. PRIVATE: only the checked
    /// newtype can start it, so an unproven (possibly open) `Ty` can never
    /// reach the conversion; recursion stays on raw children because the
    /// pool's flags are subtree unions — a closed root has no open child.
    fn to_plain_closed(&self) -> crate::Ty {
        let plain_all =
            |tys: &[Ty]| -> Box<[crate::Ty]> { tys.iter().map(Ty::to_plain_closed).collect() };
        match self.kind() {
            InferTy::Int { attr } => crate::Ty::Int { attr: attr.clone() },
            InferTy::Bigint { attr } => crate::Ty::Bigint { attr: attr.clone() },
            InferTy::Float { attr } => crate::Ty::Float { attr: attr.clone() },
            InferTy::String { attr } => crate::Ty::String { attr: attr.clone() },
            InferTy::Bool { attr } => crate::Ty::Bool { attr: attr.clone() },
            InferTy::Null { attr } => crate::Ty::Null { attr: attr.clone() },
            InferTy::Uint8Array { attr } => crate::Ty::Uint8Array { attr: attr.clone() },
            InferTy::Media(kind, attr) => crate::Ty::Media(*kind, attr.clone()),
            InferTy::Literal(lit, freshness, attr) => {
                crate::Ty::Literal(lit.clone(), *freshness, attr.clone())
            }
            InferTy::Class(name, args, attr) => {
                crate::Ty::Class(name.clone(), plain_all(args), attr.clone())
            }
            InferTy::Interface(name, args, assoc, attr) => crate::Ty::Interface(
                name.clone(),
                plain_all(args),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.to_plain_closed()))
                    .collect(),
                attr.clone(),
            ),
            InferTy::Enum(name, attr) => crate::Ty::Enum(name.clone(), attr.clone()),
            InferTy::EnumVariant(name, variant, attr) => {
                crate::Ty::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            InferTy::List(inner, attr) => {
                crate::Ty::List(Box::new(inner.to_plain_closed()), attr.clone())
            }
            InferTy::Map { key, value, attr } => crate::Ty::Map {
                key: Box::new(key.to_plain_closed()),
                value: Box::new(value.to_plain_closed()),
                attr: attr.clone(),
            },
            InferTy::Union(members, attr) => crate::Ty::Union(plain_all(members), attr.clone()),
            InferTy::Function {
                params,
                ret,
                throws,
                attr,
            } => crate::Ty::Function {
                params: params
                    .iter()
                    .map(|param| crate::FunctionParamTy {
                        name: param.name.clone(),
                        ty: param.ty.to_plain_closed(),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(ret.to_plain_closed()),
                throws: Box::new(throws.to_plain_closed()),
                attr: attr.clone(),
            },
            InferTy::Future(value, error, attr) => crate::Ty::Future(
                Box::new(value.to_plain_closed()),
                Box::new(error.to_plain_closed()),
                attr.clone(),
            ),
            InferTy::RustType { attr } => crate::Ty::RustType { attr: attr.clone() },
            InferTy::Type { attr } => crate::Ty::Type { attr: attr.clone() },
            InferTy::Resource { attr } => crate::Ty::Resource { attr: attr.clone() },
            InferTy::PromptAst { attr } => crate::Ty::PromptAst { attr: attr.clone() },
            InferTy::Void { attr } => crate::Ty::Void { attr: attr.clone() },
            InferTy::TypeAlias(name, attr) => crate::Ty::TypeAlias(name.clone(), attr.clone()),
            InferTy::TypeVar(param, attr) => crate::Ty::TypeVar(param.clone(), attr.clone()),
            InferTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => crate::Ty::AssociatedTypeProjection {
                base: Box::new(base.to_plain_closed()),
                interface: Box::new(crate::Interface::new(
                    interface.name.clone(),
                    plain_all(&interface.generics),
                    interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), ty.to_plain_closed()))
                        .collect(),
                )),
                member: member.clone(),
                attr: attr.clone(),
            },
            InferTy::Unknown { attr } => crate::Ty::Unknown { attr: attr.clone() },
            InferTy::Never { attr } => crate::Ty::Never { attr: attr.clone() },
            InferTy::Error { attr } => crate::Ty::Error { attr: attr.clone() },
            // Unreachable BY INVARIANT: `ClosedTy` construction checked the
            // cached HAS_INFER flag over the whole tree.
            InferTy::InferVar { .. } => {
                unreachable!("ClosedTy invariant: no live inference variables")
            }
        }
    }
}

/// An interned type PROVEN free of live inference variables — the only
/// vocabulary that can leave the interned world.
///
/// The interned→plain conversion ([`ClosedTy::to_plain`]) is reachable
/// through this newtype and nowhere else, so "a live variable never
/// converts" is a type invariant with checked entry points, not a
/// call-site discipline. The [`TryFrom`] constructor costs one cached-flag
/// test; its `Err` forces every boundary that can meet an open type to
/// pick an explicit disposition — defer (relation oracles), suppress
/// (exhaustiveness columns), rename-for-rendering (diagnostic payloads),
/// or dispose (inference finalize, which diagnoses and substitutes the
/// Error sentinel). An `Error` minted anywhere else would be unsound: its
/// always-compatible algebra is justified only by an already-emitted
/// fatal diagnostic, and an erased-but-legal variable has none.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosedTy(Ty);

/// The type still contains a live inference variable; the boundary must
/// defer, suppress, rename, or dispose instead of converting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenTy;

impl TryFrom<&Ty> for ClosedTy {
    type Error = OpenTy;

    fn try_from(ty: &Ty) -> Result<ClosedTy, OpenTy> {
        if ty.is_closed() {
            Ok(ClosedTy(ty.clone()))
        } else {
            Err(OpenTy)
        }
    }
}

impl TryFrom<Ty> for ClosedTy {
    type Error = OpenTy;

    fn try_from(ty: Ty) -> Result<ClosedTy, OpenTy> {
        if ty.is_closed() {
            Ok(ClosedTy(ty))
        } else {
            Err(OpenTy)
        }
    }
}

/// The interface-constraint twin of the [`ClosedTy`] gate: a plain
/// [`Interface`](crate::Interface) exists for an [`InferInterface`] iff every
/// carried type is closed. The `Err` forces the boundary to pick a
/// disposition, exactly like the type-level gate.
impl TryFrom<&InferInterface> for crate::Interface {
    type Error = OpenTy;

    fn try_from(reference: &InferInterface) -> Result<crate::Interface, OpenTy> {
        let closed = |ty: &Ty| ClosedTy::try_from(ty).map(|closed| closed.to_plain());
        Ok(crate::Interface::new(
            reference.name.clone(),
            reference
                .generics
                .iter()
                .map(&closed)
                .collect::<Result<_, OpenTy>>()?,
            reference
                .associated_types
                .iter()
                .map(|(name, ty)| Ok((name.clone(), closed(ty)?)))
                .collect::<Result<_, OpenTy>>()?,
        ))
    }
}

/// The interface-constraint twin of [`ClosedTy`]: an [`InferInterface`]
/// whose carried types are all free of live inference variables.
///
/// Exists for the same reason as `ClosedTy` — so a boundary that stores or
/// hands out declaration-side interface references converts to the plain
/// vocabulary TOTALLY, instead of re-deriving "this cannot contain a
/// variable" with an `unreachable!` at each exit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosedInterface(InferInterface);

impl TryFrom<&InferInterface> for ClosedInterface {
    type Error = OpenTy;

    fn try_from(reference: &InferInterface) -> Result<ClosedInterface, OpenTy> {
        let closed = reference.generics.iter().all(Ty::is_closed)
            && reference
                .associated_types
                .iter()
                .all(|(_, ty)| ty.is_closed());
        if closed {
            Ok(ClosedInterface(reference.clone()))
        } else {
            Err(OpenTy)
        }
    }
}

impl std::ops::Deref for ClosedInterface {
    type Target = InferInterface;

    fn deref(&self) -> &InferInterface {
        &self.0
    }
}

impl ClosedInterface {
    /// Interning a plain constraint, TOTAL into the closed world: the plain
    /// vocabulary has no inference variants.
    pub fn from_constraint(interface: &crate::Interface) -> ClosedInterface {
        ClosedInterface(InferInterface::from_constraint(interface))
    }

    /// The underlying reference.
    pub fn as_reference(&self) -> &InferInterface {
        &self.0
    }

    /// Materializes the plain constraint — total by the closed invariant.
    pub fn to_plain(&self) -> crate::Interface {
        crate::Interface::try_from(&self.0).unwrap_or_else(|_| {
            unreachable!("ClosedInterface invariant: no live inference variables")
        })
    }
}

impl PartialEq<Ty> for ClosedTy {
    fn eq(&self, other: &Ty) -> bool {
        &self.0 == other
    }
}

impl PartialEq<ClosedTy> for Ty {
    fn eq(&self, other: &ClosedTy) -> bool {
        self == &other.0
    }
}

impl std::ops::Deref for ClosedTy {
    type Target = Ty;

    fn deref(&self) -> &Ty {
        &self.0
    }
}

impl ClosedTy {
    /// The underlying handle.
    pub fn as_ty(&self) -> &Ty {
        &self.0
    }

    /// Unwrap the handle. Deliberately available: `Ty` is the general
    /// vocabulary, so re-opening loses only the proof, never soundness.
    pub fn into_ty(self) -> Ty {
        self.0
    }

    /// Materializes the finalized plain structure — THE interned→plain
    /// conversion, total by the closed invariant.
    pub fn to_plain(&self) -> crate::Ty {
        self.0.to_plain_closed()
    }

    /// Interning a plain type, TOTAL into the closed world: the finalized
    /// plain vocabulary has no inference variants, so its image cannot
    /// carry a variable.
    pub fn from_plain(ty: &crate::Ty) -> ClosedTy {
        ClosedTy(Ty::from_plain(ty))
    }

    /// Crate-internal constructor for values closed BY CONSTRUCTION
    /// (children of a closed node, plain-derived interning). The single
    /// home of the subtree argument; the O(1) flag check still guards it
    /// in debug builds.
    pub(crate) fn closed_by_construction(ty: Ty) -> ClosedTy {
        debug_assert!(ty.is_closed(), "closed_by_construction on an open type");
        ClosedTy(ty)
    }

    /// Visits each direct child, closed: `HAS_INFER` is a subtree union,
    /// so every child of a closed node is closed. This pair of walkers is
    /// where that argument lives — descents stay in the closed world
    /// without re-proving it per node.
    pub fn for_each_child(&self, mut f: impl FnMut(&ClosedTy)) {
        for_each_child(self.0.kind(), |child| {
            f(&ClosedTy::closed_by_construction(child.clone()));
        });
    }

    /// Rebuilds the node with each child mapped through `f`, closed on
    /// both sides: children are closed (subtree union), and the rebuilt
    /// node's head is this node's head, which the closed invariant says
    /// is not a variable.
    pub fn map_children(&self, mut f: impl FnMut(&ClosedTy) -> ClosedTy) -> ClosedTy {
        ClosedTy::closed_by_construction(Ty::intern(
            self.0.kind().map_children(|child| {
                f(&ClosedTy::closed_by_construction(child.clone())).into_ty()
            }),
        ))
    }
}

// -- Leaf helpers -------------------------------------------------------------

impl Ty {
    pub fn int() -> Ty {
        Ty::intern(InferTy::Int {
            attr: TyAttr::default(),
        })
    }

    pub fn float() -> Ty {
        Ty::intern(InferTy::Float {
            attr: TyAttr::default(),
        })
    }

    pub fn string() -> Ty {
        Ty::intern(InferTy::String {
            attr: TyAttr::default(),
        })
    }

    pub fn bool() -> Ty {
        Ty::intern(InferTy::Bool {
            attr: TyAttr::default(),
        })
    }

    pub fn null() -> Ty {
        Ty::intern(InferTy::Null {
            attr: TyAttr::default(),
        })
    }

    pub fn never() -> Ty {
        Ty::intern(InferTy::Never {
            attr: TyAttr::default(),
        })
    }

    pub fn void() -> Ty {
        Ty::intern(InferTy::Void {
            attr: TyAttr::default(),
        })
    }

    pub fn error() -> Ty {
        Ty::intern(InferTy::Error {
            attr: TyAttr::default(),
        })
    }

    pub fn infer_var(var: InferVar) -> Ty {
        Ty::intern(InferTy::InferVar {
            var,
            attr: TyAttr::default(),
        })
    }

    pub fn list(inner: Ty) -> Ty {
        Ty::intern(InferTy::List(inner, TyAttr::default()))
    }

    pub fn union(members: impl IntoIterator<Item = Ty>) -> Ty {
        Ty::intern(InferTy::Union(
            members.into_iter().collect(),
            TyAttr::default(),
        ))
    }

    /// `T?` is a flat `T | null` union.
    pub fn optional(inner: Ty) -> Ty {
        match inner.kind() {
            InferTy::Union(members, attr) => {
                if members
                    .iter()
                    .any(|member| matches!(member.kind(), InferTy::Null { .. }))
                {
                    inner
                } else {
                    let mut members = members.to_vec();
                    members.push(Ty::null());
                    Ty::intern(InferTy::Union(members.into(), attr.clone()))
                }
            }
            InferTy::Null { .. } => inner,
            _ => Ty::union([inner, Ty::null()]),
        }
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Literal, ParamTy};

    fn plain_samples() -> Vec<crate::Ty> {
        use crate::Ty as P;
        let a = TyAttr::default;
        let int = || P::Int { attr: a() };
        let name = || TypeName::local(Name::new("Foo"));
        vec![
            P::Int { attr: a() },
            P::Bigint { attr: a() },
            P::Float { attr: a() },
            P::String { attr: a() },
            P::Bool { attr: a() },
            P::Null { attr: a() },
            P::Uint8Array { attr: a() },
            P::Literal(Literal::Int(1), Freshness::Fresh, a()),
            P::Class(name(), Box::new([int()]), a()),
            P::Interface(
                name(),
                Box::new([int()]),
                Box::new([(Name::new("Item"), int())]),
                a(),
            ),
            P::Enum(name(), a()),
            P::EnumVariant(name(), Name::new("A"), a()),
            P::List(Box::new(int()), a()),
            P::Map {
                key: Box::new(P::String { attr: a() }),
                value: Box::new(int()),
                attr: a(),
            },
            P::Union(Box::new([int(), P::Null { attr: a() }]), a()),
            P::Function {
                params: Box::new([crate::FunctionParamTy::required(
                    Some(Name::new("x")),
                    int(),
                )]),
                ret: Box::new(int()),
                throws: Box::new(P::Never { attr: a() }),
                attr: a(),
            },
            P::Future(Box::new(int()), Box::new(P::Never { attr: a() }), a()),
            P::RustType { attr: a() },
            P::Type { attr: a() },
            P::Resource { attr: a() },
            P::PromptAst { attr: a() },
            P::Void { attr: a() },
            P::TypeAlias(name(), a()),
            P::TypeVar(ParamTy::new(0, Name::new("T")), a()),
            P::AssociatedTypeProjection {
                base: Box::new(int()),
                interface: Box::new(crate::Interface::new(name(), Box::new([]), Box::new([]))),
                member: Name::new("Item"),
                attr: a(),
            },
            P::Unknown { attr: a() },
            P::Never { attr: a() },
            P::Error { attr: a() },
        ]
    }

    #[test]
    fn roundtrip_every_variant() {
        for plain in plain_samples() {
            let interned = Ty::from_plain(&plain);
            let closed = ClosedTy::try_from(&interned).expect("plain input carries no variables");
            assert_eq!(plain, closed.to_plain());
        }
    }

    #[test]
    fn open_type_cannot_close() {
        // The boundary is the type system: an open tree never reaches the
        // conversion — the caller must defer, suppress, rename, or dispose.
        let open = Ty::list(Ty::infer_var(InferVar::new(3)));
        assert!(!open.is_closed());
        assert_eq!(ClosedTy::try_from(&open), Err(OpenTy));
        assert!(ClosedTy::try_from(&Ty::list(Ty::int())).is_ok());
    }

    #[test]
    fn optional_flattens_union_and_is_idempotent() {
        let optional = Ty::optional(Ty::union([Ty::int(), Ty::string()]));
        let InferTy::Union(members, _) = optional.kind() else {
            panic!("expected union");
        };
        assert_eq!(members.len(), 3);
        assert!(members.iter().any(|member| member == &Ty::int()));
        assert!(members.iter().any(|member| member == &Ty::string()));
        assert!(
            members
                .iter()
                .any(|member| matches!(member.kind(), InferTy::Null { .. }))
        );
        assert!(
            !members
                .iter()
                .any(|member| matches!(member.kind(), InferTy::Union(..)))
        );
        assert!(Ty::optional(optional.clone()) == optional);
    }

    #[test]
    fn structurally_equal_types_share_one_allocation() {
        for plain in plain_samples() {
            let a = Ty::from_plain(&plain);
            let b = Ty::from_plain(&plain);
            assert!(a == b, "expected pointer-equal handles for {plain:?}");
        }
        // Substructure is shared too: the element of `int[]` is the `int`.
        let int = Ty::int();
        let list = Ty::list(int.clone());
        let InferTy::List(elem, _) = list.kind() else {
            panic!("expected list");
        };
        assert!(*elem == int);
    }

    #[test]
    fn flags_propagate_from_children() {
        let infer = Ty::infer_var(InferVar::new(0));
        assert!(infer.has_infer());
        let deep = Ty::list(Ty::union([Ty::int(), infer]));
        assert!(deep.has_infer());
        assert!(!deep.has_error());
        assert!(!Ty::list(Ty::int()).has_infer());
        assert!(Ty::list(Ty::error()).has_error());
    }

    #[test]
    fn map_children_rebuilds_nested_structure() {
        let var = Ty::infer_var(InferVar::new(7));
        let nested = Ty::list(Ty::union([Ty::int(), var.clone()]));

        fn substitute(ty: &Ty, from: &Ty, to: &Ty) -> Ty {
            if ty == from {
                return to.clone();
            }
            if !ty.has_infer() {
                return ty.clone();
            }
            Ty::intern(ty.kind().map_children(|child| substitute(child, from, to)))
        }

        let resolved = substitute(&nested, &var, &Ty::string());
        assert!(resolved == Ty::list(Ty::union([Ty::int(), Ty::string()])));
        assert!(!resolved.has_infer());
        // Untouched subtrees keep their identity.
        let unchanged = substitute(&nested, &Ty::infer_var(InferVar::new(8)), &Ty::string());
        assert!(unchanged == nested);
    }

    #[test]
    fn ordering_matches_plain_ordering() {
        let mut plain = plain_samples();
        let mut interned: Vec<Ty> = plain.iter().map(Ty::from_plain).collect();
        plain.sort();
        interned.sort();
        let materialized: Vec<crate::Ty> = interned
            .iter()
            .map(|ty| {
                ClosedTy::try_from(ty)
                    .expect("samples are closed")
                    .to_plain()
            })
            .collect();
        assert_eq!(plain, materialized);
    }

    /// Whether the pool currently holds an entry for `kind`. Test-only; the
    /// pool is global and shared with concurrently running tests, so tests
    /// probe unique keys instead of asserting pool sizes.
    fn pool_contains(kind: &InferTy) -> bool {
        let data = TyData {
            flags: compute_flags(kind),
            kind: kind.clone(),
        };
        pool().lock().unwrap().contains(&data)
    }

    #[test]
    fn dropping_last_handle_evicts_pool_entry() {
        let probe_kind = InferTy::Literal(
            Literal::String("interned-eviction-probe".into()),
            Freshness::Regular,
            TyAttr::default(),
        );
        assert!(!pool_contains(&probe_kind));
        let ty = Ty::intern(probe_kind.clone());
        assert!(pool_contains(&probe_kind));
        let second = ty.clone();
        drop(ty);
        assert!(
            pool_contains(&probe_kind),
            "live handle must keep the entry"
        );
        drop(second);
        assert!(!pool_contains(&probe_kind), "last drop must evict");
        // Re-interning after eviction works.
        let again = Ty::intern(probe_kind.clone());
        assert!(pool_contains(&probe_kind));
        drop(again);
    }
}
