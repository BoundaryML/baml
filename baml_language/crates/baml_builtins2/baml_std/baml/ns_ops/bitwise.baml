//! Native bitwise operators (`& | ^ << >>`) for the integer primitives,
//! declared as `baml.ops.{BitAnd,BitOr,BitXor,ShiftLeft,ShiftRight}` and
//! implemented in `crates/bex_vm/src/package_baml/ops_bitwise.rs`.
//!
//! Like the math ops in `math.baml`, the compiler special-cases these for
//! statically-typed operands (emitting the `BitAnd` / `Shl` / ... `BinOp`
//! bytecode); the implementations here back the same operations when reached
//! through a generic bound (`T extends BitAnd<...>`) and define the canonical
//! semantics the bytecode matches (B-1075).
//!
//! Result-type rule (mirrors the bytecode's operand widening): `int` with `int`
//! stays `int`; any `int`/`bigint` mix widens to `bigint` - including a shift of
//! an `int` by a `bigint` count, where the VALUE widens before shifting.
//!
//! Panics are orthogonal to the `throws` contract (see `math.baml`), so every
//! signature stays `throws never`; the fallible cases match the opcodes exactly:
//!   - `int << int` raises catchable `baml.panics.IntegerOverflow` on a result
//!     outside the i63 range, and `baml.panics.NegativeBitShift` on a negative
//!     count (like the validated `Shl` opcode).
//!   - `int >> int` raises `baml.panics.NegativeBitShift` on a negative count; a
//!     count past every bit saturates to the sign (arithmetic shift).
//!   - `bigint` shifts raise `baml.panics.NegativeBitShift` on a negative count;
//!     `<<` raises `baml.panics.AllocFailure` when the result would exceed the
//!     workspace bigint cap; `>>` past every bit saturates to `0n`/`-1n`.
//!   - `& | ^` never panic (results never grow past their operands).

/// `self & rhs`
interface BitAnd<Rhs extends root.Concrete> requires root.Concrete {
    /// The type `self & rhs` produces: `int` when both operands are `int`, otherwise `bigint`.
    type Output extends root.Concrete;
    /// The `&` operator.
    function bit_and(self, rhs: Rhs) -> Self.Output throws never
}

/// `self | rhs`
interface BitOr<Rhs extends root.Concrete> requires root.Concrete {
    /// The type `self | rhs` produces: `int` when both operands are `int`, otherwise `bigint`.
    type Output extends root.Concrete;
    /// The `|` operator.
    function bit_or(self, rhs: Rhs) -> Self.Output throws never
}

/// `self ^ rhs`
interface BitXor<Rhs extends root.Concrete> requires root.Concrete {
    /// The type `self ^ rhs` produces: `int` when both operands are `int`, otherwise `bigint`.
    type Output extends root.Concrete;
    /// The `^` operator.
    function bit_xor(self, rhs: Rhs) -> Self.Output throws never
}

/// `self << rhs`
interface ShiftLeft<Rhs extends root.Concrete> requires root.Concrete {
    /// The type `self << rhs` produces: `int` when both operands are `int`, otherwise `bigint`.
    /// A `bigint` count widens the shifted value before shifting.
    type Output extends root.Concrete;
    /// The `<<` operator.
    function shl(self, rhs: Rhs) -> Self.Output throws never
}

/// `self >> rhs`
interface ShiftRight<Rhs extends root.Concrete> requires root.Concrete {
    /// The type `self >> rhs` produces: `int` when both operands are `int`, otherwise `bigint`.
    /// A `bigint` count widens the shifted value before shifting.
    type Output extends root.Concrete;
    /// The `>>` operator.
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
