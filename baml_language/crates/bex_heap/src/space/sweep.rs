//! Reclaiming after the mark: a from-space block with no pin goes back to
//! the pool empty; one with a pin is retained: dead cells and moved cells'
//! old copies lose their start bits and pinned survivors keep theirs. A
//! mutator's current block is from-space like any other; its allocator sees
//! the collection's epoch and takes a fresh block. To-space blocks hold only
//! copies and keep their bits. Only the Rust-valued cells (the block's
//! `hooked` bitmap) are dropped and measured; a dead instance, list or map is
//! never read.
//!
//! A moved cell's old copy is marked and never dropped: its bytes went with
//! the copy, and the copy's destructor runs when that dies.

use crate::descriptor::CellHeader as _;
use bex_vm_types::Meter;

use super::{
    BLOCK_BYTES, GRANULE_BYTES,
    block::{BlockHeader, FIRST_CELL_OFFSET},
    blocks::BlockSpace,
    large::LargeSpace,
    mark::is_cell_marked,
};
use crate::descriptor::{DescriptorTable, Header, OBJECT_DESC, RUST_PAYLOAD_OFFSET, Trace};

/// What a reclaim kept and reclaimed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Swept {
    pub(crate) live_cells: usize,
    pub(crate) dead_cells: usize,
    /// What the survivors keep alive outside their cells, shared storage
    /// counted once.
    pub(crate) live_payload_bytes: usize,
}

/// The byte every reclaimed from-space cell is filled with under
/// [`poison`](super::gc_poison): a read through a stale pointer finds this,
/// not the object's last state.
pub(crate) const POISON_BYTE: u8 = 0xD5;

/// Reclaim every block and large object.
///
/// # Safety
/// No mutator runs, and the marks are those of a complete mark.
pub(crate) unsafe fn reclaim(
    blocks: &mut BlockSpace,
    large: &mut LargeSpace,
    descriptors: &DescriptorTable,
    poison: bool,
) -> Swept {
    let mut census = Meter::census();
    let mut swept = Swept::default();
    for &base in blocks.pool().blocks() {
        // SAFETY: forwarded to the caller.
        unsafe { reclaim_block(base, descriptors, poison, &mut census, &mut swept) };
    }
    // SAFETY: each dead large cell is dropped exactly once, before freeing.
    swept.dead_cells += large.sweep(|cell| unsafe { drop_cell(descriptors, cell, poison) });
    for cell in large.cells() {
        swept.live_cells += 1;
        // SAFETY: a marked, live cell.
        unsafe { measure_cell(descriptors, cell, &mut census) };
    }
    // SAFETY: forwarded to the caller.
    unsafe { blocks.end_sweep() };
    swept.live_payload_bytes = census.total();
    swept
}

/// # Safety
/// As for [`reclaim`]; `base` is a block of the space.
unsafe fn reclaim_block(
    base: usize,
    descriptors: &DescriptorTable,
    poison: bool,
    census: &mut Meter,
    swept: &mut Swept,
) {
    // SAFETY: the pool's blocks are initialized.
    let header = unsafe { BlockHeader::at(base) };
    if header.starts.is_empty() {
        return;
    }
    if !header.is_from_space() {
        // To-space of this collection: every cell is a live copy.
        swept.live_cells += header.starts.count_ones();
        for granule in header.hooked.ones() {
            // SAFETY: a copy of a marked Rust-valued cell.
            unsafe { measure_cell(descriptors, base + granule * GRANULE_BYTES, census) };
        }
        return;
    }
    swept.dead_cells += header.starts.count_ones_not_in(&header.marks);
    for granule in header.hooked.ones_not_in(&header.marks) {
        // SAFETY: an allocated Rust-valued cell the mark did not reach.
        unsafe { drop_cell(descriptors, base + granule * GRANULE_BYTES, poison) };
    }
    let retained = !header.pins.is_empty();
    if !retained {
        // Every cell is dead or moved: the block goes back to the pool.
        if poison {
            // SAFETY: the block's cell area, which nothing live occupies.
            unsafe { poison_bytes(base + FIRST_CELL_OFFSET, base + BLOCK_BYTES) };
        }
        header.starts.clear_all();
        header.hooked.clear_all();
        header.marks.clear_all();
        return;
    }
    // A pinned survivor is marked in place; a marked, unpinned cell is a
    // moved cell's old copy, whose slot is free.
    swept.live_cells += header.marks.count_ones_in(&header.pins);
    for granule in header.hooked.ones_in_both(&header.marks, &header.pins) {
        // SAFETY: a marked Rust-valued cell, in place.
        unsafe { measure_cell(descriptors, base + granule * GRANULE_BYTES, census) };
    }
    if poison {
        // A moved cell's old header is its forwarding address, not its size:
        // each reclaimed cell extends to the next start, or the block's end.
        let starts: Vec<usize> = header.starts.ones().collect();
        for (index, &granule) in starts.iter().enumerate() {
            if header.marks.get(granule) && header.pins.get(granule) {
                continue;
            }
            let end = starts
                .get(index + 1)
                .map_or(base + BLOCK_BYTES, |&next| base + next * GRANULE_BYTES);
            // SAFETY: a dead or moved cell and the free slots after it.
            unsafe { poison_bytes(base + granule * GRANULE_BYTES, end) };
        }
    }
    header.starts.retain_in_both(&header.marks, &header.pins);
    header.hooked.retain_in_both(&header.marks, &header.pins);
}

/// Fill `start..end` with [`POISON_BYTE`].
///
/// # Safety
/// `start..end` is writable and holds nothing live.
unsafe fn poison_bytes(start: usize, end: usize) {
    // SAFETY: forwarded to the caller.
    unsafe { (start as *mut u8).write_bytes(POISON_BYTE, end - start) };
}

/// Run a dead cell's destructor.
///
/// # Safety
/// `cell` is allocated, unmarked, and never read again.
unsafe fn drop_cell(descriptors: &DescriptorTable, cell: usize, poison: bool) {
    // SAFETY: allocated, so its header is written.
    let id = unsafe { Header::at(cell) }.descriptor;
    debug_assert!(!unsafe { is_cell_marked(cell) });
    let Trace::Rust(hooks) = descriptors.get(id).trace else {
        return;
    };
    let payload = (cell + RUST_PAYLOAD_OFFSET) as *mut u8;
    // SAFETY: forwarded to the caller.
    unsafe { (hooks.drop)(payload) };
    if cfg!(feature = "heap_debug") && !poison && id == OBJECT_DESC {
        // A read through a stale pointer finds a tombstone, not freed state.
        // Under poisoning the cell's bytes are filled over instead.
        // SAFETY: the dropped value's storage is still the cell's.
        unsafe {
            payload
                .cast::<bex_vm_types::Object>()
                .write(bex_vm_types::Object::Tombstone)
        };
    }
}

/// Add what a live cell keeps alive outside itself to `census`.
///
/// # Safety
/// `cell` is allocated, intact, and no mutator runs.
unsafe fn measure_cell(descriptors: &DescriptorTable, cell: usize, census: &mut Meter) {
    // SAFETY: allocated, so its header is written.
    let Trace::Rust(hooks) = descriptors
        .get(unsafe { Header::at(cell) }.descriptor)
        .trace
    else {
        return;
    };
    // SAFETY: forwarded to the caller.
    unsafe { (hooks.measure)((cell + RUST_PAYLOAD_OFFSET) as *mut u8, census) };
}
