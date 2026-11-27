//! `baml.time`: the calendar and the clock behind `Instant`, the three
//! plain types, `TimeZoneOffset` and `ZonedDateTime`.
//!
//! Every value is plain data in its instance: nanoseconds as a `bigint` or
//! an `int`, or days as an `int`. The calendar is the `time` crate's and the
//! time zone database is the host's, read through `jiff`, as the
//! interpreter reads them: a message that quotes either says the same here.

use std::fmt::Write as _;

use num_bigint::BigInt;
use time::{
    Date, Month, OffsetDateTime, PrimitiveDateTime, Time,
    format_description::{
        BorrowedFormatItem,
        well_known::{Iso8601, Rfc2822, Rfc3339},
    },
    macros::{date, format_description},
};

use super::{
    Big, BigRef, NativeBamlTime, NativeBamlTimeInstant, NativeBamlTimePlainDate,
    NativeBamlTimePlainDateTime, NativeBamlTimePlainTime, NativeBamlTimeTimeZoneOffset,
    NativeBamlTimeZonedDateTime, Natives, Obj, Root, Scope, Str, StrRef, Thrown,
};
use crate::layout::{
    NAMED_TIME_INSTANT, NAMED_TIME_PLAIN_DATE, NAMED_TIME_PLAIN_DATE_TIME, NAMED_TIME_PLAIN_TIME,
    NAMED_TIME_ZONE_OFFSET, NAMED_TIME_ZONED_DATE_TIME,
};

const NANOS_PER_SECOND: i64 = 1_000_000_000;
const NANOS_PER_MINUTE: i64 = 60 * NANOS_PER_SECOND;
const NANOS_PER_HOUR: i64 = 60 * NANOS_PER_MINUTE;
const NANOS_PER_DAY: i64 = 24 * NANOS_PER_HOUR;
const EPOCH: Date = date!(1970 - 01 - 01);

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");
const TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second][optional [.[subsecond]]]");
const DATE_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]");

/// A wall-clock reading, nanoseconds since 1970 as if it were UTC, as a
/// date and a time, or the message of the error for one past the years
/// the calendar has.
fn civil(nanoseconds: &BigInt, what: &str) -> Result<OffsetDateTime, String> {
    i128::try_from(nanoseconds)
        .ok()
        .and_then(|nanoseconds| OffsetDateTime::from_unix_timestamp_nanos(nanoseconds).ok())
        .ok_or_else(|| format!("{what}: value is outside the supported year range"))
}

fn date_of(year: i64, month: i64, day: i64) -> Result<Date, String> {
    let in_year = i32::try_from(year).map_err(|_| format!("year {year} is out of range"))?;
    let in_month = u8::try_from(month)
        .ok()
        .and_then(|month| Month::try_from(month).ok())
        .ok_or_else(|| format!("month {month} is out of range [1, 12]"))?;
    let in_day = u8::try_from(day).map_err(|_| format!("day {day} is out of range"))?;
    Date::from_calendar_date(in_year, in_month, in_day)
        .map_err(|error| format!("invalid date {year:04}-{month:02}-{day:02}: {error}"))
}

fn days_since_epoch(date: Date) -> i64 {
    (date - EPOCH).whole_days()
}

fn date_for_days(days: i64, what: &str) -> Result<Date, String> {
    EPOCH
        .checked_add(time::Duration::days(days))
        .ok_or_else(|| format!("{what}: date is outside the supported year range"))
}

/// The nanoseconds since midnight of a clock reading, each part checked.
fn clock_nanos(parts: [i64; 6]) -> Result<i64, String> {
    const PARTS: [(&str, i64, i64); 6] = [
        ("hour", 23, NANOS_PER_HOUR),
        ("minute", 59, NANOS_PER_MINUTE),
        ("second", 59, NANOS_PER_SECOND),
        ("millisecond", 999, 1_000_000),
        ("microsecond", 999, 1_000),
        ("nanosecond", 999, 1),
    ];
    let mut nanoseconds = 0;
    for (value, (name, most, each)) in parts.into_iter().zip(PARTS) {
        if !(0..=most).contains(&value) {
            return Err(format!("{name} {value} is out of range [0, {most}]"));
        }
        nanoseconds += value * each;
    }
    Ok(nanoseconds)
}

/// `2026-03-18`. A year before zero carries its sign.
fn write_date(out: &mut String, date: Date) {
    let year = date.year();
    if year < 0 {
        let _ = write!(out, "-{:04}", -i64::from(year));
    } else {
        let _ = write!(out, "{year:04}");
    }
    let _ = write!(out, "-{:02}-{:02}", u8::from(date.month()), date.day());
}

/// `07:32:00`, or `07:32:00.5`: the zeros a fraction ends with are left
/// out.
fn write_clock(out: &mut String, hour: u8, minute: u8, second: u8, nanosecond: u32) {
    let _ = write!(out, "{hour:02}:{minute:02}:{second:02}");
    if nanosecond != 0 {
        let digits = format!("{nanosecond:09}");
        let _ = write!(out, ".{}", digits.trim_end_matches('0'));
    }
}

fn write_date_time(out: &mut String, at: OffsetDateTime) {
    write_date(out, at.date());
    out.push('T');
    write_clock(out, at.hour(), at.minute(), at.second(), at.nanosecond());
}

/// The `bigint` in field `field` of `instance`.
fn big_field(scope: &Scope<'_>, instance: Root<'_, Obj>, field: usize) -> BigInt {
    // SAFETY: the classes of `baml.time` read here hold a `bigint` there.
    unsafe { instance.get(scope).field::<Big>(field) }.value()
}

/// The `int` in the one field of `instance`.
fn int_field(scope: &Scope<'_>, instance: Root<'_, Obj>) -> i64 {
    // SAFETY: the classes of `baml.time` read here hold one `int`.
    unsafe { instance.get(scope).slot(0) }.cast_signed() as i64
}

/// An instance of the class at `named` whose one field is a `bigint`.
fn of_big<'s>(scope: &mut Scope<'s>, named: usize, nanoseconds: i128) -> Root<'s, Obj> {
    // 128 bits are far within what a `bigint` holds.
    let nanoseconds = scope.make_big(&BigInt::from(nanoseconds));
    let class = scope.named_class(named);
    let mut made = scope.build(class);
    made.set(scope, 0, nanoseconds);
    made.finish()
}

/// An instance of the class at `named` whose one field is an `int`.
fn of_int<'s>(scope: &mut Scope<'s>, named: usize, value: i64) -> Root<'s, Obj> {
    let class = scope.named_class(named);
    let mut made = scope.build(class);
    made.set(scope, 0, value);
    made.finish()
}

/// A part of the date and time the `PlainDateTime` `instance` reads as.
fn civil_part<'s>(
    scope: &mut Scope<'s>,
    instance: Root<'s, Obj>,
    what: &str,
    part: impl FnOnce(OffsetDateTime) -> i64,
) -> Result<i64, Thrown<'s>> {
    match civil(&big_field(scope, instance, 0), what) {
        Ok(at) => Ok(part(at)),
        Err(message) => Err(scope.invalid_argument(&message)),
    }
}

/// A part of the date the `PlainDate` `instance` reads as.
fn date_part<'s>(
    scope: &mut Scope<'s>,
    instance: Root<'s, Obj>,
    what: &str,
    part: impl FnOnce(Date) -> i64,
) -> Result<i64, Thrown<'s>> {
    match date_for_days(int_field(scope, instance), what) {
        Ok(date) => Ok(part(date)),
        Err(message) => Err(scope.invalid_argument(&message)),
    }
}

impl NativeBamlTimeInstant for Natives {
    /// The wall clock, in nanoseconds since 1970.
    fn now<'s>(scope: &mut Scope<'s>) -> Root<'s, Obj> {
        let nanoseconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        of_big(scope, NAMED_TIME_INSTANT, nanoseconds.cast_signed())
    }

    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let text = s.get(scope).as_str();
        let parsed = OffsetDateTime::parse(text, &Rfc3339)
            .or_else(|_| OffsetDateTime::parse(text, &Iso8601::DEFAULT))
            .or_else(|_| OffsetDateTime::parse(text, &Rfc2822));
        match parsed {
            Ok(at) => Ok(of_big(scope, NAMED_TIME_INSTANT, at.unix_timestamp_nanos())),
            Err(error) => {
                let message = format!(
                    "Instant.parse: cannot parse {text:?} as an RFC 3339, ISO 8601, or RFC 2822 timestamp: {error}"
                );
                Err(scope.parse_error(&message))
            }
        }
    }

    fn _to_string_impl<'s>(
        scope: &mut Scope<'s>,
        instant: Root<'s, Obj>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let text = i128::try_from(&big_field(scope, instant, 0))
            .ok()
            .and_then(|nanoseconds| OffsetDateTime::from_unix_timestamp_nanos(nanoseconds).ok())
            .and_then(|at| at.format(&Rfc3339).ok());
        match text {
            Some(text) => Ok(scope.alloc_str(&text)),
            None => Err(scope.invalid_argument(
                "Could not serialize Instant with year out of Rfc3339 timestamp range.",
            )),
        }
    }
}

impl NativeBamlTimePlainDate for Natives {
    fn from_components<'s>(
        scope: &mut Scope<'s>,
        year: i64,
        month: i64,
        day: i64,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        match date_of(year, month, day) {
            Ok(date) => Ok(of_int(scope, NAMED_TIME_PLAIN_DATE, days_since_epoch(date))),
            Err(message) => Err(scope.invalid_argument(&message)),
        }
    }

    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let text = s.get(scope).as_str();
        match Date::parse(text, DATE_FORMAT) {
            Ok(date) => Ok(of_int(scope, NAMED_TIME_PLAIN_DATE, days_since_epoch(date))),
            Err(error) => {
                let message = format!(
                    "PlainDate.parse: cannot parse {text:?} as an ISO 8601 calendar date: {error}"
                );
                Err(scope.parse_error(&message))
            }
        }
    }

    fn _to_string_impl<'s>(
        scope: &mut Scope<'s>,
        plaindate: Root<'s, Obj>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        match date_for_days(int_field(scope, plaindate), "PlainDate.to_string") {
            Ok(date) => {
                let mut text = String::new();
                write_date(&mut text, date);
                Ok(scope.alloc_str(&text))
            }
            Err(message) => Err(scope.invalid_argument(&message)),
        }
    }

    fn _to_plain_datetime<'s>(
        scope: &mut Scope<'s>,
        plaindate: Root<'s, Obj>,
        time: i64,
    ) -> Root<'s, Obj> {
        let days = int_field(scope, plaindate);
        of_big(
            scope,
            NAMED_TIME_PLAIN_DATE_TIME,
            i128::from(days) * i128::from(NANOS_PER_DAY) + i128::from(time),
        )
    }

    fn year<'s>(scope: &mut Scope<'s>, plaindate: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        date_part(scope, plaindate, "PlainDate.year", |date| {
            i64::from(date.year())
        })
    }

    fn month<'s>(scope: &mut Scope<'s>, plaindate: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        date_part(scope, plaindate, "PlainDate.month", |date| {
            i64::from(u8::from(date.month()))
        })
    }

    fn day<'s>(scope: &mut Scope<'s>, plaindate: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        date_part(scope, plaindate, "PlainDate.day", |date| {
            i64::from(date.day())
        })
    }
}

impl NativeBamlTimePlainDateTime for Natives {
    fn _from_components<'s>(
        scope: &mut Scope<'s>,
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let nanoseconds = date_of(year, month, day).and_then(|date| {
            let clock = clock_nanos([hour, minute, second, millisecond, microsecond, nanosecond])?;
            Ok(i128::from(days_since_epoch(date)) * i128::from(NANOS_PER_DAY) + i128::from(clock))
        });
        match nanoseconds {
            Ok(nanoseconds) => Ok(of_big(scope, NAMED_TIME_PLAIN_DATE_TIME, nanoseconds)),
            Err(message) => Err(scope.invalid_argument(&message)),
        }
    }

    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let text = s.get(scope).as_str();
        match PrimitiveDateTime::parse(text, DATE_TIME_FORMAT) {
            Ok(at) => Ok(of_big(
                scope,
                NAMED_TIME_PLAIN_DATE_TIME,
                at.assume_utc().unix_timestamp_nanos(),
            )),
            Err(error) => {
                let message = format!(
                    "PlainDateTime.parse: cannot parse {text:?} as a zoneless ISO 8601 datetime: {error}"
                );
                Err(scope.parse_error(&message))
            }
        }
    }

    fn _to_string_impl<'s>(
        scope: &mut Scope<'s>,
        plaindatetime: Root<'s, Obj>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        match civil(
            &big_field(scope, plaindatetime, 0),
            "PlainDateTime.to_string",
        ) {
            Ok(at) => {
                let mut text = String::new();
                write_date_time(&mut text, at);
                Ok(scope.alloc_str(&text))
            }
            Err(message) => Err(scope.invalid_argument(&message)),
        }
    }

    fn to_plain_date<'s>(
        scope: &mut Scope<'s>,
        plaindatetime: Root<'s, Obj>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let days = i128::try_from(&big_field(scope, plaindatetime, 0))
            .ok()
            .map(|nanoseconds| nanoseconds.div_euclid(i128::from(NANOS_PER_DAY)))
            .and_then(|days| i64::try_from(days).ok());
        match days {
            Some(days) => Ok(of_int(scope, NAMED_TIME_PLAIN_DATE, days)),
            None => Err(scope.invalid_argument(
                "PlainDateTime.to_plain_date: value is outside the supported year range",
            )),
        }
    }

    /// The time of day, whatever the size or the sign of the reading.
    fn to_plain_time<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Root<'s, Obj> {
        let day = BigInt::from(NANOS_PER_DAY);
        let within = ((big_field(scope, plaindatetime, 0) % &day) + &day) % &day;
        let nanoseconds = i64::try_from(within).unwrap_or_default();
        of_int(scope, NAMED_TIME_PLAIN_TIME, nanoseconds)
    }

    fn year<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.year", |at| {
            i64::from(at.year())
        })
    }

    fn month<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.month", |at| {
            i64::from(u8::from(at.month()))
        })
    }

    fn day<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.day", |at| {
            i64::from(at.day())
        })
    }

    fn hour<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.hour", |at| {
            i64::from(at.hour())
        })
    }

    fn minute<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.minute", |at| {
            i64::from(at.minute())
        })
    }

    fn second<'s>(scope: &mut Scope<'s>, plaindatetime: Root<'s, Obj>) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.second", |at| {
            i64::from(at.second())
        })
    }

    fn millisecond<'s>(
        scope: &mut Scope<'s>,
        plaindatetime: Root<'s, Obj>,
    ) -> Result<i64, Thrown<'s>> {
        civil_part(scope, plaindatetime, "PlainDateTime.millisecond", |at| {
            i64::from(at.millisecond())
        })
    }
}

impl NativeBamlTimePlainTime for Natives {
    fn _from_components<'s>(
        scope: &mut Scope<'s>,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        match clock_nanos([hour, minute, second, millisecond, microsecond, nanosecond]) {
            Ok(nanoseconds) => Ok(of_int(scope, NAMED_TIME_PLAIN_TIME, nanoseconds)),
            Err(message) => Err(scope.invalid_argument(&message)),
        }
    }

    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let text = s.get(scope).as_str();
        match Time::parse(text, TIME_FORMAT) {
            Ok(parsed) => {
                let nanoseconds = i64::from(parsed.hour()) * NANOS_PER_HOUR
                    + i64::from(parsed.minute()) * NANOS_PER_MINUTE
                    + i64::from(parsed.second()) * NANOS_PER_SECOND
                    + i64::from(parsed.nanosecond());
                Ok(of_int(scope, NAMED_TIME_PLAIN_TIME, nanoseconds))
            }
            Err(error) => {
                let message = format!(
                    "PlainTime.parse: cannot parse {text:?} as an ISO 8601 time-of-day: {error}"
                );
                Err(scope.parse_error(&message))
            }
        }
    }

    /// A reading a program built past a day is taken within one.
    fn _to_string_impl<'s>(scope: &mut Scope<'s>, plaintime: Root<'s, Obj>) -> Root<'s, Str> {
        let within = int_field(scope, plaintime).rem_euclid(NANOS_PER_DAY);
        let part = |each: i64, of: i64| u8::try_from(within / each % of).unwrap_or_default();
        let mut text = String::new();
        write_clock(
            &mut text,
            part(NANOS_PER_HOUR, 24),
            part(NANOS_PER_MINUTE, 60),
            part(NANOS_PER_SECOND, 60),
            u32::try_from(within % NANOS_PER_SECOND).unwrap_or_default(),
        );
        scope.alloc_str(&text)
    }
}

impl NativeBamlTimeTimeZoneOffset for Natives {
    fn from_duration<'s>(
        scope: &mut Scope<'s>,
        duration: Root<'s, Obj>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let offset = i64::try_from(&big_field(scope, duration, 0))
            .ok()
            .filter(|nanoseconds| nanoseconds.abs() <= NANOS_PER_DAY);
        match offset {
            Some(offset) => Ok(of_int(scope, NAMED_TIME_ZONE_OFFSET, offset)),
            None => {
                Err(scope.invalid_argument("TimeZoneOffset out of range: must be within ±24 hours"))
            }
        }
    }
}

impl NativeBamlTimeZonedDateTime for Natives {
    /// An RFC 3339 timestamp, and after it the time zone in brackets when
    /// there is one, which then stands for the offset.
    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let full = s.get(scope).as_str();
        let (timestamp, zone) = match (full.strip_suffix(']'), full.find('[')) {
            (Some(stripped), Some(open)) if open < stripped.len() => {
                let annotation = &stripped[open + 1..];
                // A leading `!` marks the zone as one the reader must know.
                let annotation = annotation.strip_prefix('!').unwrap_or(annotation);
                if annotation.is_empty() {
                    let message =
                        format!("ZonedDateTime.parse: empty timezone annotation in {full:?}");
                    return Err(scope.parse_error(&message));
                }
                (&full[..open], Some(annotation.to_owned()))
            }
            _ => (full, None),
        };
        let at = match OffsetDateTime::parse(timestamp, &Rfc3339) {
            Ok(at) => at,
            Err(error) => {
                let message = format!(
                    "ZonedDateTime.parse: cannot parse {timestamp:?} as an RFC 3339 timestamp (zoneless strings belong to PlainDateTime.parse): {error}"
                );
                return Err(scope.parse_error(&message));
            }
        };
        let offset = i64::from(at.offset().whole_seconds()) * NANOS_PER_SECOND;
        let nanoseconds = scope.make_big(&BigInt::from(at.unix_timestamp_nanos()));
        let zone = zone.map(|zone| scope.alloc_str(&zone));
        let class = scope.named_class(NAMED_TIME_ZONED_DATE_TIME);
        let mut made = scope.build(class);
        made.set(scope, 0, nanoseconds);
        made.set(scope, 1, zone.is_none().then_some(offset));
        made.set(scope, 2, zone);
        Ok(made.finish())
    }
}

/// `±hh:mm`, or `Z` for no offset when `z` allows it. What an offset has
/// past its minutes is left out.
fn write_offset(out: &mut String, offset: i64, z: bool) {
    if offset == 0 && z {
        out.push('Z');
        return;
    }
    let sign = if offset < 0 { '-' } else { '+' };
    let size = offset.unsigned_abs();
    let hours = size / NANOS_PER_HOUR.unsigned_abs();
    let minutes = size / NANOS_PER_MINUTE.unsigned_abs() % 60;
    let _ = write!(out, "{sign}{hours:02}:{minutes:02}");
}

/// An instant as `jiff` holds one: the nearest it has to one past its
/// range, where a time zone's offset no longer changes.
fn timestamp(nanoseconds: &BigInt) -> jiff::Timestamp {
    let past = if nanoseconds.sign() == num_bigint::Sign::Minus {
        jiff::Timestamp::MIN
    } else {
        jiff::Timestamp::MAX
    };
    i128::try_from(nanoseconds)
        .ok()
        .and_then(|nanoseconds| jiff::Timestamp::from_nanosecond(nanoseconds).ok())
        .unwrap_or(past)
}

impl NativeBamlTime for Natives {
    fn _format_zoned<'s>(
        scope: &mut Scope<'s>,
        epoch_ns: Root<'s, Big>,
        offset_ns: i64,
        iana: Option<Root<'s, Str>>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let local = epoch_ns.get(scope).value() + BigInt::from(offset_ns);
        let at = match civil(&local, "ZonedDateTime.to_string") {
            Ok(at) => at,
            Err(message) => return Err(scope.invalid_argument(&message)),
        };
        let mut text = String::new();
        write_date_time(&mut text, at);
        // `Z` is for a fixed offset of zero: a time zone is written with
        // its offset in numbers and its name in brackets.
        write_offset(&mut text, offset_ns, iana.is_none());
        if let Some(iana) = iana {
            let _ = write!(text, "[{}]", iana.get(scope).as_str());
        }
        Ok(scope.alloc_str(&text))
    }

    /// The name of the host's time zone.
    fn system_timezone<'s>(scope: &mut Scope<'s>) -> Result<Root<'s, Str>, Thrown<'s>> {
        match jiff::tz::TimeZone::system().iana_name() {
            Some(name) => Ok(scope.alloc_str(name)),
            None => Err(scope.io_error("could not determine the system's IANA timezone")),
        }
    }

    /// The offset of `timezone` at an instant, in nanoseconds: nothing for
    /// a name the host's database does not have.
    fn _tz_offset_at(timezone: StrRef<'_>, at_ns: BigRef<'_>) -> Option<i64> {
        let zone = jiff::tz::TimeZone::get(timezone.as_str()).ok()?;
        let offset = zone.to_offset(timestamp(&at_ns.value()));
        Some(i64::from(offset.seconds()) * NANOS_PER_SECOND)
    }

    /// The instant a wall-clock reading is in `timezone`: nothing for a
    /// name the host's database does not have. A reading the zone has
    /// twice or never is settled by `disambiguation`.
    fn _tz_to_instant<'s>(
        scope: &mut Scope<'s>,
        timezone: Root<'s, Str>,
        civil_ns: Root<'s, Big>,
        disambiguation: Root<'s, Str>,
    ) -> Result<Option<Root<'s, Big>>, Thrown<'s>> {
        let name = timezone.get(scope).as_str();
        let Ok(zone) = jiff::tz::TimeZone::get(name) else {
            return Ok(None);
        };
        let reading = i128::try_from(&civil_ns.get(scope).value())
            .ok()
            .and_then(|nanoseconds| jiff::Timestamp::from_nanosecond(nanoseconds).ok())
            .map(|at| at.to_zoned(jiff::tz::TimeZone::UTC).datetime());
        let Some(reading) = reading else {
            return Err(scope.io_error("civil time is outside the supported year range"));
        };
        let ambiguous = zone.to_ambiguous_timestamp(reading);
        let resolved = match disambiguation.get(scope).as_str() {
            "earlier" => ambiguous.earlier(),
            "later" => ambiguous.later(),
            _ => ambiguous.compatible(),
        };
        match resolved {
            Ok(at) => Ok(Some(scope.make_big(&BigInt::from(at.as_nanosecond())))),
            Err(error) => {
                let message = format!(
                    "cannot resolve civil time in timezone '{}': {error}",
                    timezone.get(scope).as_str()
                );
                Err(scope.io_error(&message))
            }
        }
    }
}
