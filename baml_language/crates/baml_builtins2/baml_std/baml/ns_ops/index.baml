//! The indexing operator (`self[idx]`), declared as `baml.ops.Index` -
//! Rust's `ops::Index` shape: `Idx` as the interface input, the element as
//! the `Output` associated type.
//!
//! `a[i]` and `baml.ops.Index.index(a, i)` always agree, whether `a`'s type
//! is known statically or only reached through a bound
//! (`T extends Index<int>`).
//!
//! `Idx` and `Output` are deliberately UNBOUNDED: a map's key may be a
//! string-literal union and a container's element any type, neither of
//! which is `baml.Concrete`. Indexing a `uint8array` yields the byte as an
//! `int` in `[0, 255]`; BAML has no byte scalar.
//!
//! Panics are orthogonal to `throws` (see `math.baml`): indexing raises the
//! catchable `baml.panics.IndexOutOfBounds` for an out-of-range list or
//! `uint8array` index, and `baml.panics.MapKeyNotFound` for a missing map
//! key.

/// The indexing operator, `self[idx]`.
///
/// `Idx` is the index type and `Output` the element type, the shape of Rust's
/// [`ops::Index`](https://doc.rust-lang.org/std/ops/trait.Index.html). `Idx` is
/// an interface parameter, so one type could be indexable at several index
/// types; the standard library gives each container just one.
///
/// This is read-only. There is no `IndexMut` counterpart: assigning through
/// `self[idx] = v` is not routed through an interface.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Index<Idx> requires root.Concrete {
    /// The element type. Unbounded, deliberately: an element may be any type at
    /// all, including ones that are not `baml.Concrete`.
    type Output;
    /// Reads the element of `self` at `idx`.
    ///
    /// The standard library's containers index as Python does: a list or
    /// `uint8array` index is a zero-based `int` and a negative one counts back
    /// from the end, `-1` being the last element; a map index is its `string`
    /// key. They are all-or-nothing, so use a list's `at` or a map's `get` —
    /// both of which return an optional — when a missing element should be a
    /// value rather than a panic.
    function index(self, idx: Idx) -> Self.Output throws never
}

implement<T> Index<int> for T[] {
    type Output = T;
    function index(self, idx: int) -> T throws never {
        self[idx]
    }
}

// Map keys are `string` by the language contract (E0067 enforces it in
// impl headers); a literal-key map type cannot `.set` either, so it has
// no index impl.
implement<V> Index<string> for map<string, V> {
    type Output = V;
    function index(self, idx: string) -> V throws never {
        self[idx]
    }
}

implement Index<int> for uint8array {
    type Output = int;
    function index(self, idx: int) -> int throws never {
        self[idx]
    }
}
