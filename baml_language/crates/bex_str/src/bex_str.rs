use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, Range},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// Maximum bytes stored inline without heap allocation.
/// 56 (target size) - 1 (discriminant) - 1 (`InlineLen`) = 54.
const INLINE_CAPACITY: usize = 54;

/// The length byte of an inline string: the byte length in the low six bits
/// (`INLINE_CAPACITY` < 64) and, in the high bit, whether every byte is ASCII.
///
/// The flag makes `char_len()` and `ascii_prefix_len()` O(1) for the common
/// all-ASCII inline string, where both equal the byte length. A non-ASCII
/// inline string still scans its at most 54 bytes. The exact ASCII prefix
/// would need six more bits, which the handle doesn't have without giving up
/// a byte of inline capacity.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InlineLen(u8);

impl InlineLen {
    const ASCII: u8 = 0x80;
    const LEN_MASK: u8 = 0x3f;

    /// The empty string, which is (vacuously) all ASCII.
    const EMPTY: InlineLen = InlineLen(Self::ASCII);

    /// A length of `byte_len` bytes, `ascii` if every one of them is ASCII.
    fn new(byte_len: usize, ascii: bool) -> InlineLen {
        debug_assert!(byte_len <= INLINE_CAPACITY);
        InlineLen(byte_len as u8 | if ascii { Self::ASCII } else { 0 })
    }

    /// The length of `bytes` (at most `INLINE_CAPACITY`), checking them for ASCII.
    fn of(bytes: &[u8]) -> InlineLen {
        InlineLen::new(bytes.len(), bytes.is_ascii())
    }

    /// Byte length.
    pub fn byte_len(self) -> usize {
        (self.0 & Self::LEN_MASK) as usize
    }

    /// Whether every byte is ASCII.
    pub fn is_ascii(self) -> bool {
        self.0 & Self::ASCII != 0
    }
}

/// Sentinel for a `Slice` whose ASCII prefix doesn't fit in `u32` (a slice of
/// 4 GiB or more): `ascii_prefix_len()` recomputes it by scanning.
pub(crate) const SLICE_ASCII_PREFIX_UNKNOWN: u32 = u32::MAX;

/// V8-inspired immutable string. O(1) clone. 56 bytes.
///
/// # Units
///
/// The two lengths are `byte_len()` and `char_len()`. Methods without a
/// unit prefix measure positions in **bytes**, as `str` does
/// (and every `str` method reached through `Deref` does). Methods whose
/// positions are `char` indices (Unicode scalar values) start with `char_`.
/// `char_to_byte` / `byte_to_char` convert between the two.
///
/// # ASCII prefix
///
/// Every variant knows, in O(1), its exact [`ascii_prefix_len`]: the byte
/// offset of its first non-ASCII byte, or `byte_len()` if it has none. Within that
/// prefix char `n` starts at byte `n`, so char-indexed operations there skip
/// the codepoint scan, and scans elsewhere start at the prefix's end.
///
/// [`ascii_prefix_len`]: BexStr::ascii_prefix_len
pub enum BexStr {
    /// Strings ≤54 bytes: zero heap allocation, clone = memcpy.
    Inline {
        len: InlineLen,
        data: [u8; INLINE_CAPACITY],
    },
    /// Heap-allocated, ref-counted, immutable. Clone = Arc bump.
    Flat(Arc<FlatStr>),
    /// Zero-copy view into a Flat string's buffer. Depth-1 invariant:
    /// parent is ALWAYS Flat, never Slice or Concat.
    Slice {
        parent: Arc<FlatStr>,
        offset: u64,
        byte_len: u64,
        char_len: u64,
        /// Exact ASCII prefix of this slice (not its parent), or
        /// `SLICE_ASCII_PREFIX_UNKNOWN`. Fits in the padding after the enum
        /// tag, so it doesn't grow `BexStr`.
        ascii_prefix_len: u32,
        hash: [AtomicU64; 2],
    },
    /// Deferred concatenation. Flattened on first byte-level access.
    Concat(Arc<ConcatNode>),
}

/// Heap-allocated immutable string data with cached hash.
pub struct FlatStr {
    /// Cached XXH3-128; the high word publishes the immutable low word.
    /// A zero high word is uncached (that rare digest is simply recomputed).
    pub(crate) hash: [AtomicU64; 2],
    /// Number of Unicode codepoints. Computed once at construction.
    pub(crate) char_len: u64,
    /// Byte offset of the first non-ASCII byte, or `data.len()` if none.
    /// Computed once at construction.
    pub(crate) ascii_prefix_len: u64,
    /// UTF-8 byte data. Tight allocation (no excess capacity).
    pub(crate) data: Box<[u8]>,
}

/// Deferred concatenation node.
pub struct ConcatNode {
    pub(crate) total_byte_len: u64,
    /// Number of Unicode codepoints, summed from the children at construction
    /// (each O(1)), so `char_len()` needs no recursive walk of a deep rope.
    pub(crate) total_char_len: u64,
    /// Exact ASCII prefix, derived from the children's at construction (each
    /// O(1)) for the same reason.
    pub(crate) total_ascii_prefix_len: u64,
    pub(crate) state: Mutex<ConcatState>,
}

pub(crate) enum ConcatState {
    Deferred { left: BexStr, right: BexStr },
    Flattened(Arc<FlatStr>),
}

/// Bytes of an `Arc<FlatStr>` allocation beyond the text itself: the two
/// reference counts and the header.
const FLAT_OVERHEAD: usize = 2 * std::mem::size_of::<usize>() + std::mem::size_of::<FlatStr>();

/// Bytes of an `Arc<ConcatNode>` allocation: the two reference counts and the node.
const CONCAT_OVERHEAD: usize = 2 * std::mem::size_of::<usize>() + std::mem::size_of::<ConcatNode>();

// Size assertion — must fit within Object payload budget
const _: () = assert!(
    std::mem::size_of::<BexStr>() == 56,
    "BexStr size changed — expected exactly 56 bytes"
);

// Send + Sync assertions
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BexStr>();
};

impl BexStr {
    /// Empty string constant (Inline with byte length 0).
    pub fn empty() -> BexStr {
        BexStr::Inline {
            len: InlineLen::EMPTY,
            data: [0; INLINE_CAPACITY],
        }
    }

    /// An inline copy of `bytes` (at most `INLINE_CAPACITY`) whose ASCII-ness
    /// is already known.
    fn inline(bytes: &[u8], len: InlineLen) -> BexStr {
        debug_assert_eq!(len.byte_len(), bytes.len());
        debug_assert_eq!(len.is_ascii(), bytes.is_ascii());
        let mut data = [0u8; INLINE_CAPACITY];
        data[..bytes.len()].copy_from_slice(bytes);
        BexStr::Inline { len, data }
    }

    pub fn is_empty(&self) -> bool {
        self.byte_len() == 0
    }

    /// Heap allocation bytes retained by this string, excluding `Self` and
    /// allocator metadata. Shared backing is charged in full, including the
    /// entire parent of a slice. Concatenations are flattened, as by `as_bytes`,
    /// so subsequent access cannot increase the retained backing.
    ///
    /// Returns `None` on arithmetic overflow.
    pub fn retained_heap_bytes(&self) -> Option<usize> {
        match self {
            Self::Inline { .. } => Some(0),
            Self::Flat(flat) | Self::Slice { parent: flat, .. } => {
                FLAT_OVERHEAD.checked_add(flat.data.len())
            }
            Self::Concat(node) => CONCAT_OVERHEAD
                .checked_add(FLAT_OVERHEAD)?
                .checked_add(node.flatten().data.len()),
        }
    }

    /// Heap bytes this string would own if it shared nothing: zero when
    /// inline, otherwise an allocation holding its full length. A slice counts
    /// only the bytes it views, and a concatenation counts the buffer that
    /// reading it would materialize. Never flattens.
    ///
    /// Every handle to shared storage reports it in full, so summing this over
    /// handles overestimates; see [`Self::shared_heap_bytes`] for the share.
    pub fn unshared_heap_bytes(&self) -> usize {
        match self {
            Self::Inline { .. } => 0,
            Self::Flat(flat) => FLAT_OVERHEAD.saturating_add(flat.data.len()),
            Self::Slice { byte_len, .. } => usize::try_from(*byte_len).unwrap_or(usize::MAX),
            Self::Concat(node) => CONCAT_OVERHEAD
                .saturating_add(usize::try_from(node.total_byte_len).unwrap_or(usize::MAX)),
        }
    }

    /// This handle's share of the allocation it keeps alive: the allocation's
    /// size divided among the handles holding it, rounded up. Summed over every
    /// handle, a shared buffer is counted about once. A slice shares its whole
    /// parent, so a small slice of a large string reports a large share.
    ///
    /// A concatenation is sized as in [`Self::unshared_heap_bytes`] without
    /// looking inside, so its pieces are counted again if they are also held
    /// elsewhere. Never flattens.
    pub fn shared_heap_bytes(&self) -> usize {
        match self {
            Self::Inline { .. } => 0,
            Self::Flat(flat) | Self::Slice { parent: flat, .. } => FLAT_OVERHEAD
                .saturating_add(flat.data.len())
                .div_ceil(Arc::strong_count(flat)),
            Self::Concat(node) => CONCAT_OVERHEAD
                .saturating_add(usize::try_from(node.total_byte_len).unwrap_or(usize::MAX))
                .div_ceil(Arc::strong_count(node)),
        }
    }

    /// Byte length. O(1) for all variants.
    pub fn byte_len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } => len.byte_len(),
            BexStr::Flat(f) => f.data.len(),
            BexStr::Slice { byte_len, .. } => *byte_len as usize,
            BexStr::Concat(c) => c.total_byte_len as usize,
        }
    }

    /// Number of Unicode codepoints. O(1) for all variants: a Concat caches
    /// its count at construction, so this never walks (or recurses into) the
    /// rope, however deep repeated appends have made it; a non-ASCII inline
    /// string counts its at most 54 bytes.
    pub fn char_len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } if len.is_ascii() => len.byte_len(),
            BexStr::Inline { len, data } => bytecount::num_chars(&data[..len.byte_len()]),
            BexStr::Flat(f) => f.char_len as usize,
            BexStr::Slice { char_len, .. } => *char_len as usize,
            BexStr::Concat(c) => c.total_char_len as usize,
        }
    }

    /// Length in bytes of the longest all-ASCII prefix: the byte offset of the
    /// first non-ASCII byte, or `byte_len()` if there is none. Exact, and O(1) for
    /// all variants (a non-ASCII inline string scans at most 54 bytes; a slice
    /// of 4 GiB or more rescans). For every `n <= ascii_prefix_len()`, char `n`
    /// starts at byte `n`.
    pub fn ascii_prefix_len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } if len.is_ascii() => len.byte_len(),
            BexStr::Inline { len, data } => ascii_prefix_len(&data[..len.byte_len()]),
            BexStr::Flat(f) => f.ascii_prefix_len as usize,
            BexStr::Slice {
                ascii_prefix_len: SLICE_ASCII_PREFIX_UNKNOWN,
                ..
            } => ascii_prefix_len(self.as_bytes()),
            BexStr::Slice {
                ascii_prefix_len, ..
            } => *ascii_prefix_len as usize,
            BexStr::Concat(c) => c.total_ascii_prefix_len as usize,
        }
    }

    /// `(ascii_prefix_len(), char_len())`, with a single scan for a
    /// non-ASCII inline string.
    fn ascii_prefix_and_char_len(&self) -> (usize, usize) {
        match self {
            BexStr::Inline { len, data } if !len.is_ascii() => {
                ascii_prefix_and_char_len(&data[..len.byte_len()])
            }
            _ => (self.ascii_prefix_len(), self.char_len()),
        }
    }

    /// Whether every byte is ASCII. O(1); shadows the O(n) `str::is_ascii`.
    pub fn is_ascii(&self) -> bool {
        self.ascii_prefix_len() == self.byte_len()
    }

    /// O(1) for Inline/Flat/Slice. O(n) first-call flatten for Concat.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BexStr::Inline { len, data } => &data[..len.byte_len()],
            BexStr::Flat(f) => &f.data,
            BexStr::Slice {
                parent,
                offset,
                byte_len,
                ..
            } => {
                let o = *offset as usize;
                let l = *byte_len as usize;
                &parent.data[o..o + l]
            }
            BexStr::Concat(c) => {
                let flat = c.flatten();
                // SAFETY: The Arc<FlatStr> inside the Mutex outlives &self
                // because self holds Arc<ConcatNode> which holds the Mutex.
                // The flattened Arc is never replaced once set.
                #[allow(unsafe_code)]
                unsafe {
                    let ptr = flat.data.as_ptr();
                    let len = flat.data.len();
                    std::slice::from_raw_parts(ptr, len)
                }
            }
        }
    }

    /// XXH3-128 (seed zero) of the logical UTF-8 bytes. Heap-backed strings
    /// cache the result; inline strings compute it directly. The first call on
    /// a concat flattens it, sharing the flat allocation/cache with its clones.
    /// This is separate from `Hash`, whose `Borrow<str>` contract must be retained.
    pub fn content_hash(&self) -> u128 {
        match self {
            Self::Inline { len, data } => xxhash_rust::xxh3::xxh3_128(&data[..len.byte_len()]),
            Self::Flat(flat) => cached_hash(&flat.hash, &flat.data),
            Self::Slice { hash, .. } => cached_hash(hash, self.as_bytes()),
            Self::Concat(node) => {
                let flat = node.flatten();
                cached_hash(&flat.hash, &flat.data)
            }
        }
    }

    /// UTF-8 view; concatenations are flattened on first access.
    pub fn as_str(&self) -> &str {
        // SAFETY: BexStr is always constructed from valid UTF-8.
        #[allow(unsafe_code)]
        unsafe {
            std::str::from_utf8_unchecked(self.as_bytes())
        }
    }

    /// Zero-copy substring by byte range. Enforces depth-1 invariant.
    /// Callers must ensure both ends are UTF-8 char boundaries.
    pub fn byte_slice(&self, range: Range<usize>) -> BexStr {
        let Range { start, end } = range;
        debug_assert!(start <= end);
        debug_assert!(end <= self.byte_len());
        let slice_len = end - start;

        // Empty result → Inline
        if slice_len == 0 {
            return BexStr::empty();
        }

        // Short result → Inline. The receiver's ASCII flag or prefix usually
        // settles the slice's ASCII-ness without a scan.
        if slice_len <= INLINE_CAPACITY {
            let bytes = &self.as_bytes()[start..end];
            let ascii = match self {
                BexStr::Inline { len, .. } => len.is_ascii() || bytes.is_ascii(),
                _ => end <= self.ascii_prefix_len() || bytes.is_ascii(),
            };
            return BexStr::inline(bytes, InlineLen::new(slice_len, ascii));
        }

        // Long result → Slice (depth-1 invariant). The receiver's own ASCII
        // prefix covers the slice's start when `start` falls inside it; only
        // otherwise does the slice need a scan, fused with its codepoint count.
        let bytes = &self.as_bytes()[start..end];
        let outer_prefix = self.ascii_prefix_len();
        let (prefix, char_len) = if start < outer_prefix {
            let prefix = (outer_prefix - start).min(slice_len);
            (prefix, prefix + bytecount::num_chars(&bytes[prefix..]))
        } else {
            ascii_prefix_and_char_len(bytes)
        };
        let ascii_prefix_len = u32::try_from(prefix)
            .ok()
            .filter(|&p| p != SLICE_ASCII_PREFIX_UNKNOWN)
            .unwrap_or(SLICE_ASCII_PREFIX_UNKNOWN);
        let (parent, offset) = match self {
            BexStr::Inline { .. } => {
                // Can't reach here: INLINE_CAPACITY < slice_len
                // but slice_len <= self.byte_len() <= INLINE_CAPACITY. Contradiction.
                unreachable!("Inline string cannot produce a Slice longer than INLINE_CAPACITY");
            }
            BexStr::Flat(f) => (f.clone(), start as u64),
            // Re-slice: point at SAME parent, adjust offset. Depth stays 1.
            BexStr::Slice { parent, offset, .. } => (parent.clone(), *offset + start as u64),
            // `as_bytes` above already flattened; this returns the cached result.
            BexStr::Concat(c) => (c.flatten(), start as u64),
        };
        BexStr::Slice {
            parent,
            offset,
            byte_len: slice_len as u64,
            char_len: char_len as u64,
            ascii_prefix_len,
            hash: [const { AtomicU64::new(0) }; 2],
        }
    }

    // ── Char-indexed methods ───────────────────────────────────────────
    //
    // All char ↔ byte conversion goes through `char_to_byte_from` and
    // `byte_to_char_from`, the only places that scan.

    /// Byte offset where char `n` starts, or `byte_len()` if `n >= char_len()`.
    /// O(1) when `n <= ascii_prefix_len()`; otherwise scans from the end of
    /// the ASCII prefix (flattening a `Concat` first).
    pub fn char_to_byte(&self, n: usize) -> usize {
        self.char_to_byte_from((0, 0), n)
    }

    /// Char index of the char starting at byte `b`, which must be a char
    /// boundary (`char_len()` for `b >= byte_len()`). Inverse of `char_to_byte`.
    /// O(1) when `b <= ascii_prefix_len()` or `b >= byte_len()`; otherwise counts
    /// from the end of the ASCII prefix.
    pub fn byte_to_char(&self, b: usize) -> usize {
        self.byte_to_char_from((0, 0), b)
    }

    /// `char_to_byte(n)`, given that char `known.0` starts at byte `known.1`
    /// (`known.0 <= n`; past the end, `known.1` may be `byte_len()`): the scan
    /// starts from whichever of that point and the end of the ASCII prefix is
    /// further along.
    fn char_to_byte_from(&self, known: (usize, usize), n: usize) -> usize {
        // At most 54 bytes: when not all ASCII, scanning directly beats
        // finding the prefix first.
        if let BexStr::Inline { len, data } = self {
            if len.is_ascii() {
                return n.min(len.byte_len());
            }
            let bytes = &data[..len.byte_len()];
            return known.1 + scan_char_to_byte(&bytes[known.1..], n - known.0);
        }
        let prefix = self.ascii_prefix_len();
        if n <= prefix {
            return n;
        }
        if n >= self.char_len() {
            return self.byte_len();
        }
        let (char_idx, byte_idx) = if known.0 > prefix {
            known
        } else {
            (prefix, prefix)
        };
        debug_assert!(char_idx <= n);
        byte_idx + scan_char_to_byte(&self.as_bytes()[byte_idx..], n - char_idx)
    }

    /// `byte_to_char(b)`, given that byte `known.0` starts char `known.1`
    /// (`known.0 <= b`), counting from whichever of that point and the end of
    /// the ASCII prefix is further along.
    fn byte_to_char_from(&self, known: (usize, usize), b: usize) -> usize {
        if let BexStr::Inline { len, data } = self {
            if len.is_ascii() {
                return b.min(len.byte_len());
            }
            let bytes = &data[..len.byte_len()];
            return known.1 + bytecount::num_chars(&bytes[known.0..b.min(bytes.len())]);
        }
        let prefix = self.ascii_prefix_len();
        if b <= prefix {
            return b;
        }
        if b >= self.byte_len() {
            return self.char_len();
        }
        let (byte_idx, char_idx) = if known.0 > prefix {
            known
        } else {
            (prefix, prefix)
        };
        debug_assert!(byte_idx <= b);
        char_idx + bytecount::num_chars(&self.as_bytes()[byte_idx..b])
    }

    /// The char at index `n`, or `None` if `n >= char_len()`.
    pub fn char_at(&self, n: usize) -> Option<char> {
        // Past the end, `b == byte_len()` and the suffix is empty.
        let b = self.char_to_byte(n);
        self.as_str()[b..].chars().next()
    }

    /// The char at index `n` as a one-char string, or `None` if
    /// `n >= char_len()`. Inside the ASCII prefix this copies the byte, with
    /// no decoding and no separate count for the bounds check.
    pub fn char_get(&self, n: usize) -> Option<BexStr> {
        let ascii_byte = match self {
            BexStr::Inline { len, data } if len.is_ascii() => (n < len.byte_len()).then(|| data[n]),
            _ => (n < self.ascii_prefix_len()).then(|| self.as_bytes()[n]),
        };
        if let Some(byte) = ascii_byte {
            return Some(BexStr::inline(&[byte], InlineLen::new(1, true)));
        }
        let c = self.char_at(n)?;
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        Some(BexStr::inline(
            bytes,
            InlineLen::new(bytes.len(), c.is_ascii()),
        ))
    }

    /// Substring by char range. Clamps both ends to `char_len()` (and `end`
    /// to at least `start`), so it never panics.
    pub fn char_slice(&self, range: Range<usize>) -> BexStr {
        // `end - start`, not `start + 1`: `start` may be `usize::MAX`.
        if range.end.checked_sub(range.start) == Some(1) {
            return self.char_get(range.start).unwrap_or_else(BexStr::empty);
        }
        // `char_to_byte` maps every index past the end to `byte_len()`, which is
        // the clamp; `end` is scanned onward from `start`.
        let byte_start = self.char_to_byte(range.start);
        let end = range.end.max(range.start);
        let byte_end = self.char_to_byte_from((range.start, byte_start), end);
        self.byte_slice(byte_start..byte_end)
    }

    /// Finds `needle` and returns its char index, or `None`.
    pub fn char_find(&self, needle: &str) -> Option<usize> {
        self.char_find_from(needle, 0)
    }

    /// Finds `needle` at or after char `start`, returning an absolute char
    /// index. A start past the end returns `None`, even for an empty needle.
    /// Searches the borrowed suffix without allocating, and counts chars only
    /// between `start` and the match.
    pub fn char_find_from(&self, needle: &str, start: usize) -> Option<usize> {
        if start > self.char_len() {
            return None;
        }
        let byte_start = self.char_to_byte(start);
        let relative = self.as_str()[byte_start..].find(needle)?;
        Some(self.byte_to_char_from((byte_start, start), byte_start + relative))
    }

    /// Finds the *last* occurrence of `needle` and returns the char index of
    /// its start, or `None`.
    pub fn char_rfind(&self, needle: &str) -> Option<usize> {
        let byte_idx = self.as_str().rfind(needle)?;
        Some(self.byte_to_char(byte_idx))
    }

    /// Repeats this string `n` times.
    pub fn repeat(&self, n: usize) -> BexStr {
        BexStr::from(self.as_str().repeat(n))
    }

    /// O(1) deferred concatenation.
    pub fn concat(left: BexStr, right: BexStr) -> BexStr {
        if left.is_empty() {
            return right;
        }
        if right.is_empty() {
            return left;
        }
        let total_byte_len = left.byte_len() + right.byte_len();

        // Short result → Inline. Only inline strings are this short, so both
        // ASCII checks read a flag.
        if total_byte_len <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            let left_bytes = left.as_bytes();
            let right_bytes = right.as_bytes();
            data[..left_bytes.len()].copy_from_slice(left_bytes);
            data[left_bytes.len()..total_byte_len].copy_from_slice(right_bytes);
            return BexStr::Inline {
                len: InlineLen::new(total_byte_len, left.is_ascii() && right.is_ascii()),
                data,
            };
        }

        let (left_prefix, left_chars) = left.ascii_prefix_and_char_len();
        let (right_prefix, right_chars) = right.ascii_prefix_and_char_len();
        let total_char_len = (left_chars + right_chars) as u64;
        let total_ascii_prefix_len = if left_prefix == left.byte_len() {
            left.byte_len() + right_prefix
        } else {
            left_prefix
        } as u64;
        BexStr::Concat(Arc::new(ConcatNode {
            total_byte_len: total_byte_len as u64,
            total_char_len,
            total_ascii_prefix_len,
            state: Mutex::new(ConcatState::Deferred { left, right }),
        }))
    }

    /// Pointer equality check — true if both point to the same Arc.
    fn ptr_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BexStr::Flat(a), BexStr::Flat(b)) => Arc::ptr_eq(a, b),
            (
                BexStr::Slice {
                    parent: a,
                    offset: ao,
                    byte_len: al,
                    ..
                },
                BexStr::Slice {
                    parent: b,
                    offset: bo,
                    byte_len: bl,
                    ..
                },
            ) => Arc::ptr_eq(a, b) && ao == bo && al == bl,
            (BexStr::Concat(a), BexStr::Concat(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Try to read the cached hash if already computed (non-zero).
    fn try_get_hash(&self) -> Option<u128> {
        match self {
            BexStr::Inline { .. } => None, // No cache for Inline
            BexStr::Flat(f) => read_hash(&f.hash),
            BexStr::Slice { hash, .. } => read_hash(hash),
            BexStr::Concat(c) => {
                // If flattened, check the FlatStr's hash
                let guard = c.state.lock().unwrap();
                match &*guard {
                    ConcatState::Flattened(f) => read_hash(&f.hash),
                    ConcatState::Deferred { .. } => None,
                }
            }
        }
    }
}

// ── FlatStr ────────────────────────────────────────────────────────

impl FlatStr {
    /// Wraps `data`, computing its codepoint count and ASCII prefix in one pass.
    fn new(data: Box<[u8]>) -> FlatStr {
        let (prefix, char_len) = ascii_prefix_and_char_len(&data);
        FlatStr::from_parts(data, char_len as u64, prefix as u64)
    }

    /// Wraps `data` whose codepoint count and ASCII prefix are already known.
    fn from_parts(data: Box<[u8]>, char_len: u64, ascii_prefix_len: u64) -> FlatStr {
        debug_assert_eq!(
            ascii_prefix_and_char_len(&data),
            (ascii_prefix_len as usize, char_len as usize)
        );
        FlatStr {
            hash: [const { AtomicU64::new(0) }; 2],
            char_len,
            ascii_prefix_len,
            data,
        }
    }

    /// The empty string, left behind in a `ConcatNode` whose children were taken.
    fn placeholder() -> FlatStr {
        FlatStr::from_parts(Box::new([]), 0, 0)
    }
}

// ── ConcatNode ─────────────────────────────────────────────────────

impl ConcatNode {
    /// Iterative flatten. Materializes bytes into Arc<FlatStr>.
    /// Stores result back so subsequent calls return O(1).
    pub(crate) fn flatten(&self) -> Arc<FlatStr> {
        let mut guard = self.state.lock().unwrap();
        match &*guard {
            ConcatState::Flattened(f) => return f.clone(),
            ConcatState::Deferred { .. } => {}
        }

        // Take ownership of the deferred state
        let old = std::mem::replace(
            &mut *guard,
            ConcatState::Flattened(Arc::new(FlatStr::placeholder())),
        );
        let (left, right) = match old {
            ConcatState::Deferred { left, right } => (left, right),
            ConcatState::Flattened(_) => unreachable!(),
        };

        // Iterative tree walk — heap-allocated work-stack
        let mut buf = Vec::with_capacity(self.total_byte_len as usize);
        let mut stack: Vec<BexStr> = Vec::new();
        // Push right first so left is processed first (stack is LIFO)
        stack.push(right);
        stack.push(left);

        while let Some(node) = stack.pop() {
            match node {
                BexStr::Inline { len, data } => {
                    buf.extend_from_slice(&data[..len.byte_len()]);
                }
                BexStr::Flat(f) => {
                    buf.extend_from_slice(&f.data);
                }
                BexStr::Slice {
                    parent,
                    offset,
                    byte_len,
                    ..
                } => {
                    let o = offset as usize;
                    let l = byte_len as usize;
                    buf.extend_from_slice(&parent.data[o..o + l]);
                }
                BexStr::Concat(c) => {
                    let inner_guard = c.state.lock().unwrap();
                    match &*inner_guard {
                        ConcatState::Flattened(f) => {
                            buf.extend_from_slice(&f.data);
                        }
                        ConcatState::Deferred { left, right } => {
                            // Clone the child handles (O(1): Arc bump or inline
                            // memcpy) and leave this node's `Deferred` state
                            // intact. The node may be shared (Arc refcount > 1,
                            // e.g. `let n = a + b; let p = n + c`); destructively
                            // emptying it here would corrupt every *other*
                            // reference to it. (B-262 / B-233)
                            let left = left.clone();
                            let right = right.clone();
                            drop(inner_guard);
                            stack.push(right);
                            stack.push(left);
                        }
                    }
                }
            }
        }

        let flat = Arc::new(FlatStr::from_parts(
            buf.into_boxed_slice(),
            self.total_char_len,
            self.total_ascii_prefix_len,
        ));
        *guard = ConcatState::Flattened(flat.clone());
        flat
    }
}

/// Iterative Drop to avoid stack overflow on deep left-leaning trees.
impl Drop for ConcatNode {
    fn drop(&mut self) {
        let state = match self.state.get_mut() {
            Ok(s) => std::mem::replace(s, ConcatState::Flattened(Arc::new(FlatStr::placeholder()))),
            Err(_) => return, // Mutex poisoned — leak rather than risk UB
        };
        let (left, right) = match state {
            ConcatState::Deferred { left, right } => (left, right),
            ConcatState::Flattened(_) => return,
        };

        let mut stack: Vec<BexStr> = Vec::new();
        stack.push(left);
        stack.push(right);

        while let Some(node) = stack.pop() {
            if let BexStr::Concat(arc) = node
                && let Ok(mut owned) = Arc::try_unwrap(arc)
            {
                let inner = match owned.state.get_mut() {
                    Ok(s) => std::mem::replace(
                        s,
                        ConcatState::Flattened(Arc::new(FlatStr::placeholder())),
                    ),
                    Err(_) => continue,
                };
                if let ConcatState::Deferred { left, right } = inner {
                    stack.push(left);
                    stack.push(right);
                }
            }
            // Inline/Flat/Slice drop in O(1)
        }
    }
}

// ── Trait Implementations ──────────────────────────────────────────

impl Clone for BexStr {
    fn clone(&self) -> Self {
        match self {
            BexStr::Inline { len, data } => BexStr::Inline {
                len: *len,
                data: *data,
            },
            BexStr::Flat(f) => BexStr::Flat(f.clone()),
            BexStr::Slice {
                parent,
                offset,
                byte_len,
                char_len,
                ascii_prefix_len,
                hash,
            } => BexStr::Slice {
                parent: parent.clone(),
                offset: *offset,
                byte_len: *byte_len,
                char_len: *char_len,
                ascii_prefix_len: *ascii_prefix_len,
                hash: clone_hash(hash),
            },
            BexStr::Concat(c) => BexStr::Concat(c.clone()),
        }
    }
}

impl Deref for BexStr {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Hash for BexStr {
    /// Hash raw bytes — same as `<str as Hash>` — to satisfy `Borrow<str>` contract.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq for BexStr {
    fn eq(&self, other: &Self) -> bool {
        if self.byte_len() != other.byte_len() {
            return false;
        }
        if self.ptr_eq(other) {
            return true;
        }
        if let (Some(h1), Some(h2)) = (self.try_get_hash(), other.try_get_hash())
            && h1 != h2
        {
            return false;
        }
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for BexStr {}

impl PartialEq<str> for BexStr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<BexStr> for str {
    fn eq(&self, other: &BexStr) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<String> for BexStr {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<BexStr> for String {
    fn eq(&self, other: &BexStr) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialOrd for BexStr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BexStr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl Borrow<str> for BexStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for BexStr {
    fn from(s: String) -> Self {
        if s.len() <= INLINE_CAPACITY {
            BexStr::inline(s.as_bytes(), InlineLen::of(s.as_bytes()))
        } else {
            BexStr::Flat(Arc::new(FlatStr::new(s.into_bytes().into_boxed_slice())))
        }
    }
}

impl From<&str> for BexStr {
    fn from(s: &str) -> Self {
        if s.len() <= INLINE_CAPACITY {
            BexStr::inline(s.as_bytes(), InlineLen::of(s.as_bytes()))
        } else {
            BexStr::Flat(Arc::new(FlatStr::new(s.as_bytes().into())))
        }
    }
}

// ── Helper functions ──────────────────────────────────────────────────

/// Returns the byte offset where the `n`-th codepoint (0-indexed) starts,
/// or `bytes.len()` if `n` is past the last codepoint.
///
/// For a string with `k` codepoints:
///   - `n = 0` → 0 (start of first codepoint)
///   - `n = k` → `bytes.len()` (one past the last byte)
///
/// Processes 8 bytes at a time using bit masks and popcount for ~30x speedup
/// over byte-by-byte `char_indices().nth()`.
fn scan_char_to_byte(bytes: &[u8], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    // We want to skip `n` leading bytes and return the position of the (n+1)-th,
    // or bytes.len() if fewer than `n` codepoints exist.
    let mut remaining = n;
    let mut i = 0;

    // Process 8 bytes at a time.
    while i + 8 <= bytes.len() {
        let word = u64::from_ne_bytes(bytes[i..i + 8].try_into().unwrap());
        let hi = word & 0x8080_8080_8080_8080;
        let lo = word & 0x4040_4040_4040_4040;
        // A continuation byte is `10xxxxxx`: bit 7 set, bit 6 clear. We read
        // only bit 7 below (`>> 7`), so the bit-6 information has to be shifted
        // up into the bit-7 lane — `!(lo << 1)` clears bit 7 for multibyte
        // *leading* bytes (`11xxxxxx`), leaving only true continuations.
        // (`hi & !lo` left bit 7 untouched, counting every multibyte lead as a
        // continuation and undercounting codepoint starts by one per char.)
        let cont_mask = hi & !(lo << 1);
        let num_leading = 8 - (cont_mask >> 7).count_ones() as usize;

        if remaining <= num_leading {
            break;
        }
        remaining -= num_leading;
        i += 8;
    }

    // Scalar scan: skip `remaining` leading bytes.
    while i < bytes.len() {
        if (bytes[i] & 0xC0) != 0x80 {
            if remaining == 0 {
                return i;
            }
            remaining -= 1;
        }
        i += 1;
    }
    bytes.len()
}

/// Byte offset of the first non-ASCII byte in `bytes`, or `bytes.len()`.
fn ascii_prefix_len(bytes: &[u8]) -> usize {
    // OR whole 64-byte blocks, which LLVM vectorizes, until one has a high
    // bit set; then find the exact byte a word at a time.
    const BLOCK: usize = 64;
    let mut i = 0;
    for block in bytes.as_chunks::<BLOCK>().0 {
        if block.iter().fold(0, |acc, &b| acc | b) >= 0x80 {
            break;
        }
        i += BLOCK;
    }
    const HIGH_BITS: u64 = 0x8080_8080_8080_8080;
    while i + 8 <= bytes.len() {
        let high = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap()) & HIGH_BITS;
        if high != 0 {
            // The lowest set high bit of a little-endian word is its first
            // non-ASCII byte.
            return i + (high.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }
    bytes[i..]
        .iter()
        .position(|b| !b.is_ascii())
        .map_or(bytes.len(), |p| i + p)
}

/// `(ascii_prefix_len, char_len)` of `bytes`. Counts first: when every byte
/// starts a char, the whole string is ASCII and needs no second scan, so
/// ASCII strings cost exactly one (vectorized) count.
fn ascii_prefix_and_char_len(bytes: &[u8]) -> (usize, usize) {
    let char_len = bytecount::num_chars(bytes);
    if char_len == bytes.len() {
        (char_len, char_len)
    } else {
        (ascii_prefix_len(bytes), char_len)
    }
}

impl fmt::Debug for BexStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for BexStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// All writers compute the same digest of immutable bytes. Publishing the high
// word with Release makes the low word visible; racing writers only repeat
// identical stores. No torn *different* digest can be observed. Zero-high hashes
// are returned exactly but not cached, so no valid digest needs remapping.
fn read_hash(cache: &[AtomicU64; 2]) -> Option<u128> {
    let high = cache[1].load(Ordering::Acquire);
    (high != 0).then(|| (u128::from(high) << 64) | u128::from(cache[0].load(Ordering::Relaxed)))
}

fn cached_hash(cache: &[AtomicU64; 2], bytes: &[u8]) -> u128 {
    if let Some(hash) = read_hash(cache) {
        return hash;
    }
    let hash = xxhash_rust::xxh3::xxh3_128(bytes);
    cache[0].store(hash as u64, Ordering::Relaxed);
    cache[1].store((hash >> 64) as u64, Ordering::Release);
    hash
}

fn clone_hash(cache: &[AtomicU64; 2]) -> [AtomicU64; 2] {
    let hash = read_hash(cache).unwrap_or(0);
    [
        AtomicU64::new(hash as u64),
        AtomicU64::new((hash >> 64) as u64),
    ]
}
