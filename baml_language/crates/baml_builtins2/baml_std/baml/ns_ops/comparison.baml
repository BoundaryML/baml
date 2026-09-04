//! Equality and ordering (`== != < <= > >=`), declared as `baml.ops.Equals`
//! and `baml.ops.Compare`.
//!
//! `a < b` and `baml.ops.Compare.lt(a, b)` always agree, whether the operands'
//! types are known statically or only reached through a bound
//! (`T extends Compare`). `==` is broader than `Equals`; see
//! `baml.ops.equals_equals`.
//!
//! Both relations are total for every implementor: `Equals` is an equivalence
//! relation and `Compare` a total order. What the primitive implementations do:
//!   - `int` and `bigint` compare by signed numeric value. `bigint` equality is
//!     by value, not by allocation: two separately built `bigint`s holding the
//!     same integer are equal.
//!   - `float` departs from IEEE-754 to stay total: `NaN` is a single value
//!     that equals itself and orders above every number
//!     (`-inf < …reals… < +inf < NaN`), and `-0.0 == 0.0` is `true`.
//!   - `bool` orders `false` before `true`.
//!   - `string` equality is exact, code point for code point, with no Unicode
//!     normalization: a precomposed `é` and an `e` plus a combining accent are
//!     unequal. The order is lexicographic by code point, not a locale-aware
//!     collation — uppercase before lowercase, accents after the whole ASCII
//!     range — so do not use it to order text for display.
//!   - `uint8array` compares by content and only at equal lengths; it has no
//!     ordering.

/// Equality that is an *equivalence relation*: reflexive (`x.eq(x)` is always
/// true), symmetric, and transitive. There is no partial counterpart, so an
/// implementor that cannot make equality reflexive should not implement this.
///
/// Reflexivity is what lets `==` settle two references to the same value
/// without inspecting it, so a non-reflexive `eq` is not merely surprising —
/// it will not be consulted for `x == x`.
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
    /// against itself; there is no heterogeneous equality. Implementations must
    /// be reflexive, symmetric and transitive — Rust's
    /// [`Eq`](https://doc.rust-lang.org/std/cmp/trait.Eq.html), not
    /// `PartialEq`: the `float` impl keeps the law by making `NaN` equal to
    /// itself rather than agreeing with IEEE.
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

/// A *total* ordering: every pair of values is ordered, and the ordering is
/// antisymmetric and transitive. There is no partial counterpart.
///
/// Backs the `<`, `<=`, `>` and `>=` operators, and is what a `T extends
/// Compare` bound asks for. Closest to Rust's
/// [`Ord`](https://doc.rust-lang.org/std/cmp/trait.Ord.html): `requires Equals`
/// is the counterpart of `Ord: Eq`, and — unlike `PartialOrd` — the order is
/// total for every implementor, `float` included.
///
/// Must be consistent with `baml.ops.Equals`: `self.cmp(other)` is
/// `Ordering.Equal` exactly when `self.eq(other)`.
///
/// `cmp` is the only required method — `lt`/`le`/`gt`/`ge`/`min`/`max`/`clamp`
/// all derive from it, and an implementor overrides them only to be faster. An
/// override is responsible for agreeing with `cmp`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Compare requires Equals {
    /// Three-way comparison of `self` against `other`.
    ///
    /// The one method with no default; the rest of the interface is defined in
    /// terms of it.
    function cmp(self, other: Self) -> Ordering throws never

    /// Whether `self` orders strictly before `other`. Defaults to
    /// `self.cmp(other) == Ordering.Less`.
    function lt(self, other: Self) -> bool throws never {
        self.cmp(other) == Ordering.Less
    }
    /// Whether `self` orders before `other` or equals it. Defaults to
    /// `self.cmp(other) != Ordering.Greater`.
    function le(self, other: Self) -> bool throws never {
        self.cmp(other) != Ordering.Greater
    }
    /// Whether `self` orders strictly after `other`. Defaults to
    /// `self.cmp(other) == Ordering.Greater`.
    function gt(self, other: Self) -> bool throws never {
        self.cmp(other) == Ordering.Greater
    }
    /// Whether `self` orders after `other` or equals it. Defaults to
    /// `self.cmp(other) != Ordering.Less`.
    function ge(self, other: Self) -> bool throws never {
        self.cmp(other) != Ordering.Less
    }

    /// Ties return `self`, so `max` is stable when two values compare `Equal`
    /// without being interchangeable.
    function max(self, other: Self) -> Self throws never {
        match (self.cmp(other)) {
            Ordering.Less => other,
            Ordering.Equal => self,
            Ordering.Greater => self,
        }
    }

    /// Ties return `self`, matching `max`.
    function min(self, other: Self) -> Self throws never {
        match (self.cmp(other)) {
            Ordering.Less => self,
            Ordering.Equal => self,
            Ordering.Greater => other,
        }
    }

    /// Clamps `self` into the range `[min, max]`.
    ///
    /// Callers should pass `min <= max`; if `min > max` the result is always
    /// `min`, because the lower clamp runs after the upper one. Written as the
    /// `min`/`max` chain rather than a three-way match so it agrees with
    /// `int.clamp` / `float.clamp` on that degenerate input too.
    function clamp(self, min: Self, max: Self) -> Self throws never {
        self.min(max).max(min)
    }
}

/// Represents the result of a comparison between two values.
enum Ordering {
    Less,
    Equal,
    Greater,
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
/// - It is reflexive. `Equals` is an equivalence relation and `float` equality
///   is BAML's total one (`NaN` equals itself), so a value is always equal to
///   itself — including a class instance or array that contains a `NaN` — and
///   two references to the same value are equal without being walked.
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
// Note that the compiler usually special-cases these for performance, emitting
// bytecode that directly compares the primitives. These implementations exist
// so that primitives satisfy interface bounds (`T extends Compare`) and can be
// reached via dynamic dispatch when a comparison cannot be specialized.
//
// They are also where a primitive's `min` / `max` / `clamp` come from: `class
// Int`, `class Bigint` and `class Float` declare none of their own, so each
// type has exactly one of each, and `(3).max(x)` cannot answer differently from
// the `max` a `T extends Compare` bound reaches.
//
// `eq` is native, but the primitive `cmp` bodies are written here in BAML on
// `<` and `>`. Those two operators lower to the very comparison bytecode these
// implementations have to agree with, so the agreement is structural — there is
// no second copy of the ordering to keep in sync. (`self` and `other` are both
// the primitive, so `<` and `>` take the specialized opcode; neither re-enters
// `cmp`.) A native `cmp` would also have to resolve the `Ordering` enum by name
// on every call, which is more expensive than the extra comparison.

implement Equals for int {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for int {
    function cmp(self, other: Self) -> Ordering throws never {
        if (self < other) {
            Ordering.Less
        } else if (self > other) {
            Ordering.Greater
        } else {
            Ordering.Equal
        }
    }
}

implement Equals for bigint {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bigint {
    function cmp(self, other: Self) -> Ordering throws never {
        if (self < other) {
            Ordering.Less
        } else if (self > other) {
            Ordering.Greater
        } else {
            Ordering.Equal
        }
    }
}

/// Reflexive, unlike IEEE 754: every NaN equals every other NaN (and so
/// itself), and `-0.0` equals `0.0`. Consistent with `Compare for float` below.
implement Equals for float {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

/// A *total* order over every float, unlike IEEE 754, which leaves NaN
/// unordered:
///
/// ```text
/// -inf  <  …negative reals…  <  0.0  <  …positive reals…  <  +inf  <  NaN
/// ```
///
/// NaN is a single value here, greater than every number: its sign and payload
/// are not observable, because the NaN a hardware invalid operation produces
/// differs across CPU architectures and comparison must not. `-0.0` and `0.0`
/// compare `Ordering.Equal`, as under IEEE 754. Every other pair orders exactly
/// as IEEE 754 orders it.
///
/// This is the order `<`, `<=`, `>`, `>=` and `float[].sort()` all use.
implement Compare for float {
    function cmp(self, other: Self) -> Ordering throws never {
        if (self < other) {
            Ordering.Less
        } else if (self > other) {
            Ordering.Greater
        } else {
            // Equal, both zeros, or both NaN — the operators agree with this
            // order, so anything neither less nor greater is `Equal`.
            Ordering.Equal
        }
    }
}

implement Equals for bool {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bool {
    function cmp(self, other: Self) -> Ordering throws never {
        // todo: single match once we have tuples
        match (self) {
            true => match (other) {
                true => Ordering.Equal,
                false => Ordering.Greater,
            },
            false => match (other) {
                true => Ordering.Less,
                false => Ordering.Equal,
            },
        }
    }
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
    function cmp(self, other: Self) -> Ordering throws never {
        if (self < other) {
            Ordering.Less
        } else if (self > other) {
            Ordering.Greater
        } else {
            Ordering.Equal
        }
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
