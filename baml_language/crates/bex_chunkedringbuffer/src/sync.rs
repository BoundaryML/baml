pub(crate) use std::sync::TryLockError;
#[cfg(not(baml_loom))]
pub(crate) use std::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
};

#[cfg(baml_loom)]
pub(crate) use loom::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
};

#[inline]
pub(crate) fn spin() {
    #[cfg(not(baml_loom))]
    std::hint::spin_loop();
    // Only the model yields so Loom can explore the other participant.
    #[cfg(baml_loom)]
    thread::yield_now();
}

/// A producer's wait for storage to come back. How long it has waited is
/// held against a threshold the caller reads afresh at every probe, so the
/// wait ends as soon as the threshold falls to meet it. The first probes
/// spin, for a recycle that is moments away; then the thread yields; then
/// it sleeps, so a long wait costs no CPU.
///
/// Loom has no clock: under the model a wait is past any threshold after
/// [`btel_settings::overage::MODEL_PROBES`] probes, so both outcomes are
/// explored without exhausting the model's budget.
#[derive(Debug)]
pub struct Wait {
    #[cfg(not(baml_loom))]
    started: web_time::Instant,
    probes: usize,
}

impl Wait {
    #[inline]
    pub fn begin() -> Self {
        Self {
            #[cfg(not(baml_loom))]
            started: web_time::Instant::now(),
            probes: 0,
        }
    }

    /// Whether the wait so far reaches `threshold`.
    #[inline]
    pub fn past(&self, threshold: std::time::Duration) -> bool {
        #[cfg(not(baml_loom))]
        {
            self.started.elapsed() >= threshold
        }
        #[cfg(baml_loom)]
        {
            let _ = threshold;
            self.probes >= btel_settings::overage::MODEL_PROBES
        }
    }

    /// One more probe found nothing: pause before the next.
    #[inline]
    pub fn pause(&mut self) {
        let probe = self.probes;
        self.probes = probe.saturating_add(1);
        #[cfg(all(not(baml_loom), not(target_arch = "wasm32")))]
        {
            if probe < btel_settings::overage::SPIN_PROBES {
                std::hint::spin_loop();
            } else if probe < btel_settings::overage::YIELD_PROBES {
                thread::yield_now();
            } else {
                thread::sleep(btel_settings::overage::SLEEP_PROBE_DURATION);
            }
        }
        // There is no other thread to yield to, and sleeping is an error.
        #[cfg(all(not(baml_loom), target_arch = "wasm32"))]
        std::hint::spin_loop();
        #[cfg(baml_loom)]
        thread::yield_now();
    }
}

pub(crate) use btel_settings::layout::Padded;
