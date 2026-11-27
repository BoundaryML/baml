//! Why a native that does I/O failed, carried out of its job as plain
//! Rust and made into the error the standard library declares once the
//! native is running again.

use std::time::Duration;

use num_bigint::BigInt;

use super::{Scope, Thrown};
use crate::layout::NAMED_TIMEOUT;

/// The field of `baml.errors.Timeout` that holds the limit that passed, a
/// `baml.time.Duration` or `null`, by position.
const TIMEOUT_LIMIT: usize = 1;

/// An error of `baml.errors`, before it is an object.
pub(super) enum Failure {
    Io(String),
    InvalidArgument(String),
    /// `baml.errors.Timeout`: its message, the limit that passed, and
    /// which limit that was.
    Timeout {
        message: String,
        limit: Option<Duration>,
        which: &'static str,
    },
}

impl Failure {
    /// An `Io` whose message is `context`, a colon and `error`.
    pub(super) fn io(context: impl std::fmt::Display, error: impl std::fmt::Display) -> Self {
        Self::Io(format!("{context}: {error}"))
    }

    /// The error to throw. A `Timeout` is built only by a native whose
    /// declaration names it.
    pub(super) fn throw<'s>(self, scope: &mut Scope<'s>) -> Thrown<'s> {
        let (message, limit, which) = match self {
            Self::Io(message) => return scope.io_error(&message),
            Self::InvalidArgument(message) => return scope.invalid_argument(&message),
            Self::Timeout {
                message,
                limit,
                which,
            } => (message, limit, which),
        };
        let class = scope.named_class(NAMED_TIMEOUT);
        let message = scope.alloc_str(&message);
        let which = scope.alloc_str(which);
        let limit = limit.map(|limit| {
            let duration = class
                .field_class(TIMEOUT_LIMIT)
                .expect("a timeout's limit is of a class");
            let nanos = scope.make_big(&BigInt::from(limit.as_nanos()));
            let mut built = scope.build(duration);
            built.set(scope, 0, nanos);
            built.finish()
        });
        let mut thrown = scope.build(class);
        thrown.set(scope, 0, message);
        thrown.set(scope, TIMEOUT_LIMIT, limit);
        thrown.set(scope, 2, which);
        let thrown = thrown.finish();
        scope.throw(thrown)
    }
}

/// A limit from the nanoseconds the standard library passes one as: none
/// when zero or negative, and the longest there is when past it.
pub(super) fn limit(nanos: &BigInt) -> Option<Duration> {
    (nanos.sign() == num_bigint::Sign::Plus)
        .then(|| Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
}
