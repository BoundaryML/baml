use std::sync::Arc;

use bex_vm_types::Value;
use num_bigint::{BigInt, BigUint, Sign};

use super::{BamlClassBigint, PackageBamlImpl};
use crate::errors::{VmBamlError, VmPanic, VmRustFnError};

/// Upper bound on the bit-length of a bigint we are willing to allocate from
/// `pow` / `shl`. ~268 million bits ≈ 80 million decimal digits ≈ 32 MiB of
/// digits. Beyond this we raise [`VmPanic::AllocFailure`] instead of letting
/// the allocator either succeed (and starve the rest of the runtime) or abort
/// the process outright.
pub(crate) const MAX_BIGINT_BITS: u64 = 1 << 28;

impl BamlClassBigint for PackageBamlImpl {
    fn to_json(_bigint: Arc<BigInt>) -> Value {
        unimplemented!("bigint.to_json: not yet implemented")
    }

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
        // 0^0 == 1 by convention; `BigInt::pow(0)` also yields 1 so this falls
        // through naturally.
        //
        // Pre-flight memory guard: estimate the bit-length of the result and
        // reject anything that would blow past `MAX_BIGINT_BITS`. We use
        // `bits() * exp` as an upper bound — exact for `|base| >= 2` and an
        // over-estimate (still safe) for base in {-1, 0, 1}, which collapse to
        // tiny results anyway and need no guard.
        let base_bits = bigint.bits();
        // `BigInt::pow` takes a `u32`. Anything outside that range is far past
        // what we could materialise even with a single-bit base, so map the
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
        // Compute floor(log_base(bigint)) by repeated division.
        let mut count: u64 = 0;
        let mut current: BigInt = bigint.as_ref().clone();
        while current >= *base {
            current /= base.as_ref();
            count += 1;
        }
        Ok(Arc::new(BigInt::from(count)))
    }

    fn parse(text: &str) -> Result<Arc<BigInt>, VmRustFnError> {
        // Accept an optional leading sign followed by ASCII digits, matching the
        // documented behaviour: no whitespace, no underscores, no other formats.
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

    fn random(lower: Arc<BigInt>, upper: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        if lower >= upper {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "bigint.random: lower ({lower}) must be less than upper ({upper}); range is empty"
                ),
            }
            .into());
        }

        // range = upper - lower, always positive.
        let range: BigUint = (upper.as_ref() - lower.as_ref())
            .to_biguint()
            .expect("range is always positive because lower < upper");

        // Number of bytes needed to represent `range`.
        let range_bytes = range.to_bytes_be();
        let byte_len = range_bytes.len();

        // Rejection sampling: generate `byte_len` random bytes, interpret as a
        // big-endian unsigned integer, accept if < range. This gives a uniform
        // distribution with no modular bias.
        let mut buf = vec![0u8; byte_len];
        loop {
            getrandom::getrandom(&mut buf).map_err(|e| VmPanic::HostUnavailable {
                resource: "entropy".to_string(),
                message: format!("getrandom failed in bigint.random: {e}"),
            })?;

            let sample = BigUint::from_bytes_be(&buf);
            if sample < range {
                let result = BigInt::from(sample) + lower.as_ref();
                return Ok(Arc::new(result));
            }
        }
    }
}

/// Build a [`VmRustFnError::Panic`] carrying [`VmPanic::AllocFailure`] with
/// the given message. Centralises the wrapping so call sites stay readable.
pub(crate) fn alloc_failure_panic(message: String) -> VmRustFnError {
    VmRustFnError::Panic(VmPanic::AllocFailure { message })
}
