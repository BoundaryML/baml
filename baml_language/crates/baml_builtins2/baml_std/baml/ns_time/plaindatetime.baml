/// A civil ("wall-clock") date and time with no timezone, on the proleptic
/// Gregorian calendar: `1979-05-27T07:32:00`. It cannot be located on the
/// global timeline until a timezone is supplied via `to_zoned`.
///
/// Equivalent to `Temporal.PlainDateTime` (TC39), whose `Plain` prefix marks
/// anything without a timezone.
///
/// The supported range is year ±9999. `from_components` and `parse` enforce it;
/// `+ Duration` does not, and is what walks a value out of range. Past the
/// boundary every component accessor below throws
/// `baml.errors.InvalidArgument`, and `to_string` panics.
class PlainDateTime {
    /// (internal) The wall-clock reading encoded as nanoseconds since
    /// 1970-01-01T00:00:00, interpreting the reading as if it were UTC.
    ///
    /// A civil quantity, not an absolute time: UTC is only the arithmetic frame
    /// the encoding borrows, and the same bit pattern in `Instant._nanoseconds`
    /// or `ZonedDateTime._nanoseconds` means a different thing. `to_zoned` is
    /// what converts between the two readings.
    _nanoseconds: bigint,

    /// Creates a `PlainDateTime` from calendar and clock components.
    ///
    /// Every clock component is defaulted and therefore named-only:
    /// `PlainDateTime.from_components(1979, 5, 27, hour = 7)`.
    ///
    /// # Parameters
    ///
    /// - `year`: the proleptic Gregorian year, in `[-9999, 9999]`. Year 0 is
    ///   1 BCE, following ISO 8601's astronomical numbering.
    /// - `month`: 1-based, in `[1, 12]`.
    /// - `day`: 1-based, and must exist in that month of that year.
    /// - `hour`: in `[0, 23]`.
    /// - `minute`, `second`: each in `[0, 59]`; there is no leap-second value.
    /// - `millisecond`, `microsecond`, `nanosecond`: each in `[0, 999]`, and
    ///   each an independent digit group — the subsecond part is
    ///   `millisecond * 1000000 + microsecond * 1000 + nanosecond`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if a component is out of range
    ///   (e.g. month 13, Feb 30) or the year is outside ±9999.
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
    /// (internal) all-required native behind `from_components` (native
    /// functions have no BAML prologue to fill defaulted parameters).
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if a component is out of range.
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

    /// Parses a zoneless ISO 8601 string: `1979-05-27T07:32:00`,
    /// `1979-05-27T07:32:00.5`. These are exactly the strings `Instant.parse`
    /// and `ZonedDateTime.parse` reject for carrying no offset.
    ///
    /// The `-`/`:`-separated form with a literal `T` only. Seconds are
    /// required, and the fractional part is optional. An offset or `Z` suffix
    /// is rejected rather than ignored: a string that pins itself to the global
    /// timeline is not a `PlainDateTime`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `s` is not a zoneless ISO 8601 datetime,
    ///   or names a component out of range.
    //baml:mut_vm
    function parse(s: string) -> PlainDateTime throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes to ISO 8601 with no offset suffix:
        /// `1979-05-27T07:32:00`. A negative year carries a leading `-`.
        ///
        /// Sub-second precision is preserved, and trailing subsecond zeros are
        /// truncated, so a whole second renders with no fractional part. No `Z`
        /// is ever emitted — the value is a wall-clock reading, and a `Z` would
        /// claim it is UTC.
        ///
        /// # Panics
        ///
        /// - `baml.panics.UserPanic` if the year is outside ±9999. Use
        ///   `to_json` where that failure needs to be handled rather than
        ///   propagated.
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    root.sys.panic("PlainDateTime.to_string: " + message)
                },
            }
        }
    }

    /// (internal) ISO 8601 formatter, shared by `to_string` and `to_json` so
    /// the two agree byte for byte.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the year is outside ±9999.
    ///   `baml.ToString.to_string` turns that into a panic, while `to_json`
    ///   surfaces it as a `baml.json.SerializationError`.
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Locates this wall-clock reading in a timezone, producing an absolute
    /// time. The counterpart of `ZonedDateTime.to_plain`.
    ///
    /// With a `TimeZoneOffset` the conversion is exact arithmetic: a fixed
    /// offset has no transitions, every civil reading maps to exactly one
    /// instant, and none of the failures below can arise. With an IANA
    /// identifier it is a lookup in the host's timezone database, and that
    /// mapping is not total — a spring-forward transition leaves a *gap* of
    /// readings that never happened, and a fall-back transition leaves an
    /// *overlap* of readings that happened twice. `disambiguation` decides what
    /// to do at both.
    ///
    /// # Parameters
    ///
    /// - `timezone`: a `TimeZoneOffset` for a fixed offset, or an IANA
    ///   identifier such as `"America/Los_Angeles"`.
    /// - `disambiguation`: how to resolve a DST gap or overlap; see
    ///   `Disambiguation`. Defaults to `"compatible"` and, being defaulted, is
    ///   named-only: `dt.to_zoned(tz, disambiguation = "earlier")`. Ignored for
    ///   a `TimeZoneOffset`, which can never be ambiguous.
    ///
    /// # Returns
    ///
    /// A `ZonedDateTime` carrying the resolved absolute time and the timezone
    /// as given — an IANA identifier stays an identifier, so the value keeps
    /// tracking that zone's future transitions rather than freezing today's
    /// offset.
    ///
    /// # Throws
    ///
    /// - `baml.time.UnknownTimezoneError` if `timezone` is a string the host's
    ///   timezone database does not know.
    /// - `baml.time.AmbiguousTimeError` if `disambiguation` is `"reject"` and
    ///   the reading falls in a gap or an overlap.
    /// - `baml.errors.Io` if the host's timezone database fails to resolve the
    ///   reading, including when the civil value is outside the year range the
    ///   host can represent.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host has no timezone database at
    ///   all. An unknown identifier is an error because the caller chose the
    ///   identifier; a missing database is a property of the platform.
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

    /// The later of `self` and `other` on the clock face; `self` if they read
    /// the same.
    ///
    /// A civil comparison: both readings are compared as if in the same
    /// timezone. Two readings taken in different zones are not ordered by this
    /// — convert to `Instant` or `ZonedDateTime` first if absolute order is
    /// what you mean.
    function max(self, other: PlainDateTime) -> PlainDateTime throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// The earlier of `self` and `other` on the clock face; `self` if they read
    /// the same. A civil comparison, with the same caveat as `max`.
    function min(self, other: PlainDateTime) -> PlainDateTime throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }

    /// Drops the time-of-day, keeping the calendar date. The inverse of
    /// `PlainDate.to_plain_datetime`.
    ///
    /// The check here is laxer than the ±9999 year range: it can hand back a
    /// `PlainDate` whose own accessors then throw. See `PlainDate.year`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is too far from the epoch
    ///   for its day count to be computed.
    //baml:mut_vm
    function to_plain_date(self) -> PlainDate throws root.errors.InvalidArgument {
        $rust_function
    }
    /// Drops the calendar date, keeping the time-of-day. The inverse of
    /// `PlainTime.to_plain_datetime`.
    ///
    /// Total: the time-of-day is the reading modulo one day, which is defined
    /// for every value the type can hold, however far from the epoch.
    //baml:mut_vm
    function to_plain_time(self) -> PlainTime throws never {
        $rust_function
    }

    /// The calendar year. Negative years follow ISO 8601's astronomical
    /// numbering, where year 0 is 1 BCE.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function year(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The calendar month, in `[1, 12]`. January is 1, not 0.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function month(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The day of the month, in `[1, 31]`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function day(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The hour of the day, in `[0, 23]`. A 24-hour clock.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function hour(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The minute of the hour, in `[0, 59]`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function minute(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The second of the minute, in `[0, 59]`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function second(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The millisecond of the second, in `[0, 999]`.
    ///
    /// The finer components `from_components` accepts — microsecond and
    /// nanosecond — have no accessor of their own; `to_string` renders them.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the reading is outside ±9999.
    function millisecond(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    implements baml.FromJson {
        /// Decodes a `PlainDateTime` from a JSON string, accepting exactly what
        /// `PlainDateTime.parse` accepts.
        ///
        /// # Throws
        ///
        /// - `baml.json.DecodeError` if the value is not a string, or is a
        ///   string `PlainDateTime.parse` rejects — including an offset-bearing
        ///   timestamp, which belongs to `ZonedDateTime` or `Instant`.
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
        /// Encodes `self` as the same zoneless ISO 8601 string `to_string`
        /// produces.
        ///
        /// # Throws
        ///
        /// - `baml.json.SerializationError` if the year is outside ±9999. The
        ///   same failure panics in `to_string`: a serializer is called with a
        ///   caller standing by to react, so it gets the error channel.
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
}

implement root.ops.Add<Duration> for PlainDateTime {
    type Output = PlainDateTime
    /// Advances the wall-clock reading by `other`.
    ///
    /// Pure civil arithmetic: no timezone is consulted, so adding 24 hours
    /// always lands on the same clock time the next day, even where the
    /// eventual timezone has a DST transition in between. `ZonedDateTime`'s `+`
    /// is the one that moves absolute time instead.
    ///
    /// The result is not range-checked; walking past year ±9999 produces a
    /// value whose accessors and `to_string` then fail.
    function add(self, other: Duration) -> PlainDateTime {
        PlainDateTime {
            _nanoseconds: self._nanoseconds + other._nanoseconds
        }
    }
}

implement root.ops.Subtract<Duration> for PlainDateTime {
    type Output = PlainDateTime
    /// Rewinds the wall-clock reading by `other`. Civil arithmetic, with the
    /// same caveats as `PlainDateTime + Duration`.
    function sub(self, other: Duration) -> PlainDateTime {
        PlainDateTime {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}

implement root.ops.Subtract<PlainDateTime> for PlainDateTime {
    type Output = Duration
    /// The signed gap from `other` to `self`, read off the clock face: positive
    /// when `self` is the later reading, negative when it is the earlier.
    ///
    /// Civil, so it counts the nominal distance between two readings. That is
    /// not the elapsed time between the corresponding instants unless both
    /// readings sit in the same UTC offset.
    function sub(self, other: PlainDateTime) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds
        }
    }
}
