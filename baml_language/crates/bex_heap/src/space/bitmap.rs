//! Fixed-size bitmaps for block side tables.
//!
//! Words are atomics accessed with `Relaxed` loads and stores, never
//! read-modify-write: each bitmap has one writer at a time (the mutator that
//! owns the block, or the collector while every mutator is parked), and the
//! heap permit hand-off orders one writer after the next. The atomics only
//! make a concurrent statistics read well-defined; they compile to plain
//! loads and stores.

use std::sync::atomic::{AtomicU64, Ordering};

const WORD_BITS: usize = u64::BITS as usize;

pub(crate) struct Bitmap<const WORDS: usize> {
    words: [AtomicU64; WORDS],
}

impl<const WORDS: usize> Bitmap<WORDS> {
    #[inline]
    fn word(&self, word: usize) -> u64 {
        self.words[word].load(Ordering::Relaxed)
    }

    #[inline]
    fn store(&self, word: usize, value: u64) {
        self.words[word].store(value, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn get(&self, bit: usize) -> bool {
        self.word(bit / WORD_BITS) & (1 << (bit % WORD_BITS)) != 0
    }

    #[inline]
    pub(crate) fn set(&self, bit: usize) {
        let word = bit / WORD_BITS;
        self.store(word, self.word(word) | (1 << (bit % WORD_BITS)));
    }

    pub(crate) fn clear_all(&self) {
        for word in 0..WORDS {
            self.store(word, 0);
        }
    }

    pub(crate) fn count_ones(&self) -> usize {
        (0..WORDS)
            .map(|word| self.word(word).count_ones() as usize)
            .sum()
    }

    pub(crate) fn is_empty(&self) -> bool {
        (0..WORDS).all(|word| self.word(word) == 0)
    }

    /// Keep only the bits also set in `other`.
    pub(crate) fn retain(&self, other: &Self) {
        for word in 0..WORDS {
            self.store(word, self.word(word) & other.word(word));
        }
    }

    /// Indices of set bits, ascending.
    pub(crate) fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        set_bits::<WORDS>(move |word| self.word(word))
    }

    /// Indices set here but clear in `other`, ascending.
    pub(crate) fn ones_not_in<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = usize> + 'a {
        set_bits::<WORDS>(move |word| self.word(word) & !other.word(word))
    }

    /// The highest set bit at or below `bit` and at or above `floor`.
    pub(crate) fn last_one_in(&self, floor: usize, bit: usize) -> Option<usize> {
        let mut word = bit / WORD_BITS;
        let mut bits = self.word(word) & (u64::MAX >> (WORD_BITS - 1 - bit % WORD_BITS));
        loop {
            if bits != 0 {
                let found = word * WORD_BITS + (WORD_BITS - 1 - bits.leading_zeros() as usize);
                return (found >= floor).then_some(found);
            }
            if word == 0 || word * WORD_BITS <= floor {
                return None;
            }
            word -= 1;
            bits = self.word(word);
        }
    }
}

/// Indices of the set bits of `WORDS` words produced by `word`, ascending.
fn set_bits<const WORDS: usize>(word: impl Fn(usize) -> u64) -> impl Iterator<Item = usize> {
    (0..WORDS).flat_map(move |index| {
        let mut bits = word(index);
        std::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }
            let bit = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            Some(index * WORD_BITS + bit)
        })
    })
}

impl<const WORDS: usize> Default for Bitmap<WORDS> {
    fn default() -> Self {
        Self {
            words: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_iterate() {
        let bits = Bitmap::<2>::default();
        for bit in [0, 5, 63, 64, 127] {
            bits.set(bit);
        }
        assert!(bits.get(63) && bits.get(64) && !bits.get(62));
        assert_eq!(bits.ones().collect::<Vec<_>>(), vec![0, 5, 63, 64, 127]);
        assert_eq!(bits.count_ones(), 5);
        let keep = Bitmap::<2>::default();
        keep.set(5);
        keep.set(64);
        assert_eq!(
            bits.ones_not_in(&keep).collect::<Vec<_>>(),
            vec![0, 63, 127]
        );
        bits.retain(&keep);
        assert_eq!(bits.ones().collect::<Vec<_>>(), vec![5, 64]);
        bits.clear_all();
        assert!(bits.is_empty());
    }

    #[test]
    fn predecessor_search_crosses_words_and_respects_the_floor() {
        let bits = Bitmap::<4>::default();
        bits.set(10);
        bits.set(130);
        assert_eq!(bits.last_one_in(0, 255), Some(130));
        assert_eq!(bits.last_one_in(0, 129), Some(10));
        assert_eq!(bits.last_one_in(11, 129), None);
        assert_eq!(bits.last_one_in(0, 9), None);
        assert_eq!(bits.last_one_in(0, 10), Some(10));
    }
}
