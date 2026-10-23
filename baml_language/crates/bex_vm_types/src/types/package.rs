use std::{
    hash::{Hash, Hasher},
    sync::{Arc, atomic::AtomicBool},
};

use baml_base::{LangPackage, Name};
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

    /// This path under a package labelled `package`: the rendering
    /// `package.ns.Item`. The one place a path and a package's display label
    /// are put together. What comes out is for display, as the label is
    /// ([`Package::name`]): two declarations may render alike.
    #[must_use]
    pub fn under(&self, package: &Name) -> baml_type::TypeName {
        baml_type::TypeName::new(package.clone(), self.namespace.clone(), self.name.clone())
    }

    /// This path rendered under a package labelled `label`: `label.ns.Item`,
    /// or `ns.Item` with no label. Display only, as the label is.
    #[must_use]
    pub fn render_under(&self, label: Option<&Name>) -> String {
        match label {
            Some(label) => format!("{label}.{self}"),
            None => self.to_string(),
        }
    }
}

/// A [`LocalName`] borrowed in parts, for looking a declaration up in a
/// package's tables without building the owned key: hashes and compares as
/// the `LocalName` with the same namespace and name does.
#[derive(Debug, Clone, Copy)]
pub struct LocalNameRef<'a> {
    pub namespace: &'a [Name],
    pub name: &'a Name,
}

impl Hash for LocalNameRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Field by field, as `LocalName`'s derived `Hash` does; a `Vec<Name>`
        // hashes as its slice.
        self.namespace.hash(state);
        self.name.hash(state);
    }
}

impl indexmap::Equivalent<LocalName> for LocalNameRef<'_> {
    fn equivalent(&self, key: &LocalName) -> bool {
        key.namespace == self.namespace && key.name == *self.name
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
    /// A display label for the package: the name its manifest (`baml.toml`)
    /// declares, or a stand-in when it declares none.
    ///
    /// For lossy display only: a trace, a diagnostic, a declaration's
    /// rendered name. It is neither unique nor identifying. A heap holds
    /// many packages, any number of which carry the same label — every
    /// runtime-compiled package is labelled `user`, as an unnamed one is. A
    /// package's identity is its pointer. Do not key or compare by this.
    ///
    /// It promises nothing about how the package is addressed. A package
    /// refers to itself as `root`, and another package reaches it under the
    /// name on its own edge ([`Self::edges`]). Whether the label also
    /// reaches the package — from inside it, or from a host — is not
    /// decided either way, so new code must not rely on it and must not
    /// rely on its absence.
    //
    // NOTE: that question is live because the label does reach the package
    // today. `accessible` answers it with the package itself, which is how
    // a host's name for one of the root's own declarations (`user.Foo`)
    // resolves, and `Program::rendered_callables` lists the root's callables
    // under it.
    //
    // BUG: a declaration's rendered name (`declaration_name`) still crosses
    // the host boundary as a wire name, and the engine's class and enum
    // tables are keyed by it — so two packages carrying one label collide
    // there.
    pub name: Name,
    /// What the package is ([`PackageKind`]): which language package — one
    /// of the prelude every package reaches under a fixed name — or an
    /// ordinary one. A fact about the package, carried from the compiler's
    /// installation table by the linker and set by whatever creates a package
    /// at run time; never decided from a name or from how an edge reaches it.
    pub kind: PackageKind,
    /// The package's edge table: every package it reaches, by the name it
    /// reaches it under — its declared dependencies, then the language
    /// packages under their fixed names. The runtime mirror of the compiler's
    /// edge table, and the only way a name becomes a package at run time: a
    /// host names a package as the world's root reaches it, and a runtime
    /// package's wire names resolve through its own edges. Names live on
    /// edges, so two packages may share a spelling. What an edge reaches —
    /// a language package, an ordinary one, an anonymous declaration — is
    /// read off the target; the edge itself records only how the target's
    /// exports relate to this package ([`EdgeKind`]).
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

/// What a package is, as opposed to how another package reaches it
/// ([`EdgeKind`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PackageKind {
    /// One of the standard library's packages, and which: shipped with the
    /// toolchain and reached by every package under its fixed name without
    /// being declared — the prelude. A dependency slot binds one by this identity
    /// and carries no interface fingerprint (`Locator::Prelude`). The
    /// language's own implementation reaches the package by this identity
    /// (a [`LangItem`](baml_base::LangItem) names a declaration of it), so
    /// an executable holds at most one package per language package.
    Language(LangPackage),
    /// Any other package: the program's root or one of its dependencies, a
    /// `Package.compile` result, a session, a `with_types` view.
    #[default]
    Ordinary,
}

/// How the exports of what an edge points at relate to the package holding
/// the edge: reachable under the edge's name (a manifest dependency, a
/// `Package.compile` mount, the prelude under its fixed name, an anonymous
/// declaration a `with_types` view exports — the edge's target is then the
/// declaration itself, which the compile seam treats as a root of one row
/// and the loader binds a dependency slot to); or additionally re-exported
/// wholesale (a `with_types` view reaches its base this way: every export of
/// the target is an export of the package — Rust's `pub use base::*`).
/// What the target IS — a language package, an ordinary one, a declaration —
/// is read off the target, never recorded here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum EdgeKind {
    Dependency,
    ReExported,
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

/// An entry of a package's edge table on the heap: a package, or a class or
/// enum declaration belonging to no package (a `with_types` view's minted
/// declaration); the object at `target` says which.
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
    /// An ordinary package labelled `name` that declares, reaches and runs
    /// nothing, with tables of its own.
    #[must_use]
    pub fn empty(name: Name) -> Self {
        Self {
            name,
            kind: PackageKind::Ordinary,
            edges: IndexMap::new(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            type_aliases: IndexMap::new(),
            globals: IndexMap::new(),
            slots: Slots::Own {
                cells: Box::new([]),
                initialized: true,
            },
            objects: Objects::Own(Box::new([])),
            surface: ExportSurface::Projected,
            init: None,
            test_init: None,
            diagnostics: Vec::new(),
            session: None,
        }
    }

    /// The package `own` — this package, by its own pointer — spells as
    /// `name`: itself by its own name, else the package its edge `name`
    /// reaches. Anything else is invisible from here.
    ///
    /// The compiler spells a package's own declarations `root.…` only
    /// (`hir::package::path_package`); the self-name clause here serves
    /// HOST-supplied names (`user.main`), which spell the root by its label
    /// until the SDK boundary refers to declarations by image identity.
    /// Nothing compiled relies on it.
    pub fn accessible(&self, own: HeapPtr, name: &Name) -> Option<HeapPtr> {
        if self.name == *name {
            return Some(own);
        }
        self.edges.get(name).map(|edge| edge.target)
    }

    /// The class, enum, interface, or recursive type alias the package
    /// declares at `namespace`.`name`, if any. The one type namespace: a path
    /// names at most one declaration.
    pub fn declaration(&self, namespace: &[Name], name: &Name) -> Option<HeapPtr> {
        let key = LocalNameRef { namespace, name };
        self.classes
            .get(&key)
            .or_else(|| self.enums.get(&key))
            .or_else(|| self.interfaces.get(&key))
            .or_else(|| self.type_aliases.get(&key))
            .copied()
    }

    /// Everything this package reaches, in edge order: ordinary packages,
    /// the prelude, a re-exported base, anonymous declarations alike. What
    /// each target is, the object at it says ([`PackageKind`], or a class or
    /// enum for an anonymous declaration).
    pub fn dependencies(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.edges.values().map(|edge| edge.target)
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
    /// The package's display label — [`Package::name`], and under the same
    /// contract: lossy display only, neither unique nor identifying. The
    /// package's identity in the executable is its position in
    /// [`Program::packages`](crate::Program::packages), and two packages may
    /// carry the same label.
    pub name: Name,
    /// What the package is — [`Package::kind`], from the compiler's
    /// installation table: which language package, or an ordinary one.
    pub kind: PackageKind,
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
    /// The pool objects the package's units and tails were placed as — the
    /// linker lays a package out contiguously. Every object in the range
    /// belongs to this package: its declarations, named functions and
    /// bodies, its initializers and their helpers, its closure bodies; no
    /// other package's object lies in it. What the loader binds every
    /// function's owner from, and `Program::validate` holds every table to.
    /// A pooled object past every package's range (a program a test extends
    /// by hand) is owned by none.
    pub objects: std::ops::Range<u32>,
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

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use indexmap::IndexMap;

    use super::{LocalName, LocalNameRef};
    use crate::HeapPtr;

    /// A borrowed key finds the owned entry: the two hash alike and compare
    /// equal, so a package table is read without building a `LocalName`.
    #[test]
    fn a_borrowed_local_name_finds_the_owned_entry() {
        let mut table: IndexMap<LocalName, HeapPtr> = IndexMap::new();
        let table_entry = LocalName::new(vec![Name::new("toml")], Name::new("Table"));
        let root_entry = LocalName::new(Vec::new(), Name::new("Arg"));
        table.insert(table_entry, HeapPtr::null());
        table.insert(root_entry, HeapPtr::null());

        let toml = [Name::new("toml")];
        assert!(
            table
                .get(&LocalNameRef {
                    namespace: &toml,
                    name: &Name::new("Table"),
                })
                .is_some()
        );
        assert!(
            table
                .get(&LocalNameRef {
                    namespace: &[],
                    name: &Name::new("Arg"),
                })
                .is_some()
        );
        // A different namespace or name is a different declaration.
        assert!(
            table
                .get(&LocalNameRef {
                    namespace: &[],
                    name: &Name::new("Table"),
                })
                .is_none()
        );
        assert!(
            table
                .get(&LocalNameRef {
                    namespace: &toml,
                    name: &Name::new("Arg"),
                })
                .is_none()
        );
    }
}
