//! Seeded generators belong to the loop; entropy requests carry owned bytes.
use crate::{
    R,
    io::{Failure, Output},
    opaque::Opaque,
};
use rand_core::SeedableRng;
use std::cell::RefCell;

enum Generator {
    Xoshiro(rand_xoshiro::Xoshiro256PlusPlus),
    ChaCha(rand_chacha::ChaCha20Rng),
}
type State = RefCell<Generator>;

pub fn new(seed: usize, chacha: bool) -> R<usize> {
    let seed = bex_vm_types::random::seed_array(&unsafe { crate::native::bytes_owned(seed) })
        .map_err(crate::scalars::error)?;
    let generator = if chacha {
        Generator::ChaCha(rand_chacha::ChaCha20Rng::from_seed(seed))
    } else {
        Generator::Xoshiro(rand_xoshiro::Xoshiro256PlusPlus::from_seed(seed))
    };
    Opaque::new(17, RefCell::new(generator)).map(Opaque::as_raw)
}
/// # Safety
/// `state` is a live seeded generator's opaque state in the current instance.
pub unsafe fn random(state: usize, bytes: i64) -> R<usize> {
    let value = unsafe { Opaque::<State>::from_raw(state) }
        .with(|state| match &mut *state.borrow_mut() {
            Generator::Xoshiro(r) => bex_vm_types::random::fill(r, bytes),
            Generator::ChaCha(r) => bex_vm_types::random::fill(r, bytes),
        })
        .map_err(crate::scalars::error)?;
    crate::native::bytes(value)
}
/// # Safety
/// `state` is a live seeded generator's opaque state in the current instance.
pub unsafe fn random_int(state: usize) -> i64 {
    unsafe { Opaque::<State>::from_raw(state) }.with(|state| match &mut *state.borrow_mut() {
        Generator::Xoshiro(r) => bex_vm_types::random::next_i63(r),
        Generator::ChaCha(r) => bex_vm_types::random::next_i63(r),
    })
}
fn failure(e: bex_vm_types::errors::VmRustFnError) -> Failure {
    use bex_vm_types::errors::{VmPanic, VmRustFnError};
    let (class, message) = match e {
        VmRustFnError::Panic(VmPanic::UserPanic { message }) => ("UserPanic", message),
        VmRustFnError::Panic(VmPanic::AllocFailure { message }) => ("AllocFailure", message),
        e => ("InternalError", e.to_string()),
    };
    Failure {
        class,
        message,
        resource: String::new(),
    }
}
fn entropy(bytes: &mut [u8], method: &str) -> Result<(), Failure> {
    getrandom::getrandom(bytes).map_err(|e| Failure {
        class: "HostUnavailable",
        resource: "randomness".into(),
        message: format!("SystemRandom.{method}: system entropy unavailable: {e}"),
    })
}
pub fn system_random(count: i64) -> Output {
    Output::Ready((|| {
        let mut bytes = bex_vm_types::random::buffer(count).map_err(failure)?;
        entropy(&mut bytes, "random")?;
        Ok(serde_json::Value::from(bytes))
    })())
}
pub fn system_random_int() -> Output {
    Output::Ready((|| {
        let mut bytes = [0u8; 8];
        entropy(&mut bytes, "random_int")?;
        Ok(serde_json::Value::from(i64::from_le_bytes(bytes) >> 1))
    })())
}
