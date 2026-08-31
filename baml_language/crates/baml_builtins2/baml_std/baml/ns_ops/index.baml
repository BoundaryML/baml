//! The indexing operator (`self[idx]`), declared as `baml.ops.Index` -
//! Rust's `ops::Index` shape: `Idx` as the interface input, the element as
//! the `Output` associated type.
//!
//! The compiler special-cases indexing for statically-typed operands
//! (emitting the direct index bytecode); these implementations back the
//! same operation when reached through a generic bound
//! (`T extends Index<int>`). Their bodies use the operator on the
//! statically-typed `self`, which lowers to the direct instruction - the
//! same delegation discipline as `Equals for T[]` (no native glue needed).
//!
//! `Idx` and `Output` are deliberately UNBOUNDED: a map's key may be a
//! string-literal union and a container's element any type, neither of
//! which is `baml.Concrete`. Panics are orthogonal to `throws` (see
//! `math.baml`): an out-of-range list or uint8array index raises the
//! catchable `baml.panics.IndexOutOfBounds`, exactly like the bytecode.

/// `self[idx]`
interface Index<Idx> requires root.Concrete {
    type Output;
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
