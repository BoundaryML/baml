/// A civil ("wall-clock") date and time without a timezone, on the proleptic
/// Gregorian calendar: `1979-05-27T07:32:00`. It cannot be located on the
/// global timeline without supplying a timezone via `to_zoned`.
///
/// Equivalent to `Temporal.PlainDateTime` (TC39). The `Plain` prefix marks
/// anything without a timezone, following TC39 Temporal.
class PlainDateTime {
    /// Internal representation: the wall-clock reading encoded as nanoseconds
    /// since 1970-01-01T00:00:00, interpreting the reading as if it were UTC.
    /// This is a civil quantity, not an absolute time.
    _nanoseconds: bigint,

    /// Creates a `PlainDateTime` from calendar/clock components. `month` and
    /// `day` are 1-based. Defaulted clock components are passed by name:
    /// `PlainDateTime.from_components(1979, 5, 27, hour = 7)`.
    ///
    /// Throws `baml.errors.InvalidArgument` if a component is out of range
    /// (e.g. month 13, Feb 30) or the year is outside ±9999.
    function from_components(
        year: int,
        month: int,
        day: int,
        hour: int = 0,
        minute: int = 0,
        second: int = 0,
        millisecond: int = 0,
        microsecond: int = 0,
        nanosecond: int = 0,
    ) -> PlainDateTime throws root.errors.InvalidArgument {
        PlainDateTime._from_components(
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
            microsecond,
            nanosecond,
        )
    }
    /// (internal) Internal: all-required native behind `from_components` (native
    /// functions have no BAML prologue to fill defaulted parameters).
    //baml:mut_vm
    function _from_components(
        year: int,
        month: int,
        day: int,
        hour: int,
        minute: int,
        second: int,
        millisecond: int,
        microsecond: int,
        nanosecond: int,
    ) -> PlainDateTime throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Parses a zoneless ISO 8601 string (`1979-05-27T07:32:00`,
    /// `1979-05-27T07:32:00.5`) — exactly the strings `Instant.parse` and
    /// `ZonedDateTime.parse` reject for carrying no offset.
    //baml:mut_vm
    function parse(s: string) -> PlainDateTime throws root.errors.ParseError {
        $rust_function
    }

    /// Serializes to ISO 8601 with no offset suffix: `1979-05-27T07:32:00`.
    /// Sub-second precision is supported; trailing subsecond zeros are
    /// truncated.
    ///
    /// # Panics
    ///
    /// - `baml.panics.UserPanic` if the year is outside ±9999.
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    root.sys.panic("PlainDateTime.to_string: " + message)
                },
            }
        }
    }

    /// (internal) ISO 8601 formatter. Throws `InvalidArgument` when the year is
    /// outside ±9999; `baml.ToString.to_string` turns that into a panic, while
    /// `to_json` surfaces it as a `baml.json.SerializationError`.
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Locates this wall-clock reading in a timezone, producing an absolute
    /// time. With a `TimeZoneOffset` the conversion is exact. With an IANA
    /// identifier, DST gaps/overlaps are resolved per `disambiguation`
    /// (passed by name: `dt.to_zoned(tz, disambiguation = "earlier")`).
    function to_zoned(
        self,
        timezone: TimeZoneOffset | string,
        disambiguation: Disambiguation = "compatible",
    ) -> ZonedDateTime throws UnknownTimezoneError | AmbiguousTimeError | root.errors.Io {
        match (timezone) {
            TimeZoneOffset => {
                ZonedDateTime {
                    _nanoseconds: self._nanoseconds - timezone._nanoseconds,
                    _offset_ns: timezone._nanoseconds,
                    _iana: null,
                }
            },
            string => {
                if (disambiguation == "reject") {
                    // "reject" is implemented as: resolve both sides of any DST
                    // gap/overlap and require them to agree.
                    match (_tz_to_instant(timezone, self._nanoseconds, "earlier")) {
                        null => throw UnknownTimezoneError { timezone: timezone },
                        let earlier => {
                            match (_tz_to_instant(timezone, self._nanoseconds, "later")) {
                                null => throw UnknownTimezoneError { timezone: timezone },
                                let later => {
                                    if (earlier == later) {
                                        // `?? 0n` is unreachable: the null arm already matched.
                                        ZonedDateTime {
                                            _nanoseconds: earlier ?? 0n,
                                            _offset_ns: null,
                                            _iana: timezone,
                                        }
                                    } else {
                                        throw AmbiguousTimeError { message: "wall-clock time is ambiguous or skipped in timezone " + timezone }
                                    }
                                },
                            }
                        },
                    }
                } else {
                    match (_tz_to_instant(timezone, self._nanoseconds, disambiguation)) {
                        null => throw UnknownTimezoneError { timezone: timezone },
                        // `?? 0n` is unreachable: the null arm already matched.
                        let resolved => {
                            ZonedDateTime {
                                _nanoseconds: resolved ?? 0n,
                                _offset_ns: null,
                                _iana: timezone,
                            }
                        },
                    }
                }
            },
        }
    }

    /// If `self` is before `other` (civil comparison), returns `other`.
    /// Otherwise, returns `self`.
    function max(self, other: PlainDateTime) -> PlainDateTime throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// If `self` is after `other` (civil comparison), returns `other`.
    /// Otherwise, returns `self`.
    function min(self, other: PlainDateTime) -> PlainDateTime throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }

    /// Drops the time-of-day, keeping the calendar date.
    //baml:mut_vm
    function to_plain_date(self) -> PlainDate throws root.errors.InvalidArgument {
        $rust_function
    }
    /// Drops the calendar date, keeping the time-of-day.
    //baml:mut_vm
    function to_plain_time(self) -> PlainTime throws never {
        $rust_function
    }

    /// The calendar year.
    function year(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The calendar month, in `[1, 12]`.
    function month(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The day of the month, in `[1, 31]`.
    function day(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The hour of the day, in `[0, 23]`.
    function hour(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The minute of the hour, in `[0, 59]`.
    function minute(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The second of the minute, in `[0, 59]`.
    function second(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The millisecond of the second, in `[0, 999]`.
    function millisecond(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            match (j) {
                string => {
                    PlainDateTime.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.DecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.DecodeError { message: "Expected zoneless ISO 8601 string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.SerializationError { message: message, path: "" }
                },
            }
        }
    }
}

implement root.ops.Add<Duration> for PlainDateTime {
    type Output = PlainDateTime
    /// Adds a `Duration` to this `PlainDateTime`, returning a new `PlainDateTime`
    /// with the adjusted epoch time.
    function add(self, other: Duration) -> PlainDateTime {
        PlainDateTime {
            _nanoseconds: self._nanoseconds + other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Duration> for PlainDateTime {
    type Output = PlainDateTime
    /// Subtracts a `Duration` from this `PlainDateTime`, returning a new `PlainDateTime`
    /// with the adjusted epoch time.
    function sub(self, other: Duration) -> PlainDateTime {
        PlainDateTime {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}

implement root.ops.Subtract<PlainDateTime> for PlainDateTime {
    type Output = Duration
    /// Subtracts a `PlainDateTime` from this `PlainDateTime`, returning a `Duration`
    /// representing the time difference between the two.
    function sub(self, other: PlainDateTime) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}
