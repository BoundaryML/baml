//! Equality and ordering (`== != < <= > >=`), declared as `baml.ops.Equals`
//! and `baml.ops.Compare`.
//!
//! `a < b` and `baml.ops.Compare.lt(a, b)` always agree, whether the operands'
//! types are known statically or only reached through a bound
//! (`T extends Compare`). `==` is broader than `Equals`; see
//! `baml.ops.equals_equals`.
//!
//! What the primitive implementations do:
//!   - `int` and `bigint` compare by signed numeric value, a total order.
//!     `bigint` equality is by value, not by allocation: two separately built
//!     `bigint`s holding the same integer are equal.
//!   - `float` is IEEE-754, so equality is not reflexive — `NaN` equals
//!     nothing, itself included — and the order is only partial: `lt`, `le`,
//!     `gt` and `ge` are all `false` when either operand is `NaN`.
//!     `-0.0 == 0.0` is `true`.
//!   - `bool` orders `false` before `true`.
//!   - `string` equality is exact, code point for code point, with no Unicode
//!     normalization: a precomposed `é` and an `e` plus a combining accent are
//!     unequal. The order is lexicographic by code point, not a locale-aware
//!     collation — uppercase before lowercase, accents after the whole ASCII
//!     range — so do not use it to order text for display.
//!   - `uint8array` compares by content and only at equal lengths; it has no
//!     ordering.

/// Equality for a type that defines it.
///
/// This is the interface a `T extends baml.ops.Equals` bound asks for, and the
/// hook a type implements to be compared through dynamic dispatch — Python's
/// `__eq__`, near enough. It is narrower than the `==` operator:
/// `baml.ops.equals_equals` below compares any pair of compatible types and
/// routes here only when the operand's runtime type implements this interface.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Equals {
    /// Whether `self` and `other` are equal.
    ///
    /// `other` is typed `Self`, so an implementation only ever compares a type
    /// against itself; there is no heterogeneous equality. Implementations are
    /// expected to be symmetric and transitive, as Rust's
    /// [`PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html)
    /// asks — and, like `PartialEq`, *not* necessarily reflexive: the `float`
    /// impl reports `NaN != NaN` because agreeing with IEEE and with the `==`
    /// operator matters more than the law. BAML has no separate total-equality
    /// interface for the types that do satisfy reflexivity.
    function eq(self, other: Self) -> bool throws never
    /// Whether `self` and `other` differ. Defaults to `!self.eq(other)`.
    ///
    /// Override this only to compute the answer more directly; an override that
    /// disagrees with `!eq` leaves the pair inconsistent. `Equals for null`
    /// overrides it with a constant `false`, which is consistent because `null`
    /// has exactly one value.
    function neq(self, other: Self) -> bool throws never {
        !self.eq(other)
    }
}

/// Ordering for a type that also defines equality.
///
/// Backs the `<`, `<=`, `>` and `>=` operators, and is what a `T extends
/// Compare` bound asks for. Closest to Rust's
/// [`PartialOrd`](https://doc.rust-lang.org/std/cmp/trait.PartialOrd.html):
/// `requires Equals` is the counterpart of `PartialOrd: PartialEq`, and — as
/// with `float` — the order need not be total.
///
/// Only `lt` has no default: `le`, `gt`, and `ge` derive from it and `eq`, so
/// an implementation that overrides them is responsible for keeping all four
/// consistent with one another. `Compare for float` does exactly that, and
/// must: the boolean-derived defaults would report `NaN > NaN` as `true`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Compare requires Equals {
    /// Whether `self` orders strictly before `other`.
    ///
    /// The one method with no default; the other three are defined in terms of
    /// it and `Equals.eq`.
    function lt(self, other: Self) -> bool throws never
    /// Whether `self` orders before `other` or equals it. Defaults to
    /// `self.lt(other) || self.eq(other)`.
    function le(self, other: Self) -> bool throws never {
        self.lt(other) || self.eq(other)
    }
    /// Whether `self` orders strictly after `other`. Defaults to
    /// `!self.le(other)`.
    ///
    /// The default assumes the order is total. A partial order — one with
    /// incomparable values, as `float` has with `NaN` — must override this.
    function gt(self, other: Self) -> bool throws never {
        !self.le(other)
    }
    /// Whether `self` orders after `other` or equals it. Defaults to
    /// `!self.lt(other)`.
    ///
    /// Like `gt`, the default assumes a total order and must be overridden by
    /// a partial one.
    function ge(self, other: Self) -> bool throws never {
        !self.lt(other)
    }
}

/// The broad `==` / `!=` operator driver.
///
/// Unlike `Equals`, which is only available on types that implement it, `==`
/// works on any pair of compatible types, and gives the same answer whether
/// those types are known statically or erased to `unknown`, a union, or an
/// interface. This function is what `==` means in the erased case, and what
/// structural comparison recurses through.
///
/// It compares by the operands' concrete runtime types: a type that implements
/// `Equals` uses its `eq`; a class without `Equals` compares structurally,
/// field by field; enums compare by identity; and operands whose concrete
/// runtime types differ are never equal. Containers recurse — arrays must have
/// the same length and compare element-wise by position, maps must have the
/// same key set and compare values key-wise, so map order never matters. Values
/// that hold a reference rather than data — functions, closures, futures,
/// collectors — compare by identity. A `type` value compares by type
/// equivalence, not by spelling.
///
/// Two properties are worth knowing before relying on this:
///
/// - It is not reflexive. `float` equality is IEEE, so any value that reaches a
///   `NaN` is unequal to itself — including a class instance or array that
///   merely contains one.
/// - It terminates on cyclic data. A pair of objects already being compared
///   higher up the walk is taken as equal, so a self-referential value does not
///   spin; every other reachable element is still compared.
//baml:mut_vm
//baml:may_yield
function equals_equals(a: unknown, b: unknown) -> bool throws never {
    $rust_function
}

// Implement for primitives, containers, and `null`.
//
// The compiler usually special-cases these for performance, emitting bytecode
// that directly compares the primitives. These implementations exist so that
// primitives satisfy interface bounds (`T extends Compare`) and can be reached
// via dynamic dispatch when a comparison cannot be specialized.

implement Equals for int {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for int {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for bigint {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bigint {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for float {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for float {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
    function gt(self, other: Self) -> bool throws never {
        $rust_function
    }
    function ge(self, other: Self) -> bool throws never {
        $rust_function
    }
    function le(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for bool {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bool {
    function lt(self, other: Self) -> bool throws never {
        !self && other
    }
    function gt(self, other: Self) -> bool throws never {
        self && !other
    }
    // `ge`/`le` inherit the `Compare` defaults (`ge = !lt`, `le = lt || eq`), which are
    // reflexive: `ge(x, x) = !lt(x, x) = true`, `le(x, x) = lt(x, x) || eq(x, x) = true`.
}

implement Equals for null {
    function eq(self, other: Self) -> bool throws never {
        true
    }
    function neq(self, other: Self) -> bool throws never {
        false
    }
}

implement Equals for string {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for string {
    /// In lexicographic order.
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function gt(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function ge(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function le(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for uint8array {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

// Containers implement `Equals` for *all* element types (unbounded), comparing
// element-wise with the broad `==` (`baml.ops.equals_equals`) rather than the
// element `Equals.eq`. This is sound because `==` is total — it always produces a
// result for operands of the same type — and `self`/`other` are the same container
// type here. It also lets `T[]`/`map<string, V>` satisfy `Equals` bounds for any `T`/`V`.
implement<T> Equals for T[] {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self, other)
    }
}

implement<V> Equals for map<string, V> {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self, other)
    }
}
