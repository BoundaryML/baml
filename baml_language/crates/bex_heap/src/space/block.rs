//! One 32 KiB block and the side tables at its start.

use super::{
    BLOCK_BYTES, GRANULE_BYTES, GRANULES_PER_BLOCK, LARGE_OBJECT_THRESHOLD, RegionKind,
    bitmap::Bitmap, granule_align, small::BlockMeta,
};
use crate::descriptor::CellHeader as _;
use crate::descriptor::Header;

pub(crate) const GRANULE_WORDS: usize = GRANULES_PER_BLOCK / u64::BITS as usize;

/// The most granules a small cell spans. Bounds the backward search for a
/// cell start.
const MAX_SMALL_CELL_GRANULES: usize = LARGE_OBJECT_THRESHOLD / GRANULE_BYTES;

/// Side tables, written at the block's base. Cells start after them.
#[repr(C)]
pub(crate) struct BlockHeader {
    kind: RegionKind,
    /// One bit per granule: a cell starts here.
    pub(crate) starts: Bitmap<GRANULE_WORDS>,
    /// One bit per granule: the cell starting here was marked live.
    pub(crate) marks: Bitmap<GRANULE_WORDS>,
    /// One bit per granule: a conservative root named the cell starting
    /// here during the latest collection, so it was marked in place rather
    /// than copied, and its block was retained.
    pub(crate) pins: Bitmap<GRANULE_WORDS>,
    /// One bit per granule: the cell starting here is a Rust value
    /// ([`Trace::Rust`](crate::Trace::Rust)) with a destructor and outside
    /// storage. Only these cells' descriptors are read by the sweep; a dead
    /// instance, list or map is reclaimed from the bitmaps alone.
    pub(crate) hooked: Bitmap<GRANULE_WORDS>,
    /// The small-object policy's state for this block.
    pub(crate) policy: BlockMeta,
}

/// Where a block's first cell goes; the header is before it.
pub(crate) const FIRST_CELL_OFFSET: usize = granule_align(size_of::<BlockHeader>());
/// Bytes of a block cells may use.
pub(crate) const USABLE_BLOCK_BYTES: usize = BLOCK_BYTES - FIRST_CELL_OFFSET;
pub(crate) const FIRST_GRANULE: usize = FIRST_CELL_OFFSET / GRANULE_BYTES;

// Four granule bitmaps and the policy state: a sixteenth of the block at most.
const _: () = assert!(FIRST_CELL_OFFSET <= BLOCK_BYTES / 16);
const _: () = assert!(LARGE_OBJECT_THRESHOLD < USABLE_BLOCK_BYTES);

impl BlockHeader {
    /// Write a fresh header at `base`.
    ///
    /// # Safety
    /// `base` is the start of `BLOCK_BYTES` writable bytes aligned to
    /// `BLOCK_BYTES` that nothing else uses.
    pub(crate) unsafe fn init(base: usize) {
        let header = Self {
            kind: RegionKind::Block,
            starts: Bitmap::default(),
            marks: Bitmap::default(),
            pins: Bitmap::default(),
            hooked: Bitmap::default(),
            policy: BlockMeta::default(),
        };
        // SAFETY: the caller hands over the block's memory.
        unsafe { (base as *mut Self).write(header) };
    }

    /// The header of the block at `base`.
    ///
    /// # Safety
    /// `base` is a block initialized by [`Self::init`] and not yet freed.
    #[inline]
    pub(crate) unsafe fn at<'a>(base: usize) -> &'a Self {
        // SAFETY: the caller guarantees an initialized block.
        unsafe { &*(base as *const Self) }
    }

    /// The granule index of `address` within the block at `base`.
    #[inline]
    pub(crate) const fn granule(base: usize, address: usize) -> usize {
        (address - base) / GRANULE_BYTES
    }

    /// Forget the previous collection's marks.
    pub(crate) fn clear_marks(&self) {
        self.marks.clear_all();
    }

    /// Forget the previous collection's pins.
    pub(crate) fn clear_pins(&self) {
        self.pins.clear_all();
    }

    /// Pin the cell `cell` of the block at `base`: a conservative root
    /// names it.
    #[inline]
    pub(crate) fn pin(&self, base: usize, cell: usize) {
        self.pins.set(Self::granule(base, cell));
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "the collector reads the bitmap directly"))]
    #[inline]
    pub(crate) fn is_pinned(&self, base: usize, cell: usize) -> bool {
        self.pins.get(Self::granule(base, cell))
    }

    /// The cell of the block at `base` whose bytes contain `address`, if
    /// any. Used for conservative roots, which may point anywhere.
    ///
    /// # Safety
    /// `base` is this block's address, and every set start bit names a cell
    /// with an initialized header.
    pub(crate) unsafe fn cell_containing(&self, base: usize, address: usize) -> Option<usize> {
        let granule = Self::granule(base, address);
        if granule < FIRST_GRANULE {
            return None;
        }
        let floor = granule
            .saturating_sub(MAX_SMALL_CELL_GRANULES - 1)
            .max(FIRST_GRANULE);
        let start = self.starts.last_one_in(floor, granule)?;
        let cell = base + start * GRANULE_BYTES;
        // SAFETY: a set start bit names a cell with a written header.
        let bytes = unsafe { Header::at(cell) }.cell_bytes();
        (address < cell + bytes).then_some(cell)
    }

    /// The cell of the block at `base` whose bytes end exactly at
    /// `address`, if any: a one-past-the-end pointer still names its cell.
    /// One lookback from the granule before `address`.
    ///
    /// # Safety
    /// As for [`Self::cell_containing`].
    pub(crate) unsafe fn cell_ending_at(&self, base: usize, address: usize) -> Option<usize> {
        if !address.is_multiple_of(GRANULE_BYTES) {
            return None;
        }
        let granule = Self::granule(base, address);
        if granule <= FIRST_GRANULE {
            return None;
        }
        let last = granule - 1;
        let floor = last
            .saturating_sub(MAX_SMALL_CELL_GRANULES - 1)
            .max(FIRST_GRANULE);
        let start = self.starts.last_one_in(floor, last)?;
        let cell = base + start * GRANULE_BYTES;
        // SAFETY: a set start bit names a cell with a written header.
        let bytes = unsafe { Header::at(cell) }.cell_bytes();
        (address == cell + bytes).then_some(cell)
    }

    /// Every Rust-valued cell in the block, ascending.
    pub(crate) fn hooked_cells(&self, base: usize) -> impl Iterator<Item = usize> + '_ {
        self.hooked
            .ones()
            .map(move |granule| base + granule * GRANULE_BYTES)
    }

    /// Every cell in the block, ascending.
    pub(crate) fn cells(&self, base: usize) -> impl Iterator<Item = usize> + '_ {
        self.starts
            .ones()
            .map(move |granule| base + granule * GRANULE_BYTES)
    }
}
