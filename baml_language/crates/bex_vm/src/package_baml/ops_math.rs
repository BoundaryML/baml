//! Native implementations of the `baml.ops` arithmetic interfaces
//! (`Add` / `Subtract` / `Multiply` / `Divide` / `Remainder` / `Negate`) for the
//! numeric primitives, declared in `baml_std/baml/ns_ops/math.baml`.
//!
//! These mirror BAML's `+` / `-` / `*` / `/` / `%` and unary `-` operators, which
//! the compiler special-cases to direct `AddInt` / `AddFloat` / `AddBigint` …
//! bytecode when the operand types are statically known. They back the same
//! operations when one is reached through a generic bound (`T extends Add<...>`)
//! and define the canonical result the specialized bytecode matches.
//!
//! Semantics (see also the header in `math.baml`):
//! - `int` is i63 (the low `Value` bit is the tag), so arithmetic **wraps** into
//!   `[Value::INT_MIN, Value::INT_MAX]` on overflow — exactly what the
//!   `AddInt` / `SubInt` bytecode does on the tagged representation
//!   ([`Value::tagged_int_add`]). Wrapping keeps these `throws never`: a result
//!   the glue's `Value::try_int` range check would reject can never escape.
//! - `float` follows IEEE-754 throughout, including `/` and `%` by zero (which
//!   yield `inf` / `NaN`, not a panic).
//! - integer (`int` / `bigint`) `/` and `%` by zero raise
//!   [`VmPanic::DivisionByZero`]; a `bigint` product beyond the workspace size
//!   cap raises [`VmPanic::AllocFailure`]. Both are panics, which are orthogonal
//!   to the `throws` contract — so the signatures stay `throws never` (the panic
//!   is reported via `//baml:fallible` glue).
//! - mixed `int`/`float` widens the `int` to `f64` (lossy past 2^53, same as the
//!   `AddFloat`-family bytecode); mixed `int`/`bigint` widens the `int` to a
//!   `BigInt`.

use std::sync::Arc;

use bex_vm_types::{Value, errors::VmPanic};
use num_bigint::BigInt;

use super::{
    BamlClassOpsAdd_bigint__for_bigint, BamlClassOpsAdd_bigint__for_int,
    BamlClassOpsAdd_float__for_float, BamlClassOpsAdd_float__for_int,
    BamlClassOpsAdd_int__for_bigint, BamlClassOpsAdd_int__for_float, BamlClassOpsAdd_int__for_int,
    BamlClassOpsDivide_bigint__for_bigint, BamlClassOpsDivide_bigint__for_int,
    BamlClassOpsDivide_float__for_float, BamlClassOpsDivide_float__for_int,
    BamlClassOpsDivide_int__for_bigint, BamlClassOpsDivide_int__for_float,
    BamlClassOpsDivide_int__for_int, BamlClassOpsMultiply_bigint__for_bigint,
    BamlClassOpsMultiply_bigint__for_int, BamlClassOpsMultiply_float__for_float,
    BamlClassOpsMultiply_float__for_int, BamlClassOpsMultiply_int__for_bigint,
    BamlClassOpsMultiply_int__for_float, BamlClassOpsMultiply_int__for_int,
    BamlClassOpsNegate_for_bigint, BamlClassOpsNegate_for_float, BamlClassOpsNegate_for_int,
    BamlClassOpsRemainder_bigint__for_bigint, BamlClassOpsRemainder_bigint__for_int,
    BamlClassOpsRemainder_float__for_float, BamlClassOpsRemainder_float__for_int,
    BamlClassOpsRemainder_int__for_bigint, BamlClassOpsRemainder_int__for_float,
    BamlClassOpsRemainder_int__for_int, BamlClassOpsSubtract_bigint__for_bigint,
    BamlClassOpsSubtract_bigint__for_int, BamlClassOpsSubtract_float__for_float,
    BamlClassOpsSubtract_float__for_int, BamlClassOpsSubtract_int__for_bigint,
    BamlClassOpsSubtract_int__for_float, BamlClassOpsSubtract_int__for_int, PackageBamlImpl,
    bigint::{MAX_BIGINT_BITS, alloc_failure_panic},
};
use crate::{BexVm, errors::VmRustFnError};

// ── shared helpers ──────────────────────────────────────────────────────────

/// Reduce an `i64` into the i63 range `[INT_MIN, INT_MAX]` by two's-complement
/// wrapping — the same wrap the `AddInt` / `SubInt` / `MulInt` bytecode performs
/// (those operate on the `(value << 1) | 1` tagged bits). Shifting left then
/// arithmetic-right by one keeps the low 63 bits and sign-extends from bit 62.
const fn wrap_i63(x: i64) -> i64 {
    (x << 1) >> 1
}

/// Widen a BAML `int` (i63) to `f64` for mixed `int`/`float` arithmetic. Values
/// past 2^53 lose precision — the same widening the `AddFloat`-family bytecode
/// applies (`as_int().map(|i| i as f64)`), so operator and method agree.
#[expect(clippy::cast_precision_loss)]
const fn widen(n: i64) -> f64 {
    n as f64
}

/// Build the `baml.panics.DivisionByZero` raised by integer `/` or `%` with a
/// zero divisor. `left` is the dividend (surfaced to `catch` handlers); `right`
/// is the divisor, kept for the panic's `Display`.
fn division_by_zero(left: Value, right: Value) -> VmRustFnError {
    VmPanic::DivisionByZero { left, right }.into()
}

/// `a * b` as a `bigint`, rejecting before materializing a product past the
/// workspace cap. `bits(a*b) <= bits(a) + bits(b)` exactly, so the estimate
/// never under-counts — mirrors `bigint_binop`'s `Mul` pre-flight so a generic
/// `Multiply` dispatch can't allocate an intermediate twice the limit.
fn checked_bigint_mul(a: &BigInt, b: &BigInt) -> Result<Arc<BigInt>, VmRustFnError> {
    let estimated_bits = a.bits().saturating_add(b.bits());
    if estimated_bits > MAX_BIGINT_BITS {
        return Err(alloc_failure_panic(format!(
            "bigint mul: result would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})"
        )));
    }
    Ok(Arc::new(a * b))
}

// ── Add ─────────────────────────────────────────────────────────────────────

impl BamlClassOpsAdd_int__for_int for PackageBamlImpl {
    fn add(int: i64, rhs: i64) -> i64 {
        wrap_i63(int.wrapping_add(rhs))
    }
}

impl BamlClassOpsAdd_float__for_float for PackageBamlImpl {
    fn add(float: f64, rhs: f64) -> f64 {
        float + rhs
    }
}

impl BamlClassOpsAdd_bigint__for_bigint for PackageBamlImpl {
    fn add(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() + rhs.as_ref())
    }
}

impl BamlClassOpsAdd_int__for_float for PackageBamlImpl {
    // `float + int`
    fn add(float: f64, rhs: i64) -> f64 {
        float + widen(rhs)
    }
}

impl BamlClassOpsAdd_float__for_int for PackageBamlImpl {
    // `int + float`
    fn add(int: i64, rhs: f64) -> f64 {
        widen(int) + rhs
    }
}

impl BamlClassOpsAdd_int__for_bigint for PackageBamlImpl {
    // `bigint + int`
    fn add(bigint: Arc<BigInt>, rhs: i64) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() + BigInt::from(rhs))
    }
}

impl BamlClassOpsAdd_bigint__for_int for PackageBamlImpl {
    // `int + bigint`
    fn add(int: i64, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(BigInt::from(int) + rhs.as_ref())
    }
}

// ── Subtract ────────────────────────────────────────────────────────────────

impl BamlClassOpsSubtract_int__for_int for PackageBamlImpl {
    fn sub(int: i64, rhs: i64) -> i64 {
        wrap_i63(int.wrapping_sub(rhs))
    }
}

impl BamlClassOpsSubtract_float__for_float for PackageBamlImpl {
    fn sub(float: f64, rhs: f64) -> f64 {
        float - rhs
    }
}

impl BamlClassOpsSubtract_bigint__for_bigint for PackageBamlImpl {
    fn sub(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() - rhs.as_ref())
    }
}

impl BamlClassOpsSubtract_int__for_float for PackageBamlImpl {
    // `float - int`
    fn sub(float: f64, rhs: i64) -> f64 {
        float - widen(rhs)
    }
}

impl BamlClassOpsSubtract_float__for_int for PackageBamlImpl {
    // `int - float`
    fn sub(int: i64, rhs: f64) -> f64 {
        widen(int) - rhs
    }
}

impl BamlClassOpsSubtract_int__for_bigint for PackageBamlImpl {
    // `bigint - int`
    fn sub(bigint: Arc<BigInt>, rhs: i64) -> Arc<BigInt> {
        Arc::new(bigint.as_ref() - BigInt::from(rhs))
    }
}

impl BamlClassOpsSubtract_bigint__for_int for PackageBamlImpl {
    // `int - bigint`
    fn sub(int: i64, rhs: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(BigInt::from(int) - rhs.as_ref())
    }
}

// ── Multiply ────────────────────────────────────────────────────────────────

impl BamlClassOpsMultiply_int__for_int for PackageBamlImpl {
    fn mul(int: i64, rhs: i64) -> i64 {
        wrap_i63(int.wrapping_mul(rhs))
    }
}

impl BamlClassOpsMultiply_float__for_float for PackageBamlImpl {
    fn mul(float: f64, rhs: f64) -> f64 {
        float * rhs
    }
}

impl BamlClassOpsMultiply_bigint__for_bigint for PackageBamlImpl {
    fn mul(bigint: Arc<BigInt>, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        checked_bigint_mul(bigint.as_ref(), rhs.as_ref())
    }
}

impl BamlClassOpsMultiply_int__for_float for PackageBamlImpl {
    // `float * int`
    fn mul(float: f64, rhs: i64) -> f64 {
        float * widen(rhs)
    }
}

impl BamlClassOpsMultiply_float__for_int for PackageBamlImpl {
    // `int * float`
    fn mul(int: i64, rhs: f64) -> f64 {
        widen(int) * rhs
    }
}

impl BamlClassOpsMultiply_int__for_bigint for PackageBamlImpl {
    // `bigint * int`
    fn mul(bigint: Arc<BigInt>, rhs: i64) -> Result<Arc<BigInt>, VmRustFnError> {
        checked_bigint_mul(bigint.as_ref(), &BigInt::from(rhs))
    }
}

impl BamlClassOpsMultiply_bigint__for_int for PackageBamlImpl {
    // `int * bigint`
    fn mul(int: i64, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        checked_bigint_mul(&BigInt::from(int), rhs.as_ref())
    }
}

// ── Divide ──────────────────────────────────────────────────────────────────
//
// `int` / `bigint` division by zero panics (`DivisionByZero`); `float` division
// is IEEE-754 (`x / 0.0` is `±inf` / `NaN`, never a panic).

impl BamlClassOpsDivide_int__for_int for PackageBamlImpl {
    fn div(int: i64, rhs: i64) -> Result<i64, VmRustFnError> {
        if rhs == 0 {
            return Err(division_by_zero(Value::int(int), Value::int(rhs)));
        }
        // `INT_MIN / -1` is the one quotient outside i63; `wrapping_div` avoids the
        // i64 overflow path (unreachable for i63 inputs) and `wrap_i63` folds it
        // back to `INT_MIN`, matching the `DivInt` bytecode in release.
        Ok(wrap_i63(int.wrapping_div(rhs)))
    }
}

impl BamlClassOpsDivide_float__for_float for PackageBamlImpl {
    fn div(float: f64, rhs: f64) -> f64 {
        float / rhs
    }
}

impl BamlClassOpsDivide_bigint__for_bigint for PackageBamlImpl {
    fn div(
        vm: &mut BexVm,
        bigint: Arc<BigInt>,
        rhs: Arc<BigInt>,
    ) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs.as_ref() == &BigInt::ZERO {
            let left = vm.try_alloc_bigint(bigint)?;
            let right = vm.try_alloc_bigint(rhs)?;
            return Err(division_by_zero(left, right));
        }
        Ok(Arc::new(bigint.as_ref() / rhs.as_ref()))
    }
}

impl BamlClassOpsDivide_int__for_float for PackageBamlImpl {
    // `float / int` — IEEE, so a zero divisor yields `±inf` / `NaN`.
    fn div(float: f64, rhs: i64) -> f64 {
        float / widen(rhs)
    }
}

impl BamlClassOpsDivide_float__for_int for PackageBamlImpl {
    // `int / float` — IEEE, so a zero divisor yields `±inf` / `NaN`.
    fn div(int: i64, rhs: f64) -> f64 {
        widen(int) / rhs
    }
}

impl BamlClassOpsDivide_int__for_bigint for PackageBamlImpl {
    // `bigint / int`
    fn div(vm: &mut BexVm, bigint: Arc<BigInt>, rhs: i64) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs == 0 {
            let left = vm.try_alloc_bigint(bigint)?;
            return Err(division_by_zero(left, Value::int(rhs)));
        }
        Ok(Arc::new(bigint.as_ref() / BigInt::from(rhs)))
    }
}

impl BamlClassOpsDivide_bigint__for_int for PackageBamlImpl {
    // `int / bigint`
    fn div(vm: &mut BexVm, int: i64, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs.as_ref() == &BigInt::ZERO {
            let right = vm.try_alloc_bigint(rhs)?;
            return Err(division_by_zero(Value::int(int), right));
        }
        Ok(Arc::new(BigInt::from(int) / rhs.as_ref()))
    }
}

// ── Remainder ───────────────────────────────────────────────────────────────
//
// `int` / `bigint` `%` by zero panics like `/` (matching the `ModInt` /
// `ModBigint` bytecode); `float` `%` is IEEE truncated remainder (`fmod`, sign of
// the dividend), with `x % 0.0` yielding `NaN`.

impl BamlClassOpsRemainder_int__for_int for PackageBamlImpl {
    fn rem(int: i64, rhs: i64) -> Result<i64, VmRustFnError> {
        if rhs == 0 {
            return Err(division_by_zero(Value::int(int), Value::int(rhs)));
        }
        Ok(wrap_i63(int.wrapping_rem(rhs)))
    }
}

impl BamlClassOpsRemainder_float__for_float for PackageBamlImpl {
    fn rem(float: f64, rhs: f64) -> f64 {
        float % rhs
    }
}

impl BamlClassOpsRemainder_bigint__for_bigint for PackageBamlImpl {
    fn rem(
        vm: &mut BexVm,
        bigint: Arc<BigInt>,
        rhs: Arc<BigInt>,
    ) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs.as_ref() == &BigInt::ZERO {
            let left = vm.try_alloc_bigint(bigint)?;
            let right = vm.try_alloc_bigint(rhs)?;
            return Err(division_by_zero(left, right));
        }
        Ok(Arc::new(bigint.as_ref() % rhs.as_ref()))
    }
}

impl BamlClassOpsRemainder_int__for_float for PackageBamlImpl {
    // `float % int` — IEEE, so a zero divisor yields `NaN`.
    fn rem(float: f64, rhs: i64) -> f64 {
        float % widen(rhs)
    }
}

impl BamlClassOpsRemainder_float__for_int for PackageBamlImpl {
    // `int % float` — IEEE, so a zero divisor yields `NaN`.
    fn rem(int: i64, rhs: f64) -> f64 {
        widen(int) % rhs
    }
}

impl BamlClassOpsRemainder_int__for_bigint for PackageBamlImpl {
    // `bigint % int`
    fn rem(vm: &mut BexVm, bigint: Arc<BigInt>, rhs: i64) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs == 0 {
            let left = vm.try_alloc_bigint(bigint)?;
            return Err(division_by_zero(left, Value::int(rhs)));
        }
        Ok(Arc::new(bigint.as_ref() % BigInt::from(rhs)))
    }
}

impl BamlClassOpsRemainder_bigint__for_int for PackageBamlImpl {
    // `int % bigint`
    fn rem(vm: &mut BexVm, int: i64, rhs: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        if rhs.as_ref() == &BigInt::ZERO {
            let right = vm.try_alloc_bigint(rhs)?;
            return Err(division_by_zero(Value::int(int), right));
        }
        Ok(Arc::new(BigInt::from(int) % rhs.as_ref()))
    }
}

// ── Negate ──────────────────────────────────────────────────────────────────

impl BamlClassOpsNegate_for_int for PackageBamlImpl {
    // `-INT_MIN` is unrepresentable in i63 and wraps back to `INT_MIN`, matching
    // the `Neg` bytecode in release.
    fn neg(int: i64) -> i64 {
        wrap_i63(int.wrapping_neg())
    }
}

impl BamlClassOpsNegate_for_float for PackageBamlImpl {
    fn neg(float: f64) -> f64 {
        -float
    }
}

impl BamlClassOpsNegate_for_bigint for PackageBamlImpl {
    fn neg(bigint: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(-bigint.as_ref())
    }
}
