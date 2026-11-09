//! Heap-neutral random algorithms shared by both execution backends.
use crate::errors::{VmPanic, VmRustFnError};
use rand_core::RngCore;

pub fn seed_array(seed: &[u8]) -> Result<[u8; 32], VmRustFnError> {
    let head = seed.get(..32).ok_or_else(|| VmPanic::UserPanic {
        message: format!("Rng seed must be at least 32 bytes, got {}", seed.len()),
    })?;
    Ok(head.try_into().expect("seed length checked"))
}
pub fn buffer(bytes: i64) -> Result<Vec<u8>, VmRustFnError> {
    let n = usize::try_from(bytes).map_err(|_| VmPanic::UserPanic {
        message: format!("Rng.random: byte count must be non-negative, got {bytes}"),
    })?;
    let mut buf = Vec::new();
    buf.try_reserve(n).map_err(|_| VmPanic::AllocFailure {
        message: format!("Rng.random: allocation of {n} bytes failed"),
    })?;
    buf.resize(n, 0);
    Ok(buf)
}
pub fn fill(rng: &mut impl RngCore, bytes: i64) -> Result<Vec<u8>, VmRustFnError> {
    let mut buf = buffer(bytes)?;
    rng.fill_bytes(&mut buf);
    Ok(buf)
}
pub fn next_i63(rng: &mut impl RngCore) -> i64 {
    rng.next_u64().cast_signed() >> 1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_preserves_rng_error_contract() {
        assert!(
            matches!(seed_array(&[0;31]),Err(VmRustFnError::Panic(VmPanic::UserPanic{message})) if message=="Rng seed must be at least 32 bytes, got 31")
        );
        assert_eq!(seed_array(&[7; 33]).unwrap(), [7; 32]);
        assert!(
            matches!(buffer(-1),Err(VmRustFnError::Panic(VmPanic::UserPanic{message})) if message=="Rng.random: byte count must be non-negative, got -1")
        );
        assert!(matches!(
            buffer(i64::MAX),
            Err(VmRustFnError::Panic(VmPanic::AllocFailure { .. }))
        ));
    }
}
