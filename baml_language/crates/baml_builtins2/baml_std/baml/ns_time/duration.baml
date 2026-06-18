function _duration_amount_to_bigint(amount: int | bigint) -> bigint throws never {
    match (amount) {
        let value: bigint => value,
        let value: int => 0n + value,
    }
}

/// Represents a non-localized timespan.
///
/// May be negative: while `Instant` represents absolute points in time, a `Duration` represents the difference between two `Instant`s.
class Duration {
    _nanoseconds: bigint;

    /// Creates a new `Duration` representing the positive magnitude of `self`.
    function abs(self) -> Duration throws never {
        Duration { _nanoseconds: self._nanoseconds.abs() }
    }

    function from_nanoseconds(ns: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(ns) }
    }
    function from_microseconds(us: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(us) * 1000n }
    }
    function from_milliseconds(ms: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(ms) * 1000000n }
    }
    function from_seconds(s: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(s) * 1000000000n }
    }
    function from_minutes(m: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(m) * 60000000000n }
    }
    function from_hours(h: int | bigint) -> Duration throws never {
        Duration { _nanoseconds: _duration_amount_to_bigint(h) * 3600000000000n }
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
