//! Consumer wake/park protocol (design D4).
//!
//! The consumer sets a parked flag and sleeps in `park_timeout`; the producer
//! checks the flag only on segment fill (~1 in `seg_bytes / record_len`
//! events) and calls [`std::thread::Thread::unpark`] if set. There is no
//! mutex and no condvar anywhere on the producer side: it runs holding the
//! engine's heap permit, where blocking would stall GC engine-wide.
//!
//! The flag-then-recheck dance *shrinks* but does not *close* the classic
//! lost-wakeup window. That residual race is deliberately benign: the park
//! timeout bounds its cost to one interval of buffered ring growth. Bounding
//! that race is why the timeout exists — do not "fix" the race by adding
//! producer-side blocking.

// On wasm32 there is no background consumer to wake; cooperative drains call
// the registry directly. Keep the types compiled so shared ring code stays
// dual-target without per-call-site cfgs.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

pub(crate) use imp::Wake;

#[cfg(not(baml_loom))]
mod imp {
    use std::{
        sync::{
            OnceLock,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    pub(crate) struct Wake {
        parked: AtomicBool,
        consumer: OnceLock<std::thread::Thread>,
    }

    impl Wake {
        pub(crate) const fn new() -> Self {
            Self {
                parked: AtomicBool::new(false),
                consumer: OnceLock::new(),
            }
        }

        /// The consumer thread registers itself once at startup.
        pub(crate) fn register_consumer(&self) {
            let _ = self.consumer.set(std::thread::current());
        }

        /// Producer slow path. Never blocks: a Relaxed flag load, a lock-free
        /// `OnceLock::get`, and at most an `unpark` (a wake, not a wait).
        #[inline]
        pub(crate) fn wake_if_parked(&self) {
            if self.parked.load(Ordering::Relaxed) {
                self.force_wake();
            }
        }

        /// Unconditional wake (control-message paths like flush requests,
        /// which must not depend on the parked flag's timing).
        pub(crate) fn force_wake(&self) {
            if let Some(consumer) = self.consumer.get() {
                consumer.unpark();
            }
        }

        /// Consumer: announce intent to park, *before* the final
        /// no-work recheck. `SeqCst` is cheap here (runs at most ~20×/s).
        pub(crate) fn pre_park(&self) {
            self.parked.store(true, Ordering::SeqCst);
        }

        /// Consumer: sleep until unparked or the timeout elapses. The
        /// timeout, not the flag, is the loss-of-wakeup backstop.
        #[expect(clippy::unused_self, reason = "API symmetry with pre/post_park")]
        pub(crate) fn park(&self, timeout: Duration) {
            std::thread::park_timeout(timeout);
        }

        /// Consumer: clear the parked flag after waking.
        pub(crate) fn post_park(&self) {
            self.parked.store(false, Ordering::Relaxed);
        }
    }
}

// loom cannot model `park_timeout` (timeouts have no place in exhaustive
// interleaving search), and the protocol's correctness deliberately does not
// depend on parking — the timer bounds the benign race. Loom builds get a
// flag-only stub; the real protocol is exercised by the std-thread stress
// tests.
#[cfg(baml_loom)]
mod imp {
    use std::time::Duration;

    use crate::prof::sync::{AtomicBool, Ordering};

    pub(crate) struct Wake {
        parked: AtomicBool,
    }

    impl Wake {
        pub(crate) fn new() -> Self {
            Self {
                parked: AtomicBool::new(false),
            }
        }

        pub(crate) fn register_consumer(&self) {}

        #[inline]
        pub(crate) fn wake_if_parked(&self) {
            let _ = self.parked.load(Ordering::Relaxed);
        }

        pub(crate) fn force_wake(&self) {}

        pub(crate) fn pre_park(&self) {
            self.parked.store(true, Ordering::SeqCst);
        }

        pub(crate) fn park(&self, _timeout: Duration) {
            loom::thread::yield_now();
        }

        pub(crate) fn post_park(&self) {
            self.parked.store(false, Ordering::Relaxed);
        }
    }
}
