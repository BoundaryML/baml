use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// Maximum bytes stored inline without heap allocation.
/// 56 (target size) - 1 (discriminant) - 1 (len byte) = 54.
const INLINE_CAPACITY: usize = 54;

/// V8-inspired immutable string. O(1) clone. 56 bytes.
pub enum BexStr {
    /// Strings ≤54 bytes: zero heap allocation, clone = memcpy.
    Inline {
        len: u8,
        data: [u8; INLINE_CAPACITY],
    },
    /// Heap-allocated, ref-counted, immutable. Clone = Arc bump.
    Flat(Arc<FlatStr>),
    /// Zero-copy view into a Flat string's buffer. Depth-1 invariant:
    /// parent is ALWAYS Flat, never Slice or Concat.
    Slice {
        parent: Arc<FlatStr>,
        offset: u32,
        len: u32,
        hash: AtomicU64,
    },
    /// Deferred concatenation. Flattened on first byte-level access.
    Concat(Arc<ConcatNode>),
}

/// Heap-allocated immutable string data with cached hash.
pub struct FlatStr {
    /// 0 = not yet computed. Set atomically on first hash() call.
    pub(crate) hash: AtomicU64,
    /// UTF-8 byte data. Tight allocation (no excess capacity).
    pub(crate) data: Box<[u8]>,
}

/// Deferred concatenation node.
pub struct ConcatNode {
    pub(crate) total_len: u32,
    pub(crate) state: Mutex<ConcatState>,
}

pub(crate) enum ConcatState {
    Deferred { left: BexStr, right: BexStr },
    Flattened(Arc<FlatStr>),
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
    pub fn empty() -> BexStr {
        BexStr::Inline {
            len: 0,
            data: [0; INLINE_CAPACITY],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// O(1) for all variants.
    pub fn len(&self) -> usize {
        match self {
            BexStr::Inline { len, .. } => *len as usize,
            BexStr::Flat(f) => f.data.len(),
            BexStr::Slice { len, .. } => *len as usize,
            BexStr::Concat(c) => c.total_len as usize,
        }
    }

    /// O(1) for Inline/Flat/Slice. O(n) first-call flatten for Concat.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BexStr::Inline { len, data } => &data[..*len as usize],
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

    /// Returns `&str`. Delegates to `as_bytes()`.
    pub fn as_str(&self) -> &str {
        // SAFETY: BexStr is always constructed from valid UTF-8.
        #[allow(unsafe_code)]
        unsafe {
            std::str::from_utf8_unchecked(self.as_bytes())
        }
    }

    /// Zero-copy substring. Enforces depth-1 invariant.
    /// Callers must ensure start/end are valid UTF-8 char boundaries.
    pub fn substring(&self, start: usize, end: usize) -> BexStr {
        debug_assert!(start <= end);
        debug_assert!(end <= self.len());
        let slice_len = end - start;

        // Empty result → Inline
        if slice_len == 0 {
            return BexStr::empty();
        }

        // Short result → Inline
        if slice_len <= INLINE_CAPACITY {
            let bytes = &self.as_bytes()[start..end];
            let mut data = [0u8; INLINE_CAPACITY];
            data[..slice_len].copy_from_slice(bytes);
            return BexStr::Inline {
                len: slice_len as u8,
                data,
            };
        }

        // Long result → Slice (depth-1 invariant)
        match self {
            BexStr::Inline { .. } => {
                // Can't reach here: INLINE_CAPACITY < slice_len
                // but slice_len <= self.len() <= INLINE_CAPACITY. Contradiction.
                unreachable!("Inline string cannot produce a Slice longer than INLINE_CAPACITY");
            }
            BexStr::Flat(f) => BexStr::Slice {
                parent: f.clone(),
                offset: start as u32,
                len: slice_len as u32,
                hash: AtomicU64::new(0),
            },
            BexStr::Slice { parent, offset, .. } => {
                // Re-slice: point at SAME parent, adjust offset. Depth stays 1.
                BexStr::Slice {
                    parent: parent.clone(),
                    offset: *offset + start as u32,
                    len: slice_len as u32,
                    hash: AtomicU64::new(0),
                }
            }
            BexStr::Concat(c) => {
                // Flatten first, then slice into the result.
                let flat = c.flatten();
                BexStr::Slice {
                    parent: flat,
                    offset: start as u32,
                    len: slice_len as u32,
                    hash: AtomicU64::new(0),
                }
            }
        }
    }

    /// O(1) deferred concatenation.
    pub fn concat(left: BexStr, right: BexStr) -> BexStr {
        if left.is_empty() {
            return right;
        }
        if right.is_empty() {
            return left;
        }
        let total_len = left.len() + right.len();

        // Short result → Inline
        if total_len <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            let left_bytes = left.as_bytes();
            let right_bytes = right.as_bytes();
            data[..left_bytes.len()].copy_from_slice(left_bytes);
            data[left_bytes.len()..total_len].copy_from_slice(right_bytes);
            return BexStr::Inline {
                len: total_len as u8,
                data,
            };
        }

        BexStr::Concat(Arc::new(ConcatNode {
            total_len: total_len as u32,
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
    fn try_get_hash(&self) -> Option<u64> {
        let h = match self {
            BexStr::Inline { .. } => return None, // No cache for Inline
            BexStr::Flat(f) => f.hash.load(Ordering::Relaxed),
            BexStr::Slice { hash, .. } => hash.load(Ordering::Relaxed),
            BexStr::Concat(c) => {
                // If flattened, check the FlatStr's hash
                let guard = c.state.lock().unwrap();
                match &*guard {
                    ConcatState::Flattened(f) => f.hash.load(Ordering::Relaxed),
                    ConcatState::Deferred { .. } => 0,
                }
            }
        };
        if h != 0 { Some(h) } else { None }
    }

    /// Compute and cache the ahash of this string's bytes.
    /// Used for BexStr-internal equality short-circuits only.
    #[allow(dead_code)]
    fn compute_cached_hash(&self) -> u64 {
        let hash_val = ahash::RandomState::with_seeds(0, 0, 0, 0).hash_one(self.as_bytes());
        // Ensure non-zero (0 means "not computed")
        let hash_val = if hash_val == 0 { 1 } else { hash_val };

        match self {
            BexStr::Inline { .. } => {} // No cache slot
            BexStr::Flat(f) => {
                f.hash
                    .compare_exchange(0, hash_val, Ordering::Relaxed, Ordering::Relaxed)
                    .ok();
            }
            BexStr::Slice { hash, .. } => {
                hash.compare_exchange(0, hash_val, Ordering::Relaxed, Ordering::Relaxed)
                    .ok();
            }
            BexStr::Concat(_) => {} // Flattened FlatStr caches its own
        }
        hash_val
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
            ConcatState::Flattened(Arc::new(FlatStr {
                hash: AtomicU64::new(0),
                data: Box::new([]),
            })),
        );
        let (left, right) = match old {
            ConcatState::Deferred { left, right } => (left, right),
            ConcatState::Flattened(_) => unreachable!(),
        };

        // Iterative tree walk — heap-allocated work-stack
        let mut buf = Vec::with_capacity(self.total_len as usize);
        let mut stack: Vec<BexStr> = Vec::new();
        // Push right first so left is processed first (stack is LIFO)
        stack.push(right);
        stack.push(left);

        while let Some(node) = stack.pop() {
            match node {
                BexStr::Inline { len, data } => {
                    buf.extend_from_slice(&data[..len as usize]);
                }
                BexStr::Flat(f) => {
                    buf.extend_from_slice(&f.data);
                }
                BexStr::Slice {
                    parent,
                    offset,
                    len,
                    ..
                } => {
                    let o = offset as usize;
                    let l = len as usize;
                    buf.extend_from_slice(&parent.data[o..o + l]);
                }
                BexStr::Concat(c) => {
                    let mut inner_guard = c.state.lock().unwrap();
                    match &*inner_guard {
                        ConcatState::Flattened(f) => {
                            buf.extend_from_slice(&f.data);
                        }
                        ConcatState::Deferred { .. } => {
                            let inner_old = std::mem::replace(
                                &mut *inner_guard,
                                ConcatState::Flattened(Arc::new(FlatStr {
                                    hash: AtomicU64::new(0),
                                    data: Box::new([]),
                                })),
                            );
                            drop(inner_guard);
                            if let ConcatState::Deferred { left, right } = inner_old {
                                stack.push(right);
                                stack.push(left);
                            }
                        }
                    }
                }
            }
        }

        let flat = Arc::new(FlatStr {
            hash: AtomicU64::new(0),
            data: buf.into_boxed_slice(),
        });
        *guard = ConcatState::Flattened(flat.clone());
        flat
    }
}

/// Iterative Drop to avoid stack overflow on deep left-leaning trees.
impl Drop for ConcatNode {
    fn drop(&mut self) {
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
                        ConcatState::Flattened(Arc::new(FlatStr {
                            hash: AtomicU64::new(0),
                            data: Box::new([]),
                        })),
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
                len,
                hash,
            } => BexStr::Slice {
                parent: parent.clone(),
                offset: *offset,
                len: *len,
                hash: AtomicU64::new(hash.load(Ordering::Relaxed)),
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
    /// Hash raw bytes — same as `<str as Hash>` — to satisfy Borrow<str> contract.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq for BexStr {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
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
                len: s.len() as u8,
                data,
            }
        } else {
            BexStr::Flat(Arc::new(FlatStr {
                hash: AtomicU64::new(0),
                data: s.into_bytes().into_boxed_slice(),
            }))
        }
    }
}

impl From<&str> for BexStr {
    fn from(s: &str) -> Self {
        if s.len() <= INLINE_CAPACITY {
            let mut data = [0u8; INLINE_CAPACITY];
            data[..s.len()].copy_from_slice(s.as_bytes());
            BexStr::Inline {
                len: s.len() as u8,
                data,
            }
        } else {
            BexStr::Flat(Arc::new(FlatStr {
                hash: AtomicU64::new(0),
                data: s.as_bytes().into(),
            }))
        }
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
