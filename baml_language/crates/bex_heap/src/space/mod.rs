//! The space: mark-region blocks the collector copies out of.
//!
//! Small objects live in 32 KiB [blocks](block), at 16-byte granules. Each
//! block starts with a header holding four side tables, an object-start
//! bitmap, a mark bitmap, a pin bitmap and a hooked bitmap (one bit per
//! granule). A block is bump-filled with cells of mixed sizes, once, by a
//! mutator or by the collector as to-space ([`blocks`]). Cells above
//! [`LARGE_OBJECT_THRESHOLD`] get an allocation of their own ([`large`]) and
//! never move. A collection ([`mark`]) copies every unpinned live small cell
//! into to-space and repoints every pointer to it; a cell a conservative
//! root names is pinned and stays, and so does its block ([`sweep`]).
//!
//! Every allocation, small or large, is a *cell*: a
//! [`Header`](crate::Header) followed by the object's bytes. A pointer to an
//! object names a byte in its cell's first granule (the header, or the byte
//! right after it), so clearing a pointer's low four bits yields its cell.

pub(crate) mod bitmap;
pub(crate) mod block;
pub(crate) mod blocks;
pub(crate) mod large;
pub(crate) mod pool;

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
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "the collector reads the bitmaps directly")
)]
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

/// Whether a collection fills every reclaimed from-space cell with
/// [`sweep::POISON_BYTE`], so a use-after-move reads garbage rather than a
/// stale copy: always under `heap_debug`, else under `BAML_GC_STRESS=1`.
pub(crate) fn gc_poison() -> bool {
    use std::sync::OnceLock;
    static STRESS: OnceLock<bool> = OnceLock::new();
    #[cfg(test)]
    if POISON_FOR_TESTS.load(std::sync::atomic::Ordering::Relaxed) {
        return true;
    }
    cfg!(feature = "heap_debug")
        || *STRESS.get_or_init(|| baml_env::os_var("BAML_GC_STRESS").is_some_and(|v| v == "1"))
}

/// Turn poisoning on for every later collection in this test process.
#[cfg(test)]
pub(crate) static POISON_FOR_TESTS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
