//! Unified heap for BEX virtual machine.
//!
//! The heap stores all objects in a single `Vec<Object>` with:
//! - Compile-time objects at indices 0..compile_time_boundary (permanent)
//! - Runtime objects at indices compile_time_boundary.. (collectible)
//!
//! # Thread Safety
//!
//! The heap uses `UnsafeCell<Vec<Object>>` for lock-free field writes.
//! Safety is ensured by:
//! - TLABs give each VM exclusive write access to its allocation region
//! - BAML has no global mutable variables, so independent calls can't race
//! - GC only runs when all VMs are at safepoints (yielded)

use std::{
    cell::UnsafeCell,
    collections::HashMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use ::bex_vm_types::{Value, errors::StackFrame, types::FutureId};
use bex_external_types::{Handle, WeakHeapRef};
use bex_vm_types::{HeapPtr, Object, WriteBarrier};

use crate::{
    HeapDebuggerConfig, HeapDebuggerState, card_table::CardTable, chunked_vec::ChunkedVec,
    tlab::TlabChunk,
};

/// Minimum Gen1 live count before a Minor GC is triggered by Gen1 pressure.
pub(crate) const GEN1_FLOOR: usize = 10_000;

/// Minimum Gen2 live count before a Major GC is triggered by Gen2 pressure.
pub(crate) const GEN2_FLOOR: usize = 50_000;

/// Which generation of the heap an object lives in.
///
/// The ordering `CompileTime < Gen0 < Gen1 < Gen2` is intentional: write barriers
/// use `container_gen > ref_gen` to decide whether to mark a card dirty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Generation {
    /// Permanent compile-time objects (never collected).
    CompileTime,
    /// Gen0 nursery — new allocations land here.
    Gen0,
    /// Gen1 intermediate — objects that survived one full GC.
    Gen1,
    /// Gen2 old generation — long-lived objects.
    Gen2,
}

/// Error payload preserved from an unreachable, never-observed spawned
/// future. The engine drains these after the GC pause.
#[derive(Debug, Clone, PartialEq)]
pub struct UnhandledSpawnError {
    pub future_id: FutureId,
    pub value: Value,
    pub trace: Vec<StackFrame>,
    pub cancelled: bool,
}
impl Generation {
    /// Check if this generation is young (Gen0 or Gen1).
    pub const fn is_young(self) -> bool {
        matches!(self, Generation::Gen0 | Generation::Gen1)
    }
}

/// Default TLAB chunk size (number of object slots).
///
/// This is the number of object slots each VM gets when it requests a new TLAB.
/// When a VM exhausts its TLAB, it atomically reserves the next `tlab_size` slots.
///
/// # Relationship to ChunkedVec chunk size
///
/// The underlying storage uses `ChunkedVec` with `DEFAULT_CHUNK_SIZE` (4096).
/// For optimal memory locality, TLAB size should divide evenly into the chunk size:
///
/// - `DEFAULT_CHUNK_SIZE = 4096` (storage chunks)
/// - `DEFAULT_TLAB_SIZE = 1024` (TLAB allocation unit)
/// - Result: 4 TLABs fit per storage chunk
///
/// This isn't strictly required (TLABs can span chunk boundaries), but aligned
/// TLABs have better cache behavior since all objects in a TLAB are contiguous.
pub const DEFAULT_TLAB_SIZE: usize = 1024;

// Compile-time assertion that default TLAB size divides evenly into chunk size
const _: () = assert!(
    crate::chunked_vec::DEFAULT_CHUNK_SIZE.is_multiple_of(DEFAULT_TLAB_SIZE),
    "DEFAULT_TLAB_SIZE should divide evenly into DEFAULT_CHUNK_SIZE for optimal alignment"
);

/// Statistics about heap usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapStats {
    /// Total objects allocated (including compile-time).
    pub total_objects: usize,
    /// Compile-time objects (permanent).
    pub compile_time_objects: usize,
    /// Runtime objects (collectible).
    pub runtime_objects: usize,
    /// Number of active handles.
    pub active_handles: usize,
    /// Number of TLAB chunks allocated.
    pub tlab_chunks: usize,
}

/// Unified heap for the BEX virtual machine.
///
/// All heap-allocated objects live here. The heap is shared across
/// all VM instances via `Arc<BexHeap>`.
///
/// # Generational Layout
///
/// The heap uses four `ChunkedVec<Object>` spaces:
/// - `compile_time`: Permanent objects (functions, classes, enums) - never collected
/// - `gen0`: Gen0 nursery — all new TLAB allocations land here
/// - `gen1`: Gen1 intermediate — survivors of one full GC cycle
/// - `gen2`: Gen2 old generation — long-lived objects
/// - `inactive`: Scratch space used as copy destination during full GC
///
/// During a full GC, live objects are copied from gen0+gen1+gen2 into inactive,
/// then inactive is swapped with gen2, and gen0+gen1 are cleared.
///
/// # Example
///
/// ```ignore
/// let heap: Arc<BexHeap> = BexHeap::new(compile_time_objects);
/// ```
pub struct BexHeap {
    /// Compile-time objects (never collected).
    /// These are permanent: functions, classes, enums, string literals.
    compile_time: Vec<Object>,

    /// Gen0 nursery — all TLAB allocations land here.
    /// Uses ChunkedVec for stable pointers during concurrent access.
    ///
    /// # Why ChunkedVec?
    ///
    /// With a regular Vec, if one VM is writing to an element while another
    /// VM triggers a resize (via TLAB chunk allocation), the Vec may reallocate
    /// and invalidate the first VM's pointer - that's undefined behavior.
    ///
    /// ChunkedVec stores objects in fixed-size chunks. Growing adds new chunks
    /// without moving existing data, so pointers remain stable even during
    /// concurrent growth.
    pub(crate) gen0: UnsafeCell<ChunkedVec<Object>>,

    /// Gen1 intermediate — survivors of one full GC.
    pub(crate) gen1: UnsafeCell<ChunkedVec<Object>>,

    /// Gen2 old generation — long-lived objects.
    pub(crate) gen2: UnsafeCell<ChunkedVec<Object>>,

    /// Inactive scratch space — copy destination during full GC.
    pub(crate) inactive: UnsafeCell<ChunkedVec<Object>>,

    /// Next TLAB chunk start index within Gen0.
    ///
    /// When a VM needs a new TLAB, it atomically increments this by the
    /// chunk size to reserve its region.
    gen0_next_chunk: AtomicUsize,

    /// Card table tracking dirty cards in Gen2.
    ///
    /// A card is marked dirty when a write barrier detects that a Gen2 object
    /// holds a reference to a Gen0 or Gen1 object. Used during partial collections.
    pub(crate) gen2_cards: UnsafeCell<CardTable>,

    /// Handle table for external/FFI boundary.
    ///
    /// Maps handle keys to HeapPtr values. Handles provide safe,
    /// validated access to heap objects from external code.
    ///
    /// Uses `RwLock<HashMap>` instead of sharded_slab to allow in-place
    /// updates after GC moves objects.
    pub(crate) handles: RwLock<HashMap<usize, HeapPtr>>,

    /// Next handle key to allocate.
    next_handle_key: AtomicUsize,

    /// Next `MintId::Runtime` counter value (BEP-066 I-1): one per structured
    /// type construction, engine-wide. Lives on the heap — next to the handle
    /// counter — because the heap is the one object every allocation path
    /// already shares, including spawned VMs (each `Tlab` holds an
    /// `Arc<BexHeap>`), so two threads can never mint the same runtime
    /// identity. Monotonic and never reused; runtime type constructors allocate
    /// identities through `bex_vm_types::types::MintId`.
    next_runtime_mint: AtomicU64,

    /// BEP-042: instances whose `cleanup` finalizer must run after the current
    /// collection. Populated during a collection (`copy_collection` /
    /// `copy_collection_minor`) when a dead-but-not-yet-cleaned instance of a
    /// `has_cleanup` class is discovered and kept alive; the pointers are the
    /// **new** (post-copy) locations. Drained by the engine via
    /// [`take_pending_finalizers`](Self::take_pending_finalizers) at the same
    /// safepoint, before any other collection runs, so the pointers stay valid.
    ///
    /// Each entry pairs the instance's post-copy `HeapPtr` with its resolved
    /// `cleanup` function name (`{class_fqn}.cleanup`), computed by the GC while
    /// it still has the class in hand — so the engine drain needs no heap read
    /// to dispatch.
    pending_finalizers: Mutex<Vec<(HeapPtr, String)>>,

    /// Unobserved errors discovered while collecting unreachable futures.
    /// Entries contain post-copy values and are drained before execution
    /// resumes or another collection runs.
    pending_unhandled_spawn_errors: Mutex<Vec<UnhandledSpawnError>>,

    /// BEP-042 fast path: `true` iff at least one compile-time `Class` opts into
    /// a `cleanup` finalizer (`has_cleanup`). Classes are fixed at compile time,
    /// so this is computed once at construction. When `false`, the per-collection
    /// finalizer scan ([`scan_dead_finalizers`](Self::scan_dead_finalizers)) is
    /// skipped entirely — programs that define no `cleanup` (the common case) pay
    /// no per-GC scan cost.
    pub(crate) has_finalizable_classes: bool,

    /// TLAB chunk size for new allocations.
    tlab_size: usize,

    /// Lock for growing Gen0 (rare operation).
    ///
    /// Only held during Vec resizing when a TLAB chunk allocation needs to grow
    /// the backing storage. This doesn't affect fast-path allocation which is
    /// lock-free within a TLAB.
    growth_lock: Mutex<()>,

    /// Allocations since last GC (for triggering heuristic).
    allocs_since_gc: AtomicUsize,

    /// Number of live Gen1 objects after the last collection (minor or major).
    ///
    /// Used to compute the adaptive Gen1 collection threshold.
    gen1_live_after_last_collection: AtomicUsize,

    /// Number of live Gen2 objects after the last collection.
    ///
    /// Used to compute the adaptive Gen2 collection threshold.
    gen2_live_after_last_collection: AtomicUsize,

    /// Gen1 size threshold that triggers a Minor GC.
    ///
    /// Starts at 10,000 objects; updated after each collection to 2× the live
    /// Gen1 count (floor 10,000).
    gen1_collection_threshold: AtomicUsize,

    /// Gen2 size threshold that triggers a Major GC.
    ///
    /// Starts at 50,000 objects; updated after each collection to 2× the live
    /// Gen2 count (floor 50,000).
    gen2_collection_threshold: AtomicUsize,

    /// Debug instrumentation state and config.
    debug_state: HeapDebuggerState,
}

// SAFETY: BexHeap is Send + Sync because:
// - objects: UnsafeCell is accessed safely via TLAB exclusivity and growth_lock
// - compile_time_boundary: immutable after construction
// - next_chunk: AtomicUsize is thread-safe
// - handles: RwLock<HashMap> is thread-safe
// - tlab_size: immutable after construction
// - growth_lock: Mutex is thread-safe
unsafe impl Send for BexHeap {}
unsafe impl Sync for BexHeap {}

// Forward `bex_vm_types::WriteBarrier` to the inherent `BexHeap::write_barrier`
// so heap-mutation sites in upstream crates (e.g. `Future::set_ready` in
// `bex_vm_types`, which can't name `BexHeap` directly because of dep
// direction) can fire the barrier through a small trait.
impl WriteBarrier for BexHeap {
    #[inline]
    fn write_barrier(&self, container: HeapPtr, value: Value) {
        BexHeap::write_barrier(self, container, value);
    }
}

// Implement WeakHeapRef trait from bex_external_types
impl WeakHeapRef for BexHeap {
    fn release_handle(&self, handle_key: usize) {
        let mut handles = self.handles.write().expect("handles lock poisoned");
        handles.remove(&handle_key);
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
        Self::with_tlab_size_and_debug(
            compile_time_objects,
            DEFAULT_TLAB_SIZE,
            HeapDebuggerConfig::from_env(),
        )
    }

    /// Create a new heap with custom TLAB size.
    pub fn with_tlab_size(compile_time_objects: Vec<Object>, tlab_size: usize) -> Arc<Self> {
        Self::with_tlab_size_and_debug(
            compile_time_objects,
            tlab_size,
            HeapDebuggerConfig::from_env(),
        )
    }

    /// Create a new heap with explicit debug configuration.
    pub fn with_tlab_size_and_debug(
        compile_time_objects: Vec<Object>,
        tlab_size: usize,
        debug: HeapDebuggerConfig,
    ) -> Arc<Self> {
        Self::build_unsealed(compile_time_objects, tlab_size, debug).seal()
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
        tlab_size: usize,
        debug: HeapDebuggerConfig,
    ) -> Self {
        // Resolve bytecode constants for all Function objects before sealing.
        // This converts ConstValue (with ObjectIndex) to Value (with HeapPtr).
        Self::resolve_function_constants(&mut compile_time_objects);

        // BEP-042: precompute whether any class opts into a `cleanup` finalizer,
        // so the per-collection finalizer scan can be skipped when none do.
        let has_finalizable_classes = compile_time_objects
            .iter()
            .any(|o| matches!(o, Object::Class(c) if c.has_cleanup));

        Self {
            compile_time: compile_time_objects,
            gen0: UnsafeCell::new(ChunkedVec::new()),
            gen1: UnsafeCell::new(ChunkedVec::new()),
            gen2: UnsafeCell::new(ChunkedVec::new()),
            inactive: UnsafeCell::new(ChunkedVec::new()),
            gen0_next_chunk: AtomicUsize::new(0),
            gen2_cards: UnsafeCell::new(CardTable::new()),
            handles: RwLock::new(HashMap::new()),
            next_handle_key: AtomicUsize::new(0),
            next_runtime_mint: AtomicU64::new(0),
            pending_finalizers: Mutex::new(Vec::new()),
            pending_unhandled_spawn_errors: Mutex::new(Vec::new()),
            has_finalizable_classes,
            tlab_size,
            growth_lock: Mutex::new(()),
            allocs_since_gc: AtomicUsize::new(0),
            gen1_live_after_last_collection: AtomicUsize::new(0),
            gen2_live_after_last_collection: AtomicUsize::new(0),
            gen1_collection_threshold: AtomicUsize::new(GEN1_FLOOR),
            gen2_collection_threshold: AtomicUsize::new(GEN2_FLOOR),
            debug_state: HeapDebuggerState::new(debug),
        }
    }

    /// [`Self::build_unsealed`] with the default TLAB size and env-derived debug
    /// config (the same defaults [`Self::new`] uses).
    pub fn build_unsealed_default(compile_time_objects: Vec<Object>) -> Self {
        Self::build_unsealed(
            compile_time_objects,
            DEFAULT_TLAB_SIZE,
            HeapDebuggerConfig::from_env(),
        )
    }

    /// Freeze the heap behind the shared `Arc`. After this the compile-time
    /// objects are immutable. See [`Self::build_unsealed`].
    pub fn seal(self) -> Arc<Self> {
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
        self.compile_time[index] = object;
    }

    /// Resolve every pooled `ObjectIndex` operand into a `HeapPtr` before the
    /// heap is sealed: bytecode constants of every `Function`, and each
    /// interface's default-method bodies.
    ///
    /// Converts ConstValue (compile-time, with ObjectIndex) to Value (runtime, with HeapPtr).
    /// Must be called before wrapping in Arc since we need mutable access.
    fn resolve_function_constants(objects: &mut [Object]) {
        // First, compute pointers for all objects (they're at stable positions in the slice)
        let base_ptr = objects.as_ptr();

        let resolve_idx = |idx: bex_vm_types::ObjectIndex| -> bex_vm_types::HeapPtr {
            let ptr = unsafe { base_ptr.add(idx.into_raw()) as *mut Object };
            #[cfg(feature = "heap_debug")]
            unsafe {
                bex_vm_types::HeapPtr::from_ptr(ptr, 0)
            }
            #[cfg(not(feature = "heap_debug"))]
            unsafe {
                bex_vm_types::HeapPtr::from_ptr(ptr)
            }
        };

        for obj in objects.iter_mut() {
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
                        // ClassWithTypeArgs is NOT pre-resolved: `IsType` reads it
                        // directly from `constants` at execution time.
                        bex_vm_types::ConstValue::ClassWithTypeArgs { .. } => {
                            bex_vm_types::Value::NULL
                        }
                        other => other.to_value(resolve_idx),
                    })
                    .collect();
            } else if let Object::Interface(interface) = obj {
                // The one place a static interface's default body becomes a
                // pointer: from here on, a witness reads `default_fn` and never
                // resolves a name.
                for method in &mut interface.methods {
                    if let Some(default) = method.default {
                        method.default_fn = resolve_idx(default);
                    }
                }
            }
        }
    }

    /// Get the number of compile-time objects.
    pub fn compile_time_len(&self) -> usize {
        self.compile_time.len()
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
        let raw_ptr = ptr.as_ptr() as *const Object;
        let start = self.compile_time.as_ptr();
        let end = unsafe { start.add(self.compile_time.len()) };
        raw_ptr >= start && raw_ptr < end
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
        let raw_ptr = &self.compile_time[index] as *const Object as *mut Object;
        // SAFETY: The pointer is valid and points to a compile-time object
        // that will never be moved or deallocated.
        unsafe { self.make_heap_ptr(raw_ptr) }
    }

    /// Get a shared reference to Gen0 (the nursery / active allocation space).
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to Gen0.
    #[inline]
    pub unsafe fn gen0_ref(&self) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.gen0.get() }
    }

    /// Get a mutable reference to Gen0.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (e.g., at a GC safepoint).
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    pub(crate) unsafe fn gen0_mut(&self) -> &mut ChunkedVec<Object> {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *self.gen0.get() }
    }

    /// Get a shared reference to Gen1.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to Gen1.
    #[inline]
    pub(crate) unsafe fn gen1_ref(&self) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.gen1.get() }
    }

    /// Get a mutable reference to Gen1.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (e.g., at a GC safepoint).
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    pub(crate) unsafe fn gen1_mut(&self) -> &mut ChunkedVec<Object> {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *self.gen1.get() }
    }

    /// Get a shared reference to Gen2.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to Gen2.
    #[inline]
    pub(crate) unsafe fn gen2_ref(&self) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.gen2.get() }
    }

    /// Get a mutable reference to Gen2.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (e.g., at a GC safepoint).
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    #[allow(dead_code)] // Will be used in Phase 4 (generational collection algorithms)
    pub(crate) unsafe fn gen2_mut(&self) -> &mut ChunkedVec<Object> {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *self.gen2.get() }
    }

    /// Get a shared reference to the inactive (copy-destination) space.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to inactive.
    #[inline]
    pub(crate) unsafe fn inactive_ref(&self) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.inactive.get() }
    }

    /// Get a mutable reference to the inactive space.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (e.g., at a GC safepoint).
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    pub(crate) unsafe fn inactive_mut(&self) -> &mut ChunkedVec<Object> {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *self.inactive.get() }
    }

    /// Write barrier for field/element/cell writes.
    ///
    /// Called *before* the actual field write at each mutation site. If `container_ptr`
    /// is in an older generation than the object being written (`written_value`), the
    /// card containing `container_ptr` is marked dirty so partial GC can discover
    /// the cross-generation reference.
    ///
    /// This is a no-op when either side is not a heap object, or when the container
    /// is in Gen0 (no card table for Gen0).
    #[inline]
    pub fn write_barrier(&self, container_ptr: HeapPtr, written_value: Value) {
        if let Some(ref_ptr) = written_value.as_object_ptr() {
            let container_gen = self.generation_of(container_ptr);
            let ref_gen = self.generation_of(ref_ptr);
            if container_gen > ref_gen {
                self.mark_card_for_ptr(container_ptr);
            }
        }
    }

    /// Conservative write barrier for mutable accessor paths (builtin dispatch).
    ///
    /// Unconditionally marks the card dirty if `container_ptr` is in an older
    /// generation. Used by `as_array_mut` / `as_map_mut` where the actual written
    /// value is not yet known (it's supplied by the callee trait method).
    ///
    /// This over-marks (any mutable access to an older-gen object dirties the card),
    /// but it is always safe and the cost is negligible since most objects are Gen0.
    #[inline]
    pub fn conservative_write_barrier(&self, container_ptr: HeapPtr) {
        let container_gen = self.generation_of(container_ptr);
        if container_gen > Generation::Gen0 {
            self.mark_card_for_ptr(container_ptr);
        }
    }

    /// Determine which generation an object pointer belongs to.
    ///
    /// Only compile-time, Gen2, and Gen1 are checked directly; anything else is
    /// classified as `Generation::Gen0` by fallback. Every valid `HeapPtr` lives
    /// in exactly one of those four spaces, so the fallback is correct.
    ///
    /// # Concurrency
    ///
    /// This is called from the write-barrier hot path during mutator execution,
    /// where it must be safe under concurrent Gen0 growth (another VM's TLAB
    /// refill). Gen2 and Gen1 only grow at GC safepoints, so their chunk layout
    /// is stable during mutator execution and safe to scan without
    /// synchronization. Gen0 is the only space that grows concurrently, and we
    /// deliberately never inspect it — a Gen0 pointer falls through to the
    /// `Generation::Gen0` fallback.
    #[inline]
    pub fn generation_of(&self, ptr: HeapPtr) -> Generation {
        if self.is_compile_time_ptr(ptr) {
            return Generation::CompileTime;
        }
        let raw_ptr = ptr.as_ptr() as *const Object;
        // SAFETY: Gen2 and Gen1 chunk layouts only change at GC safepoints, so
        // reading them here — even from a concurrent mutator write-barrier —
        // cannot race with a chunk-Vec reallocation.
        unsafe {
            if Self::ptr_in_chunked_vec(&*self.gen2.get(), raw_ptr) {
                return Generation::Gen2;
            }
            if Self::ptr_in_chunked_vec(&*self.gen1.get(), raw_ptr) {
                return Generation::Gen1;
            }
        }
        Generation::Gen0
    }

    /// Check whether a raw pointer falls within any chunk of a `ChunkedVec`.
    ///
    /// # Safety
    ///
    /// The caller must ensure `vec`'s chunk layout is not growing concurrently.
    /// `ChunkedVec` never moves existing chunks, but its internal chunk
    /// `Vec<Box<[UnsafeCell<T>]>>` can reallocate its buffer on growth, and
    /// `num_chunks`/`chunk_start_ptr` are non-atomic reads of that buffer. Only
    /// call this for spaces that grow exclusively at GC safepoints (Gen1/Gen2),
    /// or while holding exclusive access to the space being scanned.
    #[inline]
    unsafe fn ptr_in_chunked_vec(vec: &ChunkedVec<Object>, raw_ptr: *const Object) -> bool {
        // `num_chunks` and `chunk_start_ptr` now serialize on the
        // ChunkedVec's internal RwLock, so the brief window is safe even
        // under a concurrent grower. `chunk_start_ptr` is still `unsafe`
        // for the bounds precondition.
        let num_chunks = vec.num_chunks();
        for chunk_idx in 0..num_chunks {
            // SAFETY: `chunk_idx < num_chunks` by loop bound.
            let chunk_start = unsafe { vec.chunk_start_ptr(chunk_idx) };
            let chunk_end = unsafe { chunk_start.add(ChunkedVec::<Object>::CHUNK_SIZE) };
            if raw_ptr >= chunk_start && raw_ptr < chunk_end {
                return true;
            }
        }
        false
    }

    /// Bug H, check 3 helper (heap_debug only): is `ptr` inside the
    /// inactive (former active) space?
    ///
    /// Used by the engine's post-`forward_roots` integrity sweep to detect
    /// stale references the GC failed to forward. The inactive space's
    /// chunks still exist (their slots have been overwritten with
    /// `Sentinel::FromSpacePoison` in heap_debug builds), so checking
    /// `ptr_in_chunked_vec` against `inactive` is safe even after
    /// `finalize_inactive_space` has run.
    #[cfg(feature = "heap_debug")]
    pub fn debug_ptr_in_inactive(&self, ptr: HeapPtr) -> bool {
        let raw = ptr.as_ptr() as *const Object;
        // SAFETY: GC has parked all permits before the engine calls this.
        unsafe { Self::ptr_in_chunked_vec(&*self.inactive.get(), raw) }
    }

    /// Mark the card dirty for the card containing `container_ptr` in Gen2.
    ///
    /// Called from write barriers when a Gen2 object receives a reference to a
    /// younger-generation object. Gen1 doesn't need card tracking because its
    /// cross-generation references are discovered during Minor GC via the
    /// promotion-time card-marking sweep in `collect_garbage_minor`.
    ///
    /// Safe to call concurrently from multiple VMs. Writes use a relaxed
    /// atomic store; no `&mut` borrow on the card table is taken. Capacity
    /// growth happens separately, at GC safepoints, since Gen2 can only grow
    /// during GC.
    #[inline]
    pub fn mark_card_for_ptr(&self, container_ptr: HeapPtr) {
        let raw_ptr = container_ptr.as_ptr() as *const Object;
        // SAFETY: Only reads `gen2` chunk layout and stores into the atomic
        // card table. Gen2 grows only at GC safepoints, so no concurrent write
        // can invalidate either access.
        unsafe {
            if let Some((chunk_idx, offset)) =
                Self::locate_in_chunked_vec(&*self.gen2.get(), raw_ptr)
            {
                let cards = &*self.gen2_cards.get();
                cards.mark_dirty_by_offset(chunk_idx, offset);
            }
        }
    }

    /// Locate a raw pointer within a `ChunkedVec`, returning `(chunk_idx, offset_in_chunk)`.
    ///
    /// Returns `None` if the pointer is not within any chunk of `vec`.
    ///
    /// # Safety
    ///
    /// Must only be called at safepoints or with appropriate external synchronization.
    #[inline]
    pub(crate) unsafe fn locate_in_chunked_vec(
        vec: &ChunkedVec<Object>,
        raw_ptr: *const Object,
    ) -> Option<(usize, usize)> {
        // `num_chunks` and `chunk_start_ptr` now serialize on the
        // ChunkedVec's internal RwLock; concurrent growers are excluded.
        let num_chunks = vec.num_chunks();
        for chunk_idx in 0..num_chunks {
            // SAFETY: `chunk_idx < num_chunks` by loop bound.
            let chunk_start = unsafe { vec.chunk_start_ptr(chunk_idx) };
            let chunk_end = unsafe { chunk_start.add(ChunkedVec::<Object>::CHUNK_SIZE) };
            if raw_ptr >= chunk_start && raw_ptr < chunk_end {
                // SAFETY: Both pointers are within the same allocated chunk.
                let offset = unsafe { raw_ptr.offset_from(chunk_start) as usize };
                return Some((chunk_idx, offset));
            }
        }
        None
    }

    /// Get the TLAB chunk size.
    pub fn tlab_size(&self) -> usize {
        self.tlab_size
    }

    /// Write an object at the given runtime index in Gen0 (the active nursery).
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. **Write exclusivity**: Only write to indices within your TLAB's
    ///    exclusive region (`tlab.alloc_ptr..tlab.alloc_limit`)
    /// 2. **Index validity**: The index must be < Gen0's current length
    ///
    /// # Why This API?
    ///
    /// ChunkedVec provides stable pointers - growing the storage never moves
    /// existing elements. This eliminates the data race that occurred with
    /// Vec, where one thread's pointer could be invalidated by another
    /// thread's resize operation.
    ///
    /// Production VMs (JVM, CLR, V8, Go) all use direct memory access for
    /// field writes. The "lock-free" property comes from:
    ///
    /// - **TLABs**: Each VM has exclusive write access to its allocation region
    /// - **No globals**: BAML has no global mutable state, preventing races
    /// - **Safepoint GC**: Collection only runs when no VMs are executing
    /// - **ChunkedVec**: Growing never moves existing elements
    #[inline]
    pub unsafe fn write_runtime_object(&self, runtime_idx: usize, obj: Object) {
        // SAFETY: Caller ensures exclusive access to this index.
        // ChunkedVec's set() is internally safe for concurrent access to different indices.
        // All TLAB allocations target Gen0.
        unsafe {
            (*self.gen0.get()).set(runtime_idx, obj);
        }
    }

    /// Get the current number of objects in the heap (compile-time + all
    /// runtime generations).
    #[cfg(any(test, feature = "heap_debug"))]
    pub(crate) fn len(&self) -> usize {
        // SAFETY: Reading len is safe on each space (AtomicUsize loads).
        let runtime_len = unsafe {
            (*self.gen0.get()).len() + (*self.gen1.get()).len() + (*self.gen2.get()).len()
        };
        self.compile_time.len() + runtime_len
    }

    /// Allocate a new TLAB chunk from Gen0 (the nursery).
    ///
    /// This method is thread-safe. Multiple VMs can request chunks
    /// concurrently - each gets a unique, non-overlapping region.
    ///
    /// # Thread Safety
    ///
    /// Uses `fetch_add` with `SeqCst` ordering to ensure each caller
    /// gets a unique chunk range, even under concurrent access. The
    /// growth_lock protects the ChunkedVec resize operation.
    ///
    /// # Why This Is Now Safe
    ///
    /// With ChunkedVec, growing the storage adds new chunks without moving
    /// existing data. So even if one VM is writing to an existing element
    /// while another VM triggers growth here, there's no data race - the
    /// existing element's memory location doesn't change.
    ///
    /// Returns a `TlabChunk` describing the exclusive region for the VM.
    /// The VM can then allocate objects within this region without locks.
    pub fn alloc_tlab_chunk(&self) -> TlabChunk {
        self.debug_verify_tlab_canaries();

        let use_canary = self.debug_config().enabled;
        let canary_slots = if use_canary { 1 } else { 0 };

        // Atomically reserve a chunk range within Gen0
        let step = self.tlab_size + canary_slots;
        let runtime_start = self.gen0_next_chunk.fetch_add(step, Ordering::SeqCst);
        let runtime_end = runtime_start + self.tlab_size;
        let reserve_end = runtime_end + canary_slots;

        // The chunk-allocation policy mutex still serializes the
        // fetch_add → resize → canary-write critical section as a unit.
        // (`ChunkedVec::resize_with` now self-synchronizes against
        // concurrent `set` callers, so this is no longer load-bearing for
        // memory safety — but keeping it preserves the existing "one
        // grower at a time" policy and keeps the canary write paired with
        // the resize that produced its slot.)
        let _guard = self.growth_lock.lock().unwrap();

        let ct_len = self.compile_time.len();
        // SAFETY: `&*self.gen0.get()` produces a `&ChunkedVec<Object>`.
        // The ChunkedVec's own RwLock now gates outer-Vec mutation, so this
        // shared reference is sound to hold concurrently with other readers
        // and growers; per-element exclusivity is gated separately by
        // `UnsafeCell` + caller-side TLAB regions.
        let gen0 = unsafe { &*self.gen0.get() };
        if gen0.len() < reserve_end {
            gen0.resize_with(reserve_end, || {
                // Placeholder object - will be overwritten by TLAB alloc
                self.placeholder_object()
            });
        }
        if use_canary {
            let chunk_start = ct_len + runtime_start;
            let chunk_end = ct_len + runtime_end;
            // SAFETY: `runtime_end` is within the freshly-grown range; the
            // canary slot is exclusive to this chunk reservation.
            unsafe {
                gen0.set(runtime_end, self.tlab_canary_object(chunk_start, chunk_end));
            }
        }

        // Return global indices (compile_time_len + runtime indices)
        if use_canary {
            let canary_idx = ct_len + runtime_end;
            self.record_tlab_canary(canary_idx);
        }
        TlabChunk {
            start: ct_len + runtime_start,
            end: ct_len + runtime_end,
        }
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

    /// Get statistics about heap usage.
    pub fn stats(&self) -> HeapStats {
        // SAFETY: Reading len is safe on each space (AtomicUsize loads).
        let (gen0_len, gen1_len, gen2_len) = unsafe {
            (
                (*self.gen0.get()).len(),
                (*self.gen1.get()).len(),
                (*self.gen2.get()).len(),
            )
        };
        let ct_len = self.compile_time.len();
        let runtime = gen0_len + gen1_len + gen2_len;
        let total = ct_len + runtime;

        let tlab_chunks = self
            .gen0_next_chunk
            .load(Ordering::Relaxed)
            .div_ceil(self.tlab_size);

        HeapStats {
            total_objects: total,
            compile_time_objects: ct_len,
            runtime_objects: runtime,
            active_handles: self.handles.read().expect("handles lock poisoned").len(),
            tlab_chunks,
        }
    }

    /// Check if GC should run based on allocation pressure (legacy, alloc-count only).
    ///
    /// Use [`BexHeap::should_collect`] for the full adaptive triggering policy.
    pub fn should_gc(&self) -> bool {
        const GC_THRESHOLD: usize = 10_000; // Tune based on profiling
        self.allocs_since_gc.load(Ordering::Relaxed) >= GC_THRESHOLD
    }

    /// Reset the allocation counter after GC.
    pub fn reset_gc_counter(&self) {
        self.allocs_since_gc.store(0, Ordering::Relaxed);
    }

    /// Increment allocation counter (called by TLAB on alloc).
    pub(crate) fn record_alloc(&self) {
        self.allocs_since_gc.fetch_add(1, Ordering::Relaxed);
    }

    /// Load the current allocation count since last GC.
    pub(crate) fn allocs_since_gc(&self) -> usize {
        self.allocs_since_gc.load(Ordering::Relaxed)
    }

    /// Load the Gen1 collection threshold.
    pub(crate) fn gen1_collection_threshold(&self) -> usize {
        self.gen1_collection_threshold.load(Ordering::Relaxed)
    }

    /// Load the Gen2 collection threshold.
    pub(crate) fn gen2_collection_threshold(&self) -> usize {
        self.gen2_collection_threshold.load(Ordering::Relaxed)
    }

    /// Update thresholds after a Minor (Gen0+Gen1) collection.
    ///
    /// Sets the Gen1 threshold to `max(2 * live_gen1, GEN1_FLOOR)` and updates
    /// Gen2 tracking if objects were promoted.
    pub(crate) fn update_thresholds_after_minor(&self, live_gen1: usize, live_gen2: usize) {
        self.gen1_live_after_last_collection
            .store(live_gen1, Ordering::Relaxed);
        self.gen1_collection_threshold
            .store((live_gen1 * 2).max(GEN1_FLOOR), Ordering::Relaxed);

        // Also update Gen2 tracking (objects may have been promoted to Gen2).
        self.gen2_live_after_last_collection
            .store(live_gen2, Ordering::Relaxed);
        self.gen2_collection_threshold
            .store((live_gen2 * 2).max(GEN2_FLOOR), Ordering::Relaxed);
    }

    /// Update thresholds after a Major (full) collection.
    ///
    /// All survivors are in Gen2. Resets Gen1 tracking to zero (Gen1 is empty
    /// after a full GC) and sets Gen2 threshold to `max(2 * live_gen2, GEN2_FLOOR)`.
    pub(crate) fn update_thresholds_after_major(&self, live_gen2: usize) {
        // Gen1 is empty after a full GC.
        self.gen1_live_after_last_collection
            .store(0, Ordering::Relaxed);
        self.gen1_collection_threshold
            .store(GEN1_FLOOR, Ordering::Relaxed);

        self.gen2_live_after_last_collection
            .store(live_gen2, Ordering::Relaxed);
        self.gen2_collection_threshold
            .store((live_gen2 * 2).max(GEN2_FLOOR), Ordering::Relaxed);
    }

    /// Reset the Gen0 TLAB allocation pointer (called by GC after collection).
    pub(crate) fn reset_next_chunk(&self, new_value: usize) {
        self.gen0_next_chunk.store(new_value, Ordering::Release);
    }

    #[cfg(feature = "heap_debug")]
    pub(crate) fn next_chunk_value(&self) -> usize {
        self.gen0_next_chunk.load(Ordering::Acquire)
    }

    pub(crate) fn debug_state(&self) -> &HeapDebuggerState {
        &self.debug_state
    }

    /// Update handle entries after GC.
    ///
    /// Updates handles to point to new object locations. Invalidates handles
    /// Rewrite every handle's `HeapPtr` through the forwarding map after GC.
    ///
    /// Handles are GC roots by contract — the engine always feeds
    /// [`collect_handle_roots`](Self::collect_handle_roots) into the GC root
    /// set — so every handle target *must* appear in `forwarding` (either
    /// relocated or identity-mapped). A missing entry means the caller broke
    /// the contract; we panic rather than silently invalidate and expose the
    /// caller to dangling pointers.
    pub fn update_handles(&self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        // Validate every handle under a read lock first. A panic while the
        // write guard is held would poison the `RwLock` and then cascade into
        // a double-panic when the panicking `Handle`'s destructor tries to
        // release its slab key through `self.handles.write()` during unwind.
        {
            let handles = self.handles.read().expect("handles lock poisoned");
            for (&key, ptr) in handles.iter() {
                assert!(
                    forwarding.contains_key(ptr),
                    "handle {key} with ptr {ptr:?} was not in the GC forwarding map — \
                     handles must be passed as GC roots via `collect_handle_roots`"
                );
            }
        }
        // Safe to mutate now; every entry is guaranteed to be in `forwarding`.
        let mut handles = self.handles.write().expect("handles lock poisoned");
        for ptr in handles.values_mut() {
            *ptr = forwarding[ptr];
        }
    }

    /// BEP-042: record an instance (by its post-copy `HeapPtr`) and its resolved
    /// `cleanup` function name, to be finalized after the current collection.
    /// Called by the GC when it discovers and keeps alive a dead, not-yet-cleaned
    /// instance of a `has_cleanup` class.
    pub(crate) fn push_pending_finalizer(&self, ptr: HeapPtr, cleanup_fn: String) {
        self.pending_finalizers
            .lock()
            .expect("pending_finalizers lock poisoned")
            .push((ptr, cleanup_fn));
    }

    /// BEP-042: take and clear the queue of instances awaiting `cleanup`. The
    /// engine drains this right after a collection (still at the GC safepoint,
    /// before resuming normal execution) and invokes each instance's `cleanup`.
    pub fn take_pending_finalizers(&self) -> Vec<(HeapPtr, String)> {
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
        // Get a unique key for this handle
        let handle_key = self.next_handle_key.fetch_add(1, Ordering::Relaxed);

        // Insert into the handle table
        {
            let mut handles = self.handles.write().expect("handles lock poisoned");
            handles.insert(handle_key, ptr);
        }

        // Handle no longer stores idx - always resolves through table
        Handle::new(handle_key, Arc::clone(self) as Arc<dyn WeakHeapRef>)
    }

    /// Mint a fresh `MintId::Runtime` identity (BEP-066 I-1): the next value
    /// of the engine-wide monotonic counter. Every VM sharing this heap —
    /// including spawned children — draws from the same counter, so two
    /// constructor evaluations can never mint the same identity. `Relaxed`
    /// suffices: uniqueness needs only the atomicity of `fetch_add`, no
    /// ordering with other memory.
    pub fn mint_runtime_id(&self) -> bex_vm_types::types::MintId {
        bex_vm_types::types::MintId::Runtime(self.next_runtime_mint.fetch_add(1, Ordering::Relaxed))
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

    /// Count the number of dirty cards currently tracked for Gen2.
    ///
    /// Intended for tests and diagnostics — e.g., asserting that a minor GC
    /// cleared the card table as part of collection.
    pub fn gen2_dirty_card_count(&self) -> usize {
        // SAFETY: Reading card-table state through the UnsafeCell. Callers are
        // expected to observe this between GC cycles, where no concurrent
        // marker can race.
        unsafe { (*self.gen2_cards.get()).dirty_card_indices().count() }
    }
}

impl std::fmt::Debug for BexHeap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: Reading lens is safe (AtomicUsize loads)
        let (gen0_len, gen1_len, gen2_len) = unsafe {
            (
                (*self.gen0.get()).len(),
                (*self.gen1.get()).len(),
                (*self.gen2.get()).len(),
            )
        };
        f.debug_struct("BexHeap")
            .field("compile_time_len", &self.compile_time.len())
            .field("gen0_len", &gen0_len)
            .field("gen1_len", &gen1_len)
            .field("gen2_len", &gen2_len)
            .field("tlab_size", &self.tlab_size)
            .finish()
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
    fn test_alloc_tlab_chunk() {
        let heap = BexHeap::with_tlab_size(vec![], 100);

        // With no compile-time objects, global indices start at 0
        let chunk1 = heap.alloc_tlab_chunk();
        assert_eq!(chunk1.start, 0); // compile_time_len(0) + runtime_start(0)
        assert_eq!(chunk1.end, 100); // compile_time_len(0) + runtime_end(100)

        let chunk2 = heap.alloc_tlab_chunk();
        assert_eq!(chunk2.start, 100); // compile_time_len(0) + runtime_start(100)
        assert_eq!(chunk2.end, 200);

        // Heap should have grown to accommodate chunks
        assert!(heap.len() >= 200);
    }

    #[test]
    fn test_alloc_tlab_chunk_with_compile_time() {
        let compile_time: Vec<Object> =
            vec![Object::String("ct1".into()), Object::String("ct2".into())];
        let heap = BexHeap::with_tlab_size(compile_time, 100);

        // With 2 compile-time objects, global indices start at 2
        let chunk1 = heap.alloc_tlab_chunk();
        assert_eq!(chunk1.start, 2); // compile_time_len(2) + runtime_start(0)
        assert_eq!(chunk1.end, 102); // compile_time_len(2) + runtime_end(100)

        let chunk2 = heap.alloc_tlab_chunk();
        assert_eq!(chunk2.start, 102); // compile_time_len(2) + runtime_start(100)
        assert_eq!(chunk2.end, 202);
    }

    #[test]
    fn test_heap_stats() {
        let compile_time: Vec<Object> = vec![Object::String("builtin".into())];
        let heap = BexHeap::with_tlab_size(compile_time, 50);

        let stats = heap.stats();
        assert_eq!(stats.compile_time_objects, 1);
        assert_eq!(stats.total_objects, 1);
        assert_eq!(stats.runtime_objects, 0);

        // Allocate a TLAB chunk
        let _chunk = heap.alloc_tlab_chunk();

        let stats = heap.stats();
        assert_eq!(stats.tlab_chunks, 1);
        assert!(stats.total_objects >= 51); // Expanded for TLAB
    }

    #[test]
    fn runtime_mints_are_heap_wide_and_disjoint_from_static_mints() {
        use bex_vm_types::types::MintId;

        let heap = BexHeap::new(vec![]);
        let shared = Arc::clone(&heap);

        let first = heap.mint_runtime_id();
        let second = shared.mint_runtime_id();
        assert_eq!(first, MintId::Runtime(0));
        assert_eq!(second, MintId::Runtime(1));
        assert_ne!(first, second);
        assert_ne!(first, MintId::Static(0));
    }

    // Note: Handle tests removed as they require HeapPtr creation which depends
    // on runtime allocation. Will be updated when full integration is complete.
}
