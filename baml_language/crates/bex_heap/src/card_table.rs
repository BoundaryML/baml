//! Card table for tracking cross-generation pointer writes.
//!
//! The heap is divided into fixed-size "cards" of 256 objects each. When a
//! write barrier detects that an older-generation object now holds a reference
//! to a younger-generation object, the card containing the older object is
//! marked dirty.
//!
//! During a partial (Gen0 or Gen1) collection, dirty cards are scanned to
//! discover cross-generation references that would otherwise be missed.
//!
//! # Concurrency
//!
//! Concurrent VM tasks can each fire a write barrier at the same time, so
//! `mark_dirty_by_offset` and `is_dirty` take `&self` and use atomic loads /
//! stores on the underlying `AtomicU8` slots. Relaxed ordering is sufficient
//! — writers only ever set the same value (`1`), and readers don't care
//! about ordering relative to other writes, only about eventual visibility
//! at the next GC safepoint (which is a strong synchronization point).
//!
//! Capacity growth (`ensure_capacity_for_chunks`, `clear`) takes `&mut self`
//! and must be called at a GC safepoint where no VM is concurrently marking.

use ::core::sync::atomic::{AtomicU8, Ordering};

use crate::chunked_vec::DEFAULT_CHUNK_SIZE;

/// Number of objects per card.
pub const CARD_SIZE: usize = 256;

/// Number of cards per `ChunkedVec` chunk.
pub const CARDS_PER_CHUNK: usize = DEFAULT_CHUNK_SIZE / CARD_SIZE;

// Compile-time assertion: chunk size must be a multiple of card size.
const _: () = assert!(
    DEFAULT_CHUNK_SIZE.is_multiple_of(CARD_SIZE),
    "DEFAULT_CHUNK_SIZE must be a multiple of CARD_SIZE"
);

/// A card table tracking which 256-object cards in a generation space are dirty.
///
/// A card is dirty when it contains an object that holds a reference to a
/// younger generation. The card table is used during partial GC to find
/// cross-generation roots without scanning the entire older generation.
///
/// # Indexing
///
/// Card indices are laid out linearly: for a space with `C` chunks, card `i`
/// covers objects at positions `[i * CARD_SIZE, (i + 1) * CARD_SIZE)` within
/// that chunk.
///
/// More precisely, for chunk index `chunk_idx` and within-chunk offset
/// `offset_in_chunk`, the card index is:
/// ```text
/// card_index = chunk_idx * CARDS_PER_CHUNK + offset_in_chunk / CARD_SIZE
/// ```
pub struct CardTable {
    /// One atomic byte per card: 0 = clean, 1 = dirty. Atomic so concurrent
    /// VM write-barriers can mark the table without a data race.
    cards: Vec<AtomicU8>,
}

impl CardTable {
    /// Create an empty card table.
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    /// Ensure the table has at least enough capacity for `num_chunks` chunks.
    ///
    /// Extends with clean (0) entries as needed. Must only be called at a GC
    /// safepoint — concurrent `mark_dirty_by_offset` calls would race against
    /// the underlying `Vec::resize` reallocation.
    pub fn ensure_capacity_for_chunks(&mut self, num_chunks: usize) {
        let needed = num_chunks * CARDS_PER_CHUNK;
        if self.cards.len() < needed {
            self.cards.resize_with(needed, || AtomicU8::new(0));
        }
    }

    /// Mark the card containing the object at `(chunk_idx, offset_in_chunk)` as dirty.
    ///
    /// Safe to call concurrently from multiple threads — writes go through a
    /// relaxed atomic store, and all writers set the same value (`1`).
    ///
    /// Callers must ensure the table has been sized via
    /// [`ensure_capacity_for_chunks`](Self::ensure_capacity_for_chunks)
    /// at a GC safepoint before VMs start mutating Gen2 objects. An
    /// out-of-bounds `card_index` means that invariant has been violated —
    /// silently dropping the mark could cause a later minor GC to miss a live
    /// Gen0 reference from Gen2, so we panic instead.
    #[inline(always)]
    pub fn mark_dirty_by_offset(&self, chunk_idx: usize, offset_in_chunk: usize) {
        let card_index = chunk_idx * CARDS_PER_CHUNK + offset_in_chunk / CARD_SIZE;
        let slot = self.cards.get(card_index).unwrap_or_else(|| {
            panic!(
                "card table under-sized: chunk_idx={chunk_idx}, offset_in_chunk={offset_in_chunk}, \
                 card_index={card_index}, capacity={}. Call `ensure_capacity_for_chunks` at the \
                 GC safepoint before mutators can race.",
                self.cards.len()
            )
        });
        slot.store(1, Ordering::Relaxed);
    }

    /// Check whether card `card_index` is dirty.
    #[cfg(test)]
    #[inline]
    pub fn is_dirty(&self, card_index: usize) -> bool {
        self.cards
            .get(card_index)
            .map(|slot| slot.load(Ordering::Relaxed) != 0)
            .unwrap_or(false)
    }

    /// Clear all cards (mark everything clean). Must only be called at a GC
    /// safepoint.
    pub fn clear(&mut self) {
        for slot in &self.cards {
            slot.store(0, Ordering::Relaxed);
        }
    }

    /// Iterate over all dirty card indices.
    pub fn dirty_card_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.cards
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.load(Ordering::Relaxed) != 0)
            .map(|(i, _)| i)
    }

    /// Return the total number of cards (dirty or clean).
    #[cfg(test)]
    pub fn num_cards(&self) -> usize {
        self.cards.len()
    }
}

impl Default for CardTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_table_new_is_empty() {
        let ct = CardTable::new();
        assert_eq!(ct.num_cards(), 0);
        assert_eq!(ct.dirty_card_indices().count(), 0);
    }

    #[test]
    fn test_ensure_capacity_for_chunks() {
        let mut ct = CardTable::new();
        ct.ensure_capacity_for_chunks(2);
        assert_eq!(ct.num_cards(), 2 * CARDS_PER_CHUNK);
        // All clean initially
        assert_eq!(ct.dirty_card_indices().count(), 0);
    }

    #[test]
    fn test_mark_dirty_by_offset() {
        let mut ct = CardTable::new();
        ct.ensure_capacity_for_chunks(1);

        // Mark the first card (objects 0..255)
        ct.mark_dirty_by_offset(0, 0);
        assert!(ct.is_dirty(0));
        assert!(!ct.is_dirty(1));

        // Mark a card in the second "slot" within the chunk
        ct.mark_dirty_by_offset(0, CARD_SIZE);
        assert!(ct.is_dirty(1));
    }

    #[test]
    fn test_clear_resets_all_cards() {
        let mut ct = CardTable::new();
        ct.ensure_capacity_for_chunks(2);
        ct.mark_dirty_by_offset(0, 0);
        ct.mark_dirty_by_offset(1, 0);
        assert_eq!(ct.dirty_card_indices().count(), 2);

        ct.clear();
        assert_eq!(ct.dirty_card_indices().count(), 0);
        assert_eq!(ct.num_cards(), 2 * CARDS_PER_CHUNK);
    }

    #[test]
    fn test_dirty_card_indices_order() {
        let mut ct = CardTable::new();
        ct.ensure_capacity_for_chunks(3);
        ct.mark_dirty_by_offset(2, 0);
        ct.mark_dirty_by_offset(0, CARD_SIZE * 2);

        let dirty: Vec<usize> = ct.dirty_card_indices().collect();
        assert!(dirty.contains(&(2 * CARDS_PER_CHUNK)));
        assert!(dirty.contains(&2));
    }

    #[test]
    fn test_cards_per_chunk_const() {
        // DEFAULT_CHUNK_SIZE == 4096, CARD_SIZE == 256 → 16 cards per chunk
        assert_eq!(CARDS_PER_CHUNK, 16);
    }
}
