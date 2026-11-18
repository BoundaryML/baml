//! The runtime a natively compiled BAML program links against.
//!
//! Compiled code reaches it only through the `baml_rt_*` symbols below, all
//! with the C calling convention and scalar arguments. The compiled object
//! defines one symbol in the other direction, `baml_native_entry`, which runs
//! the program's entry function and hands its result to one of the
//! `baml_rt_print_*` functions.
//!
//! This file has no dependencies beyond `std` and no modules, so that it can
//! be compiled on its own into the archive programs are linked against.

use std::io::Write;

/// The exit status of a program that ended in an uncaught panic, matching
/// what `baml run` exits with.
const UNCAUGHT_EXIT_STATUS: i32 = 1;

#[cfg(not(test))]
#[expect(unsafe_code)]
unsafe extern "C" {
    /// Defined by the compiled program: calls the entry function and prints
    /// its result.
    fn baml_native_entry();
}

/// Process entry point.
#[cfg(not(test))]
#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // SAFETY: the linked program defines `baml_native_entry` as a function
    // taking and returning nothing.
    unsafe { baml_native_entry() };
    0
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

/// The text of an uncaught `baml.panics.DivisionByZero`.
pub fn division_by_zero_text(dividend: i64) -> String {
    format!("baml.panics.DivisionByZero {{dividend: {dividend}}}")
}

/// The text of an uncaught `baml.panics.NegativeBitShift`.
pub fn negative_shift_text(count: i64) -> String {
    format!("baml.panics.NegativeBitShift {{message: \"bit shift count is negative: {count}\"}}")
}

fn print_line(text: &str) {
    let mut out = std::io::stdout().lock();
    // A closed stdout is not an error the program can act on.
    let _ = writeln!(out, "{text}");
}

/// Report an uncaught panic on stderr and end the process.
#[expect(clippy::exit, reason = "an uncaught panic ends the program")]
fn uncaught(thrown: &str) -> ! {
    let _ = std::io::stdout().flush();
    let _ = writeln!(std::io::stderr().lock(), "error: uncaught throw: {thrown}");
    std::process::exit(UNCAUGHT_EXIT_STATUS)
}

#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn baml_rt_print_int(value: i64) {
    print_line(&int_text(value));
}

#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn baml_rt_print_float(value: f64) {
    print_line(&float_text(value));
}

#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn baml_rt_print_bool(value: u8) {
    print_line(if value == 0 { "false" } else { "true" });
}

#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn baml_rt_panic_division_by_zero(dividend: i64) -> ! {
    uncaught(&division_by_zero_text(dividend))
}

#[unsafe(no_mangle)]
#[expect(unsafe_code)]
pub extern "C" fn baml_rt_panic_negative_shift(count: i64) -> ! {
    uncaught(&negative_shift_text(count))
}

/// Float remainder with the sign of the dividend, the `%` of `float`.
#[unsafe(no_mangle)]
#[expect(unsafe_code)]
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

    #[test]
    fn panics_render_as_the_interpreter_renders_them() {
        assert_eq!(
            division_by_zero_text(1),
            "baml.panics.DivisionByZero {dividend: 1}"
        );
        assert_eq!(
            negative_shift_text(-1),
            "baml.panics.NegativeBitShift {message: \"bit shift count is negative: -1\"}"
        );
    }
}
