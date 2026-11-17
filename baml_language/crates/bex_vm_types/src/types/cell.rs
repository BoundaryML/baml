//! Heap cells: every runtime heap object is a run of 8-byte words, a header
//! word first.
//!
//! A class instance is a cell whose header names its class and whose
//! fields follow inline (see [`InstanceRef`](crate::InstanceRef)). Every
//! other object is an *object cell*: exactly one [`Object`]. `Object` is
//! `repr(u64)`, so its first word is its variant tag, and every tag has
//! [`OBJECT_BODY`] set: the tag *is* the cell's header, and a cell costs
//! nothing beyond its `Object`. A [`HeapPtr`](crate::HeapPtr) to such an
//! object addresses the cell, which is the `Object`.
//!
//! # Header
//!
//! | bit | name | meaning |
//! |-----|------|---------|
//! | 0 | [`CLEANED`] | an instance's BEP-042 cleanup latch |
//! | 1 | [`FORWARDED`] | the collector copied this cell elsewhere |
//! | 2 | [`OBJECT_BODY`] | an object cell; when clear, an instance |
//! | 3.. | | an instance's class pointer; an object's variant |
//!
//! A forwarded cell's header is the copy's address with [`FORWARDED`] set
//! (and [`OBJECT_BODY`] kept), so a moved object's old tag is gone: the
//! collector reads a moved object only through its forwarding address.
//!
//! A header is never zero, so the zeroed words a cell space leaves unused
//! are told apart from cells by reading them.

use crate::Object;

/// Bytes in one cell word: the header and every slot.
pub const WORD_BYTES: usize = size_of::<u64>();

/// The header bit set once an instance's `cleanup` has started.
pub const CLEANED: u64 = 0b001;

/// The header bit the collector sets on a cell it has copied, so a later
/// look at the old cell knows its body now lives elsewhere.
pub const FORWARDED: u64 = 0b010;

/// The header bit of a cell that holds an [`Object`].
pub const OBJECT_BODY: u64 = 0b100;

/// The header bits that are not an instance's class pointer.
pub const HEADER_FLAGS: u64 = CLEANED | FORWARDED | OBJECT_BODY;

/// Words in an object cell: the [`Object`], whose tag is the header.
pub const OBJECT_CELL_WORDS: usize = size_of::<Object>() / WORD_BYTES;

const _: () = assert!(
    size_of::<Object>().is_multiple_of(WORD_BYTES) && align_of::<Object>() == WORD_BYTES,
    "an object cell is a whole number of aligned words"
);

/// The tag of [`Object`]'s variant number `variant`: a header word with
/// [`OBJECT_BODY`] set and the forwarding bits clear.
pub(crate) const fn cell_tag(variant: u64) -> u64 {
    (variant << 3) | OBJECT_BODY
}
