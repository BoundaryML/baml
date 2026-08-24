use std::sync::Arc;

/// Re-export of the shared `bigint` allocation cap. Defined in `baml_type`
/// so TIR's constant-folder can refuse to fold bigint expressions that the
/// VM would refuse to allocate.
pub(crate) use baml_type::MAX_BIGINT_BITS;
use bex_str::BexStr;
use bex_vm_types::Value;
use num_bigint::{BigInt, BigUint, Sign};

use super::{BamlClassBigint, PackageBamlImpl};
use crate::errors::{VmBamlError, VmPanic, VmRustFnError};

impl BamlClassBigint for PackageBamlImpl {
    fn abs(bigint: Arc<BigInt>) -> Arc<BigInt> {
        if bigint.sign() == Sign::Minus {
            // Negate to get the absolute value.
            Arc::new(-bigint.as_ref().clone())
        } else {
            bigint
        }
    }

    fn min(bigint: Arc<BigInt>, other: Arc<BigInt>) -> Arc<BigInt> {
        if bigint <= other { bigint } else { other }
    }

    fn max(bigint: Arc<BigInt>, other: Arc<BigInt>) -> Arc<BigInt> {
        if bigint >= other { bigint } else { other }
    }

    fn clamp(bigint: Arc<BigInt>, min: Arc<BigInt>, max: Arc<BigInt>) -> Arc<BigInt> {
        // Two-step clamp: first cap at max, then floor at min.
        // Matches int.clamp's behaviour when min > max (lower-clamp wins).
        let v = if bigint <= max { bigint } else { max };
        if v >= min { v } else { min }
    }

    fn isqrt(bigint: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        if bigint.sign() == Sign::Minus {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "bigint.isqrt: negative input ({bigint}) has no integer square root"
                ),
            }
            .into());
        }
        Ok(Arc::new(bigint.sqrt()))
    }

    fn pow(bigint: Arc<BigInt>, exp: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        // Negative exponent produces 0 by BEP §Methods (rounding to zero for |base|>1).
        if exp.sign() == Sign::Minus {
            return Ok(Arc::new(BigInt::ZERO));
        }
        // Short-circuit bases in {-1, 0, 1} — the result has bit-length 0 or 1
        // regardless of how large `exp` is. Without this, the `bits() * exp`
        // overestimate below would reject e.g. `(1n).pow(2^29)` with a bogus
        // AllocFailure even though the result is just `1`.
        if bigint.as_ref() == &BigInt::ZERO {
            // 0^0 == 1 by convention; 0^positive == 0.
            let is_zero_exp = exp.as_ref() == &BigInt::ZERO;
            return Ok(Arc::new(if is_zero_exp {
                BigInt::from(1)
            } else {
                BigInt::ZERO
            }));
        }
        if bigint.as_ref() == &BigInt::from(1) {
            return Ok(Arc::new(BigInt::from(1)));
        }
        if bigint.as_ref() == &BigInt::from(-1) {
            // (-1)^even == 1, (-1)^odd == -1.
            let is_even = exp.as_ref() % 2 == BigInt::ZERO;
            return Ok(Arc::new(if is_even {
                BigInt::from(1)
            } else {
                BigInt::from(-1)
            }));
        }
        // Pre-flight memory guard for |base| >= 2: bits(b^e) ≤ bits(b) * e
        // exactly, so reject anything that would blow past `MAX_BIGINT_BITS`.
        let base_bits = bigint.bits();
        // `BigInt::pow` takes a `u32`. Anything outside that range is far past
        // what we could materialise even with a 2-bit base, so map the
        // conversion failure straight onto an `AllocFailure` panic.
        let exp_u32 = u32::try_from(exp.as_ref()).map_err(|_| {
            alloc_failure_panic(format!(
                "bigint.pow: exponent ({exp}) exceeds memory limits"
            ))
        })?;
        let estimated_bits = base_bits.saturating_mul(u64::from(exp_u32));
        if estimated_bits > MAX_BIGINT_BITS {
            return Err(alloc_failure_panic(format!(
                "bigint.pow: result of {bigint}^{exp} would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})"
            )));
        }
        Ok(Arc::new(bigint.pow(exp_u32)))
    }

    fn ilog(bigint: Arc<BigInt>, base: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        if bigint.sign() != Sign::Plus {
            return Err(VmBamlError::InvalidArgument {
                message: format!("bigint.ilog: input ({bigint}) must be positive"),
            }
            .into());
        }
        if *base < BigInt::from(2u32) {
            return Err(VmBamlError::InvalidArgument {
                message: format!("bigint.ilog: base ({base}) must be at least 2"),
            }
            .into());
        }
        // Compute floor(log_base(bigint)). The previous implementation looped
        // base.pow(0) → base.pow(1) → … via repeated division, which is O(N)
        // bigint divisions where N = floor(log_base(bigint)). Replace with a
        // binary search over `k` in `[0, k_max]` using `base.pow(k)` — O(log N)
        // bigint comparisons and O(log N · M(N)) total bit-work, where M is
        // the multiplication cost.
        //
        // For `base == 2`, short-circuit via `bits()`: floor(log2(x)) is just
        // `x.bits() - 1` for any positive x.
        if base.as_ref() == &BigInt::from(2u32) {
            // bigint > 0, so bits() >= 1.
            return Ok(Arc::new(BigInt::from(bigint.bits() - 1)));
        }
        // Upper bound: `log_b(x) ≤ bits(x) / (bits(b) - 1)` because for b ≥ 2,
        // 2^(bits(b)-1) ≤ b, so b^k ≥ 2^(k * (bits(b)-1)). Add 1 for slack and
        // saturate to u32::MAX since BigInt::pow takes u32.
        let denom = base.bits().saturating_sub(1).max(1);
        let k_max_u64 = (bigint.bits() / denom)
            .saturating_add(1)
            .min(u64::from(u32::MAX));
        // Safe: clamped to u32::MAX above.
        let k_max = u32::try_from(k_max_u64).unwrap_or(u32::MAX);

        let mut lo: u32 = 0;
        let mut hi: u32 = k_max;
        while lo < hi {
            // Bias the midpoint upward so the loop invariant
            // `base.pow(lo) <= bigint` advances toward `hi`.
            let mid = lo + (hi - lo).div_ceil(2);
            let candidate = base.pow(mid);
            if &candidate <= bigint.as_ref() {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        Ok(Arc::new(BigInt::from(lo)))
    }

    fn to_int(bigint: Arc<BigInt>) -> Result<i64, VmRustFnError> {
        // BAML int is i63 (low bit reserved for the Value tag), so the
        // representable range is narrower than i64's and `i64::try_from` alone
        // is not the right gate: the codegen wraps this return in `Value::int`,
        // which only debug-asserts the i63 range and truncates in release.
        //
        // The message reports the operand's bit width rather than the operand:
        // a bigint is unbounded and has no business inside a diagnostic string.
        i64::try_from(bigint.as_ref())
            .ok()
            .filter(|i| (Value::INT_MIN..=Value::INT_MAX).contains(i))
            .ok_or_else(|| {
                VmBamlError::InvalidArgument {
                    message: format!(
                        "bigint.to_int: a {}-bit value is outside int's range \
                         (int is 63-bit signed)",
                        bigint.bits()
                    ),
                }
                .into()
            })
    }

    fn parse(text: &BexStr) -> Result<Arc<BigInt>, VmRustFnError> {
        // Accept an optional leading sign followed by ASCII digits, matching the
        // documented behaviour: no whitespace, no underscores, no other formats.
        let text: &str = text;
        let (sign_str, digits) = if let Some(rest) = text.strip_prefix('-') {
            ("-", rest)
        } else if let Some(rest) = text.strip_prefix('+') {
            ("", rest)
        } else {
            ("", text)
        };

        // Reject empty string or non-digit chars after the sign.
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(VmBamlError::ParseError {
                message: format!("bigint.parse: cannot parse {text:?} as bigint"),
            }
            .into());
        }

        // Pre-flight: reject inputs that would produce a bigint past the
        // workspace cap before allocating the parsed value. Each decimal
        // digit contributes at most ~3.32 bits, so a string longer than
        // `MAX_BIGINT_DECIMAL_DIGITS` cannot fit and must be refused.
        // `bigint.parse` is user-callable from BAML, so this is a reachable
        // allocation vector — match the SAP / FFI guards.
        if digits.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS {
            return Err(VmPanic::AllocFailure {
                message: format!(
                    "bigint.parse: input has {} decimal digits, more than the \
                     {}-digit limit (bigint cap: {} bits)",
                    digits.len(),
                    baml_type::MAX_BIGINT_DECIMAL_DIGITS,
                    MAX_BIGINT_BITS
                ),
            }
            .into());
        }

        let full = format!("{sign_str}{digits}");
        BigInt::parse_bytes(full.as_bytes(), 10)
            .map(Arc::new)
            .ok_or_else(|| {
                VmBamlError::ParseError {
                    message: format!("bigint.parse: cannot parse {text:?} as bigint"),
                }
                .into()
            })
    }

    fn _random_byte_count(lower: Arc<BigInt>, upper: Arc<BigInt>) -> i64 {
        if lower >= upper {
            return 0;
        }
        i64::try_from(random_draw_bits(&lower, &upper).div_ceil(8)).unwrap_or_else(|_| {
            unreachable!("bigint._random_byte_count: a range that wide cannot be allocated")
        })
    }

    fn _random_in_range(draw: &[u8], lower: Arc<BigInt>, upper: Arc<BigInt>) -> Arc<BigInt> {
        if lower >= upper {
            return upper;
        }
        let bits = random_draw_bits(&lower, &upper);
        let width = usize::try_from(bits.div_ceil(8)).unwrap_or_else(|_| {
            unreachable!("bigint._random_in_range: a range that wide cannot be allocated")
        });
        if draw.len() < width {
            return upper;
        }

        // Mask unused high bits so rejection stays below one half.
        let mut buf = draw[..width].to_vec();
        if let Some(top) = buf.first_mut() {
            let excess = width as u64 * 8 - bits;
            *top &= 0xFF_u8 >> excess;
        }

        let sample = BigUint::from_bytes_be(&buf);
        let range = random_range(&lower, &upper);
        if sample < range {
            Arc::new(BigInt::from(sample) + lower.as_ref())
        } else {
            upper
        }
    }
}

/// `upper - lower`, the count of values in `[lower, upper)`. Always at least 1;
/// callers are required to have rejected an empty range.
fn random_range(lower: &BigInt, upper: &BigInt) -> BigUint {
    debug_assert!(
        lower < upper,
        "bigint.random: empty range is the caller's to reject"
    );
    (upper - lower)
        .to_biguint()
        .unwrap_or_else(|| unreachable!("bigint.random: lower < upper makes the range positive"))
}

/// Number of random bits one draw over `[lower, upper)` needs: the bit length of
/// `range - 1`, i.e. the smallest `b` with `2^b >= range`.
///
/// Taking `range - 1` rather than `range` is what makes a power-of-two range
/// (including a single-value range, which needs no bits at all) reject nothing;
/// every other range then rejects less than half the time.
fn random_draw_bits(lower: &BigInt, upper: &BigInt) -> u64 {
    (random_range(lower, upper) - 1_u32).bits()
}

/// Build a [`VmRustFnError::Panic`] carrying [`VmPanic::AllocFailure`] with
/// the given message. Centralises the wrapping so call sites stay readable.
pub(crate) fn alloc_failure_panic(message: String) -> VmRustFnError {
    VmRustFnError::Panic(VmPanic::AllocFailure { message })
}
