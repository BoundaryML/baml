/// Represents an absolute point in time.
class Instant {
    /// Internal representation: the number of nanoseconds since the UNIX epoch.
    _nanoseconds: bigint,

    /// Creates a new `Instant` representing the current point in time.
    ///
    /// Note that this uses wall-clock time and is not guaranteed to be monotonic
    /// (e.g. [NTP](https://en.wikipedia.org/wiki/Network_Time_Protocol) adjustments may cause time to jump backwards).
    ///
    /// ## Panics
    /// If the system clock is not available, this function will panic.
    function now() -> Instant throws never {
        $rust_io_function
    }
    /// Creates a new `Duration` representing the time elapsed since `self`.
    /// If `time` is in the future, the duration will be negative.
    ///
    /// Uses `Instant.now()` to get the current time. See it for caveats.
    ///
    /// Also note that since this operation has limited accuracy
    /// due to measurement overhead, it is not recommended to use this
    /// for high-precision measurements.
    ///
    /// ## Panics
    /// If the system clock is not available, this function will panic.
    ///
    /// ## Examples
    /// ```baml
    /// let start = Instant.now();
    /// let response = baml.http.fetch("https://example.com");
    /// let duration = start.elapsed();
    /// ```
    function elapsed(self) -> Duration throws never {
        Duration { _nanoseconds: Instant.now()._nanoseconds - self._nanoseconds }
    }

    /// Parses a timestamp string into an `Instant`.
    ///
    /// The following formats are tried in order; the first that matches wins:
    /// 1. [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) — e.g. `"2026-05-27T12:00:00Z"`, `"2026-05-27T12:00:00.5+02:00"`
    /// 2. [ISO 8601](https://en.wikipedia.org/wiki/ISO_8601) — including the basic (separator-less) form, e.g. `"20260527T120000Z"`
    /// 3. [RFC 2822](https://www.rfc-editor.org/rfc/rfc2822) — e.g. `"Tue, 01 Jul 2003 10:52:37 +0200"`
    ///
    /// Every accepted format carries a UTC offset, which is applied to produce
    /// the absolute instant.
    ///
    /// Throws `root.errors.ParseError` if `s` matches none of the formats above.
    ///
    /// ## Examples
    /// ```baml
    /// let t = baml.time.Instant.parse("2026-05-27T12:00:00Z");
    /// ```
    //baml:mut_vm
    function parse(s: string) -> Instant throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes `self` to a standard [RFC 3339](https://datatracker.ietf.org/doc/html/rfc3339)
        /// and [ISO 8601](https://en.wikipedia.org/wiki/ISO_8601)-compatible timestamp string.
        /// Will always be in UTC.
        ///
        /// Sub-second precision is supported, but trailing subsecond zeros may be truncated.
        ///
        /// ## Panics
        /// - If the year would be more than 4 digits (outside the RFC 3339 range).
        ///
        /// ## Examples
        /// ```baml
        /// let t = baml.time.Instant.epoch();
        /// assert.equal(t.to_string(), "1970-01-01T00:00:00Z")
        /// ```
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    root.sys.panic("Instant.to_string: " + message)
                },
            }
        }
    }

    /// Internal RFC 3339 formatter. Throws `InvalidArgument` when the year is
    /// outside the 4-digit RFC 3339 range; `baml.ToString.to_string` turns that
    /// into a panic, while `to_json` surfaces it as a `JsonSerializationError`.
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            match (j) {
                string => {
                    Instant.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.JsonDecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.JsonDecodeError { message: "Expected timestamp string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.JsonSerializationError { message: message, path: "" }
                },
            }
        }
    }

    /// If `self` is before `other`, returns `other`. Otherwise, returns `self`.
    function max(self, other: Instant) -> Instant throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// If `self` is after `other`, returns `other`. Otherwise, returns `self`.
    function min(self, other: Instant) -> Instant throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// Returns the absolute value of the difference between `self` and `other`.
    /// This operation is commutative: swapping `self` and `other` does not change the result.
    function abs_diff(self, other: Instant) -> Duration throws never {
        Duration { _nanoseconds: (self._nanoseconds - other._nanoseconds).abs() }
    }

    /// Creates a new `Instant` from a number of nanoseconds since the UNIX epoch.
    function from_timestamp_nanoseconds(ns: bigint) -> Instant throws never {
        Instant { _nanoseconds: ns }
    }
    /// Creates a new `Instant` from a number of microseconds since the UNIX epoch.
    function from_timestamp_microseconds(us: bigint) -> Instant throws never {
        Instant { _nanoseconds: us * 1000 }
    }
    /// Creates a new `Instant` from a number of milliseconds since the UNIX epoch.
    function from_timestamp_milliseconds(ms: bigint) -> Instant throws never {
        Instant { _nanoseconds: ms * 1000000 }
    }
    /// Creates a new `Instant` from a number of seconds since the UNIX epoch.
    function from_timestamp_seconds(s: bigint) -> Instant throws never {
        Instant { _nanoseconds: s * 1000000000 }
    }

    /// Returns the number of nanoseconds since the UNIX epoch.
    function to_timestamp_nanoseconds(self) -> bigint throws never {
        self._nanoseconds
    }
    /// Returns the number of microseconds since the UNIX epoch.
    /// Lossy: rounds down
    function to_timestamp_microseconds(self) -> bigint throws never {
        self._nanoseconds / 1000
    }
    /// Returns the number of milliseconds since the UNIX epoch.
    /// Lossy: rounds down
    function to_timestamp_milliseconds(self) -> bigint throws never {
        self._nanoseconds / 1000000
    }
    /// Returns the number of seconds since the UNIX epoch.
    /// Lossy: rounds down
    function to_timestamp_seconds(self) -> bigint throws never {
        self._nanoseconds / 1000000000
    }

    /// Returns the Unix epoch (1970-01-01T00:00:00Z).
    function epoch() -> Instant throws never {
        Instant { _nanoseconds: 0n }
    }
}

implement root.ops.Add<Duration> for Instant {
    type Output = Instant
    function add(self, other: Duration) -> Instant {
        Instant {
            _nanoseconds: self._nanoseconds + other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Duration> for Instant {
    type Output = Instant
    function sub(self, other: Duration) -> Instant {
        Instant {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Instant> for Instant {
    type Output = Duration
    function sub(self, other: Instant) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}
