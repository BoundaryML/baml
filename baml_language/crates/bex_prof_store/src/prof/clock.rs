//! The profiling clock: raw hardware ticks on the producer side, converted
//! to nanoseconds by the consumer at transcode time.
//!
//! Producers stamp records with [`now_ticks`] — a raw counter read (`rdtsc`
//! on `x86_64`, `CNTVCT_EL0` on aarch64, a monotonic-`Instant` fallback
//! elsewhere or when the TSC is not invariant). No tick→ns conversion, no
//! calibration, and no pre-main ctor runs on any producer path; the one-time
//! anchor capture is two clock reads plus (on x86) a `cpuid` probe.
//!
//! The consumer owns conversion via [`TickConverter`]: exact rational rates
//! where the platform states them (`CNTFRQ_EL0` for the aarch64 generic
//! timer), and a two-point calibration refined across its own wake cadence
//! for the x86 TSC. Disk timestamps (`timestamp_ns`) therefore stay
//! nanoseconds —
//! readers rebase to wall time as
//! `wall(event) = started_at_epoch_ns + timestamp_ns`, unchanged.
//!
//! Tick values are absolute counter reads (since boot for TSC/CNTVCT, since
//! the process zero point for the `Instant` fallback); the converter
//! subtracts the process base. The per-call-pair budget (~25 ns) assumes
//! ≤10 ns per read — `benches/prof_clock.rs` measures it.
//!
//! On wasm32 this module uses `web_time::Instant` as a monotonic nanosecond
//! source. Under miri, which cannot execute `rdtsc`/`mrs`, it remains a
//! stub and the concurrency tests stamp their own tick values.

// Raw counter reads (`rdtsc`, `mrs`).
#![allow(unsafe_code)]

pub use imp::{base_ticks, init, meta, now_ticks, started_at_epoch_ns};

/// Which hardware source backs [`now_ticks`]. Stored in run metadata for
/// diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockKind {
    /// `x86_64` invariant TSC (`rdtsc`); rate calibrated by the consumer.
    Tsc,
    /// aarch64 generic timer (`CNTVCT_EL0`); exact architectural rate.
    Cntvct,
    /// `std::time::Instant` fallback; ticks are nanoseconds, rate (1, 1).
    Instant,
    /// miri stub; ticks are whatever the test stamped.
    Stub,
}

/// Static facts about the active clock source.
#[derive(Debug, Clone, Copy)]
pub struct ClockMeta {
    pub kind: ClockKind,
    /// Exact ns-per-tick as a `(numer, denom)` rational, when the platform
    /// states it (`CNTFRQ_EL0` on aarch64, ns-backed `Instant` fallback).
    /// `None` means the rate must be measured — x86 TSC, where the consumer
    /// calibrates against the OS clock.
    pub ns_per_tick: Option<(u64, u64)>,
}

#[cfg(not(any(target_arch = "wasm32", miri)))]
mod imp {
    use std::sync::OnceLock;

    use super::{ClockKind, ClockMeta};

    enum Source {
        #[cfg(target_arch = "x86_64")]
        Tsc,
        #[cfg(target_arch = "aarch64")]
        Cntvct,
        Instant,
    }

    struct Anchor {
        source: Source,
        /// First raw read of the chosen source; the converter's base.
        base_ticks: u64,
        /// Wall-clock time of the zero point (adjacent to `base_ticks`).
        base_unix_ns: u128,
        /// Zero point for the `Instant` fallback's tick domain.
        zero_instant: std::time::Instant,
        meta: ClockMeta,
    }

    static ANCHOR: OnceLock<Anchor> = OnceLock::new();

    /// The process clock anchor. Fast path is a single `OnceLock::get`
    /// (Acquire load + branch) that inlines into `now_ticks` — and thus
    /// into the VM's call hot path — so the raw counter read can pipeline
    /// with the surrounding encode/push work rather than hiding behind a
    /// cross-crate call. The one-time init is `#[cold]` and out-of-line.
    #[inline]
    fn anchor() -> &'static Anchor {
        match ANCHOR.get() {
            Some(a) => a,
            None => anchor_init(),
        }
    }

    #[cold]
    fn anchor_init() -> &'static Anchor {
        ANCHOR.get_or_init(|| {
            let (source, ns_per_tick) = detect();
            let kind = match source {
                #[cfg(target_arch = "x86_64")]
                Source::Tsc => ClockKind::Tsc,
                #[cfg(target_arch = "aarch64")]
                Source::Cntvct => ClockKind::Cntvct,
                Source::Instant => ClockKind::Instant,
            };
            let zero_instant = std::time::Instant::now();
            let base_unix_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let mut anchor = Anchor {
                source,
                base_ticks: 0,
                base_unix_ns,
                zero_instant,
                meta: ClockMeta { kind, ns_per_tick },
            };
            anchor.base_ticks = raw_ticks(&anchor);
            anchor
        })
    }

    /// Pick the tick source for this process (and its exact rate, when the
    /// platform states one). Runs once, inside the anchor init.
    fn detect() -> (Source, Option<(u64, u64)>) {
        #[cfg(target_arch = "x86_64")]
        {
            if tsc_is_trustworthy() {
                return (Source::Tsc, None);
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if let Some(rate) = cntvct_ns_per_tick() {
                return (Source::Cntvct, Some(rate));
            }
        }
        (Source::Instant, Some((1, 1)))
    }

    /// Whether the TSC is safe to use for cross-thread-comparable stamps
    /// (one logical thread's records are stamped from several OS threads,
    /// so the per-thread sort contract needs cross-core comparability).
    ///
    /// CPUID.80000007H:EDX[8] only guarantees constant-rate / never-stop —
    /// NOT cross-core/cross-socket synchronization. On Linux we therefore
    /// also defer to the kernel, which measures TSC sync across CPUs at
    /// boot and only offers `tsc` as a clocksource when it holds (the same
    /// check minstant used). Elsewhere we require the invariant bit AND the
    /// absence of a hypervisor: under virtualization the invariant bit is
    /// still set, but a vCPU can migrate between physical cores whose TSCs
    /// are not in lockstep, so `rdtsc` reads backwards across a thread
    /// hand-off (observed on virtualized Windows CI). Falling back to
    /// `Instant` there is correct, not just safe: Windows `Instant` is
    /// `QueryPerformanceCounter`, which the OS keeps system-wide monotonic.
    #[cfg(target_arch = "x86_64")]
    fn tsc_is_trustworthy() -> bool {
        if !has_invariant_tsc() {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string(
                "/sys/devices/system/clocksource/clocksource0/available_clocksource",
            )
            .map(|s| s.contains("tsc"))
            .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            !hypervisor_present()
        }
    }

    /// Whether a hypervisor is present: `CPUID.01H:ECX[31]`, the "hypervisor
    /// present" bit that is reserved-zero on bare metal and set by every
    /// mainstream VMM. Used to reject the TSC for cross-thread stamps under
    /// virtualization, where invariant-TSC no longer implies cross-vCPU
    /// synchronization. Conservative by design: a guest whose hypervisor
    /// *does* synchronize the TSC still falls back to `Instant`, trading a
    /// little per-stamp cost for guaranteed cross-thread monotonicity.
    #[cfg(all(target_arch = "x86_64", not(target_os = "linux")))]
    // `__cpuid` is an unsafe fn on the pinned stable toolchain but safe on
    // newer ones — keep the block, silence the newer toolchains' lint.
    #[allow(unused_unsafe)]
    fn hypervisor_present() -> bool {
        // SAFETY: cpuid leaf 1 is unprivileged and universally available.
        unsafe { core::arch::x86_64::__cpuid(1).ecx & (1 << 31) != 0 }
    }

    /// Invariant-TSC probe: CPUID.80000007H:EDX[8] (constant rate, does not
    /// stop in deep C-states).
    #[cfg(target_arch = "x86_64")]
    // `__cpuid` is an unsafe fn on the pinned stable toolchain but safe on
    // newer ones — keep the block, silence the newer toolchains' lint.
    #[allow(unused_unsafe)]
    fn has_invariant_tsc() -> bool {
        // SAFETY: cpuid is unprivileged and universally available on x86_64.
        unsafe {
            let max_ext = core::arch::x86_64::__cpuid(0x8000_0000).eax;
            if max_ext < 0x8000_0007 {
                return false;
            }
            core::arch::x86_64::__cpuid(0x8000_0007).edx & (1 << 8) != 0
        }
    }

    /// Exact ns-per-tick for the aarch64 generic timer, or `None` if it
    /// cannot be determined (falls back to `Instant`).
    ///
    /// The rate must describe the counter we actually stamp with —
    /// `CNTVCT_EL0` — so we read its architectural frequency from
    /// `CNTFRQ_EL0` on every OS, macOS included. We deliberately do **not**
    /// use `mach_timebase_info`: that states the rate of `mach_absolute_time`,
    /// which is only incidentally equal to the `CNTVCT_EL0` rate on M1–M3
    /// (both 24 MHz). On M4/M5 `CNTVCT_EL0` runs at 1 GHz while
    /// `mach_absolute_time` stays at 24 MHz, so applying the mach rational
    /// (125/3) to raw `CNTVCT_EL0` reads inflates every timestamp ~41.7×.
    #[cfg(target_arch = "aarch64")]
    fn cntvct_ns_per_tick() -> Option<(u64, u64)> {
        // CNTFRQ_EL0 states the CNTVCT_EL0 frequency in Hz.
        let freq: u64;
        // SAFETY: EL0-readable system register; no memory or flags.
        unsafe {
            core::arch::asm!(
                "mrs {f}, cntfrq_el0",
                f = out(reg) freq,
                options(nomem, nostack, preserves_flags)
            );
        }
        if freq == 0 {
            return None;
        }
        Some((1_000_000_000, freq))
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn cntvct() -> u64 {
        let ticks: u64;
        // SAFETY: CNTVCT_EL0 is EL0-readable on every supported OS; the
        // read touches no memory and preserves flags. No `isb` barrier: a
        // few ns of speculation skew is irrelevant for span timestamps and
        // the barrier would cost more than the read.
        unsafe {
            core::arch::asm!(
                "mrs {t}, cntvct_el0",
                t = out(reg) ticks,
                options(nomem, nostack, preserves_flags)
            );
        }
        ticks
    }

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "u128 nanos since process start; wraps after ~584 years"
    )]
    fn raw_ticks(a: &Anchor) -> u64 {
        match a.source {
            #[cfg(target_arch = "x86_64")]
            // SAFETY: rdtsc is unprivileged; gated on invariant-TSC detect.
            Source::Tsc => unsafe { core::arch::x86_64::_rdtsc() },
            #[cfg(target_arch = "aarch64")]
            Source::Cntvct => cntvct(),
            Source::Instant => a.zero_instant.elapsed().as_nanos() as u64,
        }
    }

    /// Pins the anchor (source detection + zero point). Cheap: two clock
    /// reads and, on x86, a `cpuid` probe — there is no calibration here.
    ///
    /// Called from ring acquisition; `now_ticks` also forces it, so every
    /// stamped tick is `>= base_ticks` regardless of call order.
    pub fn init() {
        let _ = anchor();
    }

    /// Raw ticks of the active source. Absolute counter reads (since boot
    /// for TSC/CNTVCT); the consumer's [`super::TickConverter`] subtracts
    /// the process base and applies the rate.
    #[inline]
    pub fn now_ticks() -> u64 {
        // Forcing the anchor first guarantees ticks >= base_ticks for every
        // value that ever reaches a record (engine stamps can happen before
        // ring acquisition, so init-on-acquire alone is not enough).
        let a = anchor();
        raw_ticks(a)
    }

    /// First raw tick of this process — the converter's zero point.
    pub fn base_ticks() -> u64 {
        anchor().base_ticks
    }

    /// Static facts about the active source (kind + exact rate if known).
    pub fn meta() -> ClockMeta {
        anchor().meta
    }

    /// Wall-clock time of the zero point, in nanoseconds since the Unix
    /// epoch. Stored in run metadata.
    pub fn started_at_epoch_ns() -> u128 {
        anchor().base_unix_ns
    }
}

// wasm32: use web_time's browser/worker-safe Instant as a monotonic
// nanosecond source for target-neutral direct-consumer timing.
#[cfg(target_arch = "wasm32")]
mod imp {
    use std::sync::OnceLock;

    use super::{ClockKind, ClockMeta};

    struct Anchor {
        zero_instant: web_time::Instant,
        base_unix_ns: u128,
    }

    static ANCHOR: OnceLock<Anchor> = OnceLock::new();

    fn anchor() -> &'static Anchor {
        ANCHOR.get_or_init(|| Anchor {
            zero_instant: web_time::Instant::now(),
            base_unix_ns: web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        })
    }

    pub fn init() {
        let _ = anchor();
    }

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "ns since wasm module start; wraps after ~584 years"
    )]
    pub fn now_ticks() -> u64 {
        anchor().zero_instant.elapsed().as_nanos() as u64
    }

    pub fn base_ticks() -> u64 {
        0
    }

    pub fn meta() -> ClockMeta {
        ClockMeta {
            kind: ClockKind::Instant,
            ns_per_tick: Some((1, 1)),
        }
    }

    pub fn started_at_epoch_ns() -> u128 {
        anchor().base_unix_ns
    }
}

// miri: rdtsc and `mrs` are intrinsics/asm miri cannot execute, and the
// concurrency tests miri runs stamp their own tick values.
#[cfg(all(miri, not(target_arch = "wasm32")))]
mod imp {
    use super::{ClockKind, ClockMeta};

    /// No-op in miri stub builds.
    pub fn init() {}

    /// Always `0` in stub builds (nothing reads this).
    #[inline]
    pub fn now_ticks() -> u64 {
        0
    }

    /// Always `0` in stub builds.
    pub fn base_ticks() -> u64 {
        0
    }

    /// Stub meta: tick == ns so any converter built from it is an identity.
    pub fn meta() -> ClockMeta {
        ClockMeta {
            kind: ClockKind::Stub,
            ns_per_tick: Some((1, 1)),
        }
    }

    /// Always `0` in stub builds (nothing reads this).
    pub fn started_at_epoch_ns() -> u128 {
        0
    }
}

/// How trustworthy the tick→ns rate is. Stored in run metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockQuality {
    /// Platform-stated rational rate (CNTVCT, Instant, Stub).
    Exact,
    /// x86 TSC rate refined over a ≥1 s window on the consumer thread.
    Calibrated,
    /// x86 TSC rate from the initial ~2 ms two-point estimate only
    /// (~0.1% rate error; affects at most the first second of events).
    Coarse,
}

/// Consumer-side tick→ns conversion.
///
/// Built on the consumer thread ([`TickConverter::from_clock`]); producers
/// never touch it. Conversion is piecewise-linear: when the x86 TSC rate is
/// refined, the new segment starts at the previous segment's converted
/// value for the switch tick, so per-thread `timestamp_ns` stays monotone
/// across the swap, and ticks older than the switch keep converting through
/// the previous segment (in-flight ring events drain up to seconds late).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct TickConverter {
    kind: ClockKind,
    seg_base_ticks: u64,
    seg_base_ns: u64,
    /// ns-per-tick as (numer, denom).
    rate: (u64, u64),
    /// Fixed-point reciprocal of `rate` for the hot path: `numer << SCALE_SHIFT
    /// / denom`, so `to_ns` is a `dt * scale >> SCALE_SHIFT` multiply-shift
    /// instead of a per-record u128 divide (~20-40 non-pipelined cycles). Kept
    /// in sync with `rate` at every install site. Exact for rate (1,1)
    /// (scale = `2^SCALE_SHIFT` → ns == dt); ≤1 ns rounding for measured TSC
    /// rates, which is below tracing-timestamp resolution and preserves
    /// monotonicity.
    scale: u128,
    /// Previous segment `(base_ticks, base_ns, rate)`, kept after a
    /// refinement so pre-switch ticks still convert on their original line.
    prev: Option<(u64, u64, (u64, u64))>,
    refined: bool,
    /// First calibration sample (x86 TSC only): adjacent OS-clock + tick
    /// reads taken at converter construction.
    sample0: Option<(std::time::Instant, u64)>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct TickConverter;

/// Fixed-point precision of [`TickConverter::scale`]. 2^48 keeps the rounding
/// error below 1 ns for any realistic `dt` (ticks since the segment base) while
/// leaving headroom against u128 overflow in `dt * scale`.
#[cfg(not(target_arch = "wasm32"))]
const SCALE_SHIFT: u32 = 48;

/// `numer << SCALE_SHIFT / denom` — the reciprocal computed once per rate
/// install (the only divide), consumed branchlessly per record.
#[cfg(not(target_arch = "wasm32"))]
fn scale_of(rate: (u64, u64)) -> u128 {
    (u128::from(rate.0) << SCALE_SHIFT) / u128::from(rate.1.max(1))
}

#[cfg(not(target_arch = "wasm32"))]
impl TickConverter {
    /// Identity conversion (tests): synthetic tick values pass through.
    pub fn identity() -> Self {
        TickConverter {
            kind: ClockKind::Instant,
            seg_base_ticks: 0,
            seg_base_ns: 0,
            rate: (1, 1),
            scale: scale_of((1, 1)),
            prev: None,
            refined: true,
            sample0: None,
        }
    }

    /// Build from the process clock. Call on the consumer thread only: for
    /// the x86 TSC this blocks ~2 ms taking a coarse two-point rate
    /// estimate (producers only buffer meanwhile); exact-rate sources
    /// return immediately.
    pub fn from_clock() -> Self {
        let m = meta();
        match m.ns_per_tick {
            Some(rate) => TickConverter {
                kind: m.kind,
                seg_base_ticks: base_ticks(),
                seg_base_ns: 0,
                rate,
                scale: scale_of(rate),
                prev: None,
                refined: true,
                sample0: None,
            },
            None => {
                // x86 TSC: two-point coarse estimate over ~2 ms.
                let sample0 = (std::time::Instant::now(), now_ticks());
                std::thread::sleep(std::time::Duration::from_millis(2));
                let elapsed_ns = sample0.0.elapsed().as_nanos();
                let tick_delta = now_ticks().wrapping_sub(sample0.1).max(1);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "~2ms window; fits u64 by construction"
                )]
                let rate = (elapsed_ns as u64, tick_delta);
                TickConverter {
                    kind: m.kind,
                    seg_base_ticks: base_ticks(),
                    seg_base_ns: 0,
                    rate,
                    scale: scale_of(rate),
                    prev: None,
                    refined: false,
                    sample0: Some(sample0),
                }
            }
        }
    }

    /// Convert a raw tick stamp to nanoseconds since the process zero point
    /// (the domain `started_at_epoch_ns` rebases to wall time).
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "ns since process start; wraps after ~584 years"
    )]
    pub fn to_ns(&self, ticks: u64) -> u64 {
        if ticks < self.seg_base_ticks {
            if let Some((base_ticks, base_ns, (numer, denom))) = self.prev {
                let dt = u128::from(ticks.saturating_sub(base_ticks));
                return base_ns + (dt * u128::from(numer) / u128::from(denom)) as u64;
            }
        }
        let dt = u128::from(ticks.saturating_sub(self.seg_base_ticks));
        // Hot path: multiply-shift by the precomputed reciprocal, no divide.
        self.seg_base_ns + ((dt * self.scale) >> SCALE_SHIFT) as u64
    }

    /// Refine the x86 TSC rate once the window since `sample0` spans ≥1 s.
    /// Installs the new rate with anchor continuity (see type docs). Cheap
    /// no-op for exact-rate sources or once refined; call at heartbeat
    /// cadence.
    pub fn maybe_refine(&mut self) {
        if self.refined {
            return;
        }
        let Some((i0, t0)) = self.sample0 else {
            self.refined = true;
            return;
        };
        let window_ns = i0.elapsed().as_nanos();
        if window_ns < 1_000_000_000 {
            return;
        }
        let now_t = now_ticks();
        let tick_delta = now_t.wrapping_sub(t0).max(1);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "refine window is seconds; fits u64"
        )]
        let new_rate = (window_ns as u64, tick_delta);
        let switch_ns = self.to_ns(now_t);
        self.prev = Some((self.seg_base_ticks, self.seg_base_ns, self.rate));
        self.seg_base_ticks = now_t;
        self.seg_base_ns = switch_ns;
        self.rate = new_rate;
        self.scale = scale_of(new_rate);
        self.refined = true;
    }

    pub fn kind(&self) -> ClockKind {
        self.kind
    }

    /// ns-per-tick rational currently in effect (header diagnostics).
    pub fn rate(&self) -> (u64, u64) {
        self.rate
    }

    pub fn quality(&self) -> ClockQuality {
        match self.kind {
            ClockKind::Tsc => {
                if self.refined {
                    ClockQuality::Calibrated
                } else {
                    ClockQuality::Coarse
                }
            }
            ClockKind::Cntvct | ClockKind::Instant | ClockKind::Stub => ClockQuality::Exact,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl TickConverter {
    /// Identity conversion for the wasm substrate: `now_ticks` already returns
    /// monotonic nanoseconds from `web_time::Instant`.
    pub fn identity() -> Self {
        Self
    }

    pub fn from_clock() -> Self {
        Self
    }

    #[inline]
    pub fn to_ns(&self, ticks: u64) -> u64 {
        ticks
    }

    pub fn maybe_refine(&mut self) {}

    pub fn kind(&self) -> ClockKind {
        ClockKind::Instant
    }

    pub fn rate(&self) -> (u64, u64) {
        (1, 1)
    }

    pub fn quality(&self) -> ClockQuality {
        ClockQuality::Exact
    }
}

#[cfg(all(test, not(miri), not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn monotonic_nondecreasing() {
        init();
        let mut prev = now_ticks();
        for _ in 0..10_000 {
            let next = now_ticks();
            assert!(next >= prev, "clock went backwards: {prev} -> {next}");
            prev = next;
        }
    }

    #[test]
    fn cross_thread_ordering() {
        // A stamp taken on one
        // OS thread, handed off, then re-stamped on another thread must not
        // go backwards (invariant TSC / system-wide CNTVCT).
        init();
        let (tx, rx) = std::sync::mpsc::sync_channel::<u64>(0);
        let handle = std::thread::spawn(move || {
            for t_a in rx {
                let t_b = now_ticks();
                assert!(t_b >= t_a, "cross-thread regression: {t_a} -> {t_b}");
            }
        });
        for _ in 0..1_000 {
            if tx.send(now_ticks()).is_err() {
                break;
            }
        }
        drop(tx);
        handle.join().expect("stamping thread panicked");
    }

    #[test]
    fn zero_point_matches_wall_clock() {
        // Deliberately goes through the conversion path: with raw ticks a
        // direct arithmetic check could silently pass on garbage (a small
        // elapsed value keeps even a ~3x unit error inside tolerance).
        init();
        let conv = TickConverter::from_clock();
        let wall_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let prof_now = started_at_epoch_ns() + u128::from(conv.to_ns(now_ticks()));
        let drift = wall_now.abs_diff(prof_now);
        // Generous tolerance: coarse-rate error + scheduling between the
        // reads. Catches "anchor or rate is garbage", not clock quality.
        assert!(
            drift < 5_000_000_000,
            "prof clock and wall clock disagree by {drift} ns"
        );
    }

    #[test]
    fn converter_rate_sane() {
        // Convert a measured ~50ms window and require agreement within 10%:
        // catches a wrong rational (e.g. mach timebase numer/denom swapped)
        // on any platform, which the 5s wall tolerance above would miss.
        init();
        let conv = TickConverter::from_clock();
        let wall = std::time::Instant::now();
        let t0 = now_ticks();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let t1 = now_ticks();
        let wall_ns = u64::try_from(wall.elapsed().as_nanos()).expect("window fits u64");
        let conv_ns = conv.to_ns(t1) - conv.to_ns(t0);
        let err = conv_ns.abs_diff(wall_ns);
        assert!(
            err * 10 < wall_ns,
            "converted window {conv_ns} ns vs wall {wall_ns} ns"
        );
    }

    #[test]
    fn identity_passthrough() {
        let conv = TickConverter::identity();
        for v in [0u64, 1, 42, 10_000, u64::MAX] {
            assert_eq!(conv.to_ns(v), v);
        }
    }

    #[test]
    fn reciprocal_matches_exact_divide() {
        // The fixed-point reciprocal (multiply-shift) must track the exact
        // dt*numer/denom divide within 1 ns across a range of ticks, for
        // representative non-unit rates (CNTVCT 24 MHz = 125/3; a fast TSC
        // ~3.2 GHz ≈ 5/16). Catches a gross reciprocal/SCALE_SHIFT bug that
        // the identity-rate (1/1) test cannot.
        for &(numer, denom) in &[(125u64, 3u64), (5, 16), (1_000_000_000, 24_000_000)] {
            let conv = TickConverter {
                kind: ClockKind::Cntvct,
                seg_base_ticks: 0,
                seg_base_ns: 0,
                rate: (numer, denom),
                scale: scale_of((numer, denom)),
                prev: None,
                refined: true,
                sample0: None,
            };
            for dt in [0u64, 1, 3, 7, 1_000, 1_000_003, 50_000_000] {
                let exact = u64::try_from(u128::from(dt) * u128::from(numer) / u128::from(denom))
                    .expect("test dt*rate fits u64");
                let approx = conv.to_ns(dt);
                assert!(
                    approx.abs_diff(exact) <= 1,
                    "rate {numer}/{denom} dt {dt}: reciprocal {approx} vs exact {exact}"
                );
            }
        }
    }

    #[test]
    fn refinement_keeps_monotonicity() {
        // A deliberately-wrong coarse rate, then a refine: converted values
        // sampled across the boundary must never step backwards, and ticks
        // older than the switch keep converting through the old segment.
        let mut conv = TickConverter {
            kind: ClockKind::Tsc,
            seg_base_ticks: 0,
            seg_base_ns: 0,
            rate: (3, 1), // 3 ns/tick, deliberately ~3x off
            scale: scale_of((3, 1)),
            prev: None,
            refined: false,
            sample0: Some((std::time::Instant::now(), now_ticks())),
        };
        let before_switch = now_ticks();
        let ns_before = conv.to_ns(before_switch);
        // Force the refine path regardless of wall time elapsed.
        std::thread::sleep(std::time::Duration::from_millis(5));
        conv.sample0 = Some((
            std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(2))
                .expect("process started more than 2s after boot"),
            conv.sample0.unwrap().1,
        ));
        conv.maybe_refine();
        assert!(conv.refined, "refine did not trigger");
        // Old-tick conversion is unchanged (previous segment).
        assert_eq!(conv.to_ns(before_switch), ns_before);
        // Post-switch ticks continue from at least the switch value.
        let after = now_ticks();
        assert!(
            conv.to_ns(after) >= ns_before,
            "monotonicity broken across rate swap"
        );
    }

    #[test]
    fn pre_base_ticks_clamp() {
        init();
        let conv = TickConverter::from_clock();
        // A tick below the process base (defensive: should never be
        // emitted) clamps to the segment base instead of underflowing.
        assert_eq!(conv.to_ns(0), 0);
        assert_eq!(conv.to_ns(base_ticks()), 0);
    }
}
