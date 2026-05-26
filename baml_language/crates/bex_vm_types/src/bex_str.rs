// The unsafe code in this file is intentional and necessary for the BexStr design.
// - `as_bytes` for Concat: the returned slice borrows from the Arc<FlatStr> stored inside
//   the ConcatNode's Mutex. Safety is upheld because flatten() stores the Arc into the Mutex
//   before returning, so it lives at least as long as this BexStr::Concat.
// - `as_str`: delegates to `as_bytes` and relies on the UTF-8 invariant maintained at
//   all construction sites.
#![allow(unsafe_code)]
// Casts to u8/u32: all guarded by INLINE_CAPACITY bound (54, fits u8) or by
// BexStr length being constrained to fit in u32 (4GB strings are not supported).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::explicit_auto_deref)]

use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// Maximum bytes stored inline without heap allocation.
/// Computed to keep size_of::<BexStr>() within the Object enum payload budget.
/// Object is <= 80 bytes. BexStr's Inline variant is: 1 (discriminant) + 1 (len) + INLINE_CAPACITY.
/// We target size_of::<BexStr>() == 56 bytes to avoid growing Object.
/// 56 - 2 (discriminant + len byte) = 54 bytes inline capacity.
/// Exact value tuned by the static assertion below.
const INLINE_CAPACITY: usize = 54;

/// V8-inspired string representation. Immutable. O(1) clone.
/// Stored inside Object::String(BexStr).
pub enum BexStr {
    /// Strings <= INLINE_CAPACITY bytes: zero heap allocation.
    /// Clone = memcpy of the enum value (no atomics, no heap).
    Inline {
        len: u8,
        data: [u8; INLINE_CAPACITY],
    },

    /// Heap-allocated, ref-counted, immutable. For strings > INLINE_CAPACITY bytes.
    /// Clone = Arc refcount bump (single atomic increment).
    Flat(Arc<FlatStr>),

    /// Zero-copy view into a Flat string's buffer.
    /// Invariant: parent is ALWAYS Flat (never Slice, never Concat).
    /// Bounds indirection depth to exactly 1.
    /// Has its own cached hash since the slice's bytes differ from the parent's.
    Slice {
        parent: Arc<FlatStr>,
        offset: u32,
        len: u32,
        hash: AtomicU64,
    },

    /// Deferred concatenation. Flattened to Flat on first byte-level access.
    /// Makes repeated concatenation O(n) total instead of O(n^2).
    Concat(Arc<ConcatNode>),
}

/// Heap-allocated immutable string data with cached hash.
pub struct FlatStr {
    /// 0 = not yet computed. Set atomically on first hash() call.
    hash: AtomicU64,
    /// The UTF-8 byte data. Tight allocation (no excess capacity).
    data: Box<[u8]>,
}

/// Deferred concatenation node.
pub struct ConcatNode {
    pub total_len: u32,
    state: Mutex<ConcatState>,
}

enum ConcatState {
    /// Children alive — bytes not yet materialized.
    Deferred { left: BexStr, right: BexStr },
    /// Flattened — children dropped, memory released.
    Flattened(Arc<FlatStr>),
}

// Iterative Drop for ConcatNode.
//
// Why: building strings via `s = s + "x"` in a loop produces a left-deep
// Concat tree. With the auto-derived Drop, dropping such a tree recurses
// through every Concat → Deferred { left: Concat → … } level, which blows
// the stack at ~50k iterations. (V8 doesn't hit this because it's GC'd and
// has no destructors — destruction is iterative-by-design.)
//
// This impl walks the tree onto a heap-allocated work-stack and dismantles
// each owned ConcatNode in turn. Non-Concat variants (`Inline`, `Flat`,
// `Slice`) drop in O(1) without nesting. For `Arc<ConcatNode>` we only
// recurse when `Arc::try_unwrap` succeeds (we're the last owner); otherwise
// some other BexStr still references the subtree, and that owner will be
// responsible for dismantling it.
impl Drop for ConcatNode {
    fn drop(&mut self) {
        // Take this node's children out so we own them.
        let state = match self.state.get_mut() {
            Ok(s) => std::mem::replace(
                s,
                ConcatState::Flattened(Arc::new(FlatStr {
                    hash: AtomicU64::new(0),
                    data: Box::new([]),
                })),
            ),
            Err(_) => return, // Mutex poisoned — leak rather than risk UB
        };
        let (left, right) = match state {
            ConcatState::Deferred { left, right } => (left, right),
            ConcatState::Flattened(_) => return, // Already flattened — children gone
        };

        let mut stack: Vec<BexStr> = Vec::new();
        stack.push(left);
        stack.push(right);

        while let Some(node) = stack.pop() {
            // Only BexStr::Concat can chain to more BexStr children.
            // Inline/Flat/Slice drop in O(1) without nesting.
            if let BexStr::Concat(arc) = node {
                // If we own the last reference, take its children onto our work stack
                // instead of letting Rust's auto-drop recurse.
                match Arc::try_unwrap(arc) {
                    Ok(mut owned_node) => {
                        // Replace its state so its own Drop (running when `owned_node`
                        // goes out of scope) sees Flattened and returns immediately.
                        let inner_state = match owned_node.state.get_mut() {
                            Ok(s) => std::mem::replace(
                                s,
                                ConcatState::Flattened(Arc::new(FlatStr {
                                    hash: AtomicU64::new(0),
                                    data: Box::new([]),
                                })),
                            ),
                            Err(_) => continue,
                        };
                        if let ConcatState::Deferred { left, right } = inner_state {
                            stack.push(left);
                            stack.push(right);
                        }
                    }
                    Err(_arc) => {
                        // Another owner still holds this node — just drop the Arc
                        // (decrementing refcount). They'll handle dismantling.
                    }
                }
            }
            // Else: BexStr drops here, no recursion possible.
        }
    }
}

// --- Clone (manual — AtomicU64 doesn't derive Clone) ---

impl Clone for BexStr {
    fn clone(&self) -> Self {
        match self {
            BexStr::Inline { len, data } => BexStr::Inline {
                len: *len,
                data: *data,
            },
            BexStr::Flat(arc) => BexStr::Flat(arc.clone()),
            BexStr::Slice {
                parent,
                offset,
                len,
                hash,
            } => BexStr::Slice {
                parent: parent.clone(),
                offset: *offset,
                len: *len,
                hash: AtomicU64::new(hash.load(Ordering::Relaxed)),
            },
            BexStr::Concat(arc) => BexStr::Concat(arc.clone()),
        }
    }
}

// --- Size assertion ---
const _: () = assert!(
    std::mem::size_of::<BexStr>() <= 56,
    "BexStr size regression — must fit within Object payload"
);

impl BexStr {
    /// Empty string constant. Used for GC placeholders and defaults.
    pub const fn empty() -> Self {
        BexStr::Inline {
            len: 0,
            data: [0u8; INLINE_CAPACITY],
        }
    }

    /// Byte length of the string.
    pub fn len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } => *len as usize,
            BexStr::Flat(flat) => flat.data.len(),
            BexStr::Slice { len, .. } => *len as usize,
            BexStr::Concat(node) => node.total_len as usize,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Access the raw bytes. For Concat, triggers flattening.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BexStr::Inline { len, data } => &data[..*len as usize],
            BexStr::Flat(flat) => &flat.data,
            BexStr::Slice {
                parent,
                offset,
                len,
                ..
            } => &parent.data[*offset as usize..(*offset + *len) as usize],
            BexStr::Concat(node) => {
                let flat = node.flatten();
                // SAFETY: The Arc<FlatStr> returned by flatten() is stored inside the
                // ConcatNode's Mutex, so it lives as long as the ConcatNode (which lives
                // as long as this BexStr::Concat). We leak a reference here that is valid
                // for the lifetime of the Arc inside the Mutex.
                // This is safe because once flattened, the Arc<FlatStr> is never dropped
                // until the ConcatNode itself is dropped.
                unsafe {
                    let ptr = flat.data.as_ptr();
                    let len = flat.data.len();
                    std::slice::from_raw_parts(ptr, len)
                }
            }
        }
    }

    /// Access as &str.
    pub fn as_str(&self) -> &str {
        // SAFETY: BexStr is always constructed from valid UTF-8 data.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Create a BexStr from raw bytes (must be valid UTF-8).
    /// Inlines if <= INLINE_CAPACITY, otherwise heap-allocates.
    pub fn from_utf8_unchecked(bytes: &[u8]) -> Self {
        if bytes.len() <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            data[..bytes.len()].copy_from_slice(bytes);
            BexStr::Inline {
                len: bytes.len() as u8,
                data,
            }
        } else {
            BexStr::Flat(Arc::new(FlatStr {
                hash: AtomicU64::new(0),
                data: bytes.into(),
            }))
        }
    }

    /// Create a lazy concatenation node. If the combined length fits inline,
    /// eagerly flattens instead of creating a Concat node.
    pub fn concat(left: BexStr, right: BexStr) -> BexStr {
        let total_len = left.len() + right.len();
        if total_len <= INLINE_CAPACITY {
            // Short result — eagerly flatten into Inline (no heap alloc)
            let mut data = [0u8; INLINE_CAPACITY];
            let lb = left.as_bytes();
            let rb = right.as_bytes();
            data[..lb.len()].copy_from_slice(lb);
            data[lb.len()..lb.len() + rb.len()].copy_from_slice(rb);
            BexStr::Inline {
                len: total_len as u8,
                data,
            }
        } else {
            BexStr::Concat(Arc::new(ConcatNode {
                total_len: total_len as u32,
                state: Mutex::new(ConcatState::Deferred { left, right }),
            }))
        }
    }

    /// Create a substring view. Returns Inline copy for short results,
    /// Slice view for long results. Enforces depth-1 invariant.
    pub fn substring(&self, start: usize, end: usize) -> BexStr {
        let len = end - start;
        if len == 0 {
            return BexStr::empty();
        }
        match self {
            BexStr::Inline {
                data,
                len: inline_len,
            } => {
                debug_assert!(end <= *inline_len as usize);
                BexStr::from_utf8_unchecked(&data[start..end])
            }
            BexStr::Flat(flat) => {
                if len <= INLINE_CAPACITY {
                    BexStr::from_utf8_unchecked(&flat.data[start..end])
                } else {
                    BexStr::Slice {
                        parent: flat.clone(),
                        offset: start as u32,
                        len: len as u32,
                        hash: AtomicU64::new(0),
                    }
                }
            }
            BexStr::Slice {
                parent,
                offset,
                len: _,
                ..
            } => {
                // Re-slice: point to same parent with adjusted offset (depth stays 1)
                let new_offset = *offset as usize + start;
                if len <= INLINE_CAPACITY {
                    BexStr::from_utf8_unchecked(&parent.data[new_offset..new_offset + len])
                } else {
                    BexStr::Slice {
                        parent: parent.clone(),
                        offset: new_offset as u32,
                        len: len as u32,
                        hash: AtomicU64::new(0),
                    }
                }
            }
            BexStr::Concat(node) => {
                // Flatten first, then slice
                let flat = node.flatten();
                if len <= INLINE_CAPACITY {
                    BexStr::from_utf8_unchecked(&flat.data[start..end])
                } else {
                    BexStr::Slice {
                        parent: flat,
                        offset: start as u32,
                        len: len as u32,
                        hash: AtomicU64::new(0),
                    }
                }
            }
        }
    }

    /// Check if two BexStr values share the same backing memory (O(1) equality).
    pub fn ptr_eq(&self, other: &Self) -> bool {
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
}

impl ConcatNode {
    /// Flatten the concat tree into a single contiguous Arc<FlatStr>.
    /// Uses iterative traversal to avoid stack overflow on deep trees.
    pub fn flatten(&self) -> Arc<FlatStr> {
        let mut state = self.state.lock().unwrap();
        match &*state {
            ConcatState::Flattened(flat) => flat.clone(),
            ConcatState::Deferred { .. } => {
                // Take ownership of the deferred state
                let deferred = std::mem::replace(
                    &mut *state,
                    ConcatState::Flattened(Arc::new(FlatStr {
                        hash: AtomicU64::new(0),
                        data: Box::new([]),
                    })),
                );
                let ConcatState::Deferred { left, right } = deferred else {
                    unreachable!()
                };

                // Iterative flatten: collect leaves left-to-right
                let mut buf = Vec::with_capacity(self.total_len as usize);
                let mut stack: Vec<BexStr> = vec![right, left]; // left on top = processed first
                while let Some(node) = stack.pop() {
                    match node {
                        BexStr::Inline { len, data } => {
                            buf.extend_from_slice(&data[..len as usize]);
                        }
                        BexStr::Flat(flat) => {
                            buf.extend_from_slice(&flat.data);
                        }
                        BexStr::Slice {
                            parent,
                            offset,
                            len,
                            ..
                        } => {
                            buf.extend_from_slice(
                                &parent.data[offset as usize..(offset + len) as usize],
                            );
                        }
                        BexStr::Concat(inner_node) => {
                            let inner_state = inner_node.state.lock().unwrap();
                            match &*inner_state {
                                ConcatState::Flattened(flat) => {
                                    buf.extend_from_slice(&flat.data);
                                }
                                ConcatState::Deferred { left, right } => {
                                    // Push right first so left is processed first
                                    stack.push(right.clone());
                                    stack.push(left.clone());
                                }
                            }
                            drop(inner_state);
                        }
                    }
                }

                let flat = Arc::new(FlatStr {
                    hash: AtomicU64::new(0),
                    data: buf.into_boxed_slice(),
                });
                *state = ConcatState::Flattened(flat.clone());
                flat
            }
        }
    }
}

// --- From impls ---

impl From<&str> for BexStr {
    fn from(s: &str) -> Self {
        BexStr::from_utf8_unchecked(s.as_bytes())
    }
}

impl From<String> for BexStr {
    fn from(s: String) -> Self {
        if s.len() <= INLINE_CAPACITY {
            BexStr::from(s.as_str())
        } else {
            BexStr::Flat(Arc::new(FlatStr {
                hash: AtomicU64::new(0),
                data: s.into_bytes().into_boxed_slice(),
            }))
        }
    }
}

impl From<&String> for BexStr {
    fn from(s: &String) -> Self {
        BexStr::from(s.as_str())
    }
}

// --- Deref to &str ---

impl Deref for BexStr {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for BexStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// --- PartialEq / Eq ---

impl PartialEq for BexStr {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        if self.ptr_eq(other) {
            return true;
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

impl PartialEq<&str> for BexStr {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<BexStr> for &str {
    fn eq(&self, other: &BexStr) -> bool {
        *self == other.as_str()
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

// --- PartialOrd / Ord ---

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

// --- Hash ---

impl Hash for BexStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the bytes directly — compatible with str's Hash impl.
        // The AtomicU64 cached hashes on FlatStr and Slice are available for
        // future use in BexStr-keyed maps with a fixed hasher. For now, we
        // always hash through the Hasher trait for compatibility with HashMap's
        // RandomState (which produces different hashes per process).
        self.as_str().hash(state);
    }
}

impl BexStr {
    /// Compute or retrieve a stable (non-RandomState) hash for this string.
    /// Useful for equality fast-paths and future BexStr-keyed maps.
    pub fn stable_hash(&self) -> u64 {
        match self {
            BexStr::Inline { .. } => {
                // Inline strings are short — just compute on the fly
                let mut h = std::collections::hash_map::DefaultHasher::new();
                self.as_str().hash(&mut h);
                h.finish()
            }
            BexStr::Flat(flat) => {
                let cached = flat.hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return cached;
                }
                let mut h = std::collections::hash_map::DefaultHasher::new();
                flat.data.hash(&mut h);
                let val = h.finish();
                let val = if val == 0 { 1 } else { val }; // avoid sentinel
                flat.hash.store(val, Ordering::Relaxed);
                val
            }
            BexStr::Slice { hash, .. } => {
                let cached = hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return cached;
                }
                let mut h = std::collections::hash_map::DefaultHasher::new();
                self.as_bytes().hash(&mut h);
                let val = h.finish();
                let val = if val == 0 { 1 } else { val }; // avoid sentinel
                hash.store(val, Ordering::Relaxed);
                val
            }
            BexStr::Concat(node) => {
                // Flatten first (caches on the resulting FlatStr)
                let flat = node.flatten();
                let cached = flat.hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return cached;
                }
                let mut h = std::collections::hash_map::DefaultHasher::new();
                flat.data.hash(&mut h);
                let val = h.finish();
                let val = if val == 0 { 1 } else { val };
                flat.hash.store(val, Ordering::Relaxed);
                val
            }
        }
    }
}

// --- Display / Debug ---

impl fmt::Display for BexStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for BexStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

// --- Default ---

impl Default for BexStr {
    fn default() -> Self {
        BexStr::empty()
    }
}

// ToString is provided automatically via the Display impl (blanket impl in alloc).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_short_string() {
        let s = BexStr::from("hello");
        assert!(matches!(s, BexStr::Inline { .. }));
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_flat_long_string() {
        let long = "a".repeat(INLINE_CAPACITY + 1);
        let s = BexStr::from(long.as_str());
        assert!(matches!(s, BexStr::Flat(_)));
        assert_eq!(s.as_str(), long.as_str());
    }

    #[test]
    fn test_from_string_reuses_allocation() {
        let long = "b".repeat(INLINE_CAPACITY + 10);
        let s = BexStr::from(long.clone());
        assert!(matches!(s, BexStr::Flat(_)));
        assert_eq!(s.as_str(), long.as_str());
    }

    #[test]
    fn test_inline_boundary() {
        let exact = "x".repeat(INLINE_CAPACITY);
        let s = BexStr::from(exact.as_str());
        assert!(matches!(s, BexStr::Inline { .. }));
        assert_eq!(s.len(), INLINE_CAPACITY);

        let one_over = "x".repeat(INLINE_CAPACITY + 1);
        let s2 = BexStr::from(one_over.as_str());
        assert!(matches!(s2, BexStr::Flat(_)));
    }

    #[test]
    fn test_empty() {
        let s = BexStr::empty();
        assert_eq!(s.len(), 0);
        assert_eq!(s.as_str(), "");
        assert!(s.is_empty());
    }

    #[test]
    fn test_clone_inline_is_value_copy() {
        let s = BexStr::from("hello");
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    #[test]
    fn test_clone_flat_shares_arc() {
        let long = "a".repeat(INLINE_CAPACITY + 1);
        let s = BexStr::from(long.as_str());
        let s2 = s.clone();
        assert!(s.ptr_eq(&s2));
    }

    #[test]
    fn test_concat_short_eagerly_inlines() {
        let a = BexStr::from("hello");
        let b = BexStr::from(" world");
        let c = BexStr::concat(a, b);
        assert!(matches!(c, BexStr::Inline { .. }));
        assert_eq!(c.as_str(), "hello world");
    }

    #[test]
    fn test_concat_long_creates_node() {
        let a = BexStr::from("a".repeat(INLINE_CAPACITY).as_str());
        let b = BexStr::from("b");
        let c = BexStr::concat(a, b);
        assert!(matches!(c, BexStr::Concat(_)));
        assert_eq!(c.len(), INLINE_CAPACITY + 1);
        // Accessing bytes triggers flatten
        assert_eq!(&c.as_str()[..INLINE_CAPACITY], &"a".repeat(INLINE_CAPACITY));
        assert_eq!(&c.as_str()[INLINE_CAPACITY..], "b");
    }

    #[test]
    fn test_concat_repeated_is_linear() {
        // Build a string via repeated concat — should not be O(n^2)
        let mut s = BexStr::from("x");
        for _ in 0..1000 {
            s = BexStr::concat(s, BexStr::from("y"));
        }
        assert_eq!(s.len(), 1001);
        let flat = s.as_str();
        assert_eq!(flat.len(), 1001);
    }

    #[test]
    fn test_deep_concat_drop_no_stack_overflow() {
        // Regression test: dropping a deeply-nested left-leaning Concat tree
        // must not recurse through the tree and overflow the stack.
        //
        // `s = s + "x"` in a loop produces depth N. The default stack on most
        // Rust targets is 8 MB (2 MB on debug). 100k stack frames each ~64 bytes
        // would overflow.
        //
        // This test exercises the iterative Drop impl on ConcatNode.
        let mut s = BexStr::from(&"a".repeat(INLINE_CAPACITY + 1)); // start as Flat
        for _ in 0..100_000 {
            s = BexStr::concat(s, BexStr::from("x"));
        }
        // We never read the bytes (which would trigger iterative flatten), so the
        // tree is at full depth when this scope ends and `s` drops.
        assert_eq!(s.len(), INLINE_CAPACITY + 1 + 100_000);
        // Drop happens here at end of test — must not stack-overflow.
    }

    #[test]
    fn test_substring_inline_to_inline() {
        let s = BexStr::from("hello world");
        let sub = s.substring(0, 5);
        assert!(matches!(sub, BexStr::Inline { .. }));
        assert_eq!(sub.as_str(), "hello");
    }

    #[test]
    fn test_substring_flat_short_copies_to_inline() {
        let long = "a".repeat(INLINE_CAPACITY + 10);
        let s = BexStr::from(long.as_str());
        let sub = s.substring(0, 5);
        assert!(matches!(sub, BexStr::Inline { .. }));
        assert_eq!(sub.as_str(), "aaaaa");
    }

    #[test]
    fn test_substring_flat_long_creates_slice() {
        let long = "a".repeat(INLINE_CAPACITY + 20);
        let s = BexStr::from(long.as_str());
        let sub = s.substring(0, INLINE_CAPACITY + 5);
        assert!(matches!(sub, BexStr::Slice { .. }));
        assert_eq!(sub.len(), INLINE_CAPACITY + 5);
    }

    #[test]
    fn test_slice_of_slice_depth_1() {
        let long = "a".repeat(INLINE_CAPACITY + 100);
        let s = BexStr::from(long.as_str());
        let slice1 = s.substring(10, INLINE_CAPACITY + 80);
        assert!(matches!(slice1, BexStr::Slice { .. }));
        let slice2 = slice1.substring(5, INLINE_CAPACITY + 30);
        // slice-of-slice should still be depth 1 (same parent)
        assert!(matches!(slice2, BexStr::Slice { .. }));
        assert_eq!(slice2.as_str(), &long[15..INLINE_CAPACITY + 40]);
    }

    #[test]
    fn test_equality_same_arc() {
        let long = "a".repeat(INLINE_CAPACITY + 1);
        let s = BexStr::from(long.as_str());
        let s2 = s.clone();
        assert_eq!(s, s2);
        assert!(s.ptr_eq(&s2));
    }

    #[test]
    fn test_equality_different_arcs_same_content() {
        let long = "a".repeat(INLINE_CAPACITY + 1);
        let s1 = BexStr::from(long.as_str());
        let s2 = BexStr::from(long.as_str());
        assert!(!s1.ptr_eq(&s2));
        assert_eq!(s1, s2); // byte comparison fallback
    }

    #[test]
    fn test_equality_inline() {
        let s1 = BexStr::from("hello");
        let s2 = BexStr::from("hello");
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_inequality_different_length() {
        let s1 = BexStr::from("hello");
        let s2 = BexStr::from("hello!");
        assert_ne!(s1, s2); // fast reject via length check
    }

    #[test]
    fn test_ordering() {
        let a = BexStr::from("apple");
        let b = BexStr::from("banana");
        assert!(a < b);
    }

    #[test]
    fn test_display() {
        let s = BexStr::from("display test");
        assert_eq!(format!("{}", s), "display test");
    }

    #[test]
    fn test_deref_to_str() {
        let s = BexStr::from("deref test");
        let borrowed: &str = &*s;
        assert_eq!(borrowed, "deref test");
        // str methods work via deref
        assert!(s.contains("ref"));
        assert!(s.starts_with("der"));
    }

    #[test]
    fn test_to_string() {
        let s = BexStr::from("owned");
        let owned: String = s.to_string();
        assert_eq!(owned, "owned");
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        fn hash_one(s: &BexStr) -> u64 {
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        }
        let s1 = BexStr::from("hash me");
        let s2 = BexStr::from("hash me");
        assert_eq!(hash_one(&s1), hash_one(&s2));
    }
}
