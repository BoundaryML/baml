// `baml.time`: the calendar, the clock, parsing and writing, and the host's
// time zone database.

function time_n(n: int) -> int {
  let out = 0;
  for (let i = 0; i < n; i += 1) { out += 1; };
  out
}
// `seconds` seconds in nanoseconds, multiplied at run time.
function time_ns(seconds: int) -> bigint {
  let unit = 0n;
  for (let i = 0; i < 10; i += 1) { unit = unit + 100000000n; };
  unit * seconds
}

// PlainDate.
function time_date_from_components() -> string {
  baml.time.PlainDate.from_components(time_n(2024), 2, 29)._to_string_impl()
}
function time_date_parts() -> string {
  let date = baml.time.PlainDate.from_components(time_n(1969), 12, 31);
  `${date.year()} ${date.month()} ${date.day()} ${date._days}`
}
function time_date_before_year_zero() -> string {
  baml.time.PlainDate.from_components(0 - time_n(44), 3, 15)._to_string_impl()
}
function time_date_bad_month() -> string {
  baml.time.PlainDate.from_components(time_n(2024), 13, 1)._to_string_impl()
}
function time_date_bad_day() -> string {
  baml.time.PlainDate.from_components(time_n(2023), 2, 29)._to_string_impl()
}
function time_date_bad_year() -> string {
  baml.time.PlainDate.from_components(time_n(3) * 100000000000, 1, 1)._to_string_impl()
}
function time_date_parse() -> string {
  let date = baml.time.PlainDate.parse(rep("2026", 1) + "-03-18");
  `${date._days} ${date._to_string_impl()}`
}
function time_date_parse_fails() -> string {
  baml.time.PlainDate.parse(rep("2026", 1) + "-3-18")._to_string_impl()
}
function time_date_parse_caught() -> string {
  let date = baml.time.PlainDate.parse(rep("not a date", 1)) catch (e) {
    baml.errors.ParseError => { return e.message; },
  };
  date._to_string_impl()
}
function time_date_out_of_range() -> int {
  baml.time.PlainDate { _days: time_n(4) * 1000000000000 }.year()
}
function time_date_to_plain_datetime() -> string {
  baml.time.PlainDate.from_components(time_n(2000), 1, 1)._to_plain_datetime(time_n(3723) * 1000000000 + 5000000)._to_string_impl()
}

// PlainDateTime.
function time_datetime_from_components() -> string {
  baml.time.PlainDateTime._from_components(time_n(2026), 3, 18, 7, 32, 0, 500, 0, 0)._to_string_impl()
}
function time_datetime_parts() -> string {
  let at = baml.time.PlainDateTime._from_components(time_n(1999), 12, 31, 23, 59, 58, 123, 456, 789);
  `${at.year()} ${at.month()} ${at.day()} ${at.hour()} ${at.minute()} ${at.second()} ${at.millisecond()} ${at._nanoseconds}`
}
function time_datetime_before_1970() -> string {
  let at = baml.time.PlainDateTime._from_components(time_n(1960), 6, 15, 12, 0, 0, 0, 0, 1);
  `${at._to_string_impl()} ${at.to_plain_date()._to_string_impl()} ${at.to_plain_time()._to_string_impl()}`
}
function time_datetime_bad_hour() -> string {
  baml.time.PlainDateTime._from_components(time_n(2026), 3, 18, 24, 0, 0, 0, 0, 0)._to_string_impl()
}
function time_datetime_bad_nanosecond() -> string {
  baml.time.PlainDateTime._from_components(time_n(2026), 3, 18, 0, 0, 0, 0, 0, 1000)._to_string_impl()
}
function time_datetime_parse() -> string {
  let at = baml.time.PlainDateTime.parse(rep("2026-03-18T13:04:27", 1) + ".25");
  `${at._nanoseconds} ${at._to_string_impl()}`
}
function time_datetime_parse_fails() -> string {
  baml.time.PlainDateTime.parse(rep("2026-03-18T13:04:27Z", 1))._to_string_impl()
}
function time_datetime_out_of_range() -> string {
  baml.time.PlainDateTime { _nanoseconds: time_ns(3) * 1000000000000000000000000000000n }._to_string_impl()
}
function time_datetime_to_plain_date_out_of_range() -> int {
  baml.time.PlainDateTime { _nanoseconds: time_ns(3) * 100000000000000000000000000000000000000000000n }.to_plain_date()._days
}
function time_datetime_to_plain_time_of_anything() -> int {
  baml.time.PlainDateTime { _nanoseconds: 0n - time_ns(3) * 100000000000000000000000000000000000000000000n - 1n }.to_plain_time()._nanoseconds
}

// PlainTime.
function time_time_from_components() -> string {
  let time = baml.time.PlainTime._from_components(time_n(23), 59, 59, 999, 999, 999);
  `${time._nanoseconds} ${time._to_string_impl()}`
}
function time_time_bad_minute() -> string {
  baml.time.PlainTime._from_components(time_n(1), 60, 0, 0, 0, 0)._to_string_impl()
}
function time_time_negative_second() -> string {
  baml.time.PlainTime._from_components(time_n(1), 0, 0 - time_n(1), 0, 0, 0)._to_string_impl()
}
function time_time_parse() -> string {
  let time = baml.time.PlainTime.parse(rep("07:32:00", 1));
  `${time._nanoseconds} ${time._to_string_impl()} ${baml.time.PlainTime.parse("07:32:00.5")._to_string_impl()} ${baml.time.PlainTime.parse("00:00:00.000000001")._to_string_impl()}`
}
function time_time_parse_fails() -> string {
  baml.time.PlainTime.parse(rep("25:00:00", 1))._to_string_impl()
}
// A reading past a day is written within one.
function time_time_wraps() -> string {
  `${baml.time.PlainTime { _nanoseconds: time_n(25) * 3600000000000 }._to_string_impl()} ${baml.time.PlainTime { _nanoseconds: 0 - time_n(1) }._to_string_impl()}`
}

// Instant.
function time_instant_parse() -> string {
  let at = baml.time.Instant.parse(rep("2026-03-18T13:04:27", 1) + ".5-07:00");
  `${at._nanoseconds} ${at._to_string_impl()}`
}
function time_instant_parse_iso() -> bigint {
  baml.time.Instant.parse(rep("2026-03-18T13:04:27", 1) + "Z")._nanoseconds
}
function time_instant_parse_rfc2822() -> bigint {
  baml.time.Instant.parse(rep("Wed, 18 Mar 2026 13:04:27 +0000", 1))._nanoseconds
}
function time_instant_parse_fails() -> bigint {
  baml.time.Instant.parse(rep("yesterday", 1))._nanoseconds
}
function time_instant_before_1970() -> string {
  baml.time.Instant { _nanoseconds: 0n - time_ns(86400) - 1n }._to_string_impl()
}
function time_instant_out_of_range() -> string {
  baml.time.Instant { _nanoseconds: time_ns(3) * 1000000000000000000000000000000n }._to_string_impl()
}
// The clock is the host's: later than this test was written, and not going back.
function time_instant_now() -> bool {
  let first = baml.time.Instant.now()._nanoseconds;
  let second = baml.time.Instant.now()._nanoseconds;
  first > 1700000000000000000n && second >= first
}

// TimeZoneOffset.
function time_offset_from_duration() -> int {
  baml.time.TimeZoneOffset.from_duration(baml.time.Duration { _nanoseconds: 0n - time_ns(7 * 3600) })._nanoseconds
}
function time_offset_at_the_limit() -> int {
  baml.time.TimeZoneOffset.from_duration(baml.time.Duration { _nanoseconds: time_ns(24 * 3600) })._nanoseconds
}
function time_offset_out_of_range() -> int {
  baml.time.TimeZoneOffset.from_duration(baml.time.Duration { _nanoseconds: time_ns(24 * 3600) + 1n })._nanoseconds
}
function time_offset_far_out_of_range() -> int {
  baml.time.TimeZoneOffset.from_duration(baml.time.Duration { _nanoseconds: time_ns(3) * 100000000000000000000000000000n })._nanoseconds
}

// ZonedDateTime.
function time_zoned_parse_offset() -> string {
  let at = baml.time.ZonedDateTime.parse(rep("2026-03-18T13:04:27", 1) + "-07:00");
  `${at._nanoseconds} ${at._offset_ns ?? 1} ${at._iana ?? "-"}`
}
function time_zoned_parse_zone() -> string {
  let at = baml.time.ZonedDateTime.parse(rep("2026-03-18T13:04:27", 1) + "-07:00[America/Los_Angeles]");
  `${at._nanoseconds} ${at._offset_ns ?? 1} ${at._iana ?? "-"}`
}
function time_zoned_parse_critical_zone() -> string {
  baml.time.ZonedDateTime.parse(rep("2026-03-18T13:04:27", 1) + "Z[!Europe/Paris]")._iana ?? "-"
}
function time_zoned_parse_empty_zone() -> string {
  baml.time.ZonedDateTime.parse(rep("2026-03-18T13:04:27", 1) + "Z[]")._iana ?? "-"
}
function time_zoned_parse_zoneless() -> string {
  baml.time.ZonedDateTime.parse(rep("2026-03-18T13:04:27", 1))._iana ?? "-"
}
function time_format_zoned() -> string {
  `${baml.time._format_zoned(time_ns(1773864267), 0, null)} ${baml.time._format_zoned(time_ns(1773864267), 0 - 7 * 3600000000000, null)} ${baml.time._format_zoned(time_ns(1773864267) + 5n, 19800000000000, rep("Asia/Kolkata", 1))} ${baml.time._format_zoned(time_ns(1773864267), 0, rep("UTC", 1))}`
}
function time_format_zoned_out_of_range() -> string {
  baml.time._format_zoned(time_ns(3) * 1000000000000000000000000000000n, 0, null)
}

// The host's time zone database.
function time_tz_offset_at() -> string {
  let winter = baml.time._tz_offset_at(rep("America/New_York", 1), time_ns(1768000000));
  let summer = baml.time._tz_offset_at(rep("America/New_York", 1), time_ns(1783000000));
  `${winter ?? 1} ${summer ?? 1} ${baml.time._tz_offset_at("UTC", time_ns(0)) ?? 1}`
}
function time_tz_offset_at_unknown_zone() -> bool {
  baml.time._tz_offset_at(rep("Mars/Olympus_Mons", 1), time_ns(0)) == null
}
function time_tz_offset_far_away() -> string {
  let late = baml.time._tz_offset_at(rep("Asia/Tokyo", 1), time_ns(3) * 1000000000000000000000000000000n);
  let early = baml.time._tz_offset_at(rep("Asia/Tokyo", 1), 0n - time_ns(3) * 1000000000000000000000000000000n);
  `${late ?? 1} ${(early ?? 1) != 1}`
}
// 02:30 on 2026-03-08 does not exist in New York, and 01:30 on 2026-11-01
// exists twice.
function time_tz_to_instant() -> string {
  let gap = baml.time.PlainDateTime._from_components(time_n(2026), 3, 8, 2, 30, 0, 0, 0, 0)._nanoseconds;
  let fold = baml.time.PlainDateTime._from_components(time_n(2026), 11, 1, 1, 30, 0, 0, 0, 0)._nanoseconds;
  let zone = rep("America/New_York", 1);
  `${baml.time._tz_to_instant(zone, gap, "compatible") ?? 0n} ${baml.time._tz_to_instant(zone, gap, "earlier") ?? 0n} ${baml.time._tz_to_instant(zone, gap, "later") ?? 0n} ${baml.time._tz_to_instant(zone, fold, "compatible") ?? 0n} ${baml.time._tz_to_instant(zone, fold, "earlier") ?? 0n} ${baml.time._tz_to_instant(zone, fold, "later") ?? 0n}`
}
function time_tz_to_instant_unknown_zone() -> bool {
  baml.time._tz_to_instant(rep("Mars/Olympus_Mons", 1), time_ns(0), "compatible") == null
}
function time_tz_to_instant_out_of_range() -> bigint {
  baml.time._tz_to_instant(rep("Asia/Tokyo", 1), time_ns(3) * 1000000000000000000000000000000n, "compatible") ?? 0n
}
function time_system_timezone() -> bool {
  baml.time.system_timezone().length() > 0
}
