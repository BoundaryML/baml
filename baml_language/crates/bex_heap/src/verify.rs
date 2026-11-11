//! The post-reclaim verifier: under `heap_debug` or `BAML_GC_STRESS=1`
//! (the same gate as [poisoning](crate::space::gc_poison)), after a
//! collection reclaimed, every pointer held anywhere names a live cell.
//!
//! The walk is the collector's own: every cell with a start bit in every
//! block, every large cell, every compile-time object, each traced through
//! its descriptor (`trace_slots` for a slot-described cell, the `trace` hook
//! for a Rust-valued one, the object tracer for a compile-time object); then
//! every holder root and weak reference through the holder's `visit_roots`,
//! every handle, every pending finalizer and queued spawn error. A pointer
//! that names a freed block, a poisoned cell, or a cell the mark did not
//! reach is reported with where it was held.

use bex_vm_types::{HeapPtr, RootHaver, RootVisitor};

use crate::{
    BexHeap,
    descriptor::{CellHeader as _, Header, Trace, RUST_PAYLOAD_OFFSET, trace_slots},
    space::{
        block::BlockHeader, blocks::BlockSpace, cell_bytes, cell_of, large::LargeSpace,
        sweep::POISON_BYTE,
    },
};

/// Why a pointer is bad.
#[derive(Debug)]
enum Fault {
    /// Not in any block or large cell the heap holds.
    Unallocated,
    /// In a block that holds no cell at all: freed whole by this collection.
    FreedBlock,
    /// The cell's header is the poison pattern: reclaimed and poisoned.
    Poisoned,
    /// No start bit: reclaimed, or never a cell.
    NoCell,
    /// A cell the mark did not reach: dead, or a moved cell's old copy.
    Unmarked,
}

/// The post-reclaim view of the space.
struct Space<'a> {
    heap: &'a BexHeap,
    blocks: &'a BlockSpace,
    large: &'a LargeSpace,
}

impl Space<'_> {
    /// What is wrong with `ptr`, if anything. Null is nothing.
    fn fault(&self, ptr: HeapPtr) -> Option<Fault> {
        if ptr.is_null() || self.heap.is_compile_time_ptr(ptr) {
            return None;
        }
        let cell = cell_of(ptr.as_ptr() as usize);
        if let Some(base) = self.blocks.pool().block_containing(cell) {
            // SAFETY: an initialized block; only its tables are read.
            let header = unsafe { BlockHeader::at(base) };
            if header.starts.is_empty() {
                return Some(Fault::FreedBlock);
            }
            // SAFETY: the block's cell area is mapped; one word is read.
            if unsafe { (cell as *const usize).read() } == usize::from_ne_bytes([POISON_BYTE; 8]) {
                return Some(Fault::Poisoned);
            }
            let granule = BlockHeader::granule(base, cell);
            if !header.starts.get(granule) {
                return Some(Fault::NoCell);
            }
            if !header.marks.get(granule) {
                return Some(Fault::Unmarked);
            }
            return None;
        }
        if !self.large.contains_cell(cell) {
            return Some(Fault::Unallocated);
        }
        // SAFETY: a member of the large-object set is allocated.
        if !unsafe { LargeSpace::is_marked(cell) } {
            return Some(Fault::Unmarked);
        }
        None
    }

    #[track_caller]
    fn check(&self, held_by: &dyn std::fmt::Display, ptr: HeapPtr) {
        if let Some(fault) = self.fault(ptr) {
            panic!(
                "gc verifier: {held_by} holds {:#x}, which names {}",
                ptr.as_ptr() as usize,
                match fault {
                    Fault::Unallocated => "no block or large cell of this heap",
                    Fault::FreedBlock => "a block this collection freed",
                    Fault::Poisoned => "a poisoned cell",
                    Fault::NoCell => "no allocated cell (reclaimed, or never a cell start)",
                    Fault::Unmarked => "a cell the mark did not reach (dead, or a moved cell's old copy)",
                }
            );
        }
    }
}

/// The holder walk of the verifier: every root checked, nothing changed.
struct VerifyRoots<'a> {
    space: &'a Space<'a>,
}

impl RootVisitor for VerifyRoots<'_> {
    fn visit(&mut self, root: &mut HeapPtr) {
        self.space.check(&"a holder's strong root", *root);
    }

    fn visit_weak(&mut self, root: &mut HeapPtr) -> bool {
        self.space.check(&"a holder's weak reference", *root);
        true
    }
}

impl BexHeap {
    /// Check every pointer the heap and its holders hold; see the module
    /// docs. Panics on the first bad one.
    ///
    /// # Safety
    /// Right after a collection's reclaim, with its locks still held and no
    /// mutator running: the marks are the collection's, every allocated cell
    /// is intact, and `holder` is parked.
    pub(crate) unsafe fn verify_after_reclaim(
        &self,
        blocks: &BlockSpace,
        large: &LargeSpace,
        holder: Option<&mut (dyn RootHaver + '_)>,
    ) {
        let space = Space {
            heap: self,
            blocks,
            large,
        };
        let mut edges = Vec::new();

        // Every runtime cell, through its descriptor.
        let mut cells = Vec::new();
        // SAFETY: forwarded to the caller.
        unsafe { Self::for_each_runtime_cell(blocks.pool(), large, |cell| cells.push(cell)) };
        for cell in cells {
            // SAFETY: an allocated cell with a written header.
            let id = unsafe { Header::at(cell) }.descriptor;
            let descriptor = self.descriptors().get(id);
            // SAFETY: allocated, intact and laid out by its descriptor.
            unsafe {
                match descriptor.trace {
                    Trace::Rust(hooks) => {
                        (hooks.trace)((cell + RUST_PAYLOAD_OFFSET) as *const u8, &mut edges);
                    }
                    Trace::Slots => trace_slots(descriptor, cell, cell_bytes(cell), &mut edges),
                }
            }
            let held_by = format!("cell {cell:#x} (descriptor {})", id.0);
            for edge in edges.drain(..) {
                space.check(&held_by, edge);
            }
        }

        // Every compile-time object, through the object tracer.
        for index in 0..self.compile_time_len() {
            let ptr = self.compile_time_ptr(index);
            // SAFETY: compile-time objects live for the heap's lifetime.
            crate::object_trace::trace_object(unsafe { ptr.get() }, &mut edges);
            let held_by = format!("compile-time object {index}");
            for edge in edges.drain(..) {
                space.check(&held_by, edge);
            }
        }

        // Every holder root, the handles, and the heap's own queues.
        if let Some(holder) = holder {
            holder.visit_roots(&mut VerifyRoots { space: &space });
        }
        for (key, ptr) in self.handles.read().expect("handles lock poisoned").iter() {
            space.check(&format!("handle {key}"), *ptr);
        }
        for (ptr, entry) in self
            .handles_by_ptr
            .read()
            .expect("handles lock poisoned")
            .iter()
        {
            space.check(&format!("the reverse index of handle {}", entry.0), *ptr);
        }
        for (instance, cleanup) in self
            .pending_finalizers
            .lock()
            .expect("pending_finalizers lock poisoned")
            .iter()
        {
            space.check(&"a pending finalizer's instance", *instance);
            space.check(&"a pending finalizer's cleanup body", *cleanup);
        }
        for error in self
            .pending_unhandled_spawn_errors
            .lock()
            .expect("pending_unhandled_spawn_errors lock poisoned")
            .iter()
        {
            if let Some(ptr) = error.value.as_object_ptr() {
                space.check(&"a queued spawn error's value", ptr);
            }
        }
        for ptr in &self
            .finalizable
            .lock()
            .expect("finalizable lock poisoned")
            .instances
        {
            space.check(&"the finalizable list", *ptr);
        }
    }
}
