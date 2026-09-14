/// An absolute point on the global timeline, to nanosecond precision, with no
/// timezone and no calendar attached.
///
/// Equivalent to `Temporal.Instant` (TC39). `baml.time` splits absolute time
/// (`Instant`, `ZonedDateTime`) from wall-clock time (`PlainDateTime` and the
/// other `Plain` types) at the type level, where Python's `datetime` covers
/// both and separates them only by whether a `tzinfo` happens to be attached.
///
/// An `Instant` records *when*, never *where*: two observers in different
/// timezones who agree on the moment hold equal `Instant`s. Pair it with a
/// timezone via `ZonedDateTime.from_instant` to read date and time components
/// off it.
///
/// The epoch-nanosecond count is a `bigint` and is not range-checked, where
/// Temporal caps `Instant` at ±10^8 days from the epoch. Constructing one and
/// doing arithmetic on it therefore never fails; rendering one does, because
/// RFC 3339 has only four year digits (see `to_string`).
///
/// Every `from_timestamp_*` constructor takes a signed count, so a negative
/// argument names an instant before the epoch. Every `to_timestamp_*` accessor
/// but `to_timestamp_nanoseconds` is lossy, truncating toward zero rather than
/// flooring — a pre-epoch instant therefore rounds *towards* the epoch, and 500
/// nanoseconds before the epoch reads back as 0 microseconds.
class Instant {
    /// (internal) The number of nanoseconds since the UNIX epoch
    /// (1970-01-01T00:00:00Z); negative before it.
    _nanoseconds: bigint,

    /// The current point in time, read from the host's wall clock. Mirrors
    /// `Temporal.Now.instant()`.
    ///
    /// Wall-clock time is not monotonic: [NTP](https://en.wikipedia.org/wiki/Network_Time_Protocol)
    /// adjustments and manual clock changes can make a later call return an
    /// earlier `Instant`. Do not build a monotonic timer out of it.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host provides no clock. A missing
    ///   clock is a property of the platform, not of the call, so there is
    ///   nothing for a caller to handle — hence a panic rather than an error.
    function now() -> Instant throws never {
        $rust_io_function
    }

    /// The time elapsed since `self`, i.e. `Instant.now() - self`. Negative if
    /// `self` is in the future.
    ///
    /// Reads the clock through `Instant.now()` and inherits its caveats: a
    /// non-monotonic clock can hand back a negative result even for a `self`
    /// captured earlier. The measurement also absorbs the overhead of the call
    /// itself, so it is not a high-precision benchmarking tool.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host provides no clock.
    ///
    /// # Examples
    ///
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
    /// the absolute instant and then discarded — an `Instant` keeps no
    /// timezone. Zoneless strings are rejected; parse those as `PlainDateTime`,
    /// and use `ZonedDateTime.parse` when the offset or IANA identifier is
    /// worth keeping.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `s` matches none of the formats above.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let t = baml.time.Instant.parse("2026-05-27T12:00:00Z");
    /// ```
    //baml:mut_vm
    function parse(s: string) -> Instant throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes `self` to a [RFC 3339](https://datatracker.ietf.org/doc/html/rfc3339)
        /// and [ISO 8601](https://en.wikipedia.org/wiki/ISO_8601)-compatible
        /// timestamp string.
        ///
        /// Always rendered in UTC, with the `Z` suffix — an `Instant` carries
        /// no offset to render instead. Sub-second precision is preserved, and
        /// trailing subsecond zeros are truncated, so a whole second renders
        /// with no fractional part at all.
        ///
        /// # Panics
        ///
        /// - `baml.panics.UserPanic` if the year falls outside RFC 3339's
        ///   four-digit range. Only an `Instant` built by arithmetic or by
        ///   `from_timestamp_*` can get there; every parsed one is in range.
        ///   Use `to_json` where that failure needs to be handled rather than
        ///   propagated.
        ///
        /// # Examples
        ///
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

    /// (internal) RFC 3339 formatter, shared by `to_string` and `to_json` so
    /// the two agree byte for byte.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the year is outside the four-digit
    ///   RFC 3339 range, or the nanosecond count is too large to convert.
    ///   `baml.ToString.to_string` turns that into a panic, while `to_json`
    ///   surfaces it as a `baml.json.SerializationError`.
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }
    implements baml.FromJson {
        /// Decodes an `Instant` from a JSON string, accepting exactly what
        /// `Instant.parse` accepts.
        ///
        /// # Throws
        ///
        /// - `baml.json.DecodeError` if the value is not a string, or is a
        ///   string `Instant.parse` rejects.
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            match (j) {
                string => {
                    Instant.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.DecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.DecodeError { message: "Expected timestamp string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        /// Encodes `self` as the same UTC RFC 3339 string `to_string` produces.
        ///
        /// # Throws
        ///
        /// - `baml.json.SerializationError` if the year is outside RFC 3339's
        ///   four-digit range. The same failure panics in `to_string`: a
        ///   serializer is called with a caller standing by to react, so it
        ///   gets the error channel.
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.SerializationError {
                        message: message,
                        path: "",
                        reason: "unserializable",
                    }
                },
            }
        }
    }

    /// The later of `self` and `other`; `self` if they are the same instant.
    function max(self, other: Instant) -> Instant throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// The earlier of `self` and `other`; `self` if they are the same instant.
    function min(self, other: Instant) -> Instant throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// How far apart `self` and `other` are, regardless of which came first.
    ///
    /// Never negative, and commutative: swapping `self` and `other` does not
    /// change the result. Use `self - other` instead when the sign — i.e. the
    /// direction — matters.
    function abs_diff(self, other: Instant) -> Duration throws never {
        Duration { _nanoseconds: (self._nanoseconds - other._nanoseconds).abs() }
    }

    /// The `Instant` `ns` nanoseconds after the UNIX epoch.
    function from_timestamp_nanoseconds(ns: bigint) -> Instant throws never {
        Instant { _nanoseconds: ns }
    }
    /// The `Instant` `us` microseconds after the UNIX epoch.
    function from_timestamp_microseconds(us: bigint) -> Instant throws never {
        Instant { _nanoseconds: us * 1000 }
    }
    /// The `Instant` `ms` milliseconds after the UNIX epoch. This is the unit
    /// most JSON and JavaScript timestamps arrive in.
    function from_timestamp_milliseconds(ms: bigint) -> Instant throws never {
        Instant { _nanoseconds: ms * 1000000 }
    }
    /// The `Instant` `s` seconds after the UNIX epoch. This is the unit a POSIX
    /// `time_t` arrives in.
    function from_timestamp_seconds(s: bigint) -> Instant throws never {
        Instant { _nanoseconds: s * 1000000000 }
    }

    /// The number of nanoseconds since the UNIX epoch. Exact: this is the
    /// stored unit.
    function to_timestamp_nanoseconds(self) -> bigint throws never {
        self._nanoseconds
    }
    /// The number of whole microseconds since the UNIX epoch.
    function to_timestamp_microseconds(self) -> bigint throws never {
        self._nanoseconds / 1000
    }
    /// The number of whole milliseconds since the UNIX epoch.
    function to_timestamp_milliseconds(self) -> bigint throws never {
        self._nanoseconds / 1000000
    }
    /// The number of whole seconds since the UNIX epoch.
    function to_timestamp_seconds(self) -> bigint throws never {
        self._nanoseconds / 1000000000
    }

    /// The UNIX epoch itself, 1970-01-01T00:00:00Z. Equivalent to
    /// `Instant.from_timestamp_seconds(0)`.
    function epoch() -> Instant throws never {
        Instant { _nanoseconds: 0n }
    }
}

implement root.ops.Add<Duration> for Instant {
    type Output = Instant
    /// The point in time `other` after `self`.
    ///
    /// Pure absolute-time arithmetic: no timezone or calendar is consulted, so
    /// there is no DST transition to disambiguate and the result is always
    /// exact.
    function add(self, other: Duration) -> Instant {
        Instant {
            _nanoseconds: self._nanoseconds + other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Duration> for Instant {
    type Output = Instant
    /// The point in time `other` before `self`. The inverse of
    /// `Instant + Duration`.
    function sub(self, other: Duration) -> Instant {
        Instant {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Instant> for Instant {
    type Output = Duration
    /// The signed gap from `other` to `self`: positive when `self` is the later
    /// of the two, negative when it is the earlier. Use `abs_diff` when only
    /// the magnitude matters.
    function sub(self, other: Instant) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}
