use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, Range},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

/// Maximum bytes stored inline without heap allocation.
/// 56 (target size) - 1 (discriminant) - 1 (len byte) = 54.
const INLINE_CAPACITY: usize = 54;
// Inline lengths need only six bits. Cache ASCII in a spare bit without
// reducing capacity or growing the 56-byte handle.
const INLINE_ASCII_FLAG: u8 = 0x40;
// A buffer cannot exceed isize::MAX bytes, so a slice count's high bit is
// available to record a Latin-1 scalar bound without enlarging the handle.
const SLICE_LATIN1_FLAG: u64 = 1 << 63;
const SLICE_COUNT_MASK: u64 = !SLICE_LATIN1_FLAG;
#[inline]
fn inline_len(len: u8) -> usize {
    (len & 0x3f) as usize
}
#[inline]
fn inline_len_and_ascii(len: usize, ascii: bool) -> u8 {
    len as u8
        | if ascii && len != 0 {
            INLINE_ASCII_FLAG
        } else {
            0
        }
}

/// Sentinel for a `Slice` whose ASCII prefix doesn't fit in `u32` (a slice of
/// 4 GiB or more): `ascii_prefix_len()` recomputes it by scanning.
pub(crate) const SLICE_ASCII_PREFIX_UNKNOWN: u32 = u32::MAX;

/// V8-inspired immutable string. O(1) clone. 56 bytes.
///
/// # Units
///
/// The two lengths are `byte_len()` and `codepoint_len()`. Methods without a
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
        /// Low six bits are byte length; bit 6 caches all-ASCII text.
        len: u8,
        data: [u8; INLINE_CAPACITY],
    },
    /// Heap-allocated, ref-counted, immutable. Clone = Arc bump.
    Flat(Arc<FlatStr>),
    /// Zero-copy view into a Flat string's buffer. Depth-1 invariant:
    /// parent is ALWAYS Flat, never Slice or Concat.
    Slice {
        parent: Arc<FlatStr>,
        offset: u64,
        len: u64,
        /// Low 63 bits count scalars; the high bit bounds them to Latin-1.
        codepoint_len: u64,
        /// Codepoint offset in the parent; u32::MAX uses the oversized fallback.
        /// Computed once when creating the view, so indexed reads need no atomics.
        char_offset: u32,
        /// Exact ASCII prefix of this slice (not its parent), or
        /// `SLICE_ASCII_PREFIX_UNKNOWN`. Fits in the padding after the enum
        /// tag, so it doesn't grow `BexStr`.
        ascii_prefix_len: u32,
        /// Shared separately so the handle's metadata stays immutable to LLVM.
        hash: Arc<SliceHash>,
    },
    /// Deferred concatenation. Flattened on first byte-level access.
    Concat(Arc<ConcatNode>),
}

/// Shared hash cache for a slice. Keeping atomics behind a pointer lets the
/// optimizer treat BexStr's tag/metadata as immutable during indexed loops.
pub struct SliceHash {
    hash: [AtomicU64; 2],
}
impl SliceHash {
    pub(crate) fn new() -> Self {
        Self {
            hash: [const { AtomicU64::new(0) }; 2],
        }
    }
}

/// Heap-allocated immutable string data with cached hash.
pub struct FlatStr {
    /// Cached XXH3-128; the high word publishes the immutable low word.
    /// A zero high word is uncached (that rare digest is simply recomputed).
    pub(crate) hash: [AtomicU64; 2],
    /// Number of Unicode codepoints. Computed once at construction.
    pub(crate) codepoint_len: u64,
    /// Byte offset of the first non-ASCII byte, or `data.len()` if none.
    /// Computed once at construction.
    pub(crate) ascii_prefix_len: u64,
    /// Immutable UTF-8 bytes. Constructors allocate tightly; an exclusively
    /// owned append chain may retain spare capacity for amortized growth.
    pub(crate) data: Vec<u8>,
    /// Auxiliary byte offsets and scalar values after the ASCII prefix.
    /// UTF-8 stays canonical; the read cache is built after scans pay for it.
    pub(crate) char_index: OnceLock<CharIndex>,
    scan_debt: AtomicUsize,
}

/// Auxiliary read metadata; UTF-8 remains the canonical text buffer.
pub(crate) struct CharIndex {
    pub(crate) offsets: Vec<u32>,
    points: ScalarCache,
}

enum ScalarCache {
    Latin1(Vec<u8>),
    Bmp(Vec<u16>),
    Full(Vec<char>),
}
impl ScalarCache {
    fn from_points(points: Vec<char>, max: u32) -> Self {
        if max <= 0xff {
            let mut narrow = Vec::with_capacity(points.len());
            narrow.extend(points.iter().map(|&point| point as u8));
            Self::Latin1(narrow)
        } else if max <= 0xffff {
            let mut narrow = Vec::with_capacity(points.len());
            narrow.extend(points.iter().map(|&point| point as u16));
            Self::Bmp(narrow)
        } else {
            Self::Full(points)
        }
    }
    #[inline(always)]
    fn get(&self, n: usize) -> Option<char> {
        match self {
            Self::Latin1(points) => points.get(n).map(|&point| point as char),
            Self::Bmp(points) => points
                .get(n)
                .and_then(|&point| char::from_u32(u32::from(point))),
            Self::Full(points) => points.get(n).copied(),
        }
    }
    fn push(&mut self, point: char) {
        match self {
            Self::Latin1(points) if point as u32 <= 0xff => points.push(point as u8),
            Self::Latin1(points) if point as u32 <= 0xffff => {
                let mut values = Vec::with_capacity(points.capacity());
                values.extend(points.iter().map(|&point| u16::from(point)));
                *self = Self::Bmp(values);
                self.push(point);
            }
            Self::Latin1(points) => {
                let mut values = Vec::with_capacity(points.capacity());
                values.extend(points.iter().map(|&point| point as char));
                *self = Self::Full(values);
                self.push(point);
            }
            Self::Bmp(points) if point as u32 <= 0xffff => points.push(point as u16),
            Self::Bmp(points) => {
                let mut values = Vec::with_capacity(points.capacity());
                values.extend(
                    points
                        .iter()
                        .map(|&point| char::from_u32(u32::from(point)).unwrap()),
                );
                *self = Self::Full(values);
                self.push(point);
            }
            Self::Full(points) => points.push(point),
        }
    }
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Latin1(points) => points.capacity(),
            Self::Bmp(points) => points.capacity() * std::mem::size_of::<u16>(),
            Self::Full(points) => points.capacity() * std::mem::size_of::<char>(),
        }
    }
}

/// Deferred concatenation node.
pub struct ConcatNode {
    pub(crate) total_len: u64,
    /// Number of Unicode codepoints, summed from the children at construction
    /// (each O(1)), so `codepoint_len` needs no recursive walk of a deep rope.
    pub(crate) codepoint_len: u64,
    /// Exact ASCII prefix, derived from the children's at construction (each
    /// O(1)) for the same reason.
    pub(crate) ascii_prefix_len: u64,
    pub(crate) state: Mutex<ConcatState>,
    /// Published once; cached reads borrow this without locking or Arc bumps.
    flat: OnceLock<Arc<FlatStr>>,
}

pub(crate) enum ConcatState {
    Deferred { left: BexStr, right: BexStr },
    Flattened,
}

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
    /// Empty string constant (Inline with len=0).
    #[inline]
    pub fn empty() -> BexStr {
        BexStr::Inline {
            len: 0,
            data: [0; INLINE_CAPACITY],
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.byte_len() == 0
    }

    /// Conservative heap allocation bound, excluding `Self` and allocator
    /// metadata. Shared backing is charged in full, including the entire parent
    /// of a slice and spare append capacity. Reserves the worst-case Unicode
    /// index size without building it, and flattens concatenations so later
    /// reads cannot increase the bound. Consuming concat creates a new value
    /// whose exclusive buffer may grow.
    ///
    /// Returns `None` on arithmetic overflow.
    pub fn retained_heap_bytes(&self) -> Option<usize> {
        fn flat_bytes(flat: &FlatStr) -> Option<usize> {
            std::mem::size_of::<FlatStr>()
                .checked_add(2 * std::mem::size_of::<usize>())?
                .checked_add(flat.data.capacity())?
                .checked_add(flat.index_heap_bytes().max(flat.index_bound_bytes()?))
        }
        match self {
            Self::Inline { .. } => Some(0),
            Self::Flat(flat) => flat_bytes(flat),
            Self::Slice { parent, .. } => flat_bytes(parent)?
                .checked_add(std::mem::size_of::<SliceHash>() + 2 * std::mem::size_of::<usize>()),
            Self::Concat(node) => std::mem::size_of::<ConcatNode>()
                .checked_add(2 * std::mem::size_of::<usize>())?
                .checked_add(flat_bytes(node.flatten())?),
        }
    }

    /// Byte length. O(1) for all variants.
    #[inline]
    pub fn byte_len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } => inline_len(*len),
            BexStr::Flat(f) => f.data.len(),
            BexStr::Slice { len, .. } => *len as usize,
            BexStr::Concat(c) => c.total_len as usize,
        }
    }

    /// Number of Unicode codepoints. O(1) except for non-ASCII Inline (at most
    /// 54 bytes). A Concat
    /// caches its count, so this never walks or recurses into the rope.
    #[inline]
    pub fn codepoint_len(&self) -> usize {
        match self {
            BexStr::Inline { len, data } => {
                if *len & INLINE_ASCII_FLAG != 0 {
                    inline_len(*len)
                } else {
                    bytecount::num_chars(&data[..inline_len(*len)])
                }
            }
            BexStr::Flat(f) => f.codepoint_len as usize,
            BexStr::Slice { codepoint_len, .. } => (*codepoint_len & SLICE_COUNT_MASK) as usize,
            BexStr::Concat(c) => c.codepoint_len as usize,
        }
    }

    /// Length in bytes of the longest all-ASCII prefix: the byte offset of the
    /// first non-ASCII byte, or `byte_len()` if there is none. Exact, and O(1) for
    /// all variants (an inline string scans at most 54 bytes; a slice of
    /// 4 GiB or more rescans). For every `n <= ascii_prefix_len()`, char `n`
    /// starts at byte `n`.
    #[inline]
    pub fn ascii_prefix_len(&self) -> usize {
        match self {
            BexStr::Inline { len, data } => {
                if *len & INLINE_ASCII_FLAG != 0 {
                    inline_len(*len)
                } else {
                    ascii_prefix_len(&data[..inline_len(*len)])
                }
            }
            BexStr::Flat(f) => f.ascii_prefix_len as usize,
            BexStr::Slice {
                ascii_prefix_len: SLICE_ASCII_PREFIX_UNKNOWN,
                ..
            } => ascii_prefix_len(self.as_bytes()),
            BexStr::Slice {
                ascii_prefix_len, ..
            } => *ascii_prefix_len as usize,
            BexStr::Concat(c) => c.ascii_prefix_len as usize,
        }
    }

    /// `(ascii_prefix_len(), codepoint_len())`, with a single scan for an
    /// inline string.
    fn ascii_prefix_and_codepoint_len(&self) -> (usize, usize) {
        match self {
            BexStr::Inline { len, data } => {
                if *len & INLINE_ASCII_FLAG != 0 {
                    (inline_len(*len), inline_len(*len))
                } else {
                    ascii_prefix_and_codepoint_len(&data[..inline_len(*len)])
                }
            }
            _ => (self.ascii_prefix_len(), self.codepoint_len()),
        }
    }

    /// Whether every byte is ASCII. O(1); shadows the O(n) `str::is_ascii`.
    #[inline]
    pub fn is_ascii(&self) -> bool {
        self.ascii_prefix_len() == self.byte_len()
    }

    /// O(1) for Inline/Flat/Slice. O(n) first-call flatten for Concat.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BexStr::Inline { len, data } => &data[..inline_len(*len)],
            BexStr::Flat(f) => &f.data,
            BexStr::Slice {
                parent,
                offset,
                len,
                ..
            } => {
                let o = *offset as usize;
                let l = *len as usize;
                &parent.data[o..o + l]
            }
            BexStr::Concat(c) => &c.flatten().data,
        }
    }

    /// XXH3-128 (seed zero) of the logical UTF-8 bytes. Heap-backed strings
    /// cache the result; inline strings compute it directly. The first call on
    /// a concat flattens it, sharing the flat allocation/cache with its clones.
    /// This is separate from `Hash`, whose `Borrow<str>` contract must be retained.
    pub fn content_hash(&self) -> u128 {
        match self {
            Self::Inline { len, data } => xxhash_rust::xxh3::xxh3_128(&data[..inline_len(*len)]),
            Self::Flat(flat) => cached_hash(&flat.hash, &flat.data),
            Self::Slice { hash, .. } => cached_hash(&hash.hash, self.as_bytes()),
            Self::Concat(node) => {
                let flat = node.flatten();
                cached_hash(&flat.hash, &flat.data)
            }
        }
    }

    /// UTF-8 view; concatenations are flattened on first access.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: BexStr is always constructed from valid UTF-8.
        #[allow(unsafe_code)]
        unsafe {
            std::str::from_utf8_unchecked(self.as_bytes())
        }
    }

    /// Zero-copy substring by byte range. Enforces depth-1 invariant.
    /// Panics on reversed/out-of-bounds ranges or non-UTF-8 boundaries.
    #[inline]
    pub fn byte_slice(&self, range: Range<usize>) -> BexStr {
        let Range { start, end } = range;
        let bytes = self
            .as_str()
            .get(start..end)
            .expect("invalid UTF-8 byte slice")
            .as_bytes();
        let slice_len = bytes.len();

        // Empty result → Inline
        if slice_len == 0 {
            return BexStr::empty();
        }

        // Short result → Inline
        if slice_len <= INLINE_CAPACITY {
            return inline_bytes(bytes);
        }
        self.long_byte_slice(start..end)
    }

    fn long_byte_slice(&self, range: Range<usize>) -> BexStr {
        let Range { start, end } = range;
        let slice_len = end - start;
        // Long result → Slice (depth-1 invariant). The receiver's own ASCII
        // prefix covers the slice's start when `start` falls inside it; only
        // otherwise does the slice need a scan, fused with its codepoint count.
        let bytes = &self.as_bytes()[start..end];
        let outer_prefix = self.ascii_prefix_len();
        let (prefix, codepoint_len) = if start < outer_prefix {
            let prefix = (outer_prefix - start).min(slice_len);
            (prefix, prefix + bytecount::num_chars(&bytes[prefix..]))
        } else {
            ascii_prefix_and_codepoint_len(bytes)
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
            BexStr::Concat(c) => (c.flatten().clone(), start as u64),
        };
        let char_offset = u32::try_from(parent.byte_to_char(offset as usize)).unwrap_or(u32::MAX);
        BexStr::Slice {
            parent,
            offset,
            len: slice_len as u64,
            codepoint_len: codepoint_len as u64
                | if bytes.iter().all(|&byte| byte <= 0xc3) {
                    SLICE_LATIN1_FLAG
                } else {
                    0
                },
            char_offset,
            ascii_prefix_len,
            hash: Arc::new(SliceHash::new()),
        }
    }

    // ── Char-indexed methods ───────────────────────────────────────────
    //
    // ASCII is direct; heap Unicode shares a lazy index. Cold scans charge
    // their backing buffer, including scans needed to establish slice origins.

    /// Byte offset where char `n` starts, or `byte_len()` if `n >= codepoint_len()`.
    /// O(1) in the ASCII prefix and after the shared Unicode index is built.
    /// Cold Unicode scans start at the prefix and build the index once their
    /// accumulated work reaches the buffer's byte length. Buffers at or above
    /// the u32 byte limit keep scanning. Concats flatten as needed.
    #[inline]
    pub fn char_to_byte(&self, n: usize) -> usize {
        match self {
            Self::Flat(flat) => return flat.char_to_byte(n),
            Self::Slice {
                parent,
                offset,
                len,
                codepoint_len,
                ascii_prefix_len,
                char_offset,
                ..
            } if *ascii_prefix_len != u32::MAX => {
                if n <= *ascii_prefix_len as usize {
                    return n;
                }
                if n >= (*codepoint_len & SLICE_COUNT_MASK) as usize {
                    return *len as usize;
                }
                if let Some(index) = parent.char_index.get() {
                    let base = slice_char_offset(parent, *offset, *char_offset);
                    return index.offsets[base + n - parent.ascii_prefix_len as usize] as usize
                        - *offset as usize;
                }
            }
            Self::Concat(node) => {
                if n <= node.ascii_prefix_len as usize {
                    return n;
                }
                if n >= node.codepoint_len as usize {
                    return node.total_len as usize;
                }
                return node.flatten().char_to_byte(n);
            }
            Self::Inline { len, data } => {
                let bytes = &data[..inline_len(*len)];
                let end = n.min(bytes.len());
                if *len & INLINE_ASCII_FLAG != 0 || bytes[..end].is_ascii() {
                    return end;
                }
                return scan_char_to_byte(bytes, n);
            }
            _ => {}
        }
        self.char_to_byte_from((0, 0), n)
    }

    /// Char index of the char starting at byte `b`, which must be a char
    /// boundary (`codepoint_len()` for `b >= len()`). Inverse of `char_to_byte`.
    /// O(1) in ASCII, O(log n) with the Unicode index, otherwise a prefix
    /// count. Short ranges from a known position count locally.
    #[inline]
    pub fn byte_to_char(&self, b: usize) -> usize {
        self.byte_to_char_from((0, 0), b)
    }

    /// `char_to_byte(n)`, given that char `known.0` starts at byte `known.1`
    /// (`known.0 <= n`; past the end, `known.1` may be `byte_len()`): the scan
    /// starts from whichever of that point and the end of the ASCII prefix is
    /// further along.
    fn char_to_byte_from(&self, known: (usize, usize), n: usize) -> usize {
        // At most 54 bytes: scanning directly beats finding the prefix first.
        if let BexStr::Inline { len, data } = self {
            let bytes = &data[..inline_len(*len)];
            let end = n.min(bytes.len());
            if *len & INLINE_ASCII_FLAG != 0 || bytes[..end].is_ascii() {
                return end;
            }
            return known.1 + scan_char_to_byte(&bytes[known.1..], n - known.0);
        }
        let prefix = self.ascii_prefix_len();
        if n <= prefix {
            return n;
        }
        if n >= self.codepoint_len() {
            return self.byte_len();
        }
        let (char_idx, byte_idx) = if known.0 > prefix {
            known
        } else {
            (prefix, prefix)
        };
        debug_assert!(char_idx <= n);
        let flat = match self {
            Self::Flat(flat) => &**flat,
            Self::Concat(node) => &**node.flatten(),
            Self::Slice {
                parent,
                offset,
                char_offset,
                ..
            } => {
                if parent.char_index.get().is_some() {
                    let base = slice_char_offset(parent, *offset, *char_offset);
                    let absolute = base + n;
                    let byte = if absolute <= parent.ascii_prefix_len as usize {
                        absolute
                    } else {
                        parent.indexed_char_to_byte(absolute).unwrap()
                    };
                    return byte - *offset as usize;
                }
                &**parent
            }
            Self::Inline { .. } => unreachable!(),
        };
        if !matches!(self, Self::Slice { .. })
            && let Some(b) = flat.indexed_char_to_byte(n)
        {
            return b;
        }
        let byte = byte_idx + scan_char_to_byte(&self.as_bytes()[byte_idx..], n - char_idx);
        flat.charge_scan(byte - byte_idx);
        byte
    }

    /// `byte_to_char(b)`, given that byte `known.0` starts char `known.1`
    /// (`known.0 <= b`), counting from whichever of that point and the end of
    /// the ASCII prefix is further along.
    fn byte_to_char_from(&self, known: (usize, usize), b: usize) -> usize {
        if let BexStr::Inline { len, data } = self {
            let bytes = &data[..inline_len(*len)];
            if *len & INLINE_ASCII_FLAG != 0 {
                return b.min(bytes.len());
            }
            return known.1 + bytecount::num_chars(&bytes[known.0..b.min(bytes.len())]);
        }
        let prefix = self.ascii_prefix_len();
        if b <= prefix {
            return b;
        }
        if b >= self.byte_len() {
            return self.codepoint_len();
        }
        let (byte_idx, char_idx) = if known.0 > prefix {
            known
        } else {
            (prefix, prefix)
        };
        debug_assert!(byte_idx <= b);
        let flat = match self {
            Self::Flat(flat) => Some(&**flat),
            Self::Concat(node) => Some(&**node.flatten()),
            _ => None,
        };
        if b - byte_idx > 64
            && let Some(flat) = flat
            && let Some(n) = flat.indexed_byte_to_char(b)
        {
            return n;
        }
        let result = char_idx + bytecount::num_chars(&self.as_bytes()[byte_idx..b]);
        if let Some(flat) = flat {
            flat.charge_scan(b - byte_idx);
        }
        result
    }

    /// Byte storage and position for one codepoint. Slices use the parent's
    /// absolute position directly, avoiding repeated view bounds/subtractions.
    #[inline]
    fn char_location(&self, n: usize) -> (&[u8], usize) {
        match self {
            Self::Flat(flat) => (&flat.data, flat.char_to_byte(n)),
            Self::Concat(node) => {
                let flat = node.flatten();
                (&flat.data, flat.char_to_byte(n))
            }
            Self::Slice {
                parent,
                offset,
                codepoint_len,
                ascii_prefix_len,
                char_offset,
                ..
            } => {
                let count = (*codepoint_len & SLICE_COUNT_MASK) as usize;
                if n >= count {
                    return (&[], 0);
                }
                let b = if *ascii_prefix_len != u32::MAX && n <= *ascii_prefix_len as usize {
                    *offset as usize + n
                } else if let Some(index) = parent.char_index.get() {
                    let base = slice_char_offset(parent, *offset, *char_offset);
                    index.offsets[base + n - parent.ascii_prefix_len as usize] as usize
                } else {
                    *offset as usize + self.char_to_byte(n)
                };
                (&parent.data, b)
            }
            Self::Inline { .. } => (self.as_bytes(), self.char_to_byte(n)),
        }
    }

    #[inline(always)]
    fn leaf_char_at(&self, n: usize) -> Option<char> {
        match self {
            Self::Flat(flat) => flat.char_at(n),
            Self::Slice {
                parent,
                offset,
                codepoint_len,
                char_offset,
                ascii_prefix_len,
                ..
            } => {
                if n >= (*codepoint_len & SLICE_COUNT_MASK) as usize {
                    return None;
                }
                if *ascii_prefix_len != u32::MAX && n < *ascii_prefix_len as usize {
                    return parent
                        .data
                        .get(*offset as usize + n)
                        .map(|&byte| byte as char);
                }
                if let Some(index) = parent.char_index.get() {
                    let base = slice_char_offset(parent, *offset, *char_offset);
                    let point = index
                        .points
                        .get(base + n - parent.ascii_prefix_len as usize);
                    // A narrow view can retain a parent with astral text outside
                    // the view. Preserve its scalar bound in optimized callers.
                    return if *codepoint_len & SLICE_LATIN1_FLAG != 0 {
                        point.map(|point| point as u8 as char)
                    } else {
                        point
                    };
                }
                let (bytes, b) = self.char_location(n);
                char_at_byte(bytes, b)
            }
            _ => {
                let (bytes, b) = self.char_location(n);
                char_at_byte(bytes, b)
            }
        }
    }

    /// The char at index `n`, or `None` if `n >= codepoint_len()`.
    #[inline(always)]
    pub fn char_at(&self, n: usize) -> Option<char> {
        if let Self::Concat(node) = self {
            if let Some(flat) = node.flat.get() {
                return flat.char_at(n);
            }
            if let Some(ch) = node.leaf_char(n) {
                return ch;
            }
            return node.flatten().char_at(n);
        }
        self.leaf_char_at(n)
    }

    /// One-codepoint string at `n`, or `None` past the end. Bounds checking
    /// shares the indexing work rather than counting an inline string first.
    #[inline(always)]
    pub fn char_get(&self, n: usize) -> Option<BexStr> {
        // The cached prefix proves this is ASCII; no decode/width test or
        // separate slice-codepoint bound is needed in that prefix.
        let ascii = match self {
            Self::Inline { len, data } if *len & INLINE_ASCII_FLAG != 0 => {
                data.get(n).filter(|_| n < inline_len(*len)).copied()
            }
            Self::Flat(flat) if n < flat.ascii_prefix_len as usize => flat.data.get(n).copied(),
            Self::Slice {
                parent,
                offset,
                ascii_prefix_len,
                ..
            } if *ascii_prefix_len != u32::MAX && n < *ascii_prefix_len as usize => {
                parent.data.get((*offset as usize).checked_add(n)?).copied()
            }
            _ => None,
        };
        if let Some(byte) = ascii {
            let mut data = [0; INLINE_CAPACITY];
            data[0] = byte;
            return Some(Self::Inline {
                len: 1 | INLINE_ASCII_FLAG,
                data,
            });
        }
        let point = self.char_at(n)?;
        let mut data = [0; INLINE_CAPACITY];
        if point.is_ascii() {
            data[0] = point as u8;
            return Some(Self::Inline {
                len: 1 | INLINE_ASCII_FLAG,
                data,
            });
        }
        let len = point.encode_utf8(&mut data[..4]).len() as u8;
        Some(Self::Inline { len, data })
    }

    /// Substring by char range. Clamps both ends to `codepoint_len()` (and `end`
    /// to at least `start`), so it never panics.
    #[inline(always)]
    pub fn char_slice(&self, range: Range<usize>) -> BexStr {
        if range.end.checked_sub(range.start) == Some(1) {
            return self.char_get(range.start).unwrap_or_else(Self::empty);
        }
        // A tail read after an append can use a leaf directly. Keep this
        // bounded: walking a deep left spine on every scan would be quadratic.
        if let Self::Concat(node) = self
            && node.flat.get().is_none()
            && let Some(slice) = node.leaf_slice(range.clone())
        {
            return slice;
        }
        // The end scan starts at the known start, and both bounds clamp.
        let byte_start = self.char_to_byte(range.start);
        let end = range.end.max(range.start);
        let byte_end = self.char_to_byte_from((range.start, byte_start), end);
        self.byte_slice(byte_start..byte_end)
    }

    /// Finds `needle` and returns its char index, or `None`.
    #[inline]
    pub fn char_find(&self, needle: &str) -> Option<usize> {
        self.char_find_from(needle, 0)
    }

    /// Finds `needle` at or after char `start`, returning an absolute char
    /// index. A start past the end returns `None`, even for an empty needle.
    /// Searches the borrowed suffix without allocating, and counts chars only
    /// between `start` and the match.
    #[inline]
    pub fn char_find_from(&self, needle: &str, start: usize) -> Option<usize> {
        if needle.is_empty() && start > self.codepoint_len() {
            return None;
        }
        let byte_start = self.char_to_byte(start);
        let relative = memchr::memmem::find(&self.as_bytes()[byte_start..], needle.as_bytes())?;
        Some(self.byte_to_char_from((byte_start, start), byte_start + relative))
    }

    /// Finds the *last* occurrence of `needle` and returns the char index of
    /// its start, or `None`.
    #[inline]
    pub fn char_rfind(&self, needle: &str) -> Option<usize> {
        let byte_idx = memchr::memmem::rfind(self.as_bytes(), needle.as_bytes())?;
        Some(self.byte_to_char(byte_idx))
    }

    /// Repeats this string `n` times.
    pub fn repeat(&self, n: usize) -> BexStr {
        BexStr::from(self.as_str().repeat(n))
    }

    /// Deferred concatenation for shared/large operands. Short appends to
    /// exclusively owned storage grow in amortized O(right.byte_len()) time.
    pub fn concat(mut left: BexStr, right: BexStr) -> BexStr {
        if left.is_empty() {
            return right;
        }
        if right.is_empty() {
            return left;
        }
        let total_len = left.byte_len() + right.byte_len();

        // Short result → Inline
        if total_len <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            let left_bytes = left.as_bytes();
            let right_bytes = right.as_bytes();
            data[..left_bytes.len()].copy_from_slice(left_bytes);
            data[left_bytes.len()..total_len].copy_from_slice(right_bytes);
            return BexStr::Inline {
                len: inline_len_and_ascii(total_len, left.is_ascii() && right.is_ascii()),
                data,
            };
        }

        // Once an owned append chain is large enough to benefit, move its
        // flattened buffer out of the unique node. Ordinary/shared concats
        // retain their deferred semantics, including the first short concat.
        if matches!(right, Self::Inline { .. })
            && let Self::Concat(node) = &left
            && Arc::strong_count(node) == 1
            && node.total_len >= 128
        {
            let flat = node.flatten().clone();
            left = Self::Flat(flat); // releases the node's reference to flat
        }

        let (left_prefix, left_chars) = left.ascii_prefix_and_codepoint_len();
        let (right_prefix, right_chars) = right.ascii_prefix_and_codepoint_len();
        let codepoint_len = (left_chars + right_chars) as u64;
        let ascii_prefix_len = if left_prefix == left.byte_len() {
            left.byte_len() + right_prefix
        } else {
            left_prefix
        } as u64;
        if let Self::Flat(flat) = &mut left
            && matches!(right, Self::Inline { .. })
            && let Some(flat) = Arc::get_mut(flat)
        {
            let old_len = flat.data.len();
            let bytes = right.as_bytes();
            flat.data.extend_from_slice(bytes);
            if let Some(index) = flat.char_index.get_mut() {
                index.offsets.pop(); // replace the end sentinel with new boundaries
                for (i, point) in right.as_str().char_indices() {
                    index.offsets.push((old_len + i) as u32);
                    index.points.push(point);
                }
                index.offsets.push(total_len as u32);
                // u32 offsets are valid only below the index's byte limit.
                if total_len >= u32::MAX as usize {
                    flat.char_index.take();
                }
            }
            flat.codepoint_len = codepoint_len;
            flat.ascii_prefix_len = ascii_prefix_len;
            for hash in &mut flat.hash {
                *hash.get_mut() = 0;
            }
            return left;
        }

        // Pack short appends into the last inline leaf when this handle owns
        // the node exclusively. No shared string or borrowed bytes can change.
        // This cuts allocation/destruction work without changing the rope's
        // deferred UTF-8 representation or flattening an existing prefix.
        if let Self::Concat(node) = &mut left
            && let Some(node) = Arc::get_mut(node)
            && let Ok(ConcatState::Deferred { right: tail, .. }) = node.state.get_mut()
            && let (
                Self::Inline { len, data },
                Self::Inline {
                    len: added,
                    data: bytes,
                },
            ) = (tail, &right)
            && (inline_len(*len) + inline_len(*added)) <= INLINE_CAPACITY
        {
            let start = inline_len(*len);
            let added_len = inline_len(*added);
            data[start..start + added_len].copy_from_slice(&bytes[..added_len]);
            *len = (start + added_len) as u8 | (*len & *added & INLINE_ASCII_FLAG);
            node.total_len = total_len as u64;
            node.codepoint_len = codepoint_len;
            node.ascii_prefix_len = ascii_prefix_len;
            return left;
        }
        BexStr::Concat(Arc::new(ConcatNode {
            total_len: total_len as u64,
            codepoint_len,
            ascii_prefix_len,
            state: Mutex::new(ConcatState::Deferred { left, right }),
            flat: OnceLock::new(),
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
                    len: al,
                    ..
                },
                BexStr::Slice {
                    parent: b,
                    offset: bo,
                    len: bl,
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
            BexStr::Slice { hash, .. } => read_hash(&hash.hash),
            BexStr::Concat(c) => c.flat.get().and_then(|f| read_hash(&f.hash)),
        }
    }
}

// ── FlatStr ────────────────────────────────────────────────────────

impl FlatStr {
    fn byte_to_char(&self, b: usize) -> usize {
        let prefix = self.ascii_prefix_len as usize;
        if b <= prefix {
            return b;
        }
        if b >= self.data.len() {
            return self.codepoint_len as usize;
        }
        if let Some(n) = self.indexed_byte_to_char(b) {
            return n;
        }
        let n = prefix + bytecount::num_chars(&self.data[prefix..b]);
        self.charge_scan(b - prefix);
        n
    }

    #[inline]
    fn char_to_byte(&self, n: usize) -> usize {
        let prefix = self.ascii_prefix_len as usize;
        if n <= prefix {
            return n;
        }
        if n >= self.codepoint_len as usize {
            return self.data.len();
        }
        if let Some(index) = self.char_index.get() {
            return index.offsets[n - prefix] as usize;
        }
        self.scan_char_to_byte(n)
    }

    #[inline(always)]
    fn char_at(&self, n: usize) -> Option<char> {
        let prefix = self.ascii_prefix_len as usize;
        if n < prefix {
            return self.data.get(n).map(|&byte| byte as char);
        }
        if let Some(index) = self.char_index.get() {
            return index.points.get(n - prefix);
        }
        char_at_byte(&self.data, self.char_to_byte(n))
    }

    fn scan_char_to_byte(&self, n: usize) -> usize {
        let prefix = self.ascii_prefix_len as usize;
        let b = prefix + scan_char_to_byte(&self.data[prefix..], n - prefix);
        self.charge_scan(b - prefix);
        b
    }
    /// Wraps `data`, computing its codepoint count and ASCII prefix in one pass.
    fn new(data: Vec<u8>) -> FlatStr {
        let (prefix, codepoint_len) = ascii_prefix_and_codepoint_len(&data);
        FlatStr::from_parts(data, codepoint_len as u64, prefix as u64)
    }

    /// Wraps `data` whose codepoint count and ASCII prefix are already known.
    fn from_parts(data: Vec<u8>, codepoint_len: u64, ascii_prefix_len: u64) -> FlatStr {
        debug_assert_eq!(
            ascii_prefix_and_codepoint_len(&data),
            (ascii_prefix_len as usize, codepoint_len as usize)
        );
        FlatStr {
            hash: [const { AtomicU64::new(0) }; 2],
            codepoint_len,
            ascii_prefix_len,
            data,
            char_index: OnceLock::new(),
            scan_debt: AtomicUsize::new(0),
        }
    }

    /// Four bytes of offset and 1/2/4 bytes of scalar per indexed codepoint,
    /// plus an offset end sentinel. Construction is tight; owned appends grow
    /// capacity geometrically and can promote the scalar width.
    /// Strings at or above the u32 byte limit keep the allocation-free scanner.
    fn index(&self) -> Option<&CharIndex> {
        if self.data.len() >= u32::MAX as usize || self.ascii_prefix_len == self.codepoint_len {
            return None;
        }
        let count = self.codepoint_len as usize - self.ascii_prefix_len as usize + 1;
        // In particular on wasm32, a valid text buffer can need an offset
        // table larger than Vec's isize::MAX allocation limit. Keep scanning.
        if count > isize::MAX as usize / std::mem::size_of::<u32>() {
            return None;
        }
        Some(self.char_index.get_or_init(|| {
            let prefix = self.ascii_prefix_len as usize;
            let mut offsets = Vec::with_capacity(count);
            let mut points = Vec::with_capacity(count - 1);
            let mut max = 0;
            // SAFETY: constructors and concat maintain valid canonical UTF-8.
            #[allow(unsafe_code)]
            let text = unsafe { std::str::from_utf8_unchecked(&self.data[prefix..]) };
            for (i, point) in text.char_indices() {
                offsets.push((prefix + i) as u32);
                points.push(point);
                max = max.max(point as u32);
            }
            offsets.push(self.data.len() as u32);
            CharIndex {
                offsets,
                points: ScalarCache::from_points(points, max),
            }
        }))
    }

    fn index_heap_bytes(&self) -> usize {
        self.char_index.get().map_or(0, |index| {
            index.offsets.capacity() * std::mem::size_of::<u32>() + index.points.heap_bytes()
        })
    }

    /// Bound a tight index's u32 offsets and four-byte scalars without
    /// allocating it. Owned append capacities are accounted for separately.
    fn index_bound_bytes(&self) -> Option<usize> {
        if self.data.len() >= u32::MAX as usize || self.ascii_prefix_len == self.codepoint_len {
            return Some(0);
        }
        let points = (self.codepoint_len - self.ascii_prefix_len) as usize;
        let offsets = points.checked_add(1)?;
        if offsets > isize::MAX as usize / std::mem::size_of::<u32>() {
            return Some(0);
        }
        offsets
            .checked_mul(std::mem::size_of::<u32>())?
            .checked_add(points.checked_mul(std::mem::size_of::<char>())?)
    }

    fn charge_scan(&self, bytes: usize) {
        let debt = self.scan_debt.fetch_add(bytes, Ordering::Relaxed);
        if bytes >= self.data.len().saturating_sub(debt) {
            self.index();
        }
    }

    #[inline]
    fn indexed_char_to_byte(&self, n: usize) -> Option<usize> {
        self.char_index
            .get()
            .map(|index| index.offsets[n - self.ascii_prefix_len as usize] as usize)
    }

    #[inline]
    fn indexed_byte_to_char(&self, b: usize) -> Option<usize> {
        self.char_index.get().map(|index| {
            self.ascii_prefix_len as usize
                + index
                    .offsets
                    .partition_point(|&offset| (offset as usize) < b)
        })
    }
}

// ── ConcatNode ─────────────────────────────────────────────────────

// Invariants: aggregates are cached at construction; traversals and destruction
// use heap stacks rather than native recursion; the flat result is published
// exactly once. Shared deferred children are never destructively consumed.
impl ConcatNode {
    fn leaf_char(&self, n: usize) -> Option<Option<char>> {
        let guard = self.state.lock().unwrap();
        if let ConcatState::Deferred { left, right } = &*guard {
            let split = left.codepoint_len();
            if n >= split && !matches!(right, BexStr::Concat(_)) {
                return Some(right.leaf_char_at(n - split));
            }
            if n < split && !matches!(left, BexStr::Concat(_)) {
                return Some(left.leaf_char_at(n));
            }
        }
        None
    }
    fn leaf_slice(&self, range: Range<usize>) -> Option<BexStr> {
        let guard = self.state.lock().unwrap();
        if let ConcatState::Deferred { left, right } = &*guard {
            let split = left.codepoint_len();
            if range.start >= split && !matches!(right, BexStr::Concat(_)) {
                return Some(
                    right.char_slice(range.start - split..range.end.saturating_sub(split)),
                );
            }
            if range.end <= split && !matches!(left, BexStr::Concat(_)) {
                return Some(left.char_slice(range));
            }
        }
        None
    }
    /// Iterative flatten. The returned Arc belongs to this node for its entire
    /// remaining lifetime; OnceLock gives readers both publication and a borrow.
    #[inline]
    pub(crate) fn flatten(&self) -> &Arc<FlatStr> {
        if let Some(flat) = self.flat.get() {
            return flat;
        }
        self.flatten_slow()
    }

    fn flatten_slow(&self) -> &Arc<FlatStr> {
        self.flat.get_or_init(|| {
            let mut guard = self.state.lock().unwrap();
            let old = std::mem::replace(&mut *guard, ConcatState::Flattened);
            let ConcatState::Deferred { left, right } = old else {
                unreachable!()
            };
            let mut buf = Vec::with_capacity(self.total_len as usize);
            let mut stack = vec![right, left];
            while let Some(node) = stack.pop() {
                match node {
                    BexStr::Inline { len, data } => buf.extend_from_slice(&data[..inline_len(len)]),
                    BexStr::Flat(f) => buf.extend_from_slice(&f.data),
                    BexStr::Slice {
                        parent,
                        offset,
                        len,
                        ..
                    } => {
                        let start = offset as usize;
                        buf.extend_from_slice(&parent.data[start..start + len as usize]);
                    }
                    BexStr::Concat(c) => {
                        if let Some(flat) = c.flat.get() {
                            buf.extend_from_slice(&flat.data);
                        } else {
                            let inner = c.state.lock().unwrap();
                            match &*inner {
                                ConcatState::Deferred { left, right } => {
                                    stack.push(right.clone());
                                    stack.push(left.clone());
                                }
                                ConcatState::Flattened => {
                                    // The builder owns the state lock until its bytes
                                    // are complete; wait for OnceLock publication.
                                    drop(inner);
                                    buf.extend_from_slice(&c.flatten().data);
                                }
                            }
                        }
                    }
                }
            }
            Arc::new(FlatStr::from_parts(
                buf,
                self.codepoint_len,
                self.ascii_prefix_len,
            ))
        })
    }
}

/// Iterative Drop to avoid stack overflow on deep left-leaning trees.
impl Drop for ConcatNode {
    fn drop(&mut self) {
        let Ok(state) = self.state.get_mut() else {
            return;
        };
        let ConcatState::Deferred { left, right } =
            std::mem::replace(state, ConcatState::Flattened)
        else {
            return;
        };
        let mut stack = vec![left, right];
        while let Some(node) = stack.pop() {
            if let BexStr::Concat(arc) = node
                && let Ok(mut owned) = Arc::try_unwrap(arc)
                && let Ok(state) = owned.state.get_mut()
                && let ConcatState::Deferred { left, right } =
                    std::mem::replace(state, ConcatState::Flattened)
            {
                stack.push(left);
                stack.push(right);
            }
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
                len,
                codepoint_len,
                char_offset,
                ascii_prefix_len,
                hash,
            } => BexStr::Slice {
                parent: parent.clone(),
                offset: *offset,
                len: *len,
                codepoint_len: *codepoint_len,
                char_offset: *char_offset,
                ascii_prefix_len: *ascii_prefix_len,
                hash: hash.clone(),
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
            let mut data = [0u8; INLINE_CAPACITY];
            data[..s.len()].copy_from_slice(s.as_bytes());
            BexStr::Inline {
                len: inline_len_and_ascii(s.len(), s.is_ascii()),
                data,
            }
        } else {
            BexStr::Flat(Arc::new(FlatStr::new(
                s.into_bytes().into_boxed_slice().into_vec(),
            )))
        }
    }
}

impl From<&str> for BexStr {
    fn from(s: &str) -> Self {
        if s.len() <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            data[..s.len()].copy_from_slice(s.as_bytes());
            BexStr::Inline {
                len: inline_len_and_ascii(s.len(), s.is_ascii()),
                data,
            }
        } else {
            BexStr::Flat(Arc::new(FlatStr::new(s.as_bytes().to_vec())))
        }
    }
}

// ── Helper functions ──────────────────────────────────────────────────

#[inline]
fn char_at_byte(bytes: &[u8], b: usize) -> Option<char> {
    let &lead = bytes.get(b)?;
    if lead.is_ascii() {
        return Some(lead as char);
    }
    // SAFETY: b is a codepoint boundary in valid UTF-8.
    #[allow(unsafe_code)]
    unsafe {
        std::str::from_utf8_unchecked(&bytes[b..]).chars().next()
    }
}

#[inline]
fn slice_char_offset(parent: &FlatStr, byte: u64, cached: u32) -> usize {
    if cached != u32::MAX {
        return cached as usize;
    }
    parent.byte_to_char(byte as usize)
}

/// Constant-width copies for the very common one-codepoint result. The
/// general short-string path still copies at most INLINE_CAPACITY bytes.
#[inline]
fn inline_bytes(bytes: &[u8]) -> BexStr {
    let mut data = [0; INLINE_CAPACITY];
    match bytes.len() {
        1 => data[0] = bytes[0],
        2 => data[..2].copy_from_slice(bytes),
        3 => data[..3].copy_from_slice(bytes),
        4 => data[..4].copy_from_slice(bytes),
        len => data[..len].copy_from_slice(bytes),
    }
    BexStr::Inline {
        len: inline_len_and_ascii(bytes.len(), bytes.is_ascii()),
        data,
    }
}

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

/// `(ascii_prefix_len, codepoint_len)` of `bytes`. Counts first: when every byte
/// starts a char, the whole string is ASCII and needs no second scan, so
/// ASCII strings cost exactly one (vectorized) count.
fn ascii_prefix_and_codepoint_len(bytes: &[u8]) -> (usize, usize) {
    let codepoint_len = bytecount::num_chars(bytes);
    if codepoint_len == bytes.len() {
        (codepoint_len, codepoint_len)
    } else {
        (ascii_prefix_len(bytes), codepoint_len)
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
