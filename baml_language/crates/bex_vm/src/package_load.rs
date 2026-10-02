//! Loading the per-package program structure onto the heap.
//!
//! The compiled `Program` carries packages as a global-index-keyed
//! [`ProgramPackage`] map (it must be `HeapPtr`-free — there is no heap at emit
//! time, and a `HeapPtr` can't serialize). At load we allocate the heap
//! `Object::Package` / `Object::ImplRule` objects from it and build the
//! [`PackageIndex`] (package name → its `Object::Package` pointer, plus the
//! program-wide interface → impl-rules index).
//!
//! These are *cross-referencing compile-time objects* — their `HeapPtr` fields
//! point at interfaces / functions / classes / other packages — and a
//! compile-time `HeapPtr` only exists once the compile-time `Vec` is laid out.
//! So we use the two-phase heap construction ([`BexHeap::build_unsealed`]):
//! reserve a placeholder slot per impl rule and per package, build the heap
//! unsealed, fill each slot with its resolved pointers, then seal.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_type::{MediaKind, Name};
use bex_heap::{BexHeap, Generation};
use bex_vm_types::{
    HeapPtr, Object, ObjectIndex, TypeHead,
    types::{LocalName, MethodImpl, Package, ProgramImplRule, ProgramPackage, RuntimeImplRule},
};
use indexmap::IndexMap;

/// One anonymous-class witness in the dynamic dispatch table.
///
/// Both pointers are **weak**: the table never keeps a minted definition or its
/// rule alive. `class` decides liveness (the witness dies with its class);
/// `rule` is the heap `Object::ImplRule` the resolver borrows — the one
/// authoritative copy, so the collector's fixup of that object is what keeps
/// the rule's own `interface_head`/`methods[].fqn` pointers current. The
/// class keeps its rules alive itself, as the witnesses of its
/// [`Owner::Anonymous`](bex_vm_types::types::Owner::Anonymous).
#[derive(Clone, Copy, Debug)]
pub struct DynRuleEntry {
    pub class: HeapPtr,
    pub rule: HeapPtr,
}

/// Engine-local side tables for runtime-created nominal definitions.
///
/// Anonymous typebuilder classes belong to no package whose `impl_rules` the
/// resolver could walk, so this is where their witnesses are FOUND — keyed by
/// the interface's `Object::Interface` pointer, the same key every package's
/// `impl_rules` map uses. It is a *findability* index only: the class holds
/// its rules, nothing here is a GC root, and every entry is dropped the moment
/// its class is collected.
#[derive(Default, Debug)]
pub struct DynDispatchTables {
    impl_rules: RwLock<IndexMap<HeapPtr, Vec<DynRuleEntry>>>,
}

impl DynDispatchTables {
    pub fn register_rule(&self, interface: HeapPtr, entry: DynRuleEntry) {
        self.impl_rules
            .write()
            .expect("dynamic-impl table lock poisoned")
            .entry(interface)
            .or_default()
            .push(entry);
    }

    /// The witness rules registered for `interface`, as pointers to their heap
    /// `Object::ImplRule`s. Callers borrow the rule through the VM exactly as
    /// they borrow a package-owned one.
    pub fn rules_of(&self, interface: HeapPtr) -> Vec<HeapPtr> {
        self.impl_rules
            .read()
            .expect("dynamic-impl table lock poisoned")
            .get(&interface)
            .into_iter()
            .flatten()
            .map(|entry| entry.rule)
            .collect()
    }

    /// The witness rules registered *for* `class`, as pointers to their heap
    /// `Object::ImplRule`s.
    ///
    /// The forward direction (interface → rules) is what dispatch needs; this
    /// is the reverse, and only reflection asks it — to render a runtime class
    /// back as source. Reading the rules themselves is what keeps the rendered
    /// `implements` blocks and the dispatched ones the same thing: a separate
    /// description of the witness could drift from the witness.
    pub fn rules_for_class(&self, class: HeapPtr) -> Vec<HeapPtr> {
        self.impl_rules
            .read()
            .expect("dynamic-impl table lock poisoned")
            .values()
            .flatten()
            .filter(|entry| entry.class == class)
            .map(|entry| entry.rule)
            .collect()
    }

    /// Update weak pointers after a moving collection and sweep entries whose
    /// owning runtime class died.
    ///
    /// Death is decided by `survived`, not by mere absence from `forwarding`:
    /// a *minor* collection identity-maps every Gen2 object it does not
    /// visit, so an old runtime class reachable only from other old objects is
    /// alive yet absent from the map. Treating absence as death there evicted
    /// live classes on every minor GC. `survived` must answer for exactly the
    /// pointers the collection left in place.
    pub fn sweep_and_forward(
        &self,
        forwarding: &HashMap<HeapPtr, HeapPtr>,
        survived: impl Fn(HeapPtr) -> bool,
    ) {
        // Forward a weak pointer, or report it dead.
        let follow = |ptr: &mut HeapPtr| -> bool {
            if let Some(&forwarded) = forwarding.get(ptr) {
                *ptr = forwarded;
                true
            } else {
                survived(*ptr)
            }
        };
        let mut table = self
            .impl_rules
            .write()
            .expect("dynamic-impl table lock poisoned");
        // The keys are interface pointers, which move too when the interface
        // was declared at runtime — rebuild the map through the forwarding.
        let entries = std::mem::take(&mut *table);
        for (mut interface, mut rules) in entries {
            if !follow(&mut interface) {
                continue;
            }
            rules.retain_mut(|entry| {
                if !follow(&mut entry.class) {
                    return false;
                }
                // The rule is reachable iff its class is; a class that survived
                // keeps its rule alive through the same tracing that found the
                // class, so a live class with an unforwarded rule means the rule
                // sat in a generation the collection left in place.
                let rule_live = follow(&mut entry.rule);
                debug_assert!(rule_live, "a witness rule outlived by its class");
                rule_live
            });
            if !rules.is_empty() {
                table.entry(interface).or_default().extend(rules);
            }
        }
    }

    #[cfg(test)]
    pub fn rule_count(&self) -> usize {
        self.impl_rules
            .read()
            .expect("dynamic-impl table lock poisoned")
            .values()
            .map(Vec::len)
            .sum()
    }
}

/// Permit-holder view. Dynamic entries are weak (no roots), but this holder
/// observes every forwarding map to sweep dead rules and move live pointers.
#[derive(Clone, Debug)]
pub struct DynDispatchRoot {
    pub tables: Arc<DynDispatchTables>,
    heap: Arc<BexHeap>,
}

impl DynDispatchRoot {
    pub fn new(tables: Arc<DynDispatchTables>, heap: Arc<BexHeap>) -> Self {
        Self { tables, heap }
    }
}

impl bex_vm_types::RootHaver for DynDispatchRoot {
    fn collect_roots(&self, _roots: &mut Vec<HeapPtr>) {}

    fn forward_roots(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        // A pointer the collection left in place is one that still lives in
        // Gen2 (a minor collection never moves or frees Gen2; a major moves
        // every survivor to a fresh buffer, so no stale address lands there)
        // or in the compile-time region (never collected). Everything else
        // absent from the map was in a swept generation and is dead. Only the
        // positive range tests are consulted, never the Gen0 fallthrough.
        let heap = Arc::clone(&self.heap);
        self.tables.sweep_and_forward(forwarding, move |ptr| {
            matches!(
                heap.generation_of(ptr),
                Generation::Gen2 | Generation::CompileTime
            )
        });
    }
}

/// The compile-time object slots a package and its impl rules were reserved at,
/// so the fill pass can resolve them to `HeapPtr`s.
struct PackageSlots {
    package_slot: usize,
    /// Parallel to [`ProgramPackage::impl_rules`]: per implemented-interface
    /// index, the slot of each of its rules (same order as the rule vec).
    impl_rule_slots: IndexMap<ObjectIndex, Vec<usize>>,
}

/// A throwaway placeholder for a reserved slot; overwritten before the heap is
/// sealed, so its contents are never observed.
fn placeholder() -> Object {
    Object::ImplRule(Box::new(RuntimeImplRule {
        interface_head: HeapPtr::null(),
        for_ty_pattern: bex_vm_types::TyTemplate::TypeArgRef(0),
        generic_param_bounds: Vec::new(),
        interface_args: Vec::new(),
        interface_assoc: Vec::new(),
        methods: IndexMap::new(),
        field_links: Box::default(),
    }))
}

/// Reserve one placeholder slot per impl rule and one per package, appended to
/// `compile_time_objects`. Returns where each was placed. Appending never shifts
/// the existing emit object indices, so the `ObjectIndex`es baked into
/// `ProgramPackage` stay valid.
fn reserve_package_slots(
    compile_time_objects: &mut Vec<Object>,
    packages: &[ProgramPackage],
) -> Vec<PackageSlots> {
    let mut layout = Vec::with_capacity(packages.len());
    for pkg in packages {
        let mut impl_rule_slots = IndexMap::new();
        for (iface_idx, rules) in &pkg.impl_rules {
            let slots: Vec<usize> = rules
                .iter()
                .map(|_| {
                    let slot = compile_time_objects.len();
                    compile_time_objects.push(placeholder());
                    slot
                })
                .collect();
            impl_rule_slots.insert(*iface_idx, slots);
        }
        let package_slot = compile_time_objects.len();
        compile_time_objects.push(placeholder());
        layout.push(PackageSlots {
            package_slot,
            impl_rule_slots,
        });
    }
    layout
}

/// The loaded program's package tables: every package by ordinal, the root
/// among them, plus the program-wide impl-rule index derived from them.
///
/// The two are built together and kept together because the second is a *view*
/// over the first — pairing an index with any other package set would answer
/// membership for the wrong program. Both fields are private and only the load
/// pass ([`build_heap_with_packages`]) constructs a populated value, so they
/// cannot diverge.
///
/// # Why the impl-rule index is program-wide
///
/// An `implement I for T` is baked into the package whose *source* declares it,
/// which by the orphan rule (RFC-2451 covered; `orphan_check` in the compiler's
/// `interfaces::impl_rules`) need be neither `T`'s package nor `I`'s: a package
/// may anchor the impl on any local type among `[T, I's args…]`, so
/// `implement baml.ops.Add<Meters> for int` legally lives in `Meters`' package.
/// Deriving a narrower search set at query time means re-deriving the orphan
/// rule at runtime and drifting from it — the compiler proves membership the
/// checker accepts, and the VM then fails to find the rule. Indexing every
/// package's rules once, at load, makes that whole failure class
/// unrepresentable: resolution asks one question of one complete table.
///
/// # Compile-time pointers only, immutable after load
///
/// Every pointer here is into the static image: the load pass writes package
/// and impl-rule slots it reserved in the compile-time pool, and nothing
/// writes afterwards — the engine shares the index as a plain `Arc`, with no
/// interior mutability. The static region is never moved and never
/// collected, so the index is neither a root nor traced and cannot dangle.
/// That is also why a runtime package NEVER enters it: a moving pointer here
/// would need rooting (pinning the package forever) or tracing (a fourth
/// forwarding site). Runtime packages are reached through the edges that
/// own them — an artifact's pins, a package's edges, a value's
/// owner — and their impl rules through their own `Package::impl_rules` and
/// the swept dynamic dispatch table, which the resolver chains with this
/// index. A runtime loader binds the prelude through the root's edges.
///
/// # Names resolve from the root
///
/// No spelling identifies a package program-wide: a name reaches a package
/// only through some package's edge table, and a host's names — the run
/// entry point, a fixed stdlib class, a wire type name — resolve from the
/// root's viewpoint ([`Self::accessible`]), exactly as the compiler resolves
/// the root's own source.
#[derive(Default)]
pub struct PackageIndex {
    /// Every package's `Object::Package` pointer, by executable ordinal.
    packages: Vec<HeapPtr>,
    /// The root package's pointer — null only for the empty index of a VM
    /// with no executable, where nothing resolves.
    root: HeapPtr,
    /// Canonical `Object::Interface` pointer → every `Object::ImplRule` of that
    /// interface in the program, in package-load order.
    impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
}

impl PackageIndex {
    /// The `Object::Package` pointer of the package at `ordinal` in the
    /// executable — the position `LoadCurrentPackage` carries.
    pub fn package_at(&self, ordinal: usize) -> Option<HeapPtr> {
        self.packages.get(ordinal).copied()
    }

    /// The root package — the one the executable was compiled for.
    pub fn root(&self) -> HeapPtr {
        self.root
    }

    /// The package the root spells as `name`: itself by its own name, else
    /// the package the root's edge `name` reaches. The one way a
    /// host-supplied name becomes a package.
    pub fn accessible(&self, name: &Name) -> Option<HeapPtr> {
        if self.root.is_null() {
            return None;
        }
        // SAFETY: the index holds compile-time package objects.
        #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
        let root = (unsafe { self.root.get() }).as_package()?;
        root.accessible(self.root, name)
    }

    /// Whether `package` is one of the language packages the root reaches
    /// under a fixed name — the prelude every package reaches without
    /// declaring an edge to it.
    pub fn is_prelude(&self, package: HeapPtr) -> bool {
        baml_builtins2::stdlib_package_names()
            .iter()
            .any(|name| self.accessible(&Name::new(name)) == Some(package))
    }

    /// Every loaded package's `Object::Package` pointer, by ordinal.
    pub fn package_ptrs(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.packages.iter().copied()
    }

    /// Every impl rule of the interface at `iface_ptr`, across all packages.
    /// Empty for an interface nothing implements.
    pub fn impl_rules_of(&self, iface_ptr: HeapPtr) -> &[HeapPtr] {
        self.impl_rules
            .get(&iface_ptr)
            .map_or(&[], |rules| rules.as_slice())
    }
}

/// Resolve a `LocalName → ObjectIndex` member map to `LocalName → HeapPtr`.
fn resolve_members(
    heap: &BexHeap,
    members: &IndexMap<LocalName, ObjectIndex>,
) -> IndexMap<LocalName, HeapPtr> {
    members
        .iter()
        .map(|(ln, idx)| (ln.clone(), heap.compile_time_ptr(idx.into_raw())))
        .collect()
}

/// Build the resolved [`RuntimeImplRule`] for a single [`ProgramImplRule`],
/// turning its `interface_head` / method `fqn` indices into compile-time
/// pointers.
fn resolve_impl_rule(heap: &BexHeap, rule: &ProgramImplRule) -> RuntimeImplRule {
    RuntimeImplRule {
        interface_head: heap.compile_time_ptr(rule.interface_head.into_raw()),
        for_ty_pattern: rule.for_ty_pattern.clone(),
        generic_param_bounds: rule.generic_param_bounds.clone(),
        interface_args: rule.interface_args.clone(),
        interface_assoc: rule.interface_assoc.clone(),
        methods: rule
            .methods
            .iter()
            .map(|(n, m)| {
                (
                    n.clone(),
                    MethodImpl {
                        fqn: heap.compile_time_ptr(m.fqn.into_raw()),
                        frame: m.frame.clone(),
                    },
                )
            })
            .collect(),
        field_links: rule.field_links.clone(),
    }
}

/// Fill every reserved slot with its resolved object, returning the
/// [`PackageIndex`] rooted at the package at `root`.
fn fill_package_slots(
    heap: &mut BexHeap,
    packages: &[ProgramPackage],
    root: u32,
    layout: &[PackageSlots],
) -> PackageIndex {
    // Every package's pointer is known before any is filled: the slots were
    // reserved up front, so an edge can point at a package not yet written.
    let package_ptrs: Vec<HeapPtr> = layout
        .iter()
        .map(|slots| heap.compile_time_ptr(slots.package_slot))
        .collect();
    let mut index = PackageIndex {
        packages: package_ptrs.clone(),
        root: package_ptrs[root as usize],
        impl_rules: IndexMap::new(),
    };
    for (pkg, slots) in packages.iter().zip(layout) {
        // Impl rules first: a package's `impl_rules` map points at their slots.
        let mut impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>> = IndexMap::new();
        for (iface_idx, rules) in &pkg.impl_rules {
            let iface_ptr = heap.compile_time_ptr(iface_idx.into_raw());
            let rule_slots = &slots.impl_rule_slots[iface_idx];
            // The reserve pass allocated one slot per rule from this same map, so
            // the counts must match; a mismatch would leave a reserved slot as a
            // never-overwritten `HeapPtr::null()` placeholder (UB on first deref).
            debug_assert_eq!(
                rules.len(),
                rule_slots.len(),
                "impl-rule reserve/fill count mismatch for an interface",
            );
            let mut rule_ptrs = Vec::with_capacity(rules.len());
            for (rule, &slot) in rules.iter().zip(rule_slots) {
                let resolved = resolve_impl_rule(heap, rule);
                heap.set_compile_time_object(slot, Object::ImplRule(Box::new(resolved)));
                rule_ptrs.push(heap.compile_time_ptr(slot));
            }
            // Mirror the package's rules into the program-wide index as they are
            // resolved. Interfaces are keyed by their canonical pointer, so rules
            // for one interface written across several packages accumulate under
            // the same entry (see [`PackageIndex`] for why the runtime needs the
            // union rather than a per-package view).
            index
                .impl_rules
                .entry(iface_ptr)
                .or_default()
                .extend(rule_ptrs.iter().copied());
            impl_rules.insert(iface_ptr, rule_ptrs);
        }
        let package = Package {
            name: pkg.name.clone(),
            edges: pkg
                .edges
                .iter()
                .map(|edge| {
                    (
                        edge.name.clone(),
                        bex_vm_types::types::Edge {
                            target: package_ptrs[edge.target as usize],
                            kind: edge.kind,
                        },
                    )
                })
                .collect(),
            classes: resolve_members(heap, &pkg.classes),
            enums: resolve_members(heap, &pkg.enums),
            interfaces: resolve_members(heap, &pkg.interfaces),
            impl_rules,
            type_aliases: resolve_members(heap, &pkg.type_aliases),
            globals: pkg.globals.clone(),
            slots: bex_vm_types::types::Slots::Program {
                base: pkg.slot_base,
            },
            objects: bex_vm_types::types::Objects::Program,
            surface: bex_vm_types::types::ExportSurface::Compiled(pkg.interface_blob.clone()),
            init: pkg
                .init
                .map(|index| heap.compile_time_ptr(index.into_raw())),
            test_init: pkg
                .test_init
                .map(|index| heap.compile_time_ptr(index.into_raw())),
            diagnostics: Vec::new(),
            session: None,
        };
        heap.set_compile_time_object(slots.package_slot, Object::Package(Box::new(package)));
        // Every declaration knows its package: the edge a runtime declaration
        // carries to the package that declared it, and what the compile seam
        // reads to say where a re-exported declaration is defined.
        let owner = heap.compile_time_ptr(slots.package_slot);
        for index in pkg
            .classes
            .values()
            .chain(pkg.enums.values())
            .chain(pkg.interfaces.values())
            .chain(pkg.type_aliases.values())
        {
            match heap.compile_time_object_mut(index.into_raw()) {
                Object::Class(class) => class.owner = bex_vm_types::types::Owner::Package(owner),
                Object::Enum(enm) => enm.owner = bex_vm_types::types::Owner::Package(owner),
                Object::Interface(interface) => interface.owner = owner,
                Object::TypeAlias(alias) => alias.owner = owner,
                other => unreachable!(
                    "a package's declaration table names a {:?} object",
                    bex_vm_types::ObjectType::of(other)
                ),
            }
        }
    }
    index
}

/// Look up a class, enum, or interface object pointer by its fully-qualified
/// dotted name — the package as the root reaches it, then the item path —
/// from the root's viewpoint. The free-function form of
/// [`crate::vm::BexVm::lookup_type_by_fqn`], usable before a `BexVm` exists
/// (e.g. to pre-resolve builtin error/panic classes in `BexVm::new`).
pub fn lookup_type_by_fqn(packages: &PackageIndex, fqn: &str) -> Option<HeapPtr> {
    let mut parts: Vec<Name> = fqn.split('.').map(Name::new).collect();
    let name = parts.pop()?;
    if parts.is_empty() {
        return None;
    }
    let pkg = parts.remove(0);
    let pkg_ptr = packages.accessible(&pkg)?;
    // SAFETY: `packages` only ever holds compile-time `Object::Package` pointers.
    #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
    let package = (unsafe { pkg_ptr.get() }).as_package()?;
    let local = LocalName {
        namespace: parts,
        name,
    };
    package
        .classes
        .get(&local)
        .or_else(|| package.enums.get(&local))
        .or_else(|| package.interfaces.get(&local))
        .copied()
}

/// The head of the class, enum, interface, or type alias `fqn` names — its
/// pointer and the tag it carries — from the root's viewpoint. Usable before
/// a `BexVm` exists.
pub fn declaration_head_by_fqn(packages: &PackageIndex, fqn: &str) -> Option<TypeHead> {
    let mut parts: Vec<Name> = fqn.split('.').map(Name::new).collect();
    let name = parts.pop()?;
    if parts.is_empty() {
        return None;
    }
    let pkg = parts.remove(0);
    let pkg_ptr = packages.accessible(&pkg)?;
    // SAFETY: `packages` only ever holds compile-time `Object::Package` pointers.
    #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
    let package = (unsafe { pkg_ptr.get() }).as_package()?;
    let local = LocalName {
        namespace: parts,
        name,
    };
    let ptr = *package
        .classes
        .get(&local)
        .or_else(|| package.enums.get(&local))
        .or_else(|| package.interfaces.get(&local))
        .or_else(|| package.type_aliases.get(&local))?;
    // SAFETY: a package's declaration pointers are compile-time objects.
    #[expect(unsafe_code, reason = "deref a compile-time declaration pointer")]
    let tag = match unsafe { ptr.get() } {
        Object::Class(class) => class.type_tag,
        Object::Enum(enm) => enm.type_tag,
        Object::Interface(interface) => interface.type_tag,
        Object::TypeAlias(alias) => alias.type_tag,
        _ => return None,
    };
    Some(TypeHead::new(ptr, tag))
}

/// One of the stdlib time classes a CSV cell decodes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeClass {
    Instant,
    PlainDate,
    PlainDateTime,
}

/// The stdlib declarations the runtime recognizes structurally — the CSV
/// cell classes, the JSON media wrappers, the `json` alias, the prompt cache
/// delimiter, and the live
/// capabilities a host proxies — resolved to heads once from the loaded
/// packages and shared by every VM the engine spawns, like the error and
/// panic class tables.
///
/// A program that does not load one of them (no stdlib) has no value of that
/// type, so the entry is absent and nothing matches it.
#[derive(Debug, Default)]
pub struct StdlibHeads {
    time: Vec<(TimeClass, TypeHead)>,
    media: Vec<(MediaKind, TypeHead)>,
    json: Option<TypeHead>,
    cache_delimiter: Option<TypeHead>,
    capabilities: Vec<(StdlibCapability, TypeHead)>,
}

/// A live stdlib capability a host reaches through a proxy rather than a
/// value: an instance stays on the heap behind a handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdlibCapability {
    /// `ai.FunctionSpec`.
    FunctionSpec,
    /// `ai.stream.Stream`.
    Stream,
}

impl StdlibHeads {
    pub fn resolve(packages: &PackageIndex) -> Self {
        let head = |fqn: &str| declaration_head_by_fqn(packages, fqn);
        let time = [
            (TimeClass::Instant, "baml.time.Instant"),
            (TimeClass::PlainDate, "baml.time.PlainDate"),
            (TimeClass::PlainDateTime, "baml.time.PlainDateTime"),
        ]
        .into_iter()
        .filter_map(|(class, fqn)| Some((class, head(fqn)?)))
        .collect();
        // A runtime media value is an instance of one of these classes; there
        // is no `Generic` media value, and no wrapper class for it.
        let media = [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Pdf,
        ]
        .into_iter()
        .filter_map(|kind| Some((kind, head(kind.wrapper_class_name()?)?)))
        .collect();
        // Resolved from the root's viewpoint under the prelude's fixed names,
        // once, at load: a package of the user's spelled `ai` cannot be
        // reached under that name (the edge is reserved), and nothing here is
        // ever re-derived from a declaration's rendered name.
        let capabilities = [
            (
                StdlibCapability::FunctionSpec,
                baml_type::qualified_name::AI_FUNCTION_SPEC,
            ),
            (
                StdlibCapability::Stream,
                baml_type::qualified_name::AI_STREAM_STREAM,
            ),
        ]
        .into_iter()
        .filter_map(|(capability, fqn)| Some((capability, head(fqn)?)))
        .collect();
        Self {
            time,
            media,
            json: head("baml.json.json"),
            cache_delimiter: head("ai.CacheDelimiter"),
            capabilities,
        }
    }

    /// Which live stdlib capability the declaration tagged `tag` is, if any:
    /// the stdlib's own declaration, by identity, never a same-spelled one.
    pub fn capability(&self, tag: baml_type::typetag::TypeTag) -> Option<StdlibCapability> {
        self.capabilities
            .iter()
            .find(|(_, head)| head.tag() == tag)
            .map(|(capability, _)| *capability)
    }

    /// Which stdlib time class `head` is, if any.
    pub fn time_class(&self, head: TypeHead) -> Option<TimeClass> {
        self.time
            .iter()
            .find(|(_, h)| *h == head)
            .map(|(class, _)| *class)
    }

    /// Which media wrapper class `head` is, if any.
    pub fn media_kind(&self, head: TypeHead) -> Option<MediaKind> {
        self.media
            .iter()
            .find(|(_, h)| *h == head)
            .map(|(kind, _)| *kind)
    }

    /// Whether `head` is `ai.CacheDelimiter`, the value `${cache(args)}`
    /// interpolates into a prompt.
    pub fn is_cache_delimiter(&self, head: TypeHead) -> bool {
        self.cache_delimiter == Some(head)
    }

    /// The recursive `baml.json.json` alias, if the program loads it.
    pub fn json_alias(&self) -> Option<TypeHead> {
        self.json
    }

    /// Whether `head` is the recursive `baml.json.json` alias.
    pub fn is_json_alias(&self, head: TypeHead) -> bool {
        self.json == Some(head)
    }
}

/// Flatten every package's recursive type aliases into one `TypeName → RuntimeTy`
/// map (the shape `SysOpContext::type_alias_definitions` wants for output-format
/// rendering), reconstructing each qualified name from its package + `LocalName`.
pub fn all_recursive_type_aliases(
    packages: &PackageIndex,
) -> IndexMap<baml_type::TaggedTypeName, bex_vm_types::RealizedTy> {
    let mut out = IndexMap::new();
    for pkg_ptr in packages.package_ptrs() {
        // SAFETY: `packages` only ever holds compile-time `Object::Package`
        // pointers (built by `fill_package_slots`), valid for the heap's lifetime.
        #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
        let object = unsafe { pkg_ptr.get() };
        let Some(package) = object.as_package() else {
            continue;
        };
        let pkg_name = &package.name;
        for (local, alias_ptr) in &package.type_aliases {
            // SAFETY: as above — a package's alias map only holds compile-time
            // `Object::TypeAlias` pointers, allocated alongside the package.
            #[expect(unsafe_code, reason = "deref a compile-time alias pointer")]
            let alias_object = unsafe { alias_ptr.get() };
            let Object::TypeAlias(alias) = alias_object else {
                continue;
            };
            // Keyed by declaration identity: the alias object's own tag, with
            // its declared name carried alongside for rendering.
            let qtn = baml_type::TypeName::new(
                pkg_name.clone(),
                local.namespace.clone(),
                local.name.clone(),
            );
            out.insert(
                baml_type::TaggedTypeName::new(
                    alias.type_tag,
                    baml_type::DeclarationName::Declared(qtn),
                ),
                alias.definition.clone(),
            );
        }
    }
    out
}

/// Build the unified heap from `compile_time_objects`, additionally allocating
/// the per-package `Object::Package` / `Object::ImplRule` objects and returning
/// the [`PackageIndex`] rooted at the package at `root`. The heap is sealed
/// on return.
///
/// The input is a converted program (`convert_program`), which checked the
/// executable against the format's laws (`Program::validate`) before handing
/// it on: every index here is within its pool and names the kind its table
/// says, every head binds, every switch is solved. The assertions below hold
/// the loader to that, not to a decoded artifact.
pub fn build_heap_with_packages(
    mut compile_time_objects: Vec<Object>,
    packages: &[ProgramPackage],
    root: u32,
) -> (Arc<BexHeap>, PackageIndex) {
    let layout = reserve_package_slots(&mut compile_time_objects, packages);
    let mut heap = BexHeap::build_unsealed_default(compile_time_objects);
    let index = fill_package_slots(&mut heap, packages, root, &layout);
    // Every slot is now real (impl rules carry heads in their patterns), so
    // this is the one point where the whole image can bind — and prove it
    // bound — before anything can dereference a head.
    heap.bind_type_heads();
    heap.assert_switch_tables_dispatch();
    (heap.seal(), index)
}

#[cfg(test)]
mod tests {
    use bex_heap::{CollectionLevel, Tlab};
    use bex_vm_types::{RootHaver, types::Object};

    use super::*;

    fn empty_rule(interface: HeapPtr, class: bex_vm_types::TypeHead) -> RuntimeImplRule {
        RuntimeImplRule {
            interface_head: interface,
            for_ty_pattern: bex_vm_types::TyTemplate::Class(class, Box::new([])),
            generic_param_bounds: Vec::new(),
            interface_args: Vec::new(),
            interface_assoc: Vec::new(),
            methods: IndexMap::new(),
            field_links: Box::default(),
        }
    }

    /// Allocate `n` (class, rule) pairs, register them under `interface`, and
    /// return the class pointers.
    fn register_witnesses(
        heap: &Arc<BexHeap>,
        tables: &DynDispatchTables,
        interface: HeapPtr,
        n: u64,
    ) -> (Tlab, Vec<HeapPtr>) {
        let mut tlab = Tlab::new(Arc::clone(heap));
        let mut owners = Vec::new();
        for index in 0..n {
            // The side table is pointer-kind agnostic; these moving heap
            // allocations stand in for runtime Object::Class owners and let
            // the test isolate the weak-lifetime/forwarding contract.
            let owner = tlab.alloc_string(format!("runtime class {index}"));
            let head =
                bex_vm_types::TypeHead::new(owner, baml_type::typetag::TypeTag::fresh_dynamic());
            let rule = tlab.alloc(Object::ImplRule(Box::new(empty_rule(interface, head))));
            tables.register_rule(interface, DynRuleEntry { class: owner, rule });
            owners.push(owner);
        }
        (tlab, owners)
    }

    #[test]
    fn dynamic_dispatch_entries_are_forwarded_then_swept_with_owners() {
        let heap = BexHeap::new(vec![Object::String(bex_str::BexStr::from(
            "static interface",
        ))]);
        let interface = heap.compile_time_ptr(0);
        let tables = Arc::new(DynDispatchTables::default());
        let mut hook = DynDispatchRoot::new(Arc::clone(&tables), Arc::clone(&heap));
        let (mut tlab, owners) = register_witnesses(&heap, &tables, interface, 128);

        // Root every other owner AND its rule, as a live class's own reachability
        // would (an instance reaches the class; the class's provenance/type value
        // reaches its witnesses).
        let mut live: Vec<_> = owners.iter().step_by(2).copied().collect();
        live.extend(tables.rules_of(interface).into_iter().step_by(2));
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_stats, _remapped, forwarding) = unsafe { heap.collect_garbage(&live) };
        hook.forward_roots(&forwarding);
        assert_eq!(
            tables.rule_count(),
            64,
            "dead owners must sweep their rules"
        );
        let forwarded_owner = forwarding
            .get(&owners[0])
            .copied()
            .expect("live owner must be forwarded");
        assert!(
            !tables.rules_for_class(forwarded_owner).is_empty(),
            "live owner pointers must follow relocation"
        );
        for rule in tables.rules_of(interface) {
            #[expect(unsafe_code, reason = "reading a just-forwarded pointer")]
            let obj = unsafe { rule.get() };
            assert!(
                matches!(obj, Object::ImplRule(_)),
                "rule pointers must follow relocation"
            );
        }

        tlab.invalidate();
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_stats, _remapped, forwarding) = unsafe { heap.collect_garbage(&[]) };
        hook.forward_roots(&forwarding);
        assert_eq!(
            tables.rule_count(),
            0,
            "dropping the last runtime class/witness reachability must sweep every rule"
        );
    }

    /// A minor collection identity-maps every Gen2 object it does not visit, so
    /// an old runtime class reachable only from other old objects is alive yet
    /// absent from the forwarding map. The sweep must not evict it.
    #[test]
    fn minor_collection_keeps_unvisited_old_witnesses() {
        let heap = BexHeap::new(vec![Object::String(bex_str::BexStr::from(
            "static interface",
        ))]);
        let interface = heap.compile_time_ptr(0);
        let tables = Arc::new(DynDispatchTables::default());
        let mut hook = DynDispatchRoot::new(Arc::clone(&tables), Arc::clone(&heap));
        let (mut tlab, owners) = register_witnesses(&heap, &tables, interface, 8);

        // Promote everything into Gen2: two minors move Gen0 -> Gen1 -> Gen2.
        // `roots[0]` is the first owner throughout — each collection returns
        // the remapped root list in order.
        let mut roots = owners;
        roots.extend(tables.rules_of(interface));
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_, roots, forwarding) =
            unsafe { heap.collect_garbage_generational(&roots, CollectionLevel::Minor) };
        hook.forward_roots(&forwarding);
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_, roots, forwarding) =
            unsafe { heap.collect_garbage_generational(&roots, CollectionLevel::Minor) };
        hook.forward_roots(&forwarding);
        assert_eq!(tables.rule_count(), 8);
        let old_class = roots[0];
        assert_eq!(heap.generation_of(old_class), Generation::Gen2);

        // Now a minor collection with NO roots at all: nothing young is live,
        // and the old witnesses are simply not visited. They must all survive.
        tlab.invalidate();
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_, _, forwarding) =
            unsafe { heap.collect_garbage_generational(&[], CollectionLevel::Minor) };
        assert!(
            !forwarding.contains_key(&old_class),
            "precondition: an unvisited Gen2 object is absent from a minor forwarding map"
        );
        hook.forward_roots(&forwarding);
        assert_eq!(
            tables.rule_count(),
            8,
            "a minor collection must not evict live, unvisited old witnesses"
        );
        assert!(
            !tables.rules_for_class(old_class).is_empty(),
            "the unvisited owner keeps its witness"
        );

        // A major with no roots reclaims them for real.
        #[expect(unsafe_code, reason = "standalone stop-the-world GC test")]
        let (_, _, forwarding) =
            unsafe { heap.collect_garbage_generational(&[], CollectionLevel::Major) };
        hook.forward_roots(&forwarding);
        assert_eq!(tables.rule_count(), 0);
        drop(roots);
    }
}
