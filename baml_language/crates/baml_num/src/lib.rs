//! The numbers of the BAML language, independent of any backend.
//!
//! `int` is a 63-bit two's-complement integer ([`Int63`]) with checked
//! arithmetic, shifts and bit operations. `baml_type` re-exports it, so the
//! compiler and the VM see the same type they always did. The crate has no
//! dependencies (serde is optional), so code that needs only the integer, such
//! as a BAML program compiled to native Rust, links it without `baml_type` and
//! everything underneath.

mod int;

pub use int::{Int63, IntShiftError};

/// Upper bound on the bit-length of a `bigint` value we are willing to
/// materialize at runtime. ~268 million bits ≈ 80 million decimal digits ≈ 32
/// MiB of digits. Operations that would produce a larger result raise
/// `baml.panics.AllocFailure` instead of either succeeding (and starving the
/// rest of the runtime) or aborting the process outright.
///
/// Shared by the VM's allocation guard (`bex_vm::package_baml::bigint`),
/// the FFI decoder's pre-allocation cap
/// (`bridge_ctypes::value_decode::MAX_BIGINT_HEX_LEN`), and TIR's
/// constant-folding refusal threshold.
pub const MAX_BIGINT_BITS: u64 = 1 << 28;

/// Permissive upper bound on the number of base-ten digits a `bigint` may
/// have before it cannot possibly fit in [`MAX_BIGINT_BITS`].
///
/// Each base-ten digit carries `log2(10) ≈ 3.32` bits, so any decimal string
/// longer than `MAX_BIGINT_BITS / 3 + 2` is guaranteed to overflow the cap.
/// Used as a cheap pre-flight reject before `BigInt::parse_bytes`; callers
/// follow up with an exact `bits()` check for borderline inputs. Shared by
/// SAP deserialization, the jsonish number visitor, and `bigint.parse`.
#[allow(clippy::cast_possible_truncation)] // MAX_BIGINT_BITS is 2^28; fits in usize on 32/64-bit
pub const MAX_BIGINT_DECIMAL_DIGITS: usize = (MAX_BIGINT_BITS / 3 + 2) as usize;
