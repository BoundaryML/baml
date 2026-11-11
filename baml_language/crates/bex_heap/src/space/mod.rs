//! The non-moving mark-sweep space.
//!
//! Small objects live in 32 KiB [blocks](block), at 16-byte granules. Each
//! block starts with a header holding three side tables, an object-start
//! bitmap, a mark bitmap and a pin bitmap (one bit per granule), and the state of the
//! small-object policy ([`small`]: size classes, one class per block, a free
//! list per block). Cells above [`LARGE_OBJECT_THRESHOLD`] get an allocation
//! of their own ([`large`]). Nothing ever moves.
//!
//! Every allocation, small or large, is a *cell*: a
//! [`Header`](crate::Header) followed by the object's bytes. A pointer to an
//! object names a byte in its cell's first granule (the header, or the byte
//! right after it), so clearing a pointer's low four bits yields its cell.

pub(crate) mod bitmap;
pub(crate) mod block;
pub(crate) mod large;
pub(crate) mod pool;
pub(crate) mod small;

/// Bytes in one block. Blocks are aligned to their size, so masking an
/// address finds the block (or large-object region) holding it.
pub const BLOCK_BYTES: usize = 32 * 1024;
/// Allocation alignment and the resolution of the start and mark bitmaps.
pub const GRANULE_BYTES: usize = 16;
/// Cells (header included) larger than this are allocated on their own,
/// outside blocks.
pub const LARGE_OBJECT_THRESHOLD: usize = 8 * 1024;

pub(crate) const GRANULES_PER_BLOCK: usize = BLOCK_BYTES / GRANULE_BYTES;

const _: () = assert!(LARGE_OBJECT_THRESHOLD < BLOCK_BYTES / 2);

/// The block or large-object region holding `address`.
#[inline]
pub(crate) const fn region_base(address: usize) -> usize {
    address & !(BLOCK_BYTES - 1)
}

/// The cell whose first granule holds `address`.
#[inline]
pub(crate) const fn cell_of(address: usize) -> usize {
    address & !(GRANULE_BYTES - 1)
}

/// `bytes` rounded up to whole granules.
#[inline]
pub(crate) const fn granule_align(bytes: usize) -> usize {
    (bytes + GRANULE_BYTES - 1) & !(GRANULE_BYTES - 1)
}

/// What a block-aligned region is. Stored in its first byte so a pointer
/// known to name a live object can find its metadata with one load.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Block = 0xB1,
    Large = 0x1A,
}

/// The kind of the region at `base`.
///
/// # Safety
/// `base` is a live block or large-object region.
#[inline]
pub(crate) unsafe fn region_kind(base: usize) -> RegionKind {
    // SAFETY: both region headers are `repr(C)` with the kind first.
    unsafe { *(base as *const RegionKind) }
}

pub(crate) mod mark;
pub(crate) mod sweep;

/// Pin the allocated cell `cell`: a conservative root names it, so it must
/// not move this collection.
///
/// # Safety
/// `cell` is an allocated cell.
#[inline]
pub(crate) unsafe fn pin_cell(cell: usize) {
    let base = region_base(cell);
    // SAFETY: forwarded to the caller.
    match unsafe { region_kind(base) } {
        // SAFETY: as above.
        RegionKind::Block => unsafe { block::BlockHeader::at(base) }.pin(base, cell),
        // SAFETY: as above.
        RegionKind::Large => unsafe { large::LargeSpace::pin(cell) },
    }
}

/// Whether the allocated cell `cell` was pinned by the latest collection.
///
/// # Safety
/// `cell` is an allocated cell.
#[cfg_attr(not(test), allow(dead_code, reason = "read once the collector moves cells"))]
pub(crate) unsafe fn is_cell_pinned(cell: usize) -> bool {
    let base = region_base(cell);
    // SAFETY: forwarded to the caller.
    match unsafe { region_kind(base) } {
        // SAFETY: as above.
        RegionKind::Block => unsafe { block::BlockHeader::at(base) }.is_pinned(base, cell),
        // SAFETY: as above.
        RegionKind::Large => unsafe { large::LargeSpace::is_pinned(cell) },
    }
}

/// Forget every pin of the previous collection, in every block and large
/// cell.
pub(crate) fn clear_pins(pool: &pool::BlockPool, large: &large::LargeSpace) {
    for &base in pool.blocks() {
        // SAFETY: the pool's blocks are initialized.
        unsafe { block::BlockHeader::at(base) }.clear_pins();
    }
    large.clear_pins();
}

/// The size of the cell `cell`.
///
/// # Safety
/// `cell` is an allocated cell.
#[inline]
pub(crate) unsafe fn cell_bytes(cell: usize) -> usize {
    // SAFETY: forwarded to the caller.
    match unsafe { region_kind(region_base(cell)) } {
        // SAFETY: an allocated small cell has a written header.
        RegionKind::Block => {
            use crate::descriptor::CellHeader;
            // SAFETY: as above.
            unsafe { crate::descriptor::Header::at(cell) }.cell_bytes()
        }
        // SAFETY: an allocated large cell has a live prefix.
        RegionKind::Large => unsafe { large::LargeSpace::cell_bytes(cell) },
    }
}

#[cfg(test)]
mod tests;
