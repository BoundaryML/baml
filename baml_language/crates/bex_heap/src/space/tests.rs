//! The space itself: start bitmap, conservative lookup, bump blocks and
//! their reuse, large objects, descriptor-driven tracing and the counters.

use crate::descriptor::CellHeader as _;
use std::{borrow::Cow, ops::Range, sync::Arc};

use bex_vm_types::{HeapPtr, Object};

use super::{
    BLOCK_BYTES, GRANULE_BYTES, LARGE_OBJECT_THRESHOLD,
    block::{BlockHeader, FIRST_CELL_OFFSET},
    cell_of, is_cell_pinned,
    mark::{Marker, Resolved},
    region_base,
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
fn resolve(heap: &BexHeap, address: usize) -> Resolved {
    let mut blocks = heap.blocks.lock();
    let large = heap.large.lock();
    Marker::new(
        &mut blocks,
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
        .map(|i| cell_of_ptr(tlab.alloc_string(format!("object {i}"))))
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
    let small = cell_of_ptr(tlab.alloc_string("small"));
    let medium = tlab.alloc_zeroed(f.backing, MEDIUM_BYTES).as_ptr() as usize;
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;

    for (cell, bytes) in [
        (small, OBJECT_CELL_BYTES),
        (medium, MEDIUM_BYTES),
        (large, LARGE_BACKING_BYTES),
    ] {
        for offset in [0, 1, HEADER_BYTES, bytes / 2, bytes - 1] {
            assert_eq!(
                resolve(&f.heap, cell + offset).containing,
                Some(cell),
                "{offset} bytes into a {bytes}-byte cell"
            );
        }
        let past = resolve(&f.heap, cell + bytes);
        assert_ne!(past.containing, Some(cell), "one past the end");
        assert_eq!(past.ending, Some(cell), "one past the end names the cell");
        assert_eq!(
            resolve(&f.heap, cell + bytes - 8).ending,
            None,
            "a word inside the cell ends no cell"
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
        assert_eq!(
            resolve(&f.heap, address),
            Resolved::default(),
            "{address:#x}"
        );
    }
}

#[test]
fn a_word_at_a_cells_end_keeps_and_pins_it() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let kept = tlab.alloc_string("ends here");
    let small = cell_of_ptr(kept);
    // Nothing is allocated after `small`, so its end is in no other cell.
    let stack: [usize; 2] = [small + OBJECT_CELL_BYTES, large + LARGE_BACKING_BYTES];
    let range: Range<*const usize> = stack.as_ptr_range();
    // SAFETY: no mutator runs; the range is readable for the collection.
    let (stats, _) = unsafe { f.heap.collect(&mut ConservativeRanges(&[range])) };
    assert_eq!(stats.live_count, 2, "both cells, by their end addresses");
    assert_eq!(stats.collected_count, 0);
    assert_eq!(string_at(kept), "ends here");
    // SAFETY: both survived.
    unsafe {
        assert!(is_cell_pinned(small));
        assert!(is_cell_pinned(large));
    }
}

#[test]
fn pins_are_cleared_at_the_next_collection() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let kept = tlab.alloc_string("kept");
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let stack: [usize; 2] = [kept.as_ptr() as usize + 8, large + 64];
    let range: Range<*const usize> = stack.as_ptr_range();
    // SAFETY: no mutator runs; the range is readable for the collection.
    let (stats, _) = unsafe { f.heap.collect(&mut ConservativeRanges(&[range])) };
    assert_eq!(stats.live_count, 2);
    // SAFETY: both survived.
    unsafe {
        assert!(is_cell_pinned(cell_of_ptr(kept)));
        assert!(is_cell_pinned(large));
    }

    // SAFETY: no mutator runs; precise roots only.
    let (stats, roots, _) = unsafe { f.heap.collect_garbage(&[kept, heap_ptr_at(large)]) };
    assert_eq!(stats.live_count, 2);
    assert_ne!(roots[0], kept, "unpinned this time, so copied");
    // SAFETY: both survived again.
    unsafe {
        assert!(
            !is_cell_pinned(cell_of_ptr(roots[0])),
            "no conservative root this time"
        );
        assert!(!is_cell_pinned(large));
    }
}

/// A cell a conservative word names stays at its address, and its block is
/// retained even with no mutator allocating into it.
#[test]
fn a_pinned_cell_stays_and_retains_its_block() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let kept = tlab.alloc_string("kept");
    tlab.alloc_string("garbage");
    let block = region_base(kept.as_ptr() as usize);
    drop(tlab);
    let free_blocks = heap.stats().free_blocks;
    let stack: [usize; 1] = [kept.as_ptr() as usize + 8];
    let range: Range<*const usize> = stack.as_ptr_range();
    // SAFETY: no mutator runs; the range is readable for the collection.
    let (stats, restamp) = unsafe { heap.collect(&mut ConservativeRanges(&[range])) };
    assert_eq!((stats.live_count, stats.collected_count), (1, 1));
    assert!(restamp.iter().all(|(old, new)| old == new), "nothing moved");
    assert_eq!(string_at(kept), "kept");
    assert!(bex_vm_types::Survival::survived(&*heap, kept));
    // SAFETY: an initialized block of this heap.
    let header = unsafe { BlockHeader::at(block) };
    assert_eq!(
        header.cells(block).collect::<Vec<_>>(),
        vec![cell_of_ptr(kept)]
    );
    assert_eq!(
        heap.stats().free_blocks,
        free_blocks,
        "the block is retained"
    );
}

/// A from-space block with no pin goes back to the pool empty, its
/// survivor copied elsewhere.
#[test]
fn an_unpinned_from_space_block_is_empty_in_the_pool() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let kept = tlab.alloc_string("kept");
    tlab.alloc_string("garbage");
    let block = region_base(kept.as_ptr() as usize);
    drop(tlab);
    let free_blocks = heap.stats().free_blocks;
    // SAFETY: no mutator runs.
    let (stats, roots, _) = unsafe { heap.collect_garbage(&[kept]) };
    assert_eq!((stats.live_count, stats.collected_count), (1, 1));
    assert_ne!(region_base(roots[0].as_ptr() as usize), block);
    assert_eq!(string_at(roots[0]), "kept");
    // SAFETY: an initialized block of this heap.
    let header = unsafe { BlockHeader::at(block) };
    assert!(header.starts.is_empty() && header.hooked.is_empty());
    // Back in the pool, where the to-space block holding the copy came from.
    assert_eq!(heap.stats().free_blocks, free_blocks);
    assert_eq!(
        heap.stats().heap_bytes,
        BLOCK_BYTES,
        "one to-space block in use"
    );
    assert!(!bex_vm_types::Survival::survived(&*heap, kept));
    assert!(bex_vm_types::Survival::survived(&*heap, roots[0]));
}

#[test]
fn a_conservative_root_keeps_its_object_and_only_its_object() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let kept = tlab.alloc_string("kept");
    tlab.alloc_string("garbage");
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
    // SAFETY: `kept` survived.
    assert!(
        unsafe { is_cell_pinned(cell_of_ptr(kept)) },
        "an interior word pins"
    );
}

/// Cells of different sizes follow one another in one block, each at the
/// previous one's end: no size classes, no rounding past the granule.
#[test]
fn mixed_size_cells_bump_contiguously_in_one_block() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let sizes = [
        OBJECT_CELL_BYTES,
        NODE.size as usize,
        MEDIUM_BYTES,
        NODE.size as usize + 8,
        OBJECT_CELL_BYTES,
        LARGE_OBJECT_THRESHOLD,
    ];
    let cells: Vec<(usize, usize)> = sizes
        .iter()
        .map(|&bytes| {
            let cell = if bytes == OBJECT_CELL_BYTES {
                cell_of_ptr(tlab.alloc_string("object"))
            } else {
                tlab.alloc_zeroed(f.backing, bytes).as_ptr() as usize
            };
            (cell, bytes)
        })
        .collect();
    let block = region_base(cells[0].0);
    assert_eq!(
        cells[0].0,
        block + FIRST_CELL_OFFSET,
        "the first cell starts the block"
    );
    for pair in cells.windows(2) {
        let ((cell, bytes), (next, _)) = (pair[0], pair[1]);
        // SAFETY: an allocated cell.
        assert_eq!(
            unsafe { Header::at(cell) }.cell_bytes(),
            crate::space::granule_align(bytes)
        );
        assert_eq!(
            next,
            cell + crate::space::granule_align(bytes),
            "the next cell starts at this one's end"
        );
        assert_eq!(region_base(next), block);
    }
    // SAFETY: an initialized block of this heap.
    let header = unsafe { BlockHeader::at(block) };
    let starts: Vec<usize> = header.cells(block).collect();
    assert_eq!(
        starts,
        cells.iter().map(|&(cell, _)| cell).collect::<Vec<_>>()
    );
    assert_eq!(
        tlab.remaining(),
        block + BLOCK_BYTES - cells.last().unwrap().0 - LARGE_OBJECT_THRESHOLD
    );
}

/// A mutator's block is from-space like any other: with nothing pinned in
/// it the collection frees it, and the pool may hand it to another
/// allocator. The first allocator's epoch is stale, so its next allocation
/// takes a fresh block and never writes into the one it lost.
#[test]
fn a_stale_tlab_takes_a_fresh_block_and_never_writes_into_a_freed_one() {
    let heap = BexHeap::new(vec![]);
    let mut first = Tlab::new(Arc::clone(&heap));
    let lost = region_base(first.alloc_string("garbage").as_ptr() as usize);
    assert!(first.is_valid());

    // SAFETY: no mutator runs; nothing is rooted.
    let (stats, _, _) = unsafe { heap.collect_garbage(&[]) };
    assert_eq!(stats.collected_count, 1);
    assert_eq!(first.remaining(), 0, "a collection ran: the block is stale");
    assert!(!first.is_valid());

    let mut second = Tlab::new_empty(Arc::clone(&heap));
    let taken = region_base(second.alloc_string("second's").as_ptr() as usize);
    assert_eq!(taken, lost, "the freed block is the lowest free one");

    let again = first.alloc_string("first's again");
    let fresh = region_base(again.as_ptr() as usize);
    assert_ne!(fresh, taken, "the stale allocator took a fresh block");
    assert!(first.is_valid() && second.is_valid());
    for _ in 0..100 {
        assert_ne!(
            region_base(first.alloc_string("more").as_ptr() as usize),
            taken
        );
    }
    assert_eq!(string_at(again), "first's again");
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
    let (stats, roots, _) = unsafe { f.heap.collect_garbage(&kept) };
    assert_eq!((stats.live_count, stats.collected_count), (2, 4));
    assert!(roots.iter().zip(&kept).all(|(new, old)| new != old));
    // The kept nodes were copied out too: the block is empty, back in the
    // pool, and the lowest free one, so it is bumped into again from its
    // start.
    let block = region_base(dead[0]);
    // SAFETY: an initialized block of this heap.
    assert!(unsafe { BlockHeader::at(block) }.starts.is_empty());
    let reused: Vec<usize> = (0..4)
        .map(|_| tlab.alloc_zeroed(f.node, NODE.size as usize).as_ptr() as usize)
        .collect();
    let first_four: Vec<usize> = nodes[..4].iter().map(|&ptr| cell_of_ptr(ptr)).collect();
    assert_eq!(reused, first_four);
}

#[test]
fn an_empty_block_returns_to_the_pool_and_is_bumped_into_again() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let first = tlab.alloc_string("only cell");
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
    assert_eq!(
        node,
        block + FIRST_CELL_OFFSET,
        "the emptied block is bumped from its start"
    );
}

/// A block with a pinned cell is retained: no allocator bumps into it, and
/// the first later collection that finds no pin in it copies its survivors
/// out and frees it.
#[test]
fn a_retained_block_is_not_allocated_into_and_is_freed_once_its_pin_goes() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let kept = tlab.alloc_string("kept");
    tlab.alloc_string("garbage");
    let retained = region_base(kept.as_ptr() as usize);
    drop(tlab);
    let stack: [usize; 1] = [kept.as_ptr() as usize + 8];
    let range: Range<*const usize> = stack.as_ptr_range();
    // SAFETY: no mutator runs; the range is readable for the collection.
    let (stats, _) = unsafe { heap.collect(&mut ConservativeRanges(&[range])) };
    assert_eq!((stats.live_count, stats.collected_count), (1, 1));
    assert_eq!(heap.block_occupancy().retained_blocks, 1);

    // Three blocks' worth of allocation, none of it in the retained block,
    // which is the lowest address.
    let mut other = Tlab::new_empty(Arc::clone(&heap));
    let mut blocks = std::collections::BTreeSet::new();
    while blocks.len() < 3 {
        blocks.insert(region_base(
            other.alloc_string("elsewhere").as_ptr() as usize
        ));
    }
    assert!(
        !blocks.contains(&retained),
        "nothing is allocated into a retained block"
    );
    assert!(blocks.iter().all(|&block| block > retained));
    drop(other);

    // SAFETY: no mutator runs; precise roots only, so nothing is pinned.
    let (stats, roots, _) = unsafe { heap.collect_garbage(&[kept]) };
    assert_eq!(stats.live_count, 1);
    assert_ne!(
        region_base(roots[0].as_ptr() as usize),
        retained,
        "copied out"
    );
    assert_eq!(string_at(roots[0]), "kept");
    // SAFETY: an initialized block of this heap.
    assert!(
        unsafe { BlockHeader::at(retained) }.starts.is_empty(),
        "freed whole"
    );
    assert_eq!(heap.block_occupancy().retained_blocks, 0);
    assert_eq!(
        heap.stats().heap_bytes,
        BLOCK_BYTES,
        "only the to-space block is in use"
    );
    let mut next = Tlab::new_empty(Arc::clone(&heap));
    assert_eq!(
        region_base(next.alloc_string("reused").as_ptr() as usize),
        retained,
        "the freed block is the lowest free one again"
    );
}

#[test]
fn large_objects_are_traced_kept_and_freed() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let text = tlab.alloc_string("held by a large backing store");
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let last_element = LARGE_BACKING_BYTES - 8;
    // SAFETY: an element of the fresh large cell.
    unsafe { write_word(large, last_element, text.as_ptr() as usize) };
    assert_ne!(region_base(large), region_base(text.as_ptr() as usize));
    assert_eq!(f.heap.stats().large_objects, 1);

    let root = heap_ptr_at(large);
    // SAFETY: no mutator runs.
    let (stats, roots, _) = unsafe { f.heap.collect_garbage(&[root]) };
    assert_eq!(stats.live_count, 2, "the backing store and its element");
    assert_eq!(roots[0], root, "a large cell never moves");
    // The element moved, and the large cell's slot was repointed in place.
    // SAFETY: the last element of the live large cell.
    let element = heap_ptr_at(unsafe { ((large + last_element) as *const usize).read() });
    assert_ne!(element, text);
    assert_eq!(string_at(element), "held by a large backing store");
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
    let left = tlab.alloc_string("left");
    let by_tag = tlab.alloc_string("by tag");
    let scalar_lookalike = tlab.alloc_string("only looks like a pointer");
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
    let (stats, roots, _) = unsafe {
        f.heap
            .collect_garbage(&[heap_ptr_at(node), heap_ptr_at(other)])
    };
    assert_eq!(
        stats.live_count, 4,
        "two nodes, the pointer and the tagged pointer"
    );
    assert_eq!(stats.collected_count, 1, "the int-tagged lookalike died");
    // Everything moved; the node's slots now name the copies.
    let node = cell_of_ptr(roots[0]);
    // SAFETY: slots of the node's copy.
    let (left_now, by_tag_now) = unsafe {
        (
            heap_ptr_at(((node + NODE_LEFT) as *const usize).read()),
            heap_ptr_at(((node + NODE_TAGGED + 8) as *const usize).read()),
        )
    };
    assert!(left_now != left && by_tag_now != by_tag);
    assert_eq!(string_at(left_now), "left");
    assert_eq!(string_at(by_tag_now), "by tag");
}

#[test]
fn heap_live_and_block_counters_follow_allocation_and_collection() {
    let f = fixture();
    let mut tlab = Tlab::new(Arc::clone(&f.heap));
    let stats = f.heap.stats();
    assert_eq!(stats.live_bytes, 0, "nothing collected yet");
    assert_eq!(stats.heap_bytes, BLOCK_BYTES, "the allocator's block");

    let kept = tlab.alloc_string("kept");
    for i in 0..1_000 {
        tlab.alloc_string(format!("garbage {i}"));
    }
    let large = tlab.alloc_zeroed(f.backing, LARGE_BACKING_BYTES).as_ptr() as usize;
    let stats = f.heap.stats();
    assert!(stats.heap_bytes >= 3 * BLOCK_BYTES + LARGE_BACKING_BYTES);
    assert_eq!(
        stats.heap_bytes,
        (stats.blocks - stats.free_blocks) * BLOCK_BYTES
            + (stats.reserved_bytes - stats.blocks * BLOCK_BYTES),
    );

    let occupancy = f.heap.block_occupancy();
    assert_eq!(occupancy.cells, 1_001, "the large cell is in no block");
    assert_eq!(occupancy.retained_blocks, 0);
    assert_eq!(
        (occupancy.blocks, occupancy.free_blocks),
        (stats.blocks, stats.free_blocks)
    );

    // SAFETY: no mutator runs.
    let (_, roots, _) = unsafe { f.heap.collect_garbage(&[kept, heap_ptr_at(large)]) };
    let kept = roots[0];
    assert_eq!(
        f.heap.stats().live_bytes,
        OBJECT_CELL_BYTES + LARGE_BACKING_BYTES
    );
    assert_eq!(
        f.heap.block_occupancy().cells,
        1,
        "the survivor's copy, in a to-space block"
    );
    assert_eq!(
        f.heap.stats().runtime_objects,
        2,
        "only the survivor's copy and the large cell are still allocated"
    );

    drop(tlab);
    // SAFETY: no mutator runs.
    unsafe { f.heap.collect_garbage(&[kept]) };
    let stats = f.heap.stats();
    assert_eq!(stats.live_bytes, OBJECT_CELL_BYTES);
    assert_eq!(
        stats.heap_bytes, BLOCK_BYTES,
        "one to-space block holds the survivor; the rest are free and the large object is gone"
    );
}
