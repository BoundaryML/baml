//! Dynamic opcode-sequence counts, for choosing superinstructions by measured
//! frequency.
//!
//! Built only with the `kperf` feature, like the VM-op counter. When
//! `DEV_BAML_VM_OP_MIX=<path>` is set, every opcode the dispatch loop runs is
//! counted together with the opcodes that ran just before it on the same
//! thread, as sequences of 1 to [`MAX_LEN`] opcodes. At process exit the
//! counts are written to `<path>`, one `count<TAB>OP OP ...` line per
//! sequence, most frequent first.
//!
//! Sequences are dynamic: they cross jumps, calls and returns. A fusion is
//! only possible inside one basic block, which the emitter checks; these
//! counts say which shapes are worth fusing. Each thread folds its counts
//! into the shared table every [`BATCH`] opcodes, so a thread still running
//! at exit loses at most that many.

#![allow(clippy::print_stderr)]

use std::{
    cell::RefCell,
    fmt::Write as _,
    sync::{Mutex, OnceLock},
};

use rustc_hash::FxHashMap;

/// Longest sequence counted.
pub const MAX_LEN: usize = 4;
#[allow(clippy::cast_possible_truncation)]
const MAX_FILLED: u8 = MAX_LEN as u8;

/// Opcodes a thread records before folding them into the shared table.
const BATCH: usize = 1 << 16;

/// A sequence: its length and its opcodes, oldest in the highest byte.
type Key = (u8, u32);

struct Registry {
    path: String,
    counts: Mutex<FxHashMap<Key, u64>>,
}

static REGISTRY: OnceLock<Option<Registry>> = OnceLock::new();

#[allow(unsafe_code)]
unsafe extern "C" {
    fn atexit(cb: extern "C" fn()) -> std::ffi::c_int;
}

/// The last [`MAX_LEN`] opcodes of each recorded step, with how many of them
/// are real (the first steps of a thread have fewer predecessors).
struct Local {
    window: u32,
    filled: u8,
    pending: Vec<(u8, u32)>,
}

impl Drop for Local {
    fn drop(&mut self) {
        fold(&mut self.pending);
    }
}

thread_local! {
    static LOCAL: RefCell<Local> = const {
        RefCell::new(Local { window: 0, filled: 0, pending: Vec::new() })
    };
}

fn registry() -> Option<&'static Registry> {
    REGISTRY
        .get_or_init(|| {
            let path = match baml_env::path_var("DEV_BAML_VM_OP_MIX") {
                Ok(path) => path?,
                Err(error) => {
                    eprintln!("[op-mix] {error}; disabled");
                    return None;
                }
            };
            // SAFETY: `write_report` is an `extern "C" fn()` that only reads
            // process-global state.
            #[allow(unsafe_code)]
            unsafe {
                atexit(write_report);
            }
            Some(Registry {
                path: path.display().to_string(),
                counts: Mutex::new(FxHashMap::default()),
            })
        })
        .as_ref()
}

/// Count `op` after the opcodes this thread ran before it.
#[inline(never)]
pub fn record(op: u8) {
    if registry().is_none() {
        return;
    }
    LOCAL.with_borrow_mut(|local| {
        local.window = (local.window << 8) | u32::from(op);
        local.filled = (local.filled + 1).min(MAX_FILLED);
        local.pending.push((local.filled, local.window));
        if local.pending.len() >= BATCH {
            fold(&mut local.pending);
        }
    });
}

fn fold(pending: &mut Vec<(u8, u32)>) {
    let Some(registry) = registry() else {
        return;
    };
    let mut counts = registry
        .counts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (filled, window) in pending.drain(..) {
        for len in 1..=filled {
            let mask = if len as usize == MAX_LEN {
                u32::MAX
            } else {
                (1 << (8 * u32::from(len))) - 1
            };
            *counts.entry((len, window & mask)).or_default() += 1;
        }
    }
}

extern "C" fn write_report() {
    let Some(registry) = registry() else {
        return;
    };
    let _ = LOCAL.try_with(|local| fold(&mut local.borrow_mut().pending));
    let counts = registry
        .counts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut rows: Vec<_> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    let mut out = String::new();
    for (&(len, ops), n) in rows {
        let names: Vec<String> = (0..len)
            .rev()
            .map(|i| {
                let byte = ops.to_be_bytes()[3 - usize::from(i)];
                match bex_vm_types::bytecode::OpCode::try_from(byte) {
                    Ok(op) => format!("{op:?}"),
                    Err(_) => format!("op{byte}"),
                }
            })
            .collect();
        let _ = writeln!(out, "{n}\t{}", names.join(" "));
    }
    if let Err(error) = std::fs::write(&registry.path, out) {
        eprintln!("[op-mix] could not write {}: {error}", registry.path);
    }
}
