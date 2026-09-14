/// A civil wall-clock time-of-day with no date and no timezone: `07:32:00.5`.
///
/// Equivalent to `Temporal.PlainTime` (TC39). A wall-clock reading, not a point
/// on the global timeline — attach a date with `to_plain_datetime` and then a
/// timezone with `PlainDateTime.to_zoned` to locate it.
///
/// The range is one 24-hour day, `[00:00:00, 24:00:00)`. Arithmetic wraps
/// inside it rather than carrying into a date: `23:00:00` plus two hours is
/// `01:00:00`, with nothing recording that a day boundary was crossed. Leap
/// seconds are not representable, so `second` is `[0, 59]`.
class PlainTime {
    /// (internal) Nanoseconds since midnight.
    ///
    /// Invariant: in `[0, 24h)`. `_from_components` and `parse` validate it,
    /// and `_wrap_into_day` re-establishes it after arithmetic. Constructing
    /// the class field-by-field is the one way to break it, which is why
    /// `_to_string_impl` normalizes defensively.
    _nanoseconds: int,

    /// Creates a `PlainTime` from clock components.
    ///
    /// Every component but `hour` is defaulted and therefore named-only:
    /// `PlainTime.from_components(7, minute = 32)`.
    ///
    /// # Parameters
    ///
    /// - `hour`: in `[0, 23]`.
    /// - `minute`: in `[0, 59]`.
    /// - `second`: in `[0, 59]`; there is no leap-second value.
    /// - `millisecond`, `microsecond`, `nanosecond`: each in `[0, 999]`, and
    ///   each an independent digit group — the subsecond part is
    ///   `millisecond * 1000000 + microsecond * 1000 + nanosecond`, not three
    ///   spellings of the same field.
    ///
    /// # Throws
    ///
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
    /// (internal) all-required native behind `from_components`. Native
    /// functions have no BAML prologue, so defaulted parameters cannot live on
    /// the native itself — an omitted argument would reach it unfilled.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if a component is out of range.
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
    /// Hours, minutes and seconds are all required — `07:32` is rejected. The
    /// fractional-second part is optional. No offset or `Z` suffix is accepted,
    /// since a time-of-day has no timezone to attach one to.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if the string is not a valid ISO 8601
    ///   time-of-day, or names a component out of range.
    //baml:mut_vm
    function parse(s: string) -> PlainTime throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes to an ISO 8601 time-of-day: `07:32:00`.
        ///
        /// Sub-second precision is preserved, and trailing subsecond zeros are
        /// truncated, so a whole second renders with no fractional part at all.
        ///
        /// Alone among the `baml.time` types, this `to_string` cannot fail:
        /// every `PlainTime` is inside a day, and so is always renderable.
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    /// (internal) ISO 8601 time-of-day formatter behind `to_string`.
    ///
    /// Infallible: it reduces the nanosecond count into `[0, 24h)` before
    /// formatting, so even a `PlainTime` assembled field-by-field outside the
    /// class invariant renders rather than failing.
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    /// Combines with a date into a `PlainDateTime`, the mirror image of
    /// `PlainDate.to_plain_datetime`. Still a wall-clock value — pass it to
    /// `PlainDateTime.to_zoned` to place it on the global timeline.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let time = baml.time.PlainTime.parse("12:34:56");
    /// let date = baml.time.PlainDate.parse("2025-01-01");
    /// let datetime = time.to_plain_datetime(date);
    /// assert.equal(datetime, baml.time.PlainDateTime.parse("2025-01-01T12:34:56"));
    /// ```
    function to_plain_datetime(self, date: PlainDate) -> PlainDateTime throws never {
        date.to_plain_datetime(time = self)
    }

    /// The hour of the day, in `[0, 23]`. A 24-hour clock, so there is no am/pm
    /// distinction to lose.
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
    ///
    /// The finer components `from_components` accepts — microsecond and
    /// nanosecond — have no accessor of their own; `to_string` renders them.
    function millisecond(self) -> int throws never {
        self._nanoseconds / 1000000 % 1000
    }

    implements baml.FromJson {
        /// Decodes a `PlainTime` from a JSON string, accepting exactly what
        /// `PlainTime.parse` accepts.
        ///
        /// # Throws
        ///
        /// - `baml.json.DecodeError` if the value is not a string, or is a
        ///   string `PlainTime.parse` rejects.
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
        /// Encodes `self` as the same ISO 8601 time-of-day string `to_string`
        /// produces.
        ///
        /// Declares `baml.json.SerializationError` to satisfy `baml.ToJson`,
        /// but never throws it: rendering a `PlainTime` cannot fail.
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
    /// Advances the time-of-day by `other`, wrapping around midnight.
    ///
    /// A `PlainTime` has no date, so there is no day to carry into: the result
    /// is the reading a wall clock would show, and a `Duration` of a week lands
    /// in the same place as one of zero.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let time = baml.time.PlainTime.parse("23:00:00");
    /// let duration = baml.time.Duration.from_hours(2);
    /// assert.equal(time + duration, baml.time.PlainTime.parse("01:00:00"));
    /// ```
    function add(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds + other._nanoseconds),
        }
    }
}

implement root.ops.Subtract<Duration> for PlainTime {
    type Output = PlainTime
    /// Rewinds the time-of-day by `other`, wrapping around midnight.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let time = baml.time.PlainTime.parse("01:00:00");
    /// let duration = baml.time.Duration.from_hours(2);
    /// assert.equal(time - duration, baml.time.PlainTime.parse("23:00:00"));
    /// ```
    function sub(self, other: Duration) -> PlainTime {
        PlainTime {
            _nanoseconds: _wrap_into_day(self._nanoseconds - other._nanoseconds),
        }
    }
}

implement root.ops.Subtract<PlainTime> for PlainTime {
    type Output = Duration
    /// The signed gap from `other` to `self`, as read off the same clock face.
    ///
    /// Does *not* wrap: the result is in `(-24h, +24h)`, and is negative
    /// whenever `self` reads earlier in the day than `other`. So this is the
    /// distance within one day, not "time until next `self`" —
    /// `01:00:00 - 23:00:00` is `-22h`, not `+2h`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let time1 = baml.time.PlainTime.parse("01:00:00");
    /// let time2 = baml.time.PlainTime.parse("02:00:00");
    /// assert.equal(time2 - time1, baml.time.Duration.from_hours(1));
    /// ```
    function sub(self, other: PlainTime) -> Duration {
        Duration {
            _nanoseconds: 0n + (self._nanoseconds - other._nanoseconds),
        }
    }
}

/// (internal) Reduces `total_ns` to nanoseconds-since-midnight in `[0, 24h)`,
/// re-establishing `PlainTime._nanoseconds`'s invariant after arithmetic.
///
/// Written `((n % day) + day) % day` rather than `n % day` because `%` keeps
/// the sign of its left operand: a negative `total_ns` would otherwise stay
/// negative.
///
/// # Returns
///
/// The wrapped count, always in `[0, 24h)` and therefore always an `int`.
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
