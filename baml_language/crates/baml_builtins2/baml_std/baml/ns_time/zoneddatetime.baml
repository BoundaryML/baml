/// A timezone-aware point in time: an absolute instant plus a timezone —
/// either a fixed offset (`-07:00`) or an IANA identifier
/// (`America/Los_Angeles`).
///
/// The timezone does not affect the absolute time; it affects the
/// interpretation of date/time components and string formatting. The
/// internal representation is an absolute time (like `Instant`) rather than
/// calendar components, so values are unambiguous across DST transitions.
///
/// Equivalent to `Temporal.ZonedDateTime` (TC39).
class ZonedDateTime {
    /// Internal representation: nanoseconds since the Unix epoch (an inlined
    /// `Instant`).
    _nanoseconds: bigint,
    /// Internal: the fixed offset in nanoseconds, if the timezone is a fixed
    /// offset. Invariant: exactly one of `_offset_ns` / `_iana` is non-null.
    _offset_ns: int?,
    /// Internal: the IANA identifier, if the timezone is one.
    _iana: string?,

    /// Creates a new `ZonedDateTime` with the current time in the system
    /// timezone (an IANA identifier, e.g. `"America/Los_Angeles"`).
    /// Mirrors `Temporal.Now.zonedDateTimeISO()`.
    function now() -> ZonedDateTime throws root.errors.Io {
        ZonedDateTime.from_instant(Instant.now(), system_timezone())
    }
    /// Creates a new `ZonedDateTime` with the current time in the given
    /// timezone.
    function now_in(timezone: TimeZoneOffset | string) -> ZonedDateTime throws never {
        ZonedDateTime.from_instant(Instant.now(), timezone)
    }

    /// Pairs an absolute time with a timezone.
    function from_instant(
        instant: Instant,
        timezone: TimeZoneOffset | string,
    ) -> ZonedDateTime throws never {
        match (timezone) {
            TimeZoneOffset => {
                ZonedDateTime {
                    _nanoseconds: instant._nanoseconds,
                    _offset_ns: timezone._nanoseconds,
                    _iana: null,
                }
            },
            string => {
                ZonedDateTime {
                    _nanoseconds: instant._nanoseconds,
                    _offset_ns: null,
                    _iana: timezone,
                }
            },
        }
    }
    /// The absolute time, dropping the timezone.
    function to_instant(self) -> Instant throws never {
        Instant { _nanoseconds: self._nanoseconds }
    }

    /// Creates a `ZonedDateTime` from calendar/clock components read in
    /// `timezone`. `month` and `day` are 1-based. Defaulted clock components
    /// and `disambiguation` are passed by name. With an IANA timezone, DST
    /// gaps/overlaps are resolved per `disambiguation` (see `Disambiguation`).
    function from_components(
        timezone: TimeZoneOffset | string,
        year: int,
        month: int,
        day: int,
        hour: int = 0,
        minute: int = 0,
        second: int = 0,
        millisecond: int = 0,
        microsecond: int = 0,
        nanosecond: int = 0,
        disambiguation: Disambiguation = "compatible",
    ) -> ZonedDateTime throws root.errors.InvalidArgument | UnknownTimezoneError | AmbiguousTimeError | root.errors.Io {
        PlainDateTime
            .from_components(
                year,
                month,
                day,
                hour = hour,
                minute = minute,
                second = second,
                millisecond = millisecond,
                microsecond = microsecond,
                nanosecond = nanosecond,
            )
            .to_zoned(timezone, disambiguation = disambiguation)
    }

    /// Parses [RFC 3339](https://datatracker.ietf.org/doc/html/rfc3339)
    /// (`2026-03-18T13:04:27-07:00`) and
    /// [RFC 9557](https://datatracker.ietf.org/doc/html/rfc9557)
    /// (`2026-03-18T13:04:27-07:00[America/Los_Angeles]`) strings.
    ///
    /// Zoneless strings are rejected — parse those as `PlainDateTime`.
    /// A bracketed IANA annotation is kept as the timezone but is not
    /// validated against the timezone database until it is first resolved
    /// (e.g. by `timezone_offset` or a component accessor).
    //baml:mut_vm
    function parse(s: string) -> ZonedDateTime throws root.errors.ParseError {
        $rust_function
    }
    /// Serializes the way Temporal does: RFC 9557 with the bracketed
    /// annotation when the timezone is an IANA identifier
    /// (`2026-03-18T13:04:27-07:00[America/Los_Angeles]`); plain RFC 3339
    /// when it is a fixed offset (with `Z` for the zero offset).
    ///
    /// Panics if the value cannot be formatted (year out of range, unknown
    /// timezone, or an IO error resolving the IANA zone).
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (_) {
                _ => {
                    root.sys.panic(
                        "ZonedDateTime.to_string: value cannot be formatted (year out of range, unknown timezone, or IO error)",
                    )
                }
            }
        }
    }

    /// Internal RFC 9557 / RFC 3339 formatter. Throws when the value cannot be
    /// formatted; `baml.ToString.to_string` turns that into a panic, while
    /// `to_json` surfaces it as a `JsonSerializationError`.
    function _to_string_impl(
        self,
    ) -> string throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        _format_zoned(self._nanoseconds, self.timezone_offset()._nanoseconds, self._iana)
    }

    /// If `self` is before `other` (absolute-time comparison), returns
    /// `other`. Otherwise, returns `self`.
    function max(self, other: ZonedDateTime) -> ZonedDateTime throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// If `self` is after `other` (absolute-time comparison), returns
    /// `other`. Otherwise, returns `self`.
    function min(self, other: ZonedDateTime) -> ZonedDateTime throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }

    /// Same absolute time, different timezone.
    function with_timezone(self, timezone: TimeZoneOffset | string) -> ZonedDateTime throws never {
        ZonedDateTime.from_instant(self.to_instant(), timezone)
    }
    /// The timezone: a `TimeZoneOffset` if fixed, or the IANA identifier.
    function timezone(self) -> TimeZoneOffset | string throws never {
        match (self._iana) {
            null => TimeZoneOffset { _nanoseconds: self._offset_ns ?? 0 },
            // `?? ""` is unreachable: the null arm above already matched.
            let iana => iana ?? "",
        }
    }
    /// The resolved `TimeZoneOffset` for this `ZonedDateTime`. If the
    /// timezone is an IANA identifier, it is resolved to a concrete offset
    /// based on the absolute time (DST-aware), using the host's timezone
    /// database.
    function timezone_offset(self) -> TimeZoneOffset throws UnknownTimezoneError | root.errors.Io {
        match (self._iana) {
            null => TimeZoneOffset { _nanoseconds: self._offset_ns ?? 0 },
            // The `?? ""` / `?? 0` fallbacks are unreachable: the null arms
            // already matched.
            let iana => match (_tz_offset_at(iana ?? "", self._nanoseconds)) {
                null => throw UnknownTimezoneError { timezone: iana ?? "" },
                let offset => TimeZoneOffset { _nanoseconds: offset ?? 0 },
            },
        }
    }

    /// Drops the timezone, keeping the wall-clock reading.
    function to_plain(self) -> PlainDateTime throws UnknownTimezoneError | root.errors.Io {
        PlainDateTime { _nanoseconds: self._nanoseconds + self.timezone_offset()._nanoseconds }
    }

    /// The calendar year, resolved through the timezone.
    function year(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().year()
    }
    /// The calendar month, in `[1, 12]`, resolved through the timezone.
    function month(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().month()
    }
    /// The day of the month, in `[1, 31]`, resolved through the timezone.
    function day(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().day()
    }
    /// The hour of the day, in `[0, 23]`, resolved through the timezone.
    function hour(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().hour()
    }
    /// The minute of the hour, in `[0, 59]`, resolved through the timezone.
    function minute(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().minute()
    }
    /// The second of the minute, in `[0, 59]`, resolved through the timezone.
    function second(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().second()
    }
    /// The millisecond of the second, in `[0, 999]`, resolved through the
    /// timezone.
    function millisecond(
        self,
    ) -> int throws root.errors.InvalidArgument | UnknownTimezoneError | root.errors.Io {
        self.to_plain().millisecond()
    }

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            match (j) {
                string => {
                    ZonedDateTime.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.JsonDecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.JsonDecodeError { message: "Expected RFC 3339 / RFC 9557 timestamp string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.JsonSerializationError { message: message, path: "", reason: message }
                },
                UnknownTimezoneError { timezone } => {
                    throw baml.json.JsonSerializationError {
                        message: "unknown timezone: " + timezone,
                        path: "",
                        reason: "unknown timezone: " + timezone,
                    }
                },
                root.errors.Io { message } => {
                    throw baml.json.JsonSerializationError { message: message, path: "", reason: message }
                },
            }
        }
    }
}

/// Internal: formats an absolute time as RFC 3339 at the given offset,
/// appending an RFC 9557 `[iana]` annotation when `iana` is non-null.
function _format_zoned(
    epoch_ns: bigint,
    offset_ns: int,
    iana: string?,
) -> string throws root.errors.InvalidArgument {
    $rust_function
}

implement root.ops.Add<Duration> for ZonedDateTime {
    type Output = ZonedDateTime
    function add(self, other: Duration) -> ZonedDateTime {
        ZonedDateTime {
            _nanoseconds: self._nanoseconds + other._nanoseconds,
            _offset_ns: self._offset_ns,
            _iana: self._iana,
        }
    }
}

implement root.ops.Subtract<Duration> for ZonedDateTime {
    type Output = ZonedDateTime
    function sub(self, other: Duration) -> ZonedDateTime {
        ZonedDateTime {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
            _offset_ns: self._offset_ns,
            _iana: self._iana,
        }
    }
}

implement root.ops.Subtract<ZonedDateTime> for ZonedDateTime {
    type Output = Duration
    function sub(self, other: ZonedDateTime) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
        }
    }
}
