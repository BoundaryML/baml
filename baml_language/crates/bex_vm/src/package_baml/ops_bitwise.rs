//! Native implementations of the `baml.ops` bitwise interfaces
//! (`BitAnd` / `BitOr` / `BitXor` / `ShiftLeft` / `ShiftRight`) for the integer
//! primitives, declared in `baml_std/baml/ns_ops/bitwise.baml` (B-1075).
//!
//! These mirror BAML's `&` / `|` / `^` / `<<` / `>>` operators, which the
//! compiler special-cases to the `BinOp::BitAnd` / `Shl` / ... bytecode when
//! the operand types are statically known. They back the same operations when
//! one is reached through a generic bound (`T extends BitAnd<...>`) and define
//! the canonical result the specialized bytecode matches:
//! - `int` `& | ^` are plain i63 bitwise ops (a result of two in-range
//!   operands is always in range) - never panic, like the tagged fast path.
//! - `int << int` shares `Int63`'s implementation with the `Shl` opcode, so it
//!   truncates rather than overflowing: bits past the i63 width are discarded
//!   (`1 << 62` is `int.min_value()`, `1 << 63` is `0`) and only a negative
//!   count raises the catchable `baml.panics.NegativeBitShift`.
//! - `int >> int` is arithmetic; negative counts throw `NegativeBitShift`, a
//!   count past every bit saturates to the sign (`min(63)`).
//! - `bigint` `& | ^` delegate to `num-bigint` (results never grow past their
//!   operands, so no cap check - same as `bigint_binop`).
//! - `bigint <<` rejects negative counts (`NegativeBitShift`), counts that do
//!   not fit `usize` (`AllocFailure` - the result is unrepresentable), and
//!   results past the workspace cap (`AllocFailure`), exactly like the heap
//!   `Shl` path.
//! - `bigint >>` rejects negative counts; a non-negative count too large for
//!   `usize` is "shift past every bit": `0n` for non-negatives, `-1n` for
//!   negatives (`num-bigint`'s `Shr` is arithmetic).
//! - mixed `int`/`bigint` widens the `int` operand to a `BigInt` first, the
//!   same widening the bytecode's `bigint_operand` applies.

use std::sync::Arc;

use baml_type::{Int63, IntShiftError};
use bex_vm_types::errors::VmPanic;
use num_bigint::BigInt;

use super::{
    BamlClassOpsBitAnd_bigint__for_bigint, BamlClassOpsBitAnd_bigint__for_int,
    BamlClassOpsBitAnd_int__for_bigint, BamlClassOpsBitAnd_int__for_int,
    BamlClassOpsBitOr_bigint__for_bigint, BamlClassOpsBitOr_bigint__for_int,
    BamlClassOpsBitOr_int__for_bigint, BamlClassOpsBitOr_int__for_int,
    BamlClassOpsBitXor_bigint__for_bigint, BamlClassOpsBitXor_bigint__for_int,
    BamlClassOpsBitXor_int__for_bigint, BamlClassOpsBitXor_int__for_int,
    BamlClassOpsShiftLeft_bigint__for_bigint, BamlClassOpsShiftLeft_bigint__for_int,
    BamlClassOpsShiftLeft_int__for_bigint, BamlClassOpsShiftLeft_int__for_int,
    BamlClassOpsShiftRight_bigint__for_bigint, BamlClassOpsShiftRight_bigint__for_int,
    BamlClassOpsShiftRight_int__for_bigint, BamlClassOpsShiftRight_int__for_int, PackageBamlImpl,
    bigint::{MAX_BIGINT_BITS, alloc_failure_panic},
};
use crate::errors::VmRustFnError;

// ── shared helpers ──────────────────────────────────────────────────────────

/// The catchable `baml.panics.NegativeBitShift`, message matching the VM's
/// `negative_bit_shift` so operator and method agree.
fn negative_bit_shift(count: impl std::fmt::Display) -> VmRustFnError {
    VmPanic::NegativeBitShift {
        message: format!("bit shift count is negative: {count}"),
    }
    .into()
}

/// `int << r`, sharing `Int63`'s semantics with the `Shl` opcode.
///
/// Truncating, never overflowing: bits shifted past the i63 width are
/// discarded, so `1 << 62` is `int.min_value()` and any count at or above 63
/// yields `0`. Delegating to `Int63` is what keeps the operator and this
/// interface impl from drifting apart — they were previously separate
/// implementations that disagreed on every truncating input.
fn int_shl(l: i64, r: i64) -> Result<i64, VmRustFnError> {
    let Some(value) = Int63::new(l) else {
        return Err(VmPanic::IntegerOverflow {
            message: format!("{l} is outside the int range"),
        }
        .into());
    };
    match value.shift_left(r) {
        Ok(shifted) => Ok(shifted.get()),
        Err(IntShiftError::NegativeCount(count)) => Err(negative_bit_shift(count)),
    }
}

/// `int >> r` (arithmetic), sharing `Int63`'s semantics with the `Shr` opcode.
///
/// A count at or above the i63 width saturates to the sign: `0` for a
/// non-negative receiver, `-1` for a negative one.
fn int_shr(l: i64, r: i64) -> Result<i64, VmRustFnError> {
    let Some(value) = Int63::new(l) else {
        return Err(VmPanic::IntegerOverflow {
            message: format!("{l} is outside the int range"),
        }
        .into());
    };
    match value.shift_right(r) {
        Ok(shifted) => Ok(shifted.get()),
        Err(IntShiftError::NegativeCount(count)) => Err(negative_bit_shift(count)),
    }
}

/// Resolve a `bigint <<` count: negative throws, too-large-for-`usize` is an
/// unrepresentable result (`AllocFailure`) - the heap `Shl` path's rules.
fn bigint_shl_count(rhs: &BigInt) -> Result<usize, VmRustFnError> {
    if rhs.sign() == num_bigint::Sign::Minus {
        return Err(negative_bit_shift(rhs));
    }
    usize::try_from(rhs).map_err(|_| {
        alloc_failure_panic(format!(
            "bigint shl: shift count ({rhs}) does not fit in usize"
        ))
    })
}

/// `l << shift` with the workspace cap pre-flight the heap `Shl` path applies.
fn bigint_shl(l: &BigInt, shift: usize) -> Result<Arc<BigInt>, VmRustFnError> {
    let estimated_bits = l.bits().saturating_add(shift as u64);
    if estimated_bits > MAX_BIGINT_BITS {
        return Err(alloc_failure_panic(format!(
            "bigint shl: result of {l} << {shift} would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})"
        )));
    }
    Ok(Arc::new(l << shift))
}

/// `l >> count` where the count is already known non-negative; a count too
/// large for `usize` is "shift past every bit" - `0n` / `-1n` by sign.
fn bigint_shr(l: &BigInt, count: Option<usize>) -> Arc<BigInt> {
    Arc::new(match count {
        Some(shift) => l >> shift,
        None if l.sign() == num_bigint::Sign::Minus => BigInt::from(-1),
        None => BigInt::ZERO,
    })
}

/// Resolve a `bigint >>` count: negative throws; `None` means past every bit.
fn bigint_shr_count(rhs: &BigInt) -> Result<Option<usize>, VmRustFnError> {
    if rhs.sign() == num_bigint::Sign::Minus {
        return Err(negative_bit_shift(rhs));
    }
    Ok(usize::try_from(rhs).ok())
}

// ── BitAnd ──────────────────────────────────────────────────────────────────

impl BamlClassOpsBitAnd_int__for_int for PackageBamlImpl {
    fn bit_and(int: i64, rhs: i64) -> i64 {
        int & rhs
    }
}

impl BamlClassOpsBitAnd_bigint__for_int for PackageBamlImpl {
    // `int & bigint`
    fn bit_and(int: i64, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(BigInt::from(int) & rhs.as_ref())
    }
}

impl BamlClassOpsBitAnd_int__for_bigint for PackageBamlImpl {
    // `bigint & int`
    fn bit_and(bigint: Arc<BigInt>, rhs: i64) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() & BigInt::from(rhs))
    }
}

impl BamlClassOpsBitAnd_bigint__for_bigint for PackageBamlImpl {
    fn bit_and(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() & rhs.as_ref())
    }
}

// ── BitOr ───────────────────────────────────────────────────────────────────

impl BamlClassOpsBitOr_int__for_int for PackageBamlImpl {
    fn bit_or(int: i64, rhs: i64) -> i64 {
        int | rhs
    }
}

impl BamlClassOpsBitOr_bigint__for_int for PackageBamlImpl {
    // `int | bigint`
    fn bit_or(int: i64, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(BigInt::from(int) | rhs.as_ref())
    }
}

impl BamlClassOpsBitOr_int__for_bigint for PackageBamlImpl {
    // `bigint | int`
    fn bit_or(bigint: Arc<BigInt>, rhs: i64) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() | BigInt::from(rhs))
    }
}

impl BamlClassOpsBitOr_bigint__for_bigint for PackageBamlImpl {
    fn bit_or(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() | rhs.as_ref())
    }
}

// ── BitXor ──────────────────────────────────────────────────────────────────

impl BamlClassOpsBitXor_int__for_int for PackageBamlImpl {
    fn bit_xor(int: i64, rhs: i64) -> i64 {
        int ^ rhs
    }
}

impl BamlClassOpsBitXor_bigint__for_int for PackageBamlImpl {
    // `int ^ bigint`
    fn bit_xor(int: i64, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(BigInt::from(int) ^ rhs.as_ref())
    }
}

impl BamlClassOpsBitXor_int__for_bigint for PackageBamlImpl {
    // `bigint ^ int`
    fn bit_xor(bigint: Arc<BigInt>, rhs: i64) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() ^ BigInt::from(rhs))
    }
}

impl BamlClassOpsBitXor_bigint__for_bigint for PackageBamlImpl {
    fn bit_xor(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() ^ rhs.as_ref())
    }
}

// ── ShiftLeft ───────────────────────────────────────────────────────────────

impl BamlClassOpsShiftLeft_int__for_int for PackageBamlImpl {
    fn shl(int: i64, rhs: i64) -> Result<i64, VmRustFnError> {
        int_shl(int, rhs)
    }
}

impl BamlClassOpsShiftLeft_bigint__for_int for PackageBamlImpl {
    // `int << bigint`: the VALUE widens to bigint, matching the bytecode's
    // operand widening (the result-type rule in `bitwise.baml`).
    fn shl(int: i64, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        let shift = bigint_shl_count(&rhs)?;
        bigint_shl(&BigInt::from(int), shift)
    }
}

impl BamlClassOpsShiftLeft_int__for_bigint for PackageBamlImpl {
    // `bigint << int`
    fn shl(bigint: Arc<BigInt>, rhs: i64) -> Result<Arc<BigInt>, VmRustFnError> {
        let Ok(shift) = usize::try_from(rhs) else {
            return Err(negative_bit_shift(rhs));
        };
        bigint_shl(&bigint, shift)
    }
}

impl BamlClassOpsShiftLeft_bigint__for_bigint for PackageBamlImpl {
    fn shl(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        let shift = bigint_shl_count(&rhs)?;
        bigint_shl(&bigint, shift)
    }
}

// ── ShiftRight ──────────────────────────────────────────────────────────────

impl BamlClassOpsShiftRight_int__for_int for PackageBamlImpl {
    fn shr(int: i64, rhs: i64) -> Result<i64, VmRustFnError> {
        int_shr(int, rhs)
    }
}

impl BamlClassOpsShiftRight_bigint__for_int for PackageBamlImpl {
    // `int >> bigint`: widens like `<<` above.
    fn shr(int: i64, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        let count = bigint_shr_count(&rhs)?;
        Ok(bigint_shr(&BigInt::from(int), count))
    }
}

impl BamlClassOpsShiftRight_int__for_bigint for PackageBamlImpl {
    // `bigint >> int`
    fn shr(bigint: Arc<BigInt>, rhs: i64) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs < 0 {
            return Err(negative_bit_shift(rhs));
        }
        Ok(bigint_shr(&bigint, usize::try_from(rhs).ok()))
    }
}

impl BamlClassOpsShiftRight_bigint__for_bigint for PackageBamlImpl {
    fn shr(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        let count = bigint_shr_count(&rhs)?;
        Ok(bigint_shr(&bigint, count))
    }
}
