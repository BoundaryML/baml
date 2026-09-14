/// An exact, signed timespan: a count of nanoseconds, with no calendar
/// attached.
///
/// Where an `Instant` names an absolute point on the timeline, a `Duration` is
/// the gap between two of them, so it may be negative — `a - b` is negative
/// exactly when `a` precedes `b`. The count is a `bigint`, so every `from_*`
/// constructor scales in exact arbitrary-precision arithmetic, accepts a
/// negative argument, and cannot overflow.
///
/// Deliberately narrower than `Temporal.Duration` (TC39): there are no
/// year/month/week/day fields, only nanoseconds. Calendar units are the part of
/// Temporal's duration model that cannot be interpreted without a reference
/// point — a month is 28 to 31 days, and a day is not 24 hours on the dates a
/// zone shifts. Omitting them keeps every `Duration` operation exact. The
/// consequence: adding a 24-hour `Duration` to a `ZonedDateTime` advances the
/// absolute time by exactly 24 hours, which is not "the same wall-clock time
/// tomorrow" across a DST transition.
///
/// Every `to_*` accessor below is lossy except `to_nanoseconds`. Each truncates
/// toward zero rather than flooring, so a negative `Duration` rounds up: -1500
/// nanoseconds reads back as -1 microsecond, not -2.
class Duration {
    /// (internal) The length of the timespan in nanoseconds. May be negative.
    _nanoseconds: bigint,

    /// The magnitude of `self`, with the sign discarded.
    function abs(self) -> Duration throws never {
        Duration { _nanoseconds: self._nanoseconds.abs() }
    }

    /// A timespan of `ns` nanoseconds.
    function from_nanoseconds(ns: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (ns) {
                let value: bigint => value,
                let value: int => 0n + value,
            },
        }
    }
    /// A timespan of `us` microseconds, i.e. `us * 1000` nanoseconds.
    function from_microseconds(us: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (us) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000n,
        }
    }
    /// A timespan of `ms` milliseconds, i.e. `ms * 1000000` nanoseconds.
    function from_milliseconds(ms: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (ms) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000000n,
        }
    }
    /// A timespan of `s` seconds, i.e. `s * 1000000000` nanoseconds.
    function from_seconds(s: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (s) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000000000n,
        }
    }
    /// A timespan of `m` minutes, i.e. `m * 60` seconds.
    function from_minutes(m: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (m) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 60000000000n,
        }
    }
    /// A timespan of `h` hours, i.e. `h * 3600` seconds.
    ///
    /// A fixed 3600-second hour, never a calendar hour: it does not absorb a
    /// DST transition. See the note on `Duration`.
    function from_hours(h: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (h) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 3600000000000n,
        }
    }

    /// The length of `self` in nanoseconds. Exact: this is the stored unit.
    function to_nanoseconds(self) -> bigint throws never {
        self._nanoseconds
    }
    /// The length of `self` in whole microseconds.
    function to_microseconds(self) -> bigint throws never {
        self._nanoseconds / 1000
    }
    /// The length of `self` in whole milliseconds.
    function to_milliseconds(self) -> bigint throws never {
        self._nanoseconds / 1000000
    }
    /// The length of `self` in whole seconds.
    function to_seconds(self) -> bigint throws never {
        self._nanoseconds / 1000000000
    }
    /// The length of `self` in whole minutes.
    function to_minutes(self) -> bigint throws never {
        self._nanoseconds / 60000000000
    }
    /// The length of `self` in whole hours, each a fixed 3600 seconds.
    function to_hours(self) -> bigint throws never {
        self._nanoseconds / 3600000000000
    }
}

implement root.ops.Add<Duration> for Duration {
    type Output = Duration
    /// The sum of two timespans. Exact, and cannot overflow.
    function add(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds + other._nanoseconds,
        }
    }
}

implement root.ops.Subtract<Duration> for Duration {
    type Output = Duration
    /// The difference between two timespans. Negative when `other` is the
    /// longer of the two.
    function sub(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
        }
    }
}

implement root.ops.Multiply<int> for Duration {
    type Output = Duration
    /// Scales `self` by an integer factor. A negative factor also flips the
    /// sign of the timespan.
    function mul(self, other: int) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds * other,
        }
    }
}

implement root.ops.Multiply<bigint> for Duration {
    type Output = Duration
    /// Scales `self` by an arbitrarily large integer factor. A negative factor
    /// also flips the sign of the timespan.
    function mul(self, other: bigint) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds * other,
        }
    }
}

implement root.ops.Divide<int> for Duration {
    type Output = Duration
    /// Splits `self` into `other` equal parts, returning one of them.
    ///
    /// The nanosecond count truncates toward zero, so the parts may not sum
    /// back to `self`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.DivisionByZero` if `other` is zero.
    function div(self, other: int) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds / other,
        }
    }
}

implement root.ops.Divide<bigint> for Duration {
    type Output = Duration
    /// Splits `self` into `other` equal parts, returning one of them.
    ///
    /// The nanosecond count truncates toward zero, so the parts may not sum
    /// back to `self`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.DivisionByZero` if `other` is zero.
    function div(self, other: bigint) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds / other,
        }
    }
}

implement root.ops.Remainder<Duration> for Duration {
    type Output = Duration
    /// What is left of `self` after removing every whole multiple of `other`.
    /// The sub-minute part of a timespan is `d % Duration.from_minutes(1)`.
    ///
    /// Division truncates toward zero, so the result carries the sign of
    /// `self`, not of `other`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.DivisionByZero` if `other` is zero.
    function rem(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds % other._nanoseconds,
        }
    }
}

implement root.ops.Negate for Duration {
    type Output = Duration
    /// The same timespan in the opposite direction.
    function neg(self) -> Duration throws never {
        Duration {
            _nanoseconds: -self._nanoseconds
        }
    }
}
