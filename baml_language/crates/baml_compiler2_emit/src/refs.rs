//! The per-package resolver: every declaration a body references, by
//! identity, to the operand the unit convention gives it — a local ordinal
//! for the package's own declarations, an import ordinal for everyone
//! else's — with dependency slots and import entries interned on first use.
//!
//! A declaration's import key is a pure function of its identity: which
//! package root declares it and its item path there. A type head is a
//! declaration operand like any other: MIR hands every head over as the
//! declaration's identity, and [`PackageRefs::head`] answers with that
//! declaration's operand — so no head is ever resolved from a spelling, no
//! operand carries a tag, and a head names nothing the import table does not
//! list. What a unit carries of a name is display data: a declaration's own
//! display name and operand metadata, spelled by the program's table.

use std::collections::HashMap;

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{
        MethodOwner, class_data, enum_data, function_data, interface_data, let_data, method_owner,
    },
    loc::{ClassLoc, DeclRef, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
    package::{DependencyTable, edge_spelling, spelling},
};
use baml_compiler2_hir_ty::{
    callable::ExternalCallTarget,
    extern_loc::{ClassRef, EnumRef, ExternFunctionLoc, ExternRowAddr, FunctionRef, InterfaceRef},
    layout,
    lower::qualify_def,
    package_interface::interface_digest,
};
use baml_compiler2_mir::RuntimeLowering;
use baml_linker_types::{DeclKey, ImportEntry, Locator, import_operand};
use baml_type::DeclName;
use bex_vm_types::{
    BodyKey, DeclPath, FnPath, GlobalIndex, ImplBodyKey, InterfaceKey, ObjectIndex, TypeHead,
    bytecode::SwitchKey, types::LocalName,
};

use crate::items::{impl_rule_target, owns_no_slot};

// ── Declaration coordinates ──────────────────────────────────────────────────

/// The tag field a declaration object carries in a unit: its own operand,
/// exactly what every head naming it carries, until the linker or grafter
/// assigns the real tag.
pub(crate) fn unit_tag(operand: u32) -> baml_type::typetag::TypeTag {
    TypeHead::unresolved_operand(ObjectIndex::from_raw(operand as usize)).tag()
}

/// A declaration's item path within its package.
pub(crate) fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
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
            DeclPath::InterfaceBody(BodyKey::ImplMethod(Box::new(ImplBodyKey {
                // Located from the body's package by edge path: the interface
                // may be one a dependency re-exports, whose own package no
                // edge of the body's names directly.
                interface: InterfaceKey {
                    package: edge_spelling(db, body_root, interface.root()),
                    path: item_path(&interface),
                },
                coherence: target.coherence_key(db, body_root),
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

/// A unit's or tail's dependency table as the unit carries it
/// ([`DependencyTable::locators`]): each direct entry fingerprinted with the
/// digest of the interface its owner compiled against ([`interface_digest`]),
/// the value the linker checks the bound package's record against; a
/// prelude entry and a transitive entry carry none — the toolchain pairing
/// covers the prelude, the parent's digest covers a transitive root.
pub(crate) fn dependency_locators(db: &dyn crate::Db, deps: &DependencyTable) -> Vec<Locator> {
    deps.locators(db, |root| interface_digest(db, root))
}

/// One index space's import table, interned on first use.
#[derive(Default)]
pub(crate) struct ImportSpace {
    entries: Vec<ImportEntry>,
    ordinals: HashMap<DeclKey, usize>,
}

impl ImportSpace {
    /// The ordinal of `key`, interned at first use.
    pub(crate) fn intern(&mut self, key: DeclKey) -> usize {
        if let Some(&ordinal) = self.ordinals.get(&key) {
            return ordinal;
        }
        let ordinal = self.entries.len();
        self.entries.push(ImportEntry { key: key.clone() });
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
    pub(crate) fn class_object(k: u32) -> u32 {
        k
    }

    pub(crate) fn enum_object(&self, k: u32) -> u32 {
        Self::bucket_len(&self.classes) + k
    }

    pub(crate) fn interface_object(&self, k: u32) -> u32 {
        self.enum_object(Self::bucket_len(&self.enums)) + k
    }

    pub(crate) fn alias_object(&self, k: u32) -> u32 {
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
    /// A package's unit: every declaration of the package is a local.
    Unit(&'w LocalTables<'db>),
    /// A session submission's unit: the submission's declarations are
    /// locals, and the package's others — earlier submissions, live in the
    /// session package — are imports at `SELF`.
    Session(&'w LocalTables<'db>),
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
    /// Declaration → the unit-convention head it anchored to: a memo keyed
    /// by identity.
    heads: HashMap<DeclName, TypeHead>,
}

impl<'w, 'db> PackageRefs<'w, 'db> {
    pub(crate) fn new(db: &'db dyn crate::Db, root: SourceRoot, own: Own<'w, 'db>) -> Self {
        Self::with_tables(
            db,
            root,
            own,
            DependencyTable::new(root),
            ImportTables::default(),
        )
    }

    /// A resolver continuing from tables an earlier pass interned.
    pub(crate) fn with_tables(
        db: &'db dyn crate::Db,
        root: SourceRoot,
        own: Own<'w, 'db>,
        deps: DependencyTable,
        imports: ImportTables,
    ) -> Self {
        Self {
            db,
            root,
            own,
            deps,
            imports,
            heads: HashMap::new(),
        }
    }

    /// An object import of `path` in `root`.
    fn import_object(&mut self, root: SourceRoot, path: DeclPath) -> ObjectIndex {
        let key = DeclKey {
            dep: self.deps.slot(self.db, root),
            path,
        };
        ObjectIndex::from_raw(import_operand(self.imports.objects.intern(key)))
    }

    /// A global-slot import of `path` in `root`.
    fn import_global(&mut self, root: SourceRoot, path: DeclPath) -> GlobalIndex {
        let key = DeclKey {
            dep: self.deps.slot(self.db, root),
            path,
        };
        GlobalIndex::from_raw(import_operand(self.imports.globals.intern(key)))
    }

    /// The object of a type declaration: local when this unit pools it,
    /// else an import.
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
                Own::Session(tables) => {
                    if let Some(k) = local(tables) {
                        return ObjectIndex::from_raw(k as usize);
                    }
                }
                Own::Tail => {}
            }
        }
        self.import_object(head.root(), path)
    }

    /// The object of whichever type declaration `head` is: a class, enum,
    /// interface, or (recursive) alias.
    fn declaration_object(&mut self, head: &DeclName) -> ObjectIndex {
        let db = self.db;
        if let Some(class) = layout::class_ref_of(db, head) {
            return self.class(class);
        }
        if let Some(enum_ref) = layout::enum_ref_of(db, head) {
            return self.enum_(enum_ref);
        }
        if let Some(interface) = layout::interface_ref_of(db, head) {
            return self.interface(interface);
        }
        if let Some(alias) = layout::alias_ref_of(db, head) {
            let path = DeclPath::TypeAlias(item_path(head));
            return self.type_object(head, path, |tables| match alias {
                DeclRef::Source(alias) => {
                    tables.aliases.get(&alias).map(|&k| tables.alias_object(k))
                }
                DeclRef::External(_) => None,
            });
        }
        unreachable!(
            "`{}` resolved to a declaration no lane declares",
            spelling(db).wire(head)
        )
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

    /// The unit-convention head of a declaration: the operand of its object,
    /// local or imported.
    pub(crate) fn head(&mut self, decl: &DeclName) -> TypeHead {
        if let Some(&head) = self.heads.get(decl) {
            return head;
        }
        let head = TypeHead::unresolved_operand(self.declaration_object(decl));
        self.heads.insert(decl.clone(), head);
        head
    }

    /// The global slot of a slot-owning function, local or imported; `None`
    /// when the declaration owns none.
    pub(crate) fn function_slot<'a>(&mut self, function: FunctionRef<'a>) -> Option<GlobalIndex>
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
                        Own::Session(tables) => {
                            if let Some(slot) = tables.function_slot(function) {
                                return Some(GlobalIndex::from_raw(slot as usize));
                            }
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

impl<'db> PackageRefs<'_, 'db> {
    /// The pooled object of `class`: a local ordinal for this package's own
    /// declaration, an import ordinal otherwise. Every class has one.
    pub(crate) fn class<'a>(&mut self, class: ClassRef<'a>) -> ObjectIndex
    where
        'db: 'a,
    {
        let head = class_head(self.db, class);
        let path = DeclPath::Class(item_path(&head));
        self.type_object(&head, path, |tables| match class {
            DeclRef::Source(class) => tables.class(class),
            DeclRef::External(_) => None,
        })
    }

    /// The pooled object of `enum_ref`; an emitted enum always has one.
    pub(crate) fn enum_<'a>(&mut self, enum_ref: EnumRef<'a>) -> ObjectIndex
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

    /// The global slot of a top-level `let`.
    pub(crate) fn let_global<'a>(&mut self, binding: LetLoc<'a>) -> GlobalIndex
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
                Own::Session(tables) => {
                    if let Some(slot) = tables.let_slot(binding) {
                        return GlobalIndex::from_raw(slot as usize);
                    }
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

    /// The class `head` is, for its layout facts; `None` when `head` is no
    /// class.
    pub(crate) fn class_ref(&self, head: &DeclName) -> Option<ClassRef<'db>> {
        layout::class_ref_of(self.db, head)
    }

    /// The enum `head` is; `None` when `head` is no enum.
    pub(crate) fn enum_ref(&self, head: &DeclName) -> Option<EnumRef<'db>> {
        layout::enum_ref_of(self.db, head)
    }

    /// A class arm's switch key: the declaration's operand, which the linker
    /// or grafter solves against the tag it assigns.
    pub(crate) fn switch_key<'a>(&mut self, class: ClassRef<'a>) -> SwitchKey
    where
        'db: 'a,
    {
        SwitchKey::Declaration(self.class(class))
    }

    // The one place a compile-time head becomes a runtime head: every
    // runtime type emit produces goes through one of these, by the
    // declaration's identity, so no type carries a head the linker or
    // grafter cannot bind and none is ever resolved from a spelling.

    pub(crate) fn anchor_template(
        &mut self,
        ty: &baml_compiler2_mir::TyTemplate,
    ) -> bex_vm_types::TyTemplate {
        ty.map_heads(&mut |decl: &DeclName| self.head(decl))
    }

    pub(crate) fn anchor_runtime_ty(
        &mut self,
        ty: &baml_compiler2_mir::RuntimeTy,
    ) -> bex_vm_types::RuntimeTy {
        ty.map_heads(&mut |decl: &DeclName| self.head(decl))
    }

    pub(crate) fn anchor_realized(
        &mut self,
        ty: &baml_compiler2_mir::RealizedTy,
    ) -> bex_vm_types::RealizedTy {
        ty.map_heads(&mut |decl: &DeclName| self.head(decl))
    }

    pub(crate) fn anchor_interface(
        &mut self,
        interface: &baml_type::RuntimeInterface<DeclName>,
    ) -> bex_vm_types::RuntimeInterface {
        interface.map_heads(&mut |decl: &DeclName| self.head(decl))
    }
}
