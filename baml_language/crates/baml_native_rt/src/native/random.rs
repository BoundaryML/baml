//! `baml.random`: the two seeded generators, each of which owns its state
//! as Rust state, and the system's own.
//!
//! The generators are the crates the interpreter uses, so a seed gives the
//! same draws here. A draw changes the state, and two tasks may hold one
//! generator, so the state is behind a lock.

use std::sync::{Arc, Mutex, PoisonError};

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

use super::{
    Bytes, Elem, NativeBamlRandomChaCha20, NativeBamlRandomRngForChaCha20,
    NativeBamlRandomRngForSystemRandom, NativeBamlRandomRngForXoshiro256PlusPlus,
    NativeBamlRandomXoshiro256PlusPlus, Natives, Obj, ObjRef, Opaque, Root, Scope, Thrown,
};
use crate::layout::{NAMED_RANDOM_CHACHA20, NAMED_RANDOM_XOSHIRO};

/// How many bytes of a seed a generator reads. What a seed has past them
/// is not read.
const SEED: usize = 32;

/// A generator of the class at `named`, seeded with the first bytes of
/// `seed`.
fn seeded<'s, R>(
    scope: &mut Scope<'s>,
    named: usize,
    seed: Root<'s, Bytes>,
) -> Result<Root<'s, Obj>, Thrown<'s>>
where
    R: SeedableRng<Seed = [u8; SEED]> + Send + 'static,
{
    let given = seed.get(scope).len();
    if given < SEED {
        return Err(scope.user_panic(&format!(
            "Rng seed must be at least {SEED} bytes, got {given}"
        )));
    }
    let mut bytes = [0; SEED];
    seed.get(scope).copy_to(0, &mut bytes);
    let state = scope.alloc_owned(Arc::new(Mutex::new(R::from_seed(bytes))));
    let class = scope.named_class(named);
    let mut generator = scope.build(class);
    generator.set(scope, 0, state);
    Ok(generator.finish())
}

/// How many bytes `Rng.random` was asked for, when that is a count.
fn byte_count<'s>(scope: &mut Scope<'s>, bytes: i64) -> Result<usize, Thrown<'s>> {
    usize::try_from(bytes).map_err(|_| {
        scope.user_panic(&format!(
            "Rng.random: byte count must be non-negative, got {bytes}"
        ))
    })
}

/// Room for `count` bytes, or the panic of a count no allocation meets.
fn buffer<'s>(scope: &mut Scope<'s>, count: usize) -> Result<Vec<u8>, Thrown<'s>> {
    let mut bytes = Vec::new();
    if bytes.try_reserve(count).is_err() {
        return Err(scope.alloc_failure(&format!("Rng.random: allocation of {count} bytes failed")));
    }
    bytes.resize(count, 0);
    Ok(bytes)
}

/// The next `bytes` bytes of `generator`, whose state is an `R`.
fn drawn<'s, R: RngCore + Send + 'static>(
    scope: &mut Scope<'s>,
    generator: Root<'s, Obj>,
    bytes: i64,
) -> Result<Root<'s, Bytes>, Thrown<'s>> {
    let count = byte_count(scope, bytes)?;
    let mut drawn = buffer(scope, count)?;
    // SAFETY: a generator has one field, its state.
    unsafe { generator.get(scope).field::<Opaque>(0) }
        .owned::<Mutex<R>>()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .fill_bytes(&mut drawn);
    Ok(scope.alloc_bytes(&drawn))
}

/// The next `int` of `generator`, whose state is an `R`: all 64 bits it
/// draws, every `int` equally likely.
fn drawn_int<R: RngCore + Send + 'static>(generator: ObjRef<'_>) -> i64 {
    // SAFETY: a generator has one field, its state.
    let draw = unsafe { generator.field::<Opaque>(0) }
        .owned::<Mutex<R>>()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .next_u64();
    draw.cast_signed()
}

impl NativeBamlRandomXoshiro256PlusPlus for Natives {
    fn _new<'s>(scope: &mut Scope<'s>, seed: Root<'s, Bytes>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        seeded::<Xoshiro256PlusPlus>(scope, NAMED_RANDOM_XOSHIRO, seed)
    }
}

impl NativeBamlRandomRngForXoshiro256PlusPlus for Natives {
    fn random<'s>(
        scope: &mut Scope<'s>,
        xoshiro256plusplus: Root<'s, Obj>,
        bytes: i64,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        drawn::<Xoshiro256PlusPlus>(scope, xoshiro256plusplus, bytes)
    }

    fn random_int(xoshiro256plusplus: ObjRef<'_>) -> i64 {
        drawn_int::<Xoshiro256PlusPlus>(xoshiro256plusplus)
    }
}

impl NativeBamlRandomChaCha20 for Natives {
    fn _new<'s>(scope: &mut Scope<'s>, seed: Root<'s, Bytes>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        seeded::<ChaCha20Rng>(scope, NAMED_RANDOM_CHACHA20, seed)
    }
}

impl NativeBamlRandomRngForChaCha20 for Natives {
    fn random<'s>(
        scope: &mut Scope<'s>,
        chacha20: Root<'s, Obj>,
        bytes: i64,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        drawn::<ChaCha20Rng>(scope, chacha20, bytes)
    }

    fn random_int(chacha20: ObjRef<'_>) -> i64 {
        drawn_int::<ChaCha20Rng>(chacha20)
    }
}

/// A system without entropy has nothing a program can go on with.
fn entropy(bytes: &mut [u8]) {
    if let Err(error) = getrandom::getrandom(bytes) {
        panic!("SystemRandom: system entropy unavailable: {error}");
    }
}

impl NativeBamlRandomRngForSystemRandom for Natives {
    /// Listed as `async` for what it throws, which its declaration does
    /// not say: a count below zero, and one no allocation meets. It never
    /// stops.
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "listed as `async` to be able to throw"
    )]
    async fn random<'s>(
        scope: &mut Scope<'s>,
        _systemrandom: Elem<'s>,
        bytes: i64,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let count = byte_count(scope, bytes)?;
        let mut drawn = buffer(scope, count)?;
        entropy(&mut drawn);
        Ok(scope.alloc_bytes(&drawn))
    }

    fn random_int(_systemrandom: usize) -> i64 {
        let mut drawn = [0; 8];
        entropy(&mut drawn);
        i64::from_le_bytes(drawn)
    }
}
