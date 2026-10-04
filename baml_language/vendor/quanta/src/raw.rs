//! Raw source selection without calibration or the process-global cache.
use crate::{ClockSource, Monotonic};
#[cfg(any(all(target_arch = "x86_64", target_feature = "sse2"), all(target_arch = "aarch64", any(target_os = "linux", target_os = "android", target_os = "macos"))))]
use crate::Counter;

/// A fixed-point nanoseconds-per-tick ratio, computed on a cold path.
#[derive(Clone, Copy, Debug)]
pub struct RawScale {
    /// Fixed-point numerator.
    pub multiplier: u64,
    /// Power-of-two denominator.
    pub shift: u32,
}

impl RawScale {
    /// Convert a positive rational ratio without floating-point rounding.
    pub fn ratio(numerator: u64, denominator: u64) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        let shift = 32;
        let multiplier = (u128::from(numerator) << shift) / u128::from(denominator);
        Some(Self { multiplier: u64::try_from(multiplier).ok().filter(|m| *m != 0)?, shift })
    }
}

#[derive(Clone, Debug)]
enum Backend {
    Os(Monotonic),
    #[cfg(any(all(target_arch = "x86_64", target_feature = "sse2"), all(target_arch = "aarch64", any(target_os = "linux", target_os = "android", target_os = "macos"))))]
    Counter(Counter),
    #[cfg(target_vendor = "apple")]
    Mach,
}

/// A readable source and optional platform-reported scale. Construction never
/// waits for counter calibration; consumers can measure a missing scale later.
#[derive(Clone, Debug)]
pub struct RawClock {
    backend: Backend,
    source: ClockSource,
    scale: Option<RawScale>,
}

impl RawClock {
    /// Select a source without waiting or calibrating.
    pub fn new() -> Self {
        // CPUID reports the CPU counter's frequency. QPF only describes QPC.
        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        if crate::detection::has_counter_support() {
            let scale = raw_cpuid::CpuId::new().get_tsc_info()
                .and_then(|info| info.tsc_frequency())
                .and_then(|hz| RawScale::ratio(1_000_000_000, hz));
            return Self { backend: Backend::Counter(Counter), source: ClockSource::Tsc, scale };
        }
        // Generic timer user access is part of these native platform ABIs.
        // iOS and Windows ARM use supported OS clock interfaces instead.
        #[cfg(all(target_arch = "aarch64", any(target_os = "linux", target_os = "android", target_os = "macos")))]
        {
            let frequency: u64;
            // SAFETY: CNTFRQ/CNTVCT are readable at EL0 on these platforms.
            unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency, options(nomem, nostack, preserves_flags)); }
            if let Some(scale) = RawScale::ratio(1_000_000_000, frequency) {
                return Self { backend: Backend::Counter(Counter), source: ClockSource::SystemCounter, scale: Some(scale) };
            }
        }
        #[cfg(target_vendor = "apple")]
        {
            let mut info = mach::Timebase { numer: 0, denom: 0 };
            // SAFETY: a valid writable timebase structure.
            if unsafe { mach::mach_timebase_info(&mut info) } == 0 {
                if let Some(scale) = RawScale::ratio(u64::from(info.numer), u64::from(info.denom)) {
                    return Self { backend: Backend::Mach, source: ClockSource::MachAbsolute, scale: Some(scale) };
                }
            }
        }
        Self::monotonic()
    }

    /// Select the OS reference with its known scale.
    pub fn monotonic() -> Self {
        let clock = Monotonic::default();
        #[cfg(target_os = "windows")]
        let (source, scale) = {
            let (multiplier, shift) = clock.conversion();
            (ClockSource::PerformanceCounter, RawScale { multiplier, shift })
        };
        #[cfg(not(target_os = "windows"))]
        let (source, scale) = (ClockSource::Monotonic, RawScale { multiplier: 1, shift: 0 });
        Self { backend: Backend::Os(clock), source, scale: Some(scale) }
    }

    /// Read raw ticks; this does not inspect scale or calibration.
    #[inline(always)]
    pub fn raw(&self) -> u64 {
        match &self.backend {
            #[cfg(target_os = "windows")]
            Backend::Os(clock) => clock.raw(),
            #[cfg(not(target_os = "windows"))]
            Backend::Os(clock) => clock.now(),
            #[cfg(any(all(target_arch = "x86_64", target_feature = "sse2"), all(target_arch = "aarch64", any(target_os = "linux", target_os = "android", target_os = "macos"))))]
            Backend::Counter(counter) => counter.now(),
            #[cfg(target_vendor = "apple")]
            // SAFETY: mach_absolute_time has no preconditions.
            Backend::Mach => unsafe { mach::mach_absolute_time() },
        }
    }

    /// Identify the retained source.
    pub fn source(&self) -> ClockSource { self.source }
    /// Platform-reported scale, absent when measurement is needed.
    pub fn reported_scale(&self) -> Option<RawScale> { self.scale }
}

impl Default for RawClock {
    fn default() -> Self { Self::new() }
}

#[cfg(target_vendor = "apple")]
mod mach {
    #[repr(C)]
    pub struct Timebase { pub numer: u32, pub denom: u32 }
    unsafe extern "C" {
        pub fn mach_timebase_info(info: *mut Timebase) -> i32;
        pub fn mach_absolute_time() -> u64;
    }
}
