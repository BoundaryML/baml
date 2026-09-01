/// A civil wall-clock time without a date or timezone: `07:32:00.5`.
/// Has a range of 24 hours.
///
/// Equivalent to `Temporal.PlainTime` (TC39).
class PlainTime {
    /// Internal representation: nanoseconds since midnight, in `[0, 24h)`.
    _nanoseconds: int,

    /// Creates a `PlainTime` from clock components. Defaulted components
    /// are passed by name: `PlainTime.from_components(7, minute = 32)`.
    ///
    /// # Throws
    /// - `baml.errors.InvalidArgument` if a component is out of range
    ///   (e.g. hour 24, minute 60).
    function from_components(
        hour: int,
        minute: int = 0,
        second: int = 0,
        millisecond: int = 0,
        microsecond: int = 0,
        nanosecond: int = 0,
    ) -> PlainTime throws root.errors.InvalidArgument {
        PlainTime._from_components(hour, minute, second, millisecond, microsecond, nanosecond)
    }
    /// (internal): all-required native behind `from_components`. Native
    /// functions have no BAML prologue, so defaulted parameters cannot live
    /// on the native itself (an omitted argument would reach the native
    /// unfilled).
    //baml:mut_vm
    function _from_components(
        hour: int,
        minute: int,
        second: int,
        millisecond: int,
        microsecond: int,
        nanosecond: int,
    ) -> PlainTime throws root.errors.InvalidArgument {
        $rust_function
    }
    /// Parses an ISO 8601 time-of-day: `07:32:00`, `07:32:00.5`.
    ///
    /// # Throws
    /// - `baml.errors.ParseError` if the string is not a valid ISO 8601 time-of-day.
    //baml:mut_vm
    function parse(s: string) -> PlainTime throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes to an ISO 8601 time-of-day: `07:32:00`. Sub-second
        /// precision is supported; trailing subsecond zeros are truncated.
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    /// (internal) TODO: just do the impl
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    /// Combines with a date into a `PlainDateTime`.
    ///
    /// # Examples
    /// ```baml
    /// let time = baml.time.PlainTime.parse("12:34:56");
    /// let date = baml.time.PlainDate.parse("2025-01-01");
    /// let datetime = time.to_plain_datetime(date);
    /// assert.equal(datetime, baml.time.PlainDateTime.parse("2025-01-01T12:34:56"));
    /// ```
    function to_plain_datetime(self, date: PlainDate) -> PlainDateTime throws never {
        date.to_plain_datetime(time = self)
    }

    /// The hour of the day, in `[0, 23]`.
    function hour(self) -> int throws never {
        self._nanoseconds / 3600000000000
    }
    /// The minute of the hour, in `[0, 59]`.
    function minute(self) -> int throws never {
        self._nanoseconds / 60000000000 % 60
    }
    /// The second of the minute, in `[0, 59]`.
    function second(self) -> int throws never {
        self._nanoseconds / 1000000000 % 60
    }
    /// The millisecond of the second, in `[0, 999]`.
    function millisecond(self) -> int throws never {
        self._nanoseconds / 1000000 % 1000
    }

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            match (j) {
                string => {
                    PlainTime.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.DecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.DecodeError { message: "Expected ISO 8601 time string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            // Route through `baml.json.from` so the body can throw the
            // `baml.json.SerializationError` the interface declares (a `string`-returning
            // `to_string()` is `throws never`, which would trip the extraneous-throws
            // check on a BAML-bodied impl). The string flows into `json` unchanged.
            baml.json.from(self.to_string())
        }
    }
}

implement root.ops.Add<Duration> for PlainTime {
    type Output = PlainTime
    /// Adds a `Duration` to this `PlainTime`, wrapping around midnight.
    ///
    /// # Examples
    /// ```baml
    /// let time = baml.time.PlainTime.parse("23:00");
    /// let duration = baml.time.Duration.from_hours(2);
    /// assert.equal(time + duration, baml.time.PlainTime.parse("01:00"));
    /// ```
    function add(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds + other._nanoseconds),
        }
    }
}

implement root.ops.Subtract<Duration> for PlainTime {
    type Output = PlainTime
    /// Subtracts a `Duration` from this `PlainTime`, wrapping around midnight.
    ///
    /// # Examples
    /// ```baml
    /// let time = baml.time.PlainTime.parse("01:00");
    /// let duration = baml.time.Duration.from_hours(2);
    /// assert.equal(time - duration, baml.time.PlainTime.parse("23:00"));
    /// ```
    function sub(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds - other._nanoseconds),
        }
    }
}

implement root.ops.Subtract<PlainTime> for PlainTime {
    type Output = Duration
    /// Subtracts one `PlainTime` from another, returning the signed gap between them.
    ///
    /// # Examples
    /// ```baml
    /// let time1 = baml.time.PlainTime.parse("01:00");
    /// let time2 = baml.time.PlainTime.parse("02:00");
    /// assert.equal(time2 - time1, baml.time.Duration.from_hours(1));
    /// ```
    function sub(self, other: PlainTime) -> Duration {
        Duration {
            _nanoseconds: 0n + (self._nanoseconds - other._nanoseconds),
        }
    }
}

/// (internal) Internal: reduce `total_ns` to nanoseconds-since-midnight in `[0, 24h)`.
///
/// Written `((n % day) + day) % day` rather than `n % day` because `%` keeps
/// the sign of the left operand, so a negative `total_ns` would otherwise stay
/// negative and break `PlainTime`'s range invariant.
function _wrap_into_day(total_ns: bigint) -> int throws never {
    let day_ns = 86400000000000n;
    let wrapped = ((total_ns % day_ns) + day_ns) % day_ns;
    // `wrapped` is in `[0, 24h)`, i.e. under 2^47 ns, so it always fits an int.
    {
        wrapped.to_int()
    } catch (e) {
        root.errors.InvalidArgument => {
            baml.sys.panic("unreachable: nanoseconds wrapped into [0, 24h) exceed int range")
        }
    }
}
