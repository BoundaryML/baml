//! Slice accounting uses the system-wide architectural timer on AArch64. This
//! is the same monotonic hardware source behind Linux's vDSO clock, without its
//! wall-time conversion/seqlock. Deadline/timer APIs still use Instant.
use std::time::Duration;
#[cfg(target_arch = "aarch64")]
pub type Stamp = u64;
#[cfg(not(target_arch = "aarch64"))]
pub type Stamp = std::time::Instant;
#[derive(Clone, Copy)]
pub struct Slice {
    #[cfg(target_arch = "aarch64")]
    ticks: u64,
    #[cfg(not(target_arch = "aarch64"))]
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
        #[cfg(not(target_arch = "aarch64"))]
        {
            Self { duration }
        }
    }
    pub fn expired(self, start: Stamp) -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            now().wrapping_sub(start) >= self.ticks
        }
        #[cfg(not(target_arch = "aarch64"))]
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
        // SAFETY: read-only monotonic architectural timer, shared across cores.
        unsafe {
            std::arch::asm!("isb", "mrs {v}, cntvct_el0", v = out(reg) value, options(nostack, preserves_flags));
        }
        value
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        std::time::Instant::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_obeys_configured_wall_time_slice() {
        let start = now();
        assert!(Slice::new(Duration::ZERO).expired(start));
        assert!(!Slice::new(Duration::from_secs(60)).expired(start));
        std::thread::sleep(Duration::from_millis(5));
        assert!(Slice::new(Duration::from_millis(1)).expired(start));
    }
}
