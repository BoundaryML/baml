//! In-process Apple-Silicon performance-counter probe (cycles + instructions
//! retired) for the VM dispatch loop, plus a deterministic VM-op counter.
//!
//! Enabled only when `BAML_KPERF=1`. Reads the per-thread FIXED PMCs
//! (cycles, instructions) around each `exec()` call on whatever worker thread
//! runs it, accumulates the deltas process-globally, and prints a summary at
//! exit (cyc/op, instr/op, IPC) so we can compare the VM against a reference
//! interpreter on a stable, frequency-independent basis.
//!
//! macOS/aarch64 only; a no-op shim elsewhere. Needs the process to have PMC
//! access (run under `sudo`), otherwise it disables itself silently.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[allow(
    unsafe_code,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
mod imp {
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    };

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, sym: *const c_char) -> *mut c_void;
        fn atexit(cb: extern "C" fn()) -> c_int;
    }

    const KPC_CLASS_FIXED_MASK: u32 = 1;

    type ForceAll = extern "C" fn(c_int) -> c_int;
    type SetCounting = extern "C" fn(u32) -> c_int;
    type CounterCount = extern "C" fn(u32) -> u32;
    type GetThread = extern "C" fn(u32, u32, *mut u64) -> c_int;

    static INIT: std::sync::Once = std::sync::Once::new();
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static N_FIXED: AtomicU32 = AtomicU32::new(0);
    static GET_THREAD: AtomicU64 = AtomicU64::new(0); // fn ptr as usize

    static ACC_CYCLES: AtomicU64 = AtomicU64::new(0);
    static ACC_INSTRS: AtomicU64 = AtomicU64::new(0);
    static ACC_OPS: AtomicU64 = AtomicU64::new(0);
    static EXEC_CALLS: AtomicU64 = AtomicU64::new(0);

    unsafe fn load_sym<T>(h: *mut c_void, name: &str) -> Option<T> {
        let c = CString::new(name).ok()?;
        let p = unsafe { dlsym(h, c.as_ptr()) };
        if p.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
        }
    }

    /// Idempotent: runs `init()` once, returns whether counters are live.
    #[inline]
    pub fn enabled() -> bool {
        INIT.call_once(|| {
            init();
        });
        ENABLED.load(Ordering::Relaxed)
    }

    /// One-time setup; returns whether counters are live.
    fn init() -> bool {
        if std::env::var("BAML_KPERF").as_deref() != Ok("1") {
            return false;
        }
        unsafe {
            let path =
                CString::new("/System/Library/PrivateFrameworks/kperf.framework/kperf").unwrap();
            let h = dlopen(path.as_ptr(), 2 /* RTLD_NOW */);
            if h.is_null() {
                eprintln!("[kperf] dlopen failed; disabled");
                return false;
            }
            let (
                Some(force_all),
                Some(set_counting),
                Some(set_thread),
                Some(count),
                Some(get_thread),
            ) = (
                load_sym::<ForceAll>(h, "kpc_force_all_ctrs_set"),
                load_sym::<SetCounting>(h, "kpc_set_counting"),
                load_sym::<SetCounting>(h, "kpc_set_thread_counting"),
                load_sym::<CounterCount>(h, "kpc_get_counter_count"),
                load_sym::<GetThread>(h, "kpc_get_thread_counters"),
            )
            else {
                eprintln!("[kperf] missing symbols; disabled");
                return false;
            };
            if force_all(1) != 0 {
                eprintln!("[kperf] kpc_force_all_ctrs_set failed (need sudo); disabled");
                return false;
            }
            // kpc_* return 0 on success; bail (fail closed) on any error so a
            // failed setup never feeds zeroed reads into the wrapping deltas.
            if set_counting(KPC_CLASS_FIXED_MASK) != 0 || set_thread(KPC_CLASS_FIXED_MASK) != 0 {
                eprintln!("[kperf] failed to enable counting (need sudo?); disabled");
                return false;
            }
            let n = count(KPC_CLASS_FIXED_MASK);
            if n < 2 {
                eprintln!("[kperf] fixed counter count {n} < 2; disabled");
                return false;
            }
            N_FIXED.store(n, Ordering::Relaxed);
            GET_THREAD.store(get_thread as usize as u64, Ordering::Relaxed);
            ENABLED.store(true, Ordering::Relaxed);
            atexit(report);
            eprintln!("[kperf] enabled ({n} fixed counters)");
            true
        }
    }

    /// Read (cycles, instructions) for the current thread, or `None` if the
    /// counters aren't live or the kernel rejects the read. Apple fixed PMCs are
    /// [cycles, instructions, ...].
    #[inline]
    fn read() -> Option<(u64, u64)> {
        let f = GET_THREAD.load(Ordering::Relaxed);
        if f == 0 {
            return None;
        }
        let get_thread: GetThread = unsafe { std::mem::transmute(f as usize as *const c_void) };
        let n = N_FIXED.load(Ordering::Relaxed);
        let mut buf = [0u64; 32];
        // Non-zero return (e.g. EINVAL on too-small a buffer) means the values
        // are garbage; reject so they don't become huge wrapped deltas.
        if get_thread(0, n, buf.as_mut_ptr()) != 0 {
            return None;
        }
        Some((buf[0], buf[1]))
    }

    /// Snapshot at the start of an `exec()` call. `None` if the read failed.
    #[inline]
    pub fn exec_start() -> Option<(u64, u64)> {
        read()
    }

    /// Accumulate the delta over an `exec()` call plus the ops it dispatched.
    /// Skips the sample entirely if either the start or end read failed.
    #[inline]
    pub fn exec_end(start: Option<(u64, u64)>, ops: u64) {
        let (Some((c0, i0)), Some((c1, i1))) = (start, read()) else {
            return;
        };
        ACC_CYCLES.fetch_add(c1.wrapping_sub(c0), Ordering::Relaxed);
        ACC_INSTRS.fetch_add(i1.wrapping_sub(i0), Ordering::Relaxed);
        ACC_OPS.fetch_add(ops, Ordering::Relaxed);
        EXEC_CALLS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn report() {
        let c = ACC_CYCLES.load(Ordering::Relaxed);
        let i = ACC_INSTRS.load(Ordering::Relaxed);
        let ops = ACC_OPS.load(Ordering::Relaxed);
        let calls = EXEC_CALLS.load(Ordering::Relaxed);
        // Total instructions/cycles come straight from the hardware counters and
        // are the primary metric — always reported. The per-op breakdown needs
        // the VM op counter, which is only populated under the `kperf` feature.
        eprintln!(
            "[kperf] exec calls={calls}  VM ops={ops}\n\
             [kperf]   cycles={:.3}e9  instructions={:.3}e9  IPC={:.3}",
            c as f64 / 1e9,
            i as f64 / 1e9,
            i as f64 / c.max(1) as f64,
        );
        if ops > 0 {
            eprintln!(
                "[kperf]   per VM-op: {:.2} cyc/op   {:.2} instr/op",
                c as f64 / ops as f64,
                i as f64 / ops as f64,
            );
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use imp::{enabled, exec_end, exec_start};

// ── No-op shim for every other platform ──────────────────────────────────
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod shim {
    #[inline]
    pub fn enabled() -> bool {
        false
    }
    #[inline]
    pub fn exec_start() -> Option<(u64, u64)> {
        None
    }
    #[inline]
    pub fn exec_end(_start: Option<(u64, u64)>, _ops: u64) {}
}
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub use shim::{enabled, exec_end, exec_start};
