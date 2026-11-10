//! Where blocks come from: slabs of [`BLOCKS_PER_SLAB`] blocks, allocated
//! block-aligned and kept for the heap's lifetime. Which blocks are handed
//! out, and when, is the [small-object policy](super::small)'s business.

use std::alloc::{Layout, alloc, dealloc};

use super::{BLOCK_BYTES, block::BlockHeader, region_base};

/// Blocks per backing allocation: 1 MiB, so slabs are few and a lookup that
/// binary-searches them stays short.
const BLOCKS_PER_SLAB: usize = 32;
const SLAB_BYTES: usize = BLOCKS_PER_SLAB * BLOCK_BYTES;

#[derive(Default)]
pub(crate) struct BlockPool {
    /// Slab addresses, sorted.
    slabs: Vec<usize>,
    /// Every block, in the order slabs were carved.
    blocks: Vec<usize>,
}

fn slab_layout() -> Layout {
    Layout::from_size_align(SLAB_BYTES, BLOCK_BYTES).expect("slab layout is valid")
}

impl BlockPool {
    /// Carve a slab into initialized blocks. Returns them, ascending.
    pub(crate) fn grow(&mut self) -> impl DoubleEndedIterator<Item = usize> + use<> {
        let layout = slab_layout();
        // SAFETY: the layout has nonzero size.
        let base = unsafe { alloc(layout) } as usize;
        if base == 0 {
            std::alloc::handle_alloc_error(layout);
        }
        let position = self.slabs.partition_point(|&slab| slab < base);
        self.slabs.insert(position, base);
        let blocks = (0..BLOCKS_PER_SLAB).map(move |index| base + index * BLOCK_BYTES);
        for block in blocks.clone() {
            // SAFETY: a fresh, block-aligned part of the slab just allocated.
            unsafe { BlockHeader::init(block) };
        }
        self.blocks.extend(blocks.clone());
        blocks
    }

    /// The block holding `address`, if a slab of this pool holds it.
    pub(crate) fn block_containing(&self, address: usize) -> Option<usize> {
        let position = self.slabs.partition_point(|&slab| slab <= address);
        let slab = *self.slabs.get(position.checked_sub(1)?)?;
        (address < slab + SLAB_BYTES).then(|| region_base(address))
    }

    pub(crate) fn blocks(&self) -> &[usize] {
        &self.blocks
    }
}

impl Drop for BlockPool {
    fn drop(&mut self) {
        for &slab in &self.slabs {
            // SAFETY: allocated in `grow` with this layout; cells were dropped
            // by the heap before the pool.
            unsafe { dealloc(slab as *mut u8, slab_layout()) };
        }
    }
}
