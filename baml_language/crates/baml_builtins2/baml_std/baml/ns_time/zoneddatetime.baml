/// A timezone-aware point in time: an absolute instant plus a timezone, either
/// a fixed offset (`-07:00`) or an IANA identifier (`America/Los_Angeles`).
///
/// Equivalent to `Temporal.ZonedDateTime` (TC39). The timezone does not affect
/// the absolute time; it affects how the date and time components are read and
/// how the value is formatted. What is stored is the absolute time, as in
/// `Instant`, rather than calendar components, so a value is never ambiguous
/// across a DST transition.
///
/// An IANA identifier is kept exactly as supplied and is never validated on
/// construction. A bad one surfaces at the first call that has to resolve it:
/// `timezone_offset`, `to_plain`, a component accessor, or `to_string`.
///
/// Every component accessor below reads through `to_plain`, and so shares one
/// failure set: it throws `baml.time.UnknownTimezoneError` for an IANA
/// identifier the host does not know, throws `baml.errors.InvalidArgument` if
/// the local reading is outside the ±9999 year range, and panics with
/// `baml.panics.HostUnavailable` if the host has no timezone database at all.
class ZonedDateTime {
    /// (internal) Nanoseconds since the Unix epoch — an inlined `Instant`, and
    /// the absolute time this value denotes.
    _nanoseconds: bigint,
    /// (internal) The fixed offset in nanoseconds, if the timezone is a fixed
    /// offset. Invariant: exactly one of `_offset_ns` / `_iana` is non-null.
    _offset_ns: int?,
    /// (internal) The IANA identifier, if the timezone is one. Not validated on
    /// construction — see `parse`.
    _iana: string?,

    /// The current time in the system timezone, kept as an IANA identifier
    /// (e.g. `"America/Los_Angeles"`) rather than as today's offset. Mirrors
    /// `Temporal.Now.zonedDateTimeISO()`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.Io` if the host has a timezone database but cannot tell
    ///   which zone it is configured for.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host provides no clock, or no
    ///   timezone database at all.
    function now() -> ZonedDateTime throws root.errors.Io {
        ZonedDateTime.from_instant(Instant.now(), system_timezone())
    }
    /// The current time in `timezone`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host provides no clock.
    function now_in(timezone: TimeZoneOffset | string) -> ZonedDateTime throws never {
        ZonedDateTime.from_instant(Instant.now(), timezone)
    }

    /// Pairs an absolute time with a timezone, without moving it: the result
    /// denotes the same moment `instant` does, and only its date and time
    /// components and its rendering differ.
    ///
    /// Total, and never ambiguous — a gap or an overlap is a property of a
    /// civil reading, and this starts from an absolute time.
    ///
    /// # Examples
    ///
    /// ```baml
    /// ZonedDateTime.from_instant(Instant.now(), "America/Los_Angeles")
    /// ```
    function from_instant(
        instant: Instant,
        timezone: TimeZoneOffset | string,
    ) -> ZonedDateTime throws never {
        match (timezone) {
            TimeZoneOffset => {
                ZonedDateTime {
                    _nanoseconds: instant._nanoseconds,
                    _offset_ns: timezone._nanoseconds,
                    _iana: null,
                }
            },
            string => {
                ZonedDateTime {
                    _nanoseconds: instant._nanoseconds,
                    _offset_ns: null,
                    _iana: timezone,
                }
            },
        }
    }
    /// The absolute time, dropping the timezone. The inverse of `from_instant`,
    /// and infallible for the same reason: the absolute time is what is stored,
    /// so no timezone database is consulted.
    ///
    /// Contrast `to_plain`, which keeps the components and drops the moment.
    function to_instant(self) -> Instant throws never {
        Instant { _nanoseconds: self._nanoseconds }
    }

    /// Creates a `ZonedDateTime` from calendar and clock components read in
    /// `timezone` — i.e. from a wall-clock reading someone in that zone would
    /// give you.
    ///
    /// Equivalent to building a `PlainDateTime` from the same components and
    /// calling `to_zoned` on it, and it inherits that method's DST handling.
    ///
    /// Every clock component and `disambiguation` are defaulted and therefore
    /// named-only: `ZonedDateTime.from_components(tz, 1979, 5, 27, hour = 7)`.
    ///
    /// # Parameters
    ///
    /// - `timezone`: a `TimeZoneOffset` for a fixed offset, or an IANA
    ///   identifier. Leads rather than trails the components because the
    ///   components are meaningless without it.
    /// - `year`: the proleptic Gregorian year, in `[-9999, 9999]`.
    /// - `month`: 1-based, in `[1, 12]`.
    /// - `day`: 1-based, and must exist in that month of that year.
    /// - `hour`: in `[0, 23]`.
    /// - `minute`, `second`: each in `[0, 59]`.
    /// - `millisecond`, `microsecond`, `nanosecond`: each in `[0, 999]`, and
    ///   each an independent digit group.
    /// - `disambiguation`: how to resolve a DST gap or overlap; see
    ///   `Disambiguation`. Ignored for a `TimeZoneOffset`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if a component is out of range.
    /// - `baml.time.UnknownTimezoneError` if `timezone` is a string the host's
    ///   timezone database does not know.
    /// - `baml.time.AmbiguousTimeError` if `disambiguation` is `"reject"` and
    ///   the reading falls in a DST gap or overlap.
    /// - `baml.errors.Io` if the host's timezone database fails to resolve the
    ///   reading.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the host has no timezone database at
    ///   all.
    function from_components(
        timezone: TimeZoneOffset | string,
        year: int,
        month: int,
        day: int,
        hour: int = 0,
        minute: int = 0,
        second: int = 0,
        millisecond: int = 0,
        microsecond: int = 0,
        nanosecond: int = 0,
        disambiguation: Disambiguation = "compatible",
    ) -> ZonedDateTime throws root.errors.InvalidArgument | UnknownTimezoneError | AmbiguousTimeError | root.errors.Io {
        PlainDateTime
            .from_components(
                year,
                month,
                day,
                hour = hour,
                minute = minute,
                second = second,
                millisecond = millisecond,
                microsecond = microsecond,
                nanosecond = nanosecond,
            )
            .to_zoned(timezone, disambiguation = disambiguation)
    }

    /// Parses [RFC 3339](https://datatracker.ietf.org/doc/html/rfc3339)
    /// (`2026-03-18T13:04:27-07:00`) and
    /// [RFC 9557](https://datatracker.ietf.org/doc/html/rfc9557)
    /// (`2026-03-18T13:04:27-07:00[America/Los_Angeles]`) strings. Zoneless
    /// strings are rejected — parse those as `PlainDateTime`.
    ///
    /// The numeric offset always determines the absolute time. When a bracketed
    /// IANA annotation is also present it becomes the timezone and the numeric
    /// offset is not retained, so a string whose offset disagrees with what the
    /// named zone had at that moment is accepted as written rather than
    /// rejected. RFC 9557's leading `!` critical-annotation marker is accepted
    /// and stripped; the identifier is used either way.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `s` is not an RFC 3339 timestamp, or its
    ///   bracketed annotation is empty.
    //baml:mut_vm
    function parse(s: string) -> ZonedDateTime throws root.errors.ParseError {
        $rust_function
    }

    implements baml.ToString {
        /// Serializes the way Temporal does: RFC 9557 with the bracketed
        /// annotation when the timezone is an IANA identifier
        /// (`2026-03-18T13:04:27-07:00[America/Los_Angeles]`); plain RFC 3339
        /// when it is a fixed offset, with `Z` for the zero offset.
        ///
        /// Rendering an IANA-zoned value resolves the identifier against the
        /// host's timezone database, so this is the only `to_string` in
        /// `baml.time` that can fail for a reason other than the year range.
        ///
        /// # Panics
        ///
        /// - `baml.panics.UserPanic` if the value cannot be formatted: the year
        ///   is outside ±9999, or the timezone is an IANA identifier the host
        ///   does not know. Use `to_json` where either needs to be handled
        ///   rather than propagated.
        /// - `baml.panics.HostUnavailable` if the timezone is an IANA
        ///   identifier and the host has no timezone database at all. This one
        ///   is not re-labelled as a `UserPanic`: host unavailability travels
        ///   the panic channel, which a wildcard `catch` arm re-throws rather
        ///   than swallowing.
        function to_string(self) -> string throws never {
            _format_zoned(self._nanoseconds, self.timezone_offset()._nanoseconds, self._iana) catch (_) {
                _ => {
                    root.sys.panic(
                        "ZonedDateTime.to_string: value cannot be formatted (year out of range, unknown timezone, or IO error)",
                    )
                }
            }
        }
    }

    /// The later of `self` and `other`; `self` if they name the same moment.
    ///
    /// An absolute-time comparison, so it is meaningful across timezones: the
    /// two values need not share one, and the timezone of the winner comes
    /// along unchanged. No timezone database is consulted.
    function max(self, other: ZonedDateTime) -> ZonedDateTime throws never {
        if (other._nanoseconds > self._nanoseconds) {
            other
        } else {
            self
        }
    }
    /// The earlier of `self` and `other`; `self` if they name the same moment.
    /// An absolute-time comparison, with the same properties as `max`.
    function min(self, other: ZonedDateTime) -> ZonedDateTime throws never {
        if (other._nanoseconds < self._nanoseconds) {
            other
        } else {
            self
        }
    }

    /// The same moment, presented in a different timezone: the date and time
    /// components change, the instant does not.
    ///
    /// A re-labelling, not a shift — `with_timezone` never moves the value
    /// along the timeline, so it cannot land in a DST gap and takes no
    /// `Disambiguation`. To keep the *components* and change the moment, go
    /// through `to_plain` and `PlainDateTime.to_zoned` instead.
    function with_timezone(self, timezone: TimeZoneOffset | string) -> ZonedDateTime throws never {
        ZonedDateTime.from_instant(self.to_instant(), timezone)
    }

    /// The timezone, in whichever form it was supplied: a `TimeZoneOffset` if
    /// fixed, or the IANA identifier as a `string`. Use `timezone_offset` for
    /// the concrete offset either way.
    function timezone(self) -> TimeZoneOffset | string throws never {
        match (self._iana) {
            null => TimeZoneOffset { _nanoseconds: self._offset_ns ?? 0 },
            // `?? ""` is unreachable: the null arm above already matched.
            let iana => iana ?? "",
        }
    }

    /// The concrete `TimeZoneOffset` in effect at this moment.
    ///
    /// A fixed offset is returned as-is. An IANA identifier is resolved against
    /// the host's timezone database at `self`'s absolute time, so the answer is
    /// DST-aware and specific to this moment — the same value's offset six
    /// months later may differ.
    ///
    /// # Throws
    ///
    /// - `baml.time.UnknownTimezoneError` if the timezone is an unknown IANA
    ///   identifier.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if timezone data is unavailable.
    function timezone_offset(self) -> TimeZoneOffset throws UnknownTimezoneError {
        match (self._iana) {
            null => TimeZoneOffset { _nanoseconds: self._offset_ns ?? 0 },
            // The `?? ""` / `?? 0` fallbacks are unreachable: the null arms
            // already matched.
            let iana => match (_tz_offset_at(iana ?? "", self._nanoseconds)) {
                null => throw UnknownTimezoneError { timezone: iana ?? "" },
                let offset => TimeZoneOffset { _nanoseconds: offset ?? 0 },
            },
        }
    }

    /// Drops the timezone, keeping the wall-clock reading a local observer
    /// would give. The counterpart of `PlainDateTime.to_zoned`, and the
    /// opposite trade from `to_instant`, which keeps the moment and drops the
    /// components.
    ///
    /// Resolving the reading needs the offset in effect at this moment, so this
    /// consults the host's timezone database whenever the timezone is an IANA
    /// identifier.
    ///
    /// Lossy: converting back with `to_zoned` recovers `self` only when the
    /// reading is unambiguous in that zone. During a fall-back overlap the same
    /// reading names two moments, and only one comes back.
    ///
    /// # Throws
    ///
    /// - `baml.time.UnknownTimezoneError` if the timezone is an IANA identifier
    ///   the host does not know.
    ///
    /// # Panics
    ///
    /// - `baml.panics.HostUnavailable` if the timezone is an IANA identifier
    ///   and the host has no timezone database.
    function to_plain(self) -> PlainDateTime throws UnknownTimezoneError {
        PlainDateTime { _nanoseconds: self._nanoseconds + self.timezone_offset()._nanoseconds }
    }

    /// The calendar year as read in this timezone.
    function year(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().year()
    }
    /// The calendar month, in `[1, 12]`, as read in this timezone.
    function month(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().month()
    }
    /// The day of the month, in `[1, 31]`, as read in this timezone.
    ///
    /// This is the *local* day: the same moment can fall on different dates in
    /// two timezones.
    function day(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().day()
    }
    /// The hour of the day, in `[0, 23]`, as read in this timezone.
    ///
    /// Across a DST transition this is not a continuous function of the
    /// absolute time: an hour is skipped in spring and repeated in autumn.
    function hour(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().hour()
    }
    /// The minute of the hour, in `[0, 59]`, as read in this timezone.
    function minute(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().minute()
    }
    /// The second of the minute, in `[0, 59]`, as read in this timezone.
    function second(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().second()
    }
    /// The millisecond of the second, in `[0, 999]`, as read in this timezone.
    ///
    /// The finer components have no accessor of their own; `to_string` renders
    /// them.
    function millisecond(self) -> int throws root.errors.InvalidArgument | UnknownTimezoneError {
        self.to_plain().millisecond()
    }

    implements baml.FromJson {
        /// Decodes a `ZonedDateTime` from a JSON string, accepting exactly what
        /// `ZonedDateTime.parse` accepts.
        ///
        /// # Throws
        ///
        /// - `baml.json.DecodeError` if the value is not a string, or is a
        ///   string `ZonedDateTime.parse` rejects — including a zoneless
        ///   timestamp, which belongs to `PlainDateTime`.
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            match (j) {
                string => {
                    ZonedDateTime.parse(j) catch (_) {
                        root.errors.ParseError { message } => {
                            throw baml.json.DecodeError { message: message, path: "" }
                        },
                    }
                },
                _ => {
                    throw baml.json.DecodeError { message: "Expected RFC 3339 / RFC 9557 timestamp string", path: "" }
                },
            }
        }
    }
    implements baml.ToJson {
        /// Encodes `self` as the same RFC 3339 / RFC 9557 string `to_string`
        /// produces.
        ///
        /// # Throws
        ///
        /// - `baml.json.SerializationError` if the year is outside ±9999, or
        ///   the timezone is an IANA identifier the host does not know. Both
        ///   panic in `to_string`: a serializer is called with a caller
        ///   standing by to react, so it gets the error channel.
        ///
        /// # Panics
        ///
        /// - `baml.panics.HostUnavailable` if the timezone is an IANA
        ///   identifier and the host has no timezone database.
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            _format_zoned(self._nanoseconds, self.timezone_offset()._nanoseconds, self._iana) catch (_) {
                root.errors.InvalidArgument { message } => {
                    throw baml.json.SerializationError {
                        message: message,
                        path: "",
                        reason: "unserializable",
                    }
                },
                UnknownTimezoneError { timezone } => {
                    throw baml.json.SerializationError {
                        message: "unknown timezone: " + timezone,
                        path: "",
                        reason: "unserializable",
                    }
                },
            }
        }
    }
}

/// (internal) Formats an absolute time as RFC 3339 at the given offset,
/// appending an RFC 9557 `[iana]` annotation when `iana` is non-null.
///
/// Takes the offset already resolved rather than resolving it itself, so the
/// timezone-database lookup stays in BAML where its failure can be routed to
/// either channel — `to_string` panics, `to_json` throws.
///
/// # Parameters
///
/// - `epoch_ns`: the absolute time, in nanoseconds since the Unix epoch.
/// - `offset_ns`: the offset to render at, in nanoseconds east of UTC. Any
///   sub-minute part is truncated toward zero, since RFC 3339 offsets are
///   `±hh:mm`.
/// - `iana`: the identifier for the bracketed annotation, or `null` for a fixed
///   offset. Its presence also decides the zero-offset spelling: `Z` for a
///   fixed offset, `+00:00` alongside the annotation otherwise — the way
///   Temporal serializes.
///
/// # Throws
///
/// - `baml.errors.InvalidArgument` if the local reading is outside the
///   supported ±9999 year range.
function _format_zoned(
    epoch_ns: bigint,
    offset_ns: int,
    iana: string?,
) -> string throws root.errors.InvalidArgument {
    $rust_function
}

implement root.ops.Add<Duration> for ZonedDateTime {
    type Output = ZonedDateTime
    /// The moment `other` after `self`, in the same timezone.
    ///
    /// Absolute-time arithmetic, matching Temporal's treatment of exact
    /// (sub-day) duration units: adding 24 hours advances the instant by
    /// exactly 24 hours, so across a DST transition the local clock reads an
    /// hour off from the day before. Do the arithmetic on `to_plain()` and
    /// re-zone the result when "same wall-clock time tomorrow" is what is
    /// meant.
    ///
    /// Never ambiguous, and never consults the timezone database — the timezone
    /// travels along untouched, and only a later reading of the components
    /// resolves it.
    function add(self, other: Duration) -> ZonedDateTime {
        ZonedDateTime {
            _nanoseconds: self._nanoseconds + other._nanoseconds,
            _offset_ns: self._offset_ns,
            _iana: self._iana,
        }
    }
}

implement root.ops.Subtract<Duration> for ZonedDateTime {
    type Output = ZonedDateTime
    /// The moment `other` before `self`, in the same timezone. Absolute-time
    /// arithmetic, with the same caveats as `ZonedDateTime + Duration`.
    function sub(self, other: Duration) -> ZonedDateTime {
        ZonedDateTime {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
            _offset_ns: self._offset_ns,
            _iana: self._iana,
        }
    }
}

implement root.ops.Subtract<ZonedDateTime> for ZonedDateTime {
    type Output = Duration
    /// The signed gap from `other` to `self`: positive when `self` is the later
    /// moment, negative when it is the earlier.
    ///
    /// Elapsed absolute time, so the two values need not share a timezone and
    /// the answer counts every second that actually passed — including the one
    /// a DST transition added to or removed from the local clock.
    function sub(self, other: ZonedDateTime) -> Duration {
        Duration {
            _nanoseconds: self._nanoseconds - other._nanoseconds,
        }
    }
}
