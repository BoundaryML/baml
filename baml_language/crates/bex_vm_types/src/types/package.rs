use std::sync::{Arc, atomic::AtomicBool};

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{
    AtomicValueSlot, GlobalIndex, HeapPtr, ObjectIndex, RuntimeCompileDiagnostic, TyTemplate,
    types::{DeclPath, interface::InterfaceBound},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct LocalName {
    pub namespace: Vec<Name>,
    pub name: Name,
}

impl LocalName {
    pub fn new(namespace: Vec<Name>, name: Name) -> Self {
        Self { namespace, name }
    }
}

/// Dotted, package-relative: `ns.sub.Item`.
impl std::fmt::Display for LocalName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for segment in &self.namespace {
            write!(f, "{}.", segment.as_str())?;
        }
        f.write_str(self.name.as_str())
    }
}

/// A package object on the heap: what every package has, whatever compiled
/// it — its tables, its edges, its objects and cells, its initializers. No
/// field's meaning depends on how the package was compiled; a static package
/// finds its objects and cells in the program's pools, a runtime package in
/// tables of its own ([`Objects`], [`Slots`]).
///
/// A package has no wire form: [`ProgramPackage`] is the executable's record
/// of a static package, and the loader allocates this from it.
#[derive(Debug, Clone)]
pub struct Package {
    /// The package's display spelling: what traces and the reflection surface
    /// print it under, and the name it spells itself by from its own
    /// viewpoint ([`Self::accessible`]) — the reserved `user` for an unnamed
    /// root, which is how a host addresses it. Display data: a package's
    /// identity is its pointer.
    pub name: Name,
    /// The package's edge table: every package it reaches, by the name it
    /// reaches it under — its declared dependencies, then the language
    /// packages under their fixed names ([`EdgeKind`]). The runtime mirror of
    /// the compiler's edge table, and the only way a name becomes a package
    /// at run time: a host names a package as the world's root reaches it,
    /// and a runtime package's wire names resolve through its own edges.
    /// Names live on edges, so two packages may share a spelling. An edge
    /// of kind [`EdgeKind::ReExported`] makes every export of its target an
    /// export of this package too (a `with_types` view of its base).
    pub edges: IndexMap<Name, Edge>,
    /// Classes defined in the package.
    pub classes: IndexMap<LocalName, HeapPtr>,
    /// Enums defined in the package.
    pub enums: IndexMap<LocalName, HeapPtr>,
    /// Interfaces defined in the package.
    pub interfaces: IndexMap<LocalName, HeapPtr>,
    /// Implementation rules defined in the package.
    /// May include implementations for interfaces in the package's dependencies.
    /// key references an `Object::Interface` and each value is an `Object::ImplRule`
    pub impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
    /// Recursive type aliases defined in the package, each an
    /// `Object::TypeAlias`. Non-recursive aliases are expanded at lowering and
    /// never reach here.
    pub type_aliases: IndexMap<LocalName, HeapPtr>,
    /// The package's own cells by declaration path, as ordinals into
    /// [`Self::slots`]: every named function (free or method), every `let`,
    /// and every interface body under its structural key. A function's or
    /// body's cell holds its object, so this is also how a consumer reaches
    /// one without a rendered spelling; an intrinsic owns no cell. A body's
    /// entry is package-private: the package's tail and a later session
    /// submission reach a body through it, while across packages a default
    /// body is reached through its interface's bound default, and a provided
    /// body is dispatched through its rule, never addressed.
    pub globals: IndexMap<DeclPath, u32>,
    /// Where the package's cells live.
    pub slots: Slots,
    /// Where the package's code finds its objects.
    pub objects: Objects,
    /// How a dependent's compiler learns what this package exports.
    pub surface: ExportSurface,
    /// The package's `$init`, when it has `let`s to run.
    pub init: Option<HeapPtr>,
    /// Compiler-synthesized test registrar for this package, when it has tests.
    pub test_init: Option<HeapPtr>,
    /// Compiler warnings retained on the package. A static package's were
    /// reported at compile time; it retains none.
    pub diagnostics: Vec<RuntimeCompileDiagnostic>,
    /// The persistent state of a Session, for the package a Session owns.
    pub session: Option<Box<SessionState>>,
}

/// How a package reaches what an edge points at: a package it declared (a
/// manifest edge or a `Package.compile` mount); a package it re-exports
/// wholesale (a `with_types` view reaches its base this way: every export of
/// the target is an export of the package — Rust's `pub use base::*`); a
/// declaration belonging to no package that it exports (a `with_types` view
/// reaches a minted class this way — the edge's target is the declaration
/// itself, which the compile seam treats as a root of one row and the loader
/// binds a dependency slot to); or the language's prelude, which every
/// package reaches under fixed names without declaring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum EdgeKind {
    Declared,
    ReExported,
    Anonymous,
    Prelude,
}

/// How a dependent's compiler learns what a package exports.
#[derive(Debug, Clone)]
pub enum ExportSurface {
    /// The versioned `PackageInterface` artifact the package's compile
    /// exported: what a dependent compiles against when the package is
    /// mounted.
    Compiled(Vec<u8>),
    /// The package had no compile — it is a `with_types` view, or the owner
    /// of runtime-minted declarations — and its exports are projected from
    /// its tables when a dependent compiles against it: its own classes,
    /// enums and aliases, the declarations it re-exports, its impl rules, and
    /// every export of a [`EdgeKind::ReExported`] edge's target.
    Projected,
}

/// An entry of a package's edge table on the heap: a package, or — for
/// [`EdgeKind::Anonymous`] — a class or enum declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub target: HeapPtr,
    pub kind: EdgeKind,
}

/// An entry of the executable's package record's edge table: the package it
/// reaches, as an ordinal into [`Program::packages`](crate::Program::packages).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramEdge {
    pub name: Name,
    pub target: u32,
    pub kind: EdgeKind,
}

/// Where a package's code finds its objects — one meaning for every kind of
/// package.
#[derive(Debug, Clone)]
pub enum Objects {
    /// The program's compile-time pool, which static bytecode addresses
    /// absolutely.
    Program,
    /// The package's own table, grown by each graft: what its bytecode's
    /// image-local operands index.
    Own(Box<[HeapPtr]>),
}

impl Objects {
    /// The package's own table, if it owns one.
    pub fn own(&self) -> Option<&[HeapPtr]> {
        match self {
            Self::Own(objects) => Some(objects),
            Self::Program => None,
        }
    }
}

/// Where a package's cells live — one meaning for every kind of package,
/// addressed by the ordinals in [`Package::globals`].
#[derive(Debug, Clone)]
pub enum Slots {
    /// Cells in the program's global pool from `base`: the linker lays a
    /// static package's cells out contiguously, functions then `let`s. The
    /// pool guards its own initialization window.
    Program { base: GlobalIndex },
    /// The package's own table, grown by each graft.
    Own {
        cells: Box<[AtomicValueSlot]>,
        /// False while the package's `$init` may write the cells; true after
        /// it commits. A Session keeps this false because its cells stay
        /// mutable across evals.
        initialized: bool,
    },
}

impl Slots {
    /// The package's own table, if it owns one.
    pub fn own(&self) -> Option<&[AtomicValueSlot]> {
        match self {
            Self::Own { cells, .. } => Some(cells),
            Self::Program { .. } => None,
        }
    }
}

impl Package {
    /// The package `own` — this package, by its own pointer — spells as
    /// `name`: itself by its own name, else the package its edge `name`
    /// reaches. Anything else is invisible from here; the runtime mirror of
    /// the compiler's `accessible_package`.
    pub fn accessible(&self, own: HeapPtr, name: &Name) -> Option<HeapPtr> {
        if self.name == *name {
            return Some(own);
        }
        self.edges.get(name).map(|edge| edge.target)
    }

    /// The packages this one declared as dependencies, in declaration order
    /// — its package edges minus the prelude, a re-exported base included.
    pub fn declared(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.edges
            .values()
            .filter(|edge| matches!(edge.kind, EdgeKind::Declared | EdgeKind::ReExported))
            .map(|edge| edge.target)
    }

    /// The declarations belonging to no package this one exports, in edge
    /// order: the targets of its [`EdgeKind::Anonymous`] edges.
    pub fn anonymous(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.edges
            .values()
            .filter(|edge| edge.kind == EdgeKind::Anonymous)
            .map(|edge| edge.target)
    }

    /// The packages every export of which is also an export of this one, in
    /// edge order: the targets of its [`EdgeKind::ReExported`] edges.
    pub fn reexported(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.edges
            .values()
            .filter(|edge| edge.kind == EdgeKind::ReExported)
            .map(|edge| edge.target)
    }

    pub fn session(&self) -> Option<&SessionState> {
        self.session.as_deref()
    }

    pub fn session_mut(&mut self) -> Option<&mut SessionState> {
        self.session.as_deref_mut()
    }
}

/// Compiler-free persistent state of one `reflect.Session`.
#[derive(Clone, Debug)]
pub struct SessionState {
    /// Committed, hygienically lowered source, replayed into every fresh DB.
    pub history: IndexMap<String, String>,
    /// Newest source-visible name to its persistent generated symbol.
    pub visible: IndexMap<String, crate::SessionVisibleSymbol>,
    /// Atomic single-eval admission bit shared with RAII compile artifacts.
    pub busy: Arc<AtomicBool>,
    pub submission_counter: u64,
}

/// The serialized, global-index-keyed twin of [`Package`]. The `Program` must be
/// `HeapPtr`-free (pointers are runtime-only and there is no heap at emit time),
/// so the emit produces this; the loader allocates the [`Package`] +
/// [`Object::Interface`](super::Object::Interface) /
/// [`Object::ImplRule`](super::Object::ImplRule) from it, resolving each
/// [`ObjectIndex`] to a compile-time `HeapPtr`. Mirrors how classes/enums/
/// functions are carried as pooled objects referenced by index.
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct ProgramPackage {
    /// The package's display spelling, and the name it spells itself by from
    /// its own viewpoint. Display data: the package's identity in the
    /// executable is its position in
    /// [`Program::packages`](crate::Program::packages), and nothing resolves
    /// a spelling program-wide — two packages may share one.
    pub name: Name,
    /// The package's edge table: every package it reaches, by the name it
    /// reaches it under, as ordinals into
    /// [`Program::packages`](crate::Program::packages) — its declared
    /// dependencies, then the language packages under their fixed names.
    /// A name resolves only from some package's viewpoint; a host's names
    /// resolve from the root's ([`Program::root`](crate::Program::root)).
    pub edges: Vec<ProgramEdge>,
    pub classes: IndexMap<LocalName, ObjectIndex>,
    pub enums: IndexMap<LocalName, ObjectIndex>,
    pub interfaces: IndexMap<LocalName, ObjectIndex>,
    /// Implemented-interface `ObjectIndex` → the impl rules of it declared in
    /// this package (may target an interface from a dependency).
    pub impl_rules: IndexMap<ObjectIndex, Vec<ProgramImplRule>>,
    /// Recursive type aliases defined in the package.
    pub type_aliases: IndexMap<LocalName, ObjectIndex>,
    /// Versioned `PackageInterface` artifact captured at build time and
    /// embedded in generated programs.
    pub interface_blob: Vec<u8>,
    /// The package's synthesized `$init_test`, if present.
    pub test_init: Option<ObjectIndex>,
    /// The package's synthesized `$init`, if it has `let`s.
    pub init: Option<ObjectIndex>,
    /// The package's own cells by declaration — every named function (free
    /// or method), every `let`, and every interface body under its
    /// structural key — as ordinals from [`Self::slot_base`]. A function's
    /// or body's cell holds its object. This is how a consumer binds a
    /// function VALUE (`GenericFunction { function: GlobalIndex }`) without a
    /// rendered spelling.
    pub globals: IndexMap<DeclPath, u32>,
    /// The first of the package's cells in the program's global pool: the
    /// linker lays them out contiguously, functions then `let`s.
    pub slot_base: GlobalIndex,
}

impl ProgramPackage {
    /// The program-pool slot of the cell `path` owns, if it owns one.
    #[must_use]
    pub fn global_slot(&self, path: &DeclPath) -> Option<GlobalIndex> {
        self.globals
            .get(path)
            .map(|&ordinal| GlobalIndex::from_raw(self.slot_base.raw() + ordinal as usize))
    }

    /// Canonicalize implementation rules, whose source tables do not carry a
    /// user-observable declaration order. Declaration maps deliberately keep
    /// the deterministic source order established by the compiler pipeline.
    ///
    /// Impl rules key on their rendered `for_ty_pattern`; that `Display` drops
    /// module paths, so `{:?}` (module-qualified identity) breaks ties, and the
    /// interface instantiation (args + associated bindings) is folded in last so
    /// the same for-type implementing one interface at several instantiations
    /// orders by content rather than declaration order.
    ///
    /// The full-compile emit and the incremental linker both apply this so their
    /// `Program`s stay byte-identical.
    pub fn canonicalize_impl_rules(&mut self) {
        self.impl_rules.sort_keys();
        for rules in self.impl_rules.values_mut() {
            rules.sort_by_cached_key(|rule| {
                (
                    rule.for_ty_pattern.to_string(),
                    format!("{:?}", rule.for_ty_pattern),
                    format!("{:?}", rule.interface_args),
                    // Bounds are part of the identity (`ImplCoherenceKey`):
                    // without them, two bound-disjoint same-head rules would
                    // sort by insertion order — nondeterministic bytes the
                    // day coherence admits such a pair.
                    format!("{:?}", rule.generic_param_bounds),
                    format!("{:?}", rule.interface_assoc),
                )
            });
        }
    }
}

/// The global-index-keyed twin of [`RuntimeImplRule`](super::RuntimeImplRule);
/// `interface_head`/`fqn` are `ObjectIndex`es the loader resolves to `HeapPtr`s.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramImplRule {
    pub interface_head: ObjectIndex,
    pub for_ty_pattern: TyTemplate,
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
    pub interface_args: Vec<TyTemplate>,
    pub interface_assoc: Vec<(Name, TyTemplate)>,
    pub methods: IndexMap<Name, ProgramMethodImpl>,
    /// See [`RuntimeImplRule::field_links`](super::RuntimeImplRule::field_links).
    /// Positional, so — unlike the name-keyed maps — it needs no canonical ordering
    /// pass in [`ProgramPackage::canonicalize_impl_rules`].
    pub field_links: Box<[u32]>,
}

/// The impl identity key, per interface: everything coherence's admissibility
/// check discriminates on.
///
/// THE INVARIANT: this key is injective over the set of impls coherence
/// ADMITS — which holds exactly when it carries at least coherence's full
/// discriminant (see `interfaces::coherence` in `baml_compiler2_hir_ty`; the
/// two carry cross-referencing contracts). Under today's open-world regime
/// two same-head impls differing only in (positive) bounds genuinely
/// overlap — a later package can always introduce a type satisfying both —
/// so coherence rejects them and the constraint set never separates two
/// admitted impls. The set is in the key anyway because the REGIME is what
/// that argument depends on: negative bounds would make same-head
/// disjointness provable and specialization would make same-head overlap
/// admissible, and this key must not need rediscovering on that day. If
/// coherence ever gains a discriminant this key lacks, the decompose
/// link-key uniqueness hard-error fires on the first legal program that
/// exercises it — extend BOTH together.
///
/// The impl's own associated BINDINGS are deliberately absent: they are
/// outputs of the match, not inputs to admissibility. Bound-side associated
/// pins are inputs and ride inside each [`InterfaceBound`]. The interface
/// itself is not a field because every consumer already groups per
/// interface; this struct is the per-interface discriminant.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImplCoherenceKey {
    pub for_ty_pattern: TyTemplate,
    pub interface_args: Vec<TyTemplate>,
    /// The canonical constraint set: per impl-frame param in frame order,
    /// that param's bounds canonically sorted (a written reorder of
    /// `A + B` cannot fork the key).
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
}

impl ProgramImplRule {
    /// This rule's identity key. The bake stores the same canonicalized
    /// bounds the key carries, so a rule and its declaring block compare
    /// equal by construction.
    #[must_use]
    pub fn coherence_key(&self) -> ImplCoherenceKey {
        ImplCoherenceKey {
            for_ty_pattern: self.for_ty_pattern.clone(),
            interface_args: self.interface_args.clone(),
            generic_param_bounds: self.generic_param_bounds.clone(),
        }
    }
}

/// The global-index-keyed twin of [`MethodImpl`](super::MethodImpl); `fqn` is the
/// callee function's `ObjectIndex`, resolved to a `HeapPtr` at load.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImpl {
    pub fqn: ObjectIndex,
    pub frame: Vec<TyTemplate>,
}
