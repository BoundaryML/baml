//! Slice accounting uses the system-wide architectural timer on AArch64. This
//! is the same monotonic hardware source behind Linux's vDSO clock, without its
//! wall-time conversion/seqlock. Deadline/timer APIs still use Instant.
use std::time::Duration;
#[cfg(target_arch = "aarch64")]
pub type Stamp = u64;
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
pub struct Stamp {
    ticks: u64,
    hardware: bool,
}
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub type Stamp = std::time::Instant;
#[cfg(target_arch = "x86_64")]
struct CounterClock {
    source: quanta::Clock,
    reference: quanta::Clock,
    metadata: quanta::CalibrationMetadata,
    hardware: std::cell::Cell<bool>,
    reads: std::cell::Cell<u16>,
    anchor_ticks: std::cell::Cell<u64>,
    anchor_ns: std::cell::Cell<u64>,
}
#[cfg(target_arch = "x86_64")]
impl CounterClock {
    fn new() -> Self {
        let clock = quanta::Clock::new();
        let metadata = clock.calibration_metadata();
        let hardware = metadata.source == quanta::ClockSource::Tsc
            && metadata.quality.status == quanta::CalibrationStatus::Converged;
        let reference = quanta::Clock::monotonic();
        let anchor_ns = reference.raw();
        let anchor_ticks = clock.raw();
        Self {
            source: clock,
            reference,
            metadata,
            hardware: hardware.into(),
            reads: 0.into(),
            anchor_ticks: anchor_ticks.into(),
            anchor_ns: anchor_ns.into(),
        }
    }
    fn read(&self) -> Stamp {
        if !self.hardware.get() {
            return Stamp {
                ticks: self.reference.raw(),
                hardware: false,
            };
        }
        let ticks = self.source.raw();
        let reads = self.reads.get().wrapping_add(1);
        self.reads.set(reads);
        // Validate on execution boundaries without putting an OS read on every
        // task switch. A failed/migrated counter permanently selects the OS
        // clock on this owner thread. Cross-domain slices yield conservatively.
        if reads & 1023 == 0 {
            let reference = self.reference.raw();
            let elapsed = self.source.delta_as_nanos(self.anchor_ticks.get(), ticks);
            let expected = reference.saturating_sub(self.anchor_ns.get());
            if !counter_matches_reference(ticks >= self.anchor_ticks.get(), elapsed, expected) {
                self.hardware.set(false);
                return Stamp {
                    ticks: reference,
                    hardware: false,
                };
            }
            self.anchor_ticks.set(ticks);
            self.anchor_ns.set(reference);
        }
        Stamp {
            ticks,
            hardware: true,
        }
    }
}
#[cfg(target_arch = "x86_64")]
fn counter_matches_reference(forward: bool, elapsed: u64, expected: u64) -> bool {
    forward && elapsed.abs_diff(expected) <= 100_000u64.saturating_add(expected / 1000)
}
#[cfg(target_arch = "x86_64")]
thread_local! {
    // Only cooperative slice accounting uses this clock. Language timers and
    // deadlines use Instant, and telemetry keeps its independently owned clock.
    static CLOCK: CounterClock = CounterClock::new();
}
#[derive(Clone, Copy)]
pub struct Slice {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    ticks: u64,
    #[cfg(target_arch = "x86_64")]
    nanos: u64,
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    duration: Duration,
}
impl Slice {
    pub fn new(duration: Duration) -> Self {
        #[cfg(target_arch = "aarch64")]
        {
            let frequency: u64;
            // SAFETY: Linux exposes the read-only architectural timer registers.
            unsafe {
                std::arch::asm!("mrs {f}, cntfrq_el0", f = out(reg) frequency, options(nomem, nostack, preserves_flags));
            }
            let ticks = duration
                .as_nanos()
                .saturating_mul(u128::from(frequency))
                .div_ceil(1_000_000_000)
                .min(u128::from(u64::MAX)) as u64;
            Self { ticks }
        }
        #[cfg(target_arch = "x86_64")]
        {
            let metadata = CLOCK.with(|c| c.metadata);
            let ticks = duration
                .as_nanos()
                .saturating_mul(1u128 << metadata.shift)
                .div_ceil(u128::from(metadata.multiplier))
                .min(u128::from(u64::MAX)) as u64;
            Self {
                ticks,
                nanos: duration.as_nanos().min(u128::from(u64::MAX)) as u64,
            }
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            Self { duration }
        }
    }
    pub fn expired(self, start: Stamp) -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            now().wrapping_sub(start) >= self.ticks
        }
        #[cfg(target_arch = "x86_64")]
        {
            let end = now();
            end.hardware != start.hardware
                || end.ticks.wrapping_sub(start.ticks)
                    >= if end.hardware { self.ticks } else { self.nanos }
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            start.elapsed() >= self.duration
        }
    }
}
#[inline]
pub fn now() -> Stamp {
    #[cfg(target_arch = "aarch64")]
    {
        let value: u64;
        // FEAT_ECV supplies an ordered read without the pipeline-wide ISB.
        // Cache the hardware capability on the owner thread, with no shared
        // synchronization or process-global mutable state.
        #[cfg(target_os = "linux")]
        {
            thread_local! {
                static SELF_SYNCHRONIZED: bool = unsafe { libc::getauxval(libc::AT_HWCAP2) } & (1 << 19) != 0;
            }
            if SELF_SYNCHRONIZED.with(|available| *available) {
                unsafe {
                    std::arch::asm!("mrs {v}, S3_3_C14_C0_6", v = out(reg) value, options(nostack, preserves_flags));
                }
                return value;
            }
        }
        // SAFETY: read-only monotonic architectural timer, shared across cores.
        unsafe {
            std::arch::asm!("isb", "mrs {v}, cntvct_el0", v = out(reg) value, options(nostack, preserves_flags));
        }
        value
    }
    #[cfg(target_arch = "x86_64")]
    {
        CLOCK.with(CounterClock::read)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        std::time::Instant::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn counter_validation_rejects_backward_jumps_and_rate_changes() {
        assert!(counter_matches_reference(true, 50_000_000, 50_010_000));
        assert!(!counter_matches_reference(false, 50_000_000, 50_000_000));
        assert!(!counter_matches_reference(true, 1_000_000, 50_000_000));
        assert!(!counter_matches_reference(true, 60_000_000, 50_000_000));
        let clock = CounterClock::new();
        clock.hardware.set(false);
        assert!(!clock.read().hardware);
    }
    #[test]
    fn counter_obeys_configured_wall_time_slice() {
        let start = now();
        assert!(Slice::new(Duration::ZERO).expired(start));
        assert!(!Slice::new(Duration::from_secs(60)).expired(start));
        std::thread::sleep(Duration::from_millis(5));
        assert!(Slice::new(Duration::from_millis(1)).expired(start));
    }
}
