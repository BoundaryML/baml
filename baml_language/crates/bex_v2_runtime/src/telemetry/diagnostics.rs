//! Opt-in experiments, never enabled by generated production packs.
//! Modes: 0 reference, 1 raw counter, 2 shared returns, 3 cheaper exact paths,
//! 4 best exact (2+3; ordered clock), 5 count only, 6/7 periodic 16/64,
//! 8 interval flag at 1 kHz. Audit builds additionally read an exact reference.
use super::*;
use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn environment(name: &str, fallback: &str) -> String {
    baml_env::os_var(name)
        .map(|s| s.into_string().expect("diagnostic Unicode setting"))
        .unwrap_or_else(|| fallback.to_owned())
}
#[derive(Clone, Copy)]
struct FastCache {
    parent: u64,
    path: CallPathId,
    slot: *mut AggregateDelta,
}
impl Default for FastCache {
    fn default() -> Self {
        Self {
            parent: u64::MAX,
            path: CallPathId::ROOT,
            slot: std::ptr::null_mut(),
        }
    }
}
#[derive(Default)]
struct Audit {
    count: u64,
    ticks: u64,
    reference: u64,
    samples: u64,
    reads: u64,
    backwards: u64,
}
pub(super) struct State {
    mode: u8,
    audit_enabled: bool,
    cache: Vec<FastCache>,
    recursive: Vec<bool>,
    visits: Vec<u64>,
    samples: Vec<u64>,
    last_exit: Option<ClockInstant>,
    audit: BTreeMap<u64, Audit>,
    flag: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    timer: Option<std::thread::JoinHandle<()>>,
}
impl State {
    pub(super) fn new(sites: &[Site]) -> Self {
        let mode = environment("BAML_V2_DIAG_MODE", "0")
            .parse()
            .expect("diagnostic mode");
        assert!(mode <= 8);
        let flag = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let timer = (mode == 8).then(|| {
            let flag = flag.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                #[cfg(target_os = "linux")]
                unsafe {
                    let mut mask: libc::cpu_set_t = std::mem::zeroed();
                    libc::CPU_SET(
                        environment("BAML_V2_TIMER_CPU", "13").parse().unwrap(),
                        &mut mask,
                    );
                    assert_eq!(
                        libc::sched_setaffinity(0, std::mem::size_of_val(&mask), &mask),
                        0
                    );
                }
                let mut next = Instant::now();
                while !stop.load(Ordering::Relaxed) {
                    next += Duration::from_millis(1);
                    std::thread::sleep(next.saturating_duration_since(Instant::now()));
                    flag.store(true, Ordering::Relaxed);
                }
            })
        });
        let seed: u64 = environment("BAML_V2_DIAG_SEED", "42").parse().unwrap();
        let visits = (0..sites.len())
            .map(|i| {
                let mut x = (i as u64)
                    .wrapping_add(seed)
                    .wrapping_add(0x9e3779b97f4a7c15);
                x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
                x ^ (x >> 31)
            })
            .collect();
        Self {
            mode,
            audit_enabled: baml_env::os_var("BAML_V2_DIAG_AUDIT").is_some(),
            cache: vec![FastCache::default(); sites.len()],
            recursive: sites.iter().map(|s| s.caller == Some(s.callee)).collect(),
            visits,
            samples: vec![],
            last_exit: None,
            audit: BTreeMap::new(),
            flag,
            stop,
            timer,
        }
    }
    pub(super) fn reset(&mut self) {
        self.cache.fill(FastCache::default());
        self.samples.clear();
        self.audit.clear();
        self.last_exit = None;
        self.flag.store(false, Ordering::Relaxed);
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.timer.take() {
            t.join().unwrap();
        }
    }
}
/// Mark intervening BAML work; only compiler-proven adjacent returns may share.
#[inline(always)]
pub fn invalidate() {
    let p = CURRENT.get();
    if !p.is_null() {
        unsafe {
            (*p).diagnostic.last_exit = None;
        }
    }
}
#[inline(always)]
fn raw(s: &super::State) -> ClockInstant {
    #[cfg(target_arch = "aarch64")]
    {
        debug_assert!(s.clock.metadata().source == btel_clock::Source::SystemCounter);
        let ticks;
        unsafe {
            std::arch::asm!("mrs {}, cntvct_el0",out(reg) ticks,options(nostack,preserves_flags));
        }
        ClockInstant::from_ticks(ticks)
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        s.read()
    }
}
pub struct Frame {
    inner: super::Frame,
    sampled: bool,
    reference: ClockInstant,
    reads: u64,
}
/// # Safety
/// The session and live site requirements of `enter_observed` apply.
#[inline(always)]
pub unsafe fn enter<const M: u8, const A: bool>(site_id: usize) -> Frame {
    let p = CURRENT.get();
    let s = unsafe { &mut *p };
    let reference = if A {
        s.read()
    } else {
        ClockInstant::from_ticks(0)
    };
    let sampled = if M == 5 {
        false
    } else if M == 6 || M == 7 {
        let v = unsafe { s.diagnostic.visits.get_unchecked_mut(site_id) };
        let selected = *v % if M == 6 { 16 } else { 64 } == 0;
        *v = v.wrapping_add(1);
        selected
    } else if M == 8 {
        s.diagnostic.flag.load(Ordering::Relaxed)
            && s.diagnostic.flag.swap(false, Ordering::Relaxed)
    } else {
        true
    };
    let shared_entry = if M == 2 || M == 4 {
        s.diagnostic.last_exit.take()
    } else {
        None
    };
    let entered_at = if !sampled {
        ClockInstant::from_ticks(0)
    } else if let Some(at) = shared_entry {
        at
    } else if M == 1 {
        raw(s)
    } else if A {
        reference
    } else {
        s.read()
    };
    if M == 2 || M == 4 {
        s.diagnostic.last_exit = None;
    }
    let site = unsafe { *s.sites.get_unchecked(site_id) };
    let reentry = if M >= 3 {
        (unsafe { *s.diagnostic.recursive.get_unchecked(site_id) })
            && s.active_function == Some(site.callee)
    } else {
        site.caller == Some(site.callee) && s.active_function == Some(site.callee)
    };
    let (path, slot) = if reentry {
        (s.active, unsafe { s.active_slot.add(1) })
    } else if M >= 3 {
        let c = unsafe { *s.diagnostic.cache.get_unchecked(site_id) };
        if c.parent == u64::from(s.active.get()) {
            (c.path, c.slot)
        } else {
            let (path, slot) = s.path(site_id);
            unsafe {
                *s.diagnostic.cache.get_unchecked_mut(site_id) = FastCache {
                    parent: u64::from(s.active.get()),
                    path,
                    slot,
                };
            }
            (path, slot)
        }
    } else {
        let c = unsafe { *s.caches.get_unchecked(site_id) };
        if c.parent == Some(s.active) {
            (c.path, c.slot)
        } else {
            let (path, slot) = s.path(site_id);
            unsafe {
                *s.caches.get_unchecked_mut(site_id) = Cache {
                    parent: Some(s.active),
                    path,
                    slot,
                };
            }
            (path, slot)
        }
    };
    if M == 8 && s.diagnostic.samples.len() <= slot_index(slot) {
        s.diagnostic.samples.resize(slot_index(slot) + 1, 0);
    }
    let inner = super::Frame {
        state: p,
        entered_at,
        saved_path: s.active,
        saved_function: s.active_function,
        saved_slot: s.active_slot,
        slot,
        await_duration: AwaitDuration::ZERO,
        flags: u8::from(reentry),
    };
    s.depth += 1;
    s.active = path;
    s.active_function = Some(site.callee);
    s.active_slot = if reentry {
        unsafe { slot.sub(1) }
    } else {
        slot
    };
    Frame {
        inner,
        sampled,
        reference,
        reads: u64::from(sampled && shared_entry.is_none()),
    }
}
#[inline(always)]
fn slot_index(slot: *mut AggregateDelta) -> usize {
    unsafe { (*slot).node.get() as usize }
}
impl Frame {
    #[inline(always)]
    pub fn finish<const M: u8, const A: bool, T>(self, result: &mut R<T>) {
        let f = self.inner;
        let s = unsafe { &mut *f.state };
        let reference = if A {
            match result {
                Ok(_) => s.read(),
                Err(e) => *e.telemetry_at.get_or_insert_with(|| s.read()),
            }
        } else {
            ClockInstant::from_ticks(0)
        };
        let mut reads = self.reads;
        let at = if !self.sampled {
            ClockInstant::from_ticks(0)
        } else if let Err(e) = result {
            // Keep the runtime's one ordered timestamp per raise, including
            // in raw mode; do not change error evidence or unwind endpoints.
            *e.telemetry_at.get_or_insert_with(|| {
                reads += 1;
                s.read()
            })
        } else if (M == 2 || M == 4)
            && let Some(at) = s.diagnostic.last_exit
        {
            at
        } else {
            reads += 1;
            if M == 1 {
                raw(s)
            } else if A {
                reference
            } else {
                s.read()
            }
        };
        if M == 2 || M == 4 {
            s.diagnostic.last_exit = Some(at);
        }
        let backwards = self.sampled && at.get() < f.entered_at.get();
        let ticks = if self.sampled {
            at.get().saturating_sub(f.entered_at.get())
        } else {
            0
        };
        let scaled = if M == 6 || M == 7 {
            ticks
                .checked_mul(if M == 6 { 16 } else { 64 })
                .expect("diagnostic duration overflow")
        } else {
            ticks
        };
        let outcome = result
            .as_ref()
            .map_or_else(|e| super::outcome(e), |_| InvocationOutcome::Ok);
        let duration = btel_types::ClockDuration::from_ticks(scaled);
        let slot = unsafe { &mut *f.slot };
        if !(outcome == InvocationOutcome::Ok && add_ok(slot, duration)) {
            complete_unusual(s, f.slot, duration, f.await_duration, outcome);
        }
        if M == 8 && self.sampled {
            s.diagnostic.samples[slot_index(f.slot)] += 1;
        }
        if A {
            let a = s
                .diagnostic
                .audit
                .entry(slot_index(f.slot) as u64)
                .or_default();
            a.count += 1;
            if M != 8 {
                a.ticks += scaled;
            }
            a.reference += self.reference.elapsed_until(reference).get();
            a.samples += u64::from(self.sampled);
            a.reads += reads;
            a.backwards += u64::from(backwards);
        }
        evidence::frame_completed(s, result);
        s.depth -= 1;
        s.active = f.saved_path;
        s.active_function = f.saved_function;
        s.active_slot = f.saved_slot;
    }
}
/// # Safety
/// Same requirements as `enter`.
pub unsafe fn enter_audit(site: usize) -> Frame {
    let m = unsafe { (*CURRENT.get()).diagnostic.mode };
    match m {
        0 => unsafe { enter::<0, true>(site) },
        1 => unsafe { enter::<1, true>(site) },
        2 => unsafe { enter::<2, true>(site) },
        3 => unsafe { enter::<3, true>(site) },
        4 => unsafe { enter::<4, true>(site) },
        5 => unsafe { enter::<5, true>(site) },
        6 => unsafe { enter::<6, true>(site) },
        7 => unsafe { enter::<7, true>(site) },
        8 => unsafe { enter::<8, true>(site) },
        _ => unreachable!(),
    }
}
impl Frame {
    pub fn finish_audit<T>(self, r: &mut R<T>) {
        let m = unsafe { (*CURRENT.get()).diagnostic.mode };
        match m {
            0 => self.finish::<0, true, T>(r),
            1 => self.finish::<1, true, T>(r),
            2 => self.finish::<2, true, T>(r),
            3 => self.finish::<3, true, T>(r),
            4 => self.finish::<4, true, T>(r),
            5 => self.finish::<5, true, T>(r),
            6 => self.finish::<6, true, T>(r),
            7 => self.finish::<7, true, T>(r),
            8 => self.finish::<8, true, T>(r),
            _ => unreachable!(),
        }
    }
}
pub(super) fn prepare_flush(s: &mut super::State) {
    if s.diagnostic.mode == 8 {
        for slot in s.slots.iter_mut().flat_map(|p| p.iter_mut()) {
            let n = s
                .diagnostic
                .samples
                .get(slot.node.get() as usize)
                .copied()
                .unwrap_or(0);
            if n > 0 {
                slot.total_duration = btel_types::ClockDuration::from_ticks(
                    ((u128::from(slot.total_duration.get()) * u128::from(slot.count))
                        / u128::from(n))
                    .try_into()
                    .unwrap(),
                );
            }
            if let Some(a) = s.diagnostic.audit.get_mut(&slot.node.get()) {
                a.ticks += slot.total_duration.get();
            }
        }
        s.diagnostic.samples.fill(0);
    }
}
pub(super) fn end_run(s: &mut super::State) {
    if !s.diagnostic.audit.is_empty() {
        let rows:Vec<_>=s.diagnostic.audit.iter().map(|(node,a)|{
            let ticks=a.ticks;
            serde_json::json!({"node":node,"count":a.count,"ticks":ticks,"reference":a.reference,"samples":a.samples,"reads":a.reads,"backwards":a.backwards})
        }).collect();
        eprintln!(
            "DIAGNOSTIC {}",
            serde_json::json!({"mode":s.diagnostic.mode,"rows":rows})
        );
    }
}

/// # Safety
/// Same session/site requirements as `enter`.
pub unsafe fn observe_timing<T>(site: usize, call: impl FnOnce() -> R<T>) -> R<T> {
    let s = unsafe { &*CURRENT.get() };
    let mode = s.diagnostic.mode;
    let audit = s.diagnostic.audit_enabled;
    if audit {
        let f = unsafe { enter_audit(site) };
        let mut r = call();
        invalidate();
        f.finish_audit(&mut r);
        return r;
    }
    macro_rules! run {
        ($m:literal) => {{
            let f = unsafe { enter::<$m, false>(site) };
            let mut r = call();
            invalidate();
            f.finish::<$m, false, T>(&mut r);
            r
        }};
    }
    match mode {
        0 => run!(0),
        1 => run!(1),
        2 => run!(2),
        3 => run!(3),
        4 => run!(4),
        5 => run!(5),
        6 => run!(6),
        7 => run!(7),
        8 => run!(8),
        _ => unreachable!(),
    }
}

#[cfg(test)]
pub(super) fn audit_reads(s: &super::State) -> u64 {
    s.diagnostic.audit.values().map(|a| a.reads).sum()
}

/// Diagnostic generated JSON/formatting wrappers are opaque program work.
/// # Safety
/// Same requirements as the production `observed_captured` API.
pub unsafe fn observed_captured<const M: u8, const A: bool, T>(
    word: &AtomicU8,
    site: usize,
    inputs: impl FnOnce() -> InputCaptures,
    output: impl FnOnce(&R<T>) -> Option<Snapshot>,
    call: impl FnOnce() -> R<T>,
) -> R<T> {
    let mode = super::mode(word);
    let call = || {
        let result = call();
        invalidate();
        result
    };
    if mode == 0 {
        return call();
    }
    if mode > 3 {
        return unsafe {
            evidence::observe_diagnostic::<M, A, _>(site, mode & 3, inputs, output, call)
        };
    }
    if mode == 1 {
        if M == 9 {
            return unsafe { observe_timing(site, call) };
        }
        let frame = unsafe { enter::<M, A>(site) };
        let mut result = call();
        frame.finish::<M, A, _>(&mut result);
        return result;
    }
    unsafe { super::observe_captured(site, mode, inputs, output, call) }
}
