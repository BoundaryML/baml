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
    ops::Range,
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

    #[inline]
    fn hash_word(self) -> &'a AtomicU64 {
        // SAFETY: the hash word is inside the live cell.
        unsafe { &*(self.cell().byte_add(STR_HASH_OFFSET as usize) as *const AtomicU64) }
    }

    /// The string's hash, computed on first use and kept in the cell. Never
    /// zero: zero means "not computed", so a hash of zero reads as one.
    /// Two racing first uses compute the same value.
    ///
    /// The value is [`map_string_hash`](crate::types::map_string_hash) of the
    /// text: a string key's map hash, so every map operation keyed by a
    /// string cell hashes its bytes at most once.
    #[inline]
    pub fn hash(self) -> u64 {
        let word = self.hash_word();
        let cached = word.load(Ordering::Relaxed);
        if cached != 0 {
            return cached;
        }
        let hash = crate::types::map_string_hash(self.as_str()).max(1);
        word.store(hash, Ordering::Relaxed);
        hash
    }

    /// The hash if a use already computed it: what [`Self::hash`] would
    /// return, without computing it.
    #[inline]
    pub fn cached_hash(self) -> Option<u64> {
        match self.hash_word().load(Ordering::Relaxed) {
            0 => None,
            hash => Some(hash),
        }
    }

    /// Whether `other` holds the same text, settled as cheaply as the cells
    /// allow: the same cell, else unequal lengths, else (when both are
    /// cached) unequal hashes, else the bytes.
    #[inline]
    pub fn text_eq(self, other: Self) -> bool {
        if self.ptr == other.ptr {
            return true;
        }
        if self.byte_len() != other.byte_len() {
            return false;
        }
        if let (Some(mine), Some(theirs)) = (self.cached_hash(), other.cached_hash())
            && mine != theirs
        {
            return false;
        }
        self.as_bytes() == other.as_bytes()
    }

    /// The string copied out as a `BexStr`, for the compiler and host
    /// boundary, which still speak `BexStr`.
    pub fn to_bex_str(self) -> bex_str::BexStr {
        bex_str::BexStr::from(self.as_str())
    }

    /// Whether every byte is ASCII, so char `n` starts at byte `n`. O(1):
    /// the cell keeps both lengths.
    #[inline]
    pub fn is_ascii(self) -> bool {
        self.byte_len() == self.char_len()
    }

    /// Whether `other` is the same cell.
    #[inline]
    pub fn ptr_eq(self, other: Self) -> bool {
        self.ptr == other.ptr
    }

    /// The byte offset of char `n`: `byte_len` when `n` is past the end.
    /// O(1) for ASCII text; otherwise a scan of the first `n` chars.
    pub fn char_to_byte(self, n: usize) -> usize {
        char_to_byte_in(self.as_str(), self.char_len(), n)
    }

    /// The char index of the char starting at byte `b` (a char boundary);
    /// `char_len` when `b` is past the end. Inverse of [`Self::char_to_byte`].
    pub fn byte_to_char(self, b: usize) -> usize {
        byte_to_char_in(self.as_str(), self.char_len(), b)
    }

    /// The byte range of the chars `chars`, both ends clamped to the string
    /// (and the end to at least the start), scanned once.
    pub fn char_bytes(self, chars: Range<usize>) -> Range<usize> {
        char_bytes_in(self.as_str(), self.char_len(), chars)
    }

    /// The substring at the chars `chars`, clamped like [`Self::char_bytes`].
    pub fn char_slice(self, chars: Range<usize>) -> &'a str {
        &self.as_str()[self.char_bytes(chars)]
    }

    /// The char at index `n`, if there is one.
    pub fn char_at(self, n: usize) -> Option<char> {
        self.as_str()[self.char_to_byte(n)..].chars().next()
    }

    /// The char index where `needle` first occurs.
    pub fn char_find(self, needle: &str) -> Option<usize> {
        self.char_find_from(needle, 0)
    }

    /// The char index where `needle` first occurs at or after char `start`.
    /// A start past the end finds nothing, even an empty needle. Counts
    /// chars only between `start` and the match.
    pub fn char_find_from(self, needle: &str, start: usize) -> Option<usize> {
        char_find_from_in(self.as_str(), self.char_len(), needle, start)
    }

    /// The char index where `needle` last occurs.
    pub fn char_rfind(self, needle: &str) -> Option<usize> {
        let byte = self.as_str().rfind(needle)?;
        Some(self.byte_to_char(byte))
    }
}

/// The byte width of the UTF-8 sequence `lead` begins.
#[inline]
const fn utf8_width(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

/// `char_to_byte` over `text` of `char_len` chars; O(1) when all ASCII.
fn char_to_byte_in(text: &str, char_len: usize, n: usize) -> usize {
    if char_len == text.len() {
        return n.min(text.len());
    }
    scan_char_to_byte(text, (0, 0), n)
}

/// `byte_to_char` over `text` of `char_len` chars; O(1) when all ASCII.
fn byte_to_char_in(text: &str, char_len: usize, b: usize) -> usize {
    if char_len == text.len() {
        return b.min(text.len());
    }
    scan_byte_to_char(text, (0, 0), b)
}

/// `char_bytes` over `text` of `char_len` chars; O(1) when all ASCII.
fn char_bytes_in(text: &str, char_len: usize, chars: Range<usize>) -> Range<usize> {
    let end = chars.end.max(chars.start);
    if char_len == text.len() {
        return chars.start.min(text.len())..end.min(text.len());
    }
    let byte_start = scan_char_to_byte(text, (0, 0), chars.start);
    byte_start..scan_char_to_byte(text, (chars.start, byte_start), end)
}

/// `char_find_from` over `text` of `char_len` chars.
fn char_find_from_in(text: &str, char_len: usize, needle: &str, start: usize) -> Option<usize> {
    if start > char_len {
        return None;
    }
    let byte_start = char_to_byte_in(text, char_len, start);
    let found = byte_start + text[byte_start..].find(needle)?;
    if char_len == text.len() {
        return Some(found);
    }
    Some(scan_byte_to_char(text, (start, byte_start), found))
}

/// The byte offset of char `n`, scanning forward from char `known.0` at
/// byte `known.1`: `text.len()` when `n` is past the end.
fn scan_char_to_byte(text: &str, known: (usize, usize), n: usize) -> usize {
    let bytes = text.as_bytes();
    let (mut chars, mut byte) = known;
    while chars < n && byte < bytes.len() {
        byte += utf8_width(bytes[byte]);
        chars += 1;
    }
    byte.min(bytes.len())
}

/// The char index of byte `b` (a char boundary), counting forward from
/// char `known.0` at byte `known.1`.
fn scan_byte_to_char(text: &str, known: (usize, usize), b: usize) -> usize {
    let bytes = text.as_bytes();
    let end = b.min(bytes.len());
    let starts = bytes[known.1..end]
        .iter()
        .filter(|&&byte| byte & 0xC0 != 0x80)
        .count();
    known.0 + starts
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
        self.text_eq(*other)
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

    fn chars_of(text: &str) -> usize {
        text.chars().count()
    }

    #[test]
    fn char_and_byte_offsets_agree_with_a_scan() {
        for text in ["", "abc", "héllo wörld", "日本語テキスト", "a日b本c"] {
            let n = chars_of(text);
            for i in 0..=n + 2 {
                let expected = text
                    .char_indices()
                    .nth(i)
                    .map_or(text.len(), |(byte, _)| byte);
                assert_eq!(char_to_byte_in(text, n, i), expected, "{text:?}[{i}]");
                assert_eq!(
                    byte_to_char_in(text, n, expected),
                    i.min(n),
                    "{text:?}[{i}]"
                );
            }
            assert_eq!(char_to_byte_in(text, n, usize::MAX), text.len());
            assert_eq!(byte_to_char_in(text, n, usize::MAX), n);
        }
    }

    #[test]
    fn char_ranges_clamp_and_slice_on_boundaries() {
        for text in ["abcdef", "héllo wörld", "日本語テキスト"] {
            let n = chars_of(text);
            let chars: Vec<char> = text.chars().collect();
            for start in 0..=n + 1 {
                for end in 0..=n + 1 {
                    let range = char_bytes_in(text, n, start..end);
                    let expected: String =
                        chars[start.min(n)..end.max(start).min(n)].iter().collect();
                    assert_eq!(&text[range], expected, "{text:?}[{start}..{end}]");
                }
            }
            assert_eq!(
                char_bytes_in(text, n, usize::MAX - 1..usize::MAX),
                text.len()..text.len()
            );
        }
    }

    /// A string cell in a buffer, for tests that need no heap.
    #[cfg(not(feature = "heap_debug"))]
    #[repr(C, align(16))]
    struct Cell([u8; 256]);

    #[cfg(not(feature = "heap_debug"))]
    impl Cell {
        fn holding(text: &str) -> Self {
            assert!(str_cell_bytes(text.len()) <= 256);
            let mut cell = Self([0; 256]);
            let granules = (str_cell_bytes(text.len()) / STR_GRANULE_BYTES) as u16;
            let header = crate::object_format::Header::new(STR_DESC, granules);
            // SAFETY: the buffer is 16-aligned and sized for the header and
            // the cell; nothing reads it before `init`.
            unsafe {
                cell.0
                    .as_mut_ptr()
                    .cast::<crate::object_format::Header>()
                    .write(header);
                StrRef::init(cell.payload(), text);
            }
            cell
        }

        fn payload(&self) -> HeapPtr {
            let ptr = self.0.as_ptr().wrapping_add(HEADER_BYTES).cast_mut();
            // SAFETY: a `StrRef` reads only through header-relative offsets
            // inside the buffer, and the buffer outlives every view.
            unsafe { HeapPtr::from_ptr(ptr.cast()) }
        }

        fn view(&self) -> StrRef<'_> {
            // SAFETY: `holding` wrote a string cell.
            unsafe { StrRef::new_unchecked(self.payload()) }
        }
    }

    #[cfg(not(feature = "heap_debug"))]
    #[test]
    fn the_cached_hash_is_the_map_string_hash() {
        for text in [
            "",
            "a",
            "key",
            "héllo wörld",
            "日本語テキスト",
            "a日b本c",
            "x".repeat(200).as_str(),
        ] {
            let cell = Cell::holding(text);
            let string = cell.view();
            assert_eq!(
                string.cached_hash(),
                None,
                "{text:?}: fresh cells hold no hash"
            );
            let expected = crate::types::map_string_hash(text).max(1);
            assert_eq!(string.hash(), expected, "{text:?}");
            assert_eq!(
                string.cached_hash(),
                Some(expected),
                "{text:?}: cached after one use"
            );
            assert_eq!(
                string.hash(),
                expected,
                "{text:?}: read back, not recomputed"
            );
        }
    }

    #[cfg(not(feature = "heap_debug"))]
    #[test]
    fn text_eq_settles_by_length_hash_then_bytes() {
        let a = Cell::holding("héllo");
        let b = Cell::holding("héllo");
        let c = Cell::holding("hello!");
        let d = Cell::holding("hållo!");
        assert!(a.view().text_eq(a.view()), "the same cell");
        assert!(a.view().text_eq(b.view()), "equal bytes, nothing cached");
        assert!(!a.view().text_eq(c.view()), "lengths differ");
        assert_eq!(c.view().byte_len(), 6);
        assert_eq!(d.view().byte_len(), 7);
        assert!(!c.view().text_eq(d.view()), "lengths differ");
        let e = Cell::holding("hellp!");
        assert!(!c.view().text_eq(e.view()), "same length, bytes differ");
        c.view().hash();
        e.view().hash();
        assert!(!c.view().text_eq(e.view()), "same length, hashes differ");
        a.view().hash();
        b.view().hash();
        assert!(a.view().text_eq(b.view()), "equal with both hashes cached");
        assert_eq!(a.view(), b.view());
        assert_ne!(a.view(), c.view());
    }

    #[test]
    fn char_find_counts_chars_not_bytes() {
        let text = "héllo wörld, héllo";
        let n = chars_of(text);
        assert_eq!(char_find_from_in(text, n, "llo", 0), Some(2));
        assert_eq!(char_find_from_in(text, n, "llo", 3), Some(15));
        assert_eq!(char_find_from_in(text, n, "llo", 16), None);
        assert_eq!(char_find_from_in(text, n, "", n), Some(n));
        assert_eq!(char_find_from_in(text, n, "", n + 1), None);
        assert_eq!(char_find_from_in("plain", 5, "", 6), None);
        assert_eq!(char_find_from_in("plain", 5, "in", 1), Some(3));
    }
}
