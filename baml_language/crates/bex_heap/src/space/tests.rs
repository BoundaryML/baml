//! The space itself: start bitmap, conservative lookup, size classes and
//! free lists, large objects, descriptor-driven tracing and the counters.

use crate::descriptor::CellHeader as _;
use std::{borrow::Cow, ops::Range, sync::Arc};

use bex_vm_types::{HeapPtr, Object};

use super::{
    BLOCK_BYTES, GRANULE_BYTES, LARGE_OBJECT_THRESHOLD,
    block::{BlockHeader, FIRST_CELL_OFFSET},
    cell_of,
    mark::Marker,
    region_base,
    small::{SIZE_CLASSES, cell_size, class_of},
};
use crate::{
    BexHeap, ConservativeRanges, Descriptor, DescriptorId, ElemKind, Kind, SlotKind, TaggedKind,
    TaggedSlot, Tlab, Trace,
    descriptor::{HEADER_BYTES, Header, OBJECT_CELL_BYTES},
    object_trace::heap_ptr_at,
};

/// A node: two pointer slots and one tagged slot.
const NODE: Descriptor = Descriptor {
    kind: Kind::Instance,
    trace: Trace::Slots,
    size: 48,
    elems: None,
    ptr_slots: Cow::Borrowed(&[8, 16]),
    tagged_slots: Cow::Borrowed(&[24]),
    slot_kinds: Cow::Borrowed(&[SlotKind::Ptr, SlotKind::Ptr, SlotKind::Tagged16]),
    type_id: 1,
    finalizer: None,
};
const NODE_LEFT: usize = 8;
const NODE_TAGGED: usize = 24;

/// A backing store of pointers after a 16-byte prefix.
const BACKING: Descriptor = Descriptor {
    kind: Kind::Backing,
    trace: Trace::Slots,
    size: 16,
    elems: Some(ElemKind::Ptr),
    ptr_slots: Cow::Borrowed(&[]),
    tagged_slots: Cow::Borrowed(&[]),
    slot_kinds: Cow::Borrowed(&[]),
    type_id: 2,
    finalizer: None,
};

/// Bytes of a backing store too large for a block.
const LARGE_BACKING_BYTES: usize = 16 * 1024;
/// Bytes of a medium cell: well over a VM object, small enough for a block.
const MEDIUM_BYTES: usize = 2 * 1024;

struct Fixture {
    heap: Arc<BexHeap>,
    node: DescriptorId,
    backing: DescriptorId,
}

fn fixture() -> Fixture {
    let heap = BexHeap::build_unsealed_default(vec![]);
    let node = heap.register_descriptor(NODE).unwrap();
    let backing = heap.register_descriptor(BACKING).unwrap();
    Fixture {
        heap: heap.seal(),
        node,
        backing,
    }
}

fn cell_of_ptr(ptr: HeapPtr) -> usize {
    cell_of(ptr.as_ptr() as usize)
}

/// # Safety
/// `cell + offset` is a writable word of an allocated cell.
unsafe fn write_word(cell: usize, offset: usize, value: usize) {
    // SAFETY: forwarded to the caller.
    unsafe { ((cell + offset) as *mut usize).write(value) };
}

fn string_at(ptr: HeapPtr) -> String {
    // SAFETY: tests read only objects they keep alive.
    match unsafe { ptr.get() } {
        Object::String(text) => text.as_str().to_owned(),
        other => panic!("expected a string, found {other:?}"),
    }
}

/// Resolve `address` as a conservative root would.
fn resolve(heap: &BexHeap, address: usize) -> Option<usize> {
    let small = heap.small.lock();
    let large = heap.large.lock();
    Marker::new(
        small.pool(),
        &large,
        heap.descriptors(),
        heap.compile_time_range(),
    )
    .resolve_conservative(address)
}

#[test]
fn every_cell_start_is_recorded_and_nothing_between() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let cells: Vec<usize> = (0..10)
        .map(|i| cell_of_ptr(tlab.alloc(Object::String((format!("object {i}")).into()))))
        .collect();
    let base = region_base(cells[0]);
    // SAFETY: an initialized block of this heap.
    let header = unsafe { BlockHeader::at(base) };
    let starts: Vec<usize> = header.cells(base).collect();
    assert_eq!(starts, cells, "one start per cell, ascending");
    for pair in cells.windows(2) {
        assert_eq!(pair[1] - pair[0], OBJECT_CELL_BYTES);
    }
    for &cell in &cells {
        // SAFETY: an allocated cell.
        let cell_header = unsafe { Header::at(cell) };
        assert_eq!(cell_header.descriptor, crate::OBJECT_DESC);
        assert_eq!(cell_header.cell_bytes(), OBJECT_CELL_BYTES);
    }
}

#[test]
fn conservative_lookup_resolves_interior_pointers_and_ignores_the_rest() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let small = cell_of_ptr(tlab.alloc(Object::String(("small").into())));
    let medium = tlab.alloc_zeroed(f.backing, MEDIUM_BYTES).as_ptr() as usize;
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;

    for (cell, bytes) in [
        (small, OBJECT_CELL_BYTES),
        (medium, MEDIUM_BYTES),
        (large, LARGE_BACKING_BYTES),
    ] {
        for offset in [0, 1, HEADER_BYTES, bytes / 2, bytes - 1] {
            assert_eq!(
                resolve(&f.heap, cell + offset),
                Some(cell),
                "{offset} bytes into a {bytes}-byte cell"
            );
        }
        assert_ne!(
            resolve(&f.heap, cell + bytes),
            Some(cell),
            "one past the end"
        );
    }

    let block = region_base(small);
    let local = 0_usize;
    let boxed = Box::new(0_u64);
    let outside = [
        0,
        block,                               // the block header
        block + FIRST_CELL_OFFSET - 8,       // still the block header
        small - 8,                           // before the first cell
        std::ptr::from_ref(&local).addr(),   // the stack
        std::ptr::from_ref(&*boxed).addr(),  // the Rust heap
        block + BLOCK_BYTES - GRANULE_BYTES, // unallocated end of block
        region_base(large),                  // a large object's prefix
        usize::MAX,
    ];
    for address in outside {
        assert_eq!(resolve(&f.heap, address), None, "{address:#x}");
    }
}

#[test]
fn a_conservative_root_keeps_its_object_and_only_its_object() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let kept = tlab.alloc(Object::String(("kept").into()));
    tlab.alloc(Object::String(("garbage").into()));
    let unrelated = Box::new(7_u64);
    // A fake stack: an interior pointer, a scalar and a Rust heap pointer.
    let stack: [usize; 3] = [
        kept.as_ptr() as usize + 24,
        42,
        std::ptr::from_ref(&*unrelated).addr(),
    ];
    let range: Range<*const usize> = stack.as_ptr_range();
    // SAFETY: no mutator runs; the range is readable for the collection.
    let (stats, _) = unsafe { heap.collect(&mut ConservativeRanges(&[range])) };
    assert_eq!(stats.live_count, 1);
    assert_eq!(stats.collected_count, 1);
    assert_eq!(string_at(kept), "kept");
}

#[test]
fn requests_round_up_to_the_smallest_class_that_fits() {
    assert!(SIZE_CLASSES.is_sorted());
    for (class, &size) in SIZE_CLASSES.iter().enumerate() {
        assert!(size.is_multiple_of(GRANULE_BYTES));
        assert_eq!(class_of(size), class, "a class size is its own class");
    }
    for bytes in (GRANULE_BYTES..=LARGE_OBJECT_THRESHOLD).step_by(GRANULE_BYTES) {
        let size = cell_size(bytes);
        assert!(size >= bytes);
        let smaller = SIZE_CLASSES[..class_of(bytes)].iter().all(|&c| c < bytes);
        assert!(smaller, "{bytes} bytes go in the smallest class that fits");
        assert!(size - bytes <= bytes / 5, "{bytes} rounds to {size}");
    }
    assert_eq!(
        cell_size(OBJECT_CELL_BYTES),
        OBJECT_CELL_BYTES,
        "VM objects round to nothing"
    );
    assert_eq!(cell_size(272), 288);
    assert_eq!(cell_size(4112), 4864);
}

#[test]
fn a_sweep_frees_slots_from_the_mark_bits() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let cells: Vec<HeapPtr> = (0..10)
        .map(|i| tlab.alloc(Object::String((format!("cell {i}")).into())))
        .collect();
    let kept: Vec<HeapPtr> = cells.iter().copied().step_by(2).collect();
    let dead: Vec<usize> = cells
        .iter()
        .skip(1)
        .step_by(2)
        .map(|&ptr| cell_of_ptr(ptr))
        .collect();
    let room = tlab.remaining();

    // SAFETY: no mutator runs.
    let (stats, _, _) = unsafe { heap.collect_garbage(&kept) };
    assert_eq!(stats.collected_count, 5);
    assert_eq!(tlab.remaining(), room + 5 * OBJECT_CELL_BYTES);

    // The freed slots come back first, lowest address first.
    let reused: Vec<usize> = (0..5)
        .map(|i| cell_of_ptr(tlab.alloc(Object::String((format!("again {i}")).into()))))
        .collect();
    assert_eq!(reused, dead);
    for (i, &ptr) in kept.iter().enumerate() {
        assert_eq!(string_at(ptr), format!("cell {}", 2 * i));
    }
}

#[test]
fn a_sweep_never_reads_a_dead_slot_cell() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let mut node = || {
        let cell = tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr();
        // SAFETY: a fresh cell of this heap; its payload follows the header.
        unsafe {
            f.heap
                .make_heap_ptr(cell.add(HEADER_BYTES).cast::<Object>())
        }
    };
    let nodes: Vec<HeapPtr> = (0..6).map(|_| node()).collect();
    let kept = [nodes[0], nodes[3]];
    let dead: Vec<usize> = [1, 2, 4, 5].map(|i| cell_of_ptr(nodes[i])).to_vec();
    for &cell in &dead {
        // A descriptor no table has: reading it would panic. The sweep frees
        // a dead instance from the bitmaps alone, and the allocator does not
        // read a free slot either.
        // SAFETY: an allocated cell's header word.
        unsafe { (cell as *mut Header).write(Header::for_cell(DescriptorId(u32::MAX), 48, false)) };
    }

    // SAFETY: no mutator runs.
    let (stats, _, _) = unsafe { f.heap.collect_garbage(&kept) };
    assert_eq!((stats.live_count, stats.collected_count), (2, 4));
    let reused: Vec<usize> = (0..4)
        .map(|_| tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr() as usize)
        .collect();
    assert_eq!(reused, dead, "freed slots come back lowest address first");
}

#[test]
fn an_empty_block_returns_to_the_pool_and_serves_another_class() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let first = tlab.alloc(Object::String(("only cell").into()));
    let block = region_base(first.as_ptr() as usize);
    drop(tlab);
    let stats = f.heap.stats();

    // SAFETY: no mutator runs; nothing is rooted.
    unsafe { f.heap.collect_garbage(&[]) };
    assert_eq!(
        f.heap.stats().free_blocks,
        stats.free_blocks + 1,
        "the block is empty"
    );

    let mut tlab = Tlab::new_empty(Arc::clone(&f.heap));
    let node = tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr() as usize;
    assert_ne!(
        cell_size(NODE.size as usize),
        OBJECT_CELL_BYTES,
        "a different class"
    );
    assert_eq!(
        region_base(node),
        block,
        "the emptied block serves the new class"
    );
}

#[test]
fn large_objects_are_traced_kept_and_freed() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let text = tlab.alloc(Object::String(("held by a large backing store").into()));
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let last_element = LARGE_BACKING_BYTES - 8;
    // SAFETY: an element of the fresh large cell.
    unsafe { write_word(large, last_element, text.as_ptr() as usize) };
    assert_ne!(region_base(large), region_base(text.as_ptr() as usize));
    assert_eq!(f.heap.stats().large_objects, 1);

    let root = heap_ptr_at(large);
    // SAFETY: no mutator runs.
    let (stats, _, _) = unsafe { f.heap.collect_garbage(&[root]) };
    assert_eq!(stats.live_count, 2, "the backing store and its element");
    assert_eq!(string_at(text), "held by a large backing store");
    assert!(bex_vm_types::Survival::survived(&*f.heap, root));

    // SAFETY: as above.
    let (stats, _, _) = unsafe { f.heap.collect_garbage(&[]) };
    assert_eq!(stats.collected_count, 2);
    assert_eq!(f.heap.stats().large_objects, 0);
    assert!(!bex_vm_types::Survival::survived(&*f.heap, root));
}

#[test]
fn slot_descriptors_trace_pointers_and_tagged_pointers_only() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let left = tlab.alloc(Object::String(("left").into()));
    let by_tag = tlab.alloc(Object::String(("by tag").into()));
    let scalar_lookalike = tlab.alloc(Object::String(("only looks like a pointer").into()));
    let node = tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr() as usize;
    let other = tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr() as usize;
    // A tagged slot is a tag byte, padding, then the payload word.
    let tagged = |slot: usize, kind: TaggedKind, ptr: HeapPtr| unsafe {
        (slot as *mut u8).write(kind as u8);
        ((slot + 8) as *mut u64).write(ptr.as_ptr() as u64);
    };
    const _: () = assert!(size_of::<TaggedSlot>() == 16);
    // SAFETY: slots of the fresh node cells.
    unsafe {
        write_word(node, NODE_LEFT, left.as_ptr() as usize);
    }
    tagged(node + NODE_TAGGED, TaggedKind::Ptr, by_tag);
    tagged(other + NODE_TAGGED, TaggedKind::Int, scalar_lookalike);
    // SAFETY: no mutator runs.
    let (stats, _, _) = unsafe {
        f.heap
            .collect_garbage(&[heap_ptr_at(node), heap_ptr_at(other)])
    };
    assert_eq!(
        stats.live_count, 4,
        "two nodes, the pointer and the tagged pointer"
    );
    assert_eq!(stats.collected_count, 1, "the int-tagged lookalike died");
    assert_eq!(string_at(left), "left");
    assert_eq!(string_at(by_tag), "by tag");
}

#[test]
fn heap_live_and_class_counters_follow_allocation_and_collection() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let stats = f.heap.stats();
    assert_eq!(stats.live_bytes, 0, "nothing collected yet");
    assert_eq!(stats.heap_bytes, BLOCK_BYTES, "the allocator's block");

    let kept = tlab.alloc(Object::String(("kept").into()));
    for i in 0..1_000 {
        tlab.alloc(Object::String((format!("garbage {i}")).into()));
    }
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let stats = f.heap.stats();
    assert!(stats.heap_bytes >= 3 * BLOCK_BYTES + LARGE_BACKING_BYTES);
    assert_eq!(
        stats.heap_bytes,
        (stats.blocks - stats.free_blocks) * BLOCK_BYTES
            + (stats.reserved_bytes - stats.blocks * BLOCK_BYTES),
    );

    let object_class = class_of(OBJECT_CELL_BYTES);
    let occupancy = f.heap.class_occupancy();
    assert_eq!(occupancy[object_class].cells, 1_001);
    assert_eq!(
        occupancy.iter().map(|class| class.cells).sum::<usize>(),
        1_001,
        "the large cell is in no class"
    );

    // SAFETY: no mutator runs.
    unsafe { f.heap.collect_garbage(&[kept, heap_ptr_at(large)]) };
    assert_eq!(
        f.heap.stats().live_bytes,
        OBJECT_CELL_BYTES + LARGE_BACKING_BYTES
    );
    let objects = f.heap.class_occupancy()[object_class];
    assert_eq!(objects.cell_bytes, OBJECT_CELL_BYTES);
    assert_eq!(objects.cells, 1, "only the survivor is still allocated");
    assert_eq!(
        objects.capacity,
        objects.blocks * ((BLOCK_BYTES - FIRST_CELL_OFFSET) / OBJECT_CELL_BYTES)
    );

    drop(tlab);
    // SAFETY: no mutator runs.
    unsafe { f.heap.collect_garbage(&[kept]) };
    let stats = f.heap.stats();
    assert_eq!(stats.live_bytes, OBJECT_CELL_BYTES);
    assert_eq!(
        stats.heap_bytes, BLOCK_BYTES,
        "one block holds the survivor; the rest are free and the large object is gone"
    );
}
