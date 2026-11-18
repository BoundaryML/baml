//! The runtime a natively compiled BAML program links against.
//!
//! Compiled code reaches it only through the `baml_rt_*` symbols, all with
//! the C calling convention but the slow paths in [`stubs`], which preserve
//! every register. The compiled object defines two symbols in the
//! other direction: `baml_native_entry`, which runs the program's entry
//! function and hands its result to one of the `baml_rt_print_*` functions,
//! and the two tables the collector reads: `baml_native_stack_maps`, where
//! compiled frames keep their references, and `baml_native_types`, what
//! each type of object holds. A throw reads two more, `baml_native_unwind`
//! and `baml_native_functions`: see [`unwind`].
//!
//! The standard library's natives are in [`native`]: their symbols are
//! generated from the standard library's own declarations.

#![expect(
    unsafe_code,
    reason = "the runtime is the unsafe half of compiled code: raw objects, raw frames, C entry points"
)]

pub mod array;
mod class;
mod describe;
pub mod heap;
pub mod json_layout;
pub mod layout;
pub mod map;
pub mod native;
pub mod sched;
pub mod string;
pub mod stubs;
pub mod unwind;

use std::io::Write;

#[cfg(not(test))]
use heap::{Mutator, Program};

/// The exit status of a program that ended in an uncaught panic, matching
/// what `baml run` exits with.
const UNCAUGHT_EXIT_STATUS: i32 = 1;

/// Set in the environment to collect on every allocation.
#[cfg(not(test))]
const GC_STRESS_VARIABLE: &str = "BAML_GC_STRESS";

#[cfg(not(test))]
unsafe extern "C" {
    /// Defined by the compiled program: calls the entry function with the
    /// mutator in its dedicated register and prints the result.
    fn baml_native_entry(mutator: *mut std::ffi::c_void);
    /// Defined by the compiled program: the first word of its stack maps.
    static baml_native_stack_maps: usize;
    /// Defined by the compiled program: the first word of its type table.
    static baml_native_types: usize;
    /// Defined by the compiled program: its table of calls with a handler.
    static baml_native_unwind: usize;
    /// Defined by the compiled program: its function table.
    static baml_native_functions: usize;
}

/// Process entry point.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let stress = std::env::var_os(GC_STRESS_VARIABLE).is_some();
    // SAFETY: the linked program defines both symbols as declared.
    unsafe {
        let program = Program {
            stack_maps: &raw const baml_native_stack_maps as usize,
            types: &raw const baml_native_types as usize,
            unwind: &raw const baml_native_unwind as usize,
            functions: &raw const baml_native_functions as usize,
        };
        map::randomize_seed();
        let mut mutator = Mutator::new(program, stress);
        // The entry function runs as a task, like everything it spawns.
        sched::run(&raw mut mutator, baml_native_entry as *const () as usize)
    }
}

/// An `int` result, as `baml run` prints it.
pub fn int_text(value: i64) -> String {
    value.to_string()
}

/// A `float` result, as `baml run` prints it: the shortest decimal that
/// round-trips, never in exponent form, with `.0` appended to a finite value
/// that would otherwise read as an integer.
pub fn float_text(value: f64) -> String {
    let text = value.to_string();
    if text.contains('.') || !value.is_finite() {
        text
    } else {
        format!("{text}.0")
    }
}

fn print_line(text: &str) {
    let mut out = std::io::stdout().lock();
    // A closed stdout is not an error the program can act on.
    let _ = writeln!(out, "{text}");
}

/// Report an uncaught throw on stderr and end the process.
#[expect(clippy::exit, reason = "an uncaught panic ends the program")]
fn uncaught(thrown: &str) -> ! {
    let _ = std::io::stdout().flush();
    let _ = writeln!(std::io::stderr().lock(), "error: uncaught throw: {thrown}");
    std::process::exit(UNCAUGHT_EXIT_STATUS)
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_print_int(value: i64) {
    print_line(&int_text(value));
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_print_float(value: f64) {
    print_line(&float_text(value));
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_print_bool(value: u8) {
    print_line(if value == 0 { "false" } else { "true" });
}

/// Print a `string` result as `baml run` does: quoted, with escapes.
///
/// # Safety
///
/// `value` is a live string object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_print_string(value: usize) {
    // SAFETY: the caller's contract.
    print_line(&format!("{:?}", unsafe { string::text(value) }));
}

/// Print an enum result as `baml run` does: the variant's name.
///
/// # Safety
///
/// Called from compiled code with its mutator, the type of the enum's boxes
/// and a variant of that enum.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_print_enum(mutator: &Mutator, enum_type: usize, index: usize) {
    print_line(mutator.variant_name(enum_type, index));
}

/// Float remainder with the sign of the dividend, the `%` of `float`.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_float_rem(left: f64, right: f64) -> f64 {
    left % right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_as_baml_run_prints_them() {
        assert_eq!(float_text(6.5), "6.5");
        assert_eq!(float_text(2.0), "2.0");
        assert_eq!(float_text(-0.0), "-0.0");
        assert_eq!(float_text(1e22), "10000000000000000000000.0");
        assert_eq!(float_text(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(float_text(1.0 / 3.0 * 1e-7), "0.00000003333333333333333");
        assert_eq!(float_text(f64::INFINITY), "inf");
        assert_eq!(float_text(f64::NEG_INFINITY), "-inf");
        assert_eq!(float_text(f64::NAN), "NaN");
    }
}
