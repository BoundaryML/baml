//! The loader for runtime-compiled output: one [`EmittedPackage`] grafted
//! onto the heap as a runtime package's image — a fresh `Package.compile`
//! package, or one more submission of a `Session`. The runtime twin of the
//! static linker: the same unit convention read the same way, with pointers
//! where the linker writes image indices and fresh dynamic tags where it
//! assigns image positions.
//!
//! # What binds what
//!
//! A unit names everything foreign by [`DeclKey`]: a dependency SLOT and a
//! declaration PATH. A slot binds to a package object — `SELF` is the
//! package being loaded, a direct edge is the package in its dependency
//! table (an artifact's pins, a session's `_new` map), a prelude name is the
//! host image's package — and a path resolves through the bound package's
//! own tables, kind-directed. Nothing is looked up by a rendered spelling.
//!
//! An interface body has no name in any table. Within its own package it is
//! addressed through the package-private slot entry — a session's later
//! submission, or the tail, reaching a body the package pooled. From a
//! dependency, a default body is the function its interface binds, and a
//! provided body is not addressable at all: it is dispatched through its
//! rule, as the static linker also insists.
//!
//! # The image
//!
//! The package's object table is the operand space: the unit's and the
//! tail's locals in their order, and one entry per distinct declaration the
//! package imports. An import the image already holds — an earlier
//! submission's declaration, the unit's own reached from its tail, a prelude
//! class every submission names — is that entry, so the table grows by what
//! a load declares and first reaches, never by what it repeats.
//!
//! # Publish once
//!
//! A load builds the package's next image in full — its objects and cells,
//! the declaration and slot tables that index them, its rules, and the
//! tail — and writes it into the package object once, at the end, dirtying
//! the package's card as part of that write. The tail names the unit's
//! declarations as `SELF` imports, so own resolution reads the image being
//! built, never the package object. A load that fails publishes nothing: the
//! package is exactly what it was, and what the load allocated is garbage. A
//! session survives a refused submission this way — nothing it can reach
//! points at objects only a finished load would have kept alive.
//!
//! # Allocation and collection
//!
//! Every pointer here is held in plain locals across allocations. That is
//! sound for the same reason it is in the rest of the reflection natives: a
//! native call runs between safepoints, and the collector moves nothing
//! until the VM reaches one.

use std::collections::{HashMap, HashSet};

use baml_linker_types::{
    CompilationUnit, DeclKey, DepSlot, EmittedPackage, ImportEntry, InitTail, LocalRef, Locator,
    ProgramImplRuleFrag, import_ordinal,
};
use baml_type::typetag::TypeTag;
use bex_heap::TlabHolder;
use bex_vm_types::{
    AtomicValueSlot, BodyKey, ConstValue, DeclPath, FnPath, GlobalIndex, HeapPtr, Object,
    ObjectIndex, ObjectType, TyTemplate, TypeHead,
    bytecode::{SwitchDispatch, SwitchKey},
    head_walk::visit_object_heads_mut,
    relink::{IndexOperand, visit_object_operands},
    types::{
        EdgeKind, ExportSurface, LocalName, MethodImpl, Objects, Owner, Package, RuntimeImplRule,
        Slots, Value,
    },
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

/// Whose image a load extends.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadTarget {
    /// A fresh `Package.compile` package: its tables start empty, so its
    /// unit imports nothing at `SELF` — the static linker's malformed-unit
    /// rule — and only the tail names the unit just loaded.
    Package,
    /// A session package: its earlier submissions are live in its tables,
    /// so a `SELF` import may name any of them — a `let` binds its CELL, a
    /// body its own package-private entry.
    Session,
}

/// One initializer of a session submission, as the session will run it: the
/// tail slot of the helper computing the value and the `let` whose cell
/// receives it — the binding the step declares, or, for an assignment step,
/// the visible binding it writes back to.
pub(super) struct Commit<'a> {
    pub(super) helper: u32,
    pub(super) target: &'a LocalName,
}

/// What a load produced beyond the tables it filled.
pub(super) struct Loaded {
    /// The package's `$init`, when its tail has one.
    pub(super) init: Option<HeapPtr>,
    /// A submission's commits in execution order: each helper's pointer and
    /// the package slot of the cell it commits into.
    pub(super) commits: Vec<(HeapPtr, usize)>,
}

/// Graft `emitted` onto `package_ptr`'s image. The package object already
/// exists (its dependency table is what slots bind through); on return its
/// declaration tables, slot table, object table, rules, and `$init`s hold
/// the output — written once, after the whole load succeeded — and the
/// tail's `$init` is returned for the caller to run.
///
/// # Errors
///
/// A dependency the package cannot bind, an import naming nothing in the
/// bound package, an operand outside the unit, a switch table no hash
/// separates, or a commit into a cell the package does not own: each is a
/// link error, never a dangling reference, and leaves the package untouched.
pub(super) fn load_package(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    target: LoadTarget,
    emitted: &EmittedPackage,
    commits: &[Commit<'_>],
) -> Result<Loaded, VmRustFnError> {
    let unit = &emitted.unit;
    if let Some(misplaced) = unit
        .misplaced_object()
        .or_else(|| emitted.tail.as_ref().and_then(InitTail::misplaced_object))
    {
        return Err(link_error(format!(
            "the unit pools a {:?} object at offset {} of `{}`",
            ObjectType::of(misplaced.object),
            misplaced.offset,
            misplaced.bucket
        )));
    }
    if target == LoadTarget::Package
        && let Some(entry) = unit
            .object_imports
            .iter()
            .chain(&unit.global_imports)
            .find(|entry| entry.key.dep.is_self())
    {
        return Err(link_error(format!(
            "the unit imports `{}` from the package it defines",
            entry.key.path
        )));
    }
    let slots = bind_slots(vm, package_ptr, &unit.dependencies)?;
    let mut image = Image::of(vm, package_ptr);

    let (locals, imports) = place_objects(
        vm,
        &mut image,
        package_ptr,
        &slots,
        unit.objects(),
        &unit.object_imports,
    )?;
    // The unit's own slots — every function and `let` in export order — then
    // one slot per global import that copies a dependency's value.
    let base_slot = image.cells.len();
    let local_count = unit.exports.globals.len();
    image
        .cells
        .extend((0..local_count).map(|_| AtomicValueSlot::new(Value::NULL)));
    slot_unit_functions(vm, &mut image, unit, &locals, base_slot)?;
    let import_slots = place_globals(vm, &mut image, package_ptr, &slots, &unit.global_imports)?;
    let space = Space {
        locals,
        imports,
        local_slots: (base_slot..base_slot + local_count).collect(),
        import_slots,
    };

    // Relocation: every operand and head of every owned object, then the
    // rules the unit declares.
    relocate_all(vm, &mut image, &space)?;
    let type_count = unit.type_object_count();
    for rule in &unit.impl_rules {
        let (interface_ptr, rule_ptr) = load_rule(vm, rule, &space, &image, type_count)?;
        image.add(vm, rule_ptr);
        image
            .impl_rules
            .entry(interface_ptr)
            .or_default()
            .push(rule_ptr);
    }

    let mut slot_table: IndexMap<DeclPath, u32> = IndexMap::new();
    for (path, slot) in &unit.exports.globals {
        let ordinal = u32::try_from(base_slot + *slot as usize)
            .map_err(|_| link_error("the package's slot table outgrew u32".to_string()))?;
        slot_table.insert(path.clone(), ordinal);
    }
    image.declare(vm, unit, &space, &slot_table)?;
    image.globals.extend(slot_table);

    let tail = match &emitted.tail {
        Some(tail) => load_tail(vm, package_ptr, tail, commits, &mut image)?,
        None => {
            if let Some(commit) = commits.first() {
                return Err(link_error(format!(
                    "a commit into `{}` is recorded, but the submission has no tail",
                    commit.target
                )));
            }
            LoadedTail::default()
        }
    };
    let LoadedTail {
        init,
        test_init,
        commits,
    } = tail;
    publish(vm, package_ptr, image, init, test_init);
    Ok(Loaded { init, commits })
}

/// What a tail added to the image beyond its objects and cells.
#[derive(Default)]
struct LoadedTail {
    init: Option<HeapPtr>,
    test_init: Option<HeapPtr>,
    commits: Vec<(HeapPtr, usize)>,
}

/// Graft a package's tail after its unit: the `let` helpers, `$init`, and
/// `$init_test` chainer, each in a slot of its own, and each recorded commit
/// bound to the cell it writes.
fn load_tail(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    tail: &InitTail,
    commits: &[Commit<'_>],
    image: &mut Image,
) -> Result<LoadedTail, VmRustFnError> {
    let slots = bind_slots(vm, package_ptr, &tail.dependencies)?;
    let (locals, imports) = place_objects(
        vm,
        image,
        package_ptr,
        &slots,
        tail.objects.iter(),
        &tail.object_imports,
    )?;
    let base_slot = image.cells.len();
    for &object in &tail.slot_objects {
        let local = *locals
            .get(object as usize)
            .ok_or_else(|| link_error(format!("tail slot object {object} is outside the tail")))?;
        image.add_cell(Value::object(image.objects[local]));
    }
    let import_slots = place_globals(vm, image, package_ptr, &slots, &tail.global_imports)?;
    let space = Space {
        locals,
        imports,
        local_slots: (base_slot..base_slot + tail.slot_objects.len()).collect(),
        import_slots,
    };
    relocate_all(vm, image, &space)?;

    let tail_object = |index: u32| -> Result<HeapPtr, VmRustFnError> {
        space
            .locals
            .get(index as usize)
            .map(|&local| image.objects[local])
            .ok_or_else(|| link_error(format!("tail object {index} is outside the tail")))
    };
    let init = tail.init.map(tail_object).transpose()?;
    let test_init = tail.init_test.map(tail_object).transpose()?;
    let mut bound = Vec::with_capacity(commits.len());
    for commit in commits {
        let helper = tail
            .slot_objects
            .get(commit.helper as usize)
            .copied()
            .ok_or_else(|| {
                link_error(format!(
                    "commit helper slot {} is outside the tail",
                    commit.helper
                ))
            })?;
        let cell = image
            .globals
            .get(&DeclPath::Let(commit.target.clone()))
            .copied()
            .ok_or_else(|| {
                link_error(format!(
                    "a commit into `{}` names no cell the package owns",
                    commit.target
                ))
            })?;
        bound.push((tail_object(helper)?, cell as usize));
    }
    Ok(LoadedTail {
        init,
        test_init,
        commits: bound,
    })
}

/// The one write of a load into its package: every table, the cells and
/// objects, the rules, and the tail's `$init`s. The package may be old and
/// everything written is young, so the write dirties its card.
fn publish(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    image: Image,
    init: Option<HeapPtr>,
    test_init: Option<HeapPtr>,
) {
    let mut metered = vm.get_object_mut(package_ptr);
    let Object::Package(package) = &mut *metered else {
        unreachable!("a graft target is a package")
    };
    let initialized = matches!(
        package.slots,
        Slots::Own {
            initialized: true,
            ..
        }
    );
    package.classes = image.classes;
    package.enums = image.enums;
    package.interfaces = image.interfaces;
    package.type_aliases = image.type_aliases;
    package.impl_rules = image.impl_rules;
    package.globals = image.globals;
    package.slots = Slots::Own {
        cells: image.cells.into_boxed_slice(),
        initialized,
    };
    package.objects = Objects::Own(image.objects.into_boxed_slice());
    package.init = init;
    package.test_init = test_init;
    vm.tlab.heap().conservative_write_barrier(package_ptr);
}

fn link_error(message: impl std::fmt::Display) -> VmRustFnError {
    VmRustFnError::from(VmBamlError::InvalidArgument {
        message: format!("runtime link: {message}"),
    })
}

fn live(ptr: HeapPtr) -> Option<HeapPtr> {
    (!ptr.is_null()).then_some(ptr)
}

/// The package's image as this load extends it, and what [`publish`]
/// writes: the object table with each entry's declaration tag beside it
/// (what a head relocates to and a switch table is solved over), the entry
/// each pointer holds, the cells, and the tables that index them — the
/// declarations by kind and item, the slot each `let`, function, and body
/// owns, and the rules by interface. Own resolution reads these, so the tail
/// sees the unit's declarations before either is published.
struct Image {
    objects: Vec<HeapPtr>,
    tags: Vec<Option<TypeTag>>,
    entry_of: HashMap<HeapPtr, usize>,
    cells: Vec<AtomicValueSlot>,
    classes: IndexMap<LocalName, HeapPtr>,
    enums: IndexMap<LocalName, HeapPtr>,
    interfaces: IndexMap<LocalName, HeapPtr>,
    type_aliases: IndexMap<LocalName, HeapPtr>,
    globals: IndexMap<DeclPath, u32>,
    impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
}

impl Image {
    /// The image as it stands — empty for a fresh package, a session's
    /// accumulated one otherwise.
    fn of(vm: &BexVm, package_ptr: HeapPtr) -> Self {
        let Object::Package(package) = vm.get_object(package_ptr) else {
            unreachable!("a graft target is a package")
        };
        let objects = package
            .objects
            .own()
            .unwrap_or_else(|| unreachable!("a graft target owns its objects"));
        let cells = package
            .slots
            .own()
            .unwrap_or_else(|| unreachable!("a graft target owns its cells"));
        let mut image = Self {
            objects: Vec::with_capacity(objects.len()),
            tags: Vec::with_capacity(objects.len()),
            entry_of: HashMap::with_capacity(objects.len()),
            cells: cells.to_vec(),
            classes: package.classes.clone(),
            enums: package.enums.clone(),
            interfaces: package.interfaces.clone(),
            type_aliases: package.type_aliases.clone(),
            globals: package.globals.clone(),
            impl_rules: package.impl_rules.clone(),
        };
        for &ptr in objects {
            image.add(vm, ptr);
        }
        image
    }

    /// Enter the unit's declarations from its export table: each from the
    /// bucket its path says, of the kind its path says, once — checked
    /// against the slot table the unit's own exports produced.
    fn declare(
        &mut self,
        vm: &BexVm,
        unit: &CompilationUnit,
        space: &Space,
        slots: &IndexMap<DeclPath, u32>,
    ) -> Result<(), VmRustFnError> {
        let mut exported: HashSet<&DeclPath> = HashSet::with_capacity(unit.exports.objects.len());
        for (path, local_ref) in &unit.exports.objects {
            if !exported.insert(path) {
                return Err(link_error(format!("the unit exports `{path}` twice")));
            }
            if !local_ref.holds(path) {
                return Err(link_error(format!(
                    "the unit exports `{path}` from the {local_ref:?} bucket"
                )));
            }
            let local = space
                .locals
                .get(unit.local_index(*local_ref))
                .copied()
                .ok_or_else(|| link_error(format!("`{path}` points outside the unit")))?;
            let ptr = self.objects[local];
            // The buckets hold their kind (checked before placement); a code
            // export must be the function its path says.
            if let DeclPath::Function(_) | DeclPath::InterfaceBody(_) = path {
                let is_body = matches!(path, DeclPath::InterfaceBody(_));
                if !matches!(
                    vm.get_object(ptr),
                    Object::Function(function) if function.is_interface_body == is_body
                ) {
                    return Err(link_error(format!(
                        "the unit exports `{path}` as a {:?} object",
                        ObjectType::of(vm.get_object(ptr))
                    )));
                }
            }
            match path {
                DeclPath::Class(item) => {
                    self.classes.insert(item.clone(), ptr);
                }
                DeclPath::Enum(item) => {
                    self.enums.insert(item.clone(), ptr);
                }
                DeclPath::Interface(item) => {
                    self.interfaces.insert(item.clone(), ptr);
                }
                DeclPath::TypeAlias(item) => {
                    self.type_aliases.insert(item.clone(), ptr);
                }
                // A callable is reached through its cell (a method also
                // through its class's method table).
                DeclPath::Function(_) => {}
                DeclPath::InterfaceBody(_) => {
                    if !slots.contains_key(path) {
                        return Err(link_error(format!("`{path}` is pooled but owns no slot")));
                    }
                }
                DeclPath::Let(_) => {
                    return Err(link_error(format!(
                        "`{path}` is exported as an object, but a `let` owns none"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Append `ptr` as a new entry.
    fn add(&mut self, vm: &BexVm, ptr: HeapPtr) -> usize {
        let index = self.objects.len();
        self.objects.push(ptr);
        self.tags
            .push(bex_heap::BexHeap::declaration_tag(vm.get_object(ptr)));
        self.entry_of.entry(ptr).or_insert(index);
        index
    }

    /// The entry holding `ptr`, appended if the image has none.
    fn place(&mut self, vm: &BexVm, ptr: HeapPtr) -> usize {
        match self.entry_of.get(&ptr) {
            Some(&index) => index,
            None => self.add(vm, ptr),
        }
    }

    fn add_cell(&mut self, value: Value) -> usize {
        let slot = self.cells.len();
        self.cells.push(AtomicValueSlot::new(value));
        slot
    }

    /// The tag of the entry at `index`, where it is a declaration.
    fn tag(&self, index: usize) -> Option<TypeTag> {
        self.tags.get(index).copied().flatten()
    }

    /// The head of the declaration at `index`.
    fn head(&self, index: usize) -> Result<TypeHead, VmRustFnError> {
        match self.tag(index) {
            Some(tag) => Ok(TypeHead::new(self.objects[index], tag)),
            None => Err(link_error(format!(
                "a type head names object {index}, which is not a declaration"
            ))),
        }
    }
}

/// Allocate a table's owned objects and place its object imports: the
/// owned entries' and the imports' image indices, each in table order.
fn place_objects<'a>(
    vm: &mut BexVm,
    image: &mut Image,
    package_ptr: HeapPtr,
    slots: &[Bound],
    owned: impl IntoIterator<Item = &'a Object>,
    imports: &[ImportEntry],
) -> Result<(Vec<usize>, Vec<usize>), VmRustFnError> {
    // A declaration is minted with its identity here — the only place a
    // runtime tag comes from.
    let mut locals = Vec::new();
    for object in owned {
        let mut object = object.clone();
        adopt(&mut object, package_ptr);
        // A runtime function takes its telemetry identity at allocation.
        let ptr = vm
            .alloc_runtime_object(object)
            .map_err(VmRustFnError::InternalError)?;
        locals.push(image.add(vm, ptr));
    }
    let mut placed = Vec::with_capacity(imports.len());
    for entry in imports {
        let resolved = resolve_object(vm, image, package_ptr, slots, &entry.key)?;
        placed.push(image.place(vm, resolved));
    }
    Ok((locals, placed))
}

/// Place a table's global imports: the slot each ordinal reads.
fn place_globals(
    vm: &BexVm,
    image: &mut Image,
    package_ptr: HeapPtr,
    slots: &[Bound],
    imports: &[ImportEntry],
) -> Result<Vec<usize>, VmRustFnError> {
    let mut placed = Vec::with_capacity(imports.len());
    for entry in imports {
        placed.push(
            match resolve_global(vm, image, package_ptr, slots, &entry.key)? {
                GlobalBinding::Slot(slot) => slot,
                GlobalBinding::Value(value) => image.add_cell(value),
            },
        );
    }
    Ok(placed)
}

/// Fill the unit's own function and body slots with their objects, checking
/// the slot table on the way: every slot within the table, none owned twice.
/// A `let`'s slot stays `null` for its initializer.
fn slot_unit_functions(
    vm: &BexVm,
    image: &mut Image,
    unit: &CompilationUnit,
    locals: &[usize],
    base_slot: usize,
) -> Result<(), VmRustFnError> {
    let mut owned = vec![false; unit.exports.globals.len()];
    for (path, slot) in &unit.exports.globals {
        let Some(seen) = owned.get_mut(*slot as usize) else {
            return Err(link_error(format!(
                "`{path}` owns slot {slot}, outside the unit's slot table"
            )));
        };
        if std::mem::replace(seen, true) {
            return Err(link_error(format!(
                "the unit places two globals at local slot {slot}"
            )));
        }
    }
    let code_of: HashMap<&DeclPath, u32> = unit
        .exports
        .objects
        .iter()
        .filter_map(|(path, local)| match local {
            LocalRef::Code(offset) => Some((path, *offset)),
            LocalRef::Class(_)
            | LocalRef::Enum(_)
            | LocalRef::Interface(_)
            | LocalRef::TypeAlias(_) => None,
        })
        .collect();
    let type_count = unit.type_object_count();
    for (path, slot) in &unit.exports.globals {
        let offset = match path {
            DeclPath::Let(_) => continue,
            DeclPath::Function(_) | DeclPath::InterfaceBody(_) => {
                *code_of.get(path).ok_or_else(|| {
                    link_error(format!(
                        "`{path}` owns a slot, but the unit pools no function for it"
                    ))
                })?
            }
            DeclPath::Class(_)
            | DeclPath::Enum(_)
            | DeclPath::Interface(_)
            | DeclPath::TypeAlias(_) => {
                return Err(link_error(format!(
                    "`{path}` is a type, which owns no slot"
                )));
            }
        };
        let object = *locals.get(type_count + offset as usize).ok_or_else(|| {
            link_error(format!(
                "`{path}` references code offset {offset} outside the unit"
            ))
        })?;
        let global = base_slot + *slot as usize;
        let function = image.objects[object];
        if !matches!(vm.get_object(function), Object::Function(_)) {
            return Err(link_error(format!(
                "`{path}` owns a slot, but the unit pools a {:?} object for it",
                ObjectType::of(vm.get_object(function))
            )));
        }
        *image.cells.get_mut(global).ok_or_else(|| {
            link_error(format!(
                "`{path}` owns slot {slot}, outside the unit's slot table"
            ))
        })? = AtomicValueSlot::new(Value::object(function));
    }
    Ok(())
}

/// Relocate every local of `space`, keeping the float constants it boxes in
/// the image.
fn relocate_all(vm: &mut BexVm, image: &mut Image, space: &Space) -> Result<(), VmRustFnError> {
    for &index in &space.locals {
        let floats = relocate(vm, image.objects[index], space, image)?;
        for float in floats {
            image.add(vm, float);
        }
    }
    Ok(())
}

/// What a dependency slot binds to: a package, or — through an anonymous
/// edge — a class or enum declaration belonging to no package, which exports
/// exactly itself.
#[derive(Clone, Copy)]
enum Bound {
    Package(HeapPtr),
    Declaration(HeapPtr),
}

/// Bind a dependency table's slots (slot `k + 1` is `entries[k]`): each
/// entry's edge name is read in the edge table of the package bound to its
/// `via` slot — the package being loaded for a direct edge (its edges are
/// the compile's mounts and the host image's prelude, exactly what the unit
/// could name), an earlier slot's package for a transitive root (one the
/// consumer reaches through a dependency's re-exports or API). The static
/// linker's rule, read off package objects instead of a link set. A slot
/// bound to a declaration has no edges to read.
fn bind_slots(
    vm: &BexVm,
    package_ptr: HeapPtr,
    entries: &[Locator],
) -> Result<Vec<Bound>, VmRustFnError> {
    let mut bound: Vec<Bound> = Vec::with_capacity(entries.len());
    for (index, locator) in entries.iter().enumerate() {
        let via = locator.via();
        let parent = match via.dependency_index() {
            None => package_ptr,
            Some(via_index) => match bound.get(via_index) {
                Some(Bound::Package(parent)) => *parent,
                Some(Bound::Declaration(_)) => {
                    return Err(link_error(format!(
                        "dependency slot {} is reached via slot {}, a declaration with no edges",
                        index + 1,
                        via.0
                    )));
                }
                None => {
                    return Err(link_error(format!(
                        "dependency slot {} is reached via slot {}, which is not earlier",
                        index + 1,
                        via.0
                    )));
                }
            },
        };
        let Object::Package(package) = vm.get_object(parent) else {
            unreachable!("a bound dependency slot is a package")
        };
        let edge = package.edges.get(locator.edge()).ok_or_else(|| {
            link_error(format!(
                "dependency `{}` names no package the compile was given",
                locator.edge()
            ))
        })?;
        verify_fingerprint(vm, locator, edge.kind, edge.target)?;
        bound.push(match edge.kind {
            EdgeKind::Anonymous => Bound::Declaration(edge.target),
            EdgeKind::Declared | EdgeKind::ReExported | EdgeKind::Prelude => {
                Bound::Package(edge.target)
            }
        });
    }
    Ok(bound)
}

/// The static linker's fingerprint law, as a consistency assertion over the
/// package objects the artifact's pins already bound by identity: a direct
/// entry carries the digest of the interface payload the compile read, and a
/// package served from a compiled blob must still carry those bytes. A
/// projected surface (a `with_types` view, an anonymous declaration) has no
/// payload — the compile read the seam's projection of it — so the pin is its
/// whole identity. A prelude entry binds a prelude edge and a transitive
/// entry either kind; neither carries a digest, as in the static lane.
fn verify_fingerprint(
    vm: &BexVm,
    locator: &Locator,
    kind: EdgeKind,
    target: HeapPtr,
) -> Result<(), VmRustFnError> {
    let (edge, expected) = match (locator, kind) {
        (Locator::Prelude { .. }, EdgeKind::Prelude) | (Locator::Transitive { .. }, _) => {
            return Ok(());
        }
        (
            Locator::Prelude { edge },
            EdgeKind::Declared | EdgeKind::ReExported | EdgeKind::Anonymous,
        ) => {
            return Err(link_error(format!(
                "dependency `{edge}` carries no interface fingerprint"
            )));
        }
        (Locator::Direct { edge, .. }, EdgeKind::Prelude) => {
            return Err(link_error(format!(
                "dependency `{edge}` is a prelude package but carries an interface fingerprint"
            )));
        }
        (
            Locator::Direct { edge, digest },
            EdgeKind::Declared | EdgeKind::ReExported | EdgeKind::Anonymous,
        ) => (edge, *digest),
    };
    let blob = match vm.get_object(target) {
        Object::Package(target) => match &target.surface {
            ExportSurface::Compiled(blob) => blob,
            ExportSurface::Projected => return Ok(()),
        },
        // An anonymous declaration is mounted as a projected root of one row.
        Object::Class(_) | Object::Enum(_) => return Ok(()),
        other => unreachable!(
            "a dependency edge targets a package or an anonymous declaration, found {:?}",
            ObjectType::of(other)
        ),
    };
    let found = baml_artifact::payload_digest(baml_artifact::ArtifactKind::PackageInterface, blob)
        .map_err(|error| {
            link_error(format!(
                "dependency `{edge}` carries an interface payload that does not decode: {error}"
            ))
        })?;
    if found != expected {
        return Err(link_error(format!(
            "dependency `{edge}` is not the interface the artifact was compiled against"
        )));
    }
    Ok(())
}

fn slot_binding(
    package_ptr: HeapPtr,
    slots: &[Bound],
    dep: DepSlot,
) -> Result<Bound, VmRustFnError> {
    match dep.dependency_index() {
        None => Ok(Bound::Package(package_ptr)),
        Some(index) => slots.get(index).copied().ok_or_else(|| {
            link_error(format!(
                "dependency slot {} names no entry of the table of {}",
                dep.0,
                slots.len()
            ))
        }),
    }
}

/// The object an anonymous declaration exports under `path`: itself, under
/// its own kind and item name, and nothing else — it has no members a
/// consumer imports (a minted class has no inherent methods).
fn anonymous_export(vm: &BexVm, declaration: HeapPtr, path: &DeclPath) -> Option<HeapPtr> {
    let exported = match (vm.get_object(declaration), path) {
        (Object::Class(class), DeclPath::Class(item)) => {
            item.namespace.is_empty() && item.name == *class.name.item_name()
        }
        (Object::Enum(enm), DeclPath::Enum(item)) => {
            item.namespace.is_empty() && item.name == *enm.name.item_name()
        }
        _ => false,
    };
    exported.then_some(declaration)
}

/// The tables a path resolves through: the image being built when the slot
/// is the package itself, the live package object for a dependency. A
/// package's own declarations are never read off its object mid-load —
/// they are not there until [`publish`].
#[derive(Clone, Copy)]
enum Tables<'a> {
    Own(&'a Image),
    Dependency(&'a Package),
}

impl Tables<'_> {
    fn class(self, item: &LocalName) -> Option<HeapPtr> {
        match self {
            Self::Own(image) => image.classes.get(item).copied(),
            Self::Dependency(package) => package.classes.get(item).copied(),
        }
    }

    fn enum_(self, item: &LocalName) -> Option<HeapPtr> {
        match self {
            Self::Own(image) => image.enums.get(item).copied(),
            Self::Dependency(package) => package.enums.get(item).copied(),
        }
    }

    fn interface(self, item: &LocalName) -> Option<HeapPtr> {
        match self {
            Self::Own(image) => image.interfaces.get(item).copied(),
            Self::Dependency(package) => package.interfaces.get(item).copied(),
        }
    }

    fn type_alias(self, item: &LocalName) -> Option<HeapPtr> {
        match self {
            Self::Own(image) => image.type_aliases.get(item).copied(),
            Self::Dependency(package) => package.type_aliases.get(item).copied(),
        }
    }

    /// The slot `path` owns.
    fn slot(self, path: &DeclPath) -> Option<u32> {
        match self {
            Self::Own(image) => image.globals.get(path).copied(),
            Self::Dependency(package) => package.globals.get(path).copied(),
        }
    }

    /// The value in the cell at `ordinal`.
    fn cell(self, vm: &BexVm, ordinal: u32) -> Option<Value> {
        match self {
            Self::Own(image) => image.cells.get(ordinal as usize).map(AtomicValueSlot::load),
            Self::Dependency(package) => cell_value(vm, package, ordinal),
        }
    }
}

/// What `dep` resolves through: tables, or the declaration it binds.
enum BoundTables<'a> {
    Tables(Tables<'a>),
    Declaration(HeapPtr),
}

fn bound_tables<'a>(
    vm: &'a BexVm,
    image: &'a Image,
    package_ptr: HeapPtr,
    slots: &[Bound],
    dep: DepSlot,
) -> Result<BoundTables<'a>, VmRustFnError> {
    Ok(match slot_binding(package_ptr, slots, dep)? {
        Bound::Package(owner) if owner == package_ptr => BoundTables::Tables(Tables::Own(image)),
        Bound::Package(owner) => {
            let Object::Package(package) = vm.get_object(owner) else {
                unreachable!("a bound dependency slot is a package")
            };
            BoundTables::Tables(Tables::Dependency(package))
        }
        Bound::Declaration(declaration) => BoundTables::Declaration(declaration),
    })
}

/// The object `key` names, through the bound package's tables.
fn resolve_object(
    vm: &BexVm,
    image: &Image,
    package_ptr: HeapPtr,
    slots: &[Bound],
    key: &DeclKey,
) -> Result<HeapPtr, VmRustFnError> {
    let tables = match bound_tables(vm, image, package_ptr, slots, key.dep)? {
        BoundTables::Tables(tables) => tables,
        BoundTables::Declaration(declaration) => {
            return anonymous_export(vm, declaration, &key.path).ok_or_else(|| {
                link_error(format!(
                    "`{}` is not the declaration bound at dependency slot {}",
                    key.path, key.dep.0
                ))
            });
        }
    };
    let found = match &key.path {
        DeclPath::Class(item) => tables.class(item),
        DeclPath::Enum(item) => tables.enum_(item),
        DeclPath::Interface(item) => tables.interface(item),
        DeclPath::TypeAlias(item) => tables.type_alias(item),
        DeclPath::Function(FnPath::Free(_)) => tables
            .slot(&key.path)
            .and_then(|cell| tables.cell(vm, cell))
            .and_then(|value| value.as_object_ptr()),
        DeclPath::Function(FnPath::Method { class, name }) => {
            tables
                .class(class)
                .and_then(|class_ptr| match vm.get_object(class_ptr) {
                    Object::Class(class) => class
                        .methods
                        .get(name)
                        .and_then(|method| live(method.function_ptr)),
                    _ => None,
                })
        }
        DeclPath::InterfaceBody(body) => match resolve_body(vm, tables, &key.path, body)? {
            Some(BoundBody::Own(cell)) => tables
                .cell(vm, cell)
                .and_then(|value| value.as_object_ptr()),
            Some(BoundBody::Function(function)) => Some(function),
            None => None,
        },
        DeclPath::Let(_) => {
            return Err(link_error(format!(
                "`{}` is a `let`, which owns no object",
                key.path
            )));
        }
    };
    found.ok_or_else(|| {
        link_error(format!(
            "`{}` names no declaration of the package bound at dependency slot {}",
            key.path, key.dep.0
        ))
    })
}

/// A body, as the package that binds it holds it.
enum BoundBody {
    /// The package's own: the cell holding it, as an ordinal into its slots.
    Own(u32),
    /// A dependency's default body: the function its interface binds.
    Function(HeapPtr),
}

/// The value in the cell at `ordinal` of `package`'s slots.
pub(super) fn cell_value(vm: &BexVm, package: &Package, ordinal: u32) -> Option<Value> {
    match &package.slots {
        Slots::Program { base } => Some(vm.globals.get(
            vm.proof(),
            GlobalIndex::from_raw(base.raw() + ordinal as usize),
        )),
        Slots::Own { cells, .. } => cells.get(ordinal as usize).map(AtomicValueSlot::load),
    }
}

/// The body `key` names in `tables`. Within its own package a body is
/// addressed through its package-private slot entry; from a dependency a
/// default body is the function its interface object binds, and a provided
/// body is refused — it is dispatched through its rule, never addressed.
fn resolve_body(
    vm: &BexVm,
    tables: Tables<'_>,
    path: &DeclPath,
    key: &BodyKey,
) -> Result<Option<BoundBody>, VmRustFnError> {
    if let Tables::Own(image) = tables {
        return Ok(image.globals.get(path).copied().map(BoundBody::Own));
    }
    match key {
        BodyKey::Default { interface, method } => Ok(tables
            .interface(interface)
            .and_then(|interface_ptr| match vm.get_object(interface_ptr) {
                Object::Interface(interface) => interface
                    .methods
                    .iter()
                    .find(|row| row.name == *method)
                    .and_then(|row| live(row.default_fn)),
                _ => None,
            })
            .map(BoundBody::Function)),
        BodyKey::ImplMethod(_) => Err(link_error(format!(
            "`{path}` is an impl-provided body of another package, which is dispatched through \
             its rule, never referenced directly"
        ))),
    }
}

/// How a global import binds.
enum GlobalBinding {
    /// The package's own existing slot — a session cell, or a function the
    /// package already holds.
    Slot(usize),
    /// A dependency's slot value, copied at load. A compiled package's cells
    /// are immutable after its `$init`; a session reached as a dependency
    /// has mutable cells, and a value it stores after the load is not seen.
    Value(Value),
}

fn resolve_global(
    vm: &BexVm,
    image: &Image,
    package_ptr: HeapPtr,
    slots: &[Bound],
    key: &DeclKey,
) -> Result<GlobalBinding, VmRustFnError> {
    let tables = match bound_tables(vm, image, package_ptr, slots, key.dep)? {
        BoundTables::Tables(tables) => tables,
        BoundTables::Declaration(_) => {
            return Err(link_error(format!(
                "`{}` names a cell of a declaration, which owns none",
                key.path
            )));
        }
    };
    if let DeclPath::InterfaceBody(body) = &key.path {
        return match resolve_body(vm, tables, &key.path, body)? {
            Some(BoundBody::Own(cell)) => Ok(GlobalBinding::Slot(cell as usize)),
            Some(BoundBody::Function(function)) => {
                Ok(GlobalBinding::Value(Value::object(function)))
            }
            None => Err(link_error(format!(
                "`{}` names no body of the package bound at dependency slot {}",
                key.path, key.dep.0
            ))),
        };
    }
    let slot = tables.slot(&key.path).ok_or_else(|| {
        link_error(format!(
            "`{}` owns no slot of the package bound at dependency slot {}",
            key.path, key.dep.0
        ))
    })?;
    if let Tables::Own(_) = tables {
        return Ok(GlobalBinding::Slot(slot as usize));
    }
    tables
        .cell(vm, slot)
        .map(GlobalBinding::Value)
        .ok_or_else(|| link_error(format!("`{}` names a slot outside its package", key.path)))
}

/// Give a freshly cloned unit object its runtime identity and owner edges
/// before it is allocated: a declaration's tag (the one mint) and the package
/// that owns it — the edge that keeps the package, and so its globals and
/// dependencies, alive from any member — and a body's owning package.
fn adopt(object: &mut Object, package_ptr: HeapPtr) {
    match object {
        Object::Class(class) => {
            class.type_tag = TypeTag::fresh_dynamic();
            class.owner = Owner::Package(package_ptr);
        }
        Object::Enum(enm) => {
            enm.type_tag = TypeTag::fresh_dynamic();
            enm.owner = Owner::Package(package_ptr);
        }
        Object::Interface(interface) => {
            interface.type_tag = TypeTag::fresh_dynamic();
            interface.owner = package_ptr;
        }
        Object::TypeAlias(alias) => {
            alias.type_tag = TypeTag::fresh_dynamic();
            alias.owner = package_ptr;
        }
        Object::Function(function) => function.runtime_package = package_ptr,
        Object::GenericFunction(function) => function.runtime_package = package_ptr,
        // Neither a declaration nor a body: nothing to mint or own.
        Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Type(_)
        | Object::Package(_)
        | Object::ImplRule(_)
        | Object::Instance(_)
        | Object::Variant(_)
        | Object::Closure(_)
        | Object::BoundMethod(_)
        | Object::HostClosure(_)
        | Object::Cell(_)
        | Object::Array(_)
        | Object::Map(_)
        | Object::Float(_)
        | Object::Future(_)
        | Object::RustData(_)
        | Object::Tombstone => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
}

/// One table's operand space: what each local index and import ordinal is
/// in the package's object and slot tables.
struct Space {
    locals: Vec<usize>,
    imports: Vec<usize>,
    local_slots: Vec<usize>,
    import_slots: Vec<usize>,
}

impl Space {
    fn object(&self, raw: usize) -> Result<usize, VmRustFnError> {
        match import_ordinal(raw) {
            Some(ordinal) => self.imports.get(ordinal).copied(),
            None => self.locals.get(raw).copied(),
        }
        .ok_or_else(|| link_error(format!("object operand {raw} is outside the unit")))
    }

    fn global(&self, raw: usize) -> Result<usize, VmRustFnError> {
        match import_ordinal(raw) {
            Some(ordinal) => self.import_slots.get(ordinal).copied(),
            None => self.local_slots.get(raw).copied(),
        }
        .ok_or_else(|| link_error(format!("global operand {raw} is outside the unit")))
    }
}

/// Rewrite `ptr`'s operands and heads into the package's tables, solve its
/// switch tables, resolve its constants, and bind the pointers its wire
/// operands stood for. Returns the boxed float constants it allocated, which
/// the package's object table must keep alive.
fn relocate(
    vm: &mut BexVm,
    ptr: HeapPtr,
    space: &Space,
    image: &Image,
) -> Result<Vec<HeapPtr>, VmRustFnError> {
    let mut failure = None;
    let mut metered = vm.get_object_mut(ptr);
    let object = &mut *metered;
    visit_object_operands(object, |operand| {
        if failure.is_some() {
            return;
        }
        match operand {
            IndexOperand::Object(index) => match space.object(index.raw()) {
                Ok(relocated) => *index = ObjectIndex::from_raw(relocated),
                Err(error) => failure = Some(error),
            },
            IndexOperand::Global(slot) => match space.global(slot.raw()) {
                Ok(relocated) => *slot = GlobalIndex::from_raw(relocated),
                Err(error) => failure = Some(error),
            },
        }
    });
    visit_object_heads_mut(object, &mut |head| {
        if failure.is_some() {
            return;
        }
        match head_operand(head)
            .and_then(|operand| space.object(operand.raw()))
            .and_then(|index| image.head(index))
        {
            Ok(relocated) => *head = relocated,
            Err(error) => failure = Some(error),
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    let mut floats = Vec::new();
    match object {
        Object::Function(function) => {
            for switch in &mut function.bytecode.switch_tables {
                let SwitchDispatch::Keys(keys) = &switch.dispatch else {
                    return Err(link_error(format!(
                        "function `{}` states a switch by values instead of keys",
                        function.name
                    )));
                };
                if let Some(index) = keys.iter().find_map(|key| match key {
                    SwitchKey::Declaration(declaration)
                        if image.tag(declaration.raw()).is_none() =>
                    {
                        Some(declaration.raw())
                    }
                    SwitchKey::Declaration(_) | SwitchKey::Kind(_) => None,
                }) {
                    return Err(link_error(format!(
                        "a type switch keys on object {index}, which is not a declaration"
                    )));
                }
                switch.dispatch = SwitchDispatch::solved(keys, |declaration| {
                    image
                        .tag(declaration.raw())
                        .unwrap_or_else(|| unreachable!("every declaration key was checked"))
                });
            }
            let constants = function.bytecode.constants.clone();
            // Allocating the boxed floats needs the VM; the function is
            // fetched again once they exist.
            drop(metered);
            let mut resolved = Vec::with_capacity(constants.len());
            for constant in constants {
                resolved.push(match constant {
                    // Materialized at execution from `constants` itself.
                    ConstValue::Type(_)
                    | ConstValue::ClassWithTypeArgs { .. }
                    | ConstValue::Literal(_) => Value::NULL,
                    ConstValue::Float(value) => {
                        let boxed = vm.alloc_float(value);
                        floats.push(boxed);
                        Value::object(boxed)
                    }
                    other => other.to_value(|index| image.objects[index.raw()]),
                });
            }
            let mut metered = vm.get_object_mut(ptr);
            let Object::Function(function) = &mut *metered else {
                unreachable!("the object at `ptr` was a function above")
            };
            function.bytecode.resolved_constants = resolved;
            function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        }
        Object::Interface(interface) => {
            if let Some(default) = &mut interface.structural_default {
                default.function_ptr = image.objects[default.function.raw()];
            }
            for method in &mut interface.methods {
                if let Some(default) = method.default {
                    method.default_fn = image.objects[default.raw()];
                }
            }
        }
        Object::Class(class) => {
            for method in class.methods.values_mut() {
                method.function_ptr = image.objects[method.function.raw()];
            }
        }
        // No pointers to bind beyond the operand and head walks above.
        Object::Enum(_)
        | Object::TypeAlias(_)
        | Object::GenericFunction(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Type(_)
        | Object::Package(_)
        | Object::ImplRule(_)
        | Object::Instance(_)
        | Object::Variant(_)
        | Object::Closure(_)
        | Object::BoundMethod(_)
        | Object::HostClosure(_)
        | Object::Cell(_)
        | Object::Array(_)
        | Object::Map(_)
        | Object::Float(_)
        | Object::Future(_)
        | Object::RustData(_)
        | Object::Tombstone => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
    Ok(floats)
}

/// Relocate every head of a template through the space.
fn relocate_template(
    template: &mut TyTemplate,
    space: &Space,
    image: &Image,
) -> Result<(), VmRustFnError> {
    let mut failure = None;
    template.visit_heads_mut(&mut |head| {
        if failure.is_some() {
            return;
        }
        match head_operand(head)
            .and_then(|operand| space.object(operand.raw()))
            .and_then(|index| image.head(index))
        {
            Ok(relocated) => *head = relocated,
            Err(error) => failure = Some(error),
        }
    });
    failure.map_or(Ok(()), Err)
}

/// The declaration operand of a unit-convention head; a head outside the
/// convention is a link error.
fn head_operand(head: &TypeHead) -> Result<ObjectIndex, VmRustFnError> {
    head.try_operand()
        .ok_or_else(|| link_error("the unit carries a type head outside the unit convention"))
}

/// Build and allocate one `implements` rule from its fragment: the
/// interface it implements, its provided bodies by code offset, and its
/// patterns with every head relocated.
fn load_rule(
    vm: &mut BexVm,
    rule: &ProgramImplRuleFrag,
    space: &Space,
    image: &Image,
    type_count: usize,
) -> Result<(HeapPtr, HeapPtr), VmRustFnError> {
    let interface_ptr = image.objects[space.object(rule.interface_head.raw())?];
    if !matches!(vm.get_object(interface_ptr), Object::Interface(_)) {
        return Err(link_error(
            "an `implements` rule names an object that is not an interface",
        ));
    }
    let mut methods = IndexMap::new();
    for (name, body) in &rule.methods {
        let local = space
            .locals
            .get(type_count + body.code_offset as usize)
            .ok_or_else(|| {
                link_error(format!(
                    "impl method `{name}` references code offset {} outside the unit",
                    body.code_offset
                ))
            })?;
        let fqn = image.objects[*local];
        if !matches!(vm.get_object(fqn), Object::Function(function) if function.is_interface_body) {
            return Err(link_error(format!(
                "impl method `{name}` resolves to an object that is not an interface body"
            )));
        }
        let mut frame = body.frame.clone();
        for template in &mut frame {
            relocate_template(template, space, image)?;
        }
        methods.insert(name.clone(), MethodImpl { fqn, frame });
    }
    let mut for_ty_pattern = rule.for_ty_pattern.clone();
    relocate_template(&mut for_ty_pattern, space, image)?;
    let mut generic_param_bounds = rule.generic_param_bounds.clone();
    for bound in generic_param_bounds.iter_mut().flatten() {
        bound.interface = image.head(space.object(head_operand(&bound.interface)?.raw())?)?;
        for arg in &mut bound.args {
            relocate_template(arg, space, image)?;
        }
        for (_, assoc) in &mut bound.assoc {
            relocate_template(assoc, space, image)?;
        }
    }
    let mut interface_args = rule.interface_args.clone();
    for arg in &mut interface_args {
        relocate_template(arg, space, image)?;
    }
    let mut interface_assoc = rule.interface_assoc.clone();
    for (_, assoc) in &mut interface_assoc {
        relocate_template(assoc, space, image)?;
    }
    let rule_ptr = vm.alloc(Object::ImplRule(Box::new(RuntimeImplRule {
        interface_head: interface_ptr,
        for_ty_pattern,
        generic_param_bounds,
        interface_args,
        interface_assoc,
        methods,
        field_links: rule.field_links.clone(),
    })));
    Ok((interface_ptr, rule_ptr))
}

/// The unit's objects in operand order and the bucket arithmetic behind it.
trait UnitObjects {
    fn objects(&self) -> impl Iterator<Item = &Object>;
    fn type_object_count(&self) -> usize;
    fn local_index(&self, local: LocalRef) -> usize;
}

impl UnitObjects for CompilationUnit {
    fn objects(&self) -> impl Iterator<Item = &Object> {
        self.classes
            .iter()
            .chain(&self.enums)
            .chain(&self.interfaces)
            .chain(&self.type_alias_objects)
            .chain(&self.code)
    }

    fn type_object_count(&self) -> usize {
        self.classes.len()
            + self.enums.len()
            + self.interfaces.len()
            + self.type_alias_objects.len()
    }

    fn local_index(&self, local: LocalRef) -> usize {
        let classes = self.classes.len();
        let enums = self.enums.len();
        let interfaces = self.interfaces.len();
        let aliases = self.type_alias_objects.len();
        match local {
            LocalRef::Class(k) => k as usize,
            LocalRef::Enum(k) => classes + k as usize,
            LocalRef::Interface(k) => classes + enums + k as usize,
            LocalRef::TypeAlias(k) => classes + enums + interfaces + k as usize,
            LocalRef::Code(k) => classes + enums + interfaces + aliases + k as usize,
        }
    }
}
