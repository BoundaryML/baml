//! The per-package resolver: every declaration a body references, by
//! identity, to the operand the unit convention gives it — a local ordinal
//! for the package's own declarations, an import ordinal for everyone
//! else's — with dependency slots and import entries interned on first use.
//!
//! A declaration's import key is a pure function of its identity: which
//! package root declares it and its item path there. Nothing here renders a
//! spelling to look anything up; the one rendering, the baked type tag of a
//! type import, is the relocation record the runtime binder needs and is
//! derived from the same wire name the unit's bytecode carries.

use std::collections::{HashMap, VecDeque};

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{
        MethodOwner, class_data, enum_data, function_data, interface_data, let_data, method_owner,
    },
    loc::{ClassLoc, DeclRef, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
    package::{edge_table, spelling},
};
use baml_compiler2_hir_ty::{
    callable::ExternalCallTarget,
    extern_loc::{ClassRef, EnumRef, ExternFunctionLoc, ExternRowAddr, FunctionRef, InterfaceRef},
    layout,
    lower::qualify_def,
};
use baml_compiler2_mir::RuntimeLowering;
use baml_linker_types::{DeclKey, DepSlot, DependencyEntry, ImportEntry, import_operand};
use baml_type::{DeclName, TypeName, typetag::TypeTag};
use bex_vm_types::{
    BodyKey, DeclPath, FnPath, GlobalIndex, ImplBodyKey, InterfaceKey, ObjectIndex, TypeHead,
    types::LocalName,
};

use crate::{
    emit::CodegenRefs,
    items::{impl_rule_target, owns_no_slot},
    package::HeadIndex,
};

// ── Declaration coordinates ──────────────────────────────────────────────────

/// A declaration's item path within its package.
pub(crate) fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
}

/// The tag a head's references bake: the content tag of its wire spelling,
/// minted by the one formula every runtime type uses.
pub(crate) fn head_tag(db: &dyn crate::Db, head: &DeclName) -> TypeTag {
    TypeHead::of_name(&spelling(db).wire(head)).tag()
}

/// The head of a class, wherever it is declared.
pub(crate) fn class_head<'db>(db: &'db dyn crate::Db, class: ClassRef<'db>) -> DeclName {
    match class {
        DeclRef::Source(class) => {
            qualify_def(db, Definition::Class(class), &class_data(db, class).name)
        }
        DeclRef::External(class) => class.head(db).clone(),
    }
}

/// The head of an enum, wherever it is declared.
pub(crate) fn enum_head<'db>(db: &'db dyn crate::Db, enum_ref: EnumRef<'db>) -> DeclName {
    match enum_ref {
        DeclRef::Source(enum_loc) => qualify_def(
            db,
            Definition::Enum(enum_loc),
            &enum_data(db, enum_loc).name,
        ),
        DeclRef::External(enum_loc) => enum_loc.head(db).clone(),
    }
}

/// The head of an interface, wherever it is declared.
pub(crate) fn interface_head<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceRef<'db>,
) -> DeclName {
    match interface {
        DeclRef::Source(interface) => qualify_def(
            db,
            Definition::Interface(interface),
            &interface_data(db, interface).name,
        ),
        DeclRef::External(interface) => interface.head(db).clone(),
    }
}

/// The edge name `from` reaches `to` under, if it has a direct edge.
fn edge_name(db: &dyn crate::Db, from: SourceRoot, to: SourceRoot) -> Option<Name> {
    edge_table(db, from)
        .into_iter()
        .find(|(_, root)| *root == to)
        .map(|(name, _)| name)
}

/// The path a source function is exported under: a free function, a class's
/// inherent method, an interface's default body, or an `implements` block's
/// provided body. The caller has already established that the function owns
/// a slot ([`owns_no_slot`] is false).
pub(crate) fn function_path<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> DeclPath {
    let name = function_data(db, function).name.clone();
    let owner_path =
        |definition: Definition<'db>, owner: &Name| item_path(&qualify_def(db, definition, owner));
    match method_owner(db, function) {
        None => DeclPath::Function(FnPath::Free(item_path(&qualify_def(
            db,
            Definition::Function(function),
            &name,
        )))),
        Some(MethodOwner::Class(class)) => DeclPath::Function(FnPath::Method {
            class: owner_path(Definition::Class(class), &class_data(db, class).name),
            name,
        }),
        Some(MethodOwner::Interface(interface)) => DeclPath::InterfaceBody(BodyKey::Default {
            interface: owner_path(
                Definition::Interface(interface),
                &interface_data(db, interface).name,
            ),
            method: name,
        }),
        Some(MethodOwner::Impl(block)) => {
            let body_root = file_package(db, block.file(db)).root;
            let lowering = RuntimeLowering::of(db, body_root);
            // A block that cannot be baked is a diagnosed program, which
            // never reaches emit.
            let target = impl_rule_target(db, block, &lowering)
                .unwrap_or_else(|| unreachable!("an accepted `implements` block bakes to a rule"));
            let interface = interface_head(db, target.interface);
            let package = if interface.root() == body_root {
                baml_type::Package::Local
            } else {
                // The block names its interface from its own package, so the
                // interface's package is a direct edge of it.
                baml_type::Package::Dep(edge_name(db, body_root, interface.root()).unwrap_or_else(
                    || unreachable!("an implemented interface is reachable by a direct edge"),
                ))
            };
            DeclPath::InterfaceBody(BodyKey::ImplMethod(Box::new(ImplBodyKey {
                interface: InterfaceKey {
                    package,
                    path: item_path(&interface),
                },
                coherence: target.coherence_key(),
                method: name,
            })))
        }
    }
}

/// The root that declares a served callable row and the path the row is
/// exported under. An impl-provided row has no path: such a body is reached
/// rule-relatively, never by a direct call across packages.
fn extern_function_coords<'db>(
    db: &'db dyn crate::Db,
    row: ExternFunctionLoc<'db>,
) -> (SourceRoot, DeclPath) {
    match row.addr(db) {
        ExternRowAddr::Declared(target) => match target {
            ExternalCallTarget::Free { function } => (
                function.root(),
                DeclPath::Function(FnPath::Free(item_path(function))),
            ),
            ExternalCallTarget::Method { class, name } => (
                class.root(),
                DeclPath::Function(FnPath::Method {
                    class: item_path(class),
                    name: name.clone(),
                }),
            ),
            ExternalCallTarget::Interface { interface, method } => (
                interface.root(),
                DeclPath::InterfaceBody(BodyKey::Default {
                    interface: item_path(interface),
                    method: method.clone(),
                }),
            ),
        },
        ExternRowAddr::ImplProvided { .. } => {
            unreachable!("an impl-provided row is dispatched, never called directly")
        }
    }
}

// ── Interned tables ──────────────────────────────────────────────────────────

/// A unit's or tail's dependency table, interned on first use: one slot per
/// package root the table's owner reaches, located through the owner's edge
/// table directly or through an earlier slot's.
pub(crate) struct DependencyTable {
    owner: SourceRoot,
    entries: Vec<(SourceRoot, DependencyEntry)>,
}

impl DependencyTable {
    pub(crate) fn new(owner: SourceRoot) -> Self {
        Self {
            owner,
            entries: Vec::new(),
        }
    }

    /// The slot `root` is (or becomes) bound to. A root the owner has no
    /// direct edge to is located through the nearest package that does: the
    /// path from the owner is interned hop by hop, each hop `via` its
    /// parent's slot.
    pub(crate) fn slot(&mut self, db: &dyn crate::Db, root: SourceRoot) -> DepSlot {
        if root == self.owner {
            return DepSlot::SELF;
        }
        if let Some(slot) = self.existing(root) {
            return slot;
        }
        for (parent, edge, hop) in self.path_to(db, root) {
            if self.existing(hop).is_some() {
                continue;
            }
            let via = if parent == self.owner {
                DepSlot::SELF
            } else {
                self.existing(parent)
                    .unwrap_or_else(|| unreachable!("a path's parent is interned before its child"))
            };
            self.entries.push((
                hop,
                DependencyEntry {
                    edge,
                    via,
                    fingerprint: None,
                },
            ));
        }
        self.existing(root)
            .unwrap_or_else(|| unreachable!("the path ends at the root"))
    }

    fn existing(&self, root: SourceRoot) -> Option<DepSlot> {
        self.entries
            .iter()
            .position(|(bound, _)| *bound == root)
            .map(DepSlot::of_dependency_index)
    }

    /// The root bound to `slot`.
    pub(crate) fn root_of(&self, slot: DepSlot) -> SourceRoot {
        match slot.dependency_index() {
            None => self.owner,
            Some(index) => self.entries[index].0,
        }
    }

    /// The shortest edge path from the owner to `target`, as
    /// `(parent, edge name, child)` hops, in order. Breadth-first over the
    /// edge tables, so the located root is always the nearest one.
    fn path_to(
        &self,
        db: &dyn crate::Db,
        target: SourceRoot,
    ) -> Vec<(SourceRoot, Name, SourceRoot)> {
        let mut parents: HashMap<SourceRoot, (SourceRoot, Name)> = HashMap::new();
        let mut queue = VecDeque::from([self.owner]);
        while let Some(current) = queue.pop_front() {
            if current == target {
                break;
            }
            for (edge, root) in edge_table(db, current) {
                if root == self.owner || parents.contains_key(&root) {
                    continue;
                }
                parents.insert(root, (current, edge));
                queue.push_back(root);
            }
        }
        let mut hops = Vec::new();
        let mut child = target;
        while let Some((parent, edge)) = parents.get(&child) {
            hops.push((*parent, edge.clone(), child));
            child = *parent;
        }
        assert!(
            child == self.owner,
            "package `{}` references package `{}`, which no edge path from it reaches",
            self.owner.path(db).display(),
            target.path(db).display()
        );
        hops.reverse();
        hops
    }

    pub(crate) fn into_entries(self) -> Vec<DependencyEntry> {
        self.entries.into_iter().map(|(_, entry)| entry).collect()
    }
}

/// One index space's import table, interned on first use.
#[derive(Default)]
pub(crate) struct ImportSpace {
    entries: Vec<ImportEntry>,
    ordinals: HashMap<DeclKey, usize>,
}

impl ImportSpace {
    /// The ordinal of `key`, interned at first use.
    pub(crate) fn intern(&mut self, key: DeclKey, baked_tag: Option<TypeTag>) -> usize {
        if let Some(&ordinal) = self.ordinals.get(&key) {
            debug_assert_eq!(self.entries[ordinal].baked_tag, baked_tag);
            return ordinal;
        }
        let ordinal = self.entries.len();
        self.entries.push(ImportEntry {
            key: key.clone(),
            baked_tag,
        });
        self.ordinals.insert(key, ordinal);
        ordinal
    }

    pub(crate) fn entries(&self) -> &[ImportEntry] {
        &self.entries
    }

    pub(crate) fn into_entries(self) -> Vec<ImportEntry> {
        self.entries
    }
}

/// A unit's or tail's two import tables.
#[derive(Default)]
pub(crate) struct ImportTables {
    pub(crate) objects: ImportSpace,
    pub(crate) globals: ImportSpace,
}

// ── The package's own declarations ───────────────────────────────────────────

/// Where a package's own declarations sit in its unit: each type by its
/// bucket offset, each slotted function and `let` by its local slot.
#[derive(Default)]
pub(crate) struct LocalTables<'db> {
    pub(crate) classes: HashMap<ClassLoc<'db>, u32>,
    pub(crate) enums: HashMap<EnumLoc<'db>, u32>,
    pub(crate) interfaces: HashMap<InterfaceLoc<'db>, u32>,
    pub(crate) aliases: HashMap<TypeAliasLoc<'db>, u32>,
    pub(crate) function_slots: HashMap<FunctionLoc<'db>, u32>,
    pub(crate) let_slots: HashMap<LetLoc<'db>, u32>,
}

impl<'db> LocalTables<'db> {
    fn bucket_len(bucket: &HashMap<impl std::hash::Hash, u32>) -> u32 {
        u32::try_from(bucket.len()).expect("bucket fits u32")
    }

    /// The unit-local object index of the `k`-th class: the class bucket
    /// comes first.
    fn class_object(k: u32) -> u32 {
        k
    }

    fn enum_object(&self, k: u32) -> u32 {
        Self::bucket_len(&self.classes) + k
    }

    fn interface_object(&self, k: u32) -> u32 {
        self.enum_object(Self::bucket_len(&self.enums)) + k
    }

    fn alias_object(&self, k: u32) -> u32 {
        self.interface_object(Self::bucket_len(&self.interfaces)) + k
    }

    /// Where the code bucket begins: after every type bucket.
    pub(crate) fn code_base(&self) -> u32 {
        self.alias_object(Self::bucket_len(&self.aliases))
    }

    fn class<'a>(&self, class: ClassLoc<'a>) -> Option<u32>
    where
        'db: 'a,
    {
        let classes: &HashMap<ClassLoc<'a>, u32> = &self.classes;
        classes.get(&class).map(|&k| Self::class_object(k))
    }

    fn enum_<'a>(&self, enum_loc: EnumLoc<'a>) -> Option<u32>
    where
        'db: 'a,
    {
        let enums: &HashMap<EnumLoc<'a>, u32> = &self.enums;
        enums.get(&enum_loc).map(|&k| self.enum_object(k))
    }

    fn interface<'a>(&self, interface: InterfaceLoc<'a>) -> Option<u32>
    where
        'db: 'a,
    {
        let interfaces: &HashMap<InterfaceLoc<'a>, u32> = &self.interfaces;
        interfaces
            .get(&interface)
            .map(|&k| self.interface_object(k))
    }

    fn function_slot<'a>(&self, function: FunctionLoc<'a>) -> Option<u32>
    where
        'db: 'a,
    {
        let slots: &HashMap<FunctionLoc<'a>, u32> = &self.function_slots;
        slots.get(&function).copied()
    }

    fn let_slot<'a>(&self, binding: LetLoc<'a>) -> Option<u32>
    where
        'db: 'a,
    {
        let slots: &HashMap<LetLoc<'a>, u32> = &self.let_slots;
        slots.get(&binding).copied()
    }
}

/// How a resolver answers its own package's declarations: a unit pools them
/// (locals); a tail is a separate table and reaches them as imports at
/// [`DepSlot::SELF`].
pub(crate) enum Own<'w, 'db> {
    Unit(&'w LocalTables<'db>),
    Tail,
}

// ── The resolver ─────────────────────────────────────────────────────────────

/// One unit's or tail's resolver over one package's tables.
pub(crate) struct PackageRefs<'w, 'db> {
    pub(crate) db: &'db dyn crate::Db,
    pub(crate) root: SourceRoot,
    pub(crate) own: Own<'w, 'db>,
    pub(crate) deps: DependencyTable,
    pub(crate) imports: ImportTables,
}

impl<'w, 'db> PackageRefs<'w, 'db> {
    pub(crate) fn new(db: &'db dyn crate::Db, root: SourceRoot, own: Own<'w, 'db>) -> Self {
        Self {
            db,
            root,
            own,
            deps: DependencyTable::new(root),
            imports: ImportTables::default(),
        }
    }

    /// An object import of `path` in `root`.
    fn import_object(
        &mut self,
        root: SourceRoot,
        path: DeclPath,
        baked_tag: Option<TypeTag>,
    ) -> ObjectIndex {
        let key = DeclKey {
            dep: self.deps.slot(self.db, root),
            path,
        };
        ObjectIndex::from_raw(import_operand(self.imports.objects.intern(key, baked_tag)))
    }

    /// A global-slot import of `path` in `root`.
    fn import_global(&mut self, root: SourceRoot, path: DeclPath) -> GlobalIndex {
        let key = DeclKey {
            dep: self.deps.slot(self.db, root),
            path,
        };
        GlobalIndex::from_raw(import_operand(self.imports.globals.intern(key, None)))
    }

    /// The object of a type declaration: local when this unit pools it,
    /// else an import carrying the tag the bytecode bakes for it.
    fn type_object(
        &mut self,
        head: &DeclName,
        path: DeclPath,
        local: impl FnOnce(&LocalTables<'db>) -> Option<u32>,
    ) -> ObjectIndex {
        if head.root() == self.root {
            match self.own {
                Own::Unit(tables) => {
                    return local(tables)
                        .map(|k| ObjectIndex::from_raw(k as usize))
                        .unwrap_or_else(|| {
                            unreachable!("the package's own {path} was not pooled")
                        });
                }
                Own::Tail => {}
            }
        }
        let tag = head_tag(self.db, head);
        self.import_object(head.root(), path, Some(tag))
    }

    /// Intern an import for the declaration `head` refers to, unless this
    /// unit pools it. A head no package of the closure declares is a compiler
    /// bug: every head reaching emit was resolved by the checker.
    pub(crate) fn import_head(
        &mut self,
        heads: &HeadIndex,
        head: &TypeHead,
    ) -> Result<(), crate::LoweringError> {
        let tag = head.tag();
        let Some((root, path)) = heads.declaration(tag) else {
            return Err(crate::LoweringError::Internal(format!(
                "a baked type head ({tag:?}) refers to a declaration no package in reach declares"
            )));
        };
        if root == self.root && matches!(self.own, Own::Unit(_)) {
            return Ok(());
        }
        self.import_object(root, path.clone(), Some(tag));
        Ok(())
    }

    /// The object of an interface, local or imported.
    pub(crate) fn interface<'a>(&mut self, interface: InterfaceRef<'a>) -> ObjectIndex
    where
        'db: 'a,
    {
        let head = interface_head(self.db, interface);
        let path = DeclPath::Interface(item_path(&head));
        self.type_object(&head, path, |tables| match interface {
            DeclRef::Source(interface) => tables.interface(interface),
            DeclRef::External(_) => None,
        })
    }

    /// The global slot of a slot-owning function, local or imported; `None`
    /// when the declaration owns none.
    fn function_slot<'a>(&mut self, function: FunctionRef<'a>) -> Option<GlobalIndex>
    where
        'db: 'a,
    {
        match function {
            DeclRef::Source(function) => {
                if owns_no_slot(self.db, function) {
                    return None;
                }
                let root = file_package(self.db, function.file(self.db)).root;
                if root == self.root {
                    match self.own {
                        Own::Unit(tables) => {
                            let slot = tables.function_slot(function).unwrap_or_else(|| {
                                unreachable!(
                                    "a slot-owning function of this package was not slotted"
                                )
                            });
                            return Some(GlobalIndex::from_raw(slot as usize));
                        }
                        Own::Tail => {}
                    }
                }
                let path = function_path(self.db, function);
                Some(self.import_global(root, path))
            }
            DeclRef::External(row) => {
                let (root, path) = extern_function_coords(self.db, row);
                Some(self.import_global(root, path))
            }
        }
    }
}

impl<'db> CodegenRefs<'db> for PackageRefs<'_, 'db> {
    fn class<'a>(&mut self, class: ClassRef<'a>) -> Option<ObjectIndex>
    where
        'db: 'a,
    {
        let head = class_head(self.db, class);
        let path = DeclPath::Class(item_path(&head));
        Some(self.type_object(&head, path, |tables| match class {
            DeclRef::Source(class) => tables.class(class),
            DeclRef::External(_) => None,
        }))
    }

    fn class_named(&mut self, tn: &TypeName) -> Option<ObjectIndex> {
        let head = spelling(self.db).resolve(self.db, self.root, tn)?;
        let class = layout::class_ref_of(self.db, &head)?;
        self.class(class)
    }

    fn enum_<'a>(&mut self, enum_ref: EnumRef<'a>) -> ObjectIndex
    where
        'db: 'a,
    {
        let head = enum_head(self.db, enum_ref);
        let path = DeclPath::Enum(item_path(&head));
        self.type_object(&head, path, |tables| match enum_ref {
            DeclRef::Source(enum_loc) => tables.enum_(enum_loc),
            DeclRef::External(_) => None,
        })
    }

    fn enum_named(&mut self, tn: &TypeName) -> Option<ObjectIndex> {
        let head = spelling(self.db).resolve(self.db, self.root, tn)?;
        let enum_ref = layout::enum_ref_of(self.db, &head)?;
        Some(self.enum_(enum_ref))
    }

    fn function<'a>(&mut self, func: FunctionRef<'a>) -> Option<GlobalIndex>
    where
        'db: 'a,
    {
        self.function_slot(func)
    }

    fn let_global<'a>(&mut self, binding: LetLoc<'a>) -> GlobalIndex
    where
        'db: 'a,
    {
        let root = file_package(self.db, binding.file(self.db)).root;
        if root == self.root {
            match self.own {
                Own::Unit(tables) => {
                    let slot = tables
                        .let_slot(binding)
                        .unwrap_or_else(|| unreachable!("a `let` of this package was not slotted"));
                    return GlobalIndex::from_raw(slot as usize);
                }
                Own::Tail => {}
            }
        }
        let head = qualify_def(
            self.db,
            Definition::Let(binding),
            &let_data(self.db, binding).name,
        );
        self.import_global(root, DeclPath::Let(item_path(&head)))
    }
}
