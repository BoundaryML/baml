//! Population outcomes are evidence carried by aggregate deltas, not inferred
//! from retained spans. Absence in an older delta must survive every rollup.
pub(crate) const RECORDED: i64 = 1;
pub(crate) const MISSING: i64 = 2;
pub(crate) const INVALID: i64 = 4;
/// Some delta counted outcomes but not panics (format minor < 3).
pub(crate) const PANICS_MISSING: i64 = 16;

#[derive(Clone, Copy, Default)]
pub(crate) struct Totals {
    pub evidence: i64,
    pub errored: u128,
    pub cancelled: u128,
    /// The errored completions a panic ended.
    pub panicked: u128,
}

impl Totals {
    pub(crate) fn from_wire(
        count: u64,
        outcomes: Option<&btel_recorder::proto::AggregateOutcomes>,
    ) -> Self {
        let Some(outcomes) = outcomes else {
            return Self {
                evidence: MISSING,
                ..Self::default()
            };
        };
        let errored = u128::from(outcomes.errored);
        let cancelled = u128::from(outcomes.cancelled);
        let panicked = outcomes.panicked.map(u128::from);
        if errored + cancelled > u128::from(count) || panicked.is_some_and(|p| p > errored) {
            return Self {
                evidence: INVALID,
                ..Self::default()
            };
        }
        Self {
            evidence: RECORDED
                | if panicked.is_none() {
                    PANICS_MISSING
                } else {
                    0
                },
            errored,
            cancelled,
            panicked: panicked.unwrap_or(0),
        }
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            evidence: self.evidence | other.evidence,
            errored: self.errored.saturating_add(other.errored),
            cancelled: self.cancelled.saturating_add(other.cancelled),
            panicked: self.panicked.saturating_add(other.panicked),
        }
    }

    /// Panicked completions, when every delta counted them.
    pub(crate) fn panicked(self) -> Option<i64> {
        if self.evidence & !PANICS_MISSING != RECORDED || self.evidence & PANICS_MISSING != 0 {
            return None;
        }
        i64::try_from(self.panicked).ok()
    }

    /// Only a fully recorded population supports numeric outcome counts.
    /// Keep exact totals internally so a later unsupported delta cannot
    /// turn a partial population into a misleading success/error rate.
    pub(crate) fn counts(self, count: u128) -> [Option<i64>; 3] {
        if self.evidence & !PANICS_MISSING != RECORDED {
            return [None; 3];
        }
        let ok = self
            .errored
            .checked_add(self.cancelled)
            .and_then(|failed| count.checked_sub(failed));
        [
            ok.and_then(|n| i64::try_from(n).ok()),
            i64::try_from(self.errored).ok(),
            i64::try_from(self.cancelled).ok(),
        ]
    }
}
