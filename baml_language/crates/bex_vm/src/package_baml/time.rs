use std::{fmt::Write as _, sync::Arc};

use bex_heap::TlabHolder;
use bex_vm_types::types::Value;
use time::{
    Date, Month, OffsetDateTime, PrimitiveDateTime, Time,
    format_description::well_known::{Iso8601, Rfc2822, Rfc3339},
    macros::{date, format_description},
};

use super::{
    BamlClassTimeInstant, BamlClassTimePlainDate, BamlClassTimePlainDateTime,
    BamlClassTimePlainTime, BamlClassTimeTimeZoneOffset, BamlClassTimeZonedDateTime,
    BamlNamespaceTime, PackageBamlImpl, copy, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

pub(crate) const NANOS_PER_SECOND: i64 = 1_000_000_000;
pub(crate) const NANOS_PER_MINUTE: i64 = 60 * NANOS_PER_SECOND;
pub(crate) const NANOS_PER_HOUR: i64 = 60 * NANOS_PER_MINUTE;
pub(crate) const NANOS_PER_DAY: i128 = 24 * NANOS_PER_HOUR as i128;
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
pub(crate) fn civil_datetime(
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

pub(crate) fn date_from_components(year: i64, month: i64, day: i64) -> Result<Date, VmRustFnError> {
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

pub(crate) fn days_since_epoch(date: Date) -> i64 {
    (date - UNIX_EPOCH_DATE).whole_days()
}

pub(crate) fn date_for_days(days: i64, what: &str) -> Result<Date, VmRustFnError> {
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
pub(crate) fn format_date_into(out: &mut String, date: Date) {
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
pub(crate) fn format_clock_into(
    out: &mut String,
    hour: u8,
    minute: u8,
    second: u8,
    subsec_nanos: u32,
) {
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

pub(crate) const DATE_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]");
const TIME_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second][optional [.[subsecond]]]");
pub(crate) const DATETIME_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]");

impl BamlClassTimeInstant for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let s = s.as_str();
        let datetime = OffsetDateTime::parse(s, &Rfc3339)
            .or_else(|_| OffsetDateTime::parse(s, &Iso8601::DEFAULT))
            .or_else(|_| OffsetDateTime::parse(s, &Rfc2822))
            .map_err(|e| VmBamlError::ParseError {
                message: format!("Instant.parse: cannot parse {s:?} as an RFC 3339, ISO 8601, or RFC 2822 timestamp: {e}"),
            })?;
        let instant = copy::time::Instant {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(datetime.unix_timestamp_nanos())),
        };
        Ok(instant.to_value(vm))
    }

    fn _to_string_impl(
        instant: &view::time::Instant<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        #[expect(
            clippy::used_underscore_items,
            reason = "Autogenerated from private BAML field"
        )]
        let nanos = instant._nanoseconds();
        if let Ok(nanos) = i128::try_from(&*nanos)
            && let Ok(datetime) = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            && let Ok(datetime) = datetime.format(&Rfc3339)
        {
            Ok(bex_str::BexStr::from(datetime))
        } else {
            Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "Could not serialize Instant with year out of Rfc3339 timestamp range."
                    .to_string(),
            }))
        }
    }
}

#[allow(clippy::used_underscore_items)] // Autogenerated from private BAML fields
impl BamlClassTimePlainDate for PackageBamlImpl {
    fn from_components(
        vm: &mut BexVm,
        year: i64,
        month: i64,
        day: i64,
    ) -> Result<Value, VmRustFnError> {
        let date = date_from_components(year, month, day)?;
        let plaindate = copy::time::PlainDate {
            _days: days_since_epoch(date),
        };
        Ok(plaindate.to_value(vm))
    }

    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let date = Date::parse(s.as_str(), DATE_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainDate.parse: cannot parse {:?} as an ISO 8601 calendar date: {e}",
                s.as_str()
            ))
        })?;
        let plaindate = copy::time::PlainDate {
            _days: days_since_epoch(date),
        };
        Ok(plaindate.to_value(vm))
    }

    fn _to_string_impl(
        plaindate: &view::time::PlainDate<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let date = date_for_days(plaindate._days(), "PlainDate.to_string")?;
        let mut out = String::new();
        format_date_into(&mut out, date);
        Ok(bex_str::BexStr::from(out))
    }

    fn _to_plain_datetime(vm: &mut BexVm, plaindate: &Value, time: Option<&Value>) -> Value {
        let days = {
            let instance = vm
                .as_instance(plaindate)
                .expect("PlainDate.to_plain_datetime: expected PlainDate instance");
            view::time::PlainDate { instance }._days()
        };
        let time_ns = time.map_or(0, |t| {
            let instance = vm
                .as_instance(t)
                .expect("PlainDate.to_plain_datetime: expected PlainTime instance");
            view::time::PlainTime { instance }._nanoseconds()
        });
        let civil = i128::from(days) * NANOS_PER_DAY + i128::from(time_ns);
        let plaindatetime = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(civil)),
        };
        plaindatetime.to_value(vm)
    }

    fn year(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            date_for_days(plaindate._days(), "PlainDate.year")?.year(),
        ))
    }

    fn month(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(u8::from(
            date_for_days(plaindate._days(), "PlainDate.month")?.month(),
        )))
    }

    fn day(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            date_for_days(plaindate._days(), "PlainDate.day")?.day(),
        ))
    }
}

#[allow(clippy::used_underscore_items)] // Autogenerated from private BAML fields
impl BamlClassTimePlainDateTime for PackageBamlImpl {
    #[allow(clippy::too_many_arguments)] // Mirrors the BAML signature.
    fn _from_components(
        vm: &mut BexVm,
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Value, VmRustFnError> {
        let date = date_from_components(year, month, day)?;
        let time_ns = clock_nanos(hour, minute, second, millisecond, microsecond, nanosecond)?;
        let civil = i128::from(days_since_epoch(date)) * NANOS_PER_DAY + i128::from(time_ns);
        let plaindatetime = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(civil)),
        };
        Ok(plaindatetime.to_value(vm))
    }

    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let datetime = PrimitiveDateTime::parse(s.as_str(), DATETIME_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainDateTime.parse: cannot parse {:?} as a zoneless ISO 8601 datetime: {e}",
                s.as_str()
            ))
        })?;
        let civil = datetime.assume_utc().unix_timestamp_nanos();
        let plaindatetime = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(civil)),
        };
        Ok(plaindatetime.to_value(vm))
    }

    fn _to_string_impl(
        plaindatetime: &view::time::PlainDateTime<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
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
        Ok(bex_str::BexStr::from(out))
    }

    fn to_plain_date(vm: &mut BexVm, plaindatetime: &Value) -> Result<Value, VmRustFnError> {
        let civil = {
            let instance = vm
                .as_instance(plaindatetime)
                .expect("PlainDateTime.to_plain_date: expected PlainDateTime instance");
            view::time::PlainDateTime { instance }._nanoseconds()
        };
        let days = i128::try_from(&*civil)
            .ok()
            .map(|n| n.div_euclid(NANOS_PER_DAY))
            .and_then(|d| i64::try_from(d).ok())
            .ok_or_else(|| {
                invalid_argument(
                    "PlainDateTime.to_plain_date: value is outside the supported year range"
                        .to_string(),
                )
            })?;
        Ok(copy::time::PlainDate { _days: days }.to_value(vm))
    }

    fn to_plain_time(vm: &mut BexVm, plaindatetime: &Value) -> Value {
        let civil = {
            let instance = vm
                .as_instance(plaindatetime)
                .expect("PlainDateTime.to_plain_time: expected PlainDateTime instance");
            view::time::PlainDateTime { instance }._nanoseconds()
        };
        // Euclidean modulo one day is exact regardless of the bigint's
        // magnitude or sign, and the result always fits in an i64.
        let day = num_bigint::BigInt::from(NANOS_PER_DAY);
        let time_ns = ((&*civil % &day) + &day) % &day;
        let time_ns = i64::try_from(time_ns).expect("time-of-day fits in i64");
        copy::time::PlainTime {
            _nanoseconds: time_ns,
        }
        .to_value(vm)
    }

    fn year(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.year")?.year(),
        ))
    }

    fn month(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(u8::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.month")?.month(),
        )))
    }

    fn day(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.day")?.day(),
        ))
    }

    fn hour(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.hour")?.hour(),
        ))
    }

    fn minute(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.minute")?.minute(),
        ))
    }

    fn second(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.second")?.second(),
        ))
    }

    fn millisecond(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(i64::from(
            civil_datetime(&plaindatetime._nanoseconds(), "PlainDateTime.millisecond")?
                .millisecond(),
        ))
    }
}

#[allow(clippy::used_underscore_items)] // Autogenerated from private BAML fields
impl BamlClassTimePlainTime for PackageBamlImpl {
    fn _from_components(
        vm: &mut BexVm,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Value, VmRustFnError> {
        let time_ns = clock_nanos(hour, minute, second, millisecond, microsecond, nanosecond)?;
        Ok(copy::time::PlainTime {
            _nanoseconds: time_ns,
        }
        .to_value(vm))
    }

    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let parsed = Time::parse(s.as_str(), TIME_FORMAT).map_err(|e| {
            parse_error(format!(
                "PlainTime.parse: cannot parse {:?} as an ISO 8601 time-of-day: {e}",
                s.as_str()
            ))
        })?;
        let time_ns = i64::from(parsed.hour()) * NANOS_PER_HOUR
            + i64::from(parsed.minute()) * NANOS_PER_MINUTE
            + i64::from(parsed.second()) * NANOS_PER_SECOND
            + i64::from(parsed.nanosecond());
        Ok(copy::time::PlainTime {
            _nanoseconds: time_ns,
        }
        .to_value(vm))
    }

    fn _to_string_impl(plaintime: &view::time::PlainTime<'_>) -> bex_str::BexStr {
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
        bex_str::BexStr::from(out)
    }
}

#[allow(clippy::used_underscore_items)] // Autogenerated from private BAML fields
impl BamlClassTimeTimeZoneOffset for PackageBamlImpl {
    fn from_duration(vm: &mut BexVm, duration: &Value) -> Result<Value, VmRustFnError> {
        let nanos = {
            let instance = vm
                .as_instance(duration)
                .expect("TimeZoneOffset.from_duration: expected Duration instance");
            view::time::Duration { instance }._nanoseconds()
        };
        let offset_ns = i64::try_from(&*nanos)
            .ok()
            .filter(|ns| ns.abs() <= MAX_TZ_OFFSET_NS)
            .ok_or_else(|| {
                invalid_argument(
                    "TimeZoneOffset out of range: must be within ±24 hours".to_string(),
                )
            })?;
        Ok(copy::time::TimeZoneOffset {
            _nanoseconds: offset_ns,
        }
        .to_value(vm))
    }
}

impl BamlClassTimeZonedDateTime for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let full = s.as_str();
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
            Some(iana) => (Value::NULL, Value::object(vm.alloc_string(iana))),
            None => (
                Value::try_int(offset_ns).expect("offset fits in BAML int"),
                Value::NULL,
            ),
        };
        let zoned = copy::time::ZonedDateTime {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(datetime.unix_timestamp_nanos())),
            _offset_ns: offset_value,
            _iana: iana_value,
        };
        Ok(zoned.to_value(vm))
    }
}

impl BamlNamespaceTime for PackageBamlImpl {
    fn _format_zoned(
        epoch_ns: Arc<num_bigint::BigInt>,
        offset_ns: i64,
        iana: Option<&bex_str::BexStr>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let civil = &*epoch_ns + num_bigint::BigInt::from(offset_ns);
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
            let _ = write!(out, "[{}]", iana.as_str());
        }
        Ok(bex_str::BexStr::from(out))
    }
}
