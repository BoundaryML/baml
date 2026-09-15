//! Opt-in macOS allocator-pressure probe for diagnostic pack-host builds.

#![allow(unsafe_code, reason = "macOS malloc and signal APIs require FFI")]

use std::{
    fs::OpenOptions,
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const RESULT_ENV: &str = "BAML_MALLOC_PRESSURE_RELIEF_RESULT";
static RELIEF_REQUESTED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
}

extern "C" fn request_relief(_signal: libc::c_int) {
    RELIEF_REQUESTED.store(true, Ordering::Release);
}

fn append_result(path: &Path, line: &str) -> std::io::Result<()> {
    let mut output = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(output, "{line}")
}

fn install_signal_handler() -> std::io::Result<()> {
    // SAFETY: A zeroed sigaction is valid initialization on macOS. The installed handler only performs one lock-free atomic store, and SIGUSR2 remains installed for the process lifetime.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = request_relief as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&raw mut action.sa_mask);
        if libc::sigaction(libc::SIGUSR2, &raw const action, std::ptr::null_mut()) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Install the probe only when the benchmark supplies an output path.
pub(crate) fn install_from_env() {
    let Some(path) = std::env::var_os(RESULT_ENV).map(PathBuf::from) else {
        return;
    };
    if let Err(error) = install_signal_handler() {
        let _ = append_result(&path, &format!("error=install signal handler: {error}"));
        return;
    }
    let spawn = std::thread::Builder::new()
        .name("malloc-pressure-relief".to_string())
        .spawn(move || {
            if append_result(&path, "ready").is_err() {
                return;
            }
            loop {
                if RELIEF_REQUESTED.swap(false, Ordering::AcqRel) {
                    // SAFETY: Passing a null zone requests pressure relief from every registered malloc zone, as documented by macOS libmalloc.
                    let relieved = unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
                    let _ = append_result(&path, &format!("relieved={relieved}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
    if let Err(error) = spawn {
        let path = std::env::var_os(RESULT_ENV).map(PathBuf::from);
        if let Some(path) = path {
            let _ = append_result(&path, &format!("error=spawn probe thread: {error}"));
        }
    }
}
