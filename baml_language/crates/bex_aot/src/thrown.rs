//! The value a BAML `throw` carries, as generated native code sees it.
//!
//! The VM throws heap objects; a native program has no heap of BAML objects,
//! so a thrown value is either a [`Panic`] payload or a Rust value standing in
//! for a class instance ([`ErrorObject`]). Every generated function returns
//! `Result<T, Thrown>`, and a `catch` handler receives the `Thrown` itself:
//! it tests the class by name ([`is_class`], [`class_fqn`]), tells a panic
//! from an error ([`is_panic`]) and recovers the instance a user class was
//! thrown as ([`downcast`]).

use std::{any::Any, fmt};

pub use bex_lang::render_object;

use crate::{Error, Panic, errors::InvalidArgument, handle::Shared, readable::Readable};

/// A thrown class instance: a `baml.errors.*` / `baml.json.*` stdlib error or
/// a user class. The stdlib errors this crate raises itself live in
/// [`crate::errors`] and [`crate::json`]; a generated class implements
/// [`ErrorClass`] and is thrown as its `Shared` handle through the blanket
/// impl below, so a `catch` binding sees the same object the `throw` did.
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

    /// The object as `Any`, so a `catch` binding can recover its concrete
    /// handle ([`downcast`]).
    fn as_any(&self) -> &dyn Any;

    /// A copy of the object: for a handle, the same object.
    fn clone_box(&self) -> Box<dyn ErrorObject>;
}

/// A generated class as a throwable: its name and its fields rendered the
/// way the engine prints an uncaught throw. Thrown as `Shared<Self>`.
pub trait ErrorClass {
    /// Fully qualified class name, e.g. `"user.Invalid"`.
    const CLASS_FQN: &'static str;

    /// The fields in declaration order, each rendered with [`Readable`].
    fn readable_fields(&self) -> Vec<(&'static str, String)>;
}

impl<T: ErrorClass + fmt::Debug + 'static> ErrorObject for Shared<T> {
    fn class_fqn(&self) -> &'static str {
        T::CLASS_FQN
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        self.borrow().readable_fields()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn ErrorObject> {
        Box::new(self.clone())
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

impl Clone for Thrown {
    fn clone(&self) -> Self {
        match self {
            Self::Panic(panic) => Self::Panic(panic.clone()),
            Self::Error(error) => Self::Error(error.clone_box()),
        }
    }
}

/// The namespace of the panic classes: the VM decides whether a caught
/// value is a panic by its class, so a `baml.panics.*` instance a program
/// constructs and throws itself is one too.
const PANICS_NAMESPACE: &str = "baml.panics.";

/// The clean-termination panic, whose `code` is the process exit code.
const EXIT_CLASS: &str = "baml.panics.Exit";

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
    /// themselves ([`Panic::exit_code`]); a `baml.panics.Exit { code }` the
    /// program built and threw itself exits with its code, as the engine
    /// recognizes that class at its boundary (`extract_exit_code`); every
    /// other thrown error exits with `1`.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Panic(panic) => panic.exit_code(),
            Self::Error(error) if error.class_fqn() == EXIT_CLASS => error
                .fields()
                .iter()
                .find(|(name, _)| *name == "code")
                .and_then(|(_, code)| code.parse::<i64>().ok())
                .map_or(1, bex_lang::clamp_exit_code),
            Self::Error(_) => 1,
        }
    }

    /// Whether the thrown object is a `baml.panics.*` instance: what the
    /// VM's `throw_if_panic` guard in front of a wildcard `catch` arm lets
    /// through.
    pub fn is_panic(&self) -> bool {
        match self {
            Self::Panic(_) => true,
            Self::Error(error) => error.class_fqn().starts_with(PANICS_NAMESPACE),
        }
    }
}

/// Whether `thrown` is an instance of the class `fqn`: a `catch` arm's class
/// test. The VM compares class identity; two classes never share a fully
/// qualified name, so the name decides.
#[inline]
pub fn is_class(thrown: &Thrown, fqn: &str) -> bool {
    thrown.class_fqn() == fqn
}

/// Fully qualified class name of the thrown object: the `type_tag` a
/// multi-arm `catch` switches on.
#[inline]
pub fn class_fqn(thrown: &Thrown) -> &'static str {
    thrown.class_fqn()
}

/// See [`Thrown::is_panic`].
#[inline]
pub fn is_panic(thrown: &Thrown) -> bool {
    thrown.is_panic()
}

/// The instance a user class was thrown as, when `thrown` holds a `T`: a
/// `catch` binding of that class (`let e: Invalid => ..`). A panic or an
/// instance of another class is `None`.
pub fn downcast<T: Clone + 'static>(thrown: &Thrown) -> Option<T> {
    match thrown {
        Thrown::Panic(_) => None,
        Thrown::Error(error) => error.as_any().downcast_ref::<T>().cloned(),
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

/// A thrown object nested in another's field renders as it would at the top
/// level (`BexExternalValue::render_readable` recurses).
impl Readable for Thrown {
    fn readable(&self) -> String {
        self.render_readable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Int63, Str, handle::shared};

    #[derive(Debug, Clone)]
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
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn clone_box(&self) -> Box<dyn ErrorObject> {
            Box::new(self.clone())
        }
    }

    /// What the backend generates for `class Coded { code: int, message: string }`.
    #[derive(Debug)]
    struct Coded {
        code: Int63,
        message: Str,
    }

    impl ErrorClass for Coded {
        const CLASS_FQN: &'static str = "user.Coded";
        fn readable_fields(&self) -> Vec<(&'static str, String)> {
            vec![
                ("code", self.code.readable()),
                ("message", self.message.readable()),
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
        assert!(thrown.is_panic());
        assert!(is_class(&thrown, "baml.panics.AssertionFailed"));
        assert!(!is_class(&thrown, "baml.panics.Unreachable"));
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
        assert!(!thrown.is_panic());
        assert_eq!(thrown.clone().render_readable(), thrown.render_readable());
    }

    #[test]
    fn a_thrown_handle_is_recovered_by_downcast() {
        let object = shared(Coded {
            code: Int63::new(3).unwrap(),
            message: Str::from("m".to_string()),
        });
        let thrown = Thrown::error(object.clone());
        assert_eq!(class_fqn(&thrown), "user.Coded");
        assert_eq!(
            thrown.render_readable(),
            r#"user.Coded {code: 3, message: "m"}"#
        );
        let caught: Shared<Coded> = downcast(&thrown).expect("the thrown class");
        assert!(crate::handle::ptr_eq(&caught, &object), "same object");
        let copy = thrown.clone();
        let again: Shared<Coded> = downcast(&copy).expect("a clone keeps the handle");
        assert!(crate::handle::ptr_eq(&again, &object));
        assert!(downcast::<Shared<Coded>>(&Thrown::from(Panic::Unreachable)).is_none());
        assert!(downcast::<Str>(&thrown).is_none(), "another type");
    }

    #[test]
    fn a_program_built_panic_instance_counts_as_a_panic() {
        #[derive(Debug)]
        struct Exit {
            code: Int63,
        }
        impl ErrorClass for Exit {
            const CLASS_FQN: &'static str = "baml.panics.Exit";
            fn readable_fields(&self) -> Vec<(&'static str, String)> {
                vec![("code", self.code.readable())]
            }
        }
        let thrown = Thrown::error(shared(Exit {
            code: Int63::new(2).unwrap(),
        }));
        assert!(is_panic(&thrown));
        assert_eq!(thrown.render_readable(), "baml.panics.Exit {code: 2}");
        assert_eq!(
            thrown.exit_code(),
            2,
            "exits with its code, as on the engine"
        );
        let wide = Thrown::error(shared(Exit { code: Int63::MIN }));
        assert_eq!(wide.exit_code(), i32::MIN);
    }
}
