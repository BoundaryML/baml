//! A space of variable-size cells: runs of 8-byte words bump-allocated from
//! zeroed chunks. Every runtime object lives in one (see
//! `bex_vm_types/src/types/cell.rs` for the cell format).
//!
//! A space is reclaimed by dropping its chunks. An object cell whose
//! [`Object`] owns memory outside the space is dropped first, by the
//! collector, unless it was copied elsewhere. A cell's header is never zero,
//! so the zero words a space leaves unused (a chunk's tail, the rest of a
//! TLAB's block) are told apart from cells by reading them.

use std::sync::atomic::{AtomicU64, Ordering};

use bex_vm_types::{FORWARDED, HEADER_FLAGS, InstanceRef, OBJECT_BODY, Object};

/// What a cell holds, read from its header.
#[derive(Clone, Copy)]
pub(crate) enum CellBody {
    Instance(InstanceRef),
    Object(*mut Object),
}

impl CellBody {
    /// The body of the cell starting at `cell`.
    ///
    /// # Safety
    ///
    /// `cell` starts a cell whose header has been written and that the
    /// collector has not forwarded: a forwarded instance's header holds its
    /// new address, not its class.
    pub(crate) unsafe fn at(cell: *mut AtomicU64) -> Self {
        // SAFETY: the caller guarantees `cell` starts a cell.
        let header = unsafe { &*cell }.load(Ordering::Relaxed);
        if header & OBJECT_BODY != 0 {
            Self::Object(cell.cast())
        } else {
            // SAFETY: as above, and its header says it is an instance.
            Self::Instance(unsafe { InstanceRef::from_cell(cell) })
        }
    }
}

/// Whether the collector copied the cell starting at `cell` elsewhere.
///
/// # Safety
///
/// `cell` starts a cell.
pub(crate) unsafe fn is_forwarded(cell: *mut AtomicU64) -> bool {
    // SAFETY: the caller guarantees `cell` starts a cell.
    unsafe { &*cell }.load(Ordering::Relaxed) & FORWARDED != 0
}

/// Record that the cell starting at `cell` was copied to `to`: its header
/// becomes `to`'s address with [`FORWARDED`] set, keeping [`OBJECT_BODY`]
/// so the cell's kind can still be read.
///
/// # Safety
///
/// `cell` starts a cell, and `to` starts its copy.
pub(crate) unsafe fn forward_to(cell: *mut AtomicU64, to: *mut AtomicU64) {
    // SAFETY: the caller guarantees `cell` starts a cell.
    let header = unsafe { &*cell };
    let kind = header.load(Ordering::Relaxed) & OBJECT_BODY;
    header.store(
        to.expose_provenance() as u64 | FORWARDED | kind,
        Ordering::Relaxed,
    );
}

/// Where the forwarded cell starting at `cell` was copied, and whether it
/// is an object cell.
///
/// # Safety
///
/// `cell` starts a cell the collector has forwarded.
pub(crate) unsafe fn forwarding_address(cell: *mut AtomicU64) -> (*mut AtomicU64, bool) {
    // SAFETY: the caller guarantees `cell` starts a forwarded cell.
    let header = unsafe { &*cell }.load(Ordering::Relaxed);
    debug_assert!(header & FORWARDED != 0, "{cell:?} was not forwarded");
    let addr = usize::try_from(header & !HEADER_FLAGS).expect("an address fits a usize");
    let to = std::ptr::with_exposed_provenance_mut(addr);
    (to, header & OBJECT_BODY != 0)
}

/// A place in a space's carved words, for a walk that the walker itself
/// extends: the collector scans the space it copies into.
#[derive(Clone, Copy, Default)]
pub(crate) struct Cursor {
    chunk: usize,
    offset: usize,
}

impl Cursor {
    /// Step over the `words` of the cell the cursor is at.
    pub(crate) fn advance(&mut self, words: usize) {
        self.offset += words;
    }
}

/// Write an object cell: `object`, whose tag is the cell's header. Returns
/// the address of the [`Object`], which is the cell's.
///
/// # Safety
///
/// `cell` starts [`OBJECT_CELL_WORDS`](bex_vm_types::OBJECT_CELL_WORDS)
/// words nothing else uses.
pub(crate) unsafe fn write_object_cell(cell: *mut AtomicU64, object: Object) -> *mut Object {
    let body = cell.cast::<Object>();
    // SAFETY: the caller gives this cell's words to us alone.
    unsafe { body.write(object) };
    body
}

/// Words in an ordinary chunk; a larger cell gets a chunk of its own size.
pub(crate) const CHUNK_WORDS: usize = 32 * 1024;

const _: () = assert!(
    align_of::<AtomicU64>() == align_of::<u64>(),
    "a zeroed `u64` buffer must be a valid `AtomicU64` buffer"
);

#[derive(Default)]
pub(crate) struct CellSpace {
    chunks: Vec<Box<[AtomicU64]>>,
    /// Words handed out from the last chunk.
    used: usize,
}

impl CellSpace {
    /// Reserve `words` zeroed words, returning the first.
    pub(crate) fn carve(&mut self, words: usize) -> *mut AtomicU64 {
        let fits = self
            .chunks
            .last()
            .is_some_and(|chunk| chunk.len() - self.used >= words);
        if !fits {
            self.chunks.push(zeroed_chunk(words.max(CHUNK_WORDS)));
            self.used = 0;
        }
        let chunk = self.chunks.last().expect("a chunk was just ensured");
        let start = chunk[self.used..].as_ptr().cast_mut();
        self.used += words;
        start
    }

    /// Where `word` lies: its chunk and its offset there, in allocation
    /// order; `None` when it is not in this space.
    pub(crate) fn position(&self, word: *const AtomicU64) -> Option<(usize, usize)> {
        self.chunks.iter().enumerate().find_map(|(index, chunk)| {
            let offset = word.addr().checked_sub(chunk.as_ptr().addr())? / size_of::<u64>();
            (offset < chunk.len()).then_some((index, offset))
        })
    }

    pub(crate) fn contains(&self, word: *const AtomicU64) -> bool {
        self.position(word).is_some()
    }

    /// The first cell at or after `cursor`, moving the cursor there; `None`
    /// once it has passed every word carved so far. Words carved later are
    /// found by a later call.
    pub(crate) fn next_cell(&self, cursor: &mut Cursor) -> Option<*mut AtomicU64> {
        loop {
            let chunk = self.chunks.get(cursor.chunk)?;
            let last = cursor.chunk + 1 == self.chunks.len();
            let end = if last { self.used } else { chunk.len() };
            while cursor.offset < end {
                if chunk[cursor.offset].load(Ordering::Relaxed) != 0 {
                    return Some(chunk[cursor.offset..].as_ptr().cast_mut());
                }
                cursor.offset += 1;
            }
            if last {
                return None;
            }
            cursor.chunk += 1;
            cursor.offset = 0;
        }
    }

    /// Give up every chunk, leaving this space empty.
    pub(crate) fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Visit every cell in allocation order. `visit` returns the cell's size
    /// in words.
    pub(crate) fn for_each_cell(&self, mut visit: impl FnMut(*mut AtomicU64) -> usize) {
        for chunk in &self.chunks {
            let mut offset = 0;
            while offset < chunk.len() {
                if chunk[offset].load(Ordering::Relaxed) == 0 {
                    offset += 1;
                    continue;
                }
                offset += visit(chunk[offset..].as_ptr().cast_mut());
            }
        }
    }

    /// Words carved so far, counting the unused tail of every full chunk.
    pub(crate) fn used_words(&self) -> usize {
        let full: usize = self
            .chunks
            .iter()
            .rev()
            .skip(1)
            .map(|chunk| chunk.len())
            .sum();
        full + self.used
    }

    /// Words in every chunk.
    #[cfg(feature = "gc_profiling")]
    pub(crate) fn capacity_words(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.len()).sum()
    }

    /// Chunks in this space.
    #[cfg(test)]
    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) fn clear(&mut self) {
        self.chunks.clear();
        self.used = 0;
    }
}

fn zeroed_chunk(words: usize) -> Box<[AtomicU64]> {
    let raw = Box::into_raw(vec![0u64; words].into_boxed_slice());
    // SAFETY: `AtomicU64` has `u64`'s size, alignment (asserted above) and bit
    // validity, so the buffer is a valid `[AtomicU64]` owned by the box.
    #[allow(unsafe_code)]
    unsafe {
        Box::from_raw(raw as *mut [AtomicU64])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_visited_in_order_across_chunks() {
        let mut space = CellSpace::default();
        let small = space.carve(3);
        let large = space.carve(CHUNK_WORDS + 1);
        let after = space.carve(2);
        for cell in [small, large, after] {
            // SAFETY: each cell was just carved.
            #[allow(unsafe_code)]
            unsafe { &*cell }.store(1, Ordering::Relaxed);
        }
        let sizes = [(small, 3), (large, CHUNK_WORDS + 1), (after, 2)];
        let mut visited = Vec::new();
        space.for_each_cell(|cell| {
            let (_, words) = sizes.iter().find(|(start, _)| *start == cell).unwrap();
            visited.push(cell);
            *words
        });
        assert_eq!(visited, vec![small, large, after]);
        assert!(space.position(small) < space.position(after));
        assert!(space.contains(large));

        space.clear();
        assert!(!space.contains(small));
    }
}
