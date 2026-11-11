//! Sweeping: clear every unmarked cell's start bit, drop and measure only the
//! Rust-valued cells (the block's `hooked` bitmap), then let the size-class
//! policy rewind its cursors. A dead instance, list or map is never read: its
//! header and descriptor are not looked up, and nothing is written into it.

use crate::descriptor::CellHeader as _;
use bex_vm_types::Meter;

use super::{
    GRANULE_BYTES, block::BlockHeader, large::LargeSpace, mark::is_cell_marked, small::SmallSpace,
};
use crate::descriptor::{DescriptorTable, Header, OBJECT_DESC, RUST_PAYLOAD_OFFSET, Trace};

/// What a sweep kept and reclaimed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Swept {
    pub(crate) live_cells: usize,
    pub(crate) dead_cells: usize,
    /// What the survivors keep alive outside their cells, shared storage
    /// counted once.
    pub(crate) live_payload_bytes: usize,
}

/// Sweep every block and large object.
///
/// # Safety
/// No mutator runs, and the marks are those of a complete mark.
pub(crate) unsafe fn sweep(
    small: &mut SmallSpace,
    large: &mut LargeSpace,
    descriptors: &DescriptorTable,
) -> Swept {
    let mut census = Meter::census();
    let mut swept = Swept::default();
    for &base in small.pool().blocks() {
        // SAFETY: forwarded to the caller.
        unsafe { sweep_block(base, descriptors, &mut census, &mut swept) };
    }
    // SAFETY: each dead large cell is dropped exactly once, before freeing.
    swept.dead_cells += large.sweep(|cell| unsafe { drop_cell(descriptors, cell) });
    for cell in large.cells() {
        swept.live_cells += 1;
        // SAFETY: a marked, live cell.
        unsafe { measure_cell(descriptors, cell, &mut census) };
    }
    // SAFETY: forwarded to the caller.
    unsafe { small.end_sweep() };
    swept.live_payload_bytes = census.total();
    swept
}

/// # Safety
/// As for [`sweep`]; `base` is a block of the space.
unsafe fn sweep_block(
    base: usize,
    descriptors: &DescriptorTable,
    census: &mut Meter,
    swept: &mut Swept,
) {
    // SAFETY: the pool's blocks are initialized.
    let header = unsafe { BlockHeader::at(base) };
    if header.starts.is_empty() {
        return;
    }
    swept.dead_cells += header.starts.count_ones_not_in(&header.marks);
    swept.live_cells += header.marks.count_ones();
    for granule in header.hooked.ones_not_in(&header.marks) {
        // SAFETY: an allocated Rust-valued cell the mark did not reach.
        unsafe { drop_cell(descriptors, base + granule * GRANULE_BYTES) };
    }
    for granule in header.hooked.ones_in(&header.marks) {
        // SAFETY: a marked Rust-valued cell.
        unsafe { measure_cell(descriptors, base + granule * GRANULE_BYTES, census) };
    }
    header.starts.retain(&header.marks);
    header.hooked.retain(&header.marks);
}

/// Run a dead cell's destructor.
///
/// # Safety
/// `cell` is allocated, unmarked, and never read again.
unsafe fn drop_cell(descriptors: &DescriptorTable, cell: usize) {
    // SAFETY: allocated, so its header is written.
    let id = unsafe { Header::at(cell) }.descriptor;
    debug_assert!(!unsafe { is_cell_marked(cell) });
    let Trace::Rust(hooks) = descriptors.get(id).trace else {
        return;
    };
    let payload = (cell + RUST_PAYLOAD_OFFSET) as *mut u8;
    // SAFETY: forwarded to the caller.
    unsafe { (hooks.drop)(payload) };
    if cfg!(feature = "heap_debug") && id == OBJECT_DESC {
        // A read through a stale pointer finds a tombstone, not freed state.
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
/// `cell` is allocated and no mutator runs.
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
