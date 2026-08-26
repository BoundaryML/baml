use std::sync::{Arc, atomic::AtomicBool};

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{
    AtomicValueSlot, HeapPtr, ObjectIndex, RuntimeCompileDiagnostic, TyTemplate, Value,
    types::interface::InterfaceBound,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct LocalName {
    pub namespace: Vec<Name>,
    pub name: Name,
}

/// A package object on the heap.
/// Contains lookups for named items defined in the package.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Package {
    /// Every source-visible exported declaration name, including aliases that
    /// have no heap object of their own.
    pub exported_names: Vec<LocalName>,
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
    /// Exported free functions, keyed by their package-local name. This is the
    /// runtime projection of the package surface, shared by static and dynamic
    /// packages so reflection never has to deserialize compiler IR.
    pub functions: IndexMap<LocalName, HeapPtr>,
    /// Recursive type aliases defined in the package, each an
    /// `Object::TypeAlias`. Non-recursive aliases are expanded at lowering and
    /// never reach here.
    pub type_aliases: IndexMap<LocalName, HeapPtr>,
    /// Versioned artifact containing the enriched, source-less compiler
    /// interface for mounting this package under an alias in a later
    /// `Package.compile` call.
    pub interface_blob: Vec<u8>,
    /// Compiler-synthesized test registrar for this package, when it has tests.
    pub test_init: Option<HeapPtr>,
    /// Exact runtime type values attached by `Package.with_types`.
    #[borsh(skip)]
    pub mounted_types: IndexMap<String, HeapPtr>,
    /// Runtime-only state, discriminated so a package cannot be both an
    /// ordinary runtime package and a Session (or a Session without an image).
    #[borsh(skip)]
    pub kind: PackageKind,
}

/// The three legal runtime shapes of a [`Package`].
#[derive(Clone, Debug, Default)]
pub enum PackageKind {
    /// A package loaded from the serialized program image.
    #[default]
    Static,
    /// A package produced by `reflect.Package.compile`.
    Runtime(Box<RuntimePackage>),
    /// The package-shaped runtime image and persistent state owned by a Session.
    Session {
        runtime: Box<RuntimePackage>,
        state: Box<SessionState>,
    },
}

impl Package {
    pub fn runtime(&self) -> Option<&RuntimePackage> {
        match &self.kind {
            PackageKind::Static => None,
            PackageKind::Runtime(runtime) | PackageKind::Session { runtime, .. } => Some(runtime),
        }
    }

    pub fn runtime_mut(&mut self) -> Option<&mut RuntimePackage> {
        match &mut self.kind {
            PackageKind::Static => None,
            PackageKind::Runtime(runtime) | PackageKind::Session { runtime, .. } => Some(runtime),
        }
    }

    pub fn session(&self) -> Option<&SessionState> {
        match &self.kind {
            PackageKind::Session { state, .. } => Some(state),
            PackageKind::Static | PackageKind::Runtime(_) => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut SessionState> {
        match &mut self.kind {
            PackageKind::Session { state, .. } => Some(state),
            PackageKind::Static | PackageKind::Runtime(_) => None,
        }
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

/// Runtime-only package image grafted into the moving heap.
#[derive(Clone, Debug)]
pub struct RuntimePackage {
    /// Linked local object table. Imported entries point into static or other
    /// runtime packages; owned entries point back into this package's graph.
    pub objects: Box<[HeapPtr]>,
    /// Newest-wins dynamic object link table. Old objects stay in `objects`,
    /// while later submissions resolve a repeated source name to this entry.
    pub object_names: IndexMap<String, HeapPtr>,
    /// Package-local global slots, mutable only while `$init` is running.
    pub globals: Box<[AtomicValueSlot]>,
    /// Fully-qualified function/let name to this image's local global slot.
    pub global_names: IndexMap<String, usize>,
    /// Created-once reflected class, enum, and interface type values, keyed by
    /// the declaration each one names.
    ///
    /// A runtime declaration is not in the program image, so a `LoadType` that
    /// names one must reach the value allocated at package load rather than
    /// build a fresh equal-looking one — same declaration, same `type` object.
    /// The declaration pointer is that identity, and it is exactly what the
    /// type's head already carries, so the lookup is the head itself. (Keying
    /// by rendered FQN made two declarations that merely printed alike
    /// indistinguishable.)
    pub type_values: IndexMap<HeapPtr, HeapPtr>,
    /// Compiler warnings retained on a successful package.
    pub diagnostics: Vec<RuntimeCompileDiagnostic>,
    /// Runtime package objects imported by this image.
    pub dependencies: Box<[HeapPtr]>,
    /// Direct import alias to runtime package. Kept alongside the dense list so
    /// runtime type names such as `dep.models.Base` resolve by their compiler
    /// package identity.
    pub dependency_names: IndexMap<String, HeapPtr>,
    /// The candidate `$init`, if one exists.
    pub init: Option<HeapPtr>,
    /// False while `$init` may write package globals; true after commit. A
    /// Session keeps this false because its globals remain mutable across evals.
    pub initialized: bool,
}

impl RuntimePackage {
    pub fn load_global(&self, index: usize) -> Option<Value> {
        self.globals.get(index).map(AtomicValueSlot::load)
    }
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
    pub exported_names: Vec<LocalName>,
    pub classes: IndexMap<LocalName, ObjectIndex>,
    pub enums: IndexMap<LocalName, ObjectIndex>,
    pub interfaces: IndexMap<LocalName, ObjectIndex>,
    /// Exported free functions only (methods and compiler helpers are excluded).
    pub functions: IndexMap<LocalName, ObjectIndex>,
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
}

impl ProgramPackage {
    /// Sort every per-kind map and each impl-rule list into the content-determined
    /// order the serialized `Program` requires, so the bytes are reproducible
    /// regardless of the source maps' iteration order (`type_aliases` in
    /// particular is sourced from a per-process-seeded `std::HashMap`).
    ///
    /// Impl rules key on their rendered `for_ty_pattern`; that `Display` drops
    /// module paths, so `{:?}` (module-qualified identity) breaks ties, and the
    /// interface instantiation (args + associated bindings) is folded in last so
    /// the same for-type implementing one interface at several instantiations
    /// orders by content rather than declaration order.
    ///
    /// The full-compile emit and the incremental linker both apply this so their
    /// `Program`s stay byte-identical.
    pub fn sort_maps(&mut self) {
        self.exported_names.sort();
        self.exported_names.dedup();
        self.classes.sort_keys();
        self.enums.sort_keys();
        self.type_aliases.sort_keys();
        self.interfaces.sort_keys();
        self.functions.sort_keys();
        self.impl_rules.sort_keys();
        for rules in self.impl_rules.values_mut() {
            rules.sort_by_cached_key(|rule| {
                (
                    rule.for_ty_pattern.to_string(),
                    format!("{:?}", rule.for_ty_pattern),
                    format!("{:?}", rule.interface_args),
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
    /// pass in [`ProgramPackage::sort_maps`].
    pub field_links: Box<[u32]>,
}

/// The global-index-keyed twin of [`MethodImpl`](super::MethodImpl); `fqn` is the
/// callee function's `ObjectIndex`, resolved to a `HeapPtr` at load.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImpl {
    pub fqn: ObjectIndex,
    pub frame: Vec<TyTemplate>,
}
