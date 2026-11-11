//! Strings in the shared object format: one flat cell holding the bytes.
//!
//! Replaces `Object::String(BexStr)`, whose bytes lived in an `Arc` outside
//! the heap. A runtime string is now a descriptor cell the collector copies
//! and frees like any other; it has no pointer slots.
//!
//! ```text
//! #[repr(C)]
//! struct Str {
//!     hdr: Header,        // descriptor = STR_DESC
//!     byte_len: u32,
//!     char_len: u32,
//!     hash: AtomicU64,    // 0 until first asked for
//!     bytes: [u8; byte_len], // UTF-8; the cell is padded to a granule
//! }
//! ```
//!
//! A 5-byte string is a 32-byte cell. Rope and view cells (the plan's other
//! two kinds) are not here yet: every string is flat.

#![allow(unsafe_code)]
#![allow(
    clippy::cast_ptr_alignment,
    clippy::cast_possible_truncation,
    reason = "field addresses are header-relative offsets the layout keeps aligned, and \
              lengths fit their words"
)]

use std::{
    fmt,
    marker::PhantomData,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

use crate::{
    HeapPtr,
    layout::DescriptorId,
    object_format::{Format, HEADER_BYTES},
};

/// The descriptor of every flat string cell, fixed by the object format.
pub const STR_DESC: DescriptorId = DescriptorId(34);

/// Byte offsets, from the header, of a string cell's fields.
pub const STR_BYTE_LEN_OFFSET: u32 = HEADER_BYTES as u32;
pub const STR_CHAR_LEN_OFFSET: u32 = STR_BYTE_LEN_OFFSET + 4;
pub const STR_HASH_OFFSET: u32 = STR_CHAR_LEN_OFFSET + 4;
/// Where the bytes start: the descriptor's fixed size.
pub const STR_BYTES_OFFSET: u32 = STR_HASH_OFFSET + 8;

/// The allocation unit of every cell. The heap's granule; asserted equal
/// there.
pub const STR_GRANULE_BYTES: usize = 16;

/// Bytes of the cell holding a string of `byte_len` bytes, granule-rounded.
#[inline]
pub const fn str_cell_bytes(byte_len: usize) -> usize {
    let bytes = STR_BYTES_OFFSET as usize + byte_len;
    (bytes + STR_GRANULE_BYTES - 1) & !(STR_GRANULE_BYTES - 1)
}

/// A borrowed view of a string on the heap.
///
/// Valid while the string is: it hands out nothing that outlives the object,
/// and the caller keeps it alive. `'a` is the borrow of whoever holds the
/// heap permit (the VM), so a `&'a str` taken from a temporary view is fine:
/// collections only run at safepoints with every fiber parked.
#[derive(Clone, Copy)]
pub struct StrRef<'a> {
    ptr: HeapPtr,
    _heap: PhantomData<&'a ()>,
}

impl<'a> StrRef<'a> {
    /// The string `ptr` names, if it is one.
    ///
    /// # Safety
    /// `ptr` names a live object.
    #[inline]
    pub unsafe fn of(ptr: HeapPtr) -> Option<Self> {
        // SAFETY: forwarded to the caller.
        matches!(unsafe { ptr.format() }, Format::Str).then_some(Self::new(ptr))
    }

    /// The string at `ptr`.
    ///
    /// # Safety
    /// `ptr` names a live string cell.
    #[inline]
    pub unsafe fn new_unchecked(ptr: HeapPtr) -> Self {
        debug_assert!(matches!(
            // SAFETY: forwarded to the caller.
            unsafe { ptr.format() },
            Format::Str
        ));
        Self::new(ptr)
    }

    #[inline]
    const fn new(ptr: HeapPtr) -> Self {
        Self {
            ptr,
            _heap: PhantomData,
        }
    }

    /// Fill a fresh cell with `text`: the lengths, a cleared hash and the
    /// bytes. The cell's padding past the bytes is left as it was.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh cell of at least
    /// [`str_cell_bytes`]`(text.len())` bytes with its header written.
    #[inline]
    pub unsafe fn init(ptr: HeapPtr, text: &str) -> Self {
        let string = Self::new(ptr);
        let char_len = text.chars().count();
        // SAFETY: a fresh cell nothing else reads yet; every write is inside
        // the cell the caller sized for `text`.
        unsafe {
            (string.cell() as *mut u32)
                .byte_add(STR_BYTE_LEN_OFFSET as usize)
                .write(text.len() as u32);
            (string.cell() as *mut u32)
                .byte_add(STR_CHAR_LEN_OFFSET as usize)
                .write(char_len as u32);
            (string.cell() as *mut u64)
                .byte_add(STR_HASH_OFFSET as usize)
                .write(0);
            std::ptr::copy_nonoverlapping(
                text.as_ptr(),
                string.cell().byte_add(STR_BYTES_OFFSET as usize),
                text.len(),
            );
        }
        string
    }

    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.ptr
    }

    /// The cell's start: its header.
    #[inline]
    fn cell(self) -> *mut u8 {
        // SAFETY: the header precedes the payload the pointer names.
        unsafe { self.ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES) }
    }

    #[inline]
    fn u32_at(self, offset: u32) -> u32 {
        // SAFETY: the offset is inside the live cell; written once at
        // initialization, so a Relaxed read sees it (the pointer was
        // published with Release).
        unsafe { &*(self.cell().byte_add(offset as usize) as *const AtomicU32) }
            .load(Ordering::Relaxed)
    }

    #[inline]
    pub fn byte_len(self) -> usize {
        self.u32_at(STR_BYTE_LEN_OFFSET) as usize
    }

    #[inline]
    pub fn char_len(self) -> usize {
        self.u32_at(STR_CHAR_LEN_OFFSET) as usize
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.byte_len() == 0
    }

    #[inline]
    pub fn as_bytes(self) -> &'a [u8] {
        // SAFETY: the bytes follow the fixed fields inside the live cell and
        // are never written after initialization.
        unsafe {
            std::slice::from_raw_parts(
                self.cell().byte_add(STR_BYTES_OFFSET as usize),
                self.byte_len(),
            )
        }
    }

    #[inline]
    pub fn as_str(self) -> &'a str {
        // SAFETY: `init` copies the bytes of a `str`.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// The string's hash, computed on first use and kept in the cell. Never
    /// zero: zero means "not computed", so a hash of zero reads as one.
    /// Two racing first uses compute the same value.
    #[inline]
    pub fn hash(self) -> u64 {
        // SAFETY: the hash word is inside the live cell.
        let word =
            unsafe { &*(self.cell().byte_add(STR_HASH_OFFSET as usize) as *const AtomicU64) };
        let cached = word.load(Ordering::Relaxed);
        if cached != 0 {
            return cached;
        }
        let hash = crate::types::map_string_hash(self.as_str()).max(1);
        word.store(hash, Ordering::Relaxed);
        hash
    }

    /// The string copied out as a `BexStr`, for the compiler and host
    /// boundary, which still speak `BexStr`.
    pub fn to_bex_str(self) -> bex_str::BexStr {
        bex_str::BexStr::from(self.as_str())
    }

    /// The byte offset of char `n`: `byte_len` when `n` is past the end.
    pub fn char_to_byte(self, n: usize) -> usize {
        let text = self.as_str();
        text.char_indices()
            .nth(n)
            .map_or(text.len(), |(byte, _)| byte)
    }
}

impl fmt::Debug for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl PartialEq for StrRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr || self.as_str() == other.as_str()
    }
}

impl Eq for StrRef<'_> {}

impl PartialOrd for StrRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StrRef<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialEq<str> for StrRef<'_> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for StrRef<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<bex_str::BexStr> for StrRef<'_> {
    fn eq(&self, other: &bex_str::BexStr) -> bool {
        self.as_str() == other.as_str()
    }
}

impl AsRef<str> for StrRef<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_bytes_round_to_a_granule() {
        assert_eq!(str_cell_bytes(0), 32);
        assert_eq!(str_cell_bytes(5), 32);
        assert_eq!(str_cell_bytes(8), 32);
        assert_eq!(str_cell_bytes(9), 48);
        assert_eq!(STR_BYTES_OFFSET, 24);
    }
}
