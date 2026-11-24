//! `bigint`: an integer of any size, immutable once made.
//!
//! The object is laid out by [`crate::layout::BIGINT_LIMBS`]. Arithmetic is
//! `num-bigint`'s, the interpreter's: a value is read out of its limbs,
//! worked on outside the heap, and the result written into a new object. An
//! operand that is an `int` is passed as the `int`, never as an object made
//! for it.
//!
//! What fails does so as it does in the interpreter: `/` and `%` by zero
//! with `baml.panics.DivisionByZero`, a shift by a negative count with
//! `baml.panics.NegativeBitShift`, and a result past [`MAX_BITS`] with
//! `baml.panics.AllocFailure`.

use num_bigint::{BigInt, Sign};

use crate::{
    class::{self, Init},
    heap::{Mutator, load, store},
    layout::{
        BIGINT_LEFT_IS_INT as LEFT_IS_INT, BIGINT_LIMB_COUNT, BIGINT_LIMBS,
        BIGINT_OP_ADD as OP_ADD, BIGINT_OP_AND as OP_AND, BIGINT_OP_DIV as OP_DIV,
        BIGINT_OP_MUL as OP_MUL, BIGINT_OP_OR as OP_OR, BIGINT_OP_REM as OP_REM,
        BIGINT_OP_SHL as OP_SHL, BIGINT_OP_SUB as OP_SUB, BIGINT_OP_XOR as OP_XOR,
        BIGINT_RIGHT_IS_INT as RIGHT_IS_INT, TYPE_BIGINT, WELL_KNOWN_ALLOC_FAILURE,
        WELL_KNOWN_DIVISION_BY_ZERO, WELL_KNOWN_NEGATIVE_BIT_SHIFT, WORD,
    },
    native::RtResult,
};

/// The most bits a `bigint` may take: the interpreter's limit.
pub(crate) const MAX_BITS: u64 = 1 << 28;

/// The value of the `bigint` at `object`.
///
/// # Safety
///
/// `object` is a live `bigint`.
pub(crate) unsafe fn value(object: usize) -> BigInt {
    // SAFETY: the caller's contract: the count and that many limbs. Both
    // targets are little-endian, so the limbs in memory are the value's
    // bytes, least significant first.
    unsafe {
        let limbs = load(object + BIGINT_LIMB_COUNT);
        BigInt::from_signed_bytes_le(std::slice::from_raw_parts(
            (object + BIGINT_LIMBS) as *const u8,
            limbs * WORD,
        ))
    }
}

/// The value of the `bigint` at `object` when it is an `int`'s.
///
/// # Safety
///
/// `object` is a live `bigint`.
pub(crate) unsafe fn as_int(object: usize) -> Option<i64> {
    // SAFETY: the caller's contract. One limb is an `int` as it stands.
    unsafe {
        (load(object + BIGINT_LIMB_COUNT) == 1)
            .then(|| load(object + BIGINT_LIMBS).cast_signed() as i64)
    }
}

/// A new `bigint` object holding `value`, which is within the limit. May
/// collect.
pub(crate) fn make(mutator: &mut Mutator, value: &BigInt) -> usize {
    let bytes = value.to_signed_bytes_le();
    let object = mutator.allocate_object(TYPE_BIGINT, size(&bytes));
    // SAFETY: the object has the room `size` says.
    unsafe { fill(object, &bytes) };
    object
}

/// The bytes a `bigint` object takes for the value whose two's complement
/// bytes, least significant first, are `bytes`.
pub(crate) fn size(bytes: &[u8]) -> usize {
    BIGINT_LIMBS + bytes.len().div_ceil(WORD).max(1) * WORD
}

/// Write the count and the limbs of the value whose two's complement bytes
/// are `bytes` into the `bigint` at `object`.
///
/// # Safety
///
/// `object` has the room [`size`] says for `bytes`.
pub(crate) unsafe fn fill(object: usize, bytes: &[u8]) {
    let limbs = (size(bytes) - BIGINT_LIMBS) / WORD;
    let sign = if bytes.last().is_some_and(|last| last & 0x80 != 0) {
        0xFF
    } else {
        0
    };
    // SAFETY: the caller's contract.
    unsafe {
        store(object + BIGINT_LIMB_COUNT, limbs);
        let data = (object + BIGINT_LIMBS) as *mut u8;
        std::ptr::write_bytes(data, sign, limbs * WORD);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
    }
}

/// A new `bigint` object holding `value`, or the `baml.panics.AllocFailure`
/// for one past the limit. May collect.
pub(crate) fn checked(mutator: &mut Mutator, value: &BigInt) -> Result<usize, usize> {
    let bits = value.bits();
    if bits > MAX_BITS {
        return Err(alloc_failure(
            mutator,
            &format!("bigint allocation requires {bits} bits (limit: {MAX_BITS})"),
        ));
    }
    Ok(make(mutator, value))
}

/// A `baml.panics.AllocFailure`. May collect.
pub(crate) fn alloc_failure(mutator: &mut Mutator, message: &str) -> usize {
    class::well_known(mutator, WELL_KNOWN_ALLOC_FAILURE, &[Init::Text(message)])
}

/// An operand of an operator: an `int`, or a `bigint` read out of its
/// object.
enum Operand {
    Int(i64),
    Big(BigInt),
}

impl Operand {
    /// # Safety
    ///
    /// `word` is the bits of an `int` when `is_int`, a live `bigint`
    /// otherwise.
    unsafe fn read(word: usize, is_int: bool) -> Self {
        if is_int {
            Self::Int(word.cast_signed() as i64)
        } else {
            // SAFETY: the caller's contract.
            Self::Big(unsafe { value(word) })
        }
    }

    fn big(&self) -> BigInt {
        match self {
            Self::Int(value) => BigInt::from(*value),
            Self::Big(value) => value.clone(),
        }
    }

    /// The field of a panic that names the operand.
    fn init(&self) -> Init<'_> {
        match self {
            Self::Int(value) => Init::BoxedInt(*value),
            Self::Big(value) => Init::Big(value),
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => value.fmt(f),
            Self::Big(value) => value.fmt(f),
        }
    }
}

/// Why an operator has no result.
enum Failure {
    DivisionByZero,
    NegativeShift(String),
    Alloc(String),
}

/// A shift count: `None` for one no object could be shifted by.
fn shift_count(name: &str, count: &Operand) -> Result<Option<usize>, Failure> {
    let negative = match count {
        Operand::Int(count) => *count < 0,
        Operand::Big(count) => count.sign() == Sign::Minus,
    };
    if negative {
        return Err(Failure::NegativeShift(format!(
            "bigint {name}: negative shift count ({count})"
        )));
    }
    Ok(match count {
        Operand::Int(count) => usize::try_from(*count).ok(),
        Operand::Big(count) => usize::try_from(count).ok(),
    })
}

fn binary(op: usize, left: &Operand, right: &Operand) -> Result<BigInt, Failure> {
    let (l, r) = (left.big(), right.big());
    Ok(match op {
        OP_ADD => l + r,
        OP_SUB => l - r,
        OP_MUL => {
            let bits = l.bits().saturating_add(r.bits());
            if bits > MAX_BITS {
                return Err(Failure::Alloc(format!(
                    "bigint mul: result of bigint multiplication would require ~{bits} bits (limit: {MAX_BITS})"
                )));
            }
            l * r
        }
        OP_DIV | OP_REM if r.sign() == Sign::NoSign => return Err(Failure::DivisionByZero),
        OP_DIV => l / r,
        OP_REM => l % r,
        OP_AND => l & r,
        OP_OR => l | r,
        OP_XOR => l ^ r,
        OP_SHL => {
            let Some(shift) = shift_count("shl", right)? else {
                return Err(Failure::Alloc(format!(
                    "bigint shl: shift count ({right}) does not fit in usize"
                )));
            };
            let bits = l.bits().saturating_add(shift as u64);
            if bits > MAX_BITS {
                return Err(Failure::Alloc(format!(
                    "bigint shl: result of {l} << {shift} would require ~{bits} bits (limit: {MAX_BITS})"
                )));
            }
            l << shift
        }
        _ => match shift_count("shr", right)? {
            Some(shift) => l >> shift,
            None if l.sign() == Sign::Minus => BigInt::from(-1),
            None => BigInt::ZERO,
        },
    })
}

crate::stubs::anchored! {
    /// The symbol of [`bigint_binary`].
    baml_rt_bigint_binary => bigint_binary
}

/// `left op right` where at least one operand is a `bigint`: the result, a
/// new `bigint`, or what the operator throws.
///
/// # Safety
///
/// Called from compiled code with its mutator. `op` is one of the `OP_*`
/// with [`LEFT_IS_INT`] and [`RIGHT_IS_INT`] saying which operands are the
/// bits of an `int`; the others are live `bigint`s.
unsafe extern "C" fn bigint_binary(
    mutator: &mut Mutator,
    op: usize,
    left: usize,
    right: usize,
) -> RtResult {
    // SAFETY: the caller's contract. Both are read before anything is
    // allocated, so neither needs to be kept.
    let (left, right) = unsafe {
        (
            Operand::read(left, op & LEFT_IS_INT != 0),
            Operand::read(right, op & RIGHT_IS_INT != 0),
        )
    };
    let made = match binary(op & 0xFF, &left, &right) {
        Ok(result) => checked(mutator, &result),
        Err(Failure::DivisionByZero) => Err(class::well_known(
            mutator,
            WELL_KNOWN_DIVISION_BY_ZERO,
            &[left.init()],
        )),
        Err(Failure::NegativeShift(message)) => Err(class::well_known(
            mutator,
            WELL_KNOWN_NEGATIVE_BIT_SHIFT,
            &[Init::Text(&message)],
        )),
        Err(Failure::Alloc(message)) => Err(alloc_failure(mutator, &message)),
    };
    match made {
        Ok(object) => RtResult::ok(object as u64),
        Err(thrown) => RtResult { value: 0, thrown },
    }
}

/// How `left` orders against `right`, at least one of them a `bigint`:
/// negative, zero or positive. Allocates nothing on the heap.
///
/// # Safety
///
/// `flags` is [`LEFT_IS_INT`] and [`RIGHT_IS_INT`] for the operands that
/// are the bits of an `int`; the others are live `bigint`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_bigint_cmp(flags: usize, left: usize, right: usize) -> i64 {
    // SAFETY: the caller's contract.
    unsafe {
        let left = Operand::read(left, flags & LEFT_IS_INT != 0);
        let right = Operand::read(right, flags & RIGHT_IS_INT != 0);
        left.big().cmp(&right.big()) as i64
    }
}

/// A count of nanoseconds to sleep for: the value, or the nearest an
/// `int` holds.
///
/// # Safety
///
/// `value` is a live `bigint`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_bigint_to_nanos(value: usize) -> i64 {
    // SAFETY: the caller's contract.
    unsafe {
        as_int(value).unwrap_or_else(|| {
            if self::value(value).sign() == Sign::Minus {
                i64::MIN
            } else {
                i64::MAX
            }
        })
    }
}

crate::stubs::anchored! {
    /// The symbol of [`bigint_neg`].
    baml_rt_bigint_neg => bigint_neg
}

/// `-value`.
///
/// # Safety
///
/// Called from compiled code with its mutator and a live `bigint`.
unsafe extern "C" fn bigint_neg(mutator: &mut Mutator, value: usize) -> RtResult {
    // SAFETY: the caller's contract.
    let negated = -unsafe { self::value(value) };
    match checked(mutator, &negated) {
        Ok(object) => RtResult::ok(object as u64),
        Err(thrown) => RtResult { value: 0, thrown },
    }
}

crate::stubs::anchored! {
    /// The symbol of [`string_from_bigint`].
    baml_rt_string_from_bigint => string_from_bigint
}

/// The decimal text of a `bigint`, as a string object.
///
/// # Safety
///
/// Called from compiled code with its mutator and a live `bigint`.
unsafe extern "C" fn string_from_bigint(mutator: &mut Mutator, value: usize) -> usize {
    // SAFETY: the caller's contract.
    let text = unsafe { self::value(value) }.to_string();
    crate::string::from_str(mutator, &text)
}

/// Print a `bigint` result as `baml run` does.
///
/// # Safety
///
/// `value` is a live `bigint`.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_print_bigint(value: usize) {
    // SAFETY: the caller's contract.
    crate::print_line(&unsafe { self::value(value) }.to_string());
}
