//! Debug builds: which opcodes this process's dispatch loops have run.
//!
//! Every instruction the dispatch loop runs in a debug build passes the
//! operand-depth assertion that ties [`bex_vm_types::Bytecode::stack_effect`]
//! to the VM's handlers. This set shows which handlers that assertion has
//! seen: the coverage tests insist on every opcode, so none escapes it.
//!
//! With `BAML_DEBUG_EXECUTED_OPCODES=<path>` set, the set is written to
//! `<path>` at process exit, one opcode name per line, so a test can check a
//! `baml-cli` run of the corpus.
#![allow(clippy::print_stderr)]

use std::sync::atomic::{AtomicBool, Ordering};

use bex_vm_types::bytecode::OpCode;

/// The environment variable naming the file the set is written to at exit.
pub const DUMP_ENV: &str = "BAML_DEBUG_EXECUTED_OPCODES";

static SEEN: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];

/// Note that the dispatch loop ran opcode `op`.
#[inline]
pub(crate) fn record(op: u8) {
    let seen = &SEEN[usize::from(op)];
    if !seen.load(Ordering::Relaxed) {
        seen.store(true, Ordering::Relaxed);
        register_dump();
    }
}

/// Every opcode this process has run, in opcode order.
pub fn executed() -> Vec<OpCode> {
    (0..=u8::MAX)
        .filter(|&byte| SEEN[usize::from(byte)].load(Ordering::Relaxed))
        .filter_map(|byte| OpCode::try_from(byte).ok())
        .collect()
}

/// Every opcode the VM defines, in opcode order.
pub fn all() -> Vec<OpCode> {
    (0..=u8::MAX)
        .filter_map(|byte| OpCode::try_from(byte).ok())
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn register_dump() {
    use std::{fmt::Write as _, path::PathBuf, sync::OnceLock};

    #[allow(unsafe_code)]
    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
    }

    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

    extern "C" fn write_executed() {
        let Some(Some(path)) = PATH.get() else {
            return;
        };
        let mut names = String::new();
        for op in executed() {
            let _ = writeln!(names, "{op:?}");
        }
        if let Err(error) = std::fs::write(path, names) {
            eprintln!(
                "could not write executed opcodes to {}: {error}",
                path.display()
            );
        }
    }

    PATH.get_or_init(|| {
        let path = match baml_env::path_var(DUMP_ENV) {
            Ok(path) => path?,
            Err(error) => {
                eprintln!("[opcode coverage] {error}; not written");
                return None;
            }
        };
        // SAFETY: `write_executed` is an `extern "C" fn()` that only reads
        // atomics and a set-once path, and writes a file.
        #[allow(unsafe_code)]
        unsafe {
            atexit(write_executed);
        }
        Some(path)
    });
}

#[cfg(target_arch = "wasm32")]
fn register_dump() {}
