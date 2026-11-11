//! Unified heap for BEX virtual machine.
//!
//! Compile-time objects (functions, classes, enums, literals) live in a
//! permanent `Vec<Object>`. Runtime objects live in cells of a block space
//! ([`crate::space`]), allocated by per-mutator [`Tlab`](crate::Tlab)s and
//! copied or reclaimed by [`BexHeap::collect`].
//!
//! # Thread Safety
//!
//! - Each mutator allocates in blocks it owns; taking and returning a block
//!   is the only allocation step that locks.
//! - BAML has no global mutable variables, so independent calls can't race
//! - GC only runs when all VMs are at safepoints (yielded)

use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use ::bex_vm_types::{Value, errors::StackFrame, types::FutureId};
use bex_external_types::{Handle, WeakHeapRef};
use bex_vm_types::{HeapPtr, Object};

use crate::{
    HeapDebuggerConfig, HeapDebuggerState,
    descriptor::{Descriptor, DescriptorError, DescriptorId, DescriptorTable},
    space::{
        BLOCK_BYTES, block::BlockHeader, blocks::BlockSpace, cell_of, large::LargeSpace,
        mark::is_cell_marked,
    },
};

/// Where an object lives. A collection moves an object within the runtime
/// space, never between spaces, so this never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    /// Permanent compile-time objects (never collected).
    CompileTime,
    /// The collected mark-region space.
    Runtime,
}

/// Error payload preserved from an unreachable, never-observed spawned
/// future. The engine drains these after the GC pause.
#[derive(Debug, Clone, PartialEq)]
pub struct UnhandledSpawnError {
    pub future_id: FutureId,
    pub value: Value,
    pub trace: Vec<StackFrame>,
    /// Cancellation reached the task, by any route: see
    /// [`bex_vm_types::Future::was_cancelled`].
    pub cancelled: bool,
}

/// Statistics about heap usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapStats {
    /// Total objects allocated (including compile-time).
    pub total_objects: usize,
    /// Compile-time objects (permanent).
    pub compile_time_objects: usize,
    /// Runtime objects allocated and not yet reclaimed, dead ones included.
    pub runtime_objects: usize,
    /// Number of active handles.
    pub active_handles: usize,
    /// Blocks carved from the system, free ones included.
    pub blocks: usize,
    /// Empty blocks in the pool, waiting to be bumped into.
    pub free_blocks: usize,
    /// Objects above the large-object threshold.
    pub large_objects: usize,
    /// Bytes held from the system: every block plus every large object.
    pub reserved_bytes: usize,
    /// Bytes the heap occupies for objects: every block with a cell in it
    /// (or a mutator's cursor), plus every large object. Over `live_bytes`,
    /// this is the fragmentation a long run accumulates.
    pub heap_bytes: usize,
    /// Bytes of the cells the last collection found live, large ones
    /// included. Zero before the first collection.
    pub live_bytes: usize,
}

/// BEP-042: the instances a collection checks for finalization, so it need not
/// walk every object in from-space.
///
/// Invariant, unless `stale`: every `Instance` in gen0 or gen2 whose
/// class has a `cleanup` finalizer is in `instances`. Entries may repeat, and
/// may name an instance whose class has since lost its finalizer or that was
/// already cleaned: a collection re-checks each entry exactly as a full walk
/// would, so extra entries change nothing.
///
/// Kept by:
/// - [`BexHeap::register_finalizable`], called when such an instance is
///   allocated ([`Tlab::alloc`](crate::Tlab::alloc)) or, through
///   [`BexHeap::note_rewrite`], written in place (the VM's `ObjectMut`, or
///   [`Tlab::publish`](crate::Tlab::publish) filling a reservation). That
///   covers compile-time classes, runtime-compiled and session classes, and an
///   instance whose class changes.
/// - [`BexHeap::invalidate_finalizable_instances`], when a rewrite gives a
///   class a finalizer: instances of it may exist, unregistered, so the next
///   collection rebuilds the list from a walk of every object before it
///   marks anything.
/// - Every collection, which drops the entries of dead instances.
#[derive(Default)]
pub(crate) struct FinalizableInstances {
    pub(crate) instances: Vec<HeapPtr>,
    pub(crate) stale: bool,
}

/// What an object is to the collector's finalizable list (BEP-042). An
/// in-place rewrite that changes it must be reported with
/// [`BexHeap::note_rewrite`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finalizable {
    Neither,
    /// A class with a `cleanup` finalizer.
    Class,
}

impl Finalizable {
    /// What `object` is to the finalizable list.
    ///
    /// # Safety
    ///
    /// None: an instance is never rewritten in place (it is not an
    /// `Object`), so only a class can change what it is.
    pub fn of(object: &Object) -> Self {
        match object {
            Object::Class(class) if class.cleanup.is_some() => Self::Class,
            _ => Self::Neither,
        }
    }
}

/// A compile-time object with the header every heap object has, so a
/// pointer to it reads its format like any other (`HeapPtr::format`).
#[repr(C)]
struct CompileTimeCell {
    header: bex_vm_types::object_format::Header,
    object: Object,
}

impl CompileTimeCell {
    fn new(object: Object) -> Self {
        Self {
            header: bex_vm_types::object_format::Header::new(crate::OBJECT_DESC, 0),
            object,
        }
    }
}

/// Unified heap for the BEX virtual machine.
///
/// All heap-allocated objects live here. The heap is shared across
/// all VM instances via `Arc<BexHeap>`.
///
/// # Example
///
/// ```ignore
/// let heap: Arc<BexHeap> = BexHeap::new(compile_time_objects);
/// ```
pub struct BexHeap {
    /// Compile-time objects (never collected).
    /// These are permanent: functions, classes, enums, string literals.
    compile_time: Vec<CompileTimeCell>,
    pub(crate) functions: btel_types::FunctionLookup<HeapPtr>,
    pub(crate) span_definitions: std::sync::Mutex<HashMap<String, btel_types::FunctionId>>,

    /// Blocks of the mark-region space and the small-object policy's lists.
    /// Locked to take or return a block, and by a collection for its whole
    /// pause.
    pub(crate) blocks: parking_lot::Mutex<BlockSpace>,

    /// Bytes the last collection found live: survivors' cells, large ones
    /// included. Excludes what objects keep alive outside the heap.
    pub(crate) live_bytes: AtomicUsize,

    /// Objects above the large-object threshold.
    pub(crate) large: parking_lot::Mutex<LargeSpace>,

    /// How to read each kind of cell. Fixed once the heap is sealed.
    descriptors: DescriptorTable,

    /// Handle table for external/FFI boundary.
    ///
    /// Maps handle keys to HeapPtr values. Handles provide safe,
    /// validated access to heap objects from external code.
    ///
    /// Uses `RwLock<HashMap>` instead of sharded_slab to allow in-place
    /// updates after GC moves objects.
    pub(crate) handles: RwLock<HashMap<usize, HeapPtr>>,

    /// The live handle for each held object, so one object has one key.
    ///
    /// [`Handle`]'s `Eq` is its slab key plus its issuing heap, so a key that
    /// is unique *per object* rather than per call makes handle equality mean
    /// "the same object" — which is what lets a host compare two references to
    /// one runtime declaration. Minting per call instead would make a type
    /// mentioning the same class twice look like two classes.
    ///
    /// Held weakly: the entry must not keep alive the very [`HandleInner`]
    /// whose drop releases it. A [`Weak`] that fails to upgrade is a handle
    /// mid-drop, and the mint path treats it as absent.
    ///
    /// LOCK ORDER: any path taking both maps takes `handles` first, then this
    /// — mint, release, and GC fix-up all agree. A handle can drop on any
    /// thread at any moment (its `Drop` takes `handles` then this), so a path
    /// holding this lock while wanting `handles` is an AB-BA deadlock waiting
    /// for exactly that drop.
    ///
    /// [`HandleInner`]: bex_external_types::HandleInner
    handles_by_ptr: RwLock<HashMap<HeapPtr, (usize, Weak<bex_external_types::HandleInner>)>>,

    /// Next handle key to allocate.
    next_handle_key: AtomicUsize,

    /// BEP-042: instances whose `cleanup` finalizer must run after the current
    /// collection. Populated during a collection when a dead-but-not-yet-cleaned
    /// instance of a class with a finalizer is discovered and kept alive.
    /// Taken by the engine via
    /// [`take_pending_finalizers`](Self::take_pending_finalizers) inside the same
    /// pause and rooted as handles there, so they never outlive the pause as raw
    /// pointers.
    ///
    /// Each entry pairs the instance's `HeapPtr` with its class's
    /// `cleanup` function, read off the class by the GC while it has the
    /// class in hand — so the engine drain needs no heap read to dispatch.
    pub(crate) pending_finalizers: Mutex<Vec<(HeapPtr, HeapPtr)>>,

    /// Unobserved errors discovered while collecting unreachable futures.
    /// Entries are drained before execution
    /// resumes or another collection runs.
    pending_unhandled_spawn_errors: Mutex<Vec<UnhandledSpawnError>>,

    /// BEP-042 fast path: `true` iff some `Class` on this heap opts into a
    /// `cleanup` finalizer (`Class::cleanup`): computed over the compile-time
    /// classes at construction and raised when a graft adopts a class with one
    /// ([`note_finalizable_class`](Self::note_finalizable_class)); never lowered.
    /// When `false`, the per-collection finalizer scan
    /// ([`scan_dead_finalizers`](Self::scan_dead_finalizers)) is skipped
    /// entirely. When `true` (the stdlib's `Stream` defines `cleanup`, so in
    /// practice always), the scan checks only [`Self::finalizable`].
    pub(crate) has_finalizable_classes: AtomicBool,

    /// BEP-042: the instances a collection checks for finalization; see
    /// [`FinalizableInstances`].
    pub(crate) finalizable: Mutex<FinalizableInstances>,

    pub(crate) gc_policy: crate::gc_policy::AllocationBudget,
    root_release_epoch: AtomicUsize,
    gc_activity: Arc<tokio::sync::Notify>,

    /// Actual object allocations since last GC (profiling only). A shared
    /// counter every allocation would write, so it exists only in builds
    /// that read it.
    #[cfg(feature = "gc_profiling")]
    allocs_since_gc: AtomicUsize,

    /// Debug instrumentation state and config.
    debug_state: HeapDebuggerState,
}

// SAFETY: BexHeap is Send + Sync because:
// - compile_time: immutable after sealing
// - blocks / large: behind mutexes; a block's cells are written only by the
//   mutator owning the block, or by the collector while every mutator is
//   parked
// - handles: RwLock<HashMap> is thread-safe
unsafe impl Send for BexHeap {}
unsafe impl Sync for BexHeap {}

/// After a collection, a pointer names a live object iff it is compile-time
/// (never collected), or its cell is marked and still allocated: a pinned or
/// large cell marked in place, or a to-space copy. A dead cell, and a moved
/// cell's old copy, lost its start bit (a small cell) or its allocation (a
/// large one), so the verdict is read from the block tables and large-object
/// set, never from the memory. For a holder forwarded from a [`Restamp`]
/// after the pause; a holder walked in the pause gets its verdicts from the
/// marker.
///
/// [`Restamp`]: crate::gc::Restamp
impl bex_vm_types::Survival for BexHeap {
    fn survived(&self, ptr: HeapPtr) -> bool {
        if self.is_compile_time_ptr(ptr) {
            return true;
        }
        let cell = cell_of(ptr.as_ptr() as usize);
        if let Some(base) = self.blocks.lock().pool().block_containing(cell) {
            // SAFETY: an initialized block; only its tables are read.
            let header = unsafe { BlockHeader::at(base) };
            let granule = BlockHeader::granule(base, cell);
            return header.starts.get(granule) && header.marks.get(granule);
        }
        // SAFETY: a member of the large-object set is allocated.
        self.large.lock().contains_cell(cell) && unsafe { is_cell_marked(cell) }
    }
}

// Implement WeakHeapRef trait from bex_external_types
impl WeakHeapRef for BexHeap {
    fn release_handle(&self, handle_key: usize) {
        let mut handles = self.handles.write().expect("handles lock poisoned");
        let Some(ptr) = handles.remove(&handle_key) else {
            return;
        };
        // Only clear the reverse entry if it still names *this* key: a handle
        // for the same object may already have been re-minted between the last
        // clone dropping and this release running.
        let mut by_ptr = self.handles_by_ptr.write().expect("handles lock poisoned");
        if by_ptr.get(&ptr).is_some_and(|(key, _)| *key == handle_key) {
            by_ptr.remove(&ptr);
        }
        drop(by_ptr);
        drop(handles);
        self.notify_root_released();
    }

    fn resolve_handle_ptr(&self, slab_key: usize) -> Option<HeapPtr> {
        let handles = self.handles.read().expect("handles lock poisoned");
        handles.get(&slab_key).copied()
    }
}

impl BexHeap {
    /// Create a new heap with compile-time objects.
    ///
    /// The provided objects become permanent (never garbage collected).
    /// Runtime allocations will start after these objects.
    pub fn new(compile_time_objects: Vec<Object>) -> Arc<Self> {
        Self::build_unsealed_default(compile_time_objects).seal()
    }

    /// Create a new heap with explicit debug configuration.
    pub fn with_debug(compile_time_objects: Vec<Object>, debug: HeapDebuggerConfig) -> Arc<Self> {
        Self::build_unsealed(compile_time_objects, debug).seal()
    }

    /// Build the heap but **don't** seal it behind the shared `Arc` yet.
    ///
    /// The compile-time objects are laid out at their final, stable addresses, so
    /// [`Self::compile_time_ptr`] already returns valid pointers — but they can
    /// still be overwritten with [`Self::set_compile_time_object`]. This is how
    /// cross-referencing compile-time objects (packages and impl rules) are built:
    /// their `HeapPtr` fields are only knowable once the compile-time `Vec` exists,
    /// yet the objects themselves must live inside it. The caller appends
    /// placeholder slots, builds unsealed, fills each slot using
    /// `compile_time_ptr`, then calls [`Self::seal`].
    pub fn build_unsealed(
        mut compile_time_objects: Vec<Object>,
        debug: HeapDebuggerConfig,
    ) -> Self {
        // Resolve bytecode constants for all Function objects before sealing.
        // This converts ConstValue (with ObjectIndex) to Value (with HeapPtr).
        // BEP-042: precompute whether any class opts into a `cleanup` finalizer,
        // so the per-collection finalizer scan can be skipped when none do.
        let has_finalizable_classes = compile_time_objects
            .iter()
            .any(|o| matches!(o, Object::Class(c) if c.cleanup.is_some()));
        let mut compile_time: Vec<CompileTimeCell> = compile_time_objects
            .drain(..)
            .map(CompileTimeCell::new)
            .collect();
        Self::resolve_function_constants(&mut compile_time);

        Self {
            compile_time,
            functions: btel_types::FunctionLookup::default(),
            span_definitions: std::sync::Mutex::default(),
            blocks: parking_lot::Mutex::new(BlockSpace::default()),
            live_bytes: AtomicUsize::new(0),
            large: parking_lot::Mutex::new(LargeSpace::default()),
            descriptors: DescriptorTable::new(),
            handles: RwLock::new(HashMap::new()),
            handles_by_ptr: RwLock::new(HashMap::new()),
            next_handle_key: AtomicUsize::new(0),
            pending_finalizers: Mutex::new(Vec::new()),
            pending_unhandled_spawn_errors: Mutex::new(Vec::new()),
            has_finalizable_classes: AtomicBool::new(has_finalizable_classes),
            finalizable: Mutex::new(FinalizableInstances::default()),
            gc_policy: crate::gc_policy::AllocationBudget::new(),
            root_release_epoch: AtomicUsize::new(0),
            gc_activity: Arc::new(tokio::sync::Notify::new()),
            #[cfg(feature = "gc_profiling")]
            allocs_since_gc: AtomicUsize::new(0),
            debug_state: HeapDebuggerState::new(debug),
        }
    }

    /// [`Self::build_unsealed`] with the env-derived debug config (the same
    /// default [`Self::new`] uses).
    pub fn build_unsealed_default(compile_time_objects: Vec<Object>) -> Self {
        Self::build_unsealed(compile_time_objects, HeapDebuggerConfig::from_env())
    }

    /// Add a descriptor before the heap is [sealed](Self::seal). Entry
    /// [`OBJECT_DESC`](crate::OBJECT_DESC) is always the VM's `Object`.
    pub fn register_descriptor(
        &self,
        descriptor: Descriptor,
    ) -> Result<DescriptorId, DescriptorError> {
        self.descriptors.register(descriptor)
    }

    /// How to read each kind of cell.
    pub fn descriptors(&self) -> &DescriptorTable {
        &self.descriptors
    }

    /// Freeze the heap behind the shared `Arc`. Compile-time slots can no
    /// longer be replaced. See [`Self::build_unsealed`].
    pub fn seal(mut self) -> Arc<Self> {
        // Assign IDs only after all placeholders have been replaced. The Vec
        // and its pointers remain stable for the lifetime of the sealed heap.
        for index in 0..self.compile_time.len() {
            let ptr = self.compile_time_ptr(index);
            if let Object::Function(function) = &mut self.compile_time[index].object {
                function.telemetry_function_id = Some(
                    self.functions
                        .register_static(ptr, &function.telemetry_registration)
                        .expect("compile-time functions exceed the FunctionId space"),
                );
            }
        }
        Arc::new(self)
    }

    /// Overwrite a compile-time object before the heap is [sealed](Self::seal).
    ///
    /// The compile-time `Vec` is not resized, so every pointer already handed out
    /// by [`Self::compile_time_ptr`] stays valid — this only rewrites the slot's
    /// contents. Used to fill placeholder package / impl-rule slots with their
    /// resolved cross-references.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn set_compile_time_object(&mut self, index: usize, object: Object) {
        self.compile_time[index].object = object;
    }

    /// A compile-time object, mutably, before the heap is [sealed](Self::seal):
    /// what the loader fills in once every pointer is known (a declaration's
    /// owner edge to its package).
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn compile_time_object_mut(&mut self, index: usize) -> &mut Object {
        &mut self.compile_time[index].object
    }

    /// Resolve every pooled `ObjectIndex` operand into a `HeapPtr` before the
    /// heap is sealed: bytecode constants of every `Function`, and each
    /// interface's default-method bodies.
    ///
    /// Converts ConstValue (compile-time, with ObjectIndex) to Value (runtime, with HeapPtr).
    /// Must be called before wrapping in Arc since we need mutable access.
    fn resolve_function_constants(cells: &mut [CompileTimeCell]) {
        // First, compute pointers for all objects (they're at stable positions in the slice)
        let base_ptr = cells.as_ptr();

        let resolve_idx = |idx: bex_vm_types::ObjectIndex| -> bex_vm_types::HeapPtr {
            let ptr = unsafe { &raw const (*base_ptr.add(idx.into_raw())).object }.cast_mut();
            #[cfg(feature = "heap_debug")]
            unsafe {
                bex_vm_types::HeapPtr::from_ptr(ptr, 0)
            }
            #[cfg(not(feature = "heap_debug"))]
            unsafe {
                bex_vm_types::HeapPtr::from_ptr(ptr)
            }
        };

        for obj in cells.iter_mut().map(|cell| &mut cell.object) {
            if let Object::Function(func) = obj {
                // Resolve each constant, converting ObjectIndex to HeapPtr.
                // ConstValue::Type is NOT pre-resolved here — it must be
                // materialised at runtime by the LoadType instruction, which
                // reads directly from `bytecode.constants`.  We store a Null
                // placeholder so the resolved_constants vec stays index-aligned.
                func.bytecode.resolved_constants = func
                    .bytecode
                    .constants
                    .iter()
                    .map(|cv| match cv {
                        bex_vm_types::ConstValue::Type(_) => bex_vm_types::Value::NULL,
                        // ClassWithTypeArgs and Literal are NOT pre-resolved:
                        // `IsType` reads them directly from `constants` at
                        // execution time.
                        bex_vm_types::ConstValue::ClassWithTypeArgs { .. }
                        | bex_vm_types::ConstValue::Literal(_) => bex_vm_types::Value::NULL,
                        other => other.to_value(resolve_idx),
                    })
                    .collect();
            } else if let Object::Interface(interface) = obj {
                if let Some(default) = &mut interface.structural_default {
                    default.function_ptr = resolve_idx(default.function);
                }
                // The one place a static interface's default body becomes a
                // pointer: from here on, a witness reads `default_fn` and never
                // resolves a name.
                for method in &mut interface.methods {
                    if let Some(default) = method.default {
                        method.default_fn = resolve_idx(default);
                    }
                }
            } else if let Object::Class(class) = obj {
                // Likewise the one place a static class's inherent methods
                // become pointers.
                for method in class.methods.values_mut() {
                    method.function_ptr = resolve_idx(method.function);
                }
            }
        }
    }

    /// Bind every [`bex_vm_types::TypeHead`] in the compile-time image to the
    /// declaration it names.
    ///
    /// This is the loader's one-time serialization boundary: the linker
    /// assigned every declaration of the image the tag `CLASS_BASE + its
    /// object index`, and every head carries that tag, so binding is
    /// arithmetic — the tag names the pool slot — checked against the
    /// declaration's own tag field. No index is built and nothing survives
    /// into the VM.
    ///
    /// Must run after every placeholder slot is filled (impl rules carry heads
    /// in their patterns) and before the heap seals. The closing assertion is
    /// deliberately total and active in release builds: an unresolved head is
    /// not a degraded mode, it is a pointer the collector cannot trace and a
    /// dispatch that cannot resolve, so a program image that cannot bind
    /// completely must not run at all.
    pub fn bind_type_heads(&mut self) {
        let tags: Vec<Option<baml_type::typetag::TypeTag>> = self
            .compile_time
            .iter()
            .map(|cell| Self::declaration_tag(&cell.object))
            .collect();
        let ptrs: Vec<HeapPtr> = (0..self.compile_time.len())
            .map(|index| self.compile_time_ptr(index))
            .collect();
        for object in self.compile_time.iter_mut().map(|cell| &mut cell.object) {
            bex_vm_types::head_walk::visit_object_heads_mut(object, &mut |head| {
                if head.is_resolved() {
                    return;
                }
                let tag = head.tag();
                if let Some(index) = tag.static_index()
                    && tags.get(index).copied().flatten() == Some(tag)
                {
                    head.resolve(ptrs[index]);
                }
            });
        }
        let mut unbound: Vec<String> = Vec::new();
        for object in self.compile_time.iter().map(|cell| &cell.object) {
            bex_vm_types::head_walk::visit_object_heads(object, &mut |head| {
                if head.is_resolved() {
                    return;
                }
                let context = match object {
                    Object::Class(class) => format!("class {}", class.name()),
                    Object::Enum(enm) => format!("enum {}", enm.name()),
                    Object::Interface(interface) => format!("interface {}", interface.name()),
                    Object::TypeAlias(alias) => format!("type alias {}", alias.name()),
                    Object::Function(function) => format!("function {}", function.name()),
                    Object::ImplRule(rule) => format!("impl rule at {:?}", rule.interface_head),
                    other => format!("{:?} object", bex_vm_types::ObjectType::of(other)),
                };
                unbound.push(format!(
                    "  {context}: head {:?} (tag {:?})",
                    head.declared_name(),
                    head.tag(),
                ));
            });
        }
        assert!(
            unbound.is_empty(),
            "compile-time image contains heads nothing declares:\n{}",
            unbound.join("\n"),
        );
    }

    /// Refuse a compile-time image holding a switch table that cannot
    /// dispatch: one that still states its keys (whoever laid the image out
    /// solves every table), or a solved one that lacks the shape its kind
    /// states.
    ///
    /// Total and active in release builds, like [`Self::bind_type_heads`]:
    /// the interpreter dispatches through these tables without looking at
    /// them first, so an image with one it cannot answer must not run.
    pub fn assert_switch_tables_dispatch(&self) {
        let undispatchable: Vec<String> = self
            .compile_time
            .iter()
            .filter_map(|cell| match &cell.object {
                Object::Function(function) => Some(function),
                _ => None,
            })
            .flat_map(|function| {
                function
                    .bytecode
                    .switch_tables
                    .iter()
                    .enumerate()
                    .filter(|(_, table)| !table.dispatch.is_dispatchable())
                    .map(|(index, table)| {
                        format!(
                            "  function {}: switch table {index} over [{}]",
                            function.name(),
                            table.key_names.join(", ")
                        )
                    })
            })
            .collect();
        assert!(
            undispatchable.is_empty(),
            "compile-time image contains switch tables that cannot dispatch:\n{}",
            undispatchable.join("\n"),
        );
    }

    /// The tag under which `object` can head a nominal type, or `None` when it
    /// is not a declaration.
    pub fn declaration_tag(object: &Object) -> Option<baml_type::typetag::TypeTag> {
        object.declaration_tag()
    }

    /// Get the number of compile-time objects.
    pub fn compile_time_len(&self) -> usize {
        self.compile_time.len()
    }

    /// Borrow a compile-time object. These slots are immutable after sealing
    /// and are never moved or collected.
    pub(crate) fn compile_time_object(&self, index: usize) -> &Object {
        &self.compile_time[index].object
    }

    /// Get the compile-time boundary index (alias for compile_time_len).
    ///
    /// Objects before this index are permanent. Objects at or after
    /// this index are runtime allocations that can be garbage collected.
    pub fn compile_time_boundary(&self) -> usize {
        self.compile_time.len()
    }

    /// Check if a pointer refers to a compile-time object.
    ///
    /// Returns true if the pointer falls within the compile_time Vec's memory range.
    #[inline]
    pub fn is_compile_time_ptr(&self, ptr: HeapPtr) -> bool {
        if self.compile_time.is_empty() {
            return false;
        }
        self.compile_time_range().contains(&(ptr.as_ptr() as usize))
    }

    /// Addresses of the compile-time objects.
    pub(crate) fn compile_time_range(&self) -> Range<usize> {
        let range = self.compile_time.as_ptr_range();
        range.start as usize..range.end as usize
    }

    /// Get a HeapPtr to a compile-time object by index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[inline]
    pub fn compile_time_ptr(&self, index: usize) -> HeapPtr {
        assert!(
            index < self.compile_time.len(),
            "compile-time index {index} out of bounds (len={})",
            self.compile_time.len()
        );
        let raw_ptr = (&raw const self.compile_time[index].object).cast_mut();
        // SAFETY: The pointer is valid and points to a compile-time object
        // that will never be moved or deallocated.
        unsafe { self.make_heap_ptr(raw_ptr) }
    }

    /// Which space `ptr` lives in.
    #[inline]
    pub fn generation_of(&self, ptr: HeapPtr) -> Generation {
        if self.is_compile_time_ptr(ptr) {
            Generation::CompileTime
        } else {
            Generation::Runtime
        }
    }

    /// Monotonic change token for GC roots disappearing, including after
    /// the last engine call. No heap permit is required to read this token.
    pub fn root_release_epoch(&self) -> usize {
        self.root_release_epoch.load(Ordering::Acquire)
    }

    /// Record a removed root and wake idle cleanup, including when no work ends.
    /// Call after removing the root; registry callers must still hold their heap permit.
    pub fn notify_root_released(&self) {
        self.root_release_epoch.fetch_add(1, Ordering::Release);
        self.gc_activity.notify_one();
    }

    /// Dedicated wake signal for the engine's single idle-GC coordinator.
    pub fn gc_activity(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.gc_activity)
    }

    /// Get the current number of objects in the heap (compile-time plus
    /// runtime, dead runtime objects not yet swept included).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.compile_time.len() + self.runtime_objects()
    }

    /// Runtime cells allocated and not yet swept.
    fn runtime_objects(&self) -> usize {
        let blocks: usize = self
            .blocks
            .lock()
            .pool()
            .blocks()
            .iter()
            // SAFETY: the pool's blocks are initialized; only counts are read.
            .map(|&base| unsafe { BlockHeader::at(base) }.starts.count_ones())
            .sum();
        blocks + self.large.lock().len()
    }

    /// Read an object by HeapPtr (direct pointer dereference).
    ///
    /// # Safety
    ///
    /// - The pointer must be valid (not collected by GC)
    /// - Caller must ensure no concurrent writes to this object
    pub unsafe fn get_object(&self, idx: HeapPtr) -> &Object {
        #[cfg(feature = "heap_debug")]
        self.debug_assert_valid_index(idx);

        // SAFETY: HeapPtr points directly to the object
        let obj = unsafe { idx.get() };

        self.debug_assert_not_sentinel(obj);
        obj
    }

    /// Blocks and cells, from a walk of every block's tables.
    pub fn block_occupancy(&self) -> crate::BlockOccupancy {
        self.blocks.lock().occupancy()
    }

    /// Get statistics about heap usage.
    pub fn stats(&self) -> HeapStats {
        let ct_len = self.compile_time.len();
        let runtime = self.runtime_objects();
        let blocks = self.blocks.lock().stats();
        let (large_objects, large_bytes) = {
            let large = self.large.lock();
            (large.len(), large.bytes())
        };
        HeapStats {
            total_objects: ct_len + runtime,
            compile_time_objects: ct_len,
            runtime_objects: runtime,
            active_handles: self.handles.read().expect("handles lock poisoned").len(),
            blocks: blocks.blocks,
            free_blocks: blocks.free_blocks,
            large_objects,
            reserved_bytes: blocks.blocks * BLOCK_BYTES + large_bytes,
            heap_bytes: (blocks.blocks - blocks.free_blocks) * BLOCK_BYTES + large_bytes,
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
        }
    }

    /// Whether the heap has spent its allocation allowance since the last full GC.
    pub fn should_gc(&self) -> bool {
        self.gc_policy.due()
    }

    /// Reset the allocation counter after GC.
    #[cfg(feature = "gc_profiling")]
    pub(crate) fn reset_gc_counter(&self) {
        self.allocs_since_gc.store(0, Ordering::Relaxed);
    }

    /// Increment allocation counter (called by TLAB on alloc).
    #[cfg(feature = "gc_profiling")]
    pub(crate) fn record_alloc(&self) {
        self.allocs_since_gc.fetch_add(1, Ordering::Relaxed);
    }

    /// Load the current allocation count since last GC.
    #[cfg(feature = "gc_profiling")]
    pub(crate) fn allocs_since_gc(&self) -> usize {
        self.allocs_since_gc.load(Ordering::Relaxed)
    }

    pub(crate) fn debug_state(&self) -> &HeapDebuggerState {
        &self.debug_state
    }

    /// Repoint every handle in the pause of a collection. Handles are
    /// roots, so every handle target survived; the reverse index is rebuilt
    /// under the new addresses when any moved.
    ///
    /// # Safety
    /// Between a complete mark and the reclaim.
    pub(crate) unsafe fn forward_handles(&self, marker: &mut crate::space::mark::Marker<'_>) {
        // LOCK ORDER: `handles` before `handles_by_ptr`.
        let mut handles = self.handles.write().expect("handles lock poisoned");
        let mut moved_any = false;
        for ptr in handles.values_mut() {
            // SAFETY: a handle's object is a root, so it was reached.
            let moved = unsafe { marker.forward(*ptr) };
            moved_any |= moved != *ptr;
            // SAFETY: stamping an address the collection just vouched for.
            *ptr = unsafe { self.make_heap_ptr(moved.as_ptr()) };
        }
        if !moved_any {
            return;
        }
        let mut by_ptr = self.handles_by_ptr.write().expect("handles lock poisoned");
        let old = std::mem::take(&mut *by_ptr);
        by_ptr.extend(old.into_iter().map(|(ptr, entry)| {
            // SAFETY: as above.
            let moved = unsafe { marker.forward(ptr) };
            // SAFETY: as above.
            (unsafe { self.make_heap_ptr(moved.as_ptr()) }, entry)
        }));
    }

    /// BEP-042: a class with a `cleanup` finalizer now lives on this heap — a
    /// runtime-compiled package or session declared one — so every collection
    /// from here on scans for finalizable instances.
    pub fn note_finalizable_class(&self) {
        self.has_finalizable_classes.store(true, Ordering::Release);
    }

    /// BEP-042: whether `instance` is an instance of a class with a
    /// `cleanup` finalizer, and so belongs in the collector's finalizable
    /// list.
    pub fn needs_finalizer_registration(instance: bex_vm_types::InstanceRef) -> bool {
        instance.class_def().cleanup.is_some()
    }

    /// How `class`'s instances are laid out, registering the layout's
    /// descriptor with the first instance.
    pub fn instance_layout<'c>(
        &self,
        class: &'c bex_vm_types::Class,
    ) -> &'c bex_vm_types::InstanceLayout {
        class.layout.get_or_init(|| {
            let (kinds, has_type_args) = bex_vm_types::InstanceLayout::plan(class);
            self.descriptors.intern_instance(&kinds, has_type_args)
        })
    }

    /// BEP-042: keep the finalizable list complete after the object at `ptr`
    /// was rewritten in place, from an object that was `before` to one that
    /// is `after`. One that became a class with a finalizer invalidates the
    /// list: instances of it may already exist, unregistered, since an
    /// instance is checked against its class only when it is allocated.
    pub fn note_rewrite(&self, before: Finalizable, after: Finalizable) {
        if after == before {
            return;
        }
        match after {
            Finalizable::Class => self.invalidate_finalizable_instances(),
            Finalizable::Neither => {}
        }
    }

    /// BEP-042: record `instance`, an instance of a class with a `cleanup`
    /// finalizer, so collections check it for finalization (see
    /// `FinalizableInstances`). Registering one twice is harmless.
    pub fn register_finalizable(&self, instance: HeapPtr) {
        self.finalizable
            .lock()
            .expect("finalizable lock poisoned")
            .instances
            .push(instance);
    }

    /// BEP-042: an existing class gained a `cleanup` finalizer. Its older
    /// instances were never registered, so the next collection rebuilds the
    /// finalizable list from a walk of every object.
    pub fn invalidate_finalizable_instances(&self) {
        self.has_finalizable_classes.store(true, Ordering::Release);
        self.finalizable
            .lock()
            .expect("finalizable lock poisoned")
            .stale = true;
    }

    /// BEP-042: record an instance and its class's `cleanup` function, to be
    /// finalized after the current collection.
    /// Called by the GC when it discovers and keeps alive a dead, not-yet-cleaned
    /// instance of a class with a finalizer.
    pub(crate) fn push_pending_finalizer(&self, instance: HeapPtr, cleanup: HeapPtr) {
        self.pending_finalizers
            .lock()
            .expect("pending_finalizers lock poisoned")
            .push((instance, cleanup));
    }

    /// BEP-042: take and clear the queue of instances awaiting `cleanup`. The
    /// engine takes this inside the collection's pause, roots each entry as a
    /// handle, and invokes each instance's `cleanup` once execution resumes.
    /// Each entry is `(instance, its class's cleanup function)`.
    pub fn take_pending_finalizers(&self) -> Vec<(HeapPtr, HeapPtr)> {
        std::mem::take(
            &mut *self
                .pending_finalizers
                .lock()
                .expect("pending_finalizers lock poisoned"),
        )
    }

    pub(crate) fn push_unhandled_spawn_error(&self, error: UnhandledSpawnError) {
        self.pending_unhandled_spawn_errors
            .lock()
            .expect("pending_unhandled_spawn_errors lock poisoned")
            .push(error);
    }

    pub fn take_unhandled_spawn_errors(&self) -> Vec<UnhandledSpawnError> {
        std::mem::take(
            &mut *self
                .pending_unhandled_spawn_errors
                .lock()
                .expect("pending_unhandled_spawn_errors lock poisoned"),
        )
    }

    /// Create a handle to an object.
    ///
    /// Handles are used at the FFI boundary to give external code safe
    /// access to heap objects. Handles are GC roots - objects reachable
    /// from handles will not be collected.
    pub fn create_handle(self: &Arc<Self>, ptr: HeapPtr) -> Handle {
        // One key per object, not per call — see `handles_by_ptr`. Both write
        // locks span the check and the mint so a concurrent release cannot
        // retire the entry in between and leave two keys for one object.
        //
        // LOCK ORDER: `handles` before `handles_by_ptr` — the order every
        // other two-lock path takes (`release_handle`, `forward_handles`).
        // Taking `handles_by_ptr` first here deadlocked against a concurrent
        // `Handle` drop (AB-BA), which the future-cleanup stress test caught.
        let mut handles = self.handles.write().expect("handles lock poisoned");
        let mut by_ptr = self.handles_by_ptr.write().expect("handles lock poisoned");
        if let Some((_, weak)) = by_ptr.get(&ptr)
            && let Some(inner) = weak.upgrade()
        {
            // A live handle already names this object; sharing its `Arc` is
            // what counts the new reference.
            return Handle::from_inner(inner);
        }

        let handle_key = self.next_handle_key.fetch_add(1, Ordering::Relaxed);
        handles.insert(handle_key, ptr);
        // Handle no longer stores idx - always resolves through table
        let handle = Handle::new(handle_key, Arc::clone(self) as Arc<dyn WeakHeapRef>);
        by_ptr.insert(ptr, (handle_key, handle.downgrade_inner()));
        handle
    }

    /// Whether `handle` was issued by this heap.
    ///
    /// A slab key indexes one heap's handle table, so a handle from another
    /// engine must be rejected rather than resolved — see
    /// [`Handle::is_of_heap`].
    #[must_use]
    pub fn owns_handle(self: &Arc<Self>, handle: &Handle) -> bool {
        handle.is_of_heap(&(Arc::clone(self) as Arc<dyn WeakHeapRef>))
    }

    /// Collect all handle roots for garbage collection.
    ///
    /// Returns a Vec of HeapPtr values for all live handles.
    /// These should be treated as GC roots - objects reachable from
    /// handles must not be collected.
    pub fn collect_handle_roots(&self) -> Vec<HeapPtr> {
        self.handles
            .read()
            .expect("handles lock poisoned")
            .values()
            .copied()
            .collect()
    }
}

impl std::fmt::Debug for BexHeap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stats = self.stats();
        f.debug_struct("BexHeap")
            .field("compile_time_len", &stats.compile_time_objects)
            .field("runtime_objects", &stats.runtime_objects)
            .field("blocks", &stats.blocks)
            .field("large_objects", &stats.large_objects)
            .finish()
    }
}

impl Drop for BexHeap {
    /// Drop every runtime object still allocated: they own Rust storage
    /// (strings, vectors, host values) that must be released with the heap.
    fn drop(&mut self) {
        let blocks = self.blocks.get_mut();
        let large = self.large.get_mut();
        // SAFETY: `&mut self`: no mutator or collector can run.
        unsafe {
            Self::for_each_runtime_object(blocks.pool(), large, |object| {
                std::ptr::drop_in_place(object);
            });
        }
    }
}

// Static assertions to verify thread safety
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}

    // BexHeap must be Send + Sync for Arc<BexHeap> to work across threads
    assert_send::<BexHeap>();
    assert_sync::<BexHeap>();
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A bytecode function dispatching through `table`.
    fn function_switching_through(table: bex_vm_types::bytecode::SwitchTable) -> Object {
        use bex_vm_types::{
            Bytecode, Function, FunctionKind, bytecode::Instruction, types::FunctionOrigin,
        };
        Object::Function(Box::new(Function {
            identity: bex_vm_types::types::FunctionIdentity::Declared(
                bex_vm_types::types::FnPath::Free(bex_vm_types::types::LocalName::new(
                    Vec::new(),
                    baml_base::Name::new("pick"),
                )),
            ),
            source_file: "user.baml".to_string(),
            docstring: None,
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode {
                instructions: vec![Instruction::DenseTag(0), Instruction::Return],
                switch_tables: vec![table],
                ..Bytecode::default()
            },
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
            compiled_body: None,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type: bex_vm_types::TyTemplate::Unknown,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: bex_vm_types::TyTemplate::Never,
            origin: FunctionOrigin::Internal,
            native_key: None,
            body_meta: None,
            trace_passthrough: false,
            owner: HeapPtr::null(),
        }))
    }

    fn keys_of(values: &[i64]) -> Vec<bex_vm_types::bytecode::SwitchKey> {
        values
            .iter()
            .copied()
            .map(bex_vm_types::bytecode::SwitchKey::Kind)
            .collect()
    }

    #[test]
    fn an_image_whose_switches_are_solved_loads() {
        use bex_vm_types::bytecode::{SwitchDispatch, SwitchTable};
        let keys = keys_of(&[1, 100, 5_000, 70_000]);
        let heap = BexHeap::build_unsealed_default(vec![function_switching_through(SwitchTable {
            dispatch: SwitchDispatch::solved(&keys, |_| {
                unreachable!("a switch of kind keys names no declaration")
            }),
            key_names: vec!["one".to_string()],
        })]);
        heap.assert_switch_tables_dispatch();
    }

    /// Nothing dispatches through a table that only states its keys, so an
    /// image that holds one is refused where it is loaded rather than where
    /// a value first reaches the switch.
    #[test]
    #[should_panic(expected = "function pick: switch table 0 over [one, hundred]")]
    fn an_image_with_an_unsolved_switch_is_refused() {
        use bex_vm_types::bytecode::SwitchTable;
        let heap = BexHeap::build_unsealed_default(vec![function_switching_through(
            SwitchTable::of_keys(
                keys_of(&[1, 100]),
                vec!["one".to_string(), "hundred".to_string()],
            ),
        )]);
        heap.assert_switch_tables_dispatch();
    }

    /// One object, one key — so a host comparing two references to the same
    /// declaration sees one identity.
    #[test]
    fn handles_to_one_object_share_a_key() {
        let heap = BexHeap::new(vec![Object::String("shared".into())]);
        let ptr = heap.compile_time_ptr(0);

        let first = heap.create_handle(ptr);
        let second = heap.create_handle(ptr);
        assert_eq!(first, second, "one object must yield one handle identity");
        assert_eq!(
            heap.handles.read().unwrap().len(),
            1,
            "a shared handle occupies one slot, not one per request"
        );
    }

    /// The shared key outlives any single holder, and retires with the last.
    #[test]
    fn a_shared_handle_releases_once_every_holder_drops() {
        let heap = BexHeap::new(vec![Object::String("shared".into())]);
        let ptr = heap.compile_time_ptr(0);

        let first = heap.create_handle(ptr);
        let second = heap.create_handle(ptr);
        let key = first.slab_key();

        drop(first);
        assert_eq!(
            heap.resolve_handle_ptr(key),
            Some(ptr),
            "the object is still held by `second`"
        );

        drop(second);
        assert_eq!(
            heap.resolve_handle_ptr(key),
            None,
            "the last holder releases the object"
        );
        assert!(heap.handles_by_ptr.read().unwrap().is_empty());
    }

    /// A key retired by its last holder is not resurrected: the next request
    /// mints a fresh one rather than handing back a released slot.
    #[test]
    fn a_released_object_mints_a_new_handle() {
        let heap = BexHeap::new(vec![Object::String("shared".into())]);
        let ptr = heap.compile_time_ptr(0);

        let first = heap.create_handle(ptr);
        let first_key = first.slab_key();
        drop(first);

        let second = heap.create_handle(ptr);
        assert_ne!(
            second.slab_key(),
            first_key,
            "a fresh handle must not reuse a released key"
        );
        assert_eq!(heap.resolve_handle_ptr(second.slab_key()), Some(ptr));
    }

    #[test]
    fn test_new_heap_empty() {
        let heap = BexHeap::new(vec![]);
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.compile_time_boundary(), 0);
    }

    #[test]
    fn test_new_heap_with_objects() {
        let objects: Vec<Object> = vec![
            Object::String("hello".into()),
            Object::String("world".into()),
        ];
        let heap = BexHeap::new(objects);
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.compile_time_boundary(), 2);
    }

    #[test]
    fn test_heap_stats() {
        let compile_time: Vec<Object> = vec![Object::String("builtin".into())];
        let heap = BexHeap::new(compile_time);

        let stats = heap.stats();
        assert_eq!(stats.compile_time_objects, 1);
        assert_eq!(stats.total_objects, 1);
        assert_eq!(stats.runtime_objects, 0);
        assert_eq!(stats.blocks, 0);

        let mut tlab = crate::Tlab::new(Arc::clone(&heap));
        tlab.alloc_string("runtime");
        let stats = heap.stats();
        assert_eq!(stats.runtime_objects, 1);
        assert_eq!(stats.total_objects, 2);
        assert!(stats.blocks > 0);
        assert_eq!(stats.reserved_bytes, stats.blocks * BLOCK_BYTES);
    }

    // Note: Handle tests removed as they require HeapPtr creation which depends
    // on runtime allocation. Will be updated when full integration is complete.
}
