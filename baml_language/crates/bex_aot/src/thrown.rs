//! The value a BAML `throw` carries, as generated native code sees it.
//!
//! The VM throws heap objects; a native program has no heap of BAML objects,
//! so a thrown value is either a [`Panic`] payload or a Rust struct standing in
//! for a class instance ([`ErrorObject`]). Every generated function returns
//! `Result<T, Thrown>`.

use std::fmt;

pub use bex_lang::render_object;

use crate::{Error, Panic, errors::InvalidArgument};

/// A thrown class instance: a `baml.errors.*` / `baml.json.*` stdlib error or
/// a user class. Generated code implements this for each class a program can
/// throw; the stdlib errors this crate raises itself live in [`crate::errors`]
/// and [`crate::json`].
pub trait ErrorObject: fmt::Debug {
    /// Fully qualified class name, e.g. `"baml.json.ParseError"`.
    fn class_fqn(&self) -> &'static str;

    /// The fields in declaration order, each value already rendered (strings
    /// in `{:?}` form, integers in decimal).
    fn fields(&self) -> Vec<(&'static str, String)>;

    /// The object rendered the way the engine prints an uncaught throw:
    /// `Class {field: value, ...}`, see [`render_object`].
    fn render_readable(&self) -> String {
        render_object(self.class_fqn(), &self.fields())
    }
}

/// Everything a BAML `throw` (or a panic) can unwind with.
#[derive(Debug)]
pub enum Thrown {
    /// A `baml.panics.*` instance.
    Panic(Panic),
    /// A thrown class instance.
    Error(Box<dyn ErrorObject>),
}

impl Thrown {
    /// Wrap a class instance.
    pub fn error(object: impl ErrorObject + 'static) -> Self {
        Self::Error(Box::new(object))
    }

    /// Fully qualified class name of the thrown object.
    pub fn class_fqn(&self) -> &'static str {
        match self {
            Self::Panic(panic) => panic.class_fqn(),
            Self::Error(error) => error.class_fqn(),
        }
    }

    /// The thrown object's fields, rendered; see [`ErrorObject::fields`].
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Panic(panic) => panic.fields(),
            Self::Error(error) => error.fields(),
        }
    }

    /// The thrown object rendered exactly as the engine prints an uncaught one
    /// (`BexExternalValue::render_readable`): `Class {field: value, ...}` with
    /// strings in Rust `{:?}` form. The host prints
    /// `uncaught throw: {render_readable}`.
    pub fn render_readable(&self) -> String {
        match self {
            Self::Panic(panic) => panic.render_readable(),
            Self::Error(error) => error.render_readable(),
        }
    }

    /// Process exit code for an uncaught instance: panics decide for
    /// themselves ([`Panic::exit_code`]); every thrown error exits with `1`.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Panic(panic) => panic.exit_code(),
            Self::Error(_) => 1,
        }
    }
}

impl From<Panic> for Thrown {
    fn from(panic: Panic) -> Self {
        Self::Panic(panic)
    }
}

/// A shared builtin's failure: the panic as is, an `InvalidArgument` as its
/// class instance.
impl From<Error> for Thrown {
    fn from(error: Error) -> Self {
        match error {
            Error::Panic(panic) => Self::Panic(panic),
            Error::InvalidArgument { message } => Self::error(InvalidArgument { message }),
        }
    }
}

impl fmt::Display for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panic(panic) => fmt::Display::fmt(panic, f),
            Self::Error(error) => f.write_str(&error.render_readable()),
        }
    }
}

impl std::error::Error for Thrown {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Int63;

    #[derive(Debug)]
    struct Custom {
        message: String,
        code: Int63,
    }

    impl ErrorObject for Custom {
        fn class_fqn(&self) -> &'static str {
            "app.Custom"
        }
        fn fields(&self) -> Vec<(&'static str, String)> {
            vec![
                ("message", format!("{:?}", self.message)),
                ("code", self.code.get().to_string()),
            ]
        }
    }

    #[test]
    fn panics_lift_into_thrown() {
        let thrown = Thrown::from(Panic::AssertionFailed);
        assert_eq!(thrown.class_fqn(), "baml.panics.AssertionFailed");
        assert_eq!(
            thrown.render_readable(),
            r#"baml.panics.AssertionFailed {message: "assertion failed"}"#
        );
        assert_eq!(thrown.to_string(), "assertion failed");
        assert_eq!(thrown.exit_code(), 1);
        let exit = Thrown::from(Panic::Exit {
            code: Int63::new(7).unwrap(),
        });
        assert_eq!(exit.exit_code(), 7);
    }

    #[test]
    fn errors_render_like_the_engine() {
        let thrown = Thrown::error(Custom {
            message: "say \"hi\"".into(),
            code: Int63::new(-2).unwrap(),
        });
        assert_eq!(thrown.class_fqn(), "app.Custom");
        assert_eq!(
            thrown.render_readable(),
            r#"app.Custom {message: "say \"hi\"", code: -2}"#
        );
        assert_eq!(thrown.to_string(), thrown.render_readable());
        assert_eq!(thrown.exit_code(), 1);
    }
}
