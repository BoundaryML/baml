// Go deliberately projects baml.time values through the ordinary class
// pipeline. These fixtures prove that representation stays lossless in both
// directions without introducing a second, host-specific datetime model.

class TimeEnvelope {
  instant baml.time.Instant
  duration baml.time.Duration
  plain_date baml.time.PlainDate
  plain_time baml.time.PlainTime
  plain_date_time baml.time.PlainDateTime
  time_zone_offset baml.time.TimeZoneOffset
  zoned_date_time baml.time.ZonedDateTime

  optional_instant baml.time.Instant?
  instant_list baml.time.Instant[]
  optional_duration_list baml.time.Duration?[]
  plain_date_map map<string, baml.time.PlainDate>
  optional_plain_time_map map<string, baml.time.PlainTime?>
  plain_date_time_list baml.time.PlainDateTime[]
  time_zone_offset_map map<string, baml.time.TimeZoneOffset>
  optional_zoned_date_time_list baml.time.ZonedDateTime?[]
}

class TimeFields {
  instant_ns bigint
  duration_ns bigint
  plain_date_days int
  plain_time_ns int
  plain_date_time_ns bigint
  time_zone_offset_ns int
  zoned_date_time_ns bigint
  zoned_date_time_offset_ns int?
  zoned_date_time_iana string?
}

function round_trip_instant(value: baml.time.Instant) -> baml.time.Instant {
  value
}

function round_trip_duration(value: baml.time.Duration) -> baml.time.Duration {
  value
}

function round_trip_plain_date(value: baml.time.PlainDate) -> baml.time.PlainDate {
  value
}

function round_trip_plain_time(value: baml.time.PlainTime) -> baml.time.PlainTime {
  value
}

function round_trip_plain_date_time(value: baml.time.PlainDateTime) -> baml.time.PlainDateTime {
  value
}

function round_trip_time_zone_offset(value: baml.time.TimeZoneOffset) -> baml.time.TimeZoneOffset {
  value
}

function round_trip_zoned_date_time(value: baml.time.ZonedDateTime) -> baml.time.ZonedDateTime {
  value
}

function round_trip_time_envelope(value: TimeEnvelope) -> TimeEnvelope {
  value
}

function inspect_time_envelope(value: TimeEnvelope) -> TimeFields {
  TimeFields {
    instant_ns: value.instant._nanoseconds,
    duration_ns: value.duration._nanoseconds,
    plain_date_days: value.plain_date._days,
    plain_time_ns: value.plain_time._nanoseconds,
    plain_date_time_ns: value.plain_date_time._nanoseconds,
    time_zone_offset_ns: value.time_zone_offset._nanoseconds,
    zoned_date_time_ns: value.zoned_date_time._nanoseconds,
    zoned_date_time_offset_ns: value.zoned_date_time._offset_ns,
    zoned_date_time_iana: value.zoned_date_time._iana,
  }
}

function round_trip_optional_instant(value: baml.time.Instant?) -> baml.time.Instant? {
  value
}

function default_duration(
  value: baml.time.Duration = baml.time.Duration.from_nanoseconds(123456789012345678901234567890n)
) -> baml.time.Duration {
  value
}

function default_zoned_date_time(
  value: baml.time.ZonedDateTime? = null
) -> baml.time.ZonedDateTime? {
  value
}

function instant_from_timestamp_nanoseconds(ns: bigint) -> baml.time.Instant {
  baml.time.Instant.from_timestamp_nanoseconds(ns)
}

function parse_instant(value: string) -> baml.time.Instant {
  baml.time.Instant.parse(value)
}

function duration_from_nanoseconds(ns: bigint) -> baml.time.Duration {
  baml.time.Duration.from_nanoseconds(ns)
}

function plain_date_from_components(year: int, month: int, day: int) -> baml.time.PlainDate {
  baml.time.PlainDate.from_components(year, month, day)
}

function parse_plain_date(value: string) -> baml.time.PlainDate {
  baml.time.PlainDate.parse(value)
}

function plain_time_from_components(
  hour: int,
  minute: int,
  second: int,
  millisecond: int,
  microsecond: int,
  nanosecond: int,
) -> baml.time.PlainTime {
  baml.time.PlainTime.from_components(
    hour,
    minute = minute,
    second = second,
    millisecond = millisecond,
    microsecond = microsecond,
    nanosecond = nanosecond,
  )
}

function parse_plain_time(value: string) -> baml.time.PlainTime {
  baml.time.PlainTime.parse(value)
}

function plain_date_time_from_components(
  year: int,
  month: int,
  day: int,
  hour: int,
  minute: int,
  second: int,
  millisecond: int,
  microsecond: int,
  nanosecond: int,
) -> baml.time.PlainDateTime {
  baml.time.PlainDateTime.from_components(
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
}

function parse_plain_date_time(value: string) -> baml.time.PlainDateTime {
  baml.time.PlainDateTime.parse(value)
}

function time_zone_offset_from_components(hours: int, minutes: int) -> baml.time.TimeZoneOffset {
  baml.time.TimeZoneOffset.new(hours, minutes)
}

function zoned_date_time_from_fixed_offset(
  instant: baml.time.Instant,
  offset: baml.time.TimeZoneOffset,
) -> baml.time.ZonedDateTime {
  baml.time.ZonedDateTime.from_instant(instant, offset)
}

function zoned_date_time_from_iana(
  instant: baml.time.Instant,
  iana: string,
) -> baml.time.ZonedDateTime {
  baml.time.ZonedDateTime.from_instant(instant, iana)
}

function parse_zoned_date_time(value: string) -> baml.time.ZonedDateTime {
  baml.time.ZonedDateTime.parse(value)
}
