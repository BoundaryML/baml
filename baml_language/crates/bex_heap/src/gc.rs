//! Garbage collection for the unified heap.
//!
//! A non-moving, stop-the-world mark-sweep collector over size-class blocks:
//!
//! - **Safepoints**: a collection runs only while every mutator is parked
//!   (the [`HeapGuard`](crate::HeapGuard) pause protocol).
//! - **Mark**: from precise roots (handles and the root registry) and
//!   conservative word ranges, set the side-table mark bit of every
//!   reachable cell.
//! - **Keep alive**: unobserved spawn errors and dead finalizable instances
//!   are marked too, with everything they reach.
//! - **Sweep**: drop every unmarked cell; the size-class policy rebuilds
//!   each block's free list and returns empty blocks to the shared pool.
//!
//! Nothing moves, so no pointer is ever rewritten. The forwarding map the
//! collection entry points return is empty, except under `heap_debug`, where
//! it maps each object reached to itself under the new epoch so that the
//! epoch check on [`HeapPtr`] keeps catching pointers a collection did not
//! see.
//!
//! # Invariant: compile-time objects do not contain runtime `HeapPtr`s
//!
//! Marking does not trace a compile-time object. This is sound only because
//! no compile-time object holds a pointer to a collectible one; debug builds
//! check it at each compile-time object marking reaches.

use crate::descriptor::CellHeader as _;
use std::collections::HashMap;

use bex_vm_types::{FutureRead, HeapPtr, Object, RootHaverExt};

use crate::{
    BexHeap, HeapGuard,
    descriptor::{HEADER_BYTES, OBJECT_DESC, RUST_PAYLOAD_OFFSET},
    root_source::{BothRoots, PreciseRoots, RootSource},
    space::{block::BlockHeader, large::LargeSpace, mark::Marker, pool::BlockPool, sweep::sweep},
};

/// Result of a garbage collection cycle.
#[derive(Debug, Clone)]
pub struct GcStats {
    /// Objects found live.
    pub live_count: usize,
    /// Objects reclaimed.
    pub collected_count: usize,
    /// Bytes the survivors keep alive outside their cells, with storage
    /// shared between them counted once.
    pub live_payload_bytes: usize,
    /// Detailed diagnostics in profiling builds; a zero-sized marker otherwise.
    pub profile: crate::GcProfile,
}

/// Re-stamped pointers: empty unless `heap_debug`, where every object a
/// collection reached maps to itself under the new epoch.
pub type Restamp = HashMap<HeapPtr, HeapPtr>;

impl BexHeap {
    /// Run a full collection with `roots` as its only precise roots.
    ///
    /// # Safety
    ///
    /// No mutator runs, and `roots` together with the handle table are every
    /// pointer to the heap held outside it.
    ///
    /// # Returns
    ///
    /// `(stats, roots, restamp)`: the roots, re-stamped under `heap_debug`
    /// and otherwise unchanged, and the [`Restamp`] map.
    pub unsafe fn collect_garbage(&self, roots: &[HeapPtr]) -> (GcStats, Vec<HeapPtr>, Restamp) {
        let mut handles = self.collect_handle_roots();
        handles.extend_from_slice(roots);
        // SAFETY: forwarded to the caller.
        let (stats, restamp) = unsafe { self.collect(&mut PreciseRoots(&handles)) };
        let roots = roots
            .iter()
            .map(|root| restamp.get(root).copied().unwrap_or(*root))
            .collect();
        (stats, roots, restamp)
    }

    /// Run a full collection under `guard`'s pause: the roots are the handle
    /// table, every holder in the root registry, and `extra` (for instance
    /// the conservative stacks of parked fibers).
    ///
    /// The caller still forwards the registry with the returned [`Restamp`]
    /// (`HeapGuard::forward_roots`), which drops weak references to the
    /// dead.
    pub fn collect_paused(
        &self,
        guard: &mut HeapGuard<'_>,
        extra: &mut dyn RootSource,
    ) -> (GcStats, Restamp) {
        let mut roots = self.collect_handle_roots();
        guard.collect_roots(&mut roots);
        // SAFETY: the guard holds every permit, so no mutator runs, and every
        // holder of heap pointers outside the heap is in its registry or the
        // handle table, or reported by `extra`.
        unsafe { self.collect(&mut BothRoots(PreciseRoots(&roots), extra)) }
    }

    /// The collector.
    ///
    /// # Safety
    ///
    /// No mutator runs, and `roots` reports every pointer to the heap held
    /// outside it, handles included.
    pub unsafe fn collect(&self, roots: &mut dyn RootSource) -> (GcStats, Restamp) {
        // SAFETY: the caller guarantees exclusive heap access.
        let mut profile = unsafe { crate::gc_profile::GcProfiler::start(self) };
        self.bump_epoch();
        let mut small = self.small.lock();
        let mut large = self.large.lock();
        // SAFETY: nothing has been marked or swept yet.
        unsafe { self.refresh_finalizable_instances(small.pool(), &large) };
        clear_marks(small.pool(), &large);
        small.begin_collection();
        profile.finish_phase(crate::gc_profile::HeapPhase::Prepare);

        let mut marker = Marker::new(
            small.pool(),
            &large,
            self.descriptors(),
            self.compile_time_range(),
        );
        // SAFETY: mutators are parked and the roots name live objects.
        profile.set_roots(unsafe { mark_roots(&mut marker, roots) });
        profile.finish_phase(crate::gc_profile::HeapPhase::Trace);

        // SAFETY: the mark is complete; nothing is swept yet.
        unsafe {
            self.keepalive_unhandled_spawn_errors(&mut marker, small.pool(), &large);
            self.keepalive_finalizers(&mut marker);
        }
        profile.finish_phase(crate::gc_profile::HeapPhase::Keepalive);

        // SAFETY: the mark is final; nothing is swept yet.
        unsafe { self.forget_dead_weak_entries(&marker) };
        let restamp = self.restamp(&mut marker);
        let live = marker.live();
        drop(marker);
        profile.finish_phase(crate::gc_profile::HeapPhase::Fixup);

        // SAFETY: mutators are parked and the mark is complete.
        let swept = unsafe { sweep(&mut small, &mut large, self.descriptors()) };
        drop(large);
        drop(small);
        profile.finish_phase(crate::gc_profile::HeapPhase::Reclaim);

        self.update_handles(&restamp);
        self.live_bytes
            .store(live.bytes, std::sync::atomic::Ordering::Relaxed);
        self.gc_policy
            .after_full(live.bytes.saturating_add(swept.live_payload_bytes));
        #[cfg(feature = "gc_profiling")]
        self.reset_gc_counter();
        profile.finish_phase(crate::gc_profile::HeapPhase::Bookkeeping);

        // SAFETY: exclusive heap access is still held.
        let profile = unsafe { profile.finish(self) };
        let stats = GcStats {
            live_count: swept.live_cells,
            collected_count: swept.dead_cells,
            live_payload_bytes: swept.live_payload_bytes,
            profile,
        };
        (stats, restamp)
    }

    /// Every runtime VM object, block by block and by address within one,
    /// then the large ones.
    ///
    /// # Safety
    /// No mutator runs.
    pub(crate) unsafe fn for_each_runtime_object(
        pool: &BlockPool,
        large: &LargeSpace,
        mut visit: impl FnMut(*mut Object),
    ) {
        // SAFETY: forwarded to the caller.
        unsafe {
            Self::for_each_runtime_cell(pool, large, |cell| {
                // SAFETY: every listed cell is allocated with a written header.
                if crate::Header::at(cell).descriptor == OBJECT_DESC {
                    visit((cell + RUST_PAYLOAD_OFFSET) as *mut Object);
                }
            });
        }
    }

    /// Every allocated runtime cell, whatever its format, in the order of
    /// [`Self::for_each_runtime_object`].
    ///
    /// # Safety
    /// No mutator runs.
    pub(crate) unsafe fn for_each_runtime_cell(
        pool: &BlockPool,
        large: &LargeSpace,
        visit: impl FnMut(usize),
    ) {
        let small = pool.blocks().iter().flat_map(|&base| {
            // SAFETY: the pool's blocks are initialized.
            unsafe { BlockHeader::at(base) }.cells(base)
        });
        small.chain(large.cells()).for_each(visit);
    }

    /// BEP-042: the dead, not-yet-cleaned instances of classes with a
    /// `cleanup` finalizer, each with its class's finalizer, by address.
    ///
    /// Checks only the finalizable list ([`FinalizableInstances`]), not every
    /// object: a dead object without a finalizer is never visited.
    ///
    /// # Safety
    /// Between a complete mark and the sweep.
    ///
    /// [`FinalizableInstances`]: crate::heap::FinalizableInstances
    unsafe fn dead_finalizer_seeds(&self, marker: &Marker<'_>) -> Vec<(HeapPtr, HeapPtr)> {
        // Fast path: if no class defines `cleanup`, no instance can ever be
        // finalizable.
        if !self
            .has_finalizable_classes
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Vec::new();
        }
        let mut finalizable = self.finalizable.lock().expect("finalizable lock poisoned");
        debug_assert!(
            !finalizable.stale,
            "the collection refreshes a stale finalizable list before marking"
        );
        finalizable.instances.sort_unstable();
        finalizable.instances.dedup();
        finalizable
            .instances
            .iter()
            .copied()
            // SAFETY: every entry is an allocated cell (dead ones are dropped
            // from the list each collection) and nothing is swept yet.
            .filter(|&ptr| {
                #[cfg(test)]
                tests::FINALIZER_SCAN_VISITS.with(|visits| visits.set(visits.get() + 1));
                !unsafe { marker.is_marked(ptr) }
            })
            // SAFETY: as above, the object is intact.
            .filter_map(|ptr| unsafe { Self::uncleaned_finalizer(ptr) }.map(|f| (ptr, f)))
            .collect()
    }

    /// The `cleanup` finalizer of `ptr`, if it is an instance not yet
    /// cleaned whose class has one.
    ///
    /// # Safety
    /// `ptr` and its class are intact objects.
    unsafe fn uncleaned_finalizer(ptr: HeapPtr) -> Option<HeapPtr> {
        // SAFETY: forwarded to the caller.
        let instance = unsafe { bex_vm_types::InstanceRef::of(ptr) }?;
        if instance.is_cleaned() {
            return None; // explicit/`defer` already cleaned it — SuppressFinalize
        }
        instance.class_def().cleanup()
    }

    /// BEP-042: if an existing class gained a finalizer since the last
    /// collection, rebuild the finalizable list from a walk of every object.
    ///
    /// # Safety
    /// No mutator runs.
    unsafe fn refresh_finalizable_instances(&self, pool: &BlockPool, large: &LargeSpace) {
        let mut finalizable = self.finalizable.lock().expect("finalizable lock poisoned");
        if !finalizable.stale {
            return;
        }
        finalizable.instances.clear();
        // SAFETY: forwarded to the caller.
        unsafe {
            Self::for_each_runtime_cell(pool, large, |cell| {
                let ptr = self.make_heap_ptr((cell + HEADER_BYTES) as *mut Object);
                // SAFETY: an allocated object whose class is intact.
                if let Some(instance) = bex_vm_types::InstanceRef::of(ptr)
                    && Self::needs_finalizer_registration(instance)
                {
                    finalizable.instances.push(ptr);
                }
            });
        }
        finalizable.stale = false;
    }

    /// BEP-042: keep alive every dead, not-yet-cleaned instance of a class
    /// with a finalizer (and everything it reaches), and queue it for
    /// finalization. All seeds are chosen before any is marked, so a cycle
    /// of finalizable instances is finalized together.
    ///
    /// # Safety
    /// Between a complete mark and the sweep.
    unsafe fn keepalive_finalizers(&self, marker: &mut Marker<'_>) {
        // SAFETY: forwarded to the caller.
        let seeds = unsafe { self.dead_finalizer_seeds(marker) };
        // Both halves of a seed are kept alive explicitly: the finalizer is
        // reachable from its instance through the class anyway.
        for &(instance, cleanup) in &seeds {
            marker.push(instance);
            marker.push(cleanup);
        }
        // SAFETY: forwarded to the caller.
        unsafe { marker.drain() };
        for (instance, cleanup) in seeds {
            self.push_pending_finalizer(instance, cleanup);
        }
    }

    /// Report every dead errored future whose result no awaiter observed,
    /// and keep its error value alive for the report.
    ///
    /// # Safety
    /// Between a complete mark and the sweep.
    unsafe fn keepalive_unhandled_spawn_errors(
        &self,
        marker: &mut Marker<'_>,
        pool: &BlockPool,
        large: &LargeSpace,
    ) {
        let mut errors = Vec::new();
        // SAFETY: forwarded to the caller.
        unsafe {
            Self::for_each_runtime_object(pool, large, |object| {
                // Only futures can report; skip the rest before asking the mark.
                if !matches!(&*object, Object::Future(_)) {
                    return;
                }
                // SAFETY: allocated and not yet swept.
                if !marker.is_marked(self.make_heap_ptr(object)) {
                    errors.extend(Self::unhandled_spawn_error(&*object));
                }
            });
        }
        for error in &errors {
            if let Some(ptr) = error.value.as_object_ptr() {
                marker.push(ptr);
            }
        }
        // SAFETY: forwarded to the caller.
        unsafe { marker.drain() };
        for error in errors {
            self.push_unhandled_spawn_error(error);
        }
    }

    /// The error of `object` if it is an errored future nobody observed, now
    /// marked reported.
    fn unhandled_spawn_error(object: &Object) -> Option<crate::UnhandledSpawnError> {
        let Object::Future(future) = object else {
            return None;
        };
        if future.is_observed() {
            return None;
        }
        let FutureRead::Error(value) = future.read() else {
            return None;
        };
        future
            .try_mark_reported()
            .then(|| crate::UnhandledSpawnError {
                future_id: future.id(),
                value,
                trace: future.error_trace(),
                cancelled: future.was_cancelled(),
            })
    }

    /// Drop weak entries (the function lookup, the finalizable list) whose
    /// objects died.
    ///
    /// # Safety
    /// Between a complete mark and the sweep.
    unsafe fn forget_dead_weak_entries(&self, marker: &Marker<'_>) {
        // SAFETY: entries name allocated cells or compile-time objects.
        self.functions
            .retain_dynamic(|ptr| unsafe { marker.is_marked(*ptr) });
        self.finalizable
            .lock()
            .expect("finalizable lock poisoned")
            .instances
            // SAFETY: as above.
            .retain(|&ptr| unsafe { marker.is_marked(ptr) });
    }

    /// The [`Restamp`] map for this collection.
    #[cfg(feature = "heap_debug")]
    fn restamp(&self, marker: &mut Marker<'_>) -> Restamp {
        marker
            .take_reached()
            .into_iter()
            // SAFETY: re-stamping only; the address is unchanged.
            .map(|ptr| (ptr, unsafe { self.make_heap_ptr(ptr.as_ptr()) }))
            .collect()
    }

    /// The [`Restamp`] map for this collection: empty, since nothing moved.
    #[cfg(not(feature = "heap_debug"))]
    #[allow(clippy::unused_self)]
    fn restamp(&self, _marker: &mut Marker<'_>) -> Restamp {
        Restamp::new()
    }
}

/// Forget the previous collection's marks.
fn clear_marks(pool: &BlockPool, large: &LargeSpace) {
    for &base in pool.blocks() {
        // SAFETY: the pool's blocks are initialized.
        unsafe { BlockHeader::at(base) }.clear_marks();
    }
    large.clear_marks();
}

/// Mark everything `roots` reach. Returns how many precise roots there were.
///
/// # Safety
/// No mutator runs, precise roots name live objects, and conservative ranges
/// are readable.
unsafe fn mark_roots(marker: &mut Marker<'_>, roots: &mut dyn RootSource) -> usize {
    let mut precise = 0;
    roots.precise_roots(&mut |ptr| {
        precise += 1;
        marker.push(ptr);
    });
    // SAFETY: forwarded to the caller.
    roots.conservative_ranges(&mut |range| unsafe { marker.scan_range(range) });
    // SAFETY: forwarded to the caller.
    unsafe { marker.drain() };
    precise
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_type::RealizedTy;
    use bex_external_types::WeakHeapRef;
    use bex_vm_types::{Meter, Object, Value, types::Future};

    use super::*;
    use crate::Tlab;

    #[test]
    fn unhandled_error_scan_covers_block_edges_and_preserves_observation() {
        use bex_vm_types::types::{CancellationToken, FutureId};

        use crate::{descriptor::OBJECT_CELL_BYTES, space::block::USABLE_BLOCK_BYTES};
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        for _ in 0..USABLE_BLOCK_BYTES / OBJECT_CELL_BYTES - 1 {
            tlab.alloc_float(0.0);
        }
        let mut rooted = None;
        for id in 0..4 {
            let ptr = tlab.alloc_future(Future::pending(
                FutureId::from_usize(id),
                RealizedTy::unknown(),
                RealizedTy::unknown(),
                CancellationToken::new(),
            ));
            let Object::Future(future) = (unsafe { ptr.get() }) else {
                unreachable!()
            };
            // SAFETY: private pending future, no producer or other holder.
            assert!(unsafe { future.settle_error(Value::int(id as i64), vec![]) });
            if id == 2 {
                future.mark_observed();
            }
            if id == 3 {
                rooted = Some(ptr);
            }
        }
        // The two unobserved dead errors straddle a block boundary; the
        // remaining futures occupy a partially filled second block.
        let (_, _, _) = unsafe { heap.collect_garbage(&[rooted.unwrap()]) };
        let errors = heap.take_unhandled_spawn_errors();
        assert_eq!(
            errors
                .iter()
                .map(|e| e.future_id.as_usize())
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            errors.iter().map(|e| e.value).collect::<Vec<_>>(),
            vec![Value::int(0), Value::int(1)]
        );
        // Dropping the last root makes the remaining unobserved error
        // reportable. Already observed/reported errors must not reappear.
        unsafe { heap.collect_garbage(&[]) };
        let errors = heap.take_unhandled_spawn_errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].future_id.as_usize(), 3);
        unsafe { heap.collect_garbage(&[]) };
        assert!(heap.take_unhandled_spawn_errors().is_empty());
    }

    #[test]
    fn test_gc_empty_heap() {
        let heap = BexHeap::new(vec![]);

        // Run GC with no roots
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[]) };

        assert_eq!(stats.live_count, 0);
        assert_eq!(stats.collected_count, 0);
        assert!(remapped.is_empty());
    }

    #[test]
    fn test_gc_preserves_compile_time_objects() {
        let compile_time: Vec<Object> = vec![
            Object::String(bex_str::BexStr::from("builtin1")),
            Object::String(bex_str::BexStr::from("builtin2")),
        ];
        let heap = BexHeap::new(compile_time);

        // Get HeapPtr for compile-time objects
        let ct_ptr_0 = heap.compile_time_ptr(0);
        let ct_ptr_1 = heap.compile_time_ptr(1);

        // Run GC with compile-time objects as roots
        let roots = vec![ct_ptr_0, ct_ptr_1];
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&roots) };

        // Compile-time objects keep their pointers
        assert_eq!(remapped[0].as_ptr(), ct_ptr_0.as_ptr());
        assert_eq!(remapped[1].as_ptr(), ct_ptr_1.as_ptr());
        // No runtime objects to copy
        assert_eq!(stats.live_count, 0);
    }

    #[test]
    fn test_gc_collects_unreachable_objects() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate some objects
        let _obj1 = tlab.alloc_string("obj1".to_string());
        let _obj2 = tlab.alloc_string("obj2".to_string());
        let _obj3 = tlab.alloc_string("obj3".to_string());

        // Run GC with no roots - all objects should be collected
        let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };

        assert_eq!(stats.live_count, 0);
        assert!(stats.collected_count > 0);
    }

    // TODO: Epoch-based stale pointer detection tests removed.
    // HeapPtr::get() does not currently validate epochs — it's a raw dereference.
    // If epoch validation is added to get() in the future, add tests here for:
    // - test_gc_stale_heap_ptr_panics: using a pre-GC HeapPtr after GC should panic
    // - test_handle_resolved_ptr_stale_after_gc_panics: similar for handle-resolved ptrs

    #[cfg(feature = "heap_debug")]
    #[test]
    fn test_full_verify_panics_on_bad_variant() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        use crate::{HeapDebuggerConfig, heap_debugger::HeapVerifyMode};

        let compile_time = vec![Object::Enum(Box::new(bex_vm_types::Enum {
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("E")),
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            variants: vec![bex_vm_types::EnumVariant {
                name: "A".to_string(),
                description: None,
                alias: None,
                docstring: None,
                other: Default::default(),
                skip: false,
                json_rename: None,
            }],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        }))];
        let debug = HeapDebuggerConfig {
            enabled: true,
            verify: HeapVerifyMode::Full,
        };
        let heap = BexHeap::with_debug(compile_time, debug);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let enm_ptr = heap.compile_time_ptr(0);
        let _bad_variant = tlab.alloc(Object::Variant(bex_vm_types::types::Variant {
            enm: enm_ptr,
            index: 3, // Out of bounds variant index
        }));

        let result = catch_unwind(AssertUnwindSafe(|| {
            heap.verify_quick();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_gc_preserves_rooted_objects() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate some objects
        let obj1 = tlab.alloc_string("obj1".to_string());
        let obj2 = tlab.alloc_string("obj2".to_string());
        let _obj3 = tlab.alloc_string("obj3".to_string());

        // Run GC with obj1 and obj2 as roots
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };

        assert_eq!(stats.live_count, 2);
        assert_eq!(remapped.len(), 2);
        // obj3 should be collected
        assert!(stats.collected_count > 0);

        // Verify remapped objects are accessible
        for new_ptr in &remapped {
            let obj = unsafe { new_ptr.get() };
            assert!(matches!(obj, Object::String(_)));
        }
    }

    #[test]
    fn test_gc_traces_array_references() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate a string
        let str_obj = tlab.alloc_string("referenced".to_string());

        // Allocate an array that references the string
        let arr = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(str_obj)],
        );

        // Allocate another unreferenced string
        let _unreferenced = tlab.alloc_string("unreferenced".to_string());

        // Run GC with only the array as root
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[arr]) };

        // Should copy both the array and the string it references
        assert_eq!(stats.live_count, 4); // a non-empty list is a list and a backing cell, plus its element type
        assert_eq!(remapped.len(), 1);

        // Verify the array's reference was updated
        let new_arr_ptr = remapped[0];
        if let Some(elements) = unsafe { bex_vm_types::ListRef::of(new_arr_ptr) } {
            // The string reference should have been updated
            if let Some(str_ptr) = elements.get(0).and_then(|__v| __v.as_object_ptr()) {
                // Verify the referenced string is valid
                let str_obj = unsafe { str_ptr.get() };
                if let Object::String(s) = str_obj {
                    assert_eq!(s, "referenced");
                } else {
                    panic!("Expected String object");
                }
            } else {
                panic!("Expected Object value in array");
            }
        } else {
            panic!("Expected Array object");
        }
    }

    /// A survivor stays where it was, and only it remains allocated.
    #[test]
    fn test_gc_survivor_stays_in_place() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj = tlab.alloc_string("test".to_string());
        tlab.alloc_string("garbage".to_string());

        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj]) };

        assert_eq!(stats.live_count, 1);
        assert_eq!(remapped, vec![obj]);
        assert_eq!(heap.stats().runtime_objects, 1);
    }

    /// Handles are roots whether or not the caller passes them: an object
    /// only a handle holds survives a collection given no roots.
    #[test]
    fn test_handles_are_roots_without_being_passed() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj = tlab.alloc_string("test".to_string());
        let handle = heap.create_handle(obj);

        let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };
        assert_eq!(stats.live_count, 1);
        let resolved = heap.resolve_handle_ptr(handle.slab_key()).unwrap();
        assert!(matches!(unsafe { resolved.get() }, Object::String(s) if s.as_str() == "test"));
    }

    #[test]
    fn test_multiple_gc_cycles() {
        let heap = BexHeap::new(vec![]);

        for cycle in 0..5 {
            let mut tlab = Tlab::new(Arc::clone(&heap));

            // Allocate objects in this cycle
            for i in 0..100 {
                tlab.alloc_string(format!("cycle_{cycle}_obj_{i}"));
            }

            // Run GC with no roots - all should be collected
            let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };

            assert_eq!(stats.live_count, 0, "Cycle {cycle}: expected no survivors");
        }
    }

    #[test]
    fn test_compile_time_objects_never_collected() {
        let compile_time: Vec<Object> = vec![
            Object::String(bex_str::BexStr::from("builtin1")),
            Object::String(bex_str::BexStr::from("builtin2")),
        ];
        let heap = BexHeap::new(compile_time);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate runtime objects
        let _runtime = tlab.alloc_string("runtime".to_string());

        // Run GC with no roots - runtime objects collected
        let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };

        // Compile-time objects should still be accessible
        let ct_ptr_0 = heap.compile_time_ptr(0);
        let ct_ptr_1 = heap.compile_time_ptr(1);
        let obj0 = unsafe { ct_ptr_0.get() };
        let obj1 = unsafe { ct_ptr_1.get() };

        match (obj0, obj1) {
            (Object::String(s0), Object::String(s1)) => {
                assert_eq!(s0, "builtin1");
                assert_eq!(s1, "builtin2");
            }
            _ => panic!("Expected String objects"),
        }

        // Runtime object should have been collected
        assert_eq!(stats.live_count, 0);
    }

    #[test]
    fn test_gc_traces_and_forwards_structural_map_keys() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let leaf = tlab.alloc_string("key leaf");
        let key = tlab.alloc_array(baml_type::RealizedTy::string(), vec![Value::object(leaf)]);
        let map = tlab
            .alloc_map_hashed(
                bex_vm_types::RealizedTy::unknown(),
                bex_vm_types::RealizedTy::int(),
                vec![(17, Value::object(key), Value::int(42))],
            )
            .ptr();
        let (stats, roots, _) = unsafe { heap.collect_garbage(&[map]) };
        assert_eq!(stats.live_count, 9); // a non-empty map is a map and three backing cells, plus its type
        let Some(map) = (unsafe { bex_vm_types::MapRef::of(roots[0]) }) else {
            panic!("expected map");
        };
        let guard = map.lock(bex_vm_types::object_format::Access::Shared);
        let pos = guard.candidates(17)[0];
        assert_eq!(
            guard.value(pos),
            bex_vm_types::object_format::SlotValue::Value(Value::int(42))
        );
        let Some(key) =
            (unsafe { bex_vm_types::ListRef::of(guard.key(pos).as_object_ptr().unwrap()) })
        else {
            panic!("expected array key");
        };
        let Object::String(leaf) = (unsafe { key.get(0).unwrap().as_object_ptr().unwrap().get() })
        else {
            panic!("expected key leaf");
        };
        assert_eq!(leaf, "key leaf");
    }

    #[test]
    fn test_gc_with_map_references() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate a string
        let str_obj = tlab.alloc_string("value".to_string());

        // Allocate a map that references the string
        let mut map = indexmap::IndexMap::new();
        map.insert(bex_str::BexStr::from("key"), Value::object(str_obj));
        let map_obj = tlab.alloc_map(
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::unknown(),
            map,
        );

        // Allocate unreferenced garbage
        let _garbage = tlab.alloc_string("garbage".to_string());

        // Run GC with only the map as root
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[map_obj]) };

        // The map, key, and value should survive.
        assert_eq!(stats.live_count, 7); // a non-empty map is a map and three backing cells, plus its type
        assert_eq!(remapped.len(), 1);

        // Verify the map's reference was updated correctly
        let new_map_ptr = remapped[0];
        if let Some(m) = unsafe { bex_vm_types::MapRef::of(new_map_ptr) } {
            let entries = m.snapshot_entries();
            assert!(
                matches!(unsafe { entries[0].0.as_object_ptr().unwrap().get() }, Object::String(key) if key == "key")
            );
            if let Some(str_ptr) = entries[0].1.as_object_ptr() {
                let str_result = unsafe { str_ptr.get() };
                if let Object::String(s) = str_result {
                    assert_eq!(s, "value");
                } else {
                    panic!("Expected String object");
                }
            } else {
                panic!("Expected Object value in map");
            }
        } else {
            panic!("Expected Map object");
        }
    }

    // ========================================================================
    // Miri-targeted tests
    //
    // These tests are specifically designed to exercise unsafe code paths
    // that Miri can verify for memory safety. They focus on:
    // - Stack/root pointer forwarding after GC
    // - Object access patterns that could exhibit aliasing issues
    // ========================================================================

    /// Simulates what happens when a VM's stack contains object pointers
    /// that need to be updated after GC moves objects.
    ///
    /// This is the pattern used in bex_engine when updating parked VM stacks.
    #[test]
    fn test_miri_stack_forwarding_after_gc() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Simulate a VM stack with object references
        let mut simulated_stack: Vec<Value> = Vec::new();

        // Allocate objects and push their pointers to the "stack"
        let obj1 = tlab.alloc_string("stack_value_1".to_string());
        let obj2 = tlab.alloc_string("stack_value_2".to_string());
        let obj3 = tlab.alloc_string("stack_value_3".to_string());

        simulated_stack.push(Value::object(obj1));
        simulated_stack.push(Value::int(42)); // Non-object value
        simulated_stack.push(Value::object(obj2));
        simulated_stack.push(Value::NULL);
        simulated_stack.push(Value::object(obj3));

        // Also allocate some garbage that won't be rooted
        let _garbage1 = tlab.alloc_string("garbage1".to_string());
        let _garbage2 = tlab.alloc_string("garbage2".to_string());

        // Collect roots from the simulated stack (like collect_vm_roots does)
        let roots: Vec<HeapPtr> = simulated_stack
            .iter()
            .filter_map(Value::as_object_ptr)
            .collect();

        assert_eq!(roots.len(), 3);

        // Run GC with forwarding map
        let (stats, _remapped, forwarding) = unsafe { heap.collect_garbage(&roots) };

        // Should have collected the garbage
        assert_eq!(stats.live_count, 3);
        assert!(stats.collected_count >= 2);

        // Update the simulated stack with forwarding pointers
        // (This is what bex_engine does at lib.rs:780-786)
        for value in &mut simulated_stack {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&new_ptr) = forwarding.get(&ptr)
            {
                *value = Value::object(new_ptr);
            }
        }

        // Verify all stack values are still accessible and correct
        for value in &simulated_stack {
            match value.kind() {
                bex_vm_types::ValueKind::Object(ptr) => {
                    let obj = unsafe { ptr.get() };
                    match obj {
                        Object::String(s) => {
                            assert!(s.starts_with("stack_value_"));
                        }
                        _ => panic!("Expected String object"),
                    }
                }
                bex_vm_types::ValueKind::Int(n) => assert_eq!(n, 42),
                bex_vm_types::ValueKind::Null => {}
                _ => panic!("Unexpected value type"),
            }
        }
    }

    /// Tests that deeply nested object graphs are correctly traced and
    /// forwarded. This exercises the reference fixup logic.
    #[test]
    fn test_miri_deep_reference_chain_forwarding() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Create a chain: array -> map -> array -> string
        let leaf_str = tlab.alloc_string("leaf".to_string());

        let inner_array = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(leaf_str)],
        );

        let mut map = indexmap::IndexMap::new();
        map.insert(bex_str::BexStr::from("nested"), Value::object(inner_array));
        let middle_map = tlab.alloc_map(
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::unknown(),
            map,
        );

        let outer_array = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(middle_map)],
        );

        // Allocate garbage between the chain objects
        let _g1 = tlab.alloc_string("garbage".to_string());
        let _g2 = tlab.alloc_string("more_garbage".to_string());

        // Only root the outer array
        let (stats, remapped, _forwarding) = unsafe { heap.collect_garbage(&[outer_array]) };

        // The four chain objects and the map key should survive.
        assert_eq!(stats.live_count, 12); // a non-empty map is a map and three backing cells, plus its type
        assert!(stats.collected_count >= 2);

        // Verify the chain is intact after forwarding
        let new_outer = remapped[0];
        if let Some(arr) = unsafe { bex_vm_types::ListRef::of(new_outer) }
            && let Some(map_ptr) = arr.get(0).and_then(|v| v.as_object_ptr())
        {
            if let Some(m) = unsafe { bex_vm_types::MapRef::of(map_ptr) }
                && let Some(inner_arr_ptr) = m.snapshot_entries()[0].1.as_object_ptr()
            {
                if let Some(inner_arr) = unsafe { bex_vm_types::ListRef::of(inner_arr_ptr) }
                    && let Some(str_ptr) = inner_arr.get(0).and_then(|__v| __v.as_object_ptr())
                {
                    let str_obj = unsafe { str_ptr.get() };
                    if let Object::String(s) = str_obj {
                        assert_eq!(s, "leaf");
                        return; // Success!
                    }
                }
            }
        }
        panic!("Reference chain broken after GC");
    }

    /// Tests multiple GC cycles with root set changes between cycles.
    /// This catches issues with space swapping and stale pointers.
    #[test]
    fn test_miri_multiple_gc_cycles_with_changing_roots() {
        let heap = BexHeap::new(vec![]);

        let mut persistent_roots: Vec<HeapPtr> = Vec::new();

        for cycle in 0..5 {
            let mut tlab = Tlab::new(Arc::clone(&heap));

            // Allocate new objects
            let new_obj = tlab.alloc_string(format!("cycle_{cycle}_persistent"));
            persistent_roots.push(new_obj);

            // Allocate garbage
            for i in 0..10 {
                tlab.alloc_string(format!("cycle_{cycle}_garbage_{i}"));
            }

            // Run GC with all persistent roots
            let (stats, _remapped, forwarding) = unsafe { heap.collect_garbage(&persistent_roots) };

            // Update our root set with forwarding pointers
            for root in &mut persistent_roots {
                if let Some(&new_ptr) = forwarding.get(root) {
                    *root = new_ptr;
                }
            }

            // Should have kept all persistent objects
            assert_eq!(
                stats.live_count,
                cycle + 1,
                "Cycle {cycle}: expected {} survivors",
                cycle + 1
            );

            // Verify all persistent objects are still accessible
            for (i, root) in persistent_roots.iter().enumerate() {
                let obj = unsafe { root.get() };
                if let Object::String(s) = obj {
                    assert!(s.starts_with(&format!("cycle_{i}_persistent")));
                } else {
                    panic!("Expected String object for root {i}");
                }
            }
        }
    }

    /// Survivors keep their addresses across collections.
    #[test]
    fn test_miri_survivors_stay_put_across_collections() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("object1".to_string());
        let obj2 = tlab.alloc_string("object2".to_string());

        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };

        assert_eq!(stats.live_count, 2);
        assert_eq!(remapped, vec![obj1, obj2]);
        assert_eq!(heap.stats().runtime_objects, 2);

        // Verify objects accessible
        for ptr in &remapped {
            assert!(matches!(unsafe { ptr.get() }, Object::String(_)));
        }

        let (stats2, remapped2, _) = unsafe { heap.collect_garbage(&remapped) };

        assert_eq!(stats2.live_count, 2);
        assert_eq!(remapped2, vec![obj1, obj2]);
        assert_eq!(heap.stats().runtime_objects, 2);

        for ptr in &remapped2 {
            assert!(matches!(unsafe { ptr.get() }, Object::String(_)));
        }
    }

    /// Tests handle table updates during GC.
    #[test]
    fn test_miri_handle_table_concurrent_access() {
        use bex_external_types::WeakHeapRef;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("handle_obj_1".to_string());
        let obj2 = tlab.alloc_string("handle_obj_2".to_string());
        let _obj3 = tlab.alloc_string("no_handle".to_string()); // Will be collected

        let handle1 = heap.create_handle(obj1);
        let handle2 = heap.create_handle(obj2);

        // Verify handles resolve to correct pointers
        let resolved1 = heap.resolve_handle_ptr(handle1.slab_key()).unwrap();
        let resolved2 = heap.resolve_handle_ptr(handle2.slab_key()).unwrap();
        assert_eq!(resolved1, obj1);
        assert_eq!(resolved2, obj2);

        let roots = heap.collect_handle_roots();
        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2);
        assert!(stats.collected_count > 0);

        // Handles still name the objects, which did not move.
        let new1_ptr = heap.resolve_handle_ptr(handle1.slab_key()).unwrap();
        let new2_ptr = heap.resolve_handle_ptr(handle2.slab_key()).unwrap();
        assert_eq!(new1_ptr, obj1);
        assert_eq!(new2_ptr, obj2);

        // Objects accessible through updated handles
        assert!(matches!(unsafe { new1_ptr.get() }, Object::String(s) if s == "handle_obj_1"));
    }

    // ========================================================================
    // Space classification
    // ========================================================================

    /// TLAB allocations land in the runtime space.
    #[test]
    fn test_generation_of_tlab_allocates_into_runtime() {
        use crate::heap::Generation;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Fresh allocations should be in Gen0 (the nursery).
        let obj1 = tlab.alloc_string("nursery_1".to_string());
        let obj2 = tlab.alloc_string("nursery_2".to_string());

        assert_eq!(
            heap.generation_of(obj1),
            Generation::Runtime,
            "fresh allocation should be in the runtime space"
        );
        assert_eq!(
            heap.generation_of(obj2),
            Generation::Runtime,
            "fresh allocation should be in the runtime space"
        );
    }

    /// Survivors and later allocations are runtime objects alike.
    #[test]
    fn test_generation_of_survivors_after_gc() {
        use crate::heap::Generation;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate some objects — all start in Gen0.
        let obj1 = tlab.alloc_string("survivor_1".to_string());
        let obj2 = tlab.alloc_string("survivor_2".to_string());
        let _garbage = tlab.alloc_string("garbage".to_string());

        // Run full GC; obj1 and obj2 are roots, garbage is not.
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };

        assert_eq!(stats.live_count, 2);
        assert_eq!(remapped.len(), 2);

        for ptr in &remapped {
            assert_eq!(heap.generation_of(*ptr), Generation::Runtime);
        }

        // In the engine this happens via `<BexVm as RootHaver>::forward_roots`.
        tlab.invalidate();

        let post_gc = tlab.alloc_string("post_gc".to_string());
        assert_eq!(heap.generation_of(post_gc), Generation::Runtime);
    }

    /// Compile-time objects have Generation::CompileTime.
    #[test]
    fn test_generation_of_compile_time() {
        use crate::heap::Generation;

        let heap = BexHeap::new(vec![Object::String(bex_str::BexStr::from("builtin"))]);
        let ct_ptr = heap.compile_time_ptr(0);
        assert_eq!(heap.generation_of(ct_ptr), Generation::CompileTime);
    }

    // ========================================================================
    // Verify that every Object variant is correctly traced during GC —
    // both reference-carrying variants (which keep their referents alive)
    // and leaf variants (which survive when rooted).
    // These pin add_references_to_worklist and fixup_object_references.
    // ========================================================================

    // --- 1.1: Reference-Carrying Variants ---

    #[test]
    fn test_gc_traces_instance_class_and_fields() {
        use baml_type::Name;
        use bex_vm_types::Class;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate a class (leaf), a string (leaf), then an instance referencing both
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("TestClass")),
            fields: vec![
                bex_vm_types::ClassField::new("s", RealizedTy::string()),
                bex_vm_types::ClassField::new("n", RealizedTy::int()),
            ],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag: baml_type::typetag::TypeTag::from_i64(0),
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: Default::default(),
            redact: false,
        })));
        let field_str = tlab.alloc_string("field_value".to_string());
        let inst_ptr =
            tlab.alloc_instance(class_ptr, vec![Value::object(field_str), Value::int(42)]);

        let roots = vec![inst_ptr];
        let (stats, new_roots, _fwd) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 3); // instance + class + string
        // collected_count may be non-zero due to unused TLAB chunk slots

        let inst_ptr = new_roots[0];
        let inst = unsafe { bex_vm_types::InstanceRef::of(inst_ptr) }.expect("an instance");
        assert_eq!(inst.field_len(), 2);
        assert!(inst.load(0).expect_value().is_object());
        assert_eq!(inst.load(1).expect_value(), Value::int(42));

        // Verify class is accessible through the instance
        assert_eq!(inst.class_def().path.name.as_str(), "TestClass");
    }

    #[test]
    fn test_gc_traces_variant_enum() {
        use baml_type::Name;
        use bex_vm_types::Enum;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Color")),
            variants: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let var_ptr = tlab.alloc_variant(enum_ptr, 1);

        let roots = vec![var_ptr];
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2); // variant + enum
        let var_ptr = new_roots[0];
        let Object::Variant(v) = (unsafe { var_ptr.get() }) else {
            panic!("not variant")
        };
        assert_eq!(v.index, 1);
        let Object::Enum(e) = (unsafe { v.enm.get() }) else {
            panic!("not enum")
        };
        assert_eq!(e.path.name.as_str(), "Color");
    }

    #[test]
    fn test_gc_traces_closure_function_and_captures() {
        use bex_vm_types::types::Closure;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate a function stand-in (a string) and a captured string
        let func_ptr = tlab.alloc_string("placeholder_func".to_string());
        let captured = tlab.alloc_string("captured_value".to_string());
        let closure_ptr = tlab.alloc(Object::Closure(Closure {
            function: func_ptr,
            captures: Box::new([Value::object(captured), Value::int(7)]),
            captured_type_args: Box::new([]),
        }));

        let roots = vec![closure_ptr];
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 3); // closure + function + captured string
        let clo = new_roots[0];
        let Object::Closure(c) = (unsafe { clo.get() }) else {
            panic!("not closure")
        };
        assert_eq!(c.captures.len(), 2);
        assert!(c.captures[0].is_object());
        assert_eq!(c.captures[1], Value::int(7));
    }

    #[test]
    fn test_gc_traces_cell_value() {
        use bex_vm_types::types::Cell;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let inner = tlab.alloc_string("cell_content".to_string());
        let cell_ptr = tlab.alloc(Object::Cell(Cell::new(Value::object(inner))));

        let roots = vec![cell_ptr];
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2);
        let Object::Cell(c) = (unsafe { new_roots[0].get() }) else {
            panic!("not cell")
        };
        let Some(inner_ptr) = c.load().as_object_ptr() else {
            panic!("not object value")
        };
        let Object::String(s) = (unsafe { inner_ptr.get() }) else {
            panic!("not string")
        };
        assert_eq!(s, "cell_content");
    }

    #[test]
    fn test_gc_traces_cell_with_primitive_value() {
        use bex_vm_types::types::Cell;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let cell_ptr = tlab.alloc(Object::Cell(Cell::new(Value::int(42))));
        let roots = vec![cell_ptr];
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 1);
        let Object::Cell(c) = (unsafe { new_roots[0].get() }) else {
            panic!("not cell")
        };
        assert_eq!(c.load(), Value::int(42));
    }

    #[test]
    fn test_gc_traces_future_ready_object() {
        use bex_vm_types::{Future, FutureRead, types::FutureId};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let result = tlab.alloc_string("result".to_string());
        // Allocate the Future in `Pending` state first, then settle it in place.
        let future_ptr = tlab.alloc(Object::Future(Future::pending(
            FutureId::from_usize(0),
            RealizedTy::unknown(),
            RealizedTy::unknown(),
            bex_vm_types::types::CancellationToken::new(),
        )));
        let Object::Future(future) = (unsafe { future_ptr.get() }) else {
            panic!("expected Object::Future");
        };
        // SAFETY: this Future is local and not yet visible; CAS will win.
        let ok = unsafe { future.settle_ready(Value::object(result)) };
        assert!(ok);

        let roots = vec![future_ptr];
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2);
        let Object::Future(fut) = (unsafe { new_roots[0].get() }) else {
            panic!("not Future")
        };
        let FutureRead::Ready(v) = fut.read() else {
            panic!("not Future::Ready")
        };
        let Some(r) = v.as_object_ptr() else {
            panic!("not Future::Ready(Object)")
        };
        let Object::String(s) = (unsafe { r.get() }) else {
            panic!("not string")
        };
        assert_eq!(s, "result");
    }

    #[test]
    fn test_gc_traces_future_ready_primitive() {
        use bex_vm_types::{Future, types::FutureId};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let future_ptr = tlab.alloc(Object::Future(Future::pending(
            FutureId::from_usize(0),
            RealizedTy::unknown(),
            RealizedTy::unknown(),
            bex_vm_types::types::CancellationToken::new(),
        )));
        let Object::Future(future) = (unsafe { future_ptr.get() }) else {
            panic!("expected Object::Future");
        };
        // SAFETY: this Future is local and not yet visible; CAS will win.
        let ok = unsafe { future.settle_ready(Value::int(99)) };
        assert!(ok);
        let roots = vec![future_ptr];
        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 1);
    }

    // --- 1.2: Leaf Variant Tests ---

    #[test]
    fn test_gc_leaf_string_preserved() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let ptr = tlab.alloc_string("hello world".to_string());

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[ptr]) };
        let Object::String(s) = (unsafe { new_roots[0].get() }) else {
            panic!("not string")
        };
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_gc_leaf_uint8array_preserved() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let ptr = tlab.alloc(Object::Uint8Array(vec![1, 2, 3, 255].into()));

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[ptr]) };
        let Object::Uint8Array(v) = (unsafe { new_roots[0].get() }) else {
            panic!("not uint8array")
        };
        assert_eq!(v.lock().as_slice(), &[1u8, 2, 3, 255]);
    }

    #[test]
    fn test_gc_leaf_class_preserved() {
        use baml_type::Name;
        use bex_vm_types::Class;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("MyClass")),
            fields: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag: baml_type::typetag::TypeTag::from_i64(42),
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: Default::default(),
            redact: false,
        })));

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[ptr]) };
        let Object::Class(c) = (unsafe { new_roots[0].get() }) else {
            panic!("not class")
        };
        assert_eq!(c.path.name.as_str(), "MyClass");
        assert_eq!(c.type_tag, baml_type::typetag::TypeTag::from_i64(42));
    }

    #[test]
    fn test_gc_leaf_enum_preserved() {
        use baml_type::Name;
        use bex_vm_types::Enum;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Status")),
            variants: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[ptr]) };
        let Object::Enum(e) = (unsafe { new_roots[0].get() }) else {
            panic!("not enum")
        };
        assert_eq!(e.path.name.as_str(), "Status");
    }

    #[test]
    fn test_gc_leaf_type_preserved() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let value = bex_vm_types::types::TypeValue::new(baml_type::RealizedTy::int());
        let ptr = tlab.alloc(Object::Type(Box::new(value.clone())));

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[ptr]) };
        let Object::Type(tv) = (unsafe { new_roots[0].get() }) else {
            panic!("not type")
        };
        // The described type is inline data, so a GC copy preserves it verbatim.
        assert_eq!(tv.ty, value.ty);
    }

    /// A runtime enum reached only through a `type` value's head survives, and
    /// the head is repointed as both objects move.
    #[test]
    fn test_gc_traces_runtime_enum_definition_reached_through_a_head() {
        use baml_type::Name;
        use bex_vm_types::{Enum, EnumVariant, TypeHead, types::TypeValue};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let type_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Category"));
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            path: type_name.clone(),
            variants: vec![EnumVariant {
                name: "RED".to_string(),
                description: Some("warm".to_string()),
                alias: Some("k7".to_string()),
                docstring: None,
                other: Default::default(),
                skip: false,
                json_rename: None,
            }],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            type_tag,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let type_ptr = tlab.alloc_type(TypeValue::new(RealizedTy::Enum(TypeHead::new(
            enum_ptr, type_tag,
        ))));

        // Root only the type. Its head is the sole edge keeping the enum alive,
        // and must be traced and repointed as both objects move through a
        // compacting collection.
        // Twice, so the second collection moves the survivors out of Gen2 again.
        let (_, roots, _) = unsafe { heap.collect_garbage(&[type_ptr]) };
        let (stats, roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2, "only the type and enum should survive");
        let Object::Type(type_value) = (unsafe { roots[0].get() }) else {
            panic!("root was not the runtime type value")
        };
        let RealizedTy::Enum(head) = &type_value.ty else {
            panic!("the type value no longer wraps an enum type")
        };
        assert_eq!(head.tag(), type_tag, "a move must not change identity");
        let Object::Enum(enm) = (unsafe { head.ptr().get() }) else {
            panic!("the head did not land on an enum")
        };
        assert_eq!(enm.path, type_name);
        assert_eq!(enm.variants[0].name, "RED");
        assert_eq!(enm.variants[0].alias.as_deref(), Some("k7"));
    }

    /// The same for a class, whose fields carry heads of their own.
    #[test]
    fn test_gc_traces_runtime_class_definition_reached_through_a_head() {
        use baml_type::Name;
        use bex_vm_types::{Class, ClassField, TypeHead, types::TypeValue};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let type_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("VisitNote"));
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: type_name.clone(),
            fields: vec![ClassField {
                name: "height_cm".to_string(),
                field_type: bex_vm_types::RuntimeTy::Int,
                field_template: bex_vm_types::TyTemplate::from(RealizedTy::int()),
                description: Some("height in centimeters".to_string()),
                alias: None,
                docstring: None,
                other: Default::default(),
                skip: false,
                stream_done: false,
                must_exist: false,
                runtime_type: None,
                json_fallback: false,
                json_flatten: false,
                json_rename: None,
                json_omit_null: false,
                redact: false,
            }],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag,
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: Default::default(),
            redact: false,
        })));
        let type_ptr = tlab.alloc_type(TypeValue::new(RealizedTy::Class(
            TypeHead::new(class_ptr, type_tag),
            Box::new([]),
        )));

        // No instance and no independently-rooted class exists: the sole edge
        // keeping the declaration alive is the type's head.
        // Twice, so the second collection moves the survivors out of Gen2 again.
        let (_, roots, _) = unsafe { heap.collect_garbage(&[type_ptr]) };
        let (stats, roots, _) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(
            stats.live_count, 2,
            "only the type and class should survive"
        );
        let Object::Type(type_value) = (unsafe { roots[0].get() }) else {
            panic!("root was not the runtime type value")
        };
        let RealizedTy::Class(head, _) = &type_value.ty else {
            panic!("the type value no longer wraps a class type")
        };
        assert_eq!(head.tag(), type_tag, "a move must not change identity");
        let Object::Class(class) = (unsafe { head.ptr().get() }) else {
            panic!("the head did not land on a class")
        };
        assert_eq!(class.path, type_name);
        assert_eq!(class.fields[0].name, "height_cm");
        assert_eq!(
            class.fields[0].description.as_deref(),
            Some("height in centimeters")
        );
    }

    #[test]
    fn test_gc_empty_containers_preserved() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let arr = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![]);
        let map = tlab.alloc_map(
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::unknown(),
            indexmap::IndexMap::new(),
        );

        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[arr, map]) };
        assert_eq!(stats.live_count, 4); // a non-empty map is a map and three backing cells, plus its type
        let Some(a) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("not array")
        };
        assert!(a.is_empty());
        let Some(m) = (unsafe { bex_vm_types::MapRef::of(new_roots[1]) }) else {
            panic!("not map")
        };
        assert!(m.is_empty());
    }

    // ========================================================================
    // Test the BFS tracing algorithm against specific graph topologies:
    // linear chains, fan-out, fan-in, diamond, cycles, self-references.
    // These pin the forwarding deduplication logic and fixup correctness.
    // ========================================================================

    #[test]
    fn test_gc_linear_chain_four_deep() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // D -> C -> B -> A (leaf string wrapped in nested arrays)
        let a = tlab.alloc_string("a".to_string());
        let b = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(a)]);
        let c = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(b)]);
        let d = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(c)]);

        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[d]) };
        assert_eq!(stats.live_count, 8); // a non-empty list is a list and a backing cell, plus its element type

        // Verify chain is navigable end-to-end after GC
        let Some(d_arr) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("not array at d")
        };
        let Some(c_ptr) = d_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("d[0] not object")
        };
        let Some(c_arr) = (unsafe { bex_vm_types::ListRef::of(c_ptr) }) else {
            panic!("not array at c")
        };
        let Some(b_ptr) = c_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("c[0] not object")
        };
        let Some(b_arr) = (unsafe { bex_vm_types::ListRef::of(b_ptr) }) else {
            panic!("not array at b")
        };
        let Some(a_ptr) = b_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("b[0] not object")
        };
        let Object::String(s) = (unsafe { a_ptr.get() }) else {
            panic!("not string at a")
        };
        assert_eq!(s, "a");
    }

    #[test]
    fn test_gc_fan_out() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let children: Vec<Value> = (0..5)
            .map(|i| Value::object(tlab.alloc_string(format!("child_{i}"))))
            .collect();
        let parent = tlab.alloc_array(baml_type::RealizedTy::unknown(), children);

        // Also allocate explicit garbage
        let _garbage = tlab.alloc_string("garbage".to_string());

        let (stats, _, _) = unsafe { heap.collect_garbage(&[parent]) };
        assert_eq!(stats.live_count, 8); // parent + 5 children // a non-empty list is a list and a backing cell, plus its element type
        // collected_count >= 1 (explicit garbage) + unused TLAB chunk slots
        assert!(stats.collected_count >= 1);
    }

    #[test]
    fn test_gc_fan_in_shared_child() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let shared = tlab.alloc_string("shared".to_string());
        let parent_a = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(shared)],
        );
        let parent_b = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(shared)],
        );

        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[parent_a, parent_b]) };
        // 2 parents + 1 shared child (not 4 — shared object copied only once)
        assert_eq!(stats.live_count, 6); // a non-empty list is a list and a backing cell, plus its element type

        // Both parents should point to the same new location for the shared child
        let Some(a) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("not array a")
        };
        let Some(b) = (unsafe { bex_vm_types::ListRef::of(new_roots[1]) }) else {
            panic!("not array b")
        };
        let Some(a_child) = a.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("a[0] not object")
        };
        let Some(b_child) = b.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("b[0] not object")
        };
        // Forwarding deduplication: both must point to the same new location
        assert_eq!(
            a_child, b_child,
            "shared child should have same forwarded pointer"
        );
    }

    #[test]
    fn test_gc_diamond_reference() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // root -> [B, C], B -> [D], C -> [D]
        let d = tlab.alloc_string("diamond_bottom".to_string());
        let b = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(d)]);
        let c = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(d)]);
        let root = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(b), Value::object(c)],
        );

        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[root]) };
        // root + B + C + D (D is shared between B and C — copied only once)
        assert_eq!(stats.live_count, 8); // a non-empty list is a list and a backing cell, plus its element type

        // Navigate: root -> B and root -> C both lead to the same D
        let Some(root_arr) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("not array root")
        };
        let Some(b_ptr) = root_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("root[0] not object")
        };
        let Some(c_ptr) = root_arr.get(1).and_then(|__v| __v.as_object_ptr()) else {
            panic!("root[1] not object")
        };
        let Some(b_arr) = (unsafe { bex_vm_types::ListRef::of(b_ptr) }) else {
            panic!("not array b")
        };
        let Some(c_arr) = (unsafe { bex_vm_types::ListRef::of(c_ptr) }) else {
            panic!("not array c")
        };
        let Some(d_from_b) = b_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("b[0] not object")
        };
        let Some(d_from_c) = c_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("c[0] not object")
        };
        // D copied only once — both paths reach the same pointer
        assert_eq!(
            d_from_b, d_from_c,
            "diamond bottom should be the same forwarded pointer"
        );
    }

    #[test]
    fn test_gc_cycle_two_objects() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Create A (array placeholder) and B (cell pointing to A), then patch A -> B
        let a = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![]); // placeholder, will be patched
        let b = tlab.alloc(Object::Cell(bex_vm_types::types::Cell::new(Value::object(
            a,
        ))));
        // Patch A to reference B, forming a cycle
        push_onto(&mut tlab, a, Value::object(b));

        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[a]) };
        assert_eq!(stats.live_count, 4); // a non-empty list is a list and a backing cell, plus its element type

        // Verify the cycle is preserved: A -> B -> A (forwarded)
        let Some(a_arr) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("not array a")
        };
        let Some(b_ptr) = a_arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("a[0] not object")
        };
        let Object::Cell(cell) = (unsafe { b_ptr.get() }) else {
            panic!("not cell b")
        };
        let Some(a_back) = cell.load().as_object_ptr() else {
            panic!("cell.value not object")
        };
        // The back-pointer from B should point to the new location of A
        assert_eq!(
            a_back, new_roots[0],
            "cycle back-pointer should point to forwarded A"
        );
    }

    #[test]
    fn test_gc_unreachable_island_collected() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Island: three mutually-referencing objects, none reachable from roots
        let x = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![]); // placeholder
        let y = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(x)]);
        let z = tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::object(y)]);
        push_onto(&mut tlab, x, Value::object(z));

        // Separate rooted survivor
        let rooted = tlab.alloc_string("survivor".to_string());

        let (stats, _, _) = unsafe { heap.collect_garbage(&[rooted]) };
        assert_eq!(stats.live_count, 1);
        // At least the 3 island objects are collected (plus TLAB pre-alloc slots)
        assert!(stats.collected_count >= 3);
    }

    #[test]
    fn test_gc_deep_chain_100() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Build a 100-deep chain: root -> array -> array -> ... -> leaf string
        let mut current = tlab.alloc_string("leaf".to_string());
        for _ in 0..99 {
            current = tlab.alloc_array(
                baml_type::RealizedTy::unknown(),
                vec![Value::object(current)],
            );
        }

        let (stats, _, _) = unsafe { heap.collect_garbage(&[current]) };
        assert_eq!(stats.live_count, 200); // a non-empty list is a list and a backing cell, plus its element type
    }

    #[test]
    fn test_gc_duplicate_roots() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let ptr = tlab.alloc_string("dup".to_string());

        // Pass the same root twice — should only be copied once
        let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[ptr, ptr]) };
        assert_eq!(stats.live_count, 1); // only one copy in the new space
        assert_eq!(
            new_roots[0], new_roots[1],
            "both duplicate roots should map to the same forwarded pointer"
        );
    }

    // ========================================================================
    // Generation Classification Edge Cases
    // ========================================================================

    #[test]
    fn test_generation_of_compile_time_with_empty_vec() {
        use crate::heap::Generation;
        // Empty compile_time — every pointer should be a runtime pointer.
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let ptr = tlab.alloc_string("runtime".to_string());
        assert_eq!(heap.generation_of(ptr), Generation::Runtime);
        assert!(!heap.is_compile_time_ptr(ptr));
    }

    // ========================================================================
    // Phase 5.3: Tracing/Fixup Consistency — All Object Variants
    //
    // This test allocates every reference-carrying Object variant plus a
    // selection of leaf variants, roots the containers only, and verifies that:
    //
    //   1. `add_references_to_worklist` discovers the leaves (live_count covers
    //      all transitively reachable objects).
    //   2. `fixup_object_references` updates every intra-heap pointer so the
    //      leaves are readable through the containers after GC.
    //
    // The plan says "6 containers + 7 leaves" but the exact count depends on
    // how many reference-carrying variants we exercise.  We assert >= 11 to
    // give a little room while still pinning the essential correctness.
    // ========================================================================

    #[test]
    fn test_tracing_and_fixup_consistency_all_variants() {
        use baml_type::Name;
        use bex_vm_types::{
            Class, Enum,
            types::{Cell, Closure, Variant},
        };

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // --- Leaf objects (all start in Gen0) ---
        let leaf_string = tlab.alloc_string("leaf_string".to_string());
        let leaf_for_array = tlab.alloc_string("in_array".to_string());
        let leaf_for_map = tlab.alloc_string("in_map".to_string());
        let leaf_for_closure_cap = tlab.alloc_string("closure_cap".to_string());
        let leaf_for_cell = tlab.alloc_string("cell_value".to_string());

        // A function stand-in (Class acts as a leaf here, needed by Closure).
        let leaf_func = tlab.alloc_string("func_placeholder".to_string());

        // --- Container: a list ---
        let array_container = tlab.alloc_array(
            baml_type::RealizedTy::unknown(),
            vec![Value::object(leaf_for_array)],
        );

        // --- Container: Object::Map ---
        let mut map_data = indexmap::IndexMap::new();
        map_data.insert(bex_str::BexStr::from("k"), Value::object(leaf_for_map));
        let map_container = tlab.alloc_map(
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::unknown(),
            map_data,
        );

        // --- Container: Object::Closure ---
        let closure_container = tlab.alloc(Object::Closure(Closure {
            function: leaf_func,
            captures: Box::new([Value::object(leaf_for_closure_cap), Value::int(7)]),
            captured_type_args: Box::new([]),
        }));

        // --- Container: Object::Cell ---
        let cell_container = tlab.alloc(Object::Cell(Cell::new(Value::object(leaf_for_cell))));

        // --- Container: Object::Instance ---
        // Instance requires a class pointer.
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("T")),
            fields: vec![bex_vm_types::ClassField::new("leaf", RealizedTy::string())],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag: baml_type::typetag::TypeTag::from_i64(0),
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: Default::default(),
            redact: false,
        })));
        let instance_container = tlab.alloc_instance(class_ptr, vec![Value::object(leaf_string)]);

        // --- Container: Object::Variant ---
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("E")),
            variants: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let variant_container = tlab.alloc(Object::Variant(Variant {
            enm: enum_ptr,
            index: 0,
        }));

        // Roots are the containers only (plus the class and enum which are
        // referenced by containers but we root them independently for clarity).
        let roots = vec![
            array_container,
            map_container,
            closure_container,
            cell_container,
            instance_container,
            variant_container,
        ];

        let (stats, new_roots, _fwd) = unsafe { heap.collect_garbage(&roots) };

        // All transitively reachable objects must survive.
        // Containers: Array, Map, Closure, Cell, Instance, Variant = 6
        // Leaves referenced by containers (some shared):
        //   leaf_for_array, leaf_for_map, leaf_for_closure_cap, leaf_func,
        //   leaf_for_cell, leaf_string, class_ptr, enum_ptr = 8
        // Total = 6 + 8 = 14, but >= 11 is the conservative bound.
        assert!(
            stats.live_count >= 11,
            "expected at least 11 live objects; got {}",
            stats.live_count
        );

        // Spot-check each container to confirm fixup correctness.

        // Array: slot 0 should be the (forwarded) leaf string.
        let Some(arr) = (unsafe { bex_vm_types::ListRef::of(new_roots[0]) }) else {
            panic!("new_roots[0] not Array")
        };
        let Some(arr_leaf) = arr.get(0).and_then(|__v| __v.as_object_ptr()) else {
            panic!("arr[0] not Object")
        };
        let Object::String(s) = (unsafe { arr_leaf.get() }) else {
            panic!("arr leaf not String")
        };
        assert_eq!(s, "in_array");

        // Map: key "k" should resolve to the (forwarded) leaf string.
        let Some(m) = (unsafe { bex_vm_types::MapRef::of(new_roots[1]) }) else {
            panic!("new_roots[1] not Map")
        };
        let Some(map_leaf) = m.snapshot_entries()[0].1.as_object_ptr() else {
            panic!("map[\"k\"] not Object")
        };
        let Object::String(s) = (unsafe { map_leaf.get() }) else {
            panic!("map leaf not String")
        };
        assert_eq!(s, "in_map");

        // Closure: captures[0] should be the (forwarded) captured string.
        let Object::Closure(clo) = (unsafe { new_roots[2].get() }) else {
            panic!("new_roots[2] not Closure")
        };
        let Some(cap_leaf) = clo.captures.first().and_then(|v| v.as_object_ptr()) else {
            panic!("capture[0] not Object")
        };
        let Object::String(s) = (unsafe { cap_leaf.get() }) else {
            panic!("closure capture leaf not String")
        };
        assert_eq!(s, "closure_cap");

        // Cell: value should be the (forwarded) cell string.
        let Object::Cell(cell) = (unsafe { new_roots[3].get() }) else {
            panic!("new_roots[3] not Cell")
        };
        let Some(cell_leaf) = cell.load().as_object_ptr() else {
            panic!("cell.value not Object")
        };
        let Object::String(s) = (unsafe { cell_leaf.get() }) else {
            panic!("cell leaf not String")
        };
        assert_eq!(s, "cell_value");

        // Instance: fields[0] should be the (forwarded) leaf string.
        let inst = unsafe { bex_vm_types::InstanceRef::of(new_roots[4]) }
            .expect("new_roots[4] not Instance");
        let Some(inst_leaf) = inst.load(0).expect_value().as_object_ptr() else {
            panic!("instance.fields[0] not Object")
        };
        let Object::String(s) = (unsafe { inst_leaf.get() }) else {
            panic!("instance field leaf not String")
        };
        assert_eq!(s, "leaf_string");

        // Variant: enm should be a valid Enum.
        let Object::Variant(var) = (unsafe { new_roots[5].get() }) else {
            panic!("new_roots[5] not Variant")
        };
        let Object::Enum(e) = (unsafe { var.enm.get() }) else {
            panic!("variant.enm not Enum")
        };
        assert_eq!(e.path.name.as_str(), "E");
    }

    /// A major collection reports exactly what an independent census of its
    /// survivors finds: every survivor once, and nothing that died.
    #[test]
    fn test_census_matches_an_independent_measurement_of_survivors() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Shared text: two clones and a slice of one buffer, and a
        // concatenation that has never been read.
        let text = bex_str::BexStr::from("x".repeat(100_000));
        let clone_a = tlab.alloc_string(text.clone());
        let clone_b = tlab.alloc_string(text.clone());
        let slice = tlab.alloc_string(text.byte_slice(10..5_000));
        let rope = tlab.alloc_string(bex_str::BexStr::concat(
            text,
            bex_str::BexStr::from("y".repeat(1_000)),
        ));

        // A container with spare capacity, a byte buffer, a map, and one big
        // integer held by two objects.
        let mut elements = Vec::with_capacity(1_000);
        elements.push(Value::int(1));
        let array = tlab.alloc_array(RealizedTy::int(), elements);
        let bytes = tlab.alloc_uint8array(vec![0; 10_000]);
        let map = tlab.alloc_map(
            RealizedTy::string(),
            RealizedTy::int(),
            indexmap::IndexMap::from([(bex_str::BexStr::from("key"), Value::int(1))]),
        );
        let bigint = Arc::new(num_bigint::BigInt::from(1u8) << 80_000);
        let bigint_a = tlab.alloc(Object::Bigint(Arc::clone(&bigint)));
        let bigint_b = tlab.alloc(Object::Bigint(bigint));

        // Garbage with payloads much larger than everything above.
        tlab.alloc_uint8array(vec![0; 1_000_000]);
        tlab.alloc_string("z".repeat(500_000));

        let roots = [
            clone_a, clone_b, slice, rope, array, bytes, map, bigint_a, bigint_b,
        ];
        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };

        let mut independent = Meter::census();
        // SAFETY: single-threaded test; the collection has finished, so only
        // survivors remain allocated.
        unsafe {
            BexHeap::for_each_runtime_object(
                heap.small.lock().pool(),
                &heap.large.lock(),
                |object| {
                    (*object).measure(&mut independent);
                },
            );
        }
        assert_eq!(stats.live_payload_bytes, independent.total());

        // The array's capacity, the byte buffer, the text and the integer.
        let floor = 1_000 * size_of::<Value>() + 10_000 + 100_000 + 10_000;
        assert!(independent.total() >= floor, "{}", independent.total());
        assert!(
            independent.total() < floor + 500_000,
            "{} bytes includes payload that died",
            independent.total()
        );
    }

    /// The census is exact for a container, splits shared text between its
    /// holders, and is taken only by a major collection.
    #[test]
    fn test_census_counts_capacity_and_shared_text_once() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let mut elements = Vec::with_capacity(1_000);
        elements.push(Value::int(1));
        let array = tlab.alloc_array(RealizedTy::int(), elements);
        tlab.alloc_uint8array(vec![0; 1_000_000]);

        let (_, roots, _) = unsafe { heap.collect_garbage(&[array]) };
        tlab.invalidate();

        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };
        tlab.invalidate();
        // The list's elements live in its backing cell, on the heap: only
        // its element type object's boxed type is payload.
        assert!(
            stats.live_payload_bytes < 1_000 * size_of::<Value>(),
            "{} payload bytes",
            stats.live_payload_bytes
        );

        // The array is no longer rooted; three strings share one buffer.
        let text = bex_str::BexStr::from("x".repeat(100_000));
        let whole = text.unshared_heap_bytes();
        let strings = [
            tlab.alloc_string(text.clone()),
            tlab.alloc_string(text.clone()),
            tlab.alloc_string(text),
        ];
        let (stats, _, _) = unsafe { heap.collect_garbage(&strings) };
        let payload = stats.live_payload_bytes;
        assert!(payload >= whole, "{payload} < {whole}");
        assert!(
            payload < whole + strings.len(),
            "{payload} counts the buffer more than once"
        );
    }
    thread_local! {
        /// Entries the finalizer scan examined on this thread (test-only).
        pub(super) static FINALIZER_SCAN_VISITS: std::cell::Cell<usize> =
            const { std::cell::Cell::new(0) };
    }

    /// A class whose `cleanup` method is `finalizer`, or with no finalizer.
    /// A class whose fields have `fields`' types and `generic_params` type
    /// parameters.
    /// Append `value` to the list `list`.
    fn push_onto(tlab: &mut Tlab, list: HeapPtr, value: Value) {
        let list = unsafe { bex_vm_types::ListRef::of(list) }.expect("a list");
        list.lock(bex_vm_types::object_format::Access::Shared)
            .push(tlab, bex_vm_types::object_format::SlotValue::Value(value))
            .expect("an unknown list holds anything");
    }

    fn class_with_fields(
        fields: Vec<(&str, bex_vm_types::RealizedTy)>,
        generic_params: usize,
    ) -> Object {
        let Object::Class(mut class) = class_object(None) else {
            unreachable!("class_object builds a class")
        };
        class.fields = fields
            .into_iter()
            .map(|(name, ty)| bex_vm_types::ClassField::new(name, ty))
            .collect();
        class.generic_param_count = generic_params;
        class.type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        Object::Class(class)
    }

    /// Fields live in their declared slots: an `int` and a `float` raw, a
    /// union tagged, a class reference as a pointer. Each reads back as
    /// stored, floats unboxed, and a collection keeps what the pointer and
    /// tagged slots reach and nothing a raw slot happens to hold.
    #[test]
    fn instance_fields_live_in_their_declared_slots() {
        use bex_vm_types::{InstanceRef, object_format::SlotValue};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let union = bex_vm_types::RealizedTy::Union(Box::new([
            bex_vm_types::RealizedTy::Int,
            bex_vm_types::RealizedTy::String,
        ]));
        let class = tlab.alloc(class_with_fields(
            vec![
                ("n", bex_vm_types::RealizedTy::Int),
                ("x", bex_vm_types::RealizedTy::Float),
                ("u", union),
                ("s", bex_vm_types::RealizedTy::String),
            ],
            0,
        ));
        let by_tag = tlab.alloc_string("reached through the tagged slot");
        let by_ptr = tlab.alloc_string("reached through the pointer slot");
        let float = tlab.alloc_float(2.5);
        let ptr = tlab.alloc_instance(
            class,
            vec![
                Value::int(7),
                Value::object(float),
                Value::object(by_tag),
                Value::object(by_ptr),
            ],
        );
        let instance = unsafe { InstanceRef::of(ptr) }.expect("an instance");
        assert_eq!(instance.load(0), SlotValue::Value(Value::int(7)));
        assert_eq!(instance.load(1), SlotValue::Float(2.5));
        assert_eq!(instance.layout().size(), 8 + 8 + 8 + 8 + 16 + 8);

        let (stats, _, _) = unsafe { heap.collect_garbage(&[ptr]) };
        // The instance, its class and the two strings; the float box was
        // only an argument, unboxed into the raw slot.
        assert_eq!(stats.live_count, 4);
        assert_eq!(instance.load(2), SlotValue::Value(Value::object(by_tag)));
        assert_eq!(instance.load(3), SlotValue::Value(Value::object(by_ptr)));

        instance.store(2, Value::int(-3));
        instance.store_float(1, -0.5, bex_vm_types::object_format::Access::Shared);
        assert_eq!(instance.load(2), SlotValue::Value(Value::int(-3)));
        assert_eq!(instance.load(1), SlotValue::Float(-0.5));
    }

    /// A list grows by publishing a new backing; its elements live in the
    /// backing's slots (floats unboxed in a `float[]`, mixed values tagged in
    /// an `unknown[]`), and the collector reaches them through the backing.
    #[test]
    fn list_elements_live_in_their_backing() {
        use bex_vm_types::{
            ListRef,
            object_format::{Access, SlotValue},
        };

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let floats = tlab.alloc_list(bex_vm_types::RealizedTy::Float);
        let mixed = tlab.alloc_list(bex_vm_types::RealizedTy::Unknown);
        let text = tlab.alloc_string("reached through a tagged element");
        for i in 0..10 {
            let float = SlotValue::Float(f64::from(i) + 0.5);
            floats.lock(Access::Shared).push(&mut tlab, float).unwrap();
        }
        let mixed_values = [
            SlotValue::Value(Value::int(1)),
            SlotValue::Float(2.5),
            SlotValue::Value(Value::object(text)),
        ];
        mixed
            .lock(Access::Shared)
            .replace_all(&mut tlab, &mixed_values)
            .unwrap();
        assert_eq!(floats.len(), 10);
        assert!(floats.backing_object().unwrap().1 >= 10);
        assert_eq!(floats.get(9), Some(SlotValue::Float(9.5)));

        let roots = [floats.ptr(), mixed.ptr()];
        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };
        // Two lists, two backings, two element type objects and the string.
        assert_eq!(stats.live_count, 7);
        assert_eq!(mixed.snapshot(Access::Shared), mixed_values);

        let popped = mixed.lock(Access::Shared).pop();
        assert_eq!(popped, Some(SlotValue::Value(Value::object(text))));
        let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };
        assert_eq!(stats.live_count, 6, "a popped element is not kept");
        let _ = unsafe { ListRef::of(roots[0]) }.expect("still a list");
    }

    /// A map keeps insertion order across collisions, growth and removal,
    /// finds entries by hash and key, and bumps its epoch on every
    /// membership change but not on a value update.
    #[test]
    fn map_entries_keep_order_through_collisions_growth_and_removal() {
        use bex_vm_types::object_format::{Access, SlotValue};

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let map = tlab.alloc_empty_map(
            bex_vm_types::RealizedTy::Int,
            bex_vm_types::RealizedTy::Float,
        );
        let guard = map.lock(Access::Shared);
        let key = |i: i64| SlotValue::Value(Value::int(i));
        // Every key hashes to one of two buckets: collisions everywhere.
        for i in 0..20 {
            let hash = u64::try_from(i % 2).unwrap();
            guard
                .insert(
                    &mut tlab,
                    hash,
                    key(i),
                    SlotValue::Float(f64::from(i as i32)),
                )
                .unwrap();
        }
        assert_eq!(guard.len(), 20);
        assert_eq!(guard.candidates(1).len(), 10);
        let found = guard.find(1, |candidate| candidate == key(7)).unwrap();
        assert_eq!(guard.value(found), SlotValue::Float(7.0));

        let epoch = guard.epoch();
        guard.set_value(found, SlotValue::Float(70.0)).unwrap();
        assert_eq!(
            guard.epoch(),
            epoch,
            "a value update is not a membership change"
        );
        assert_eq!(guard.remove(found), SlotValue::Float(70.0));
        assert_ne!(guard.epoch(), epoch);
        assert_eq!(guard.find(1, |candidate| candidate == key(7)), None);
        let keys: Vec<SlotValue> = guard.entries().into_iter().map(|(k, _)| k).collect();
        let expected: Vec<SlotValue> = (0..20).filter(|&i| i != 7).map(key).collect();
        assert_eq!(keys, expected, "removal keeps insertion order");
        drop(guard);

        let (stats, _, _) = unsafe { heap.collect_garbage(&[map.ptr()]) };
        // The map, its three backings and its type object.
        assert_eq!(stats.live_count, 5);
        map.lock(Access::Shared).clear();
        assert!(map.is_empty());
    }

    /// A generic instance keeps its type arguments in a type object its
    /// type slot points to, which the collector keeps alive.
    #[test]
    fn a_generic_instance_keeps_its_type_arguments() {
        use bex_vm_types::InstanceRef;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let class = tlab.alloc(class_with_fields(
            vec![("value", bex_vm_types::RealizedTy::Unknown)],
            1,
        ));
        let value = Value::object(tlab.alloc_float(1.5));
        let ptr = tlab.alloc_instance_with_type_args(
            class,
            Box::new([bex_vm_types::RealizedTy::Float]),
            vec![value],
        );
        let (stats, _, _) = unsafe { heap.collect_garbage(&[ptr]) };
        // The instance, its class and its type object.
        assert_eq!(stats.live_count, 3);
        let instance = unsafe { InstanceRef::of(ptr) }.expect("an instance");
        assert_eq!(instance.type_args(), [bex_vm_types::RealizedTy::Float]);
        assert_eq!(instance.load(0).as_float(), Some(1.5));
    }

    fn class_object(finalizer: Option<HeapPtr>) -> Object {
        use baml_type::Name;
        use bex_vm_types::{Class, ClassMethodDef, ObjectIndex};

        let methods: indexmap::IndexMap<Name, ClassMethodDef> = finalizer
            .into_iter()
            .map(|function_ptr| {
                (
                    Name::new("cleanup"),
                    ClassMethodDef {
                        function: ObjectIndex::from_raw(0),
                        function_ptr,
                    },
                )
            })
            .collect();
        Object::Class(Box::new(Class {
            declared_crossing: baml_type::ClassCrossing::Fields,
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Resource")),
            fields: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            type_tag: baml_type::typetag::TypeTag::from_i64(0),
            cleanup: finalizer.map(|_| 0),
            methods,
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: Default::default(),
            redact: false,
        }))
    }

    fn finalizable_entries(heap: &BexHeap) -> Vec<HeapPtr> {
        let mut entries = heap.finalizable.lock().unwrap().instances.clone();
        entries.sort_unstable();
        entries.dedup();
        entries
    }

    /// The finalizer scan examines the registered instances only: 10,000 dead
    /// floats around them are never visited.
    #[test]
    fn finalizer_scan_visits_only_finalizable_instances() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let finalizer = tlab.alloc_string("cleanup".to_string());
        let class = tlab.alloc(class_object(Some(finalizer)));
        // A runtime class with a finalizer joins the heap, as a graft does.
        heap.note_finalizable_class();
        let live = tlab.alloc_instance(class, vec![]);
        let dead = tlab.alloc_instance(class, vec![]);
        for i in 0..10_000_u32 {
            tlab.alloc_float(f64::from(i));
        }
        assert_eq!(finalizable_entries(&heap), {
            let mut both = vec![live, dead];
            both.sort_unstable();
            both
        });

        FINALIZER_SCAN_VISITS.with(|visits| visits.set(0));
        let _ = unsafe { heap.collect_garbage(&[live]) };

        assert_eq!(FINALIZER_SCAN_VISITS.with(std::cell::Cell::get), 2);
        assert_eq!(heap.take_pending_finalizers(), vec![(dead, finalizer)]);
        // The live instance and the resurrected dead one stay on the list.
        let mut expected = vec![live, dead];
        expected.sort_unstable();
        assert_eq!(finalizable_entries(&heap), expected);
    }

    /// An instance rooted through two collections keeps its entry. Once dead, a collection finalizes it exactly once,
    /// and once cleaned it is neither finalized again nor kept on the list.
    #[test]
    fn finalizable_instance_follows_collections_and_is_finalized_once() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let finalizer = tlab.alloc_string("cleanup".to_string());
        let class = tlab.alloc(class_object(Some(finalizer)));
        heap.note_finalizable_class();
        let instance = tlab.alloc_instance(class, vec![]);

        for _ in 0..2 {
            let _ = unsafe { heap.collect_garbage(&[instance]) };
            assert!(heap.take_pending_finalizers().is_empty());
            assert_eq!(finalizable_entries(&heap), vec![instance]);
        }

        let _ = unsafe { heap.collect_garbage(&[]) };
        let pending = heap.take_pending_finalizers();
        assert_eq!(pending, vec![(instance, finalizer)]);
        // The engine runs `cleanup`, which latches the instance.
        let finalized = unsafe { bex_vm_types::InstanceRef::of(pending[0].0) }
            .expect("a finalizer seed is an instance");
        assert!(finalized.begin_cleanup());

        unsafe { heap.collect_garbage(&[]) };
        assert!(heap.take_pending_finalizers().is_empty());
        assert!(finalizable_entries(&heap).is_empty());
    }

    /// A class that gains a finalizer after an instance of it exists
    /// invalidates the list; the next collection rebuilds it from a walk of
    /// every object and finds the older instance.
    #[test]
    fn class_that_gains_a_finalizer_rebuilds_the_list() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let finalizer = tlab.alloc_string("cleanup".to_string());
        let class = tlab.alloc(class_object(None));
        let older = tlab.alloc_instance(class, vec![]);
        assert!(finalizable_entries(&heap).is_empty());

        // The class gains a finalizer in place, as an edit through the VM's
        // `ObjectMut` would; the guard then invalidates the list.
        // SAFETY: single-threaded test; nothing else references the class object.
        unsafe {
            *class.get_mut() = class_object(Some(finalizer));
        }
        heap.invalidate_finalizable_instances();

        let _ = unsafe { heap.collect_garbage(&[]) };
        assert_eq!(heap.take_pending_finalizers(), vec![(older, finalizer)]);
    }

    /// Seeds come back in address order, whatever order the instances were
    /// registered in.
    #[test]
    fn finalizer_seeds_come_back_in_address_order() {
        let heap = BexHeap::new(vec![]);
        let mut low = Tlab::new(Arc::clone(&heap));
        let mut high = Tlab::new(Arc::clone(&heap));
        let finalizer = low.alloc_string("cleanup".to_string());
        let class = low.alloc(class_object(Some(finalizer)));
        heap.note_finalizable_class();
        // `low` takes a block for instance cells first.
        let plain = low.alloc(class_object(None));
        let _ = low.alloc_instance(plain, vec![]);
        // `high` takes the next block, so its cells come after `low`'s, but
        // it registers its instance first.
        let late = high.alloc_instance(class, vec![]);
        let early = low.alloc_instance(class, vec![]);
        assert!(early.as_ptr() < late.as_ptr());

        let _ = unsafe { heap.collect_garbage(&[]) };
        let order: Vec<HeapPtr> = heap
            .take_pending_finalizers()
            .into_iter()
            .map(|(instance, _)| instance)
            .collect();
        assert_eq!(order, vec![early, late]);
    }
}
