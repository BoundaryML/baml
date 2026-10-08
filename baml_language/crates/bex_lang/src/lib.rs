//! Language semantics shared by the BAML VM and generated native code.
//!
//! The native backend links no VM crate, so the integer semantics and panic
//! payloads both backends must agree on live here, depending only on
//! [`baml_type`]. See `README.md` for what belongs in this crate.

use std::fmt;

pub use baml_type::{Int63, IntShiftError};

pub mod int;

/// A catchable BAML panic that generated code or the VM can raise without a
/// live heap. Each variant maps to one `baml.panics.*` class, carrying the
/// fields the VM stores when it materializes the panic object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Panic {
    /// `int` or `bigint` division or remainder by zero. The field is the
    /// left operand, matching `DivisionByZero { dividend }`.
    DivisionByZero {
        /// The left operand of the failing `/` or `%`.
        dividend: Int63,
    },
    /// An `int` operation left the i63 range. The message names the
    /// operation, e.g. `"4611686018427387903 + 1 overflows int"`.
    IntegerOverflow {
        /// Human-readable description of the overflowing operation.
        message: String,
    },
    /// The right operand of `<<` or `>>` was negative.
    NegativeBitShift {
        /// Human-readable description, e.g. `"bit shift count is negative: -1"`.
        message: String,
    },
    /// A user-caused panic from `baml.sys.panic`.
    UserPanic {
        /// The user-supplied message.
        message: String,
    },
    /// An `assert` statement failed.
    AssertionFailed,
    /// A branch the program declared impossible was executed.
    Unreachable,
    /// The call stack depth limit was exceeded.
    StackOverflow,
    /// A clean process-termination request from `baml.sys.exit(code)`.
    Exit {
        /// The exit code the user wrote, as a full BAML `int`.
        code: Int63,
    },
}

impl Panic {
    /// Unqualified name of the `baml.panics` class this panic materializes as.
    pub fn class_name(&self) -> &'static str {
        match self {
            Self::DivisionByZero { .. } => "DivisionByZero",
            Self::IntegerOverflow { .. } => "IntegerOverflow",
            Self::NegativeBitShift { .. } => "NegativeBitShift",
            Self::UserPanic { .. } => "UserPanic",
            Self::AssertionFailed => "AssertionFailed",
            Self::Unreachable => "Unreachable",
            Self::StackOverflow => "StackOverflow",
            Self::Exit { .. } => "Exit",
        }
    }

    /// Fully qualified name of the class this panic materializes as, e.g.
    /// `"baml.panics.DivisionByZero"`.
    pub fn class_fqn(&self) -> &'static str {
        match self {
            Self::DivisionByZero { .. } => "baml.panics.DivisionByZero",
            Self::IntegerOverflow { .. } => "baml.panics.IntegerOverflow",
            Self::NegativeBitShift { .. } => "baml.panics.NegativeBitShift",
            Self::UserPanic { .. } => "baml.panics.UserPanic",
            Self::AssertionFailed => "baml.panics.AssertionFailed",
            Self::Unreachable => "baml.panics.Unreachable",
            Self::StackOverflow => "baml.panics.StackOverflow",
            Self::Exit { .. } => "baml.panics.Exit",
        }
    }

    /// The `message` field the VM stores for the field-less variants, which
    /// the `baml.panics` classes nevertheless declare with a `message: string`.
    fn fixed_message(&self) -> Option<&'static str> {
        match self {
            Self::AssertionFailed => Some("assertion failed"),
            Self::Unreachable => Some("unreachable code executed"),
            Self::StackOverflow => Some("stack overflow"),
            _ => None,
        }
    }

    /// Render the panic object exactly as the engine prints an uncaught one,
    /// e.g. `baml.panics.DivisionByZero {dividend: 42}` or
    /// `baml.panics.IntegerOverflow {message: "1 + 1 overflows int"}`.
    ///
    /// Mirrors `BexExternalValue::render_readable` applied to the object the
    /// VM materializes: `Class {field: value, ...}` with strings in Rust
    /// `{:?}` form and integers in decimal.
    pub fn render_readable(&self) -> String {
        let class = self.class_fqn();
        match self {
            Self::DivisionByZero { dividend } => format!("{class} {{dividend: {dividend}}}"),
            Self::IntegerOverflow { message }
            | Self::NegativeBitShift { message }
            | Self::UserPanic { message } => format!("{class} {{message: {message:?}}}"),
            Self::AssertionFailed | Self::Unreachable | Self::StackOverflow => {
                let message = self
                    .fixed_message()
                    .expect("field-less panic variants carry a fixed message");
                format!("{class} {{message: {message:?}}}")
            }
            Self::Exit { code } => format!("{class} {{code: {code}}}"),
        }
    }

    /// Process exit code for an uncaught instance of this panic: `Exit`
    /// narrows its code with [`clamp_exit_code`]; every other panic is `1`.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Exit { code } => clamp_exit_code(code.get()),
            _ => 1,
        }
    }
}

/// Same text as the VM's `VmPanic` error messages for the matching variants.
impl fmt::Display for Panic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // `VmPanic` formats its `Value` operands with `{:?}`, which renders
            // a tagged int as `Int(n)`; mirror that so the strings are identical.
            Self::DivisionByZero { dividend } => {
                write!(f, "division by zero: Int({dividend}) / Int(0)")
            }
            Self::IntegerOverflow { message } => write!(f, "integer overflow: {message}"),
            Self::NegativeBitShift { message } => write!(f, "negative bit shift: {message}"),
            Self::UserPanic { message } => write!(f, "baml.sys.panic: {message}"),
            Self::AssertionFailed => f.write_str("assertion failed"),
            Self::Unreachable => f.write_str("unreachable code executed"),
            Self::StackOverflow => f.write_str("stack overflow"),
            Self::Exit { code } => write!(f, "baml.sys.exit({code})"),
        }
    }
}

impl std::error::Error for Panic {}

/// Narrow a `baml.sys.exit(code)` value (a BAML `int`) to the `i32` that
/// `std::process::exit` and C's `exit(int)` take, saturating at the `i32`
/// bounds.
pub fn clamp_exit_code(code: i64) -> i32 {
    i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Namespace of the panic classes declared in
    /// `baml_std/baml/ns_panics/panics.baml`.
    const PANICS_NAMESPACE: &str = "baml.panics";

    fn int(value: i64) -> Int63 {
        Int63::new(value).expect("test value in range")
    }

    #[test]
    fn class_names_live_in_the_panics_namespace() {
        let all = [
            Panic::DivisionByZero { dividend: int(1) },
            Panic::IntegerOverflow {
                message: String::new(),
            },
            Panic::NegativeBitShift {
                message: String::new(),
            },
            Panic::UserPanic {
                message: String::new(),
            },
            Panic::AssertionFailed,
            Panic::Unreachable,
            Panic::StackOverflow,
            Panic::Exit { code: int(0) },
        ];
        for panic in all {
            assert_eq!(
                panic.class_fqn(),
                format!("{PANICS_NAMESPACE}.{}", panic.class_name())
            );
            assert!(panic.render_readable().starts_with(panic.class_fqn()));
        }
    }

    #[test]
    fn render_readable_matches_engine_output() {
        assert_eq!(
            Panic::DivisionByZero { dividend: int(42) }.render_readable(),
            "baml.panics.DivisionByZero {dividend: 42}"
        );
        assert_eq!(
            Panic::IntegerOverflow {
                message: "4611686018427387903 + 1 overflows int".into()
            }
            .render_readable(),
            r#"baml.panics.IntegerOverflow {message: "4611686018427387903 + 1 overflows int"}"#
        );
        assert_eq!(
            Panic::NegativeBitShift {
                message: "bit shift count is negative: -1".into()
            }
            .render_readable(),
            r#"baml.panics.NegativeBitShift {message: "bit shift count is negative: -1"}"#
        );
        assert_eq!(
            Panic::UserPanic {
                message: "say \"hi\"\n".into()
            }
            .render_readable(),
            r#"baml.panics.UserPanic {message: "say \"hi\"\n"}"#
        );
        assert_eq!(
            Panic::AssertionFailed.render_readable(),
            r#"baml.panics.AssertionFailed {message: "assertion failed"}"#
        );
        assert_eq!(
            Panic::Unreachable.render_readable(),
            r#"baml.panics.Unreachable {message: "unreachable code executed"}"#
        );
        assert_eq!(
            Panic::StackOverflow.render_readable(),
            r#"baml.panics.StackOverflow {message: "stack overflow"}"#
        );
        assert_eq!(
            Panic::Exit { code: int(-3) }.render_readable(),
            "baml.panics.Exit {code: -3}"
        );
    }

    #[test]
    fn display_matches_vm_panic_messages() {
        assert_eq!(
            Panic::DivisionByZero { dividend: int(7) }.to_string(),
            "division by zero: Int(7) / Int(0)"
        );
        assert_eq!(
            Panic::IntegerOverflow {
                message: "1 + 1 overflows int".into()
            }
            .to_string(),
            "integer overflow: 1 + 1 overflows int"
        );
        assert_eq!(
            Panic::NegativeBitShift {
                message: "bit shift count is negative: -1".into()
            }
            .to_string(),
            "negative bit shift: bit shift count is negative: -1"
        );
        assert_eq!(
            Panic::UserPanic {
                message: "boom".into()
            }
            .to_string(),
            "baml.sys.panic: boom"
        );
        assert_eq!(Panic::AssertionFailed.to_string(), "assertion failed");
        assert_eq!(Panic::Unreachable.to_string(), "unreachable code executed");
        assert_eq!(Panic::StackOverflow.to_string(), "stack overflow");
        assert_eq!(
            Panic::Exit { code: int(42) }.to_string(),
            "baml.sys.exit(42)"
        );
    }

    #[test]
    fn exit_codes() {
        assert_eq!(Panic::Exit { code: int(0) }.exit_code(), 0);
        assert_eq!(Panic::Exit { code: int(7) }.exit_code(), 7);
        assert_eq!(Panic::Exit { code: int(-1) }.exit_code(), -1);
        assert_eq!(Panic::Exit { code: Int63::MAX }.exit_code(), i32::MAX);
        assert_eq!(Panic::Exit { code: Int63::MIN }.exit_code(), i32::MIN);
        assert_eq!(Panic::AssertionFailed.exit_code(), 1);
        assert_eq!(Panic::DivisionByZero { dividend: int(1) }.exit_code(), 1);
        assert_eq!(
            Panic::UserPanic {
                message: String::new()
            }
            .exit_code(),
            1
        );
    }

    #[test]
    fn clamp_exit_code_saturates() {
        assert_eq!(clamp_exit_code(0), 0);
        assert_eq!(clamp_exit_code(i64::from(i32::MAX)), i32::MAX);
        assert_eq!(clamp_exit_code(i64::from(i32::MIN)), i32::MIN);
        assert_eq!(clamp_exit_code(i64::from(i32::MAX) + 1), i32::MAX);
        assert_eq!(clamp_exit_code(i64::from(i32::MIN) - 1), i32::MIN);
        assert_eq!(clamp_exit_code(i64::MAX), i32::MAX);
        assert_eq!(clamp_exit_code(i64::MIN), i32::MIN);
    }
}
