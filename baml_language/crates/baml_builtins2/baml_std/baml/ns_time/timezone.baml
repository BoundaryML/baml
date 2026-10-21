/// How to resolve a wall-clock reading that a timezone maps to no instant or to
/// two. Mirrors TC39 Temporal's `disambiguation` option, value for value.
///
/// A daylight-saving transition leaves either a *gap* (spring forward: a range
/// of readings that never occurred) or an *overlap* (fall back: a range that
/// occurred twice). Every other reading maps to exactly one instant and is
/// unaffected by this setting.
///
/// - `"compatible"` (the default everywhere it appears): the later instant for
///   a gap, the earlier one for an overlap.
/// - `"earlier"` / `"later"`: always take that side of the transition.
/// - `"reject"`: refuse to guess, and throw `baml.time.AmbiguousTimeError`.
///
/// Used by `baml.time.PlainDateTime.to_zoned` and
/// `baml.time.ZonedDateTime.from_components`.
type Disambiguation = "compatible" | "earlier" | "later" | "reject";

/// Thrown when an IANA timezone identifier cannot be resolved against the
/// host's timezone database.
///
/// A property of the identifier, not of the host: a host with no timezone
/// database at all panics with `baml.panics.HostUnavailable` instead, since no
/// choice of identifier would have worked.
class UnknownTimezoneError {
    /// The identifier that could not be resolved, exactly as it was supplied.
    timezone: string,
}

/// Thrown when a civil time cannot be unambiguously located in a timezone (it
/// falls in a DST gap or overlap) and `disambiguation` is `"reject"`.
///
/// Any other `Disambiguation` resolves such a reading instead of throwing.
class AmbiguousTimeError {
    /// Human-readable description naming the timezone involved. Not intended to
    /// be matched on.
    message: string,
}

/// A fixed offset from UTC, permitted in the range ±24 hours (real timezones
/// currently run from -12 to +14 hours).
///
/// Unlike an IANA identifier, a `TimeZoneOffset` does not shift with daylight
/// saving. That makes every civil-to-absolute conversion total and exact — no
/// gaps, no overlaps, no `Disambiguation`, no timezone database — but it also
/// pins a moment's offset forever, so a future date carried in one goes wrong
/// as soon as that region's rules change. `ZonedDateTime` accepts either form:
/// prefer an identifier for anything scheduled ahead, and an offset for a
/// timestamp already stamped by someone else.
class TimeZoneOffset {
    /// (internal) The offset east of UTC in nanoseconds. Invariant: within ±24
    /// hours inclusive.
    _nanoseconds: int,

    /// The UTC (zero) offset.
    function utc() -> TimeZoneOffset throws never {
        TimeZoneOffset { _nanoseconds: 0 }
    }
    /// Creates a `TimeZoneOffset` from hours and minutes east of UTC.
    ///
    /// # Parameters
    ///
    /// - `hours`: whole hours east of UTC; negative west of it.
    /// - `minutes`: the sub-hour part, which must carry the same sign as
    ///   `hours` — the pair is one signed quantity split in two, not an hour
    ///   count plus an unsigned remainder. So `new(-9, -30)` is `-09:30`, while
    ///   `new(-9, 30)` is rejected rather than silently read as `-08:30`.
    ///   `new(-7, 0)` and `new(5, 30)` are the ordinary cases.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the signs differ, or the result
    ///   exceeds ±24 hours.
    ///
    /// # Panics
    ///
    /// - `baml.panics.IntegerOverflow` if `hours` or `minutes` is so large that
    ///   converting it to nanoseconds overflows `int`. The range check happens
    ///   after that conversion, so it never sees these.
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
    /// Resolves an IANA timezone identifier (e.g. `"America/Los_Angeles"`) to
    /// its concrete offset at the absolute time `at`, daylight saving included.
    ///
    /// Resolution uses the host's timezone database. The answer is a snapshot
    /// of the offset that zone had at `at`, and must not be reused for a
    /// different moment.
    ///
    /// # Parameters
    ///
    /// - `timezone`: an IANA identifier.
    /// - `at`: the absolute time to resolve at. An `Instant` rather than a
    ///   civil reading, so the query is never itself ambiguous.
    ///
    /// # Throws
    ///
    /// - `baml.time.UnknownTimezoneError` if the timezone is invalid.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if timezone data is unavailable.
    function from_timezone(
        timezone: string,
        at: Instant,
    ) -> TimeZoneOffset throws UnknownTimezoneError {
        match (_tz_offset_at(timezone, at._nanoseconds)) {
            null => throw UnknownTimezoneError { timezone: timezone },
            // `?? 0` is unreachable: the null arm above already matched.
            let offset => TimeZoneOffset { _nanoseconds: offset ?? 0 },
        }
    }

    /// The system timezone's offset as of right now.
    ///
    /// Even in one location this varies over time with daylight saving, so the
    /// result is a snapshot rather than a stand-in for the system timezone
    /// itself. Keep `system_timezone()`'s identifier where the value has to
    /// stay correct across a transition.
    ///
    /// # Throws
    ///
    /// - `baml.time.UnknownTimezoneError` if the host names a system timezone
    ///   its own timezone database then fails to resolve.
    /// - `baml.errors.Io` if the host has a timezone database but cannot tell
    ///   which zone it is configured for.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host provides no clock, or no
    ///   timezone database at all.
    function local() -> TimeZoneOffset throws UnknownTimezoneError | root.errors.Io {
        TimeZoneOffset.from_timezone(system_timezone(), Instant.now())
    }

    /// Creates a `TimeZoneOffset` east of UTC by `duration`. A negative
    /// `duration` gives an offset west of UTC.
    ///
    /// The nanosecond count is kept exactly, not rounded to a whole minute,
    /// which matters for the historical LMT offsets in the timezone database.
    /// `to_string` on a `ZonedDateTime` renders only `±hh:mm`, truncating any
    /// sub-minute part toward zero.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the duration exceeds ±24 hours.
    //baml:mut_vm
    function from_duration(
        duration: Duration,
    ) -> TimeZoneOffset throws root.errors.InvalidArgument {
        $rust_function
    }

    /// The offset as a `Duration` east of UTC; negative west of it. Exact, and
    /// the inverse of `from_duration`.
    function to_duration(self) -> Duration throws never {
        Duration { _nanoseconds: 0n + self._nanoseconds }
    }

    /// The hour component of the offset, truncated toward zero. Negative for an
    /// offset west of UTC.
    function hours(self) -> int throws never {
        self._nanoseconds / 3600000000000
    }
    /// The minute component of the offset within its hour, truncated toward
    /// zero.
    ///
    /// Signed like `hours`, and for the same reason: the two are one signed
    /// quantity split in two, so `-09:30` reads back as `hours() == -9` and
    /// `minutes() == -30`. Neither reports any sub-minute part of the offset.
    function minutes(self) -> int throws never {
        self._nanoseconds / 60000000000 % 60
    }
}

/// The system's IANA timezone identifier, for example
/// `"America/Los_Angeles"`. Mirrors `Temporal.Now.timeZoneId()`.
///
/// An IO function, so hosts can swap the system-state source out.
///
/// # Throws
///
/// - `baml.errors.Io` if the host has a timezone database but cannot tell which
///   zone it is configured for — an unset or unrecognized system setting, which
///   a caller can reasonably fall back from (say, to UTC).
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if the host provides no timezone database at
///   all. Nothing the caller could have done differently, so it takes the panic
///   channel rather than the error one.
function system_timezone() -> string throws root.errors.Io {
    $rust_io_function
}

/// (internal) Resolves an IANA identifier to its offset (in nanoseconds) at the
/// absolute time `at_ns` (nanoseconds since the Unix epoch), using the host's
/// timezone database.
///
/// # Parameters
///
/// - `timezone`: an IANA identifier.
/// - `at_ns`: the absolute time to resolve at. The native host saturates values
///   beyond what it can represent rather than failing — a zone's offset is
///   constant past the last transition in its database, so the saturated answer
///   is the correct one.
///
/// # Returns
///
/// The offset east of UTC in nanoseconds, or `null` if the identifier is
/// unknown to the host (callers turn that into
/// `baml.time.UnknownTimezoneError`).
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if timezone data is unavailable. `throws
///   never` still holds: this is the panic channel, not the error one, which is
///   exactly what `//baml:fallible` marks.
//baml:fallible
function _tz_offset_at(timezone: string, at_ns: bigint) -> int? throws never {
    $rust_io_function
}

/// (internal) Locates a civil ("wall-clock") reading in an IANA timezone,
/// producing an absolute time. The lookup behind `PlainDateTime.to_zoned`.
///
/// # Parameters
///
/// - `timezone`: an IANA identifier.
/// - `civil_ns`: the wall-clock reading as nanoseconds since
///   1970-01-01T00:00:00, interpreted as if UTC — the encoding
///   `PlainDateTime._nanoseconds` uses.
/// - `disambiguation`: `"compatible"`, `"earlier"` or `"later"`. Any other
///   string is treated as `"compatible"`. `"reject"` never reaches here:
///   callers implement it by resolving both `"earlier"` and `"later"` and
///   requiring them to agree.
///
/// # Returns
///
/// The resolved absolute time as nanoseconds since the Unix epoch, or `null` if
/// the identifier is unknown to the host's timezone database (callers turn that
/// into `baml.time.UnknownTimezoneError`).
///
/// # Throws
///
/// - `baml.errors.Io` if the reading is outside the year range the host can
///   represent, or the host's timezone database fails to resolve it.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if timezone data is unavailable.
function _tz_to_instant(
    timezone: string,
    civil_ns: bigint,
    disambiguation: string,
) -> bigint? throws root.errors.Io {
    $rust_io_function
}
