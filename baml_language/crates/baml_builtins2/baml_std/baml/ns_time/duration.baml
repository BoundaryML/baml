/// Represents a non-localized timespan.
///
/// May be negative: while `Instant` represents absolute points in time, a `Duration` represents the difference between two `Instant`s.
class Duration {
    /// (internal)
    _nanoseconds: bigint,

    /// Creates a new `Duration` representing the positive magnitude of `self`.
    function abs(self) -> Duration throws never {
        Duration { _nanoseconds: self._nanoseconds.abs() }
    }

    /// Creates a new duration of `ns` nanoseconds.
    function from_nanoseconds(ns: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (ns) {
                let value: bigint => value,
                let value: int => 0n + value,
            },
        }
    }
    /// Creates a new duration of `us` microseconds.
    function from_microseconds(us: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (us) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000n,
        }
    }
    /// Creates a new duration of `ms` milliseconds.
    function from_milliseconds(ms: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (ms) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000000n,
        }
    }
    /// Creates a new duration of `s` seconds.
    function from_seconds(s: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (s) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 1000000000n,
        }
    }
    /// Creates a new duration of `m` minutes.
    function from_minutes(m: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (m) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 60000000000n,
        }
    }
    /// Creates a new duration of `h` hours.
    function from_hours(h: int | bigint) -> Duration throws never {
        Duration {
            _nanoseconds: match (h) {
                let value: bigint => value,
                let value: int => 0n + value,
            }
                * 3600000000000n,
        }
    }

    /// Returns the number of nanoseconds in `self`.
    function to_nanoseconds(self) -> bigint throws never {
        self._nanoseconds
    }
    /// Returns the number of microseconds in `self`.
    /// Lossy: rounds down
    function to_microseconds(self) -> bigint throws never {
        self._nanoseconds / 1000
    }
    /// Returns the number of milliseconds in `self`.
    /// Lossy: rounds down
    function to_milliseconds(self) -> bigint throws never {
        self._nanoseconds / 1000000
    }
    /// Returns the number of seconds in `self`.
    /// Lossy: rounds down
    function to_seconds(self) -> bigint throws never {
        self._nanoseconds / 1000000000
    }
    /// Returns the number of minutes in `self`.
    /// Lossy: rounds down
    function to_minutes(self) -> bigint throws never {
        self._nanoseconds / 60000000000
    }
    /// Returns the number of hours in `self`.
    /// Lossy: rounds down
    function to_hours(self) -> bigint throws never {
        self._nanoseconds / 3600000000000
    }
}

implement root.ops.Add<Duration> for Duration {
    type Output = Duration
    /// Adds two `Duration`s, returning a new `Duration`.
    function add(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds + other._nanoseconds,
        }
    }
}

implement root.ops.Subtract<Duration> for Duration {
    type Output = Duration
    /// Subtracts a `Duration` from this `Duration`, returning a new `Duration`.
    function sub(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
        }
    }
}

implement root.ops.Multiply<int> for Duration {
    type Output = Duration
    /// Multiplies this `Duration` by an `int`, returning a new `Duration`.
    function mul(self, other: int) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds * other,
        }
    }
}

implement root.ops.Multiply<bigint> for Duration {
    type Output = Duration
    /// Multiplies this `Duration` by a `bigint`, returning a new `Duration`.
    function mul(self, other: bigint) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds * other,
        }
    }
}

implement root.ops.Divide<int> for Duration {
    type Output = Duration
    /// Divides this `Duration` by an `int`, returning a new `Duration`.
    function div(self, other: int) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds / other,
        }
    }
}

implement root.ops.Divide<bigint> for Duration {
    type Output = Duration
    /// Divides this `Duration` by a `bigint`, returning a new `Duration`.
    function div(self, other: bigint) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds / other,
        }
    }
}

implement root.ops.Remainder<Duration> for Duration {
    type Output = Duration
    /// Returns the remainder of this `Duration` divided by another `Duration`.
    function rem(self, other: Duration) -> Duration throws never {
        Duration {
            _nanoseconds: self._nanoseconds % other._nanoseconds,
        }
    }
}

implement root.ops.Negate for Duration {
    type Output = Duration
    /// Negates this `Duration`, returning a new `Duration`.
    function neg(self) -> Duration throws never {
        Duration {
            _nanoseconds: -self._nanoseconds
        }
    }
}
