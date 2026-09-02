//! Native bitwise operators (`& | ^ << >>`) for the integer primitives,
//! declared as `baml.ops.{BitAnd,BitOr,BitXor,ShiftLeft,ShiftRight}`.
//!
//! `a & b` and `baml.ops.BitAnd.bit_and(a, b)` always agree, whether the
//! operands' types are known statically or only reached through a bound
//! (`T extends BitAnd<...>`). The same holds for every operator here.
//!
//! Result-type rule: `int` with `int` stays `int`; any `int`/`bigint` mix
//! widens to `bigint` - including a shift of an `int` by a `bigint` count,
//! where the VALUE widens before shifting.
//!
//! Panics are orthogonal to the `throws` contract (see `math.baml`), so every
//! signature stays `throws never`:
//!   - `int << int` truncates: bits shifted past the 63-bit width are
//!     discarded, so `1 << 62` is `int.min_value()` and any count at or past 63
//!     gives `0`. Only a negative count raises `baml.panics.NegativeBitShift`.
//!   - `int << bigint` does not truncate: the `bigint` count widens the shifted
//!     value first, so no bits are lost and `1 << 62n` is the exact `bigint`,
//!     not `int.min_value()`.
//!   - `int >> int` raises `baml.panics.NegativeBitShift` on a negative count; a
//!     count past every bit saturates to the sign (arithmetic shift).
//!   - `bigint` shifts raise `baml.panics.NegativeBitShift` on a negative count;
//!     `<<` raises `baml.panics.AllocFailure` when the result would exceed the
//!     workspace bigint cap; `>>` past every bit saturates to `0n`/`-1n`.
//!   - `& | ^` never panic (results never grow past their operands).

/// The bitwise AND operator, `self & rhs`.
///
/// `Rhs` names the right-hand operand type and `Output` the result, the shape
/// of Rust's
/// [`ops::BitAnd`](https://doc.rust-lang.org/std/ops/trait.BitAnd.html). Only
/// the integer primitives implement it in the standard library; a user type
/// gains `&` by implementing this interface.
///
/// The integer impls treat operands as two's-complement bit patterns of
/// unbounded width, so a negative operand behaves as if sign-extended forever:
/// `-1 & 5` is `5`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface BitAnd<Rhs extends root.Concrete> requires root.Concrete {
    /// The result's type: `int` for two `int` operands in the integer impls,
    /// otherwise `bigint`.
    type Output extends root.Concrete;
    /// The bitwise AND of `self` and `rhs`.
    function bit_and(self, rhs: Rhs) -> Self.Output throws never
}

/// The bitwise OR operator, `self | rhs`.
///
/// The same shape as Rust's
/// [`ops::BitOr`](https://doc.rust-lang.org/std/ops/trait.BitOr.html), with the
/// same two's-complement treatment of negative operands as `baml.ops.BitAnd`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface BitOr<Rhs extends root.Concrete> requires root.Concrete {
    /// The result's type: `int` for two `int` operands in the integer impls,
    /// otherwise `bigint`.
    type Output extends root.Concrete;
    /// The bitwise OR of `self` and `rhs`.
    function bit_or(self, rhs: Rhs) -> Self.Output throws never
}

/// The bitwise XOR operator, `self ^ rhs`.
///
/// The same shape as Rust's
/// [`ops::BitXor`](https://doc.rust-lang.org/std/ops/trait.BitXor.html), with
/// the same two's-complement treatment of negative operands as
/// `baml.ops.BitAnd`. `^` is exclusive-or, not exponentiation.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface BitXor<Rhs extends root.Concrete> requires root.Concrete {
    /// The result's type: `int` for two `int` operands in the integer impls,
    /// otherwise `bigint`.
    type Output extends root.Concrete;
    /// The bitwise exclusive-OR of `self` and `rhs`.
    function bit_xor(self, rhs: Rhs) -> Self.Output throws never
}

/// The left-shift operator, `self << rhs`.
///
/// `Rhs` is the *count*, not a second operand of the same kind, which is the
/// shape of Rust's
/// [`ops::Shl`](https://doc.rust-lang.org/std/ops/trait.Shl.html). BAML differs
/// from Rust in one way worth knowing: the count's type participates in the
/// result type, so a `bigint` count widens the shifted value to `bigint` before
/// shifting rather than merely being converted to a machine width.
///
/// In the integer impls, shifting a negative value left is well defined — the
/// operand is a two's-complement bit pattern — while a negative *count* is
/// always an error, never a right shift.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface ShiftLeft<Rhs extends root.Concrete> requires root.Concrete {
    /// The result's type: `int` for two `int` operands in the integer impls,
    /// otherwise `bigint`, a `bigint` count widening the shifted value before
    /// the shift.
    type Output extends root.Concrete;
    /// Shifts `self` left by `rhs` bit positions.
    function shl(self, rhs: Rhs) -> Self.Output throws never
}

/// The right-shift operator, `self >> rhs`.
///
/// The mirror of `baml.ops.ShiftLeft`, with the same `Rhs`-is-the-count shape
/// as Rust's [`ops::Shr`](https://doc.rust-lang.org/std/ops/trait.Shr.html) and
/// the same rule that a `bigint` count widens the shifted value.
///
/// There is no separate logical-shift operator — JavaScript's `>>>` has no
/// counterpart here. The integer impls shift *arithmetically*: the sign bit is
/// propagated rather than zeros fed in, so `-16 >> 2` is `-4`, and a count at
/// or past the operand's width saturates to the sign rather than being an
/// error.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface ShiftRight<Rhs extends root.Concrete> requires root.Concrete {
    /// The result's type: `int` for two `int` operands in the integer impls,
    /// otherwise `bigint`, a `bigint` count widening the shifted value before
    /// the shift.
    type Output extends root.Concrete;
    /// Shifts `self` right by `rhs` bit positions.
    function shr(self, rhs: Rhs) -> Self.Output throws never
}

// ==== int ====
implement BitAnd<int> for int {
    type Output = int;
    function bit_and(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement BitOr<int> for int {
    type Output = int;
    function bit_or(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement BitXor<int> for int {
    type Output = int;
    function bit_xor(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement ShiftLeft<int> for int {
    type Output = int;
    //baml:fallible
    function shl(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement ShiftRight<int> for int {
    type Output = int;
    //baml:fallible
    function shr(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement BitAnd<bigint> for int {
    type Output = bigint;
    function bit_and(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement BitOr<bigint> for int {
    type Output = bigint;
    function bit_or(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement BitXor<bigint> for int {
    type Output = bigint;
    function bit_xor(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement ShiftLeft<bigint> for int {
    type Output = bigint;
    //baml:fallible
    function shl(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement ShiftRight<bigint> for int {
    type Output = bigint;
    //baml:fallible
    function shr(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

// ==== bigint ====
implement BitAnd<int> for bigint {
    type Output = bigint;
    function bit_and(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement BitOr<int> for bigint {
    type Output = bigint;
    function bit_or(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement BitXor<int> for bigint {
    type Output = bigint;
    function bit_xor(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement ShiftLeft<int> for bigint {
    type Output = bigint;
    //baml:fallible
    function shl(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement ShiftRight<int> for bigint {
    type Output = bigint;
    //baml:fallible
    function shr(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement BitAnd<bigint> for bigint {
    type Output = bigint;
    function bit_and(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement BitOr<bigint> for bigint {
    type Output = bigint;
    function bit_or(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement BitXor<bigint> for bigint {
    type Output = bigint;
    function bit_xor(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement ShiftLeft<bigint> for bigint {
    type Output = bigint;
    //baml:fallible
    function shl(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement ShiftRight<bigint> for bigint {
    type Output = bigint;
    //baml:fallible
    function shr(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
