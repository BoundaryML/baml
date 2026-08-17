/// A civil wall-clock time without a date or timezone: `07:32:00.5`.
///
/// Equivalent to `Temporal.PlainTime` (TC39).
class PlainTime {
    /// Internal representation: nanoseconds since midnight, in `[0, 24h)`.
    _nanoseconds: int,

    /// Creates a `PlainTime` from clock components. Defaulted components
    /// are passed by name: `PlainTime.from_components(7, minute = 32)`.
    ///
    /// Throws `root.errors.InvalidArgument` if a component is out of range
    /// (e.g. hour 24, minute 60).
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
    /// Internal: all-required native behind `from_components`. Native
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
    //baml:mut_vm
    function parse(s: string) -> PlainTime throws root.errors.ParseError {
        $rust_function
    }
    /// Serializes to an ISO 8601 time-of-day: `07:32:00`. Sub-second
    /// precision is supported; trailing subsecond zeros are truncated.
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    /// Combines with a date into a `PlainDateTime`.
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
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            match (j) {
                string => {
                    PlainTime.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.JsonDecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.JsonDecodeError { message: "Expected ISO 8601 time string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            // Route through `baml.json.from` so the body can throw the
            // `JsonSerializationError` the interface declares (a `string`-returning
            // `to_string()` is `throws never`, which would trip the extraneous-throws
            // check on a BAML-bodied impl). The string flows into `json` unchanged.
            baml.json.from(self.to_string())
        }
    }
}

/// Adding a `Duration` wraps around midnight: a `PlainTime` has no date to
/// carry into, so `23:00 + 2h` is `01:00`, not a next-day value.
implement root.ops.Add<Duration> for PlainTime {
    type Output = PlainTime
    function add(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds + other._nanoseconds),
        }
    }
}

/// Wraps like [`Add`]: `01:00 - 2h` is `23:00`.
implement root.ops.Subtract<Duration> for PlainTime {
    type Output = PlainTime
    function sub(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds - other._nanoseconds),
        }
    }
}

/// The signed gap between two wall-clock times, which does *not* wrap: the
/// result is negative when `other` is later in the day than `self`.
implement root.ops.Subtract<PlainTime> for PlainTime {
    type Output = Duration
    function sub(self, other: PlainTime) -> Duration {
        Duration {
            _nanoseconds: 0n + (self._nanoseconds - other._nanoseconds),
        }
    }
}

/// Internal: reduce `total_ns` to nanoseconds-since-midnight in `[0, 24h)`.
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
        root.errors.InvalidArgument => baml.sys
            .panic("unreachable: nanoseconds wrapped into [0, 24h) exceed int range")
    }
}
