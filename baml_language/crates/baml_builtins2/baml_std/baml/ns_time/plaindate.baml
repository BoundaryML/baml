/// A civil date with no time and no timezone, on the proleptic Gregorian
/// calendar: `1979-05-27`.
///
/// Equivalent to `Temporal.PlainDate` (TC39). Like every `Plain` type it is a
/// wall-clock quantity, not a point on the global timeline: "27 May 1979"
/// begins at a different absolute moment in every timezone, so a `PlainDate`
/// cannot be converted to an `Instant` without one being supplied. Give it a
/// time-of-day with `to_plain_datetime`, then a timezone with
/// `PlainDateTime.to_zoned`.
///
/// The supported range is year ±9999. `from_components` and `parse` enforce it,
/// but `PlainDateTime.to_plain_date` checks only that the day count fits, so it
/// can hand back a `PlainDate` whose `year`, `month`, `day` and `to_string`
/// then fail.
/// TODO: tighten that check so those accessors cannot throw.
class PlainDate {
    /// (internal) Days since 1970-01-01 on the proleptic Gregorian calendar;
    /// negative before it.
    _days: int,

    /// Creates a `PlainDate` from calendar components.
    ///
    /// # Parameters
    ///
    /// - `year`: the proleptic Gregorian year, in `[-9999, 9999]`. Year 0 is
    ///   1 BCE, following ISO 8601's astronomical numbering.
    /// - `month`: 1-based, in `[1, 12]`.
    /// - `day`: 1-based, and must exist in that month of that year — Feb 29 is
    ///   accepted in a leap year and rejected otherwise.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if a component is out of range
    ///   (e.g. month 13, Feb 30) or the year is outside ±9999.
    //baml:mut_vm
    function from_components(
        year: int,
        month: int,
        day: int,
    ) -> PlainDate throws root.errors.InvalidArgument {
        $rust_function
    }
    /// Parses an extended-format ISO 8601 calendar date: `1979-05-27`.
    ///
    /// The `-`-separated form only: the basic (separator-less) `19790527` is
    /// not accepted, and neither is a trailing time-of-day — parse those as
    /// `PlainDateTime`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `s` is not an ISO 8601 calendar date, or
    ///   names a day that does not exist.
    //baml:mut_vm
    function parse(s: string) -> PlainDate throws root.errors.ParseError {
        $rust_function
    }
    implements baml.ToString {
        /// Serializes to an ISO 8601 calendar date: `1979-05-27`. A negative
        /// year carries a leading `-`.
        ///
        /// # Panics
        ///
        /// - `baml.panics.UserPanic` if the date is outside the ±9999 year
        ///   range. Use `to_json` where that failure needs to be handled rather
        ///   than propagated.
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    root.sys.panic("PlainDate.to_string: " + message)
                },
            }
        }
    }

    /// (internal) ISO 8601 formatter, shared by `to_string` and `to_json` so
    /// the two agree byte for byte.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the day count puts the date outside
    ///   the ±9999 year range. `baml.ToString.to_string` turns that into a
    ///   panic, while `to_json` surfaces it as a
    ///   `baml.json.SerializationError`.
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Combines with a time-of-day into a `PlainDateTime`. Still a wall-clock
    /// value — pass it to `PlainDateTime.to_zoned` to place it on the global
    /// timeline.
    ///
    /// # Parameters
    ///
    /// - `time`: the time-of-day to attach. Defaults to midnight
    ///   (`00:00:00`); being defaulted it is named-only, so overriding it
    ///   reads `d.to_plain_datetime(time = t)`.
    function to_plain_datetime(
        self,
        time: PlainTime = PlainTime { _nanoseconds: 0 },
    ) -> PlainDateTime throws never {
        self._to_plain_datetime(time._nanoseconds)
    }
    /// (internal) all-required native behind `to_plain_datetime` (native
    /// functions have no BAML prologue to fill defaulted parameters).
    ///
    /// # Parameters
    ///
    /// - `time`: nanoseconds since midnight, i.e. `PlainTime._nanoseconds`.
    //baml:mut_vm
    function _to_plain_datetime(self, time: int) -> PlainDateTime throws never {
        $rust_function
    }

    /// The calendar year. Negative years follow ISO 8601's astronomical
    /// numbering, where year 0 is 1 BCE.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the date is outside the ±9999 year
    ///   range.
    function year(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The calendar month, in `[1, 12]`. January is 1, not 0.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the date is outside the ±9999 year
    ///   range.
    function month(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }
    /// The day of the month, in `[1, 31]`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the date is outside the ±9999 year
    ///   range.
    function day(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    implements baml.FromJson {
        /// Decodes a `PlainDate` from a JSON string, accepting exactly what
        /// `PlainDate.parse` accepts.
        ///
        /// # Throws
        ///
        /// - `baml.json.DecodeError` if the value is not a string, or is a
        ///   string `PlainDate.parse` rejects.
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            match (j) {
                string => {
                    PlainDate.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.DecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.DecodeError { message: "Expected ISO 8601 date string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        /// Encodes `self` as the same ISO 8601 date string `to_string`
        /// produces.
        ///
        /// # Throws
        ///
        /// - `baml.json.SerializationError` if the date is outside the ±9999
        ///   year range. The same failure panics in `to_string`: a serializer
        ///   is called with a caller standing by to react, so it gets the error
        ///   channel.
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            self._to_string_impl() catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.SerializationError { message: message, path: "" }
                },
            }
        }
    }
}
