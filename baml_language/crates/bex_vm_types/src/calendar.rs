//! Heap-neutral calendar algorithms shared by native packs and the VM.
use crate::errors::{VmBamlError, VmRustFnError};
use std::fmt::Write as _;
use time::{
    Date, Month, OffsetDateTime, PrimitiveDateTime, Time,
    format_description::well_known::{Iso8601, Rfc2822, Rfc3339},
    macros::{date, format_description},
};
pub const NANOS_PER_SECOND: i64 = 1_000_000_000;
pub const NANOS_PER_MINUTE: i64 = 60 * NANOS_PER_SECOND;
pub const NANOS_PER_HOUR: i64 = 60 * NANOS_PER_MINUTE;
pub const NANOS_PER_DAY: i128 = 24 * NANOS_PER_HOUR as i128;
const MAX_TZ_OFFSET_NS: i64 = 24 * NANOS_PER_HOUR;
const UNIX_EPOCH_DATE: Date = date!(1970 - 01 - 01);

fn invalid_argument(message: String) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })
}

fn parse_error(message: String) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::ParseError { message })
}

/// Interprets a civil ("wall-clock") reading — nanoseconds since
/// 1970-01-01T00:00:00, as if the reading were UTC — as a datetime.
pub fn civil_datetime(
    ns: &num_bigint::BigInt,
    what: &str,
) -> Result<OffsetDateTime, VmRustFnError> {
    i128::try_from(ns)
        .ok()
        .and_then(|n| OffsetDateTime::from_unix_timestamp_nanos(n).ok())
        .ok_or_else(|| {
            invalid_argument(format!("{what}: value is outside the supported year range"))
        })
}

pub fn date_from_components(year: i64, month: i64, day: i64) -> Result<Date, VmRustFnError> {
    let y = i32::try_from(year)
        .map_err(|_| invalid_argument(format!("year {year} is out of range")))?;
    let m = u8::try_from(month)
        .ok()
        .and_then(|m| Month::try_from(m).ok())
        .ok_or_else(|| invalid_argument(format!("month {month} is out of range [1, 12]")))?;
    let d =
        u8::try_from(day).map_err(|_| invalid_argument(format!("day {day} is out of range")))?;
    Date::from_calendar_date(y, m, d)
        .map_err(|e| invalid_argument(format!("invalid date {year:04}-{month:02}-{day:02}: {e}")))
}

pub fn days_since_epoch(date: Date) -> i64 {
    (date - UNIX_EPOCH_DATE).whole_days()
}

pub fn date_for_days(days: i64, what: &str) -> Result<Date, VmRustFnError> {
    UNIX_EPOCH_DATE
        .checked_add(time::Duration::days(days))
        .ok_or_else(|| {
            invalid_argument(format!("{what}: date is outside the supported year range"))
        })
}

/// Validates clock components and returns nanoseconds since midnight.
fn clock_nanos(
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    microsecond: i64,
    nanosecond: i64,
) -> Result<i64, VmRustFnError> {
    let check = |value: i64, max: i64, name: &str| {
        if (0..=max).contains(&value) {
            Ok(value)
        } else {
            Err(invalid_argument(format!(
                "{name} {value} is out of range [0, {max}]"
            )))
        }
    };
    Ok(check(hour, 23, "hour")? * NANOS_PER_HOUR
        + check(minute, 59, "minute")? * NANOS_PER_MINUTE
        + check(second, 59, "second")? * NANOS_PER_SECOND
        + check(millisecond, 999, "millisecond")? * 1_000_000
        + check(microsecond, 999, "microsecond")? * 1_000
        + check(nanosecond, 999, "nanosecond")?)
}

/// Formats `2026-03-18` into `out`. Negative years carry a sign.
pub fn format_date_into(out: &mut String, date: Date) {
    let year = date.year();
    if year < 0 {
        let _ = write!(out, "-{:04}", -i64::from(year));
    } else {
        let _ = write!(out, "{year:04}");
    }
    let _ = write!(out, "-{:02}-{:02}", u8::from(date.month()), date.day());
}

/// Formats `07:32:00` / `07:32:00.5` into `out`, truncating trailing
/// subsecond zeros.
pub fn format_clock_into(out: &mut String, hour: u8, minute: u8, second: u8, subsec_nanos: u32) {
    let _ = write!(out, "{hour:02}:{minute:02}:{second:02}");
    if subsec_nanos != 0 {
        let digits = format!("{subsec_nanos:09}");
        let _ = write!(out, ".{}", digits.trim_end_matches('0'));
    }
}

/// Formats a `±hh:mm` offset suffix (`Z` if zero and `allow_z`). Sub-minute
/// offset components (historical LMT zones) are truncated toward zero.
fn format_offset_into(out: &mut String, offset_ns: i64, allow_z: bool) {
    if offset_ns == 0 && allow_z {
        out.push('Z');
        return;
    }
    let sign = if offset_ns < 0 { '-' } else { '+' };
    let abs = offset_ns.unsigned_abs();
    let hours = abs / NANOS_PER_HOUR.unsigned_abs();
    let minutes = abs / NANOS_PER_MINUTE.unsigned_abs() % 60;
    let _ = write!(out, "{sign}{hours:02}:{minutes:02}");
}

pub const DATE_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]");
const TIME_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second][optional [.[subsecond]]]");
pub const DATETIME_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]");

#[derive(Clone)]
pub struct Instant {
    pub _nanoseconds: num_bigint::BigInt,
}
impl Instant {
    pub fn _nanoseconds(&self) -> num_bigint::BigInt {
        self._nanoseconds.clone()
    }
}
#[derive(Clone)]
pub struct PlainDate {
    pub _days: i64,
}
impl PlainDate {
    pub fn _days(&self) -> i64 {
        self._days
    }
}
#[derive(Clone)]
pub struct PlainDateTime {
    pub _nanoseconds: num_bigint::BigInt,
}
impl PlainDateTime {
    pub fn _nanoseconds(&self) -> num_bigint::BigInt {
        self._nanoseconds.clone()
    }
}
#[derive(Clone)]
pub struct PlainTime {
    pub _nanoseconds: i64,
}
impl PlainTime {
    pub fn _nanoseconds(&self) -> i64 {
        self._nanoseconds
    }
}
#[derive(Clone)]
pub struct TimeZoneOffset {
    pub _nanoseconds: i64,
}
impl TimeZoneOffset {
    pub fn _nanoseconds(&self) -> i64 {
        self._nanoseconds
    }
}
#[derive(Clone)]
pub struct ZonedDateTime {
    pub _nanoseconds: num_bigint::BigInt,
    pub _offset_ns: Option<i64>,
    pub _iana: Option<String>,
}
impl ZonedDateTime {
    pub fn _nanoseconds(&self) -> num_bigint::BigInt {
        self._nanoseconds.clone()
    }
    pub fn _offset_ns(&self) -> Option<i64> {
        self._offset_ns
    }
    pub fn _iana(&self) -> Option<String> {
        self._iana.clone()
    }
}
#[derive(Clone)]
pub struct Duration {
    pub _nanoseconds: num_bigint::BigInt,
}
impl Duration {
    pub fn _nanoseconds(&self) -> num_bigint::BigInt {
        self._nanoseconds.clone()
    }
}
impl Instant {
    pub fn parse(s: &str) -> Result<Instant, VmRustFnError> {
        let datetime = OffsetDateTime::parse(s, &Rfc3339)
            .or_else(|_| OffsetDateTime::parse(s, &Iso8601::DEFAULT))
            .or_else(|_| OffsetDateTime::parse(s, &Rfc2822))
            .map_err(|e| VmBamlError::ParseError {
                message: format!("Instant.parse: cannot parse {s:?} as an RFC 3339, ISO 8601, or RFC 2822 timestamp: {e}"),
            })?;
        let instant = Instant {
            _nanoseconds: num_bigint::BigInt::from(datetime.unix_timestamp_nanos()),
        };
        Ok(instant)
    }
    pub fn _to_string_impl(instant: &Instant) -> Result<String, VmRustFnError> {
        #[expect(
            clippy::used_underscore_items,
            reason = "Autogenerated from private BAML field"
        )]
        let nanos = instant._nanoseconds();
        if let Ok(nanos) = i128::try_from(&nanos)
            && let Ok(datetime) = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            && let Ok(datetime) = datetime.format(&Rfc3339)
        {
            Ok(String::from(datetime))
        } else {
            Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "Could not serialize Instant with year out of Rfc3339 timestamp range."
                    .to_string(),
            }))
        }
    }
}
impl PlainDate {
    pub fn from_components(year: i64, month: i64, day: i64) -> Result<PlainDate, VmRustFnError> {
        let date = date_from_components(year, month, day)?;
        let plaindate = PlainDate {
            _days: days_since_epoch(date),
        };
        Ok(plaindate)
    }
    pub fn parse(s: &str) -> Result<PlainDate, VmRustFnError> {
        let date = Date::parse(s, DATE_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainDate.parse: cannot parse {:?} as an ISO 8601 calendar date: {e}",
                s
            ))
        })?;
        let plaindate = PlainDate {
            _days: days_since_epoch(date),
        };
        Ok(plaindate)
    }
    pub fn _to_string_impl(plaindate: &PlainDate) -> Result<String, VmRustFnError> {
        let date = date_for_days(plaindate._days(), "PlainDate.to_string")?;
        let mut out = String::new();
        format_date_into(&mut out, date);
        Ok(String::from(out))
    }
    pub fn _to_plain_datetime(plaindate: &PlainDate, time_ns: i64) -> PlainDateTime {
        let days = plaindate._days();
        let civil = i128::from(days) * NANOS_PER_DAY + i128::from(time_ns);
        let plaindatetime = PlainDateTime {
            _nanoseconds: num_bigint::BigInt::from(civil),
        };
        plaindatetime
    }
    pub fn year(plaindate: &PlainDate) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            date_for_days(plaindate._days(), "PlainDate.year")?.year(),
        ))
    }
    pub fn month(plaindate: &PlainDate) -> Result<i64, VmRustFnError> {
        Ok(i64::from(u8::from(
            date_for_days(plaindate._days(), "PlainDate.month")?.month(),
        )))
    }
    pub fn day(plaindate: &PlainDate) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            date_for_days(plaindate._days(), "PlainDate.day")?.day(),
        ))
    }
}
impl PlainDateTime {
    pub fn _from_components(
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<PlainDateTime, VmRustFnError> {
        let date = date_from_components(year, month, day)?;
        let time_ns = clock_nanos(hour, minute, second, millisecond, microsecond, nanosecond)?;
        let civil = i128::from(days_since_epoch(date)) * NANOS_PER_DAY + i128::from(time_ns);
        let plaindatetime = PlainDateTime {
            _nanoseconds: num_bigint::BigInt::from(civil),
        };
        Ok(plaindatetime)
    }
    pub fn parse(s: &str) -> Result<PlainDateTime, VmRustFnError> {
        let datetime = PrimitiveDateTime::parse(s, DATETIME_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainDateTime.parse: cannot parse {:?} as a zoneless ISO 8601 datetime: {e}",
                s
            ))
        })?;
        let civil = datetime.assume_utc().unix_timestamp_nanos();
        let plaindatetime = PlainDateTime {
            _nanoseconds: num_bigint::BigInt::from(civil),
        };
        Ok(plaindatetime)
    }
    pub fn _to_string_impl(plaindatetime: &PlainDateTime) -> Result<String, VmRustFnError> {
        let dt = civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.to_string")?;
        let mut out = String::new();
        format_date_into(&mut out, dt.date());
        out.push('T');
        format_clock_into(
            &mut out,
            dt.hour(),
            dt.minute(),
            dt.second(),
            dt.nanosecond(),
        );
        Ok(String::from(out))
    }
    pub fn to_plain_date(plaindatetime: &PlainDateTime) -> Result<PlainDate, VmRustFnError> {
        let civil = plaindatetime._nanoseconds();
        let days = i128::try_from(&civil)
            .ok()
            .map(|n| n.div_euclid(NANOS_PER_DAY))
            .and_then(|d| i64::try_from(d).ok())
            .ok_or_else(|| {
                invalid_argument(
                    "PlainDateTime.to_plain_date: value is outside the supported year range"
                        .to_string(),
                )
            })?;
        Ok(PlainDate { _days: days })
    }
    pub fn to_plain_time(plaindatetime: &PlainDateTime) -> PlainTime {
        let civil = plaindatetime._nanoseconds();
        // Euclidean modulo one day is exact regardless of the bigint's
        // magnitude or sign, and the result always fits in an i64.
        let day = num_bigint::BigInt::from(NANOS_PER_DAY);
        let time_ns = ((&civil % &day) + &day) % &day;
        let time_ns = i64::try_from(time_ns).expect("time-of-day fits in i64");
        PlainTime {
            _nanoseconds: time_ns,
        }
    }
    pub fn year(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.year")?.year(),
        ))
    }
    pub fn month(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(u8::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.month")?.month(),
        )))
    }
    pub fn day(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.day")?.day(),
        ))
    }
    pub fn hour(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.hour")?.hour(),
        ))
    }
    pub fn minute(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.minute")?.minute(),
        ))
    }
    pub fn second(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.second")?.second(),
        ))
    }
    pub fn millisecond(plaindatetime: &PlainDateTime) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.millisecond")?
                .millisecond(),
        ))
    }
}
impl PlainTime {
    pub fn _from_components(
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<PlainTime, VmRustFnError> {
        let time_ns = clock_nanos(hour, minute, second, millisecond, microsecond, nanosecond)?;
        Ok(PlainTime {
            _nanoseconds: time_ns,
        })
    }
    pub fn parse(s: &str) -> Result<PlainTime, VmRustFnError> {
        let parsed = Time::parse(s, TIME_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainTime.parse: cannot parse {:?} as an ISO 8601 time-of-day: {e}",
                s
            ))
        })?;
        let time_ns = i64::from(parsed.hour()) * NANOS_PER_HOUR
            + i64::from(parsed.minute()) * NANOS_PER_MINUTE
            + i64::from(parsed.second()) * NANOS_PER_SECOND
            + i64::from(parsed.nanosecond());
        Ok(PlainTime {
            _nanoseconds: time_ns,
        })
    }
    pub fn _to_string_impl(plaintime: &PlainTime) -> String {
        // Normalize manually-constructed out-of-range values into [0, 24h).
        let ns = plaintime._nanoseconds().rem_euclid(MAX_TZ_OFFSET_NS);
        let mut out = String::new();
        format_clock_into(
            &mut out,
            u8::try_from(ns / NANOS_PER_HOUR).expect("hour fits in u8"),
            u8::try_from(ns / NANOS_PER_MINUTE % 60).expect("minute fits in u8"),
            u8::try_from(ns / NANOS_PER_SECOND % 60).expect("second fits in u8"),
            u32::try_from(ns % NANOS_PER_SECOND).expect("subsecond fits in u32"),
        );
        String::from(out)
    }
}
impl TimeZoneOffset {
    pub fn from_duration(duration: &Duration) -> Result<TimeZoneOffset, VmRustFnError> {
        let nanos = duration._nanoseconds();
        let offset_ns = i64::try_from(&nanos)
            .ok()
            .filter(|ns| ns.abs() <= MAX_TZ_OFFSET_NS)
            .ok_or_else(|| {
                invalid_argument(
                    "TimeZoneOffset out of range: must be within ±24 hours".to_string(),
                )
            })?;
        Ok(TimeZoneOffset {
            _nanoseconds: offset_ns,
        })
    }
}
impl ZonedDateTime {
    pub fn parse(s: &str) -> Result<ZonedDateTime, VmRustFnError> {
        let full = s;
        // Split an optional RFC 9557 bracketed timezone annotation:
        // `2026-03-18T13:04:27-07:00[America/Los_Angeles]`.
        let (timestamp, iana) = match (full.strip_suffix(']'), full.find('[')) {
            (Some(stripped), Some(open)) if open < stripped.len() => {
                let annotation = &stripped[open + 1..];
                // RFC 9557 marks annotations the producer considers essential
                // with a leading `!`; the identifier follows either way.
                let annotation = annotation.strip_prefix('!').unwrap_or(annotation);
                if annotation.is_empty() {
                    return Err(parse_error(format!(
                        "ZonedDateTime.parse: empty timezone annotation in {full:?}"
                    )));
                }
                (&full[..open], Some(annotation.to_string()))
            }
            _ => (full, None),
        };
        let datetime = OffsetDateTime::parse(timestamp, &Rfc3339).map_err(|e| {
            parse_error(format!(
                "ZonedDateTime.parse: cannot parse {timestamp:?} as an RFC 3339 timestamp \
                 (zoneless strings belong to PlainDateTime.parse): {e}"
            ))
        })?;
        let offset_ns = i64::from(datetime.offset().whole_seconds()) * NANOS_PER_SECOND;
        let (offset_value, iana_value) = match iana {
            // An IANA annotation wins over the numeric offset; the offset was
            // still used above to compute the absolute time. The identifier
            // is validated lazily, on first resolution against the host's
            // timezone database.
            Some(iana) => (None, Some(iana)),
            None => (Some(offset_ns), None),
        };
        let zoned = ZonedDateTime {
            _nanoseconds: num_bigint::BigInt::from(datetime.unix_timestamp_nanos()),
            _offset_ns: offset_value,
            _iana: iana_value,
        };
        Ok(zoned)
    }
}
pub fn _format_zoned(
    epoch_ns: num_bigint::BigInt,
    offset_ns: i64,
    iana: Option<&str>,
) -> Result<String, VmRustFnError> {
    let civil = &epoch_ns + num_bigint::BigInt::from(offset_ns);
    let dt = civil_datetime(&civil, "ZonedDateTime.to_string")?;
    let mut out = String::new();
    format_date_into(&mut out, dt.date());
    out.push('T');
    format_clock_into(
        &mut out,
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.nanosecond(),
    );
    // `Z` only for a fixed zero offset; an IANA timezone always carries a
    // numeric offset plus its bracketed annotation (RFC 9557, the way
    // Temporal serializes).
    format_offset_into(&mut out, offset_ns, iana.is_none());
    if let Some(iana) = iana {
        let _ = write!(out, "[{}]", iana);
    }
    Ok(String::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_and_unbounded_civil_times_use_euclidean_days() {
        let before = PlainDateTime {
            _nanoseconds: (-1).into(),
        };
        assert_eq!(PlainDateTime::to_plain_date(&before).unwrap()._days, -1);
        assert_eq!(
            PlainDateTime::to_plain_time(&before)._nanoseconds,
            86_399_999_999_999
        );
        let huge = PlainDateTime {
            _nanoseconds: -(num_bigint::BigInt::from(1) << 700usize),
        };
        assert!(
            (0..86_400_000_000_000).contains(&PlainDateTime::to_plain_time(&huge)._nanoseconds)
        );
        assert!(PlainDateTime::to_plain_date(&huge).is_err());
    }
    #[test]
    fn parsed_zones_preserve_absolute_time_and_annotation_priority() {
        let fixed = ZonedDateTime::parse("1970-01-01T01:00:00+01:00").unwrap();
        let zone = ZonedDateTime::parse("1970-01-01T01:00:00+01:00[!Etc/UTC]").unwrap();
        assert_eq!(fixed._nanoseconds, num_bigint::BigInt::from(0));
        assert_eq!(fixed._offset_ns, Some(3_600_000_000_000));
        assert_eq!(zone._nanoseconds, fixed._nanoseconds);
        assert_eq!(zone._offset_ns, None);
        assert_eq!(zone._iana.as_deref(), Some("Etc/UTC"));
        assert!(ZonedDateTime::parse("1970-01-01T01:00:00+01:00[]").is_err());
    }
    #[test]
    fn formatting_and_components_retain_calendar_validation() {
        let date = PlainDate::from_components(2024, 2, 29).unwrap();
        assert_eq!(PlainDate::_to_string_impl(&date).unwrap(), "2024-02-29");
        assert!(PlainDate::from_components(2023, 2, 29).is_err());
        let clock = PlainTime::_from_components(23, 59, 58, 123, 456, 789).unwrap();
        assert_eq!(PlainTime::_to_string_impl(&clock), "23:59:58.123456789");
        assert!(PlainTime::_from_components(24, 0, 0, 0, 0, 0).is_err());
        assert_eq!(
            _format_zoned(0.into(), 0, None).unwrap(),
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            _format_zoned(0.into(), 0, Some("Etc/UTC")).unwrap(),
            "1970-01-01T00:00:00+00:00[Etc/UTC]"
        );
    }
}
