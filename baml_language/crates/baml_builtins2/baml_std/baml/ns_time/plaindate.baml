/// A civil date without a time or timezone, on the proleptic Gregorian
/// calendar: `1979-05-27`.
///
/// Equivalent to `Temporal.PlainDate` (TC39).
class PlainDate {
    /// Internal representation: days since 1970-01-01 on the proleptic
    /// Gregorian calendar.
    _days: int;

    /// Creates a `PlainDate` from calendar components. `month` and `day`
    /// are 1-based.
    ///
    /// Throws `root.errors.InvalidArgument` if a component is out of range
    /// (e.g. month 13, Feb 30) or the year is outside ±9999.
    //baml:mut_vm
    function from_components(year: int, month: int, day: int) -> PlainDate throws root.errors.InvalidArgument {
        $rust_function
    }
    /// Parses an ISO 8601 calendar date: `1979-05-27`.
    //baml:mut_vm
    function parse(s: string) -> PlainDate throws root.errors.ParseError {
        $rust_function
    }
    /// Serializes to an ISO 8601 calendar date: `1979-05-27`.
    function to_string(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Combines with a time-of-day into a `PlainDateTime`. Defaults to
    /// midnight when `time` is omitted (pass by name to override:
    /// `d.to_plain_datetime(time = t)`).
    function to_plain_datetime(self, time: PlainTime? = null) -> PlainDateTime throws never {
        self._to_plain_datetime(time)
    }
    /// Internal: all-required native behind `to_plain_datetime` (native
    /// functions have no BAML prologue to fill defaulted parameters).
    //baml:mut_vm
    function _to_plain_datetime(self, time: PlainTime?) -> PlainDateTime throws never {
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

    function from_json(j: baml.json.json) -> PlainDate throws root.json.JsonDecodeError {
        match (j) {
            string => PlainDate.parse(j) catch (_) {
                root.errors.ParseError { message } => throw root.json.JsonDecodeError { message: message, path: "" },
            },
            _ => throw root.json.JsonDecodeError { message: "Expected ISO 8601 date string" }
        }
    }
    function to_json(self) -> string throws root.json.JsonSerializationError {
        self.to_string() catch (_) {
            root.errors.InvalidArgument { message } => throw root.json.JsonSerializationError { message: message, path: "" },
        }
    }
}
