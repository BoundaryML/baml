//! Strings in the shared object format: three cell kinds, one prefix.
//!
//! Replaces `Object::String(BexStr)`, whose bytes lived in an `Arc` outside
//! the heap. A runtime string is a descriptor cell the collector copies and
//! frees like any other. Every kind starts with the same 24-byte prefix, so
//! [`StrRef`] reads lengths and the cached hash the same way for each:
//!
//! ```text
//! #[repr(C)]
//! struct Flat {            // descriptor = STR_DESC, variable size
//!     hdr: Header,
//!     byte_len: u32,
//!     char_len: u32,
//!     hash: AtomicU64,     // 0 until first asked for
//!     bytes: [u8; byte_len], // UTF-8; the cell is padded to a granule
//! }
//! struct Rope {            // descriptor = ROPE_DESC, 48 bytes
//!     hdr, byte_len, char_len, hash,
//!     left: ptr,           // a string of any kind
//!     right: ptr,
//!     flat: ptr,           // null until flattened; always a flat after
//! }
//! struct View {            // descriptor = VIEW_DESC, 48 bytes (40 used)
//!     hdr, byte_len, char_len, hash,
//!     base: ptr,           // always a flat
//!     byte_offset: u64,
//! }
//! ```
//!
//! A 5-byte string is a 32-byte flat cell. Concatenation makes a rope when
//! the result is [`STR_SHARE_THRESHOLD`] bytes or more and both inputs are
//! non-empty; a sub-range makes a view when it is that long. The pointer
//! slots are in the kinds' descriptors, so the collector traces them like
//! any other object's.
//!
//! A rope's bytes are borrowable only once it is flattened: the owner of
//! the allocator does that (`Tlab::flatten_string`), storing the flat into
//! the rope's `flat` slot with a Release store; two racing flatteners both
//! succeed and one pointer wins. Reads that copy out (hashing, equality,
//! ordering, printing) walk the rope's leaves without flattening, so only
//! a borrow ([`StrRef::as_str`]) requires it.

#![allow(unsafe_code)]
#![allow(
    clippy::cast_ptr_alignment,
    clippy::cast_possible_truncation,
    reason = "field addresses are header-relative offsets the layout keeps aligned, and \
              lengths fit their words"
)]

use std::{
    borrow::Cow,
    fmt,
    marker::PhantomData,
    ops::Range,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

use crate::{
    HeapPtr,
    layout::DescriptorId,
    object_format::{Format, HEADER_BYTES},
    types::Value,
};

/// The descriptor of every flat string cell, fixed by the object format.
pub const STR_DESC: DescriptorId = DescriptorId(34);
/// The descriptor of rope cells: a concatenation not yet copied.
pub const ROPE_DESC: DescriptorId = DescriptorId(35);
/// The descriptor of view cells: a sub-range of a flat.
pub const VIEW_DESC: DescriptorId = DescriptorId(36);

/// Byte offsets, from the header, of the fields every string kind shares.
pub const STR_BYTE_LEN_OFFSET: u32 = HEADER_BYTES as u32;
pub const STR_CHAR_LEN_OFFSET: u32 = STR_BYTE_LEN_OFFSET + 4;
pub const STR_HASH_OFFSET: u32 = STR_CHAR_LEN_OFFSET + 4;
/// Where a flat's bytes start: the descriptor's fixed size.
pub const STR_BYTES_OFFSET: u32 = STR_HASH_OFFSET + 8;

/// Byte offsets, from the header, of a rope's pointer slots, and its size.
pub const ROPE_LEFT_OFFSET: u32 = STR_BYTES_OFFSET;
pub const ROPE_RIGHT_OFFSET: u32 = ROPE_LEFT_OFFSET + 8;
pub const ROPE_FLAT_OFFSET: u32 = ROPE_RIGHT_OFFSET + 8;
pub const ROPE_BYTES: u32 = ROPE_FLAT_OFFSET + 8;

/// Byte offsets, from the header, of a view's base slot and offset word,
/// and its size.
pub const VIEW_BASE_OFFSET: u32 = STR_BYTES_OFFSET;
pub const VIEW_OFFSET_OFFSET: u32 = VIEW_BASE_OFFSET + 8;
pub const VIEW_BYTES: u32 = VIEW_OFFSET_OFFSET + 8;

/// Results this long or longer share storage (a rope for a concatenation,
/// a view for a sub-range); shorter ones are copied into a flat.
pub const STR_SHARE_THRESHOLD: usize = 32;

/// The allocation unit of every cell. The heap's granule; asserted equal
/// there.
pub const STR_GRANULE_BYTES: usize = 16;

/// Bytes of the cell holding a string of `byte_len` bytes, granule-rounded.
#[inline]
pub const fn str_cell_bytes(byte_len: usize) -> usize {
    let bytes = STR_BYTES_OFFSET as usize + byte_len;
    (bytes + STR_GRANULE_BYTES - 1) & !(STR_GRANULE_BYTES - 1)
}

/// Which cell a string is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrKind {
    Flat,
    Rope,
    View,
}

impl StrKind {
    /// The kind a descriptor names, if it is a string's.
    #[inline]
    pub fn of(descriptor: DescriptorId) -> Option<Self> {
        match descriptor {
            STR_DESC => Some(Self::Flat),
            ROPE_DESC => Some(Self::Rope),
            VIEW_DESC => Some(Self::View),
            _ => None,
        }
    }
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
    /// The string `ptr` names, if it is one (of any kind).
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

    /// Fill a fresh flat cell with `text`: the lengths, a cleared hash and
    /// the bytes. The cell's padding past the bytes is left as it was.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh cell of at least
    /// [`str_cell_bytes`]`(text.len())` bytes with its header written.
    #[inline]
    pub unsafe fn init(ptr: HeapPtr, text: &str) -> Self {
        let char_len = text.chars().count();
        // SAFETY: a fresh cell nothing else reads yet; every write is inside
        // the cell the caller sized for `text`.
        unsafe { Self::init_flat_from(ptr, text.len(), char_len, 0, |out| out.push(text.as_bytes())) }
    }

    /// Fill a fresh flat cell of `byte_len` bytes, `char_len` chars and a
    /// known hash (or 0) with the chunks `fill` pushes, in order.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh flat cell of at least
    /// [`str_cell_bytes`]`(byte_len)` bytes with its header written, and
    /// the chunks are valid UTF-8 totalling `byte_len` bytes.
    unsafe fn init_flat_from(
        ptr: HeapPtr,
        byte_len: usize,
        char_len: usize,
        hash: u64,
        fill: impl FnOnce(&mut Filler),
    ) -> Self {
        let string = Self::new(ptr);
        // SAFETY: a fresh cell nothing else reads yet; the prefix is inside
        // every string cell.
        unsafe { string.write_prefix(byte_len, char_len, hash) };
        let mut filler = Filler {
            // SAFETY: the bytes start inside the cell.
            at: unsafe { string.cell().byte_add(STR_BYTES_OFFSET as usize) },
            left: byte_len,
        };
        fill(&mut filler);
        assert_eq!(filler.left, 0, "a flat is filled to its length");
        string
    }

    /// Fill a fresh flat cell with `left` then `right`: the short
    /// concatenation, copied.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh flat cell of at least
    /// [`str_cell_bytes`]`(left.byte_len() + right.byte_len())` bytes with
    /// its header written; `left` and `right` are live strings.
    pub unsafe fn init_concat(ptr: HeapPtr, left: Self, right: Self) -> Self {
        // SAFETY: forwarded to the caller; the chunks are both texts' bytes.
        unsafe {
            Self::init_flat_from(
                ptr,
                left.byte_len() + right.byte_len(),
                left.char_len() + right.char_len(),
                0,
                |out| {
                    for chunk in left.chunks().chain(right.chunks()) {
                        out.push(chunk);
                    }
                },
            )
        }
    }

    /// Fill a fresh rope cell concatenating `left` and `right`: the lengths
    /// are the sums, the hash cleared, the flat slot null.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh cell of [`ROPE_BYTES`] bytes with its
    /// header written; `left` and `right` are live strings.
    pub unsafe fn init_rope(ptr: HeapPtr, left: Self, right: Self) -> Self {
        let string = Self::new(ptr);
        // SAFETY: a fresh cell nothing else reads yet; every slot is inside
        // the rope cell.
        unsafe {
            string.write_prefix(
                left.byte_len() + right.byte_len(),
                left.char_len() + right.char_len(),
                0,
            );
            string.write_ptr(ROPE_LEFT_OFFSET, left.ptr);
            string.write_ptr(ROPE_RIGHT_OFFSET, right.ptr);
            string.write_word(ROPE_FLAT_OFFSET, 0);
        }
        string
    }

    /// Fill a fresh view cell of the bytes `bytes` of the flat `base`, which
    /// hold `char_len` chars (`bytes` lies on char boundaries).
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh cell of [`VIEW_BYTES`] bytes with its
    /// header written; `base` is a live flat and `bytes` is inside it.
    pub unsafe fn init_view(ptr: HeapPtr, base: Self, bytes: Range<usize>, char_len: usize) -> Self {
        debug_assert_eq!(base.kind(), StrKind::Flat);
        debug_assert!(bytes.end <= base.byte_len() && bytes.start <= bytes.end);
        let string = Self::new(ptr);
        // SAFETY: a fresh cell nothing else reads yet; every slot is inside
        // the view cell.
        unsafe {
            string.write_prefix(bytes.len(), char_len, 0);
            string.write_ptr(VIEW_BASE_OFFSET, base.ptr);
            string.write_word(VIEW_OFFSET_OFFSET, bytes.start as u64);
        }
        string
    }

    /// Fill a fresh flat cell with this string's bytes: the flattening of a
    /// rope (the leaves are copied iteratively, in order, however deep the
    /// rope is), or a copy of any other kind. A cached hash is carried over.
    ///
    /// # Safety
    /// `ptr` is the payload of a fresh flat cell of at least
    /// [`str_cell_bytes`]`(self.byte_len())` bytes with its header written.
    pub unsafe fn init_flat_copy(ptr: HeapPtr, source: Self) -> Self {
        let hash = source.cached_hash().unwrap_or(0);
        // SAFETY: forwarded to the caller; the chunks are the source's bytes.
        unsafe {
            Self::init_flat_from(ptr, source.byte_len(), source.char_len(), hash, |out| {
                for chunk in source.chunks() {
                    out.push(chunk);
                }
            })
        }
    }

    /// Publish `flat` as this rope's flattening. Later borrows read it. A
    /// racing flattener's cell is simply garbage.
    ///
    /// # Panics
    /// If this is not a rope, or `flat` is not a flat of the same length.
    pub fn publish_flat(self, flat: Self) {
        assert_eq!(self.kind(), StrKind::Rope, "only ropes are flattened");
        assert_eq!(flat.kind(), StrKind::Flat);
        assert_eq!(flat.byte_len(), self.byte_len());
        if let Some(hash) = flat.cached_hash() {
            self.hash_word().store(hash, Ordering::Relaxed);
        } else if let Some(hash) = self.cached_hash() {
            flat.hash_word().store(hash, Ordering::Relaxed);
        }
        self.word(ROPE_FLAT_OFFSET)
            .store(Value::object(flat.ptr).bits(), Ordering::Release);
    }

    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.ptr
    }

    /// Which cell this string is.
    #[inline]
    pub fn kind(self) -> StrKind {
        // SAFETY: a live cell has a header.
        let descriptor = unsafe { self.ptr.header() }.descriptor;
        StrKind::of(descriptor).unwrap_or_else(|| unreachable!("{descriptor:?} is not a string"))
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
    fn word(self, offset: u32) -> &'a AtomicU64 {
        // SAFETY: the word is inside the live cell.
        unsafe { &*(self.cell().byte_add(offset as usize) as *const AtomicU64) }
    }

    /// # Safety
    /// A fresh cell; `offset` is inside it.
    #[inline]
    unsafe fn write_word(self, offset: u32, word: u64) {
        // SAFETY: forwarded to the caller.
        unsafe { (self.cell() as *mut u64).byte_add(offset as usize).write(word) }
    }

    /// # Safety
    /// A fresh cell; `offset` is a pointer slot inside it.
    #[inline]
    unsafe fn write_ptr(self, offset: u32, ptr: HeapPtr) {
        // SAFETY: forwarded to the caller.
        unsafe { self.write_word(offset, Value::object(ptr).bits()) }
    }

    /// # Safety
    /// A fresh cell.
    #[inline]
    unsafe fn write_prefix(self, byte_len: usize, char_len: usize, hash: u64) {
        // SAFETY: forwarded to the caller; the prefix is in every string cell.
        unsafe {
            (self.cell() as *mut u32)
                .byte_add(STR_BYTE_LEN_OFFSET as usize)
                .write(byte_len as u32);
            (self.cell() as *mut u32)
                .byte_add(STR_CHAR_LEN_OFFSET as usize)
                .write(char_len as u32);
            self.write_word(STR_HASH_OFFSET, hash);
        }
    }

    /// The string in the pointer slot at `offset`, if the slot is set.
    #[inline]
    fn ptr_at(self, offset: u32, order: Ordering) -> Option<Self> {
        let bits = self.word(offset).load(order);
        // SAFETY: a string's pointer slots hold zero or a live string.
        let ptr = unsafe { Value::from_bits(bits) }.as_object_ptr()?;
        Some(Self::new(ptr))
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

    /// A rope's left and right halves.
    #[inline]
    pub fn rope_halves(self) -> Option<(Self, Self)> {
        (self.kind() == StrKind::Rope).then(|| {
            let left = self.ptr_at(ROPE_LEFT_OFFSET, Ordering::Relaxed);
            let right = self.ptr_at(ROPE_RIGHT_OFFSET, Ordering::Relaxed);
            (
                left.expect("a rope's left slot is set"),
                right.expect("a rope's right slot is set"),
            )
        })
    }

    /// A rope's flattening, once one is published.
    #[inline]
    pub fn rope_flat(self) -> Option<Self> {
        if self.kind() != StrKind::Rope {
            return None;
        }
        self.ptr_at(ROPE_FLAT_OFFSET, Ordering::Acquire)
    }

    /// A view's flat base and the byte offset of the view in it.
    #[inline]
    pub fn view_base(self) -> Option<(Self, usize)> {
        if self.kind() != StrKind::View {
            return None;
        }
        let base = self
            .ptr_at(VIEW_BASE_OFFSET, Ordering::Relaxed)
            .expect("a view's base slot is set");
        let offset = self.word(VIEW_OFFSET_OFFSET).load(Ordering::Relaxed) as usize;
        Some((base, offset))
    }

    /// The flat cell holding this string's bytes, and where they start in
    /// it: itself for a flat, the base for a view, the flattening for a
    /// flattened rope. `None` for a rope not yet flattened.
    #[inline]
    pub fn flat(self) -> Option<(Self, usize)> {
        match self.kind() {
            StrKind::Flat => Some((self, 0)),
            StrKind::View => self.view_base(),
            StrKind::Rope => self.rope_flat().map(|flat| (flat, 0)),
        }
    }

    /// Whether the bytes can be borrowed: everything but an unflattened
    /// rope.
    #[inline]
    pub fn is_borrowable(self) -> bool {
        self.kind() != StrKind::Rope || self.rope_flat().is_some()
    }

    /// A flat's bytes.
    #[inline]
    fn flat_bytes(self) -> &'a [u8] {
        debug_assert_eq!(self.kind(), StrKind::Flat);
        // SAFETY: the bytes follow the fixed fields inside the live flat
        // cell and are never written after initialization.
        unsafe {
            std::slice::from_raw_parts(
                self.cell().byte_add(STR_BYTES_OFFSET as usize),
                self.byte_len(),
            )
        }
    }

    /// The bytes, borrowed from the flat holding them.
    ///
    /// # Panics
    /// On a rope nobody has flattened: the allocator's owner flattens
    /// (`Tlab::flatten_string`) before a borrow.
    #[inline]
    pub fn as_bytes(self) -> &'a [u8] {
        match self.flat() {
            Some((flat, offset)) => &flat.flat_bytes()[offset..offset + self.byte_len()],
            None => unflattened_rope_borrow(),
        }
    }

    /// The text, borrowed from the flat holding it. Panics as
    /// [`Self::as_bytes`] does.
    #[inline]
    pub fn as_str(self) -> &'a str {
        // SAFETY: every flat holds the bytes of a `str`, and a view's range
        // lies on char boundaries.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// The string's bytes as the flats holding them, left to right: one
    /// chunk for a flat, a view or a flattened rope; a leaf per flat under
    /// an unflattened rope. Walks with an explicit stack, however deep the
    /// rope.
    pub fn chunks(self) -> Chunks<'a> {
        Chunks { stack: vec![self] }
    }

    /// Append the text to `out`, flattened or not.
    pub fn push_to(self, out: &mut String) {
        out.reserve(self.byte_len());
        for chunk in self.chunks() {
            // SAFETY: every chunk is the whole text of a flat or a view,
            // valid UTF-8.
            out.push_str(unsafe { std::str::from_utf8_unchecked(chunk) });
        }
    }

    /// The text: borrowed when the bytes are borrowable (everything but
    /// an unflattened rope), else copied out. For readers without the
    /// allocator; readers with it flatten and borrow.
    pub fn text(self) -> Cow<'a, str> {
        if self.is_borrowable() {
            Cow::Borrowed(self.as_str())
        } else {
            Cow::Owned(self.to_owned_string())
        }
    }

    /// The text copied out, flattened or not.
    pub fn to_owned_string(self) -> String {
        let mut out = String::new();
        self.push_to(&mut out);
        out
    }

    #[inline]
    fn hash_word(self) -> &'a AtomicU64 {
        self.word(STR_HASH_OFFSET)
    }

    /// The string's hash, computed on first use and kept in the cell. Never
    /// zero: zero means "not computed", so a hash of zero reads as one.
    /// Two racing first uses compute the same value.
    ///
    /// The value is [`map_string_hash`](crate::types::map_string_hash) of the
    /// text: a string key's map hash, so every map operation keyed by a
    /// string cell hashes its bytes at most once. A flattened rope shares
    /// its flat's; an unflattened one hashes its leaves in place.
    #[inline]
    pub fn hash(self) -> u64 {
        let word = self.hash_word();
        let cached = word.load(Ordering::Relaxed);
        if cached != 0 {
            return cached;
        }
        self.compute_hash()
    }

    #[cold]
    #[inline(never)]
    fn compute_hash(self) -> u64 {
        let hash = match self.kind() {
            StrKind::Flat | StrKind::View => crate::types::map_string_hash(self.as_str()).max(1),
            StrKind::Rope => match self.rope_flat() {
                Some(flat) => flat.hash(),
                None => {
                    use std::hash::Hasher as _;
                    let mut hasher = crate::types::map_string_hasher(self.byte_len());
                    for chunk in self.chunks() {
                        hasher.write(chunk);
                    }
                    hasher.finish().max(1)
                }
            },
        };
        self.hash_word().store(hash, Ordering::Relaxed);
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
        if self.is_borrowable() && other.is_borrowable() {
            return self.as_bytes() == other.as_bytes();
        }
        chunks_cmp(self.chunks(), other.chunks()) == std::cmp::Ordering::Equal
    }

    /// The string copied out as a `BexStr`, for the compiler and host
    /// boundary, which still speak `BexStr`.
    pub fn to_bex_str(self) -> bex_str::BexStr {
        if self.is_borrowable() {
            return bex_str::BexStr::from(self.as_str());
        }
        bex_str::BexStr::from(self.to_owned_string())
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

#[cold]
#[inline(never)]
fn unflattened_rope_borrow() -> ! {
    panic!("a rope's text was borrowed before it was flattened (flatten_string)")
}

/// Writes a flat's bytes in order; see `StrRef::init_flat_from`.
struct Filler {
    at: *mut u8,
    left: usize,
}

impl Filler {
    fn push(&mut self, chunk: &[u8]) {
        assert!(chunk.len() <= self.left, "a flat is filled to its length");
        // SAFETY: `at` has `left` bytes of the cell ahead of it.
        unsafe {
            std::ptr::copy_nonoverlapping(chunk.as_ptr(), self.at, chunk.len());
            self.at = self.at.add(chunk.len());
        }
        self.left -= chunk.len();
    }
}

/// The flats a string's bytes live in, left to right. See
/// [`StrRef::chunks`].
pub struct Chunks<'a> {
    stack: Vec<StrRef<'a>>,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        loop {
            let string = self.stack.pop()?;
            if let Some((flat, offset)) = string.flat() {
                return Some(&flat.flat_bytes()[offset..offset + string.byte_len()]);
            }
            let (left, right) = string.rope_halves().expect("an unflattened rope");
            self.stack.push(right);
            self.stack.push(left);
        }
    }
}

/// Byte-lexicographic order of two chunk streams.
fn chunks_cmp<'a, 'b>(
    mut a: impl Iterator<Item = &'a [u8]>,
    mut b: impl Iterator<Item = &'b [u8]>,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut head_a, mut head_b): (&[u8], &[u8]) = (&[], &[]);
    loop {
        if head_a.is_empty() {
            match a.next() {
                Some(chunk) => head_a = chunk,
                None => {
                    let b_done = head_b.is_empty() && b.all(<[u8]>::is_empty);
                    return if b_done { Ordering::Equal } else { Ordering::Less };
                }
            }
            continue;
        }
        if head_b.is_empty() {
            match b.next() {
                Some(chunk) => head_b = chunk,
                None => return Ordering::Greater,
            }
            continue;
        }
        let n = head_a.len().min(head_b.len());
        match head_a[..n].cmp(&head_b[..n]) {
            Ordering::Equal => {
                head_a = &head_a[n..];
                head_b = &head_b[n..];
            }
            other => return other,
        }
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

/// The chars in the bytes `bytes` of `text`, O(1) when `text` is all ASCII
/// (`char_len == text.len()`); otherwise a count of the char starts.
pub fn chars_in_bytes(text: &str, char_len: usize, bytes: Range<usize>) -> usize {
    if char_len == text.len() {
        return bytes.len();
    }
    scan_byte_to_char(text, (0, 0), bytes.end) - scan_byte_to_char(text, (0, 0), bytes.start)
}

impl fmt::Debug for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_borrowable() {
            return fmt::Debug::fmt(self.as_str(), f);
        }
        fmt::Debug::fmt(&self.to_owned_string(), f)
    }
}

impl fmt::Display for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_borrowable() {
            return fmt::Display::fmt(self.as_str(), f);
        }
        for chunk in self.chunks() {
            // SAFETY: every chunk is a whole flat's or view's text.
            f.write_str(unsafe { std::str::from_utf8_unchecked(chunk) })?;
        }
        Ok(())
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
        if self.ptr == other.ptr {
            return std::cmp::Ordering::Equal;
        }
        if self.is_borrowable() && other.is_borrowable() {
            return self.as_bytes().cmp(other.as_bytes());
        }
        chunks_cmp(self.chunks(), other.chunks())
    }
}

impl PartialEq<str> for StrRef<'_> {
    fn eq(&self, other: &str) -> bool {
        self.byte_len() == other.len()
            && chunks_cmp(self.chunks(), std::iter::once(other.as_bytes()))
                == std::cmp::Ordering::Equal
    }
}

impl PartialEq<&str> for StrRef<'_> {
    fn eq(&self, other: &&str) -> bool {
        *self == **other
    }
}

impl PartialEq<bex_str::BexStr> for StrRef<'_> {
    fn eq(&self, other: &bex_str::BexStr) -> bool {
        *self == *other.as_str()
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
        assert_eq!(ROPE_BYTES, 48);
        assert_eq!(VIEW_BYTES, 40);
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
                    assert_eq!(&text[range.clone()], expected, "{text:?}[{start}..{end}]");
                    assert_eq!(
                        chars_in_bytes(text, n, range),
                        expected.chars().count(),
                        "{text:?}[{start}..{end}] chars"
                    );
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
        assert_eq!(a.view(), "héllo");
        assert!(a.view() > c.view(), "bytes order: 0xC3 after 0x65");
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
