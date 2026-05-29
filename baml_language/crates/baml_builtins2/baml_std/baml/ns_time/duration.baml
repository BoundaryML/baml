/// Represents a non-localized timespan.
///
/// May be negative: while `Instant` represents absolute points in time, a `Duration` represents the difference between two `Instant`s.
class Duration {
    _nanoseconds: bigint;

    /// Creates a new `Duration` representing the positive magnitude of `self`.
    function abs(self) -> Duration throws never {
        Duration { _nanoseconds: self._nanoseconds.abs() }
    }

    function from_nanoseconds(ns: bigint) -> Duration throws never {
        Duration { _nanoseconds: ns }
    }
    function from_microseconds(us: bigint) -> Duration throws never {
        Duration { _nanoseconds: us * 1000 }
    }
    function from_milliseconds(ms: bigint) -> Duration throws never {
        Duration { _nanoseconds: ms * 1000000 }
    }
    function from_seconds(s: bigint) -> Duration throws never {
        Duration { _nanoseconds: s * 1000000000 }
    }
    function from_minutes(m: bigint) -> Duration throws never {
        Duration { _nanoseconds: m * 60000000000 }
    }
    function from_hours(h: bigint) -> Duration throws never {
        Duration { _nanoseconds: h * 3600000000000 }
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
