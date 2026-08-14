use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr};
use minijinja::State;

#[derive(Debug)]
pub struct XorShiftRng {
    seed: seed_impl::Seed,
}

impl Object for XorShiftRng {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }
}

impl XorShiftRng {
    pub fn for_state(state: &State) -> Arc<XorShiftRng> {
        state.get_or_set_temp_object("minijinja-contrib-rng", || {
            XorShiftRng::new(
                state
                    .lookup("RAND_SEED")
                    .and_then(|x| u64::try_from(x).ok()),
            )
        })
    }

    pub fn new(seed: Option<u64>) -> XorShiftRng {
        XorShiftRng {
            seed: seed_impl::Seed::new(
                seed.unwrap_or_else(|| RandomState::new().build_hasher().finish()),
            ),
        }
    }

    pub fn next(&self) -> u64 {
        let mut rv = seed_impl::load(&self.seed);
        rv ^= rv << 13;
        rv ^= rv >> 7;
        rv ^= rv << 17;
        seed_impl::store(&self.seed, rv);
        rv
    }

    pub fn next_usize(&self, max: usize) -> usize {
        (self.random() * max as f64) as usize
    }

    pub fn random(&self) -> f64 {
        (self.next() as f64) / (u64::MAX as f64)
    }

    pub fn random_range(&self, lower: i64, upper: i64) -> i64 {
        (self.random() * (upper - lower) as f64) as i64 + lower
    }
}

#[cfg(target_has_atomic = "64")]
mod seed_impl {
    pub type Seed = std::sync::atomic::AtomicU64;

    pub fn load(seed: &Seed) -> u64 {
        seed.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn store(seed: &Seed, v: u64) {
        seed.store(v, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(not(target_has_atomic = "64"))]
mod seed_impl {
    pub type Seed = std::sync::Mutex<u64>;

    pub fn load(seed: &Seed) -> u64 {
        *seed.lock().unwrap()
    }

    pub fn store(seed: &Seed, v: u64) {
        *seed.lock().unwrap() = v;
    }
}
