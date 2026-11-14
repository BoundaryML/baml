//! A space of variable-size cells: runs of 8-byte words bump-allocated from
//! zeroed chunks. Class instances live here (see [`bex_vm_types::InstanceRef`]
//! for the cell format).
//!
//! A cell owns nothing outside its words, so a space is reclaimed by
//! dropping its chunks: no cell is ever dropped one by one. A cell's header is
//! never zero, so the zero words a space leaves unused (a chunk's tail, the
//! rest of a TLAB's block) are told apart from cells by reading them.

use std::sync::atomic::{AtomicU64, Ordering};

/// Words in an ordinary chunk; a larger cell gets a chunk of its own size.
const CHUNK_WORDS: usize = 32 * 1024;

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

    pub(crate) fn clear(&mut self) {
        self.chunks.clear();
        self.used = 0;
    }
}

fn zeroed_chunk(words: usize) -> Box<[AtomicU64]> {
    // `vec!` of zeros asks the allocator for zeroed memory, which it can
    // supply without writing it.
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
