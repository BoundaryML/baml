/// DST gap/overlap resolution, following TC39 Temporal semantics:
/// - `"compatible"` (default): later for gaps, earlier for overlaps
/// - `"earlier"` / `"later"`: always pick that side
/// - `"reject"`: throw `AmbiguousTimeError`
type Disambiguation = "compatible" | "earlier" | "later" | "reject";

/// Thrown when an IANA timezone identifier cannot be resolved against the
/// host's timezone database.
class UnknownTimezoneError {
    timezone: string,
}

/// Thrown when a civil time cannot be unambiguously located in a timezone
/// (DST gap or overlap) and `disambiguation` is `"reject"`.
class AmbiguousTimeError {
    message: string,
}

/// A fixed offset from UTC, with a permitted range of ±24 hours (real
/// timezones currently range from -12 to +14 hours).
///
/// Note that unlike IANA identifiers, a `TimeZoneOffset` does not change
/// based on daylight savings. Sometimes this is desirable, but other times
/// it is not, so `ZonedDateTime` permits either.
class TimeZoneOffset {
    /// Internal representation: the offset in nanoseconds. Invariant: within
    /// ±24 hours.
    _nanoseconds: int,

    /// The UTC (zero) offset.
    function utc() -> TimeZoneOffset throws never {
        TimeZoneOffset { _nanoseconds: 0 }
    }
    /// Creates a `TimeZoneOffset` from hours and minutes east of UTC.
    /// Both components must carry the same sign (e.g. `new(-7, 0)`,
    /// `new(5, 30)`, `new(-9, -30)`).
    ///
    /// Throws `root.errors.InvalidArgument` if the signs differ or the
    /// result exceeds ±24 hours.
    function new(hours: int, minutes: int) -> TimeZoneOffset throws root.errors.InvalidArgument {
        if ((hours > 0 && minutes < 0) || (hours < 0 && minutes > 0)) {
            throw root.errors.InvalidArgument { message: "TimeZoneOffset.new: hours and minutes must carry the same sign" };
        }
        let nanoseconds = (hours * 60 + minutes) * 60 * 1000000000;
        if (nanoseconds > 86400000000000 || nanoseconds < -86400000000000) {
            throw root.errors.InvalidArgument { message: "TimeZoneOffset out of range: must be within ±24 hours" };
        }
        TimeZoneOffset { _nanoseconds: nanoseconds }
    }
    /// Resolves an IANA timezone identifier (e.g. `"America/Los_Angeles"`)
    /// to its concrete offset at the absolute time `at` (DST-aware).
    ///
    /// Resolution uses the host's timezone database.
    function from_timezone(
        timezone: string,
        at: Instant,
    ) -> TimeZoneOffset throws UnknownTimezoneError | root.errors.Io {
        match (_tz_offset_at(timezone, at._nanoseconds)) {
            null => throw UnknownTimezoneError { timezone: timezone },
            // `?? 0` is unreachable: the null arm above already matched.
            let offset => TimeZoneOffset { _nanoseconds: offset ?? 0 },
        }
    }
    /// Returns the local timezone offset right now.
    /// Note that even in the same location, this may vary over time based on
    /// daylight savings.
    function local() -> TimeZoneOffset throws UnknownTimezoneError | root.errors.Io {
        TimeZoneOffset.from_timezone(system_timezone(), Instant.now())
    }
    /// Creates a `TimeZoneOffset` from a `Duration`.
    ///
    /// Throws `root.errors.InvalidArgument` if the duration exceeds ±24 hours.
    //baml:mut_vm
    function from_duration(
        duration: Duration,
    ) -> TimeZoneOffset throws root.errors.InvalidArgument {
        $rust_function
    }
    function to_duration(self) -> Duration throws never {
        Duration { _nanoseconds: 0n + self._nanoseconds }
    }
    /// The hour component of the offset. Rounded toward zero.
    function hours(self) -> int throws never {
        self._nanoseconds / 3600000000000
    }
    /// The minute component of the offset, modulo one hour. Rounded toward zero.
    function minutes(self) -> int throws never {
        self._nanoseconds / 60000000000 % 60
    }
}

/// Returns the system's IANA timezone identifier, for example
/// `"America/Los_Angeles"`. Mirrors `Temporal.Now.timeZoneId()`.
///
/// An IO function so hosts can swap the system-state source out.
/// Throws if the host cannot determine its timezone.
function system_timezone() -> string throws root.errors.Io {
    $rust_io_function
}

/// Internal: resolves an IANA identifier to its offset (in nanoseconds) at
/// the absolute time `at_ns` (nanoseconds since the Unix epoch), using the
/// host's timezone database. Returns `null` if the identifier is unknown.
function _tz_offset_at(timezone: string, at_ns: bigint) -> int? throws root.errors.Io {
    $rust_io_function
}

/// Internal: locates the civil ("wall-clock") reading `civil_ns` (nanoseconds
/// since 1970-01-01T00:00:00, interpreted as if UTC) in the IANA timezone
/// `timezone`, resolving DST gaps/overlaps per `disambiguation` (which must
/// be `"compatible"`, `"earlier"`, or `"later"` — `"reject"` is implemented
/// by callers as: resolve both `"earlier"` and `"later"` and require them to
/// agree). Returns the resolved absolute time as nanoseconds since the Unix
/// epoch, or `null` if the identifier is unknown to the host's timezone
/// database.
function _tz_to_instant(
    timezone: string,
    civil_ns: bigint,
    disambiguation: string,
) -> bigint? throws root.errors.Io {
    $rust_io_function
}
