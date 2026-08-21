use std::sync::{Arc, atomic::AtomicBool};

use baml_base::Name;
use baml_type::{RuntimeTy, TyTemplate};
use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{
    AtomicValueSlot, HeapPtr, ObjectIndex, RuntimeCompileDiagnostic, Value,
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
    pub recursive_type_aliases: IndexMap<LocalName, RuntimeTy>,
    /// Enriched, source-less compiler interface for mounting this package under
    /// an alias in a later `Package.compile` call.
    pub interface_blob: Vec<u8>,
    /// Compiler-synthesized test registrar for this package, when it has tests.
    pub test_init: Option<HeapPtr>,
    /// Exact runtime type values attached by `Package.with_types`.
    #[borsh(skip)]
    pub mounted_types: IndexMap<String, HeapPtr>,
    /// Present only for a package produced by `reflect.Package.compile`.
    #[borsh(skip)]
    pub runtime: Option<Box<RuntimePackage>>,
    /// Present only for the package-shaped payload owned by a Session.
    #[borsh(skip)]
    pub session: Option<Box<SessionState>>,
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
    /// source-visible FQN.
    pub type_values: IndexMap<String, HeapPtr>,
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
    /// False while `$init` may write package globals; true after commit.
    pub initialized: bool,
    /// The mint that makes this image's own declaration names unique.
    ///
    /// The compiler names every runtime-compiled package's `Item` `user.Item`,
    /// so at load the image is re-spelled `user.$dyn.<mint>.Item`
    /// (`bex_vm_types::rename`). The package's own `LocalName` tables stay
    /// keyed by the *source* name, so a lookup arriving with a minted qualified
    /// name is translated back through [`Self::source_local_name`].
    ///
    /// `None` for an image whose declarations were never re-spelled — a Session,
    /// whose submissions are already scoped to the one Session that owns them.
    pub mint: Option<u64>,
}

impl RuntimePackage {
    pub fn load_global(&self, index: usize) -> Option<Value> {
        self.globals.get(index).map(AtomicValueSlot::load)
    }

    /// The key `qtn` has in this package's own declaration tables, or `None`
    /// when `qtn` cannot name a declaration of this package.
    ///
    /// A name minted by *this* package drops its hidden discriminator; a name
    /// minted by another one names a foreign declaration and is refused, which
    /// is what keeps two packages' same-named `Item`s apart.
    pub fn source_local_name(&self, qtn: &baml_type::TypeName) -> Option<LocalName> {
        if qtn.is_runtime_minted() && !self.mint.is_some_and(|mint| qtn.has_runtime_mint(mint)) {
            return None;
        }
        Some(LocalName {
            namespace: qtn.source_namespace().to_vec(),
            name: qtn.name().clone(),
        })
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
    pub recursive_type_aliases: IndexMap<LocalName, RuntimeTy>,
    /// `borsh(PackageInterface)`, captured at build time and embedded in packs.
    pub interface_blob: Vec<u8>,
    /// The package's synthesized `$init_test`, if present.
    pub test_init: Option<ObjectIndex>,
}

impl ProgramPackage {
    /// Sort every per-kind map and each impl-rule list into the content-determined
    /// order the serialized `Program` requires, so the bytes are reproducible
    /// regardless of the source maps' iteration order (`recursive_type_aliases` in
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
        self.recursive_type_aliases.sort_keys();
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
