//! The catchable panics of the language as plain data.

use std::fmt;

use crate::{Int63, clamp_exit_code};

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
    /// An array or byte-array subscript landed outside the sequence, even
    /// after counting a negative index back from the end. `index` is the
    /// index the program wrote, not the resolved offset.
    IndexOutOfBounds {
        /// The subscript as written, before negative-index resolution.
        index: Int63,
        /// The length of the sequence at the time of the access.
        length: Int63,
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
            Self::IndexOutOfBounds { .. } => "IndexOutOfBounds",
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
            Self::IndexOutOfBounds { .. } => "baml.panics.IndexOutOfBounds",
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

    /// The fields of the panic object the VM materializes, in declaration
    /// order, each value rendered as the engine prints it (strings in Rust
    /// `{:?}` form, integers in decimal).
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::DivisionByZero { dividend } => vec![("dividend", dividend.get().to_string())],
            Self::IndexOutOfBounds { index, length } => {
                vec![
                    ("index", index.get().to_string()),
                    ("length", length.get().to_string()),
                ]
            }
            Self::IntegerOverflow { message }
            | Self::NegativeBitShift { message }
            | Self::UserPanic { message } => vec![("message", format!("{message:?}"))],
            Self::AssertionFailed | Self::Unreachable | Self::StackOverflow => {
                let message = self
                    .fixed_message()
                    .expect("field-less panic variants carry a fixed message");
                vec![("message", format!("{message:?}"))]
            }
            Self::Exit { code } => vec![("code", code.get().to_string())],
        }
    }

    /// The panic object rendered exactly as the engine prints an uncaught
    /// one, e.g. `baml.panics.DivisionByZero {dividend: 42}`; see
    /// [`render_object`].
    pub fn render_readable(&self) -> String {
        render_object(self.class_fqn(), &self.fields())
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

/// A short description, for hosts that print a panic as an error. The
/// rendering the engine uses for an uncaught panic is
/// [`Panic::render_readable`].
impl fmt::Display for Panic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DivisionByZero { dividend } => {
                write!(f, "division of {} by zero", dividend.get())
            }
            Self::IndexOutOfBounds { index, length } => {
                write!(
                    f,
                    "index {} out of bounds for length {}",
                    index.get(),
                    length.get()
                )
            }
            Self::IntegerOverflow { message }
            | Self::NegativeBitShift { message }
            | Self::UserPanic { message } => f.write_str(message),
            Self::AssertionFailed | Self::Unreachable | Self::StackOverflow => f.write_str(
                self.fixed_message()
                    .expect("field-less panic variants carry a fixed message"),
            ),
            Self::Exit { code } => write!(f, "exit with code {}", code.get()),
        }
    }
}

impl std::error::Error for Panic {}

/// Render a class instance the way the engine prints an uncaught throw
/// (`BexExternalValue::render_readable` on an `Instance`): `Class {field:
/// value, ...}` with no space inside the braces, the fields in declaration
/// order, each value already rendered.
pub fn render_object(class_fqn: &str, fields: &[(&str, String)]) -> String {
    let mut out = String::from(class_fqn);
    out.push_str(" {");
    for (i, (name, value)) in fields.iter().enumerate() {
        if i != 0 {
            out.push_str(", ");
        }
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
    }
    out.push('}');
    out
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
            Panic::IndexOutOfBounds {
                index: int(5),
                length: int(3),
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
            Panic::IndexOutOfBounds {
                index: int(-4),
                length: int(3)
            }
            .render_readable(),
            "baml.panics.IndexOutOfBounds {index: -4, length: 3}"
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
    fn display_is_a_short_description() {
        assert_eq!(
            Panic::DivisionByZero { dividend: int(7) }.to_string(),
            "division of 7 by zero"
        );
        assert_eq!(
            Panic::IntegerOverflow {
                message: "1 + 1 overflows int".into()
            }
            .to_string(),
            "1 + 1 overflows int"
        );
        assert_eq!(
            Panic::IndexOutOfBounds {
                index: int(5),
                length: int(3)
            }
            .to_string(),
            "index 5 out of bounds for length 3"
        );
        assert_eq!(Panic::AssertionFailed.to_string(), "assertion failed");
        assert_eq!(Panic::StackOverflow.to_string(), "stack overflow");
        assert_eq!(
            Panic::Exit { code: int(42) }.to_string(),
            "exit with code 42"
        );
    }

    #[test]
    fn render_object_handles_empty_and_many_fields() {
        assert_eq!(render_object("a.B", &[]), "a.B {}");
        assert_eq!(
            render_object("a.B", &[("x", "1".into()), ("y", "\"s\"".into())]),
            "a.B {x: 1, y: \"s\"}"
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
}
