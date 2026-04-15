//! Garbage collection for the unified heap.
//!
//! BEX uses a safepoint-based, generational copying collector:
//!
//! - **Safepoints**: GC only runs when all VMs are yielded (async operations)
//! - **Generational**: Four spaces — Gen0 (nursery), Gen1, Gen2, and inactive
//! - **Full collection**: Traces all three active generations, copies survivors
//!   to inactive, swaps inactive↔Gen2, clears Gen0 and Gen1
//! - **Compacting**: No fragmentation; all live objects are contiguous in Gen2
//! - **Handle-aware**: Handles updated to point to new object locations

use std::{cell::UnsafeCell, collections::HashMap};

use bex_vm_types::{HeapPtr, Object, Value};

use crate::{
    BexHeap,
    card_table::{CARD_SIZE, CARDS_PER_CHUNK},
    chunked_vec::ChunkedVec,
    heap::Generation,
};

/// Which generation level to collect.
///
/// - `Gen0`: Minor GC — traces Gen0 only, promotes survivors to Gen1.
/// - `Minor`: Traces Gen0 + Gen1; Gen0 survivors → new Gen1 (via inactive swap),
///   Gen1 survivors → Gen2.
/// - `Major`: Full GC — traces all generations, survivors → Gen2 via inactive swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionLevel {
    /// Minor collection: Gen0 + Gen1.
    Minor,
    /// Full collection: Gen0 + Gen1 + Gen2.
    Major,
}

/// Result of a garbage collection cycle.
#[derive(Debug, Clone)]
pub struct GcStats {
    /// Objects marked as live (copied).
    pub live_count: usize,
    /// Objects collected (not copied).
    pub collected_count: usize,
    /// Handles invalidated.
    pub handles_invalidated: usize,
    /// Which collection level was run.
    pub level: CollectionLevel,
    /// Objects promoted from Gen0 to Gen1 during this cycle.
    pub promoted_to_gen1: usize,
    /// Objects promoted from Gen1 to Gen2 during this cycle.
    pub promoted_to_gen2: usize,
}

impl BexHeap {
    /// Run a full garbage collection with the given roots.
    ///
    /// Traces all live objects reachable from `roots` across Gen0, Gen1, and Gen2,
    /// copies survivors to the inactive space, then atomically swaps inactive↔Gen2
    /// and clears Gen0 and Gen1. After collection all survivors reside in Gen2.
    ///
    /// # Safety
    ///
    /// Caller must ensure all VMs are at safepoints (not executing).
    /// This is typically guaranteed by the engine's epoch-based GC protocol.
    ///
    /// # Arguments
    ///
    /// * `roots` — Stack roots from all yielded VMs plus any externally-held handles.
    ///
    /// # Returns
    ///
    /// A tuple of `(GcStats, remapped_roots, forwarding_map)` where:
    /// - `GcStats` contains live/collected counts and invalidated handle count.
    /// - `remapped_roots` contains the new `HeapPtr` for each input root (in order).
    /// - `forwarding_map` maps every moved object's old `HeapPtr` to its new location.
    ///   Callers must use this map to update any stale `HeapPtr` values they hold
    ///   (e.g., parked-VM stacks, TLAB invalidation, continuation captures).
    pub unsafe fn collect_garbage(
        &self,
        roots: &[HeapPtr],
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        self.copy_collection(roots)
    }

    /// Full generational copy collection.
    ///
    /// Full generational copy collection — the core GC implementation.
    ///
    /// Traces all live objects reachable from `roots` across Gen0, Gen1, and Gen2,
    /// copies survivors into the inactive space, then atomically swaps inactive↔Gen2
    /// and clears Gen0 and Gen1. After collection all survivors reside in Gen2.
    ///
    /// Returns `(GcStats, remapped_roots, forwarding_map)`.
    fn copy_collection(
        &self,
        roots: &[HeapPtr],
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        // Track old -> new pointer mappings (forwarding pointers)
        let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();

        self.debug_verify_tlab_canaries();

        // Advance epoch before creating any new runtime pointers.
        self.bump_epoch();

        // Count objects across all active generations for stats.
        // SAFETY: GC runs at safepoints, no VMs are executing.
        let old_count =
            unsafe { self.gen0_ref().len() + self.gen1_ref().len() + self.gen2_ref().len() };

        // Clear inactive space — it becomes our copy destination.
        // SAFETY: GC runs at safepoints, exclusive access guaranteed.
        unsafe {
            self.inactive_mut().clear();
        }

        // BFS from roots — copy every reachable object into inactive.
        let mut worklist: Vec<HeapPtr> = roots.to_vec();

        while let Some(old_ptr) = worklist.pop() {
            // Skip already-forwarded objects.
            if forwarding.contains_key(&old_ptr) {
                continue;
            }

            // Compile-time objects are permanent — keep their pointer unchanged.
            if self.is_compile_time_ptr(old_ptr) {
                forwarding.insert(old_ptr, old_ptr);
                continue;
            }

            // Copy this object to the inactive space.
            let new_ptr = self.copy_object_to_inactive(old_ptr, &mut forwarding);

            // Enqueue this object's outgoing heap references.
            // SAFETY: We just wrote the object into inactive, pointer is valid.
            let obj = unsafe { new_ptr.get() };
            self.add_references_to_worklist(obj, &mut worklist);
        }

        // Patch all intra-heap pointers in the inactive space to their new locations.
        // SAFETY: All live objects have been copied; no VMs are executing.
        unsafe {
            self.fixup_references_in_inactive(&forwarding);
        }

        // SAFETY: GC runs at safepoints.
        let live_count = unsafe { self.inactive_ref().len() };
        let collected_count = old_count.saturating_sub(live_count);

        // Swap inactive ↔ Gen2; clear Gen0 and Gen1.
        // After the swap: survivors are in Gen2, old-space debris is in inactive.
        // SAFETY: GC safepoint; exclusive access to all four spaces.
        unsafe {
            std::ptr::swap(self.gen2.get(), self.inactive.get());
            self.gen0_mut().clear();
            self.gen1_mut().clear();
        }

        // Gen0 is now empty — reset the TLAB cursor to 0.
        self.reset_next_chunk(0);
        self.clear_tlab_canaries();

        // Poison or clear the inactive space (now holds old-space debris).
        self.finalize_inactive_space();

        // Remap each root to its new location (or keep it if it was compile-time).
        let remapped_roots: Vec<HeapPtr> = roots
            .iter()
            .map(|old_ptr| *forwarding.get(old_ptr).unwrap_or(old_ptr))
            .collect();

        // Update the handle table so external handles point to new locations.
        let handles_invalidated = self.update_handles(&forwarding);

        let stats = GcStats {
            live_count,
            collected_count,
            handles_invalidated,
            level: CollectionLevel::Major,
            promoted_to_gen1: 0,
            promoted_to_gen2: live_count,
        };

        (stats, remapped_roots, forwarding)
    }

    /// Copy a single object from an active generation into the inactive space.
    /// Returns the new HeapPtr in the inactive space.
    fn copy_object_to_inactive(
        &self,
        old_ptr: HeapPtr,
        forwarding: &mut HashMap<HeapPtr, HeapPtr>,
    ) -> HeapPtr {
        // Clone the object from its old location.
        // SAFETY: GC runs at safepoints, no VMs are executing.
        let obj = unsafe { old_ptr.get().clone() };

        // Append to the inactive space and get a pointer to the new location.
        // SAFETY: GC runs at safepoints, no VMs are executing.
        let new_ptr = unsafe {
            let inactive = self.inactive_mut();
            let new_runtime_idx = inactive.len();
            inactive.push_with(obj, || Object::String(String::new()));
            let raw_ptr = inactive.get_ptr(new_runtime_idx);
            self.make_heap_ptr(raw_ptr)
        };

        // Record forwarding pointer
        forwarding.insert(old_ptr, new_ptr);

        new_ptr
    }

    /// Add object references to the worklist for tracing.
    fn add_references_to_worklist(&self, obj: &Object, worklist: &mut Vec<HeapPtr>) {
        match obj {
            Object::Array(arr) => {
                for value in arr {
                    if let Value::Object(ptr) = value {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Map(map) => {
                for value in map.values() {
                    if let Value::Object(ptr) = value {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Instance(inst) => {
                worklist.push(inst.class);
                for value in &inst.fields {
                    if let Value::Object(ptr) = value {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Closure(closure) => {
                worklist.push(closure.function);
                for value in &closure.captures {
                    if let Value::Object(ptr) = value {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::BoundMethod(bm) => {
                worklist.push(bm.function);
                if let Value::Object(ptr) = &bm.receiver {
                    worklist.push(*ptr);
                }
            }
            Object::Cell(cell) => {
                if let Value::Object(ptr) = &cell.value {
                    worklist.push(*ptr);
                }
            }
            Object::Variant(var) => {
                worklist.push(var.enm);
            }
            Object::Future(fut) => {
                use bex_vm_types::Future;
                match fut {
                    Future::Pending(pending) => {
                        for value in &pending.args {
                            if let Value::Object(ptr) = value {
                                worklist.push(*ptr);
                            }
                        }
                    }
                    Future::Ready(value) => {
                        if let Value::Object(ptr) = value {
                            worklist.push(*ptr);
                        }
                    }
                }
            }
            // Primitives have no references
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
            Object::String(_)
            | Object::Uint8Array(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Function(_)
            | Object::RustData(_)
            | Object::Collector(_)
            | Object::Type(_) => {}
        }
    }

    /// Fix up all object references in the inactive space to use forwarded addresses.
    ///
    /// # Safety
    /// Must be called after all live objects have been copied to inactive.
    unsafe fn fixup_references_in_inactive(&self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        // SAFETY: All live objects have been copied to inactive, and no VMs are executing.
        unsafe {
            let inactive = self.inactive_mut();
            for obj in inactive.iter_mut() {
                self.fixup_object_references(obj, forwarding);
            }
        }
    }

    /// Fix up references within a single object.
    fn fixup_object_references(&self, obj: &mut Object, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        match obj {
            Object::Array(arr) => {
                for value in arr.iter_mut() {
                    self.fixup_value(value, forwarding);
                }
            }
            Object::Map(map) => {
                for value in map.values_mut() {
                    self.fixup_value(value, forwarding);
                }
            }
            Object::Instance(inst) => {
                // Update class pointer
                if let Some(&new_ptr) = forwarding.get(&inst.class) {
                    inst.class = new_ptr;
                }
                for value in &mut inst.fields {
                    self.fixup_value(value, forwarding);
                }
            }
            Object::Closure(closure) => {
                if let Some(&new_ptr) = forwarding.get(&closure.function) {
                    closure.function = new_ptr;
                }
                for value in &mut closure.captures {
                    self.fixup_value(value, forwarding);
                }
            }
            Object::BoundMethod(bm) => {
                if let Some(&new_ptr) = forwarding.get(&bm.function) {
                    bm.function = new_ptr;
                }
                self.fixup_value(&mut bm.receiver, forwarding);
            }
            Object::Cell(cell) => {
                self.fixup_value(&mut cell.value, forwarding);
            }
            Object::Variant(var) => {
                // Update enum pointer
                if let Some(&new_ptr) = forwarding.get(&var.enm) {
                    var.enm = new_ptr;
                }
            }
            Object::Future(fut) => {
                use bex_vm_types::Future;
                match fut {
                    Future::Pending(pending) => {
                        for value in &mut pending.args {
                            self.fixup_value(value, forwarding);
                        }
                    }
                    Future::Ready(value) => {
                        self.fixup_value(value, forwarding);
                    }
                }
            }
            // Primitives have no references
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
            Object::String(_)
            | Object::Uint8Array(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Function(_)
            | Object::RustData(_)
            | Object::Collector(_)
            | Object::Type(_) => {}
        }
    }

    /// Fix up a single Value reference.
    fn fixup_value(&self, value: &mut Value, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Value::Object(ptr) = value
            && let Some(&new_ptr) = forwarding.get(ptr)
        {
            *ptr = new_ptr;
        }
    }

    /// Fix up all object references in an arbitrary space.
    ///
    /// Like `fixup_references_in_inactive`, but works on any `ChunkedVec`.
    ///
    /// # Safety
    ///
    /// Must be called after all live objects in this space have been copied, and no
    /// VMs are executing.
    unsafe fn fixup_references_in_space(
        &self,
        space: &UnsafeCell<ChunkedVec<Object>>,
        forwarding: &HashMap<HeapPtr, HeapPtr>,
    ) {
        // SAFETY: Caller ensures exclusive access at a GC safepoint.
        unsafe {
            let vec = &mut *space.get();
            for obj in vec.iter_mut() {
                self.fixup_object_references(obj, forwarding);
            }
        }
    }

    /// Fix up only the newly promoted tail of a space.
    ///
    /// After promoting objects from a younger generation, only the newly appended
    /// objects need reference fixup (existing objects in the space already have
    /// correct pointers, or will be handled by a full space fixup).
    ///
    /// `len_before` is the length of the space before any promotion.
    ///
    /// # Safety
    ///
    /// Must be called at a GC safepoint.
    unsafe fn fixup_promoted_objects_from(
        &self,
        space: &UnsafeCell<ChunkedVec<Object>>,
        len_before: usize,
        forwarding: &HashMap<HeapPtr, HeapPtr>,
    ) {
        // SAFETY: Caller ensures exclusive access at a GC safepoint.
        unsafe {
            let vec = &mut *space.get();
            let len_after = vec.len();
            for i in len_before..len_after {
                let obj = vec.get_mut(i);
                self.fixup_object_references(obj, forwarding);
            }
        }
    }

    /// Copy a single object into an arbitrary destination space.
    ///
    /// Generalised version of `copy_object_to_inactive` that can target any
    /// `ChunkedVec` (Gen1, Gen2, or inactive).
    ///
    /// Returns the new `HeapPtr` in the destination space.
    fn copy_object_to_space(
        &self,
        space: &UnsafeCell<ChunkedVec<Object>>,
        old_ptr: HeapPtr,
        forwarding: &mut HashMap<HeapPtr, HeapPtr>,
    ) -> HeapPtr {
        // Clone the object from its old location.
        // SAFETY: GC runs at safepoints, no VMs are executing.
        let obj = unsafe { old_ptr.get().clone() };

        // Append to the destination space and return a pointer to the new location.
        // SAFETY: GC runs at safepoints, no VMs are executing.
        let new_ptr = unsafe {
            let vec = &mut *space.get();
            let new_idx = vec.len();
            vec.push_with(obj, || Object::String(String::new()));
            let raw_ptr = vec.get_ptr(new_idx);
            self.make_heap_ptr(raw_ptr)
        };

        forwarding.insert(old_ptr, new_ptr);
        new_ptr
    }

    /// Scan dirty cards in `space`/`card_table` and push any references to
    /// objects in `target_generations` onto `worklist`.
    ///
    /// # Safety
    ///
    /// Must be called at a GC safepoint.
    unsafe fn scan_dirty_cards_for_roots(
        &self,
        card_table: &crate::card_table::CardTable,
        space: &ChunkedVec<Object>,
        worklist: &mut Vec<HeapPtr>,
        target_generations: &[Generation],
    ) {
        for card_index in card_table.dirty_card_indices() {
            let chunk_idx = card_index / CARDS_PER_CHUNK;
            let card_offset_in_chunk = (card_index % CARDS_PER_CHUNK) * CARD_SIZE;

            let start = chunk_idx * ChunkedVec::<Object>::CHUNK_SIZE + card_offset_in_chunk;
            let end = (start + CARD_SIZE).min(space.len());

            for i in start..end {
                let obj = space.get(i);
                self.collect_references_in_generations(obj, worklist, target_generations);
            }
        }
    }

    /// Like `add_references_to_worklist`, but only enqueues pointers whose
    /// generation is one of `target_generations`.
    fn collect_references_in_generations(
        &self,
        obj: &Object,
        worklist: &mut Vec<HeapPtr>,
        target_generations: &[Generation],
    ) {
        match obj {
            Object::Array(arr) => {
                for value in arr {
                    if let Value::Object(ptr) = value
                        && target_generations.contains(&self.generation_of(*ptr))
                    {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Map(map) => {
                for value in map.values() {
                    if let Value::Object(ptr) = value
                        && target_generations.contains(&self.generation_of(*ptr))
                    {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Instance(inst) => {
                if target_generations.contains(&self.generation_of(inst.class)) {
                    worklist.push(inst.class);
                }
                for value in &inst.fields {
                    if let Value::Object(ptr) = value
                        && target_generations.contains(&self.generation_of(*ptr))
                    {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Closure(closure) => {
                if target_generations.contains(&self.generation_of(closure.function)) {
                    worklist.push(closure.function);
                }
                for value in &closure.captures {
                    if let Value::Object(ptr) = value
                        && target_generations.contains(&self.generation_of(*ptr))
                    {
                        worklist.push(*ptr);
                    }
                }
            }
            Object::Cell(cell) => {
                if let Value::Object(ptr) = &cell.value
                    && target_generations.contains(&self.generation_of(*ptr))
                {
                    worklist.push(*ptr);
                }
            }
            Object::Variant(var) => {
                if target_generations.contains(&self.generation_of(var.enm)) {
                    worklist.push(var.enm);
                }
            }
            Object::Future(fut) => {
                use bex_vm_types::Future;
                match fut {
                    Future::Pending(pending) => {
                        for value in &pending.args {
                            if let Value::Object(ptr) = value
                                && target_generations.contains(&self.generation_of(*ptr))
                            {
                                worklist.push(*ptr);
                            }
                        }
                    }
                    Future::Ready(value) => {
                        if let Value::Object(ptr) = value
                            && target_generations.contains(&self.generation_of(*ptr))
                        {
                            worklist.push(*ptr);
                        }
                    }
                }
            }
            // Primitives/leaf variants have no heap references.
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
            Object::String(_)
            | Object::Uint8Array(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Function(_)
            | Object::RustData(_)
            | Object::Collector(_)
            | Object::Type(_) => {}
        }
    }

    // -------------------------------------------------------------------------
    // Generational collection implementations
    // -------------------------------------------------------------------------

    /// Minor collection (Gen0 + Gen1).
    ///
    /// Traces Gen0 + Gen1 with roots from `roots` plus dirty-card references
    /// from Gen2. Gen0 survivors are copied to the inactive space (which becomes
    /// the new Gen1 after a swap). Gen1 survivors are promoted directly to Gen2.
    /// Gen0 is cleared after collection.
    ///
    /// Returns `(GcStats, remapped_roots, forwarding_map)`.
    unsafe fn collect_garbage_minor(
        &self,
        roots: &[HeapPtr],
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();

        self.bump_epoch();

        let gen0_count = unsafe { self.gen0_ref().len() };
        let gen1_count = unsafe { self.gen1_ref().len() };

        // Record Gen2 length before any promotion so we can fix up only new objects.
        let gen2_len_before = unsafe { self.gen2_ref().len() };

        // Clear inactive — it becomes the new Gen1 after the swap.
        // SAFETY: GC safepoint.
        unsafe {
            self.inactive_mut().clear();
        }

        // Build worklist: roots + cross-generation references from Gen2 dirty cards.
        let mut worklist: Vec<HeapPtr> = roots.to_vec();
        unsafe {
            self.scan_dirty_cards_for_roots(
                &*self.gen2_cards.get(),
                self.gen2_ref(),
                &mut worklist,
                &[Generation::Gen0, Generation::Gen1],
            );
        }

        let mut promoted_to_gen2 = 0usize;

        while let Some(old_ptr) = worklist.pop() {
            if forwarding.contains_key(&old_ptr) {
                continue;
            }

            let generation = self.generation_of(old_ptr);
            match generation {
                Generation::CompileTime => {
                    forwarding.insert(old_ptr, old_ptr);
                }
                Generation::Gen0 => {
                    // Gen0 survivors → new Gen1 (inactive).
                    let new_ptr =
                        self.copy_object_to_space(&self.inactive, old_ptr, &mut forwarding);
                    let obj = unsafe { new_ptr.get() };
                    self.add_references_to_worklist(obj, &mut worklist);
                }
                Generation::Gen1 => {
                    // Gen1 survivors → promote to Gen2.
                    let new_ptr = self.copy_object_to_space(&self.gen2, old_ptr, &mut forwarding);
                    let obj = unsafe { new_ptr.get() };
                    self.add_references_to_worklist(obj, &mut worklist);
                    promoted_to_gen2 += 1;
                }
                Generation::Gen2 => {
                    // Outside collected generations — identity-map.
                    forwarding.insert(old_ptr, old_ptr);
                }
            }
        }

        // Fix up references:
        // - Full fixup for inactive (new Gen1 — all objects are freshly copied).
        // - Tail fixup for Gen2 (only the newly promoted tail needs updating).
        // SAFETY: All live objects moved; no VMs executing.
        unsafe {
            self.fixup_references_in_space(&self.inactive, &forwarding);
            self.fixup_promoted_objects_from(&self.gen2, gen2_len_before, &forwarding);
        }

        // Swap inactive ↔ Gen1; clear Gen0.
        // SAFETY: GC safepoint; exclusive access to all spaces.
        unsafe {
            std::ptr::swap(self.gen1.get(), self.inactive.get());
            self.gen0_mut().clear();
        }
        self.reset_next_chunk(0);
        self.clear_tlab_canaries();

        // Clear all card tables.
        unsafe {
            (*self.gen2_cards.get()).clear();
        }

        // Poison/clear the old Gen1 (now in inactive).
        self.finalize_inactive_space();

        let new_gen1_count = unsafe { self.gen1_ref().len() };
        let total_live = new_gen1_count + promoted_to_gen2;
        let total_before = gen0_count + gen1_count;

        let remapped_roots = roots
            .iter()
            .map(|old_ptr| *forwarding.get(old_ptr).unwrap_or(old_ptr))
            .collect();

        let handles_invalidated = self.update_handles(&forwarding);

        let stats = GcStats {
            live_count: total_live,
            collected_count: total_before.saturating_sub(total_live),
            handles_invalidated,
            level: CollectionLevel::Minor,
            promoted_to_gen1: new_gen1_count,
            promoted_to_gen2,
        };

        (stats, remapped_roots, forwarding)
    }

    /// Dispatch to the appropriate generational collection algorithm.
    ///
    /// - `Minor`: Gen0 + Gen1 traced; Gen0 survivors → new Gen1, Gen1 survivors → Gen2.
    /// - `Major`: Full GC — equivalent to [`collect_garbage`], all generations traced.
    ///
    /// # Safety
    ///
    /// Caller must ensure all VMs are at safepoints (not executing).
    pub unsafe fn collect_garbage_generational(
        &self,
        roots: &[HeapPtr],
        level: CollectionLevel,
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        match level {
            CollectionLevel::Minor => unsafe { self.collect_garbage_minor(roots) },
            CollectionLevel::Major => unsafe { self.collect_garbage(roots) },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bex_vm_types::{Object, Value};

    use super::*;
    use crate::Tlab;

    #[test]
    fn test_gc_empty_heap() {
        let heap = BexHeap::new(vec![]);

        // Run GC with no roots
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[]) };

        assert_eq!(stats.live_count, 0);
        assert_eq!(stats.collected_count, 0);
        assert_eq!(stats.handles_invalidated, 0);
        assert!(remapped.is_empty());
    }

    #[test]
    fn test_gc_preserves_compile_time_objects() {
        let compile_time: Vec<Object> = vec![
            Object::String("builtin1".to_string()),
            Object::String("builtin2".to_string()),
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
            name: baml_type::TypeName::local(baml_type::Name::new("E")),
            variants: vec![bex_vm_types::EnumVariant {
                name: "A".to_string(),
                description: None,
                alias: None,
                skip: false,
            }],
            description: None,
            alias: None,
            ty_attr: baml_type::TyAttr::default(),
        }))];
        let debug = HeapDebuggerConfig {
            enabled: true,
            verify: HeapVerifyMode::Full,
        };
        let heap = BexHeap::with_tlab_size_and_debug(compile_time, 4, debug);
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

    #[cfg(feature = "heap_debug")]
    #[test]
    fn test_full_verify_panics_on_instance_field_mismatch() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        use crate::{HeapDebuggerConfig, heap_debugger::HeapVerifyMode};

        let compile_time = vec![Object::Class(Box::new(bex_vm_types::Class {
            name: baml_type::TypeName::local(baml_type::Name::new("C")),
            fields: vec![bex_vm_types::ClassField {
                name: "x".to_string(),
                field_type: baml_type::Ty::Int {
                    attr: baml_type::TyAttr::default(),
                },
                description: None,
                alias: None,
                skip: false,
            }],
            description: None,
            alias: None,
            type_tag: 100,
            ty_attr: baml_type::TyAttr::default(),
        }))];
        let debug = HeapDebuggerConfig {
            enabled: true,
            verify: HeapVerifyMode::Full,
        };
        let heap = BexHeap::with_tlab_size_and_debug(compile_time, 4, debug);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let class_ptr = heap.compile_time_ptr(0);
        // Instance has 3 fields but class expects 1 — should panic on verify
        let _bad_instance = tlab.alloc(Object::Instance(bex_vm_types::types::Instance {
            class: class_ptr,
            fields: vec![Value::Int(1), Value::Int(2), Value::Int(3)],
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
        let arr = tlab.alloc_array(vec![Value::Object(str_obj)]);

        // Allocate another unreferenced string
        let _unreferenced = tlab.alloc_string("unreferenced".to_string());

        // Run GC with only the array as root
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[arr]) };

        // Should copy both the array and the string it references
        assert_eq!(stats.live_count, 2);
        assert_eq!(remapped.len(), 1);

        // Verify the array's reference was updated
        let new_arr_ptr = remapped[0];
        let arr_obj = unsafe { new_arr_ptr.get() };
        if let Object::Array(elements) = arr_obj {
            // The string reference should have been updated
            if let Value::Object(str_ptr) = &elements[0] {
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

    #[test]
    fn test_gc_space_swap() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate an object in Gen0
        let obj = tlab.alloc_string("test".to_string());

        // After GC, the survivor should be in Gen2 (inactive was swapped to Gen2).
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj]) };

        assert_eq!(stats.live_count, 1);
        assert_eq!(remapped.len(), 1);

        // Verify Gen0 is empty and Gen2 has the survivor
        let (gen0_len, gen2_len) = unsafe { (heap.gen0_ref().len(), heap.gen2_ref().len()) };
        assert_eq!(gen0_len, 0, "Gen0 should be empty after full GC");
        assert_eq!(gen2_len, 1, "Gen2 should contain the survivor");
    }

    #[test]
    fn test_gc_invalidates_dead_handles() {
        use bex_external_types::WeakHeapRef;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate an object and create a handle
        let obj = tlab.alloc_string("test".to_string());
        let handle = heap.create_handle(obj);

        // Verify handle is valid
        assert!(heap.resolve_handle_ptr(handle.slab_key()).is_some());

        // Run GC with no roots - object should be collected, handle invalidated
        let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };

        assert_eq!(stats.handles_invalidated, 1);
        assert!(heap.resolve_handle_ptr(handle.slab_key()).is_none());
    }

    #[test]
    fn test_gc_heuristics() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Initially should not need GC
        assert!(!heap.should_gc());

        // Allocate many objects to trigger GC threshold
        for i in 0..15_000 {
            tlab.alloc_string(format!("obj{i}"));
        }

        // Should now recommend GC
        assert!(heap.should_gc());

        // Reset counter
        heap.reset_gc_counter();
        assert!(!heap.should_gc());
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
            Object::String("builtin1".to_string()),
            Object::String("builtin2".to_string()),
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
    fn test_gc_with_map_references() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate a string
        let str_obj = tlab.alloc_string("value".to_string());

        // Allocate a map that references the string
        let mut map = indexmap::IndexMap::new();
        map.insert("key".to_string(), Value::Object(str_obj));
        let map_obj = tlab.alloc_map(map);

        // Allocate unreferenced garbage
        let _garbage = tlab.alloc_string("garbage".to_string());

        // Run GC with only the map as root
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[map_obj]) };

        // Both map and string should survive
        assert_eq!(stats.live_count, 2);
        assert_eq!(remapped.len(), 1);

        // Verify the map's reference was updated correctly
        let new_map_ptr = remapped[0];
        let map_result = unsafe { new_map_ptr.get() };
        if let Object::Map(m) = map_result {
            if let Some(Value::Object(str_ptr)) = m.get("key") {
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

        simulated_stack.push(Value::Object(obj1));
        simulated_stack.push(Value::Int(42)); // Non-object value
        simulated_stack.push(Value::Object(obj2));
        simulated_stack.push(Value::Null);
        simulated_stack.push(Value::Object(obj3));

        // Also allocate some garbage that won't be rooted
        let _garbage1 = tlab.alloc_string("garbage1".to_string());
        let _garbage2 = tlab.alloc_string("garbage2".to_string());

        // Collect roots from the simulated stack (like collect_vm_roots does)
        let roots: Vec<HeapPtr> = simulated_stack
            .iter()
            .filter_map(|v| match v {
                Value::Object(ptr) => Some(*ptr),
                _ => None,
            })
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
            if let Value::Object(ptr) = value
                && let Some(&new_ptr) = forwarding.get(ptr)
            {
                *ptr = new_ptr;
            }
        }

        // Verify all stack values are still accessible and correct
        for value in &simulated_stack {
            match value {
                Value::Object(ptr) => {
                    let obj = unsafe { ptr.get() };
                    match obj {
                        Object::String(s) => {
                            assert!(s.starts_with("stack_value_"));
                        }
                        _ => panic!("Expected String object"),
                    }
                }
                Value::Int(n) => assert_eq!(*n, 42),
                Value::Null => {}
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

        let inner_array = tlab.alloc_array(vec![Value::Object(leaf_str)]);

        let mut map = indexmap::IndexMap::new();
        map.insert("nested".to_string(), Value::Object(inner_array));
        let middle_map = tlab.alloc_map(map);

        let outer_array = tlab.alloc_array(vec![Value::Object(middle_map)]);

        // Allocate garbage between the chain objects
        let _g1 = tlab.alloc_string("garbage".to_string());
        let _g2 = tlab.alloc_string("more_garbage".to_string());

        // Only root the outer array
        let (stats, remapped, _forwarding) = unsafe { heap.collect_garbage(&[outer_array]) };

        // All 4 objects in the chain should survive
        assert_eq!(stats.live_count, 4);
        assert!(stats.collected_count >= 2);

        // Verify the chain is intact after forwarding
        let new_outer = remapped[0];
        let outer_obj = unsafe { new_outer.get() };

        if let Object::Array(arr) = outer_obj
            && let Value::Object(map_ptr) = &arr[0]
        {
            let map_obj = unsafe { map_ptr.get() };
            if let Object::Map(m) = map_obj
                && let Some(Value::Object(inner_arr_ptr)) = m.get("nested")
            {
                let inner_arr_obj = unsafe { inner_arr_ptr.get() };
                if let Object::Array(inner_arr) = inner_arr_obj
                    && let Value::Object(str_ptr) = &inner_arr[0]
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

    /// Tests that survivors move to Gen2 and Gen0 is cleared after GC cycles.
    #[test]
    fn test_miri_active_space_swap() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("object1".to_string());
        let obj2 = tlab.alloc_string("object2".to_string());

        // First GC: survivors from Gen0 move to Gen2 via inactive swap.
        let (stats, remapped, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };

        assert_eq!(stats.live_count, 2);

        // Gen0 should be empty, Gen2 should have 2 survivors.
        unsafe {
            assert_eq!(heap.gen0_ref().len(), 0);
            assert_eq!(heap.gen2_ref().len(), 2);
        }

        // Verify objects accessible
        for ptr in &remapped {
            assert!(matches!(unsafe { ptr.get() }, Object::String(_)));
        }

        // Second GC: survivors from Gen2 cycle through inactive again.
        let (stats2, remapped2, _) = unsafe { heap.collect_garbage(&remapped) };

        assert_eq!(stats2.live_count, 2);

        // Gen0 still empty, Gen2 still has 2 survivors.
        unsafe {
            assert_eq!(heap.gen0_ref().len(), 0);
            assert_eq!(heap.gen2_ref().len(), 2);
        }

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
        let (stats, _, forwarding) = unsafe { heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 2);
        assert!(stats.collected_count > 0);

        // Handles updated to new locations
        let new1_ptr = heap.resolve_handle_ptr(handle1.slab_key()).unwrap();
        let new2_ptr = heap.resolve_handle_ptr(handle2.slab_key()).unwrap();

        if let Some(&expected) = forwarding.get(&obj1) {
            assert_eq!(new1_ptr, expected);
        }
        if let Some(&expected) = forwarding.get(&obj2) {
            assert_eq!(new2_ptr, expected);
        }

        // Objects accessible through updated handles
        assert!(matches!(unsafe { new1_ptr.get() }, Object::String(s) if s == "handle_obj_1"));
    }

    // ========================================================================
    // Phase 2: Generational heap infrastructure tests
    // ========================================================================

    /// TLAB allocations land in Gen0; after a full GC all survivors are in Gen2.
    #[test]
    fn test_generation_of_tlab_allocates_into_gen0() {
        use crate::heap::Generation;

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Fresh allocations should be in Gen0 (the nursery).
        let obj1 = tlab.alloc_string("nursery_1".to_string());
        let obj2 = tlab.alloc_string("nursery_2".to_string());

        assert_eq!(
            heap.generation_of(obj1),
            Generation::Gen0,
            "fresh allocation should be in Gen0"
        );
        assert_eq!(
            heap.generation_of(obj2),
            Generation::Gen0,
            "fresh allocation should be in Gen0"
        );
    }

    /// After a full GC all survivors are promoted to Gen2.
    #[test]
    fn test_generation_of_survivors_in_gen2_after_gc() {
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

        // Survivors should now be in Gen2 (inactive was swapped into Gen2).
        for ptr in &remapped {
            assert_eq!(
                heap.generation_of(*ptr),
                Generation::Gen2,
                "survivors should be in Gen2 after full GC"
            );
        }

        // Gen0 should be empty.
        let gen0_len = unsafe { heap.gen0_ref().len() };
        assert_eq!(gen0_len, 0, "Gen0 should be empty after full GC");

        // Invalidate the TLAB so it refills from the now-empty Gen0.
        // (In the engine this is done automatically; in unit tests we do it manually.)
        tlab.invalidate();

        // New allocations after GC still go to Gen0.
        let post_gc = tlab.alloc_string("post_gc".to_string());
        assert_eq!(
            heap.generation_of(post_gc),
            Generation::Gen0,
            "allocations after GC should still land in Gen0"
        );
    }

    /// Compile-time objects have Generation::CompileTime.
    #[test]
    fn test_generation_of_compile_time() {
        use crate::heap::Generation;

        let heap = BexHeap::new(vec![Object::String("builtin".to_string())]);
        let ct_ptr = heap.compile_time_ptr(0);
        assert_eq!(heap.generation_of(ct_ptr), Generation::CompileTime);
    }
}
