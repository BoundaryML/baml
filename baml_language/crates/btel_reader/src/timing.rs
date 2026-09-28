//! Clock interpretation for recorded ticks. Durations exist only when the
//! epoch conversion is known, no invalidation was observed and the interval
//! is not backwards; otherwise the reason is reported, never a zero.
use btel_recorder::proto;

/// Latest-known validity of one clock epoch. Validity only degrades, so the
/// most severe observation wins regardless of file order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EpochStatus {
    Valid = 1,
    Restored = 2,
    Discontinuity = 3,
    Uncertain = 4,
    ModeChanged = 5,
}

impl EpochStatus {
    pub fn from_wire(status: i32) -> Option<Self> {
        Some(match proto::TimingStatus::try_from(status).ok()? {
            proto::TimingStatus::Valid => Self::Valid,
            proto::TimingStatus::Restored => Self::Restored,
            proto::TimingStatus::Discontinuity => Self::Discontinuity,
            proto::TimingStatus::Uncertain => Self::Uncertain,
            proto::TimingStatus::ModeChanged => Self::ModeChanged,
            proto::TimingStatus::Unspecified => return None,
        })
    }
    pub fn from_code(code: i64) -> Option<Self> {
        Self::from_wire(i32::try_from(code).ok()?)
    }
    pub fn code(self) -> i64 {
        self as i64
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Restored => "restored",
            Self::Discontinuity => "discontinuity",
            Self::Uncertain => "uncertain",
            Self::ModeChanged => "mode_changed",
        }
    }
    /// Combine two observations of the same epoch.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        self.max(other)
    }
}

/// Tick-to-nanosecond scale: `ns = (ticks * multiplier) >> shift`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conversion {
    pub multiplier: u64,
    pub shift: u32,
}

impl Conversion {
    pub fn ticks_to_ns(self, ticks: u64) -> Option<u64> {
        if self.shift > 127 {
            return None;
        }
        u64::try_from((u128::from(ticks) * u128::from(self.multiplier)) >> self.shift).ok()
    }
}

/// Why a derived duration is unavailable, or `Valid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingState {
    Valid,
    /// No completion observed in the indexed prefix.
    Incomplete,
    /// Thread or clock epoch definition not (yet) observed.
    UnknownClock,
    /// The epoch was invalidated.
    Invalidated(EpochStatus),
    /// The end precedes the start.
    Backward,
    /// The value does not fit the reported integer range.
    Overflow,
    /// Two different definitions were recorded for the clock epoch.
    Conflicted,
}

impl TimingState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Incomplete => "incomplete",
            Self::UnknownClock => "unknown_clock",
            Self::Invalidated(status) => match status {
                EpochStatus::Valid => "valid",
                EpochStatus::Restored => "invalidated_restored",
                EpochStatus::Discontinuity => "invalidated_discontinuity",
                EpochStatus::Uncertain => "invalidated_uncertain",
                EpochStatus::ModeChanged => "invalidated_mode_changed",
            },
            Self::Backward => "backward",
            Self::Overflow => "overflow",
            Self::Conflicted => "conflicted",
        }
    }
}

/// The clock facts a derived duration depends on.
#[derive(Clone, Copy, Debug, Default)]
pub struct Clock {
    pub conversion: Option<Conversion>,
    /// Most severe observed status; `None` when no state was observed.
    pub status: Option<EpochStatus>,
}

impl Clock {
    fn usable(self) -> Result<Conversion, TimingState> {
        match (self.conversion, self.status) {
            (Some(conversion), Some(EpochStatus::Valid)) => Ok(conversion),
            (Some(_), Some(status)) => Err(TimingState::Invalidated(status)),
            _ => Err(TimingState::UnknownClock),
        }
    }

    /// Nanoseconds between two instants of this epoch.
    pub fn interval_ns(self, start: Option<u64>, end: Option<u64>) -> Result<u64, TimingState> {
        let (Some(start), Some(end)) = (start, end) else {
            return Err(TimingState::Incomplete);
        };
        let conversion = self.usable()?;
        let ticks = end.checked_sub(start).ok_or(TimingState::Backward)?;
        conversion.ticks_to_ns(ticks).ok_or(TimingState::Overflow)
    }

    /// An accumulated tick total (durations, self-await) in nanoseconds.
    pub fn total_ns(self, ticks: Option<u64>) -> Result<u64, TimingState> {
        let ticks = ticks.ok_or(TimingState::Overflow)?;
        self.usable()?
            .ticks_to_ns(ticks)
            .ok_or(TimingState::Overflow)
    }
}

/// UTC anchor: the tick at which the producer sampled Unix time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcAnchor {
    pub ticks: u64,
    pub unix_ns: i128,
}

impl UtcAnchor {
    pub fn from_wire(anchor: &proto::UtcAnchor) -> Option<Self> {
        let unix = anchor.unix_nanos.as_ref()?;
        let unix_ns = (i128::from(unix.high) << 64) | i128::from(unix.low);
        Some(Self {
            ticks: anchor.ticks,
            unix_ns,
        })
    }

    /// Wall-clock estimate for `ticks`: anchor time plus the converted delta.
    /// Uncertainty is not modelled here; this is for display and ordering.
    pub fn unix_ns_at(self, conversion: Conversion, ticks: u64) -> Option<i128> {
        if ticks >= self.ticks {
            let delta = conversion.ticks_to_ns(ticks - self.ticks)?;
            self.unix_ns.checked_add(i128::from(delta))
        } else {
            let delta = conversion.ticks_to_ns(self.ticks - ticks)?;
            self.unix_ns.checked_sub(i128::from(delta))
        }
    }
}

/// RFC 3339 UTC with microseconds, e.g. `2026-09-23T14:05:31.448298Z`.
pub fn format_unix_ns(unix_ns: i128) -> Option<String> {
    const NS_PER_SEC: i128 = 1_000_000_000;
    let seconds = unix_ns.div_euclid(NS_PER_SEC);
    let nanos = unix_ns.rem_euclid(NS_PER_SEC);
    let days = i64::try_from(seconds.div_euclid(86_400)).ok()?;
    let secs_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(0..=9999).contains(&year) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}Z",
        secs_of_day / 3600,
        secs_of_day % 3600 / 60,
        secs_of_day % 60,
        nanos / 1000
    ))
}

// Howard Hinnant's days-to-civil algorithm (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).expect("day of month");
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).expect("month");
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_uses_a_wide_product_and_reports_overflow() {
        let c = Conversion {
            multiplier: 3 << 30,
            shift: 31,
        };
        assert_eq!(c.ticks_to_ns(10), Some(15));
        assert_eq!(
            c.ticks_to_ns(u64::MAX),
            None,
            "product exceeds u64 after the shift"
        );
        let identity = Conversion {
            multiplier: 1,
            shift: 0,
        };
        assert_eq!(identity.ticks_to_ns(u64::MAX), Some(u64::MAX));
    }

    #[test]
    fn durations_are_never_invented() {
        let valid = Clock {
            conversion: Some(Conversion {
                multiplier: 1,
                shift: 0,
            }),
            status: Some(EpochStatus::Valid),
        };
        assert_eq!(valid.interval_ns(Some(5), Some(9)), Ok(4));
        assert_eq!(
            valid.interval_ns(Some(9), Some(5)),
            Err(TimingState::Backward)
        );
        assert_eq!(
            valid.interval_ns(Some(9), None),
            Err(TimingState::Incomplete)
        );
        let unknown = Clock::default();
        assert_eq!(
            unknown.interval_ns(Some(1), Some(2)),
            Err(TimingState::UnknownClock)
        );
        let invalid = Clock {
            status: Some(EpochStatus::Valid.merge(EpochStatus::Discontinuity)),
            ..valid
        };
        assert_eq!(
            invalid.interval_ns(Some(1), Some(2)),
            Err(TimingState::Invalidated(EpochStatus::Discontinuity))
        );
    }

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(
            format_unix_ns(0).as_deref(),
            Some("1970-01-01T00:00:00.000000Z")
        );
        assert_eq!(
            format_unix_ns(1_790_000_000_123_456_789).as_deref(),
            Some("2026-09-21T14:13:20.123456Z")
        );
        assert_eq!(
            format_unix_ns(-1).as_deref(),
            Some("1969-12-31T23:59:59.999999Z")
        );
    }
}
