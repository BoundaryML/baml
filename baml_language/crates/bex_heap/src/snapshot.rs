//! Heap support for loading a snapshot.
//!
//! A snapshot loader has to know the address of every restored object before
//! it can decode any of them, because objects point at each other. `Object` is
//! a fixed-size slot and Gen0 chunks never move, so the loader reserves one
//! slot per snapshot object first
//! ([`BexHeap::alloc_snapshot_placeholders`]), learns the addresses, and then
//! overwrites each slot in place
//! ([`BexHeap::overwrite_snapshot_object`]).
//!
//! The writer needs the inverse of [`BexHeap::compile_time_ptr`]
//! ([`BexHeap::compile_time_index_of`]) so that it can refer to a compile-time
//! object by index instead of copying it.
#![allow(unsafe_code)]

use bex_vm_types::{HeapPtr, Object, snapshot_ctx::CompileTimeRegion};

use crate::BexHeap;

impl BexHeap {
    /// The compile-time region in the form the snapshot translation context
    /// (`bex_vm_types::snapshot_ctx`) consumes.
    #[must_use]
    pub fn snapshot_compile_time_region(&self) -> CompileTimeRegion {
        let len = self.compile_time_len();
        CompileTimeRegion {
            base: if len == 0 {
                HeapPtr::null()
            } else {
                self.compile_time_ptr(0)
            },
            len,
        }
    }

    /// Index of a compile-time object, the inverse of
    /// [`Self::compile_time_ptr`]. `None` for a runtime pointer.
    #[must_use]
    pub fn compile_time_index_of(&self, ptr: HeapPtr) -> Option<usize> {
        self.snapshot_compile_time_region()
            .index_of(ptr)
            .map(|index| index as usize)
    }

    /// Reserve `count` fresh Gen0 slots and return their addresses, in order.
    ///
    /// Every slot holds the heap's placeholder object until the caller
    /// overwrites it with [`Self::overwrite_snapshot_object`]. The slots come
    /// from one dedicated region reserved through the same path a TLAB refill
    /// uses, so no VM's TLAB overlaps them and the allocation budget is
    /// charged as for ordinary allocations.
    ///
    /// # GC contract
    ///
    /// The returned pointers are not roots. The caller must make sure that no
    /// collection runs until every restored value is reachable from a
    /// registered root holder (for the engine: hold a heap permit from before
    /// this call until the restored thread states are installed in their
    /// VMs). A collection in that window would not trace the restored objects
    /// and would leave the returned pointers dangling.
    #[must_use]
    pub fn alloc_snapshot_placeholders(&self, count: usize) -> Vec<HeapPtr> {
        if count == 0 {
            return Vec::new();
        }
        let chunk = self.alloc_tlab_chunk_sized(count);
        let compile_time_len = self.compile_time_len();
        // SAFETY: shared access to Gen0 for address computation only. The
        // chunk was just grown to cover `[chunk.start, chunk.end)`, and
        // `ChunkedVec` never moves existing elements.
        let gen0 = unsafe { &*self.gen0.get() };
        (chunk.start..chunk.end)
            .map(|global_index| {
                self.record_alloc();
                let raw = gen0.get_ptr(global_index - compile_time_len);
                // SAFETY: `raw` points at an initialized placeholder slot
                // inside the region reserved above.
                unsafe { self.make_heap_ptr(raw) }
            })
            .collect()
    }

    /// Replace the placeholder at `ptr` with `object`.
    ///
    /// # Safety
    ///
    /// - `ptr` was returned by [`Self::alloc_snapshot_placeholders`] on this
    ///   heap, and no collection has run since.
    /// - No other thread reads or writes the slot. Placeholder slots are
    ///   unreachable from every VM until the loader hands out the restored
    ///   values, so this holds as long as the caller does that last.
    ///
    /// No write barrier is needed: the slot is in Gen0, the youngest
    /// generation, so the write cannot create an old-to-young edge.
    pub unsafe fn overwrite_snapshot_object(&self, ptr: HeapPtr, object: Object) {
        debug_assert!(
            !self.is_compile_time_ptr(ptr),
            "snapshot loader must never overwrite a compile-time object"
        );
        // SAFETY: exclusive access per the contract above; the slot holds a
        // valid placeholder `Object`, which the assignment drops.
        unsafe {
            *ptr.get_mut() = object;
        }
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::Object;

    use crate::BexHeap;

    #[test]
    fn placeholders_are_distinct_runtime_slots_that_can_be_overwritten() {
        let heap = BexHeap::new(vec![Object::Float(1.0), Object::Float(2.0)]);
        let slots = heap.alloc_snapshot_placeholders(5000);
        assert_eq!(slots.len(), 5000);
        let mut seen = std::collections::HashSet::new();
        for (i, ptr) in slots.iter().enumerate() {
            assert!(!heap.is_compile_time_ptr(*ptr));
            assert!(seen.insert(*ptr), "slot {i} repeats an address");
            #[allow(unsafe_code)]
            // SAFETY: fresh placeholder, single-threaded test, no GC.
            unsafe {
                heap.overwrite_snapshot_object(*ptr, Object::Float(i as f64));
            }
        }
        for (i, ptr) in slots.iter().enumerate() {
            #[allow(unsafe_code)]
            // SAFETY: the slot was written above and nothing else touches it.
            let object = unsafe { heap.get_object(*ptr) };
            assert!(matches!(object, Object::Float(f) if *f == i as f64));
        }
    }

    #[test]
    fn compile_time_index_is_the_inverse_of_compile_time_ptr() {
        let heap = BexHeap::new(vec![Object::Float(1.0), Object::Float(2.0)]);
        assert_eq!(
            heap.compile_time_index_of(heap.compile_time_ptr(1)),
            Some(1)
        );
        let runtime = heap.alloc_snapshot_placeholders(1)[0];
        assert_eq!(heap.compile_time_index_of(runtime), None);
    }
}
