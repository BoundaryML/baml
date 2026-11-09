//! Owned clock and timezone algorithms shared by native providers.
use crate::errors::{VmBamlError, VmRustFnError};
use num_bigint::{BigInt, Sign};

pub fn now() -> BigInt {
    BigInt::from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| unreachable!("system clock is set before the UNIX epoch"))
            .as_nanos(),
    )
}
pub fn system_timezone() -> Result<String, VmRustFnError> {
    jiff::tz::TimeZone::system()
        .iana_name()
        .map(str::to_owned)
        .ok_or_else(|| {
            VmBamlError::Io {
                message: "could not determine the system's IANA timezone".into(),
            }
            .into()
        })
}
fn saturating_timestamp(ns: &BigInt) -> jiff::Timestamp {
    match i128::try_from(ns) {
        Ok(ns) => jiff::Timestamp::from_nanosecond(ns).unwrap_or(if ns < 0 {
            jiff::Timestamp::MIN
        } else {
            jiff::Timestamp::MAX
        }),
        Err(_) if ns.sign() == Sign::Minus => jiff::Timestamp::MIN,
        Err(_) => jiff::Timestamp::MAX,
    }
}
pub fn offset_at(timezone: &str, at_ns: &BigInt) -> Option<i64> {
    let tz = jiff::tz::TimeZone::get(timezone).ok()?;
    Some(i64::from(tz.to_offset(saturating_timestamp(at_ns)).seconds()) * 1_000_000_000)
}
pub fn to_instant(
    timezone: &str,
    civil_ns: &BigInt,
    disambiguation: &str,
) -> Result<Option<BigInt>, VmRustFnError> {
    let Ok(tz) = jiff::tz::TimeZone::get(timezone) else {
        return Ok(None);
    };
    let civil = i128::try_from(civil_ns)
        .ok()
        .and_then(|ns| jiff::Timestamp::from_nanosecond(ns).ok())
        .map(|ts| ts.to_zoned(jiff::tz::TimeZone::UTC).datetime())
        .ok_or_else(|| VmBamlError::Io {
            message: "civil time is outside the supported year range".into(),
        })?;
    let ambiguous = tz.to_ambiguous_timestamp(civil);
    let resolved = match disambiguation {
        "earlier" => ambiguous.earlier(),
        "later" => ambiguous.later(),
        _ => ambiguous.compatible(),
    };
    resolved
        .map(|ts| Some(BigInt::from(ts.as_nanosecond())))
        .map_err(|e| {
            VmBamlError::Io {
                message: format!("cannot resolve civil time in timezone '{timezone}': {e}"),
            }
            .into()
        })
}
pub fn timeout_from_nanos(nanos: &BigInt) -> Option<std::time::Duration> {
    (nanos.sign() == Sign::Plus)
        .then(|| std::time::Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timezone_saturation_and_sleep_bounds() {
        assert_eq!(offset_at("UTC", &BigInt::from(-1)), Some(0));
        assert_eq!(offset_at("NoSuchZone", &BigInt::ZERO), None);
        assert_eq!(
            to_instant("UTC", &BigInt::ZERO, "compatible").unwrap(),
            Some(BigInt::ZERO)
        );
        assert!(to_instant("UTC", &(BigInt::from(1) << 200usize), "compatible").is_err());
        assert_eq!(timeout_from_nanos(&BigInt::from(-1)), None);
        assert_eq!(
            timeout_from_nanos(&(BigInt::from(1) << 200usize))
                .unwrap()
                .as_nanos(),
            u128::from(u64::MAX)
        );
    }
}
