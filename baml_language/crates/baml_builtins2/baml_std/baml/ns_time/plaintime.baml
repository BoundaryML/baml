/// A civil wall-clock time without a date or timezone: `07:32:00.5`.
///
/// Equivalent to `Temporal.PlainTime` (TC39).
class PlainTime {
    /// Internal representation: nanoseconds since midnight, in `[0, 24h)`.
    _nanoseconds: int;

    /// Creates a `PlainTime` from clock components. Omitted components
    /// default to zero.
    ///
    /// Throws `root.errors.InvalidArgument` if a component is out of range
    /// (e.g. hour 24, minute 60).
    //baml:mut_vm
    function from_components(
        hour: int,
        minute: int?,
        second: int?,
        millisecond: int?,
        microsecond: int?,
        nanosecond: int?,
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
    function to_string(self) -> string throws never {
        $rust_function
    }

    /// Combines with a date into a `PlainDateTime`.
    function to_plain_datetime(self, date: PlainDate) -> PlainDateTime throws never {
        date.to_plain_datetime(self)
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

    function from_json(j: baml.json.json) -> PlainTime throws root.json.JsonDecodeError {
        match (j) {
            string => PlainTime.parse(j) catch (_) {
                root.errors.ParseError { message } => throw root.json.JsonDecodeError { message: message, path: "" },
            },
            _ => throw root.json.JsonDecodeError { message: "Expected ISO 8601 time string" }
        }
    }
    function to_json(self) -> string throws never {
        self.to_string()
    }
}
