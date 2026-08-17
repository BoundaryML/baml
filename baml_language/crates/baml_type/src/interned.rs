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
//! - Children of [`TyKind`] are handles, so interning is recursive: pool
//!   lookups hash and compare shallowly (child pointers, not child trees),
//!   and substructure is shared automatically.
//! - [`TypeFlags`] ("does this contain an inference variable / error /
//!   type variable...") are computed once at intern time and answered in
//!   O(1), which is what lets inference fold/resolve loops short-circuit.
//! - The pool is a global mutex-guarded set, NOT salsa: unlike compiler-local
//!   ids (`FunctionLoc`, `ScopeId`), types must outlive any database - they
//!   are held by the runtime, serialized, and crossed over FFI. This is the
//!   same reason rust-analyzer interns its types in a global pool despite
//!   being salsa-based throughout. Dead entries (no handle outside the pool)
//!   linger until an explicit [`Ty::sweep_pool`] — dropping a handle is a
//!   plain refcount decrement, never a pool lock. Short-lived processes (the
//!   CLI) never need to sweep; the long-lived LSP must sweep on idle to
//!   reclaim (a compile churns through far more transient types than survive).
//!
//! Variants, payload shapes, and discriminant order mirror the master
//! [`ty_family!`](crate::Ty) enum (conversions are exhaustive matches, so
//! drift is a compile error), with deliberate spec-driven deltas: `Infer`
//! carries an optional [`InferVar`] so inference-table variables are native
//! (`None` is the syntactic `_` hole, exactly what the plain enum's `Infer`
//! means today); TIR's internal recovery sentinels (plain `Unknown`,
//! `EvolvingList`, `EvolvingMap`) are unrepresentable - see the note at the
//! end of [`TyKind`]; and the top type takes its spec name `Unknown` here
//! (the plain enum calls it `BuiltinUnknown` only because the sentinel had
//! claimed the shorter name).

use std::{
    cmp::Ordering,
    collections::HashSet,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use rustc_hash::FxBuildHasher;

use crate::{Freshness, FunctionParamMode, Literal, MediaKind, Name, ParamTy, TyAttr, TypeName};

// -- Flags --------------------------------------------------------------------

bitflags::bitflags! {
    /// Facts about a type computed once at intern time; the union of each
    /// node's own bit and all its children's flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct TypeFlags: u16 {
        /// Contains an `Infer` node (a table variable or a syntactic `_` hole).
        const HAS_INFER = 1 << 0;
        /// Contains an `Error` sentinel.
        const HAS_ERROR = 1 << 1;
        /// Contains a named `TypeVar`.
        const HAS_TYPEVAR = 1 << 2;
        /// Contains an unresolved associated-type projection.
        const HAS_PROJECTION = 1 << 3;
        /// Contains a fresh (unwidened) literal.
        const HAS_FRESH_LITERAL = 1 << 4;
        /// Contains a `Union` node (anywhere, own node included).
        const HAS_UNION = 1 << 5;
    }
}

// -- Handle -------------------------------------------------------------------

/// An inference-table variable index. Only ever allocated by the hir_ty
/// inference table; `Infer { var: None }` is the syntactic `_` hole.
///
/// Deliberately a bare index: the representation carries variable IDENTITY,
/// the inference table carries variable KIND. Distinctions like effect vars
/// (throws slots defaulting to `never`), diverging vars, and canonical
/// placeholders are table-side policy metadata keyed by this index -
/// rust-analyzer's `diverging_type_vars` side-set pattern - not new `TyKind`
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
    kind: TyKind,
}

impl Ty {
    /// Interns `kind`, returning the unique handle for it. Flags are computed
    /// here; there is no other way to construct a `Ty`.
    pub fn intern(kind: TyKind) -> Ty {
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

    pub fn kind(&self) -> &TyKind {
        &self.0.kind
    }

    pub fn flags(&self) -> TypeFlags {
        self.0.flags
    }

    /// Whether this type still contains inference variables or `_` holes.
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

    /// Whether this type contains a union node anywhere (own node included).
    /// Union-shape folds (e.g. inference's union canonicalization) are the
    /// identity on types without one, so they short-circuit on this.
    pub fn has_union(&self) -> bool {
        self.0.flags.contains(TypeFlags::HAS_UNION)
    }
}

impl Clone for Ty {
    fn clone(&self) -> Ty {
        Ty(Arc::clone(&self.0))
    }
}

// NOTE deliberately no `Drop` impl: dropping a handle is a plain `Arc`
// decrement. Compilation churns through millions of transient types (probe
// constructions that hit the pool, inference scratch); evicting on drop made
// every one of those pay a second full hash plus the global pool lock on the
// way out. Dead entries are reclaimed explicitly via [`Ty::sweep_pool`].

impl Ty {
    /// Evicts every pool entry with no live handle outside the pool,
    /// returning how many were reclaimed.
    ///
    /// Contract: dropping a `Ty` never touches the pool, so dead entries
    /// accumulate until this runs. The short-lived CLI never needs to call
    /// it (process exit reclaims everything); the long-lived LSP is expected
    /// to call it on a periodic timer (every N seconds). Repeated calls are
    /// safe; an idle-pool call costs one scan and reclaims nothing. Safe to
    /// call concurrently with interning from other threads: only entries
    /// with no outside handle are removed, and a concurrent intern of an
    /// equal kind re-inserts a fresh entry.
    ///
    /// Deliberately a plain scan, not insert-watermark-gated: entries die
    /// via handle *drops*, which are untracked (that is the whole point), so
    /// "no interns since last sweep" would NOT imply "nothing to reclaim" —
    /// it would skip reclamation exactly in the idle-editor window where the
    /// timer is supposed to release a finished compile's memory. Each pass
    /// scans the whole pool under the global pool mutex, stalling concurrent
    /// interns for the scan's duration; callers can adapt their cadence from
    /// the returned count (an incremental/sharded sweep is the follow-up if
    /// the stall ever matters).
    pub fn sweep_pool() -> usize {
        let mut total = 0;
        // A dead parent's child handles keep the children alive until the
        // parent entry itself is freed, so reclamation is transitive: iterate
        // passes until a pass evicts nothing.
        loop {
            let mut pool = pool().lock().expect("ty intern pool poisoned");
            // Collect evictees first so their (recursive) frees run after the
            // guard is released, keeping the critical section short. A count
            // of 1 under the lock proves no outside handle exists, and none
            // can appear: minting a handle requires either the pool lock
            // (intern) or an existing outside handle (clone).
            let dead: Vec<Arc<TyData>> = pool
                .iter()
                .filter(|entry| Arc::strong_count(entry) == 1)
                .map(Arc::clone)
                .collect();
            for entry in &dead {
                pool.remove(&**entry);
            }
            drop(pool);
            if dead.is_empty() {
                return total;
            }
            total += dead.len();
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

/// The intern pool. FxHash rather than the default SipHash: the pool is
/// in-process only — never iterated for output, never serialized — so
/// HashDoS resistance buys nothing, and node hashing (discriminant + attrs +
/// name strings + child pointers) is hot enough to show as a top profile
/// leaf under SipHash.
fn pool() -> &'static Mutex<HashSet<Arc<TyData>, FxBuildHasher>> {
    static POOL: OnceLock<Mutex<HashSet<Arc<TyData>, FxBuildHasher>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashSet::default()))
}

// -- Kind ---------------------------------------------------------------------

/// Structural type kind; the interned mirror of the master `ty_family!` enum,
/// same variants in the same (discriminant) order, children as handles.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TyKind {
    // = 0
    Int {
        attr: TyAttr,
    },
    // = 1
    Bigint {
        attr: TyAttr,
    },
    // = 2
    Float {
        attr: TyAttr,
    },
    // = 3
    String {
        attr: TyAttr,
    },
    // = 4
    Bool {
        attr: TyAttr,
    },
    // = 5
    Null {
        attr: TyAttr,
    },
    // = 6
    Uint8Array {
        attr: TyAttr,
    },
    // = 7
    Media(MediaKind, TyAttr),
    // = 8
    Literal(Literal, Freshness, TyAttr),
    // = 9
    Class(TypeName, Box<[Ty]>, TyAttr),
    // = 10
    Interface(TypeName, Box<[Ty]>, Box<[(Name, Ty)]>, TyAttr),
    // = 11
    Enum(TypeName, TyAttr),
    // = 12
    EnumVariant(TypeName, Name, TyAttr),
    // = 13
    List(Ty, TyAttr),
    // = 14
    Map {
        key: Ty,
        value: Ty,
        attr: TyAttr,
    },
    // = 15
    Union(Box<[Ty]>, TyAttr),
    // = 16
    Function {
        params: Box<[FunctionParam]>,
        ret: Ty,
        throws: Ty,
        attr: TyAttr,
    },
    // = 17
    Future(Ty, Ty, TyAttr),
    // = 18
    RustType {
        attr: TyAttr,
    },
    // = 19
    Type {
        attr: TyAttr,
    },
    // = 20
    Resource {
        attr: TyAttr,
    },
    // = 21
    PromptAst {
        attr: TyAttr,
    },
    // = 22
    Void {
        attr: TyAttr,
    },
    // = 24 (23 reserved)
    TypeAlias(TypeName, TyAttr),
    // = 25
    TypeVar(ParamTy, TyAttr),
    // = 26
    AssociatedTypeProjection {
        base: Ty,
        interface: InterfaceRef,
        member: Name,
        attr: TyAttr,
    },
    // = 27; the spec's top type (the user-denotable `unknown` keyword,
    // `T <: unknown` for all `T`). NAMING: the plain enum calls this
    // `BuiltinUnknown` only because plain `Unknown` is taken by the TIR
    // error-recovery sentinel (unrepresentable here, see below). This
    // representation uses the spec name.
    Unknown {
        attr: TyAttr,
    },
    // = 28
    Never {
        attr: TyAttr,
    },
    // = 30 (29 deliberately unrepresentable, see below); the single error
    // sentinel: a diagnostic was already emitted for this node, downstream
    // must not cascade further errors from it.
    Error {
        attr: TyAttr,
    },
    // = 33 (31/32 deliberately unrepresentable, see below); `var: None` is
    // the syntactic `_` hole (the plain enum's `Infer`), `Some` is a live
    // inference-table variable (hir_ty only; must never survive
    // `resolve_all`).
    Infer {
        var: Option<InferVar>,
        attr: TyAttr,
    },
    // Deliberately absent:
    // - the plain enum's `Unknown` (= 29): TIR's second error-recovery
    //   sentinel (NOT this enum's `Unknown`, which is the top type), which
    //   TIR also overloads as a "no expectation" marker and a "not yet
    //   inferred" seed. Each job has a principled home in hir_ty: unresolved
    //   names are the single `Error` sentinel (rust-analyzer's model), "no
    //   expectation" is `Expectation::None`, and "not yet inferred" is a
    //   fresh `Infer` variable.
    // - `EvolvingList` (= 31) / `EvolvingMap` (= 32): TIR-internal sentinels
    //   for empty-container element refinement. The hir_ty engine expresses
    //   the same thing honestly as `List`/`Map` over inference variables, so
    //   these must not be representable here; they only ever exist
    //   mid-TIR-inference and never cross a boundary this module imports.
    // - `TypeArgRef` (= 34): the frame axis exists only in `TyTemplate`, not
    //   in the plain `Ty` this module mirrors; it joins this representation
    //   when the family axes migrate at cutover.
}

/// Interned twin of the `FunctionParamTy` satellite.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionParam {
    pub name: Option<Name>,
    pub ty: Ty,
    pub mode: FunctionParamMode,
}

impl FunctionParam {
    pub fn required(name: Option<Name>, ty: Ty) -> FunctionParam {
        FunctionParam {
            name,
            ty,
            mode: FunctionParamMode::Required,
        }
    }

    pub fn optional(name: Option<Name>, ty: Ty) -> FunctionParam {
        FunctionParam {
            name,
            ty,
            mode: FunctionParamMode::Optional,
        }
    }
}

/// Interned twin of the `Interface` satellite (an interface *constraint*,
/// distinct from the `TyKind::Interface` existential).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterfaceRef {
    pub name: TypeName,
    pub generics: Box<[Ty]>,
    pub associated_types: Box<[(Name, Ty)]>,
}

impl InterfaceRef {
    /// Sorts `associated_types` by name, mirroring the plain satellite's
    /// order-insensitivity invariant.
    pub fn new(
        name: TypeName,
        generics: Box<[Ty]>,
        mut associated_types: Vec<(Name, Ty)>,
    ) -> InterfaceRef {
        associated_types.sort_by(|(a, _), (b, _)| a.cmp(b));
        InterfaceRef {
            name,
            generics,
            associated_types: associated_types.into_boxed_slice(),
        }
    }

    /// The interface reference an existential type carries, when `ty`
    /// is one - the single extraction every consumer shares (rustc has
    /// exactly one `TraitRef`; nobody hand-builds a parallel copy).
    pub fn of_ty(ty: &Ty) -> Option<InterfaceRef> {
        match ty.kind() {
            TyKind::Interface(name, args, pins, _) => Some(InterfaceRef::new(
                name.clone(),
                args.to_vec().into_boxed_slice(),
                pins.to_vec(),
            )),
            _ => None,
        }
    }

    /// From the plain algebra's constraint satellite (the `TypeContext`
    /// boundary).
    pub fn from_constraint(interface: &crate::Interface) -> InterfaceRef {
        InterfaceRef::new(
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
        Ty::intern(TyKind::Interface(
            self.name.clone(),
            self.generics.to_vec().into(),
            self.associated_types.to_vec().into(),
            TyAttr::default(),
        ))
    }
}

// -- Flag computation ---------------------------------------------------------

/// Calls `visit` on each direct child type of `kind` (including types nested
/// in satellites: function params, interface generics and bindings).
pub fn for_each_child(kind: &TyKind, mut visit: impl FnMut(&Ty)) {
    match kind {
        TyKind::Int { .. }
        | TyKind::Bigint { .. }
        | TyKind::Float { .. }
        | TyKind::String { .. }
        | TyKind::Bool { .. }
        | TyKind::Null { .. }
        | TyKind::Uint8Array { .. }
        | TyKind::Media(..)
        | TyKind::Literal(..)
        | TyKind::Enum(..)
        | TyKind::EnumVariant(..)
        | TyKind::RustType { .. }
        | TyKind::Type { .. }
        | TyKind::Resource { .. }
        | TyKind::PromptAst { .. }
        | TyKind::Void { .. }
        | TyKind::TypeAlias(..)
        | TyKind::TypeVar(..)
        | TyKind::Unknown { .. }
        | TyKind::Never { .. }
        | TyKind::Error { .. }
        | TyKind::Infer { .. } => {}
        TyKind::Class(_, args, _) => args.iter().for_each(visit),
        TyKind::Interface(_, args, assoc, _) => {
            args.iter().for_each(&mut visit);
            assoc.iter().for_each(|(_, ty)| visit(ty));
        }
        TyKind::List(inner, _) => visit(inner),
        TyKind::Map { key, value, .. } => {
            visit(key);
            visit(value);
        }
        TyKind::Union(members, _) => members.iter().for_each(visit),
        TyKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().for_each(|param| visit(&param.ty));
            visit(ret);
            visit(throws);
        }
        TyKind::Future(value, error, _) => {
            visit(value);
            visit(error);
        }
        TyKind::AssociatedTypeProjection {
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

impl TyKind {
    /// Rebuilds this kind with every direct child type replaced by
    /// `f(child)` (satellite-nested children included) - the rebuild dual of
    /// [`for_each_child`]. Leaf kinds clone unchanged. Callers intern the
    /// result; short-circuit on [`Ty::flags`] first when the fold cannot
    /// apply (e.g. no `HAS_INFER`).
    pub fn map_children(&self, mut f: impl FnMut(&Ty) -> Ty) -> TyKind {
        match self {
            TyKind::Int { .. }
            | TyKind::Bigint { .. }
            | TyKind::Float { .. }
            | TyKind::String { .. }
            | TyKind::Bool { .. }
            | TyKind::Null { .. }
            | TyKind::Uint8Array { .. }
            | TyKind::Media(..)
            | TyKind::Literal(..)
            | TyKind::Enum(..)
            | TyKind::EnumVariant(..)
            | TyKind::RustType { .. }
            | TyKind::Type { .. }
            | TyKind::Resource { .. }
            | TyKind::PromptAst { .. }
            | TyKind::Void { .. }
            | TyKind::TypeAlias(..)
            | TyKind::TypeVar(..)
            | TyKind::Unknown { .. }
            | TyKind::Never { .. }
            | TyKind::Error { .. }
            | TyKind::Infer { .. } => self.clone(),
            TyKind::Class(name, args, attr) => TyKind::Class(
                name.clone(),
                args.iter().map(&mut f).collect(),
                attr.clone(),
            ),
            TyKind::Interface(name, args, assoc, attr) => TyKind::Interface(
                name.clone(),
                args.iter().map(&mut f).collect(),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), f(ty)))
                    .collect(),
                attr.clone(),
            ),
            TyKind::List(inner, attr) => TyKind::List(f(inner), attr.clone()),
            TyKind::Map { key, value, attr } => TyKind::Map {
                key: f(key),
                value: f(value),
                attr: attr.clone(),
            },
            TyKind::Union(members, attr) => {
                TyKind::Union(members.iter().map(&mut f).collect(), attr.clone())
            }
            TyKind::Function {
                params,
                ret,
                throws,
                attr,
            } => TyKind::Function {
                params: params
                    .iter()
                    .map(|param| FunctionParam {
                        name: param.name.clone(),
                        ty: f(&param.ty),
                        mode: param.mode,
                    })
                    .collect(),
                ret: f(ret),
                throws: f(throws),
                attr: attr.clone(),
            },
            TyKind::Future(value, error, attr) => TyKind::Future(f(value), f(error), attr.clone()),
            TyKind::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => TyKind::AssociatedTypeProjection {
                base: f(base),
                interface: InterfaceRef {
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

impl Ty {
    /// Rebuilds this type with each direct child replaced by `f(child)`
    /// (satellite-nested children included), interning only when a child
    /// actually changed: if every child maps to itself the ORIGINAL handle
    /// is returned — no candidate kind materialization, no pool lock. This
    /// is the form every fold/substitution pass should use: on the common
    /// mostly-unchanged tree it turns per-node intern traffic into pointer
    /// compares.
    ///
    /// `f` is called exactly once per child (it may be stateful), in
    /// [`for_each_child`] order, which [`TyKind::map_children`] mirrors.
    pub fn map_children_preserving(&self, mut f: impl FnMut(&Ty) -> Ty) -> Ty {
        let mut mapped: Vec<Ty> = Vec::new();
        let mut changed = false;
        for_each_child(self.kind(), |child| {
            let new = f(child);
            changed |= new != *child;
            mapped.push(new);
        });
        if !changed {
            return self.clone();
        }
        let mut next = mapped.into_iter();
        let rebuilt = Ty::intern(self.kind().map_children(|_| {
            next.next()
                .expect("map_children visits the same children as for_each_child")
        }));
        debug_assert!(
            next.next().is_none(),
            "map_children visited more children than for_each_child"
        );
        rebuilt
    }
}

fn compute_flags(kind: &TyKind) -> TypeFlags {
    let own = match kind {
        TyKind::Literal(_, Freshness::Fresh, _) => TypeFlags::HAS_FRESH_LITERAL,
        TyKind::TypeVar(..) => TypeFlags::HAS_TYPEVAR,
        TyKind::AssociatedTypeProjection { .. } => TypeFlags::HAS_PROJECTION,
        TyKind::Error { .. } => TypeFlags::HAS_ERROR,
        TyKind::Infer { .. } => TypeFlags::HAS_INFER,
        TyKind::Union(..) => TypeFlags::HAS_UNION,
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
    /// # Panics
    ///
    /// On the plain `Unknown`/`EvolvingList`/`EvolvingMap`: TIR-internal
    /// inference sentinels that must never reach this boundary (hir_ty uses
    /// the single `Error` sentinel and inference variables instead).
    pub fn from_plain(ty: &crate::Ty) -> Ty {
        let interned_all =
            |tys: &[crate::Ty]| -> Box<[Ty]> { tys.iter().map(Ty::from_plain).collect() };
        let kind = match ty {
            crate::Ty::Int { attr } => TyKind::Int { attr: attr.clone() },
            crate::Ty::Bigint { attr } => TyKind::Bigint { attr: attr.clone() },
            crate::Ty::Float { attr } => TyKind::Float { attr: attr.clone() },
            crate::Ty::String { attr } => TyKind::String { attr: attr.clone() },
            crate::Ty::Bool { attr } => TyKind::Bool { attr: attr.clone() },
            crate::Ty::Null { attr } => TyKind::Null { attr: attr.clone() },
            crate::Ty::Uint8Array { attr } => TyKind::Uint8Array { attr: attr.clone() },
            crate::Ty::Media(kind, attr) => TyKind::Media(*kind, attr.clone()),
            crate::Ty::Literal(lit, freshness, attr) => {
                TyKind::Literal(lit.clone(), *freshness, attr.clone())
            }
            crate::Ty::Class(name, args, attr) => {
                TyKind::Class(name.clone(), interned_all(args), attr.clone())
            }
            crate::Ty::Interface(name, args, assoc, attr) => TyKind::Interface(
                name.clone(),
                interned_all(args),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                    .collect(),
                attr.clone(),
            ),
            crate::Ty::Enum(name, attr) => TyKind::Enum(name.clone(), attr.clone()),
            crate::Ty::EnumVariant(name, variant, attr) => {
                TyKind::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            crate::Ty::List(inner, attr) => TyKind::List(Ty::from_plain(inner), attr.clone()),
            crate::Ty::Map { key, value, attr } => TyKind::Map {
                key: Ty::from_plain(key),
                value: Ty::from_plain(value),
                attr: attr.clone(),
            },
            crate::Ty::Union(members, attr) => TyKind::Union(interned_all(members), attr.clone()),
            crate::Ty::Function {
                params,
                ret,
                throws,
                attr,
            } => TyKind::Function {
                params: params
                    .iter()
                    .map(|param| FunctionParam {
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
                TyKind::Future(Ty::from_plain(value), Ty::from_plain(error), attr.clone())
            }
            crate::Ty::RustType { attr } => TyKind::RustType { attr: attr.clone() },
            crate::Ty::Type { attr } => TyKind::Type { attr: attr.clone() },
            crate::Ty::Resource { attr } => TyKind::Resource { attr: attr.clone() },
            crate::Ty::PromptAst { attr } => TyKind::PromptAst { attr: attr.clone() },
            crate::Ty::Void { attr } => TyKind::Void { attr: attr.clone() },
            crate::Ty::TypeAlias(name, attr) => TyKind::TypeAlias(name.clone(), attr.clone()),
            crate::Ty::TypeVar(param, attr) => TyKind::TypeVar(param.clone(), attr.clone()),
            crate::Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => TyKind::AssociatedTypeProjection {
                base: Ty::from_plain(base),
                interface: InterfaceRef::new(
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
            crate::Ty::BuiltinUnknown { attr } => TyKind::Unknown { attr: attr.clone() },
            crate::Ty::Never { attr } => TyKind::Never { attr: attr.clone() },
            crate::Ty::Error { attr } => TyKind::Error { attr: attr.clone() },
            crate::Ty::Unknown { .. }
            | crate::Ty::EvolvingList(..)
            | crate::Ty::EvolvingMap(..) => {
                panic!(
                    "plain Ty::Unknown/EvolvingList/EvolvingMap are TIR-internal inference \
                     sentinels with no interned form (hir_ty uses the single Error sentinel \
                     and inference variables instead; the interned Unknown is the TOP type, \
                     i.e. plain BuiltinUnknown). They must never reach this boundary."
                )
            }
            crate::Ty::Infer { attr } => TyKind::Infer {
                var: None,
                attr: attr.clone(),
            },
        };
        Ty::intern(kind)
    }

    /// Materializes the plain enum's structure. Total, but lossy for live
    /// inference variables: `Infer { var: Some(_) }` becomes the plain `Infer`
    /// hole (callers materializing results must run resolve-all first; the
    /// `has_infer` flag makes that cheap to assert).
    pub fn to_plain(&self) -> crate::Ty {
        let plain_all = |tys: &[Ty]| -> Vec<crate::Ty> { tys.iter().map(Ty::to_plain).collect() };
        match self.kind() {
            TyKind::Int { attr } => crate::Ty::Int { attr: attr.clone() },
            TyKind::Bigint { attr } => crate::Ty::Bigint { attr: attr.clone() },
            TyKind::Float { attr } => crate::Ty::Float { attr: attr.clone() },
            TyKind::String { attr } => crate::Ty::String { attr: attr.clone() },
            TyKind::Bool { attr } => crate::Ty::Bool { attr: attr.clone() },
            TyKind::Null { attr } => crate::Ty::Null { attr: attr.clone() },
            TyKind::Uint8Array { attr } => crate::Ty::Uint8Array { attr: attr.clone() },
            TyKind::Media(kind, attr) => crate::Ty::Media(*kind, attr.clone()),
            TyKind::Literal(lit, freshness, attr) => {
                crate::Ty::Literal(lit.clone(), *freshness, attr.clone())
            }
            TyKind::Class(name, args, attr) => {
                crate::Ty::Class(name.clone(), plain_all(args), attr.clone())
            }
            TyKind::Interface(name, args, assoc, attr) => crate::Ty::Interface(
                name.clone(),
                plain_all(args),
                assoc
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.to_plain()))
                    .collect(),
                attr.clone(),
            ),
            TyKind::Enum(name, attr) => crate::Ty::Enum(name.clone(), attr.clone()),
            TyKind::EnumVariant(name, variant, attr) => {
                crate::Ty::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            TyKind::List(inner, attr) => crate::Ty::List(Box::new(inner.to_plain()), attr.clone()),
            TyKind::Map { key, value, attr } => crate::Ty::Map {
                key: Box::new(key.to_plain()),
                value: Box::new(value.to_plain()),
                attr: attr.clone(),
            },
            TyKind::Union(members, attr) => crate::Ty::Union(plain_all(members), attr.clone()),
            TyKind::Function {
                params,
                ret,
                throws,
                attr,
            } => crate::Ty::Function {
                params: params
                    .iter()
                    .map(|param| crate::FunctionParamTy {
                        name: param.name.clone(),
                        ty: param.ty.to_plain(),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(ret.to_plain()),
                throws: Box::new(throws.to_plain()),
                attr: attr.clone(),
            },
            TyKind::Future(value, error, attr) => crate::Ty::Future(
                Box::new(value.to_plain()),
                Box::new(error.to_plain()),
                attr.clone(),
            ),
            TyKind::RustType { attr } => crate::Ty::RustType { attr: attr.clone() },
            TyKind::Type { attr } => crate::Ty::Type { attr: attr.clone() },
            TyKind::Resource { attr } => crate::Ty::Resource { attr: attr.clone() },
            TyKind::PromptAst { attr } => crate::Ty::PromptAst { attr: attr.clone() },
            TyKind::Void { attr } => crate::Ty::Void { attr: attr.clone() },
            TyKind::TypeAlias(name, attr) => crate::Ty::TypeAlias(name.clone(), attr.clone()),
            TyKind::TypeVar(param, attr) => crate::Ty::TypeVar(param.clone(), attr.clone()),
            TyKind::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => crate::Ty::AssociatedTypeProjection {
                base: Box::new(base.to_plain()),
                interface: Box::new(crate::Interface::new(
                    interface.name.clone(),
                    plain_all(&interface.generics),
                    interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), ty.to_plain()))
                        .collect(),
                )),
                member: member.clone(),
                attr: attr.clone(),
            },
            TyKind::Unknown { attr } => crate::Ty::BuiltinUnknown { attr: attr.clone() },
            TyKind::Never { attr } => crate::Ty::Never { attr: attr.clone() },
            TyKind::Error { attr } => crate::Ty::Error { attr: attr.clone() },
            TyKind::Infer { var: _, attr } => crate::Ty::Infer { attr: attr.clone() },
        }
    }
}

// -- Leaf helpers -------------------------------------------------------------

/// A default-attr leaf helper pinned in a `OnceLock`: after first use the
/// call is a refcount bump — no pool lock, no hashing. The pinned handle
/// also keeps the entry's strong count ≥ 2 forever, so leaves never churn
/// through [`Ty::sweep_pool`].
macro_rules! pinned_leaf {
    ($(#[$doc:meta])* $name:ident, $variant:ident) => {
        $(#[$doc])*
        pub fn $name() -> Ty {
            static PINNED: OnceLock<Ty> = OnceLock::new();
            PINNED
                .get_or_init(|| {
                    Ty::intern(TyKind::$variant {
                        attr: TyAttr::default(),
                    })
                })
                .clone()
        }
    };
}

impl Ty {
    pinned_leaf!(int, Int);
    pinned_leaf!(float, Float);
    pinned_leaf!(string, String);
    pinned_leaf!(bool, Bool);
    pinned_leaf!(null, Null);
    pinned_leaf!(never, Never);
    pinned_leaf!(void, Void);
    pinned_leaf!(error, Error);
    pinned_leaf!(
        /// The spec top type (the plain enum's `BuiltinUnknown`).
        unknown,
        Unknown
    );

    pub fn infer_var(var: InferVar) -> Ty {
        Ty::intern(TyKind::Infer {
            var: Some(var),
            attr: TyAttr::default(),
        })
    }

    pub fn list(inner: Ty) -> Ty {
        Ty::intern(TyKind::List(inner, TyAttr::default()))
    }

    pub fn union(members: impl IntoIterator<Item = Ty>) -> Ty {
        Ty::intern(TyKind::Union(
            members.into_iter().collect(),
            TyAttr::default(),
        ))
    }

    /// `T?` is a flat `T | null` union.
    pub fn optional(inner: Ty) -> Ty {
        match inner.kind() {
            TyKind::Union(members, attr) => {
                if members
                    .iter()
                    .any(|member| matches!(member.kind(), TyKind::Null { .. }))
                {
                    inner
                } else {
                    let mut members = members.to_vec();
                    members.push(Ty::null());
                    Ty::intern(TyKind::Union(members.into(), attr.clone()))
                }
            }
            TyKind::Null { .. } => inner,
            _ => Ty::union([inner, Ty::null()]),
        }
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
            P::Class(name(), vec![int()], a()),
            P::Interface(name(), vec![int()], vec![(Name::new("Item"), int())], a()),
            P::Enum(name(), a()),
            P::EnumVariant(name(), Name::new("A"), a()),
            P::List(Box::new(int()), a()),
            P::Map {
                key: Box::new(P::String { attr: a() }),
                value: Box::new(int()),
                attr: a(),
            },
            P::Union(vec![int(), P::Null { attr: a() }], a()),
            P::Function {
                params: vec![crate::FunctionParamTy::required(
                    Some(Name::new("x")),
                    int(),
                )],
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
                interface: Box::new(crate::Interface::new(name(), vec![], vec![])),
                member: Name::new("Item"),
                attr: a(),
            },
            P::BuiltinUnknown { attr: a() },
            P::Never { attr: a() },
            P::Error { attr: a() },
            P::Infer { attr: a() },
        ]
    }

    #[test]
    #[should_panic(expected = "TIR-internal inference sentinels")]
    fn tir_evolving_sentinels_are_unrepresentable() {
        let plain = crate::Ty::EvolvingList(
            Box::new(crate::Ty::Int {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );
        let _ = Ty::from_plain(&plain);
    }

    #[test]
    #[should_panic(expected = "TIR-internal inference sentinels")]
    fn tir_unknown_sentinel_is_unrepresentable() {
        let plain = crate::Ty::Unknown {
            attr: TyAttr::default(),
        };
        let _ = Ty::from_plain(&plain);
    }

    #[test]
    fn roundtrip_every_variant() {
        for plain in plain_samples() {
            let roundtripped = Ty::from_plain(&plain).to_plain();
            assert_eq!(plain, roundtripped);
        }
    }

    #[test]
    fn optional_flattens_union_and_is_idempotent() {
        let optional = Ty::optional(Ty::union([Ty::int(), Ty::string()]));
        let TyKind::Union(members, _) = optional.kind() else {
            panic!("expected union");
        };
        assert_eq!(members.len(), 3);
        assert!(members.iter().any(|member| member == &Ty::int()));
        assert!(members.iter().any(|member| member == &Ty::string()));
        assert!(
            members
                .iter()
                .any(|member| matches!(member.kind(), TyKind::Null { .. }))
        );
        assert!(
            !members
                .iter()
                .any(|member| matches!(member.kind(), TyKind::Union(..)))
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
        let TyKind::List(elem, _) = list.kind() else {
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
    fn union_flag_propagates() {
        assert!(Ty::union([Ty::int(), Ty::string()]).has_union());
        assert!(Ty::list(Ty::optional(Ty::int())).has_union());
        assert!(!Ty::list(Ty::int()).has_union());
        assert!(!Ty::int().has_union());
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
    fn map_children_preserving_matches_map_children_and_preserves_identity() {
        let var = Ty::infer_var(InferVar::new(11));
        // Children in every satellite position: function params/ret/throws,
        // projection base + interface generics + associated types.
        let samples: Vec<Ty> = plain_samples().iter().map(Ty::from_plain).collect();
        let deep = Ty::intern(TyKind::Function {
            params: [FunctionParam::required(None, var.clone())].into(),
            ret: Ty::intern(TyKind::AssociatedTypeProjection {
                base: var.clone(),
                interface: InterfaceRef::new(
                    TypeName::local(Name::new("I")),
                    [var.clone()].into(),
                    vec![(Name::new("Item"), var.clone())],
                ),
                member: Name::new("Item"),
                attr: TyAttr::default(),
            }),
            throws: Ty::never(),
            attr: TyAttr::default(),
        });

        for ty in samples.iter().chain([&deep]) {
            // Identity mapping returns the ORIGINAL handle, calling f once
            // per child.
            let mut calls_a = 0;
            let same = ty.map_children_preserving(|child| {
                calls_a += 1;
                child.clone()
            });
            assert!(same == *ty);

            // A real substitution agrees with the plain map_children +
            // intern road, and visits the same children in the same order.
            let mut calls_b = 0;
            let subst = |child: &Ty| {
                if child == &var {
                    Ty::string()
                } else {
                    child.clone()
                }
            };
            let preserving = ty.map_children_preserving(|child| {
                calls_b += 1;
                subst(child)
            });
            let baseline = Ty::intern(ty.kind().map_children(subst));
            assert!(preserving == baseline);
            assert_eq!(calls_a, calls_b);
        }
    }

    #[test]
    fn ordering_matches_plain_ordering() {
        let mut plain = plain_samples();
        let mut interned: Vec<Ty> = plain.iter().map(Ty::from_plain).collect();
        plain.sort();
        interned.sort();
        let materialized: Vec<crate::Ty> = interned.iter().map(Ty::to_plain).collect();
        assert_eq!(plain, materialized);
    }

    /// Whether the pool currently holds an entry for `kind`. Test-only; the
    /// pool is global and shared with concurrently running tests, so tests
    /// probe unique keys instead of asserting pool sizes.
    fn pool_contains(kind: &TyKind) -> bool {
        let data = TyData {
            flags: compute_flags(kind),
            kind: kind.clone(),
        };
        pool().lock().unwrap().contains(&data)
    }

    #[test]
    fn sweep_reclaims_dead_entries_transitively() {
        // Unique probe kinds so concurrent tests can't intern equal entries.
        // Only handle-free kinds (literals) are kept across the sweep for
        // probing — a `TyKind::List` probe would itself hold a child handle
        // and keep the leaf alive.
        let leaf_kind = TyKind::Literal(
            Literal::String("interned-sweep-probe-leaf".into()),
            Freshness::Regular,
            TyAttr::default(),
        );
        let keep_kind = TyKind::Literal(
            Literal::String("interned-sweep-probe-keep".into()),
            Freshness::Regular,
            TyAttr::default(),
        );
        let keep = Ty::intern(keep_kind.clone());
        {
            let leaf = Ty::intern(leaf_kind.clone());
            let _parent = Ty::intern(TyKind::List(leaf, TyAttr::default()));
        } // both handles dropped; pool entries linger (parent's entry still
        // holds a child handle to the leaf's entry)
        assert!(pool_contains(&leaf_kind), "drop must not evict");

        // The leaf can only die after the parent entry is freed (the parent
        // holds a child handle to it), so leaf eviction proves the sweep is
        // transitive across passes. Live entries must survive.
        Ty::sweep_pool();
        assert!(
            !pool_contains(&leaf_kind),
            "sweep must transitively evict the dead parent then the dead leaf"
        );
        assert!(pool_contains(&keep_kind), "sweep must keep live entries");

        // Pinned leaf helpers survive sweeps: the static handle keeps them live.
        let int_before = Ty::int();
        Ty::sweep_pool();
        assert!(int_before == Ty::int());

        // Re-interning after eviction works.
        let again = Ty::intern(leaf_kind.clone());
        assert!(pool_contains(&leaf_kind));
        drop(again);
        drop(keep);
    }
}
