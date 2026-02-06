//! Garbage collection for the unified heap.
//!
//! BEX uses a safepoint-based, semi-space copying collector:
//!
//! - **Safepoints**: GC only runs when all VMs are yielded (async operations)
//! - **Semi-space**: Live objects are copied from active to inactive space
//! - **Compacting**: No fragmentation, all live objects are contiguous
//! - **Handle-aware**: Handles updated to point to new object locations

use std::{collections::HashMap, sync::atomic::Ordering};

use bex_vm_types::{HeapPtr, Object, Value};

use crate::BexHeap;

/// Result of a garbage collection cycle.
#[derive(Debug, Clone)]
pub struct GcStats {
    /// Objects marked as live (copied).
    pub live_count: usize,
    /// Objects collected (not copied).
    pub collected_count: usize,
    /// Handles invalidated.
    pub handles_invalidated: usize,
}

impl BexHeap {
    /// Run garbage collection with the given roots.
    ///
    /// # Safety
    ///
    /// Caller must ensure all VMs are at safepoints (not executing).
    /// This is typically guaranteed by the engine's event loop.
    ///
    /// # Arguments
    ///
    /// * `roots` - Stack roots from all yielded VMs, plus any externally-held handles
    ///
    /// # Returns
    ///
    /// A tuple of (GcStats, remapped_roots) where remapped_roots contains the
    /// new HeapPtr for each root after objects have been copied to the new space.
    pub unsafe fn collect_garbage(&self, roots: &[HeapPtr]) -> (GcStats, Vec<HeapPtr>) {
        self.copy_collection(roots)
    }

    /// Run garbage collection with the given roots, returning the forwarding map.
    ///
    /// # Safety
    ///
    /// Caller must ensure all VMs are at safepoints (not executing).
    ///
    /// # Returns
    ///
    /// A tuple of (GcStats, remapped_roots, forwarding_map) where:
    /// - remapped_roots contains the new HeapPtr for each root
    /// - forwarding_map maps old HeapPtr to new HeapPtr for all moved objects
    pub unsafe fn collect_garbage_with_forwarding(
        &self,
        roots: &[HeapPtr],
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        self.copy_collection_with_forwarding(roots)
    }

    /// Semi-space copy collection.
    ///
    /// Copies all live objects reachable from roots to the inactive space,
    /// then swaps spaces. This frees all unreachable objects in one sweep.
    fn copy_collection(&self, roots: &[HeapPtr]) -> (GcStats, Vec<HeapPtr>) {
        // Track old -> new pointer mappings (forwarding pointers)
        let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();

        // Get current and next space indices
        let from_space = self.active_space_index();
        let to_space = 1 - from_space;

        self.debug_verify_tlab_canaries();

        // Advance epoch before creating any new runtime pointers.
        self.bump_epoch();

        // Get the old space size for stats calculation
        // SAFETY: GC runs at safepoints, no VMs are executing
        let old_space_size = unsafe { self.space_ref(from_space).len() };

        // Clear the target space and prepare for copying
        // SAFETY: GC runs at safepoints, no VMs are executing
        unsafe {
            self.space_mut(to_space).clear();
        }

        // Worklist for BFS traversal: HeapPtr values
        let mut worklist: Vec<HeapPtr> = roots.to_vec();

        // Process all reachable objects
        while let Some(old_ptr) = worklist.pop() {
            // Skip already forwarded objects
            if forwarding.contains_key(&old_ptr) {
                continue;
            }

            // Skip compile-time objects (they stay in place, don't need copying)
            if self.is_compile_time_ptr(old_ptr) {
                // Compile-time objects keep their pointer
                forwarding.insert(old_ptr, old_ptr);
                continue;
            }

            // Copy this object to the new space
            let new_ptr = self.copy_object_to_new_space(old_ptr, to_space, &mut forwarding);

            // Add this object's references to the worklist
            // SAFETY: We just copied the object, so it exists in to_space
            let obj = unsafe { new_ptr.get() };
            self.add_references_to_worklist(obj, &mut worklist);
        }

        // Now fix up all references in the copied objects
        unsafe {
            self.fixup_references(to_space, &forwarding);
        }

        // Calculate stats before swapping
        // SAFETY: GC runs at safepoints
        let live_count = unsafe { self.space_ref(to_space).len() };
        let collected_count = old_space_size.saturating_sub(live_count);

        // Swap spaces: make to_space the new active space
        self.active_space.store(to_space, Ordering::Release);

        // Reset TLAB allocation pointer to end of new space
        self.reset_next_chunk(live_count);
        self.clear_tlab_canaries();

        // Clear or poison the old (now inactive) space
        self.finalize_from_space(from_space);

        // Remap roots to their new locations
        let remapped_roots: Vec<HeapPtr> = roots
            .iter()
            .map(|old_ptr| *forwarding.get(old_ptr).unwrap_or(old_ptr))
            .collect();

        // Update handle table entries to point to new object locations
        let handles_invalidated = self.update_handles(&forwarding);

        let stats = GcStats {
            live_count,
            collected_count,
            handles_invalidated,
        };

        (stats, remapped_roots)
    }

    /// Semi-space copy collection, returning forwarding map for external use.
    fn copy_collection_with_forwarding(
        &self,
        roots: &[HeapPtr],
    ) -> (GcStats, Vec<HeapPtr>, HashMap<HeapPtr, HeapPtr>) {
        // Track old -> new pointer mappings (forwarding pointers)
        let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();

        // Get current and next space indices
        let from_space = self.active_space_index();
        let to_space = 1 - from_space;

        self.debug_verify_tlab_canaries();

        // Advance epoch before creating any new runtime pointers.
        self.bump_epoch();

        // Get the old space size for stats calculation
        // SAFETY: GC runs at safepoints, no VMs are executing
        let old_space_size = unsafe { self.space_ref(from_space).len() };

        // Clear the target space and prepare for copying
        // SAFETY: GC runs at safepoints, no VMs are executing
        unsafe {
            self.space_mut(to_space).clear();
        }

        // Worklist for BFS traversal
        let mut worklist: Vec<HeapPtr> = roots.to_vec();

        // Process all reachable objects
        while let Some(old_ptr) = worklist.pop() {
            if forwarding.contains_key(&old_ptr) {
                continue;
            }

            if self.is_compile_time_ptr(old_ptr) {
                forwarding.insert(old_ptr, old_ptr);
                continue;
            }

            let new_ptr = self.copy_object_to_new_space(old_ptr, to_space, &mut forwarding);

            // SAFETY: GC runs at safepoints
            let obj = unsafe { new_ptr.get() };
            self.add_references_to_worklist(obj, &mut worklist);
        }

        // Fix up all references in the copied objects
        unsafe {
            self.fixup_references(to_space, &forwarding);
        }

        // Calculate stats before swapping
        // SAFETY: GC runs at safepoints
        let live_count = unsafe { self.space_ref(to_space).len() };
        let collected_count = old_space_size.saturating_sub(live_count);

        // Swap spaces
        self.active_space.store(to_space, Ordering::Release);
        self.reset_next_chunk(live_count);
        self.clear_tlab_canaries();

        // Clear or poison the old space
        self.finalize_from_space(from_space);

        // Remap roots to their new locations
        let remapped_roots: Vec<HeapPtr> = roots
            .iter()
            .map(|old_ptr| *forwarding.get(old_ptr).unwrap_or(old_ptr))
            .collect();

        // Update handle table
        let handles_invalidated = self.update_handles(&forwarding);

        let stats = GcStats {
            live_count,
            collected_count,
            handles_invalidated,
        };

        (stats, remapped_roots, forwarding)
    }

    /// Copy a single object from old space to new space.
    /// Returns the new HeapPtr.
    fn copy_object_to_new_space(
        &self,
        old_ptr: HeapPtr,
        to_space: usize,
        forwarding: &mut HashMap<HeapPtr, HeapPtr>,
    ) -> HeapPtr {
        // Clone the object from old location
        // SAFETY: GC runs at safepoints, no VMs are executing
        let obj = unsafe { old_ptr.get().clone() };

        // Append to new space and get pointer to new location
        // SAFETY: GC runs at safepoints, no VMs are executing
        let new_ptr = unsafe {
            let to_vec = self.space_mut(to_space);
            let new_runtime_idx = to_vec.len();
            to_vec.push_with(obj, || Object::String(String::new()));
            let raw_ptr = to_vec.get_ptr(new_runtime_idx);
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
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Function(_)
            | Object::Media(_)
            | Object::PromptAst(_)
            | Object::Resource(_) => {}
        }
    }

    /// Fix up all object references in the new space to use forwarded addresses.
    ///
    /// # Safety
    /// Must be called after all live objects have been copied.
    unsafe fn fixup_references(&self, to_space: usize, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        // SAFETY: All live objects have been copied to to_space, and no VMs are executing
        unsafe {
            let to_vec = self.space_mut(to_space);

            for obj in to_vec.iter_mut() {
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
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Function(_)
            | Object::Media(_)
            | Object::PromptAst(_)
            | Object::Resource(_) => {}
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
        let (stats, remapped) = unsafe { heap.collect_garbage(&[]) };

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
        let (stats, remapped) = unsafe { heap.collect_garbage(&roots) };

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
        let (stats, _) = unsafe { heap.collect_garbage(&[]) };

        assert_eq!(stats.live_count, 0);
        assert!(stats.collected_count > 0);
    }

    #[cfg(feature = "heap_debug")]
    #[test]
    #[ignore = "Requires heap_debug feature and epoch validation which now uses HeapPtr"]
    fn test_gc_stale_runtime_index_panics() {
        // This test was checking that stale ObjectIndex causes panic.
        // With HeapPtr, the safety model is different - we need to update
        // how epoch validation works with pointers.
        //
        // use std::panic::{AssertUnwindSafe, catch_unwind};
        // use crate::{HeapDebuggerConfig, HeapVerifyMode};
        //
        // let debug = HeapDebuggerConfig {
        //     enabled: true,
        //     verify: HeapVerifyMode::Off,
        // };
        // let heap = BexHeap::with_tlab_size_and_debug(vec![], 8, debug);
        // let mut tlab = Tlab::new(Arc::clone(&heap));
        //
        // let old_ptr = tlab.alloc_string("alive".to_string());
        // let (_, remapped) = unsafe { heap.collect_garbage(&[old_ptr]) };
        // let new_ptr = remapped[0];
        // assert_ne!(old_ptr.as_ptr(), new_ptr.as_ptr());
        //
        // // Using old_ptr after GC should be detectable
        // // (would need epoch stored in HeapPtr)
    }

    #[cfg(feature = "heap_debug")]
    #[test]
    #[ignore = "Requires heap_debug feature and epoch validation which now uses HeapPtr"]
    fn test_handle_resolved_index_stale_after_gc_panics() {
        // See above - this test needs updating for HeapPtr model
    }

    #[cfg(feature = "heap_debug")]
    #[test]
    #[ignore = "Requires heap_debug feature with Instance/Variant using HeapPtr"]
    fn test_full_verify_panics_on_bad_variant() {
        // This test creates a Variant with an ObjectIndex, which is now HeapPtr
        // We need to create a valid HeapPtr pointing to an Enum.
        //
        // use std::panic::{AssertUnwindSafe, catch_unwind};
        // use crate::{HeapDebuggerConfig, HeapVerifyMode};
        //
        // let compile_time = vec![Object::Enum(Enum {
        //     name: "E".to_string(),
        //     variant_names: vec!["A".to_string()],
        // })];
        // let debug = HeapDebuggerConfig {
        //     enabled: true,
        //     verify: HeapVerifyMode::Full,
        // };
        // let heap = BexHeap::with_tlab_size_and_debug(compile_time, 4, debug);
        // let mut tlab = Tlab::new(Arc::clone(&heap));
        //
        // let enm_ptr = heap.compile_time_ptr(0);
        // let _bad_variant = tlab.alloc(Object::Variant(bex_vm_types::types::Variant {
        //     enm: enm_ptr,
        //     index: 3, // Out of bounds variant index
        // }));
        //
        // let result = catch_unwind(AssertUnwindSafe(|| {
        //     heap.verify_quick();
        // }));
        // assert!(result.is_err());
    }

    #[cfg(feature = "heap_debug")]
    #[test]
    #[ignore = "Requires heap_debug feature with Instance using HeapPtr"]
    fn test_full_verify_panics_on_instance_field_mismatch() {
        // Similar to above - needs Instance.class to be a valid HeapPtr
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
        let (stats, remapped) = unsafe { heap.collect_garbage(&[obj1, obj2]) };

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
        let (stats, remapped) = unsafe { heap.collect_garbage(&[arr]) };

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

        // Remember initial active space
        let initial_space = heap.active_space_index();

        // Allocate an object
        let obj = tlab.alloc_string("test".to_string());

        // Run GC with the object as root
        let (_, _) = unsafe { heap.collect_garbage(&[obj]) };

        // Space should have swapped
        assert_eq!(heap.active_space_index(), 1 - initial_space);
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
        let (stats, _) = unsafe { heap.collect_garbage(&[]) };

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
            let (stats, _) = unsafe { heap.collect_garbage(&[]) };

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
        let (stats, _) = unsafe { heap.collect_garbage(&[]) };

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
        let (stats, remapped) = unsafe { heap.collect_garbage(&[map_obj]) };

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
        let (stats, _remapped, forwarding) =
            unsafe { heap.collect_garbage_with_forwarding(&roots) };

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
        let (stats, remapped, _forwarding) =
            unsafe { heap.collect_garbage_with_forwarding(&[outer_array]) };

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
            let (stats, _remapped, forwarding) =
                unsafe { heap.collect_garbage_with_forwarding(&persistent_roots) };

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

    /// Tests active space swap atomics during GC cycles.
    #[test]
    fn test_miri_active_space_swap() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("object1".to_string());
        let obj2 = tlab.alloc_string("object2".to_string());

        let initial_space = heap.active_space_index();

        // GC triggers space swap
        let (stats, remapped, _) = unsafe { heap.collect_garbage_with_forwarding(&[obj1, obj2]) };

        assert_eq!(heap.active_space_index(), 1 - initial_space);
        assert_eq!(stats.live_count, 2);

        // Verify objects accessible in new space
        for ptr in &remapped {
            assert!(matches!(unsafe { ptr.get() }, Object::String(_)));
        }

        // Second GC swaps back
        let (stats2, remapped2, _) = unsafe { heap.collect_garbage_with_forwarding(&remapped) };

        assert_eq!(heap.active_space_index(), initial_space);
        assert_eq!(stats2.live_count, 2);

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
        let (stats, _, forwarding) = unsafe { heap.collect_garbage_with_forwarding(&roots) };

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
}
